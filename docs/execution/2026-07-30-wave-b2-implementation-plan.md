# Wave B2 Implementation Plan

Date: 2026-07-30
Branch: `integration/slice-b2`
Controller: Codex Desktop

## Objective

Replace the desktop's fake-only runtime path with the first real, bounded Grok
ACP adapter while preserving the accepted B1 ordering:

`validate -> persist -> commit -> emit`

The implementation must use the live-observed Grok Build 0.2.112 protocol. It
must not copy the legacy Keeper runtime wholesale.

## Protocol Decisions

- Launch `grok agent --no-leader stdio`.
- Use newline-delimited JSON-RPC 2.0 over stdio.
- Support `initialize`, `session/new`, `session/load`, `session/prompt`,
  `session/update`, `session/request_permission`, and `session/cancel`.
- Do not send `$/cancel_request` or `authenticated`; this Grok build rejects
  both.
- Treat `stopReason: end_turn` as completed.
- Treat `stopReason: cancelled` as cancelled, including
  `PermissionRejected`.
- Ignore unknown and `_x.ai/*` notifications by default.
- Allow read/search permission only for a recognized same-session call and
  only with `allow_once`; deny write, execute, network, credential, and unknown
  classes.
- Keep ACP session ids, request ids, process ids, raw envelopes, account paths,
  and credentials private to the adapter.

## Work Packages

### B2-0: B1 truth cleanup

Owner: Grok worker A

Editable scope:

- `README.md`
- `apps/desktop/src-tauri/src/conversation_service/**`
- `crates/runtime-host/src/host/config.rs`
- tests required for the same change

Deliver:

- Move pump registration hooks behind `cfg(test)` or an explicit test-support
  feature so they are not a normal release API.
- Apply the same isolation to host test hooks where practical without reducing
  race-test coverage.
- Update README to describe accepted B1 capability and honest B2 status.
- Run focused Rust tests plus clippy for touched crates.

### B2-1: Grok ACP adapter core

Owner: Grok worker B

Editable scope:

- root `Cargo.toml` only to register the new workspace crate
- root `Cargo.lock` if generated
- `crates/adapter-grok-acp/**`
- `tests/fixtures/protocol/grok-acp/**`

Deliver:

- New `keeper-adapter-grok-acp` crate implementing
  `AgentRuntimeAdapter`.
- Bounded NDJSON framing and JSON-RPC request correlation.
- Process launch with environment allow-list, isolated home override, timeout,
  bounded line/response sizes, and process-tree cleanup.
- Private runtime-session to ACP-session mapping.
- Normalized text, reasoning, tool, terminal, cancellation, and permission
  behavior.
- A deterministic fake ACP child plus unit/integration tests.
- No live account requirement in normal tests.

### B2-2: Runtime composition boundary

Owner: Grok worker C

Editable scope:

- `apps/desktop/src-tauri/src/conversation_service/**`
- `apps/desktop/src-tauri/src/lib.rs`
- desktop Rust tests for composition

Deliver:

- Add a production-safe `ConversationService` constructor that accepts
  `Arc<dyn AgentRuntimeAdapter>` and builds `RuntimeHost::new`.
- Keep deterministic fake construction available only for tests or explicit
  development use.
- Do not import Grok-specific wire types into service/domain code.
- Add tests proving injected adapter probe/session/turn/cancel flow.

## Integration Order

1. Review and merge B2-0.
2. Review and merge B2-1.
3. Rebase or adapt B2-2 after B2-0 if both touched service composition files.
4. Add the desktop Grok composition point and configuration in a dedicated
   integration patch.
5. Run fake-child gates before any live account test.
6. Run one controlled, read-only live Grok session, cancellation, restart, and
   residual-process check.

## Required Gates

- `cargo fmt --all -- --check`
- `cargo test --workspace`
- `cargo clippy --workspace --all-targets -- -D warnings`
- frontend typecheck, lint, tests, and build
- protocol fixture replay
- fake-child cancellation and process-tree cleanup
- no forbidden provider fields in normalized events
- no secrets in tracked files, logs, SQLite, or renderer payloads
- live gate identifies `grok-acp`, never `fake`
- zero residual Keeper IDE and Grok child processes after live acceptance

## HOLD Conditions

- unrestricted parent environment reaches the child;
- write/execute permissions can be auto-approved;
- cancellation can report success while a child remains;
- malformed or oversized output can hang the adapter;
- desktop still defaults to Fake while claiming B2 complete;
- live acceptance is skipped but reported as passed;
- provider-specific state leaks into domain or renderer contracts.
