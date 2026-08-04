# C4-D1 Grok2 development report — process approval policy

Date: 2026-08-02

Worker: real Grok CLI slot `grok2`
Worker session: `<keeper-session>`
Controller parent session: `<keeper-session>`
Task: `c4-d1-process-policy`
Status: **DONE** (not controller-accepted)

## Objective

Implement the pure C4 process-proposal approval state machine in
`keeper-policy` with TDD. Fixed public boundary only; no SQLite, executor,
renderer, or write-policy semantic changes.

## Public boundary delivered

| Symbol | Value / role |
| --- | --- |
| `EXEC_PROPOSAL_TTL_SECS` | `600` |
| `ExecProposalState` | `PendingApproval`, `Approved`, `Running`, `Succeeded`, `Failed`, `Rejected`, `Expired`, `Cancelled`, `Interrupted` |
| `ExecDecision` | `ApproveOnce`, `Reject` only |
| `ExecPolicyError` | `IllegalTransition`, `AlreadyTerminal`, `Expired`, `DecisionNotAllowed` |
| `validate_exec_transition` | pure graph gate |
| `apply_exec_decision` | pending-only, expiry-aware single-use decision |
| `is_exec_terminal` / `is_exec_expired` | terminal/expiry helpers (exec-scoped names) |

Write enums (`WriteProposalState`, `WriteDecision`, `PolicyError`, …) remain
separate and untouched.

## Transition graph (exact)

```text
pending_approval -> approved | rejected | expired | cancelled | interrupted
approved         -> running | expired | cancelled | interrupted
running          -> succeeded | failed | cancelled | interrupted
```

Invariants:

1. **`Approved -> Running` is the only execution claim.** No other state may
   enter `Running`.
2. **Terminal states are immutable** (all self and non-self edges denied).
3. **Restart interrupts every non-terminal state**
   (`PendingApproval` / `Approved` / `Running` → `Interrupted`); terminals
   never reopen, including into `Interrupted`.
4. Decisions only from `PendingApproval` when not expired; second decision after
   approve is `DecisionNotAllowed`.

### Complete transition matrix count

| Metric | Count |
| --- | --- |
| States | **9** |
| Full pair space | **81** (9×9) |
| **Allowed edges** | **13** |
| **Denied edges** | **68** |
| Allowed breakdown | Pending 5 + Approved 4 + Running 4 |

Locked by test `complete_transition_matrix_enumerates_all_edges`.

## Changed files (allowlist only)

| Path | Action |
| --- | --- |
| `crates/kernel-policy/src/process.rs` | **Created** — pure process state machine |
| `crates/kernel-policy/src/lib.rs` | **Modified** — module + re-exports for exec API |
| `crates/kernel-policy/tests/process_policy.rs` | **Created** — RED/GREEN matrix and decision tests |
| `docs/execution/2026-08-02-wave-c4-d1-grok2-policy-development-report.md` | **Created** — this report |

**Not modified:** `write.rs`, `write_policy.rs`, any other crate or path.

## TDD gates

### RED

```text
cargo test -p keeper-policy --test process_policy -- --nocapture
```

- **Exit non-zero** as expected: unresolved imports for
  `EXEC_PROPOSAL_TTL_SECS`, `ExecProposalState`, `ExecDecision`,
  `ExecPolicyError`, `validate_exec_transition`, `apply_exec_decision`,
  `is_exec_terminal`, `is_exec_expired` (process module absent).

### GREEN

```text
cargo test -p keeper-policy --tests
cargo fmt --all --check
git diff --check -- crates/kernel-policy
```

| Command | Result |
| --- | --- |
| `cargo test -p keeper-policy --tests` | **process_policy 21/21**, **write_policy 16/16**, lib unit 0 (empty); exit 0 |
| `cargo fmt --all --check` | exit 0 (after one fmt pass on long assert lines) |
| `git diff --check -- crates/kernel-policy` | exit 0 |

Write-policy regression: all 16 prior tests still pass; write semantics unchanged.

## Staging / commit / cached-diff

| Check | Result |
| --- | --- |
| `git diff --cached` | **empty** (no stage) |
| Commit | **not performed** |
| Working tree | pre-existing wide dirty tree from prior waves; this worker only wrote allowlist paths |

## Residual processes

| Class | Observation |
| --- | --- |
| Pillow desktop / fake-ACP | **none** attributed to this task |
| Cargo / process_policy test runners | **none** remaining after GREEN |
| Edge WebView2 | multiple host processes present; pre-existing / unrelated to this pure-policy crate work |

No process spawn, Job Object, or desktop harness was used for Task 2.

## Risks / follow-ups (controller)

1. **Not controller-accepted.** Independent review of the 13-edge graph vs
   accepted C4 design is still required.
2. **Storage / executor not in scope.** Reservation, single-use grant
   persistence, run claim, and restart interrupt application live in later
   tasks; this crate only defines pure transitions.
3. **`Running -> Cancelled` is allowed** (unlike write `Applying`, which
   cannot cancel). Controllers should confirm this matches C4 intent for
   user/host cancel during execution.
4. **Dirty sibling worktree** may include concurrent D1 sibling edits
   (protocol, UI, git parser); this report claims only the four allowlist
   files above.

## Keeper signal

```text
DONE - C4-D1 process approval policy (Task 2). Pure exec state machine in
keeper-policy; matrix 13 allowed / 68 denied / 81 total; process_policy 21/21
+ write_policy 16/16; fmt/diff-check green; staging empty; no commit.
Not controller-accepted.
```
