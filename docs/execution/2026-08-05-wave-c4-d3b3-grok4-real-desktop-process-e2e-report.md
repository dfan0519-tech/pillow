# C4 D3B3 Task 5B — Grok4 Real Desktop Process E2E Report

**Worker:** grok4 CLI (registered Pillow worker)  
**Date:** 2026-08-05  
**Task:** Real Tauri/WebView2 governed-process acceptance harness  
**Controller acceptance:** not claimed

## completion: complete

## Changed files (exact allowlist)

| Path | Action |
| --- | --- |
| `scripts/lib/c4-governed-process-acceptance.mjs` | create |
| `scripts/wave-c4-governed-process-acceptance.mjs` | create |
| `apps/desktop/tests/e2e/c4-governed-process.spec.ts` | create |
| `apps/desktop/package.json` | modify (`test:e2e` append) |
| `crates/adapter-grok-acp/src/bin/fake_acp_agent/main.rs` | modify (script catalog docs) |
| `crates/adapter-grok-acp/src/bin/fake_acp_agent/scripts.rs` | modify (new scenarios + desktop multiplex) |
| `crates/adapter-grok-acp/tests/governed_process_roundtrip.rs` | modify (nonzero + output-bounds fixtures) |
| `docs/execution/2026-08-05-wave-c4-d3b3-grok4-real-desktop-process-e2e-report.md` | create |

Generated ignored evidence under `.superpowers/c4-process-acceptance/` only.

## First RED

1. **Adapter fixture (nonzero script):**  
   `cargo test -p keeper-adapter-grok-acp --test governed_process_roundtrip`  
   Failed because production maps nonzero `exit_code` to a fixed provider Failed error; the fake script expected a success result with `exit_code: 7`. Fixed by accepting the provider-safe Failed error shape.

2. **Desktop E2E (first full run):**  
   Failed at `output-bounds` with WebView page closed under large terminal body load; then at `cancel-tree` because host decide is synchronous through execute (approval bar stays submitting for long-running commands); then cancel race required fake ACP to emit `stopReason: cancelled` so the run state machine does not observe Cancelling → Completed; then conversation/project isolation needed serial ACP turns and row scanning for `"新对话"` titles.

## Final GREEN

| Command | Result | Notes |
| --- | --- | --- |
| `node --check scripts/lib/c4-governed-process-acceptance.mjs` | PASS | |
| `node --check scripts/wave-c4-governed-process-acceptance.mjs` | PASS | |
| `cargo test -p keeper-adapter-grok-acp --test governed_process_roundtrip -- --nocapture` | PASS | 5/5 |
| `cargo test -p keeper-adapter-grok-acp --all-targets -- --nocapture` | PASS | live-account test remains ignored |
| `pnpm --filter @pillow/desktop exec node --test --test-concurrency=1 tests/e2e/c4-governed-process.spec.ts` | PASS | **1 pass / 0 fail**, ~453.6 s wall |
| `pnpm --filter @pillow/desktop typecheck` | PASS | |
| `pnpm --filter @pillow/desktop build` | PASS | |
| `cargo check --workspace --all-targets` | PASS | |
| `cargo clippy -p keeper-adapter-grok-acp -p pillow-desktop --all-targets -- -D warnings` | PASS | |
| `cargo fmt --all -- --check` | PASS | |
| `git diff --check -- <allowlist>` | PASS | |

## Scenario matrix

| Scenario | Result | Evidence |
| --- | --- | --- |
| reject | passed | single bar; 已拒绝; no start success path |
| approve-success | passed | approve once; 已成功; exit 0 |
| terminal-output | passed | 完整输出 opens contextual `terminal-output-view` |
| replay-conflict | passed | exact replay + changed-args fail-closed labels; no dual bars |
| nonzero-exit | passed | 已失败; exit 7 |
| output-bounds | passed | 输出已截断 on activity row (lossy may fold into truncate) |
| cancel-tree | passed | markersBefore=2; residualMarked=0; child cleanup |
| timeout | passed | fixed 60s → 已失败; residual node zero |
| two-project-isolation | passed | serial ACP turns; B never shows A markers; restore A |
| conversation-switch | passed | foreign cannot host A markers; scan-rows restore |
| forced-restart | passed | interrupted; residualZero; no auto-resume |

**Expiry:** not simulated with a production force seam; covered by accepted lower-layer process expiry tests (per plan).

## Runtime path

- Production Grok ACP composition (`PILLOW_GROK_EXECUTABLE` = repository-built `fake_acp_agent`)
- Sibling `keeper_fake_acp_script.txt` selects `governed-process-desktop`
- **Never** `PILLOW_RUNTIME=fake`
- Real WebView2 CDP; public composer / approval / activity / terminal surfaces only

## Screenshots (privacy-safe)

Under `.superpowers/c4-process-acceptance/`:

- `reject-approval-bar-1440x960.png`
- `approve-bar-1440x960.png`
- `terminal-output-1440x960.png`
- `conversation-dominant-1024x720.png`
- `forced-restart-before-1440x960.png`

Capture path asserts: no drive-root paths, UUIDs, `sessionId`/`runId`/`proposalId`/`approvalGrant`, bearer/token markers, or raw JSON-RPC fields in body text/HTML.

## Cleanup evidence

From acceptance record:

```json
{
  "processCleanup": "passed",
  "normalClose": "passed",
  "forcedRestartCleanup": "passed",
  "residualProcesses": 0
}
```

- Ownership-based `ProcessSupervisor` only (no numeric-PID / name-wide kill for cleanup).
- Real LocalAppData WebView2 profile snapshot unchanged.
- Forced-restart residual marked node processes zero after supervised cleanup.

## Residual risks

1. **Synchronous process decide:** Host `decide_process_proposal` blocks through execute; long-running approve leaves the bar submitting until terminal. Harness uses `approveCommandOnce(page, { waitForClear: false })` for hang/timeout paths — product UX may still look “stuck” during long runs.
2. **Serial ACP gate:** Concurrent dual pending process proposals across projects is product-infeasible; isolation proof uses serial turns (same as C3 writes).
3. **Output lossy notice:** Activity may show truncation without a separate lossy chip when host folds flags; suite accepts either visible notice.
4. **Wall-clock cost:** Full focused E2E is ~7–8 minutes (debug Tauri build + 60s timeout scenario + multi-session restarts).

## Not claimed

- Controller acceptance / aggregate serial E2E suite
- C4 checkpoint or verification documents
- Staging or commit of any files
