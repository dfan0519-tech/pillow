# C4-D1 Grok2 development brief — process approval policy

Role: real Grok CLI slot `grok2`, implementation worker.

Read the three AGENTS files, the accepted C4 design, and Task 2 of
`docs/superpowers/plans/2026-08-02-c4-governed-process-git-implementation.md`.
Boot Keeper as worker for task `c4-d1-process-policy` before editing.

## Deliverable

Implement the pure process state machine with TDD. Fixed public boundary:

- `EXEC_PROPOSAL_TTL_SECS = 600`
- states: pending approval, approved, running, succeeded, failed, rejected,
  expired, cancelled, interrupted;
- decisions: approve once or reject only;
- `Approved -> Running` is the only execution claim;
- terminal states are immutable and restart interrupts every non-terminal
  state rather than resuming it.

## Write allowlist

- `crates/kernel-policy/src/process.rs`
- `crates/kernel-policy/src/lib.rs`
- `crates/kernel-policy/tests/process_policy.rs`
- `docs/execution/2026-08-02-wave-c4-d1-grok2-policy-development-report.md`

Do not modify write-policy semantics or any other file. Do not stage or
commit. Run the Task 2 RED/GREEN gates and report the complete transition
matrix count, changed files, cached-diff state, residual processes and risks.
End with one Keeper signal.
