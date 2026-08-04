# Wave C3-F Resume Brief — Re-run After Schema Acceptance

Owner: Grok3 harness worker; controller owns review and acceptance

Controller session: `<keeper-session>`

Worker session: `<keeper-session>`

Runtime allocation: `grok3` / `<keeper-lease>`

Status: ACTIVE. The separate run-event schema fix is controller-accepted.

## Starting Truth

The first focused C3-F desktop run was correctly blocked by a product storage
constraint that rejected `write_proposal_requested` and
`write_proposal_resolved`. That defect is now fixed and independently accepted
in:

- `docs/execution/2026-08-01-wave-c3f-run-events-schema-fix-report.md`
- `docs/verification/2026-08-01-wave-c3f-run-events-schema-fix-acceptance.md`

HEAD remains `7b7f70f7360131cd1cfd984a8b19bdfb5725c21b`, staging is empty, and all
accepted C1/C2/C3-A-E plus the schema-fix working-tree changes must be
preserved.

## Task

1. Re-read the active C3-F brief and the prior blocked integration report.
2. Re-inspect the existing harness and fake-ACP candidate changes. Treat the
   earlier partial harness as unaccepted until fresh evidence passes.
3. Run Node syntax checks and the narrowly relevant adapter fixture tests.
4. Run the focused real Tauri/WebView2 C3-F acceptance serially against the
   accepted schema.
5. Fix only harness or fake-fixture defects within the whitelist below. If a
   new product renderer/native/runtime/storage defect appears, stop and record
   a minimal deterministic reproduction; do not modify product code.
6. When focused C3-F is green, run complete desktop E2E serially, `pnpm check`,
   the desktop production build, full workspace format check and diff hygiene.
7. Refresh the integration report and generated evidence with exact commands,
   counts, screenshots, hashes, isolation/restart results and residual process
   counts.

## Write Whitelist

- `apps/desktop/tests/e2e/c3-governed-write.spec.ts`
- `scripts/wave-c3-governed-write-acceptance.mjs`
- `scripts/lib/c3-governed-write-acceptance.mjs`
- `apps/desktop/package.json`
- existing C3-D fake-ACP fixture files only when a scenario is compilation-
  proven necessary, including formatting drift in
  `crates/adapter-grok-acp/src/bin/fake_acp_agent/scripts.rs`
- `docs/execution/2026-08-01-wave-c3-integration-report.md`
- generated `.superpowers/c3-acceptance/*.png` and `*.json` evidence

No product renderer, native command, runtime, policy, executor, storage schema
or write-ledger files may be changed.

## Acceptance Gates

- focused C3-F desktop acceptance serially;
- complete desktop E2E serially;
- `pnpm check`;
- `pnpm --filter @pillow/desktop build`;
- `cargo fmt --all -- --check` (the known fake-ACP fixture drift is inside this
  worker's whitelist and must be fixed, not waived);
- `git diff --check`;
- staging empty and HEAD unchanged;
- Pillow/fake ACP/test-owned WebView2 residual processes zero.

## Completion Contract

Update `docs/execution/2026-08-01-wave-c3-integration-report.md` with
`COMPLETION: DONE` or `COMPLETION: BLOCKED`, send the controller a completion
message to parent session `<keeper-session>`, and exit. The
controller will release the runtime lease. Do not reset, restore, stage,
commit, package, publish or write to the frozen old Keeper tree. Worker
completion is not controller acceptance.
