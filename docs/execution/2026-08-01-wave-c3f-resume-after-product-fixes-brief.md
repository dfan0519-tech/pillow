# Wave C3-F Resume Brief — Full Re-run After Product Fixes

Owner: Grok3 harness worker; controller owns review and acceptance

Worker session: `<keeper-session>`

Runtime allocation: `grok3` / `<keeper-lease>`

Status: ACTIVE. The schema, cancel-resolution and native-path privacy blockers
are controller-accepted.

## Accepted Prerequisites

- `docs/verification/2026-08-01-wave-c3f-run-events-schema-fix-acceptance.md`
- `docs/verification/2026-08-01-wave-c3f-cancel-resolution-fix-acceptance.md`
- `docs/verification/2026-08-01-wave-c3f-native-path-privacy-fix-acceptance.md`

HEAD remains `7b7f70f7360131cd1cfd984a8b19bdfb5725c21b`, staging is empty, and the
accepted dirty tree must be preserved.

## Task

1. Re-read the active C3-F brief and both prior blocked integration findings.
2. Treat the harness as still unaccepted except for the controller-accepted
   restored full-DOM path privacy assertion.
3. Re-run Node syntax and relevant fake-ACP adapter tests.
4. Run the focused real Tauri/WebView2 C3-F acceptance serially from the first
   scenario. Confirm cancellation now produces a non-success write terminal,
   clears the bar, never mutates the target and does not affect a foreign
   conversation.
5. Complete conflict replay, two-project isolation, forced restart, keyboard
   behavior and both required screenshot viewports. Do not stop after only the
   formerly failing scenario.
6. When focused C3-F is green, run complete desktop E2E serially, `pnpm check`,
   desktop production build, full workspace format check and diff hygiene.
7. Refresh the integration report and acceptance record with exact commands,
   counts, per-scenario hashes, screenshot hashes and zero residual process
   evidence.

## Write Whitelist

- `apps/desktop/tests/e2e/c3-governed-write.spec.ts`
- `scripts/wave-c3-governed-write-acceptance.mjs`
- `scripts/lib/c3-governed-write-acceptance.mjs`
- `apps/desktop/package.json`
- existing C3-D fake-ACP fixture files only when a scenario is compilation-
  proven necessary, including formatting the known fixture drift
- `docs/execution/2026-08-01-wave-c3-integration-report.md`
- generated `.superpowers/c3-acceptance/*.png` and `*.json` evidence

All product implementation files are frozen for this pass. If another product
defect appears, stop with a minimal deterministic reproduction.

## Gates

- focused C3-F desktop acceptance serially;
- complete desktop E2E serially;
- `pnpm check`;
- `pnpm --filter @pillow/desktop build`;
- `cargo fmt --all -- --check` with no known-drift waiver;
- `git diff --check`;
- staging empty, HEAD unchanged and test-owned residual processes zero.

## Completion

Update `docs/execution/2026-08-01-wave-c3-integration-report.md` with
`COMPLETION: DONE` or `COMPLETION: BLOCKED`, signal controller session
`<keeper-session>`, and exit. The controller releases the lease
and independently accepts or rejects the evidence. Do not reset, restore,
stage, commit, package, publish or write to the frozen old Keeper tree.
