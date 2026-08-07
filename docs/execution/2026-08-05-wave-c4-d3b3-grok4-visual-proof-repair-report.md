# C4 D3B3 Task 5B-R — Grok4 Visual Proof Repair Report

**Worker:** grok4 CLI (registered Pillow worker)  
**Date:** 2026-08-05  
**Task:** Repair false-green `conversation-dominant-1024x720.png` visual proof  
**Controller acceptance:** not claimed

## completion: complete

## Controller rejection (RED reason)

The real desktop focused E2E previously passed, but controller visual inspection
rejected `.superpowers/c4-process-acceptance/conversation-dominant-1024x720.png`:

- Empty conversation stage (`开始新对话` placeholder)
- Right pane was the **Resource browser** (`资源`), not process activity/output
- Did **not** prove in-conversation Agent process activity with a terminal
  output contextual pane open

Harness false-green causes:

1. 1024×720 capture ran after `conversation-switch` + bare `selectProject`, which
   often left an empty stage with no activity rows.
2. `openCommandOutput({ page }).catch(() => undefined)` swallowed open failures.
3. `assertConversationDominant` only checked workbench surface / approval slot
   and allowed empty stage + Resource browser to pass.

## Changed files (exact allowlist)

| Path | Action |
| --- | --- |
| `scripts/lib/c4-governed-process-acceptance.mjs` | strengthen visual/process assertions; prefer completed activity for open/output |
| `scripts/wave-c4-governed-process-acceptance.mjs` | fail-closed 1024×720 proof path (no swallow; require activity + terminal) |
| `apps/desktop/tests/e2e/c4-governed-process.spec.ts` | unchanged contract (still on allowlist; no weaken) |
| `docs/execution/2026-08-05-wave-c4-d3b3-grok4-visual-proof-repair-report.md` | create (this report) |

Generated ignored evidence only under `.superpowers/c4-process-acceptance/`.

**Production code:** not modified (React / Tauri / runtime-host / policy / executor
remain read-only).

## Repair summary

### Helper (`c4-governed-process-acceptance.mjs`)

- `assertConversationDominant` now rejects the empty `开始新对话` stage and
  requires conversation surface width ≥ contextual pane when both are present.
- New `isEmptyConversationStage` / `isResourceBrowserPaneOpen` detectors.
- New `assertProcessVisualProof`: requires completed process activity status +
  details, visible `terminal-output-view`, body or durable-only notice, and
  **not** Resource browser mode; privacy gate retained.
- New `openConversationWithCompletedProcessActivity`: scans project conversations
  and **prefers** executed statuses (`已成功` / `已失败` / cancel / interrupt)
  over reject-only for the visual proof.
- `openCommandOutput` prefers a completed activity row that exposes `完整输出`.

### Orchestrator (`wave-c4-governed-process-acceptance.mjs`)

1024×720 path now:

1. Set viewport 1024×720
2. Select Project A and collapse Resource browser if open
3. Open a conversation with completed process activity (or run a fresh
   `[c4p:approve]` visual-proof turn if none found)
4. Call `openCommandOutput` **without** swallowing errors
5. Call `assertProcessVisualProof` before capture
6. Capture privacy-safe screenshot

Strict terminal completion record contract unchanged
(`schemaVersion`, `status`, `ipcBoundary`, `runtimePath`, scenarios, viewports,
cleanup fields, `residualProcesses: 0`).

## GREEN evidence

| Command | Result | Notes |
| --- | --- | --- |
| `node --check scripts/lib/c4-governed-process-acceptance.mjs` | PASS | |
| `node --check scripts/wave-c4-governed-process-acceptance.mjs` | PASS | |
| `pnpm --filter @pillow/desktop exec node --test --test-concurrency=1 tests/e2e/c4-governed-process.spec.ts` | PASS | **1 pass / 0 fail**, ~468.0 s wall (final) |
| `pnpm --filter @pillow/desktop typecheck` | PASS | |
| `git diff --check -- <allowlist harness files>` | PASS | |

Environment note: grok4 slot `USERPROFILE` points at a home without a local
rustup toolchain; focused E2E was run with `RUSTUP_HOME=<host-user-profile>\.rustup`
and `CARGO_HOME=<host-user-profile>\.cargo` so cargo/rustc resolve. Not a product
code change.

## Screenshot path (worker-inspected)

`.superpowers/c4-process-acceptance/conversation-dominant-1024x720.png`

Worker visual inspection of the replacement capture:

| Check | Observed |
| --- | --- |
| Empty stage? | **No** — timeline shows user prompt and assistant label |
| Resource browser (`资源`)? | **No** — right pane heading is **终端输出** |
| Process activity row? | **Yes** — `代理 已成功 node "--version"` |
| Meaningful status/details? | **Yes** — 已成功, 决定: 批准一次, 退出 0, 114 ms, 已捕获 10 字节, 完整输出 |
| Terminal contextual pane? | **Yes** — program/argv, 10 字节, 退出 0, body `v24.16.0` |
| Conversation dominant? | **Yes** — center conversation stage wider than subordinate pane |
| Privacy surface? | No drive-root paths, UUIDs, grant/session/run ids, or raw RPC in view |

## Remaining risks

1. Conversation row titles remain generic (`新对话`); the harness scans by
   activity content rather than title. If UI later detaches activity from the
   timeline, the scan must be re-validated.
2. Preferential status ranking can still fall back to reject-only when no
   executed process row exists; assertions still require completed status +
   terminal pane, so empty/Resource false-green remains blocked.
3. Full serial desktop E2E suite and controller aggregate gate are **not** run
   by this worker.
4. Slot-local rustup isolation can still break harness builds unless
   `RUSTUP_HOME` / `CARGO_HOME` point at the machine toolchain install.

## Not claimed

- Controller acceptance
- Aggregate serial E2E suite
- C4 checkpoint / verification documents
- Staging or commit of any files
- Production defect (none found; harness-only false-green)
