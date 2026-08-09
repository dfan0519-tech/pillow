# Pillow C4 D2AR — single execution claim and cancelled receipt repair brief

## Worker identity

- Real Grok CLI slot `grok1`.
- Local session worker boot (optional) with `<local-parent-session>`,
  task `c4-d2ar-single-claim-repair`, agent `grok1`.
- Preserve HEAD `7b7f70f`, dirty accepted work and empty staging. No commit,
  reset, restore, clean, package or publish.

## Controller rejection and root cause

The initial ledger is not accepted because it imports write-apply lease
reclaim semantics into process execution:

- `claim_run` returns `Claimed` again to the same owner while already running.
- An expired running lease can be reassigned to any new owner and again returns
  `Claimed`.
- A caller interpreting `Claimed` as spawn authority can start a second process
  while the first is still alive. C4 locks `Approved -> Running` as the only
  execution claim; abnormal running rows are interrupted by recovery, never
  reclaimed.
- Generic `transition` currently permits ownerless `Running -> Cancelled` or
  `Running -> Interrupted`, bypassing the run owner.
- `complete_run` cannot record the policy-defined `Cancelled` terminal with its
  typed process receipt, even though the design requires cancellation flags and
  a bounded terminal result.

Root cause: the implementation mirrored renewable C3 write leases instead of
the one-shot C4 process authority model.

## Required RED -> GREEN repair

1. Read the C4 design, D1 process policy, original D2A brief/report, full
   migration/repository and focused tests.
2. Add RED tests before production edits:
   - first `Approved -> Running` returns `Claimed`;
   - every later claim while `Running` returns `Busy`, including same owner,
     different owner and expired lease;
   - generic `transition` cannot mutate a running row to `Cancelled` or
     `Interrupted` without the owner-aware API;
   - owner-aware `complete_run(..., Cancelled, receipt)` persists and replays a
     typed receipt only when `receipt.cancelled == true`;
   - mismatched state/receipt cancellation flags fail closed.
3. Implement the minimal behavior:
   - `claim_run` never renews or reassigns `Running`; return `Busy(current)`;
   - only `Approved -> Running` writes owner/lease and returns `Claimed`;
   - generic transition rejects all mutations from `Running`;
   - allow `complete_run` for `Succeeded`, `Failed`, and `Cancelled`, with exact
     cancellation-flag consistency;
   - keep `interrupt_run` owner-aware and `recover_interrupted` restart-only.
4. Update SQL coupling so a post-spawn `Cancelled` row may contain the same
   bounded receipt/terminal metadata as succeeded/failed, while a pre-spawn
   pending/approved cancellation may remain receipt-free. Transition history
   follows the same rule.
5. Preserve privacy and every existing reservation/decision/migration
   invariant. Do not change public types unless compilation proves it necessary.
6. Run:
   - `cargo test -p keeper-storage-sqlite --test exec_proposal_repository`
   - `cargo test -p keeper-storage-sqlite --tests`
   - `cargo check --workspace --all-targets`
   - `cargo clippy -p keeper-storage-sqlite --all-targets -- -D warnings`
   - `cargo fmt --all --check`
   - `git diff --check -- crates/storage-sqlite`

## Exclusive repair allowlist

- `crates/storage-sqlite/migrations/0007_exec_proposals.sql`
- `crates/storage-sqlite/src/exec_proposal.rs`
- `crates/storage-sqlite/tests/exec_proposal_repository.rs`
- `docs/execution/2026-08-02-wave-c4-d2ar-grok1-single-claim-repair-report.md`

No other storage file, manifest, lockfile, process executor, protocol, policy,
runtime, Tauri or renderer file may change.

## Completion

Report RED/GREEN evidence, claim matrix, cancelled receipt coupling, exact
files, staging state and risks. Post one completion message and emit
`COMPLETION: complete` or a precise blocker. Controller owns acceptance.
