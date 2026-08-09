# Pillow Wave C3 controller acceptance

Date: 2026-08-02

Verdict: **ACCEPT — Critical 0 / Important 0**

Repository: `this repository`

Branch / HEAD: `main` / `7b7f70f7360131cd1cfd984a8b19bdfb5725c21b`

Staging: empty; no commit was authorized or created.

## Accepted capability

C3 closes one provider-to-human-to-host governed write loop for a single
bounded UTF-8 create or modify. The provider supplies a proposal, while Pillow
owns project/session authority, captures the payload, builds the bounded diff,
requires one-shot approval, rechecks the base hash, performs the atomic host
apply and records metadata-only lifecycle/receipt evidence. Rejection, expiry,
cancellation, conflicting replay, project isolation and restart interruption
all fail closed.

## Controller-owned evidence

- Process safety: 41/41 Node tests, including immutable process identity,
  PID-reuse handling, incomplete-identity fail-closed behavior, older
  parent-PID collision rejection and the exact/default-off MSVC VCTIP boundary.
- Profile helper: 4/4 Node tests; the real LocalAppData WebView2 profile is
  compared to the caller-owned before snapshot and remained unchanged.
- Renderer: 21 Vitest files / 246 tests passed; TypeScript passed.
- Rust: workspace tests, integration tests and 5 compile-fail doctests passed;
  Clippy passed with `-D warnings`.
- Production UI: Vite build passed, 73 modules transformed.
- Focused C3 real desktop: 1/1 passed over real Tauri/WebView2.
- Full serial desktop E2E: 8/8 passed in 662 seconds, covering C1 visual, C2
  resources, C3 writes, conversation recovery, project restore, smoke cleanup
  and Cargo signal cleanup.
- Hygiene: `cargo fmt --all --check`, `git diff --check` and cached diff check
  passed; staging remained empty.

## Real C3 scenarios

The terminal record and `.superpowers/c3-acceptance/c3f-acceptance-record.json`
contain all required scenarios:

1. `reject-create` — no disk mutation;
2. `approve-create` — exact expected bytes and digest;
3. `approve-modify` — base hash checked and exact replacement applied;
4. `base-drift` — compare-and-swap rejection preserves drifted bytes;
5. `cancellation` — no disk mutation;
6. `conflict-replay` — one proposal, no duplicate apply;
7. `two-projects` — no approval/diff/outcome leakage;
8. `forced-restart` — pending proposal becomes interrupted and never auto-applies.

The record reports `status=passed`, `ipcBoundary=real-tauri-webview`, normal
close, forced-restart cleanup and process cleanup passed, with Pillow and port
listener residuals both zero.

## Visual and privacy review

The controller inspected the 1440x960 and 1024x720 screenshots. Conversation
remains the visual center; the one approval bar sits above the composer, and
the diff opens through the existing collapsible context pane. No second task
dashboard or permanent card stack was introduced. Native roots, grants, raw
session/run/tool identifiers, provider-controlled raw output and proposed
content are not persisted or exposed outside the governed diff surface.

## Development provenance this round

- Grok4 CLI developed and verified deterministic write-approval expiry
  fixtures; no product behavior was changed.
- The first profile/process helper changes were produced by a Terra/high Codex
  Desktop subagent and were subsequently reviewed and accepted by the
  controller. They must not be described as account-2 CLI work.
- Independent account-2 Codex CLI (`CODEX_HOME=.codex2`, Terra/high, session
  `019fbe4e-0ae3-7280-98aa-30e19b795400`) developed the bounded `run-cargo`
  exit fixture. A second account-2 CLI run developed the bounded desktop-smoke
  parent/child fixture. The controller reran every affected test and the full
  repository/desktop gates.

## Residual risk

Two generic detached Node interval processes were created by interrupted
account-2 CLI RED/refinement runs, and five similar generic Node processes
predated that task. They have no Pillow/Cargo/Rust identity and no supervised
port listener, but the lost launcher handles mean the controller did not use
numeric forced termination. They remain a host-cleanup concern, not a green
product-process claim. One external Visual Studio `vctip.exe` telemetry helper
also remains outside Pillow ownership and is never terminated by Pillow.

No Critical or Important product/protocol/process-safety issue remains in the
accepted tree. The bounded 500 ms / 5 s desktop-smoke fixture is retained as a
Minor host-timing risk; the controller's focused and aggregate real runs both
passed without creating a new residual.
