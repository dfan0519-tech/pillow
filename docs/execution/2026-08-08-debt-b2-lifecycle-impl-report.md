# DEBT-B2 — Flight-period process lifecycle emit implementation

Date: 2026-08-08  
Worker: grok-worker (Pillow)  
Controller: Claude  
Status: **DEBT-B2-LIFECYCLE-IMPL-COMPLETE**

## 0. Scope

Implementation of the design in
`docs/execution/2026-08-08-debt-b-lifecycle-events-report.md` (controller-
accepted). No redesign. No new event kinds. Decide remains blocking. Events
never precede durable persistence.

## 1. Actual emit sites (file:line)

| Kind | Durable gate | Mark eligibility | Waiter callback | Adapter emit |
| --- | --- | --- | --- | --- |
| **Requested** | reserve → `PendingApproval` | live insert | `on_lifecycle(Requested)` **843** | `prompt.rs` **668–684** |
| **Decided** | `decide_for_owner` accept | `decided_payload` + `cv.notify_all` **1478–1486** (approve) / `publish_terminal_for_waiter` reject | `wait_for_resolution` **882–897** → `GovernedProcessLifecycle::Decided` | in-flight **687–697**; post-return only if not yet emitted **749–758** |
| **Started** | `claim_run` → `Claimed` (`Running`) | `on_claimed` sets `started_payload` + `started_confirmed` **1509–1519**; tool-executor fires hook **782–787** | `wait_for_resolution` **900–914** → `GovernedProcessLifecycle::Started` | in-flight **699–709**; post-return only if not yet emitted **760–769** |
| **Terminal** | durable terminal + `publish_terminal_for_waiter` **1191–1220** | receipt return | *(not a waiter callback)* | post-return only **770–779** |

### Authority split (red lines held)

- `decide_process_proposal` **does not** hold an `EventSink`. It only records
  payloads and notifies the waiter CV (**1486**, reject via
  `publish_terminal_for_waiter`).
- `process-executor` is untouched. Protocol events never enter the runner.
- `validate → persist → commit → emit` order preserved: decided after decide
  commit; started only after claim `Claimed`; terminal after terminal commit.

## 2. How double-emit is prevented

1. **Host once-only flags** on `LiveProcessEntry`:
   `decided_delivered` / `started_delivered` (**267–268**). Waiter delivers each
   kind at most once per live entry.
2. **Adapter flags** `decided_emitted` / `started_emitted` (`AtomicBool`,
   `prompt.rs` **661–662**). In-flight callbacks set them; post-return path
   skips kinds already set (**749–769**). Terminal is only emitted post-return
   (never in-flight), so it cannot double.
3. **Replay**: `GovernedProcessDelivery::Replayed` short-circuits before any
   lifecycle callback and before the post-return block
   (`prompt.rs` delivery gate). `replay_receipt` still sets
   `requested/decided/started = None`.

## 3. How claim failure never emits `ProcessStarted`

1. Pre-claim: approve path sets `started_payload = None` and
   `started_confirmed = false` (**1483–1484**). Pre-claim draft is never
   started-eligible.
2. tool-executor `on_claimed` runs **only** in the `ExecRunClaimOutcome::Claimed`
   arm (**782–787**). Busy / Expired / Conflict / Terminal return without
   calling the hook.
3. Waiter emits `Started` only when `started_confirmed && started_payload`
   (**900–914**).
4. Busy Running outcome still quarantines and returns
   `GovernedProcessBridgeError::Busy` without inventing a start (**1535–1537**).

## 4. Required regression tests

| # | Name | Location | Assertion points |
| --- | --- | --- | --- |
| 1 | `in_flight_started_is_observable_before_terminal` | `governed_process_bridge.rs` | Slow node process; `Started` callback fires while `terminal_returned` is still false; order `requested → decided → started` |
| 1b | `in_flight_process_started_arrives_before_terminal` | `governed_process_roundtrip.rs` | Shared sink sees `ProcessStarted` before `ProcessTerminalOutcome` while decide is blocked; final multiset unique |
| 2 | `in_flight_lifecycle_does_not_double_emit_on_waiter` | host bridge | decided/started callback counts == 1 |
| 2b | `executed_path_emits_each_lifecycle_kind_exactly_once` | adapter roundtrip | each of 4 process kinds appears exactly once in sink |
| 3 | `replay_path_emits_zero_lifecycle_callbacks` | host bridge | second identical call → `Replayed`, callback count == 0 |
| 3b | `replay_provider_path_does_not_grow_lifecycle_beyond_first_execution` | adapter | lifecycle multiset length 4, one `ProcessStarted` |
| 4 | `on_claimed_observer_fires_only_after_claimed_not_on_busy` | `tool-executor` unit | first Claimed → hook fires once; stolen Running → Busy → hook fires 0 times |
| 5 | `lifecycle_order_is_decided_then_started_then_terminal` | host bridge | callback order decided → started, then receipt terminal; decided state is durable Approved |

## 5. tool-executor change (why unavoidable)

**Yes, modified** `crates/tool-executor/src/process.rs` (+ `lib.rs` re-export).

Reason: claim is owned by `GovernedProcessToolGateway::execute_with_observer`.
Host cannot learn durable `Claimed` without either (a) a post-claim side
channel or (b) waiting for process exit. Option (a) matches the accepted
design. Narrowest form:

- New type `ClaimedObserver = Box<dyn FnOnce() + Send>`
- Optional `on_claimed` argument on `execute_with_observer`
- Invoked only after `claim_run` returns `Claimed`, before revalidate/spawn
- No outcome authority; Busy/Expired/Conflict never call it

`execute()` passes `None` for both observer and claim hook (unchanged
behavior for existing callers).

## 6. Allowlist / export surface note

Primary writes match the task allowlist. Minimal companion re-exports were
required so callers compile:

| File | Why |
| --- | --- |
| `runtime-host/src/host/mod.rs` | export `GovernedProcessLifecycle` + test hooks |
| `runtime-host/src/lib.rs` | public crate re-export of lifecycle enum / test hooks |
| `tool-executor/src/lib.rs` | public re-export of `ClaimedObserver` |

No `apps/desktop/**`, `scripts/**`, `crates/git-executor/**`, or
`.superpowers/**` changes.

## 7. Gate results

| Command | Exit code |
| --- | --- |
| `node scripts/run-cargo.mjs test -p keeper-runtime-host` | **0** |
| `node scripts/run-cargo.mjs test -p keeper-adapter-grok-acp` | **0** |
| `node scripts/run-cargo.mjs test -p keeper-tool-executor` | **0** |
| `node scripts/run-cargo.mjs clippy --workspace -- -D warnings` | **0** |
| `node scripts/check-secrets.mjs` | **0** (`secret gate ok`) |
| `git diff --check` | **0** (CRLF warnings only) |
| `git diff --cached --name-only` | **0** / empty staging |

Local session bridge boot (optional) returned HTTP 400 in this environment; work followed
the task contract alone.

## 8. Residual risks

1. **Very short processes**: adapter mid-flight probe can race on sub-ms
   processes; host bridge test uses a 700 ms sleep to make the gap robust.
   Final multiset uniqueness still covers double-emit regardless of timing.
2. **Pre-claim test hook** (`set_pre_claim_hook_for_tests`) is cfg-gated
   (`test` / `test-support`) and is not required by the default gate suite;
   claim-Busy coverage is the tool-executor unit test above.
3. Desktop projection of process timeline is unchanged (out of allowlist);
   once events arrive in-flight the UI can show “running” without further host
   work.
4. Cancel vs process events remains run-gated (`isRunActive`); not weakened.

## 9. What this worker did / did not do

### Did

- Post-claim `on_claimed` side channel in tool-executor.
- Waiter-thread decided/started lifecycle callbacks in runtime-host.
- Adapter in-flight emit + post-return de-dupe.
- Five required regression themes (host + adapter + tool-executor unit).
- All required gates; wrote this report.

### Did not

- No new `RuntimeEventKind` values.
- No change to decide blocking model.
- No `process-executor` protocol events.
- No desktop / E2E / scripts / git-executor edits.
- No `git add/commit/stash/reset/clean/checkout/restore`.
- No auth/token file access.
- No weakening of grant privacy or project/conversation isolation.

---

**Signal:** `DEBT-B2-LIFECYCLE-IMPL-COMPLETE`
