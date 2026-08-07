# C4 D3B3 Task 5A — Codex2 Provider Process Roundtrip Brief

## Objective

Implement the missing real provider-origin governed process path before the controller attempts the desktop WebView2 E2E. The adapter must accept a strict `pillow/process_tool` RPC, delegate all authority to runtime-host, emit the accepted durable process lifecycle, and return a safe terminal provider response. The desktop conversation service must bind every admitted real run to the governed process authority.

## Required plan

Follow `docs/superpowers/plans/2026-08-05-c4-d3b3-provider-process-roundtrip.md` using TDD. Existing dirty changes belong to other accepted slices; preserve them.

## Exact write allowlist

- `crates/adapter-grok-acp/src/provider_processes.rs` (create)
- `crates/adapter-grok-acp/src/lib.rs`
- `crates/adapter-grok-acp/src/prompt.rs`
- `crates/adapter-grok-acp/src/bin/fake_acp_agent/main.rs`
- `crates/adapter-grok-acp/src/bin/fake_acp_agent/scripts.rs`
- `crates/adapter-grok-acp/tests/governed_process_roundtrip.rs` (create)
- `apps/desktop/src-tauri/src/conversation_service/service.rs`
- `apps/desktop/src-tauri/src/conversation_service/mod.rs` (only to register a focused test module)
- `apps/desktop/src-tauri/src/conversation_service/process_binding_tests.rs` (create only if needed)
- `docs/execution/2026-08-05-wave-c4-d3b3-codex2-provider-process-roundtrip-report.md` (create)

Do not write any other file. If a necessary change falls outside this list, stop and report it as blocked.

## Non-negotiable contracts

1. `keeper-runtime-host` remains the sole proposal/approval/execution/replay/output authority. No new repository, SQLite path, grant surface, or executor.
2. Method and params are strict: `pillow/process_tool` with exact `{sessionId, callId, toolId:"process.run", toolVersion:1, arguments:{program,argv,cwd?}}`. Unknown fields, wrong session, empty identity, wrong types/tool/version fail closed.
3. The adapter never returns grants, captured stdout/stderr, native absolute paths, environment values, internal session IDs, raw gateway errors, or debug metadata.
4. Fresh event order is requested → decided → started → terminal; reject/expire omit started. Provider completion occurs after the terminal event is accepted by the sink.
5. Exact replay returns terminal evidence without another spawn and without duplicate lifecycle events. Same identity with changed arguments fails closed.
6. Desktop binding uses the same run/project/conversation/root as governed reads. Process-registration failure releases the combined binding and fails before provider session start.
7. No staging, commit, reset, restore, clean, dependency update, or formatting outside the allowlist.

## Required evidence

- First RED command and failure reason.
- `cargo test -p keeper-adapter-grok-acp --test governed_process_roundtrip -- --nocapture`
- `cargo test -p keeper-adapter-grok-acp --all-targets -- --nocapture`
- `cargo test -p pillow-desktop --all-targets -- --nocapture`
- `cargo check --workspace --all-targets`
- `cargo clippy -p keeper-adapter-grok-acp -p pillow-desktop --all-targets -- -D warnings`
- `cargo fmt --all -- --check`
- Scoped `git diff --check` for every allowlisted changed file.

## Completion contract

Write the report with changed files, tests and exact counts, safety properties, residual risks, and `completion: complete` or `completion: blocked`. Do not claim controller acceptance. The controller will independently source-review and rerun all gates.

