# C4 D3B3 gate repair — Grok4 run-cargo cleanup race brief

## Role and outcome

You are the real Grok CLI `grok4` implementation worker. Repair the Windows
process-supervision race that makes a successful `pnpm check` exit 1 after all
renderer and Rust tests have passed.

The controller has reproduced the failure twice. In both runs the reported
observation-only PID disappeared naturally immediately after the command:

- `Desktop cleanup refused: observation-only PIDs remain [33344]`
- `Desktop cleanup refused: observation-only PIDs remain [9852]`

Do not weaken the fail-closed safety boundary. The fix must distinguish a
tracked descendant that has been freshly proven absent or identity-replaced
from a descendant whose identity is incomplete, never observed, reused, or
still live.

## Mandatory startup

1. Work only in `<repo>`.
2. Read `AGENTS.md` and this brief before editing.
3. Boot Keeper once as a record-only worker with project `pillow`, task
   `c4-d3b3-run-cargo-cleanup-race`, agentName `grok4`, agentRole `worker`.
   Do not request a context pack, ask Keeper to execute work, or write Vault.
4. Inspect the current dirty tree before editing. Preserve every unrelated
   change. Do not stage, commit, reset, clean, or restore files.

## Write allowlist

You may modify only:

- `scripts/lib/desktop-process.mjs`
- `scripts/lib/desktop-process-safety.test.mjs`
- `scripts/run-cargo.mjs`
- `docs/execution/2026-08-05-wave-c4-d3b3-grok4-run-cargo-cleanup-race-report.md`

Do not modify any other file. If the repair requires a broader path, stop and
report the exact need instead of crossing the allowlist.

## Required behavior

1. Add a deterministic regression fixture that models the reproduced normal
   path: the launcher-owned root exits successfully; a fully identified
   descendant was observed; the next fresh process snapshot proves that exact
   identity absent; cleanup succeeds without issuing a numeric-PID or
   name-wide termination.
2. Preserve or add deterministic rejection coverage for:
   - a still-live observed descendant;
   - an incomplete or never-observed descendant;
   - root identity mismatch;
   - descendant PID reuse / identity replacement;
   - unsafe system-process identity;
   - a real residual Pillow desktop process or port listener.
3. Keep termination launcher-owned. Never add `taskkill`, numeric-PID kill,
   name-wide kill, WMI deletion, or an unconditional grace-period success.
4. Avoid a fixed sleep as the correctness mechanism. Every success decision
   must use a fresh snapshot and the recorded immutable identity.
5. Extend `scripts/run-cargo.mjs --self-test` if needed so the normal successful
   child path and signal cleanup path remain covered.
6. Do not special-case the two observed numeric PIDs or Cargo executable names.

## Tests to run

Run the narrow tests first, then the aggregate gate only if the narrow tests
are green:

```powershell
node scripts/lib/desktop-process-safety.test.mjs
node scripts/run-cargo.mjs --self-test
pnpm check
git diff --check -- scripts/lib/desktop-process.mjs scripts/lib/desktop-process-safety.test.mjs scripts/run-cargo.mjs docs/execution/2026-08-05-wave-c4-d3b3-grok4-run-cargo-cleanup-race-report.md
```

Do not run desktop E2E in this task; the controller owns that acceptance gate.
If `pnpm check` again fails only on an observation-only PID, capture the exact
state transition and stop after one aggregate retry. Do not kill the PID.

## Completion contract

Create the report file in the allowlist with:

- root cause and why the repair is safe;
- exact changed files;
- tests and exit codes;
- any remaining risk;
- explicit confirmation that no other path was changed, staged, committed,
  reset, cleaned, or restored.

End the final worker response with exactly one line beginning:

`C4-D3B3-GROK4-CLEANUP-RACE-COMPLETE`

