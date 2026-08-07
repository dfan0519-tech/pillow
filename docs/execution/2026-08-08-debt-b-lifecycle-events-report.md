# DEBT-B — Governed process lifecycle events observability gap

Date: 2026-08-08  
Worker: grok2 (Pillow)  
Controller: Claude  
Status: **DEBT-B-LIFECYCLE-BLOCKED** (design complete; implementation not authorized under allowlist)

## 0. Scope and facts accepted

C4 checkpoint unresolved risk §5.3. Defect is real and already verified by
controller; this report does not re-litigate the UX gap.

Symptom: for a long-running approved governed process, the activity timeline
stays on “pending approval” until the process finishes, then
`ProcessProposalDecided` + `ProcessStarted` + `ProcessTerminalOutcome` appear
together.

Root surface (adapter post-return replay):

- `crates/adapter-grok-acp/src/prompt.rs` **713–742**

Cancel remains gated by run activity (`isRunActive`), not process event state.
That property is out of scope and must not be weakened.

## 1. Design question 1 — When is `receipt.started` produced?

### Produce site

| Step | Location | What happens |
| --- | --- | --- |
| Approve decision durable | `runtime-host/src/host/process.rs` `decide_process_proposal` **1348–1363** | `gateway.decide_for_owner` persists `Approved` |
| Build started payload | same fn **1383–1388** | `ProcessStartedPayload { proposal_id, program, argv, portable_cwd }` constructed in memory |
| Record on live entry | same fn **1390–1396** | `entry.decided_payload` / `entry.started_payload` set **before** spawn |
| One-shot claim + spawn | same fn **1410–1418** → `tool-executor` `execute_with_observer` **738–851** | `claim_run` → durable `Running` (**storage-sqlite** `exec_proposal.rs` **487–512**), then `runner.run_with_observer` |
| Publish to waiter | `publish_terminal_for_waiter` **1117–1139** | After execute returns, terminal row + decided/started attached; CV notified |
| Waiter returns receipt | `wait_for_resolution` **851–877** | Clones `started_payload` into `GovernedProcessReceipt.started` only when `entry.terminal` is set |
| Adapter emits | `adapter-grok-acp/src/prompt.rs` **713–731** | Emits `ProcessStarted` only after `execute_governed_process` returns and only if `delivery == Executed` |

### Protocol meaning vs code timing

Protocol doc on the payload (`agent-protocol/src/types.rs` **333–334**):

> Process start notice **after one-shot claim**.

So the *semantic* authority for “started” is post-`claim_run` (durable
`Running`), not OS PID assignment.

Code currently *constructs* the payload **before** claim (**1383–1396**), then
*keeps or drops* it when publishing terminal:

- Fresh success path: `started_out = Some(started)` (**1430–1437**)
- Busy / terminal-replay / reject-class execute outcomes: `started_out = None`
  (same block) so the receipt does not advertise a false start

### Time gap to real OS spawn

From `started_payload` construction to OS spawn:

1. `claim_run` SQLite immediate transaction (`Approved` → `Running`)
2. `revalidate_sealed` (**803–807**)
3. Pre-spawn cancel check (**809–830**)
4. Clean env build (**833–840**)
5. `ProcessSpec` assembly (**842–849**)
6. `SupervisedProcessRunner::run_with_observer` → `SupervisedChild::spawn`
   (`process-executor/src/lib.rs` **107–115**, **188–211**)

Gap is typically **milliseconds** of host work, not “after process exit.”  
The **observability** gap is different: the payload exists early, but the
**event is only emitted** after the full `run_with_observer` blocks through
process exit (**prompt.rs 713–731**). For a long process that gap is the full
wall-clock runtime.

Reject / expire / cancel-before-claim paths intentionally leave
`receipt.started == None` (`finalize_via_gateway` **1023**; reject arm
**1373**).

Replay path: `replay_receipt` **588–604** sets `started: None` and
`delivery: Replayed`; adapter skips all three post-request events when
`delivery != Executed` (**713**).

## 2. Design question 2 — Where should in-flight `ProcessStarted` emit?

### Candidate layers

| Layer | Verdict | Why |
| --- | --- | --- |
| Adapter post-return replay (`prompt.rs` 713–742) only | **Insufficient** | No intermediate return; `execute_governed_process` blocks until terminal. Cannot surface “running” mid-flight without new signals. |
| `execute_governed_process` / `wait_for_resolution` (runtime-host) | **Primary emit orchestration** | Same pattern as `on_requested` (**816–818**, **627–628**): waiter thread owns the `EventSink` callback chain. Can emit decided/started when live entry first becomes eligible, then wait for terminal. |
| `decide_process_proposal` | **Must mark eligibility, not own the sink** | Decision thread has no `EventSink`. Must not invent a second event authority. Should notify waiters after durable decision (and, for start, only after claim). |
| Gateway `execute_with_observer` post-`Claimed` | **Correct “started eligible” signal** | Matches protocol “after one-shot claim.” Claim is durable `Running` before any runner work. |
| `process-executor` observer | **Wrong layer** | OS/runner only; no proposal identity, no protocol event kinds, no durable process ledger. Observer today is data-only output chunks (`tool-executor` **734–737**). |

### Recommended emit chain (no new event kinds)

1. **Decided (approve or reject)**  
   After durable `decide_for_owner` succeeds, record `decided_payload`, notify CV.  
   Waiter invokes adapter callback → emit `ProcessProposalDecided`.

2. **Started (approve only)**  
   After `claim_run` returns `Claimed` (durable `Running`), record/confirm
   `started_payload`, notify CV.  
   Waiter invokes adapter callback → emit `ProcessStarted`.  
   **Do not** emit on the pre-claim in-memory assignment alone (claim can still
   return Busy / Expired / Conflict without a legitimate start).

3. **Terminal**  
   Unchanged: after durable terminal + `publish_terminal_for_waiter`, receipt
   returns; adapter emits `ProcessTerminalOutcome` (and only those lifecycle
   events not already emitted in-flight).

4. **Replay**  
   Unchanged: `GovernedProcessDelivery::Replayed` → no lifecycle re-emit.

### Why not process-executor alone

Spawning is after claim; emitting only after OS spawn would still fix UX, but
wiring protocol `RuntimeEvent` through the runner would reverse layering and
expand `process-executor` beyond supervised execution. Prefer a narrow
post-claim hook/callback on the gateway or host, not runner-owned protocol
events.

## 3. Design question 3 — Invariant check

### 3.1 Durable receipt unique authority / no duplicate emit

| Risk | Mitigation |
| --- | --- |
| Double `ProcessStarted` (in-flight + post-return) | Adapter must track which lifecycle kinds already emitted for this call; post-return only emits remaining kinds (typically terminal). Host may clear or flag `started_emitted` on live entry. |
| Replay re-emit | Keep `delivery == Replayed` gate (**prompt.rs 713**); `replay_receipt` leaves `started: None`. |
| Busy after pre-claim start payload | Emit started only post-`Claimed`; Busy path continues to set `started_out = None` (**1430–1435**). |

### 3.2 `validate → persist → commit → emit`

Process **ledger** order today (must keep):

1. Reserve → durable `PendingApproval`  
2. `on_requested` emit (`prompt.rs` **659–676**) after reserve  
3. Decide → durable `Approved` / `Rejected`  
4. Claim → durable `Running`  
5. Spawn / wait  
6. Complete → durable terminal  

Runtime **events** are caller-emits after durable steps (D3AR “caller-emits
pattern”). In-flight decided/started must follow the same rule:

- Decided emit **after** decide commit  
- Started emit **after** claim commit (`Running`)  
- Terminal emit **after** terminal commit (already true)

Do **not** emit started from the pre-claim assignment at **1395**.

### 3.3 Replay must not re-emit

Already structural:

- Terminal identity hit in `execute_governed_process` → `replay_receipt` (**723–730**, **588–604**)
- Adapter skips process lifecycle when `delivery != Executed` (**713**)

In-flight callbacks must only run on the live Executed wait path, never on
Replayed early return.

### 3.4 Cancel / terminal transition legality

| Concern | Status |
| --- | --- |
| Cancel vs process events | Cancel of the **run** is independent (`isRunActive`); process may still emit terminal `Cancelled` / `Interrupted`. |
| Illegal UI migration `Cancelling → Completed` | Belongs to desktop projection (out of this allowlist). Host must still emit process kinds in order `decided → started? → terminal` without inventing a second process state machine. |
| Pre-claim cancel | No started (claim never Claimed); terminal cancel/interrupt only. |
| Post-claim cancel | Started already valid; terminal `Cancelled` is legal. |

### 3.5 Project / conversation scope and grant privacy

- Decide remains owner + conversation scoped (**1305–1310**).  
- Grant still single-use cleared at decision (**1320–1321**); never on event
  payloads (`ProcessStartedPayload` fields are summary-only).  
- Callbacks must pass only typed protocol payloads already used on the receipt.
- No new public grant surface.

**Conclusion:** in-flight emit is compatible with all listed invariants **if and
only if** started is gated on post-claim durable `Running` and adapter/host
de-duplicate against post-return replay.

## 4. Design question 4 — Minimal change plan

### Preferred minimal design (no new event kinds; keep decide blocking)

`decide_process_proposal` **remains** blocking through `execute_with_observer`.
No change to approval API shape.

Required code surfaces (beyond current task allowlist):

| Crate / file | Change |
| --- | --- |
| `crates/tool-executor/src/process.rs` | Optional: after `ExecRunClaimOutcome::Claimed`, invoke a host-supplied “claimed” side channel **or** return enough for host to learn claim without waiting for process exit. Narrowest form: extend observer/callback contract for claim success only (not outcome authority). |
| `crates/runtime-host/src/host/process.rs` | (1) After durable decide, set `decided_payload` + `cv.notify_all`. (2) After claim success, set confirmed `started_payload` + notify. (3) In `wait_for_resolution`, once-only callbacks for decided/started before terminal. (4) Receipt post-return may still carry full payloads for callers that do not use callbacks. |
| `crates/adapter-grok-acp/src/prompt.rs` | Wire callbacks to `emit_process_event`; post-return emit only remaining kinds (terminal; reject path decided if not already emitted). |
| Tests | Host bridge + adapter roundtrip: long-running process → observe `ProcessStarted` **before** terminal; replay no duplicates; reject omits started; terminal order legal. |

### What this deliberately does **not** do

- Does **not** make `decide_process_proposal` return at approve-before-execute.  
- Does **not** add new `RuntimeEventKind` values.  
- Does **not** move durable process state into JSON files or UI.  
- Does **not** emit from `process-executor` protocol events.  
- Does **not** change grant privacy or cross-conversation access.

### Impact surface

- **Medium protocol/timing risk** if started is emitted pre-claim (must not).  
- **Low API surface** if modeled after existing `on_requested` callback.  
- Desktop can consume existing kinds; no desktop change required for the gap
  itself (once events arrive in-flight).  
- Existing roundtrip tests that only assert final event multiset remain valid;
  need **new timing-sensitive** tests.

### Allowlist collision (implementation blocker)

Task write allowlist:

- `crates/adapter-grok-acp/src/prompt.rs`
- `crates/adapter-grok-acp/tests/governed_process_roundtrip.rs`
- `crates/process-executor/**` (only if scheme requires)
- report path

**Correct fix requires `runtime-host` (and likely `tool-executor`)**, which are
**not** allowlisted. Adapter-only or process-executor-only patches cannot emit
protocol lifecycle events mid-flight without inventing a second authority or
weakening ordering.

Per task §3: if the scheme needs structural host changes beyond the safe
adapter-only box, **stop and report only**. Judgment: implementation blocked.

## 5. Implementation decision

| Item | Decision |
| --- | --- |
| Implemented code changes? | **No** |
| Reason | Correct mid-flight `ProcessStarted` requires post-claim signaling + waiter callbacks in **runtime-host** (and likely **tool-executor**), outside write allowlist. Adapter replay-only fix cannot close the gap. process-executor is the wrong layer for `RuntimeEvent`. |
| Blocking model change? | Proposed design: **no** |
| New event kinds? | Proposed design: **no** |
| Persist order change? | Proposed design: **no** (only *when* events are emitted relative to already-durable steps) |

## 6. Tests that implementation must add (when authorized)

Allowlist tests file: `governed_process_roundtrip.rs` (+ host bridge tests when
runtime-host is authorized).

1. **In-flight started observability**  
   Approve a process that sleeps / blocks ≥ several hundred ms. From a side
   thread or shared sink, assert `ProcessStarted` appears **before**
   `ProcessTerminalOutcome` (and while decide is still blocked).

2. **No duplicate lifecycle on replay**  
   Second identical call → `Replayed` → no second
   requested/decided/started/terminal multiset growth beyond the first
   Executed path (existing test strengthened if callbacks land).

3. **Reject omits started**  
   Still `requested → decided → terminal` only.

4. **Terminal order legal**  
   Sequence numbers monotonic; never `ProcessTerminalOutcome` before
   `ProcessStarted` on approve path; never started after terminal.

5. **Busy / failed claim never started** (host-level)  
   If claim returns Busy, no `ProcessStarted` event.

Do not weaken authority / privacy / durable order to green the assertions.

## 7. Gate commands (this worker run)

No product code modified. Gates still run as required.

| Command | Exit code | Notes |
| --- | --- | --- |
| `node scripts/run-cargo.mjs test -p keeper-adapter-grok-acp` | **0** | 60 unit + 19 fake_child + 5 process RT + 3 read + 4 write + 6 h5 + 4 live (1 ignored) + 3 fixtures; all green |
| `node scripts/run-cargo.mjs clippy -p keeper-adapter-grok-acp -- -D warnings` | **0** | |
| `node scripts/check-secrets.mjs` | **0** | `secret gate ok` |
| `git diff --cached --name-only` | **0** (empty) | Staging empty as required |

## 8. Residual risks

1. **Pre-claim started payload** already lives on the live entry (**1395**)
   before durable `Running`. Any future UI that peeks live maps must not treat
   that field as “running” without claim confirmation.
2. **No CV notify** when decided/started are first written (**1390–1396**);
   waiter only wakes on 1–250 ms timeout or terminal/output notify. In-flight
   design must add notify after eligibility updates.
3. Desktop projection / “Cancelling → Completed” is a separate concern; this
   debt only fixes process lifecycle **event** timing.
4. Keeper agent-bridge boot returned HTTP 400 in this environment; work
   proceeded from the task contract alone.
5. Parallel desktop worker owns `apps/desktop/**` and E2E; not exercised here.

## 9. What this worker did / did not do

### Did

- Traced approve / reject / replay / claim / spawn / adapter emit paths with
  file:line evidence.
- Answered all four design questions.
- Proposed minimal invariant-preserving design (no new kinds, keep decide
  blocking, post-claim started emit).
- Wrote this report under the allowlisted path.
- Ran required package gates / secrets / staging checks.

### Did not

- No edits under `crates/**` product sources.
- No `process-executor` changes (wrong layer for protocol events).
- No runtime-host / tool-executor changes (outside allowlist).
- No desktop / git-executor / scripts changes.
- No `git add/commit/stash/reset/clean/checkout/restore`.
- No desktop E2E.
- No auth/token file access.
- No weakening of authority, privacy, or persistence order.

## 10. Recommendation to controller

Authorize a follow-up with expanded allowlist:

1. `crates/runtime-host/src/host/process.rs` (+ `governed_process_bridge` tests)
2. `crates/tool-executor/src/process.rs` only if post-claim signal cannot be
   expressed without it
3. Existing adapter allowlist files for callback wiring + timing tests

Until then, ship this design report only.

---

## 11. Gate results (executed)

All required gates exited **0**. Staging empty. No product code changed.

**Signal:** `DEBT-B-LIFECYCLE-BLOCKED: correct mid-flight ProcessStarted requires runtime-host (+ likely tool-executor) outside write allowlist; adapter/process-executor-only cannot emit post-claim protocol events without weakening authority or inventing a second path`
