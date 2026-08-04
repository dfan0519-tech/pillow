# Wave C3-C controller final security fix brief

## Decision

C3-C remains rejected until both findings below are fixed and independently reverified.

## Finding 1: production-visible test mutation hook

`crates/tool-executor/src/write.rs` currently exports `__set_post_atomic_test_hook` and executes the registered callback between the atomic filesystem operation and post-write verification. `#[doc(hidden)]` does not make this test-only: downstream production code can call it. Remove the hook, its public export, and the integration-test dependency on it. Preserve post-write verification coverage with a private `#[cfg(test)]` unit seam or by directly testing the private verification helper inside the module. No callable production hook or global/thread-local mutation callback may remain.

## Finding 2: cancellation must be lease-owner-aware

`GovernedWriteToolGateway::interrupt_apply` currently calls generic owner-independent `transition(..., Interrupted, ...)`. An expired owner can therefore interrupt an `applying` row that a new owner has reclaimed. Add a narrow storage API for live cancellation/interruption that requires:

- current state exactly `applying`;
- exact `apply_owner` match;
- live lease at the supplied time;
- atomic update predicate including proposal id, state, and owner;
- transition history to `interrupted`, lease fields cleared;
- owner mismatch or expired lease returns `ReservationConflict` and does not mutate state/history.

Call that API from both pre-mutation cancellation paths in the write gateway, passing the current owner. Do not weaken restart-only `reconcile_applying`; do not route live cancellation through the generic transition API.

## Write whitelist

- `crates/storage-sqlite/src/write_proposal.rs`
- `crates/storage-sqlite/tests/write_proposal_repository.rs`
- `crates/tool-executor/src/write.rs`
- `crates/tool-executor/src/lib.rs`
- `crates/tool-executor/tests/governed_write.rs`
- `docs/execution/2026-08-01-wave-c3c-write-executor-report.md`

No other files may be changed.

## Required tests

- storage: correct live owner interrupts; wrong owner cannot; expired owner cannot; reclaimed owner cannot be interrupted by stale owner; history and lease clearing are exact.
- executor: cancellation reaches durable `interrupted` through owner-aware API; source-bearing Debug remains redacted; post-write digest/byte verification still has direct private test coverage without a production hook.
- `cargo test -p keeper-storage-sqlite --test write_proposal_repository`
- `cargo test -p keeper-tool-executor --test write_diff --test governed_write`
- `cargo test -p keeper-tool-executor --all-targets`
- `cargo check --workspace --all-targets`
- `cargo fmt --all -- --check`
- `git diff --check`
- `git diff --cached --name-only` must remain empty.

## Completion contract

Update the C3-C report with exact changed files, commands/results, remaining risks, and an explicit completion signal. Do not stage or commit.
