# Runtime Slots Foundation Implementation Plan

> **For agentic workers:** execute inline, task by task, with a red-green test cycle. Git writes are forbidden for this work order, so every commit step from the general workflow is replaced by a read-only diff review.

**Goal:** Add project-scoped runtime slots, DPAPI-protected API-key storage, private Grok model configuration, governed per-slot activation, and ungoverned external-CLI status inspection without renderer changes.

**Architecture:** SQLite owns non-secret slot metadata and opaque secret references. A replaceable host-side runtime-slot crate owns Windows DPAPI file storage, private-home configuration, and bounded structured-argv status execution. The desktop command layer exposes closed, secret-free response DTOs and activates a governed Grok conversation service through a replaceable runtime state.

**Tech Stack:** Rust 2024, rusqlite, serde, Tauri commands, Windows DPAPI through the already locked `windows-sys`, existing supervised process execution, and the existing Grok ACP adapter.

## Global Constraints

- Set `CARGO_TARGET_DIR=<repo>/target-slots` for every Cargo command.
- Do not modify `apps/desktop/src/**`, `scripts/**`, or any ACP governance profile/capability/MCP boundary.
- Do not run desktop E2E or any git write operation.
- Never serialize, log, emit, receipt, or report a secret value.
- Login slots use the current default profile without redirecting it; API-key slots use a project/slot-private home with no `auth.json`.
- `external_cli` is explicitly ungoverned and cannot be launched as an agent in this slice.

---

### Task 1: Project-scoped slot metadata

**Files:**

- Create: `crates/storage-sqlite/migrations/0015_runtime_slots.sql`
- Create: `crates/storage-sqlite/src/runtime_slot.rs`
- Create: `crates/storage-sqlite/tests/runtime_slot_repository.rs`
- Modify: `crates/storage-sqlite/src/migrations.rs`
- Modify: `crates/storage-sqlite/src/lib.rs`
- Modify: `crates/storage-sqlite/tests/migrations.rs`

**Interfaces:**

- Produces `RuntimeSlot`, `RuntimeSlotKind`, `RuntimeSlotGovernance`, `GrokSlotAuth`, `NewRuntimeSlot`, `SqliteRuntimeSlotRepository::{open,create,get,list}`.
- The schema stores only non-secret configuration and an opaque `secret_ref`; it enforces project foreign keys and valid kind/governance/auth shapes with SQL constraints.

- [ ] Write repository and migration tests for both valid variants, explicit governance, project isolation, restart persistence, cascaded project deletion, and invalid SQL shapes.
- [ ] Run `cargo test -p keeper-storage-sqlite --test runtime_slot_repository --locked --offline` and confirm failure because the schema/API is absent.
- [ ] Add the minimal migration and repository implementation.
- [ ] Re-run the focused repository and migration tests and confirm success.
- [ ] Review the diff without staging or committing.

### Task 2: Secure slot host service

**Files:**

- Create: `crates/runtime-slots/Cargo.toml`
- Create: `crates/runtime-slots/src/lib.rs`
- Create: `crates/runtime-slots/src/secret.rs`
- Create: `crates/runtime-slots/src/grok_config.rs`
- Create: `crates/runtime-slots/src/status.rs`
- Create: `crates/runtime-slots/tests/slot_service.rs`
- Modify: `Cargo.toml`

**Interfaces:**

- Produces `SecretBytes` (zero-on-drop, no `Debug`/`Display`/`Serialize`), `DpapiFileSecretStore`, `RuntimeSlotService`, `PreparedGrokSlot`, and `ExternalCliStatus`.
- `RuntimeSlotService::create_grok_api_key` protects before persistence and returns only non-secret slot metadata.
- `RuntimeSlotService::prepare_grok` decrypts in host memory, writes an `env_key`-based private config through a synced temporary file, rejects any private-home `auth.json`, and returns the model selector plus secret for adapter-private injection.
- `RuntimeSlotService::external_status` executes the registered binary and status argv directly with bounded output and no shell.

- [ ] Write Windows DPAPI round-trip/ciphertext tests, secret-free config tests, positive configured-slot preparation tests, login-home tests, and external-status tests.
- [ ] Run `cargo test -p pillow-runtime-slots --locked --offline` and confirm failure because the crate/service is absent.
- [ ] Add the minimal crate and implementation using locked `windows-sys`; use structured TOML generation and a synced temporary-file replacement path.
- [ ] Re-run the crate tests and confirm success.
- [ ] Review the diff without staging or committing.

### Task 3: Typed Grok model and secret launch path

**Files:**

- Modify: `crates/adapter-grok-acp/src/config.rs`
- Modify: `crates/adapter-grok-acp/src/process/env.rs`
- Modify: `crates/adapter-grok-acp/src/state.rs`
- Modify: `crates/adapter-grok-acp/tests/adapter_fake_child.rs`
- Modify: `crates/process-executor/src/lib.rs`
- Modify: `crates/process-executor/tests/supervised_process.rs`

**Interfaces:**

- `GrokAcpConfig::with_model` adds only `-m <model_id>` to the fixed production argv.
- `GrokAcpConfig::with_api_key` owns a redacted secret and injects it only under the fixed environment variable named by generated config.
- `extra_env` remains test-only and never accepts secrets; the dedicated host-only field is the sole exception to generic sensitive-name rejection.
- Login profile mode copies the current profile variables through the clean allowlist; isolated mode redirects only API-key slots.

- [ ] Add failing tests for model argv, redacted debug output, dedicated secret injection, generic secret rejection, and inherited versus isolated profile behavior.
- [ ] Run `cargo test -p keeper-adapter-grok-acp --lib --locked --offline` and confirm the new assertions fail.
- [ ] Implement the minimal typed fields and environment handling.
- [ ] Re-run adapter unit and protocol fixture tests and confirm success.
- [ ] Review `plaintext_agent_profile`, `disallowedTools`, `mcpServers`, and `clientCapabilities` for a zero diff.

### Task 4: Host commands and active slot runtime

**Files:**

- Create: `apps/desktop/src-tauri/src/runtime_slot_commands.rs`
- Create: `apps/desktop/src-tauri/src/runtime_slot_commands_tests.rs`
- Create: `apps/desktop/src-tauri/src/runtime_service_state.rs`
- Modify: `apps/desktop/src-tauri/Cargo.toml`
- Modify: `apps/desktop/src-tauri/src/lib.rs`
- Modify: `apps/desktop/src-tauri/src/runtime_composition.rs`
- Modify: `apps/desktop/src-tauri/src/conversation_commands.rs`
- Modify: `apps/desktop/src-tauri/src/exec_commands.rs`
- Modify: `apps/desktop/src-tauri/src/write_commands.rs`

**Interfaces:**

- Commands: `list_runtime_slots`, `create_grok_login_slot`, `create_grok_api_key_slot`, `create_external_cli_slot`, `get_external_cli_status`, `start_runtime_slot`.
- Response DTOs expose `slotId`, `projectId`, `displayName`, `kind`, `governance`, non-secret config, `secretRef`, `secretConfigured`, timestamps, bounded raw status output, and activation status. They never expose a secret value.
- `RuntimeServiceState` leaves the old service intact when replacement construction fails; after shutdown begins, any unproven shutdown/recovery leaves no active runtime and fails closed.
- `external_cli` start requests fail closed with a stable unsupported code.

- [ ] Add failing command tests for exact DTO key sets, serialized-secret absence, kind/governance pairing, positive API-slot preparation/activation, login non-redirection, external raw status, and external launch rejection.
- [ ] Run `cargo test -p pillow-desktop runtime_slot --locked --offline` and confirm expected failures.
- [ ] Implement commands, composition, and active service state without renderer edits.
- [ ] Re-run focused desktop unit tests and existing host command suites.
- [ ] Review all changed DTOs against closed renderer allowlist requirements and record the next-slice key list.

### Task 5: Verification and report closeout

**Files:**

- Modify: `docs/execution/2026-08-11-slots-foundation-report.md`

- [ ] Run formatting checks without changing scripts.
- [ ] Run each affected crate test suite serially.
- [ ] Run `cargo clippy -p <affected-crate> -- -D warnings` separately for every affected crate; record every exit code.
- [ ] Run allowed root gates serially. If a gate cannot start in the sandbox, record `ENV-BLOCKED` and do not bypass it.
- [ ] Confirm no files under `apps/desktop/src/**` or `scripts/**` changed and the four ACP boundary symbols have zero diff.
- [ ] Complete the report with storage risk, dependency rationale, DTO keys, positive/negative test evidence, write-boundary audit, and residual risks.
- [ ] Review the final working-tree diff without staging or committing.
