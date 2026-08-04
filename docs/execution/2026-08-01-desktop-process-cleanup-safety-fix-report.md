# Desktop process cleanup safety fix report

## Completion status

`ACCEPTED` — worker implementation, independent security review, and controller
acceptance are complete. Desktop product E2E remains a separate follow-on gate.

## Changed files

- `scripts/lib/desktop-process.mjs`
- `scripts/lib/desktop-process-safety.test.mjs`
- `docs/execution/2026-08-01-desktop-process-cleanup-safety-fix-report.md`

No files were staged or committed.

## TDD evidence

### RED

Command:

```powershell
node --test scripts/lib/desktop-process-safety.test.mjs
```

Actual result before the implementation: exit `1`; all 7 focused cases failed
with `TypeError: makeBoundary is not a function`. This is the expected old-code
failure: the bare-PID cleanup module had no synthetic-snapshot / injected
termination boundary with which to prove identity before killing.

### GREEN

Command:

```powershell
node --test scripts/lib/desktop-process-safety.test.mjs
```

Actual result after the implementation: exit `0`; 7 tests passed, 0 failed.
The test uses only synthetic snapshots and an injected recorder. It does not
start `taskkill.exe`, a desktop application, Cargo, Rust, WebView2, or a real
process tree.

## Historical rejected first-pass design

This section is retained only as the record of the first rejected approach. It
does not describe the current cleanup implementation.

- Process snapshots now include creation time. A tracked identity is immutable
  and contains PID, normalized creation time, executable path, and process
  name.
- Every Windows termination takes a fresh snapshot and requires an exact
  identity match. Missing or changed identity data produces an explicit
  cleanup refusal instead of a kill.
- A denylist blocks critical Windows process names, and executables rooted in
  `C:\\Windows` are blocked even when their stored identity matches.
- Descendants are added only from a currently matching tracked identity.
  A stale/reused parent PID cannot grant ownership to a new child.
- The rejected approach still issued a single-PID termination command after an
  identity check. The independent review rejected it because that left a
  time-of-check/time-of-use window.
- Residual desktop processes that cannot be identity-proven are left alive and
  cause a clear cleanup failure.

## Remaining risks

- The focused test validates the safety boundary, not Windows command output
  or desktop lifecycle timing; no desktop E2E was run by design.
- A process that cannot be captured with complete identity data is deliberately
  not cleaned up. This can leave a residual process, but avoids PID-reuse
  termination risk.

## Independent review repair pass

The first implementation was rejected because a fresh identity check followed
by `taskkill.exe` still left a numeric-PID time-of-check/time-of-use window.
This pass removes that termination path entirely.

### Additional changed files allowed by the amendment

- `scripts/conversation-recovery-desktop.mjs`
- `scripts/native-acceptance-desktop.mjs`
- `scripts/run-cargo.mjs`
- `scripts/smoke-desktop.mjs`
- `scripts/wave-c2-resource-acceptance.mjs`
- `scripts/wave-c3-governed-write-acceptance.mjs`
- `scripts/lib/desktop-smoke-self-test.mjs`

Each `ProcessSupervisor` construction now passes the launcher-owned
`ChildProcess` handle. C2/C3's tracked-PID filtering now uses the
identity-backed `trackedPids()` / `retainTrackedPids()` API rather than a
mutable public PID set.

### Repair RED

Command:

```powershell
node --test scripts/lib/desktop-process-safety.test.mjs
```

Actual result before the repair: exit `1`; 10/10 tests failed. The failures
proved the rejected design lacked the single identity store, immutable
first-observation handling, root-handle termination state, integer creation
ticks, and absence-confirmed cleanup contract. The source audit also still
found `taskkill.exe`.

### Repair GREEN

Command:

```powershell
node --test scripts/lib/desktop-process-safety.test.mjs
```

Actual result after the repair: exit `0`; 10/10 tests passed. The production
`ProcessSupervisor` is exercised through synthetic Windows snapshots and fake
child handles only. Coverage includes C2/C3 filtering, repeated tracking,
natural exit and repeated cleanup, never-observed identity, PID reuse,
non-`C:` Windows roots, stale-parent descendants, root-handle-only termination,
and a kill request that remains incomplete until fresh absence is observed.

Additional static verification passed: `node --check` for every changed script
and `git diff --check` over the allowed files. No real process, `taskkill.exe`,
desktop application, Cargo, Rust, WebView2, or desktop E2E was started.

### Current design after the first repair

- Windows snapshot identity stores PID, path, name, and exact integer creation
  FILETIME ticks; no date parsing or millisecond rounding is used.
- Repeated tracking never replaces the first identity. A changed live identity
  is permanently marked as a mismatch and blocks cleanup.
- Only the launcher-held child handle may receive `child.kill()` for the root.
  Descendants are identity-bound observation records: they may exit naturally,
  but are never terminated by PID.
- A root kill request changes state to `terminating`; cleanup succeeds only
  after a fresh snapshot confirms absence. `never-observed`, mismatch, unsafe
  system-root/name, and residual observation-only states all fail closed.
- System-root protection resolves `SystemRoot`/`WINDIR` (and is injected in the
  synthetic test); child ticks must be later than their verified parent.

### Updated remaining risk

Normal desktop cleanup can now report a residual process rather than force it
closed when the launcher lacks a stable handle or a descendant survives. This
is intentional safety behavior. No live Windows lifecycle run was authorized
for this repair pass.

## Second review final repair

### Repair RED

Command:

```powershell
node --test scripts/lib/desktop-process-safety.test.mjs
```

Actual result before this final patch: exit `1`; 11 existing cases passed and
7 new amendment cases failed. The failures covered delayed root observation,
the reduced default budget, missing injected residual proof, incomplete live
identity handling, confirmed-dead PID reuse, non-Windows fake child lifecycle,
and missing `SystemRoot`/`WINDIR` refusal.

### Repair GREEN

The same command exited `0`: 18/18 focused synthetic integration tests passed.
No test created a real child process or invoked a real termination command.

### Final current design

- A root initially absent from a snapshot is `never-observed`, not dead. It may
  receive its first complete identity only when the same launcher-owned child
  handle is still live; numeric-only descendants cannot use that recovery.
- Process lookup is explicit: `absent`, `present-incomplete`, or
  `present-complete`. Incomplete live data fails closed. Once
  `confirmed-dead`, a record is terminal and later PID reuse is ignored.
- Cleanup defaults to 120 observation polls. Tests inject shorter budgets and
  no-op delays without changing production behavior.
- On Windows, success under `enforceDesktopClean` requires an injected fresh
  residual observation proving no `pillow.exe` or supervised listener remains.
  Unbound residuals are reported and never terminated.
- Windows root termination requires a valid `SystemRoot` or `WINDIR`. On
  non-Windows, cleanup uses the launcher-held child lifecycle only and never
  calls a Windows snapshot.

### Worker pre-controller remaining risk

At worker handoff, the focused tests were synthetic by authorization and a live
Windows lifecycle exercise had not yet run. The controller subsequently ran a
bounded child-only lifecycle probe, recorded below. When process ownership,
identity, system-root configuration, or residual proof is incomplete, cleanup
still refuses to force termination and reports the remaining process state.

## Controller final acceptance

The controller independently inspected the final diff and accepted the repair
only after the independent reviewer returned `Spec PASS`, `Security APPROVED`,
with zero Critical and zero Important findings.

Fresh controller evidence:

- `node --test scripts/lib/desktop-process-safety.test.mjs`: exit `0`, 29/29
  passed.
- A controlled Windows lifecycle probe launched one child-only Node process
  (PID 22952), terminated it through the launcher-owned `ChildProcess` handle,
  and freshly confirmed `trackedState=confirmed-dead` and PID absence.
- Windows Security event audit showed no `taskkill.exe` event during the probe
  or the following five-minute window.
- `pnpm check`: exit `0`; workspace structure, TypeScript, 21 Vitest files / 246
  tests, Rust workspace tests, doctests, and Clippy `-D warnings` all passed.
  Two earlier controller runs deliberately remained recorded as failed
  acceptance attempts: they exposed fast PID reuse and transient incomplete
  descendant identities in real Cargo/Rust test lifecycles, which were then
  covered by RED/GREEN regressions before this final green run.
- `pnpm --filter @pillow/desktop build`: exit `0`, 73 modules transformed.
- `cargo fmt --all --check`, `node --check` for every changed script, focused
  `git diff --check`, and the forbidden-termination scan all passed.
- Final process audit found no Pillow, fake ACP, Cargo, or rustc residuals.

No desktop E2E was restarted as part of this incident repair. The previously
interrupted C3 product acceptance remains a separate follow-on gate. No file
was staged or committed.

## Final review closure

### Closure RED

`node --test scripts/lib/desktop-process-safety.test.mjs` exited `1` after
adding the six focused closure cases: the prior 18 cases passed and all six
new cases failed for the three final Important findings.

### Closure GREEN

The same purely synthetic command exited `0`: 24/24 tests passed. Final
static `node --check` and scoped `git diff --check` also passed.

### Closure design

- A non-empty injected residual observation is retained and re-observed inside
  the existing cleanup budget. Only the final non-empty observation is
  reported; it is never terminated.
- `SystemRoot` and `WINDIR` are independently normalized and validated as
  absolute Windows paths. An empty primary falls back to the valid secondary;
  invalid candidates and conflicting valid roots refuse root termination.
- Retained non-root `never-observed` records now block successful cleanup.
  A snapshot child of a verified parent with incomplete identity fields is
  permanently recorded as observation-only `identity-incomplete`, and blocks
  cleanup without receiving a termination request.

## Controller live acceptance compatibility repair

The controller's full `pnpm check` established that all business suites passed,
but exposed a cleanup-only false failure when a short-lived `rustc.exe`
descendant PID was immediately reused during the same Cargo run.

### Compatibility RED/GREEN

Two new synthetic tests first failed: immediate non-root PID reuse raised an
identity mismatch, and root PID reuse still failed even after its stable child
handle had reported exit. After the narrow state transition repair, the focused
command below exited `0` with 26/26 passing tests:

```powershell
node --test scripts/lib/desktop-process-safety.test.mjs
```

### Compatibility design

- A complete identity change for a non-root observation-only record marks the
  original record terminal `identity-replaced`. The replacement is neither
  rebound nor terminated, and cannot block launcher-root cleanup.
- A root identity change remains `identity-mismatch` while its stable child
  handle is live. Only a handle that already reports exit authorizes marking
  the original root `confirmed-dead`; the reused PID is never rebound or
  terminated.
- `identity-incomplete` and `never-observed` gates are unchanged and remain
  fail-closed.

## Controller second live acceptance compatibility repair

The next controller `pnpm check` passed all product tests but found a
short-lived observation-only `fake_acp_agent.exe` descendant whose snapshot
identity was incomplete at cleanup.

### Compatibility RED/GREEN

Two focused synthetic tests first failed: a transient non-root incomplete
record prevented root-handle cleanup, and persistent non-root incomplete data
did not exercise the intended observation budget. After the repair,
`node --test scripts/lib/desktop-process-safety.test.mjs` exited `0` with
28/28 tests passing.

### Compatibility design

- Root `identity-incomplete` remains an immediate fail-closed termination
  gate.
- A non-root `identity-incomplete` record is observation-only. It does not
  grant ownership or termination authority, and does not prevent one stable
  root-handle termination request.
- A later `absent` snapshot marks that original non-root record
  `confirmed-dead`. Persistent incomplete data, or a different complete
  identity with no observed absence, fails only after the cleanup budget.

## Second-live review correction

### Precise RED/GREEN

The focused pure-simulation test for `original complete -> present-incomplete
-> different complete` with no intervening `absent` first failed as intended:
cleanup returned successfully (28/29 tests passed, one expected rejection was
missing). After the minimal transition repair, the same command passed 29/29:

```powershell
node --test scripts/lib/desktop-process-safety.test.mjs
```

### State correction

For a non-root record with a complete original identity, an incomplete
observation now preserves the existing `identity-incomplete` uncertainty
marker across a later different complete identity. It is neither rebound nor
made terminal `identity-replaced`; cleanup instead consumes its observation
budget and fails closed. Only an observed `absent` transition may mark the
original record `confirmed-dead`. Direct complete-to-different-complete reuse
and root state transitions are unchanged.

No real process, desktop application, Cargo, Rust, WebView2, or desktop E2E
was started for this correction.
