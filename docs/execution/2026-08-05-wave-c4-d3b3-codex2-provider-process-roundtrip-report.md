# C4 D3B3 Task 5A — Codex2 Provider Process Roundtrip Report

completion: complete

## Scope

Changed only Task 5A allowlist files:

- `crates/adapter-grok-acp/src/provider_processes.rs` (new)
- `crates/adapter-grok-acp/src/lib.rs`
- `crates/adapter-grok-acp/src/prompt.rs`
- `crates/adapter-grok-acp/src/bin/fake_acp_agent/main.rs`
- `crates/adapter-grok-acp/src/bin/fake_acp_agent/scripts.rs`
- `crates/adapter-grok-acp/tests/governed_process_roundtrip.rs` (new)
- `apps/desktop/src-tauri/src/conversation_service/service.rs`

No files were staged, committed, reset, restored, or cleaned.

## RED evidence

First RED command:

```text
cargo test -p keeper-adapter-grok-acp --test governed_process_roundtrip -- --nocapture
```

Result: 1 passed, 1 failed. The new approve roundtrip timed out waiting for a
governed process proposal because `pillow/process_tool` had no ACP handler.

## Implemented behavior

- Added strict `pillow/process_tool` normalization for exactly
  `{sessionId, callId, toolId:"process.run", toolVersion:1, arguments:{program,argv,cwd?}}`.
  Foreign sessions, blank/invalid identities, unknown fields, mismatched tool or
  version, and invalid argument types fail closed.
- Delegated all proposal, approval, execution, terminal evidence, replay, and
  output authority to `keeper-runtime-host::execute_governed_process`.
- Fresh provider calls emit `requested → decided → started → terminal`; rejected
  calls omit `started`; replay emits no duplicate process lifecycle.
- Provider results contain only `ProcessRunOutput` metadata and an optional
  `replayed: true` marker. Fixed errors never return grants, output bytes,
  native paths, environment values, internal session identifiers, or raw host
  errors.
- Added deterministic fake ACP approve/reject/replay/conflict process scripts.
- Bound every admitted desktop run to governed process authority using the same
  run/project/conversation/root as governed reads. Process registration failure
  releases the combined binding and fails before provider session start with
  `governed_process_unavailable`.

## GREEN evidence

All required commands exited 0:

```text
cargo test -p keeper-adapter-grok-acp --test governed_process_roundtrip -- --nocapture
  3 passed

cargo test -p keeper-adapter-grok-acp --all-targets -- --nocapture
  102 passed; 1 ignored (explicit opt-in live ACP test)

cargo test -p pillow-desktop --all-targets -- --nocapture
  104 passed

cargo check --workspace --all-targets
cargo clippy -p keeper-adapter-grok-acp -p pillow-desktop --all-targets -- -D warnings
cargo fmt --all -- --check
git diff --check -- <Task 5A allowlist>
```

## Safety properties verified

- Normalizer tests cover accepted portable arguments plus wrong session,
  identity, unknown fields, tool/version, and argument types.
- Response tests cover rejected, expired, cancelled, timed-out, failed, busy,
  conflict, and replay mappings without sensitive fields.
- Fake-provider roundtrip tests cover approval ordering and completion after
  terminal, rejection without `started`, exact replay without duplicate events,
  and changed-argument conflict without a second lifecycle.

## Residual risks

- The desktop service has no existing safe fault-injection seam for
  `register_governed_process_run`; its rollback branch is implemented and is
  covered by compilation and the full desktop suite, but not independently
  forced by a production code switch. Adding such a seam would require scope
  outside this task's allowlist.
- Live ACP acceptance remains intentionally opt-in and was not invoked; the
  deterministic fake ACP suite covers this provider process contract.
