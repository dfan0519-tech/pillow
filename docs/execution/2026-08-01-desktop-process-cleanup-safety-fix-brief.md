# Desktop process cleanup safety fix brief

## Incident and root cause

The C3 desktop acceptance run was followed by Windows bugcheck `0xEF`
(`CRITICAL_PROCESS_DIED`). Windows Error Reporting classified the failure as
`svchost.exe` terminated by `taskkill.exe`. Security audit events show two
`taskkill.exe` processes created by the nested Node acceptance runners at
18:14:39 and 18:14:43; the service-host termination was recorded 96 ms after
the second call.

`scripts/lib/desktop-process.mjs` currently retains bare numeric PIDs, expands
descendants from that stale set, tests liveness by PID only, and calls
`taskkill /T /F`. A PID may be reused after its original process exits, so the
cleanup path can terminate an unrelated process and `/T` can expand that
mistake to an unverified process tree.

## Required outcome

Make cleanup fail closed. It must never terminate a process merely because its
numeric PID was tracked earlier.

1. Track an immutable process identity captured from a Windows process
   snapshot. At minimum it must bind PID, creation time, executable path, and
   process name. Normalize comparisons consistently.
2. Before every termination, take a fresh snapshot and require the live
   process to match the stored identity. Missing identity data, PID reuse, path
   change, creation-time change, or name change must be rejected rather than
   killed.
3. Never terminate Windows critical/system processes. Include an explicit
   fail-closed system-name denylist and reject executables rooted in the
   Windows directory. This defense applies even if a stored identity appears
   to match.
4. Do not use `taskkill /T`. Descendants must be discovered, identity-bound,
   revalidated, and terminated individually.
5. Descendant discovery must only expand from currently matching tracked
   identities. A stale or reused parent PID must not confer ownership on new
   children.
6. Preserve the existing exported APIs and normal callers unless a narrowly
   scoped compatibility change is necessary.
7. If a candidate cannot be safely proven owned, leave it alive and return a
   clear cleanup failure. Safety takes precedence over residual-process proof.

## TDD and safety requirements

Add a focused Node test that uses synthetic process snapshots and an injected
or pure termination boundary. The test itself must not start `taskkill.exe`, a
desktop application, Cargo, Rust, WebView2, or a real process tree.

The RED run must demonstrate at least these failures against the old code:

- a tracked PID reused by `svchost.exe` is rejected;
- the same PID with a different creation time is rejected;
- the same PID with a changed executable path/name is rejected;
- an exact owned identity is accepted;
- a system process is rejected even if its identity matches;
- descendants are not absorbed from a stale/reused parent;
- the termination invocation is exact-PID only and never contains `/T`.

Record the failing RED command/output and passing GREEN command/output in the
report. Do not run the real desktop E2E or any test that can invoke real
`taskkill.exe` during this task.

## File allowlist

- `scripts/lib/desktop-process.mjs`
- `scripts/lib/desktop-process-safety.test.mjs` (new, if useful)
- `docs/execution/2026-08-01-desktop-process-cleanup-safety-fix-report.md`

Do not edit any other file. Do not stage or commit. Preserve all existing
dirty-tree changes.

## Worker handoff contract

Before work, boot Keeper as project `pillow`, role `worker`. On completion,
write the report with changed files, RED/GREEN evidence, design decisions,
remaining risks, and an explicit completion status. Return only the status,
test summary, and concerns to the controller.

## Independent review amendment

The first implementation was rejected. The following findings are binding for
the repair pass:

1. A repeated `trackPid(pid)` must never overwrite the first captured
   identity. Same identity may be idempotent; changed identity must remain
   rejected permanently.
2. `knownPids` and the identity store must not diverge. Existing C2/C3 callers
   that filter tracked PIDs and prove natural exit must use a single coherent
   source of truth. A new narrow API may replace direct Set mutation if every
   caller is migrated in this pass.
3. Remove `taskkill.exe` from automatic cleanup entirely. A fresh snapshot
   followed by a numeric-PID command still has a TOCTOU window. Only a stable
   `ChildProcess`/OS process handle owned by the launcher may be terminated.
   Numeric-only descendants are observation-only: wait for natural exit and
   fail closed if they remain.
4. `ProcessSupervisor` must receive the launched child handle from every
   in-repository constructor call. The root handle may be terminated; a stale
   numeric PID must never be used as authority.
5. Distinguish and test natural exit, never-observed identity, identity
   mismatch/PID reuse, terminating, confirmed-dead, and repeated-cleanup
   states. A successful kill request is not completion until exit/absence is
   freshly confirmed.
6. Preserve full process creation precision using an integer tick/file-time
   representation; do not round through `Date.parse`/`toISOString`.
7. Resolve the Windows system directory from `SystemRoot`/`WINDIR` rather than
   hard-coding drive C. Children discovered by parent PID must also have a
   creation time later than their verified parent identity.
8. Tests must exercise the production `ProcessSupervisor` integration through
   injected synthetic snapshots and fake child handles. They must cover the
   C2/C3 tracked-PID filtering contract, repeated tracking, natural exit,
   repeated cleanup, PID reuse, non-C system roots, stale-parent descendants,
   and a kill request whose process has not yet exited.

The repair pass may additionally modify only these constructor/filter callers:

- `scripts/conversation-recovery-desktop.mjs`
- `scripts/native-acceptance-desktop.mjs`
- `scripts/run-cargo.mjs`
- `scripts/smoke-desktop.mjs`
- `scripts/wave-c2-resource-acceptance.mjs`
- `scripts/wave-c3-governed-write-acceptance.mjs`
- `scripts/lib/desktop-smoke-self-test.mjs`

No real process termination test is authorized. RED and GREEN remain purely
synthetic. Do not stage or commit.

## Second review amendment

The stable-root-handle architecture is accepted, but the second review found
seven remaining Important defects. This is the final patch round on this
architecture:

1. Recover from the startup snapshot race. If the constructor snapshot cannot
   see the root, later complete snapshots may establish its first identity only
   while the launcher-owned child handle still proves that original child has
   not exited. Numeric-only tracked descendants must not gain this privilege.
2. Restore a default cleanup observation budget of at least the prior 120
   polls / approximately six seconds. Tests may inject a shorter explicit
   budget.
3. Honor `enforceDesktopClean`. Before success, a fresh observation must prove
   that no `pillow.exe` and no supervised-port listener remains. Any unbound
   residual is reported and never terminated. Make this observation dependency
   injectable for synthetic tests.
4. Process lookup must distinguish `absent`, `present-incomplete`, and
   `present-complete`. Incomplete live identity fails closed; it is not death.
   `confirmed-dead` is terminal and repeated cleanup remains idempotent even if
   that numeric PID is later reused.
5. Restore non-Windows behavior using the launcher-owned `ChildProcess`
   lifecycle/handle. It must not depend on the Windows CIM snapshot. Cover it
   with a synthetic test.
6. On Windows, missing or invalid `SystemRoot`/`WINDIR` must refuse automatic
   root termination. Natural already-confirmed exit need not invoke that gate.
7. Rewrite the report so rejected first-pass design is explicitly historical;
   no current-design paragraph may claim that `taskkill.exe` is still used or
   that `C:\\Windows` is hard-coded.

Add focused RED/GREEN coverage for every item above, including `[] -> [root]`
startup observation, live incomplete identity, terminal confirmed-dead under
PID reuse, default budget, enforced residual proof, missing system root, and a
non-Windows fake child. Continue to prohibit all real process launches and
terminations, desktop E2E, Cargo, Rust, and WebView2. Do not stage or commit.

## Final review closure

The final review accepted the stable-handle architecture and found no Critical
issue. Close exactly these three remaining Important findings without broader
refactoring:

1. Retry non-empty `enforceDesktopClean` residual observations within the
   existing cleanup budget. Store the final observation and report only after
   budget exhaustion. Never terminate an unbound residual.
2. Resolve the Windows root from the first valid non-empty absolute Windows
   path among `SystemRoot` and `WINDIR`. Reject invalid candidates; reject two
   valid but conflicting roots; fail closed before root termination when no
   unambiguous valid root exists. Cover empty-primary fallback, invalid, and
   conflicting values.
3. Any retained non-root `never-observed` record must block cleanup success.
   When a verified parent has a snapshot child whose identity fields are
   incomplete, record that PID as permanent `identity-incomplete`; observe it
   only and never terminate it.

Add RED/GREEN tests for transient residual `nonempty -> empty`, residual budget
exhaustion, SystemRoot fallback/invalid/conflict, numeric-only never-observed
descendant, and discovered incomplete child. No real process or termination
test. No stage or commit.

## Controller live acceptance finding

The controller's full `pnpm check` passed all 246 Vitest tests and the complete
Rust workspace/doctest suite, but the command exited `1` during `run-cargo`
cleanup: `identity mismatch for PID 13424`. Windows Security event 4688 proves
PID 13424 was a short-lived `rustc.exe` descendant and was reused by another
`rustc.exe` during the same Cargo run.

Observation-only descendants have no termination authority. Therefore a
changed identity at the same descendant PID proves the original observed
process is gone; the replacement must never be killed or rebound, but it must
not make root-handle cleanup fail. Add a terminal `identity-replaced` (or
equivalent confirmed-original-dead) state for non-root records. Root identity
mismatch remains fail-closed while the stable child handle is live; if that
handle already reports exit, it is authoritative proof that the original root
is dead and later PID reuse must not fail cleanup.

Add RED/GREEN synthetic coverage for immediate non-root PID reuse without an
intermediate absent snapshot and immediate root PID reuse after the stable
child handle reports exit. Do not weaken incomplete-identity or never-observed
gates. Do not rebind the replacement. No real process test; no stage or commit.

## Controller second live acceptance finding

The next `pnpm check` again passed all Vitest and Rust tests, then cleanup
exited `1` on `incomplete identity for PID 12592`. Security event 4688 maps
that PID to the short-lived `fake_acp_agent.exe` child used by a Rust test.

Keep root incomplete identity as an immediate fail-closed termination gate.
For a non-root observation-only record, `identity-incomplete` must never grant
ownership or termination authority, but it may be observed for the existing
cleanup budget. If a later snapshot shows that PID absent, mark the original
record `confirmed-dead` and allow cleanup to continue. If it remains present
and incomplete (or becomes a different complete identity without an absent
observation), fail after the budget. A non-root incomplete record must not
prevent stable-handle root termination, because that operation targets only
the verified root handle and does not issue a tree/PID kill.

Add RED/GREEN tests for transient non-root `present-incomplete -> absent` and
persistent non-root incomplete budget exhaustion. Preserve the immediate root
incomplete rejection. No real process/Cargo/E2E test; no stage or commit.

## Second-live review correction

Preserve uncertainty across the complete-to-incomplete transition. For a
previously complete non-root record, the sequence `original complete ->
present-incomplete -> different complete` without an observed `absent` state
must not become terminal `identity-replaced` or allow success. Retain an
explicit incomplete/uncertain marker and fail after the observation budget.
Only an observed absence may confirm the original record dead after the
incomplete interval. Add this exact RED/GREEN sequence. Do not change any other
state transition, run real processes, stage, or commit.
