# Wave C3-F Final Resume — After Provider Cancel Recovery

Owner: Grok3 harness worker; controller owns review and acceptance

Worker session: `<keeper-session>`

Runtime allocation: `grok3` / `<keeper-lease>`

Recovery worker session after empty launcher exit:
`<keeper-session>`

Recovery runtime allocation: `grok3` / `<keeper-lease>`

Grok1 recovery session after two empty Grok3 launcher exits:
`<keeper-session>`

Grok1 recovery allocation: `grok1` / `<keeper-lease>`

Status: ACTIVE.

## Starting Truth

The following C3-F blockers are controller-accepted:

- run-event schema upgrade;
- safe cancel-resolution arbitration;
- full-DOM native-path privacy;
- provider cancel RPC completion and same-process next-turn recovery.

The active acceptance contract remains
`docs/execution/2026-08-01-wave-c3f-desktop-integration-brief.md`. HEAD is
`7b7f70f7360131cd1cfd984a8b19bdfb5725c21b`; staging is empty; preserve the
entire accepted dirty tree.

## Task

1. Re-run Node syntax and adapter fixture tests.
2. Run focused C3-F real Tauri/WebView2 acceptance serially from scenario 1.
3. Prove the formerly blocked sequence in one process: cancel a pending write,
   observe its fixed non-success terminal and cleared bar, then start the next
   conversation/turn without timeout.
4. Continue through conflict replay, two-project isolation, forced restart,
   keyboard behavior and both screenshot viewports. Do not stop after the
   recovery point.
5. If focused C3-F is green, run complete desktop E2E serially, `pnpm check`,
   desktop production build, full workspace format check and diff hygiene.
6. Refresh the integration report and acceptance record with exact counts,
   scenario/file hashes, screenshot hashes and zero residual processes.

## Write Whitelist

- `apps/desktop/tests/e2e/c3-governed-write.spec.ts`
- `scripts/wave-c3-governed-write-acceptance.mjs`
- `scripts/lib/c3-governed-write-acceptance.mjs`
- `apps/desktop/package.json`
- existing fake-ACP fixture files only when compilation-proven necessary
- `docs/execution/2026-08-01-wave-c3-integration-report.md`
- generated `.superpowers/c3-acceptance/*.png` and `*.json` evidence

Product implementation is frozen. Stop on a new product defect.

## Gates

- focused C3-F desktop acceptance;
- complete desktop E2E;
- `pnpm check`;
- `pnpm --filter @pillow/desktop build`;
- `cargo fmt --all -- --check` with no waiver;
- `git diff --check`;
- staging empty, HEAD unchanged, residual Pillow/fake ACP/test-owned WebView2
  processes zero.

## Completion

Report `COMPLETION: DONE` or `COMPLETION: BLOCKED`, signal controller session
`<keeper-session>`, and exit. The controller independently reviews
and reruns acceptance. Do not reset, restore, stage, commit, package, publish
or write to the frozen old Keeper tree.
