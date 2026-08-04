# Grok4 brief: C4 security model and acceptance research

You are the real Grok CLI worker in slot `grok4`.

## Goal

Produce an adversarial threat model and executable acceptance matrix for C4
terminal and Git, grounded in Pillow's C3 authority and Windows process-safety
lessons.

## Required evidence

1. Read the C3 governed-write design, final acceptance, process supervisor
   tests and forced-restart incident documentation.
2. Inspect the local Grok App permission/process/Git/terminal boundaries at its
   pinned commit.
3. Inspect current Pillow protocol validation, policy, receipts, cancellation,
   restart recovery and desktop E2E harness.
4. Treat reference implementations as evidence, not authority.

## Required threat coverage

- command injection and approval-text/actual-command mismatch;
- cwd escape, symlink/reparse-point escape and project-switch races;
- environment/credential leakage and malicious output escape sequences;
- output flooding, encoding faults, child/grandchild escape and cancellation;
- PID reuse, incomplete identity, host restart and orphan processes;
- Git option injection, config aliases/hooks, filters, external diff/textconv;
- repository ownership, nested repos, submodules and worktree escapes;
- diff/base drift, stale status, conflicts and destructive restore;
- cross-project/cross-conversation approval replay;
- DOM/log/SQLite/screenshot privacy;
- denial-of-service and performance budgets.

## Required output

Define a severity model, a capability-by-capability threat table, focused unit
and integration tests, real desktop adversarial scenarios, residual-process
proof and explicit Critical/Important rejection criteria.

## Write allowlist

Only create or update:

`<workspace>\pillow\docs\research\2026-08-02-c4-security-acceptance-research.md`

Do not change any other file. Finish with `Status: DONE` or
`Status: BLOCKED`, changed-files truth, residual risks and one Keeper signal.
