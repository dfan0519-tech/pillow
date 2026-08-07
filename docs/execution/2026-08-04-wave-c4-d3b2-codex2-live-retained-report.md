# C4 D3B2 Codex2 — live-retained output truth report

## Delivered

- Projected host-owned `ProcessOutputView.live_retained` to native
  `ExecOutputDto.liveRetained` and the renderer-facing `ExecOutputView` as a
  required boolean.
- Kept the field presentation-only. The output DTO still omits host-private
  program/argv material and exposes no approval or execution authority.
- Extended the strict output normalizer to require a boolean `liveRetained`,
  preserve it, and reject missing, wrong-type, approval, and debug fields.

## Changed files

- `apps/desktop/src-tauri/src/conversation_service/exec_service.rs`
- `apps/desktop/src-tauri/src/exec_commands_tests.rs`
- `apps/desktop/src/lib/execGateway.ts`
- `apps/desktop/src/lib/tauriExecGateway.ts`
- `apps/desktop/tests/c4-gateway-contract.test.ts`
- `docs/execution/2026-08-04-wave-c4-d3b2-codex2-live-retained-report.md`

## Focused verification

1. `node scripts/run-cargo.mjs test -p pillow-desktop exec_commands_tests`
   - PASS: 7 passed, 0 failed (86 filtered in the desktop library target).
2. `pnpm --filter @pillow/desktop exec vitest run tests/c4-gateway-contract.test.ts`
   - PASS: 1 file, 6 tests passed.
3. `pnpm --filter @pillow/desktop typecheck`
   - PASS: `tsc --noEmit`.

An initial package-script invocation also ran the broader desktop suite because
its argument forwarding did not select the requested test file. It passed:
24 files / 283 tests. The direct Vitest command above is the focused recorded
gateway gate.

## Residual risk

This slice deliberately does not mount or consume the new display metadata in
the D3B UI. Controller acceptance and any later UI behavior remain separate.

COMPLETION: complete
