# C3-F Schema Fix — Controller Rejection Correction

Owner: Grok3 storage worker; controller owns acceptance

Controller session: `<keeper-session>`

## Rejection

The migration design and tests are promising, but the delivery is not accepted
because it modified `crates/storage-sqlite/src/lib.rs` outside the whitelist to
publish `apply_migrations`. That expands the production crate API solely for
integration-test convenience and is unnecessary: repository `open` methods
already exercise the real migration path.

## Required Correction

1. Remove only the newly added `apply_migrations` public re-export and its doc
   comment from `src/lib.rs`. Preserve all unrelated accepted dirty-tree lines.
2. Rewrite the new migration tests to trigger production migration through an
   existing public repository open path, preferably
   `SqliteConversationRepository::open(&db)`.
3. Close/drop the setup connection before repository open and reopen a direct
   test connection afterward when assertions require SQL access. Keep foreign
   keys enabled for invariant checks.
4. Preserve the existing RED proof, fresh/legacy/data-preservation/constraint/
   idempotence/rollback coverage. Do not weaken assertions.
5. Do not otherwise redesign the accepted candidate migration unless the real
   repository-open tests expose a defect.

## Exact Whitelist

- `crates/storage-sqlite/src/lib.rs` only to remove the unauthorized two-line
  public API addition
- `crates/storage-sqlite/tests/migrations.rs`
- `docs/execution/2026-08-01-wave-c3f-run-events-schema-fix-report.md`

No other source, migration, harness or product file may change in this pass.

## Gates

- targeted migration tests;
- full storage tests;
- storage all-targets Clippy;
- workspace all-targets check;
- rustfmt check limited to storage Rust files;
- whitelist diff check, empty staging, unchanged HEAD.

Update the report with this rejection/correction and exact results. End with
`COMPLETION: DONE` or `COMPLETION: BLOCKED`; do not stage or commit.
