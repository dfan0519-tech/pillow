# Wave C3-C Report — Host Diff And Atomic Write Executor

Owner: grok-c3c controller (final security fix)
Controller session: `<keeper-session>`
Prior sessions: rejection fix `<keeper-session>`, worker `<keeper-session>`
Date: 2026-08-01
Task: `c3c-write-executor` **final security fix** (controller re-acceptance gate)

## Scope

Implement a pure host-side write preparation, bounded unified diff, and atomic
apply layer for C3 governed writes. The gateway:

1. Captures base content under path/deny policy (memory only).
2. Verifies sealed base/proposed content against durable metadata digests.
3. Builds a deterministic, capped zero-context unified diff for review.
4. Applies only after the SQLite ledger already holds an accepted one-shot
   decision: claim apply lease → revalidate → same-directory temp write →
   Windows atomic create/replace → **post-write on-disk verify** → typed
   terminal receipt via `WriteApplyReceipt`.

No runtime host, adapter, Tauri, or React work. Proposed/base content is never
written to SQLite, logs, or run events.

## Controller Rejection (prior)

C3-C was **not accepted**. Tests were green, but controller source review found:

1. **Content-privacy**: `BaseSnapshot` and `BoundedDiff` derived `Debug` with
   source-bearing fields; `PreparedWriteProposal::Debug` included the unified
   diff text.
2. **State machine**: after apply claim, pre-mutation cancel attempted
   `applying -> cancelled` (illegal under C3-A) and fell back to disguising the
   outcome as `write_failed` / durable `Cancelled`.
3. **Diff correctness**: hunk headers ignored empty-side start conventions,
   emitted context without counting it, and final-newline-only changes could
   produce an empty hunk body.
4. **Post-write evidence**: success digests were computed from sealed memory
   rather than reopening and hashing the actual target after the OS atomic op.
5. **Non-Windows create**: `exists` + `rename` could overwrite a racing target.

Those five issues were fixed in the prior rejection pass. **Final security
review still rejected** C3-C on two additional findings (this pass).

## Final Security Findings (this pass)

| # | Finding | Fix |
| --- | --- | --- |
| F1 | Production-visible test mutation hook: `__set_post_atomic_test_hook` was public (`#[doc(hidden)]` only) and ran a thread-local callback between OS atomic success and post-write verify. Downstream production code could call it. | **Removed** the hook, its `thread_local`, the public export from `lib.rs`, and the integration-test dependency. Post-write digest/byte verification is covered by private `#[cfg(test)]` unit tests that call `verify_post_write_target` directly. **No** production-callable hook or global/thread-local mutation callback remains. |
| F2 | Live cancellation was owner-independent: `GovernedWriteToolGateway::interrupt_apply` called generic `transition(..., Interrupted)`, so an expired owner could interrupt an `applying` row reclaimed by a new owner. | Added storage `SqliteWriteProposalRepository::interrupt_apply(proposal_id, owner, now)` requiring state exactly `applying`, exact `apply_owner` match, live lease at `now`, and an atomic `UPDATE ... WHERE proposal_id AND state='applying' AND apply_owner`. On success: history → `interrupted`, lease fields cleared. Owner mismatch / expired lease → `ReservationConflict`, no mutation. Gateway pre-mutation cancel paths pass the current claim owner into this API. **`reconcile_applying` unchanged** (restart-only, owner-independent). Live cancel is **not** routed through generic `transition`. |

## Exact Changed Files (whitelist only)

| Path | Change |
| --- | --- |
| `crates/storage-sqlite/src/write_proposal.rs` | New `interrupt_apply` owner+lease-aware live cancel API |
| `crates/storage-sqlite/tests/write_proposal_repository.rs` | Live owner / wrong owner / expired / reclaimed-stale / history+lease clear tests |
| `crates/tool-executor/src/write.rs` | Remove production test hook; owner-aware gateway interrupt; private post-write unit tests |
| `crates/tool-executor/src/lib.rs` | Drop `__set_post_atomic_test_hook` re-export |
| `crates/tool-executor/tests/governed_write.rs` | Drop hook-based integration test; cancel path still asserts durable `interrupted` |
| `docs/execution/2026-08-01-wave-c3c-write-executor-report.md` | This report |

No stage/commit/reset. `<workspace>\VaultSystem\keeper` not modified. No other product files touched in this pass.

## Interfaces (security-relevant surface)

```rust
// storage-sqlite — live cancel only (not restart reconcile)
pub fn interrupt_apply(
    &self,
    proposal_id: &str,
    owner: &str,
    now: DateTime<Utc>,
) -> Result<StoredWriteProposal, WriteProposalStoreError>;

// tool-executor — no production test hooks
// (removed) pub fn __set_post_atomic_test_hook(...)
pub struct GovernedWriteToolGateway;
// apply cancel after claim → store.interrupt_apply(id, owner, now)
// → GovernedWriteApplyOutcome::Interrupted
```

Typed storage APIs only: `WriteApplyReceipt`, `WriteApplyErrorCode`,
`claim_apply`, `complete_apply`, `interrupt_apply` (live cancel),
`reconcile_applying` (restart only). No free-form receipt JSON.

## TDD Evidence (final security fix)

### Storage

```text
cargo test -p keeper-storage-sqlite --test write_proposal_repository
```

- **24 passed** (includes `live_interrupt_requires_exact_owner_and_live_lease`)
- Correct live owner interrupts → durable `Interrupted`, history
  `pending_approval → approved → applying → interrupted`, lease owner/expiry
  cleared.
- Wrong owner / expired owner (exact boundary) / stale owner after reclaim →
  `ReservationConflict`, state stays `applying`, history unchanged, lease intact.
- Second interrupt after terminal → `ReservationConflict`, history length stable.

### Focused executor

```text
cargo test -p keeper-tool-executor --test write_diff --test governed_write
```

| Target | Result |
| --- | --- |
| governed_write | **16 passed** (pre-mutation cancel → durable interrupted; Debug redaction) |
| write_diff | **12 passed** |

### All tool-executor targets

```text
cargo test -p keeper-tool-executor --all-targets
```

| Target | Result |
| --- | --- |
| lib unit | **5 passed** (4 post-write verify private unit tests + 1 write_diff unit) |
| governed_gateway | 4 passed |
| governed_write | 16 passed |
| read_executor | 9 passed |
| write_diff | 12 passed |
| **Total** | **46 passed** |

Private unit coverage for `verify_post_write_target`:

- exact on-disk bytes/digest accepted
- byte-length mismatch → `WriteFailed`
- same-length digest mismatch → `WriteFailed`
- missing/unreadable target → `WriteFailed`

### Workspace / formatting / staging

```text
cargo check --workspace --all-targets  → Finished (no errors)
cargo fmt --all -- --check             → exit 0
git diff --check                       → exit 0 (existing LF/CRLF warnings only)
git diff --cached --name-only          → empty (staging empty)
```

## Residual Risks

1. **Non-Windows create via hard_link**: fails closed when hard links are
   unsupported or cross-filesystem; not exercised on this Windows host.
2. **Hostile admin / exclusive locks**: OS may still fail the atomic op;
   coded fail-closed as `write_failed` with temp cleanup.
3. **Runtime coordinator not in this wave**: sealing, grant issuance, UI, and
   run-event emission remain C3-D/E. This crate assumes durable approve +
   in-memory sealed payload are already established by the host.
4. **Diff is display-only**: review hunks are not the apply identity; digests are.
5. **Post-write failure after successful create** leaves the on-disk target as
   written by the OS/racer; ledger is failed, not succeeded (no silent success).
6. **Generic `transition(..., Interrupted)` still exists** for non-apply ledger
   uses (e.g. approved/pending → interrupted). Live apply cancel **must** use
   `interrupt_apply`; gateway does. Restart orphaned `applying` continues to use
   `reconcile_applying` only.
7. **Unit-level post-write coverage** exercises the private verifier, not a
   full OS-atomic-then-corrupt end-to-end path (that path required a production
   mutation hook and was intentionally removed).

## Working-Tree Governance

- No staging, commit, discard, or force operations.
- Whitelist-only product edits for this final security fix.
- `<workspace>\VaultSystem\keeper` not modified.
- `git diff --cached --name-only` empty.

## COMPLETION: DONE — final security findings F1 and F2 fixed; required gates green; ready for independent reverify
