# C4 D3B3 Task 5B — Grok4 Real Desktop Process E2E Brief

## Assignment

Implement the real Tauri/WebView2 governed-process acceptance harness described in `docs/superpowers/plans/2026-08-05-c4-d3b3-real-desktop-process-acceptance.md`. This is development work, not an audit. Use TDD and finish with the required execution report.

## Exact write allowlist

- `scripts/lib/c4-governed-process-acceptance.mjs` (create)
- `scripts/wave-c4-governed-process-acceptance.mjs` (create)
- `apps/desktop/tests/e2e/c4-governed-process.spec.ts` (create)
- `apps/desktop/package.json`
- `crates/adapter-grok-acp/src/bin/fake_acp_agent/main.rs`
- `crates/adapter-grok-acp/src/bin/fake_acp_agent/scripts.rs`
- `crates/adapter-grok-acp/tests/governed_process_roundtrip.rs` (only if new fixture coverage is required)
- `docs/execution/2026-08-05-wave-c4-d3b3-grok4-real-desktop-process-e2e-report.md` (create)
- Generated ignored evidence only under `.superpowers/c4-process-acceptance/`

Do not write any other file. Production React/Tauri/runtime-host/policy/executor changes are forbidden in this task. If a product defect prevents acceptance, report the exact reproduction and stop.

## Required behavioral proof

- Production Grok ACP composition using the repository fake ACP executable; never `PILLOW_RUNTIME=fake`.
- Real WebView2 CDP interaction through the public UI.
- One approval slot, approve-once/reject, process lifecycle order, complete output routing, replay/conflict, nonzero exit, bounded/lossy output, cancellation/tree cleanup, timeout, project and conversation isolation, forced restart and zero residual processes.
- Screenshots at 1440x960 and 1024x720 with conversation visually dominant and no IDs, paths, grants, credentials, raw RPC, or debug metadata.
- No numeric-PID/name-wide kill. Use existing ownership-based supervisor and fail closed when cleanup ownership is incomplete.

## Required commands

Follow the plan's RED/GREEN commands. At minimum the final report must contain fresh outputs for focused C4 E2E, desktop typecheck/build, adapter all-targets, workspace check, scoped Clippy, fmt, and scoped diff-check.

## Completion

Write `completion: complete` only if the focused real desktop test and every required static gate pass. Otherwise write `completion: blocked` with the exact product or harness blocker. Do not stage, commit, clean, or claim controller acceptance.

