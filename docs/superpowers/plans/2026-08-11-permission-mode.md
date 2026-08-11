# Permission Mode Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking. This task is explicitly assigned to one inline Worker; do not dispatch subagents.

**Goal:** Add global `default`, `accept_edits`, and `bypass` permission modes while preserving rule precedence, host-owned approval authority, and durable decision receipts.

**Architecture:** `SqliteSettingsRepository` owns the global mode and fails closed to `default` for a corrupt stored enum. Runtime-host evaluates existing rules first and consults the mode only for `RequireConfirmation`; mode approvals use dedicated SQLite receipt rows rather than rule attribution or provider flags. A separate closed settings DTO feeds the settings selector and App-owned persistent workbench indicator without widening process/write DTOs.

**Tech Stack:** Rust 2024, rusqlite, Tauri 2, React 19, TypeScript, Vitest.

## Global Constraints

- Use the dedicated Cargo target directory supplied for this task for every Cargo command.
- Do not run desktop E2E, modify `scripts/`, or perform git writes.
- Existing deny matches reject in every mode.
- Existing Grok session-parameter prohibition for `yolomode`, `automode`, and `always-approve` remains unchanged and green.
- Every automatic approval keeps the proposal, decision, execution/apply, terminal receipt, and an additional durable mode-attribution receipt.
- Permission mode is global-only and has no project or environment override.
- No process/write host DTO key is added.

---

### Task 1: Global setting and fail-closed schema

**Files:**

- Create: `crates/storage-sqlite/migrations/0014_permission_modes.sql`
- Modify: `crates/storage-sqlite/src/migrations.rs`
- Modify: `crates/storage-sqlite/src/settings.rs`
- Modify: `crates/storage-sqlite/src/lib.rs`
- Test: `crates/storage-sqlite/tests/settings_repository.rs`
- Test: `crates/storage-sqlite/tests/migrations.rs`

**Interfaces:**

- Produces `PermissionMode::{Default, AcceptEdits, Bypass}`, `KEY_PERMISSION_MODE`, `upsert_permission_mode`, and `resolve_permission_mode`.
- `resolve_permission_mode` returns `ResolvedSetting<PermissionMode>` and maps malformed stored values to `{ value: Default, source: Default }`.

- [x] Add tests proving absent/default/global values resolve correctly, project writes are rejected, raw project inserts are blocked, and a raw invalid enum resolves to `default` rather than either automatic mode.
- [x] Run the focused settings and migration tests and record the expected RED caused by missing mode APIs/schema.
- [x] Implement the typed setting, global-only validation, and migration trigger protecting the global-only key.
- [x] Re-run the focused tests to GREEN.

### Task 2: Durable automatic-approval attribution

**Files:**

- Modify: `crates/storage-sqlite/migrations/0014_permission_modes.sql`
- Modify: `crates/storage-sqlite/src/exec_proposal.rs`
- Modify: `crates/storage-sqlite/src/write_proposal.rs`
- Modify: `crates/storage-sqlite/src/lib.rs`
- Test: `crates/storage-sqlite/tests/exec_proposal_repository.rs`
- Test: `crates/storage-sqlite/tests/write_proposal_repository.rs`

**Interfaces:**

- Produces `StoredPermissionModeReceipt { proposal_id, permission_mode, decided_at }`.
- Produces `decide_by_permission_mode(..., permission_mode, ...)` and `get_permission_mode_receipt(proposal_id)` on both proposal repositories.
- Mode decision APIs accept only an approving non-default mode valid for that operation; they never accept a provider-supplied grant or rule snapshot.

- [x] Add storage tests proving `accept_edits` write and `bypass` write/process decisions are accepted with queryable mode receipts, while default/invalid operation-mode combinations reject without a decision.
- [x] Run the focused proposal repository tests and record RED.
- [x] Add one-to-one mode receipt tables and transactional decision-source handling with idempotency/conflict checks.
- [x] Re-run focused storage tests to GREEN.

### Task 3: Rule-first host evaluation

**Files:**

- Modify: `crates/runtime-host/src/governed_tool_service.rs`
- Modify: `crates/runtime-host/src/governed_write.rs`
- Modify: `crates/runtime-host/src/host/process.rs`
- Test: `crates/runtime-host/tests/governed_write_bridge.rs`
- Test: `crates/runtime-host/tests/governed_process_bridge.rs`

**Interfaces:**

- `GovernedToolService` shares the SQLite settings repository with bound write/process runs.
- On `RequireConfirmation`, writes auto-approve under `accept_edits` or `bypass`; processes auto-approve only under `bypass`; `default` remains pending.
- Rule `Allow` and `Deny` paths remain unchanged and run before mode evaluation.

- [x] Add positive host tests proving a no-rule write really applies under `accept_edits`, and a no-rule process really starts and succeeds under `bypass`, with durable mode receipts.
- [x] Add negative host tests proving `default` still asks, `accept_edits` still asks for process, and matching deny prevents write/process execution under `bypass` with no mode receipt.
- [x] Run focused runtime-host tests and require true positive execution/apply behavior (not rejection-only).
- [x] Implement mode resolution only after the existing rule evaluation returns `RequireConfirmation`, then route through storage mode-decision APIs and the unchanged apply/execute path.
- [x] Re-run focused runtime-host tests to GREEN.

### Task 4: Closed settings DTO and persistent UI safety indicator

**Files:**

- Modify: `apps/desktop/src-tauri/src/settings_commands.rs`
- Modify: `apps/desktop/src-tauri/src/settings_commands_tests.rs`
- Modify: `apps/desktop/src-tauri/src/lib.rs`
- Modify: `apps/desktop/src/features/settings/settingsTypes.ts`
- Modify: `apps/desktop/src/features/settings/settingsGateway.ts`
- Modify: `apps/desktop/src/features/settings/SettingsPanel.tsx`
- Modify: `apps/desktop/src/App.tsx`
- Modify: `apps/desktop/src/features/workbench/Workbench.tsx`
- Modify: `apps/desktop/src/styles/settings.css`
- Modify: `apps/desktop/src/styles/workbench-conversation.css`
- Test: `apps/desktop/tests/settings-panel.test.tsx`

**Interfaces:**

- Host commands: `get_permission_mode` and `set_permission_mode` return `{ mode, source }`.
- Renderer gateway accepts exactly `mode` and `source`; unknown, missing, or invalid enum fields reject.
- `App` owns the effective `PermissionModeLabel`, passes it to Settings and Workbench, and resets through `setPermissionMode("default")`.

- [x] Add renderer and Tauri tests proving the valid DTO is accepted and rendered, unknown keys are rejected, the selector is disabled in project scope, non-default mode remains visible, and one click returns it to default.
- [x] Run focused Rust and Vitest UI tests and record RED.
- [x] Implement the commands, strict parser, mock gateway, selector, persistent indicator, and one-click reset.
- [x] Re-run focused Rust and UI tests to GREEN; run desktop typecheck.

### Task 5: Provider boundary, crate lint gates, and report

**Files:**

- Verify unchanged: `crates/adapter-grok-acp/src/rpc/methods.rs`
- Complete: `docs/execution/2026-08-11-permission-mode-report.md`

**Interfaces:**

- The existing provider-session test continues to reject `yolomode`, `automode`, and `always-approve` and no permission mode is passed to Grok.

- [ ] Run the named adapter prohibition test directly.
- [ ] Run focused storage, runtime-host, desktop Rust, desktop Vitest, and TypeScript checks without output-truncating pipelines.
- [ ] Run `cargo clippy -p keeper-storage-sqlite -- -D warnings`, `cargo clippy -p keeper-runtime-host -- -D warnings`, `cargo clippy -p keeper-adapter-grok-acp -- -D warnings`, and `cargo clippy -p pillow-desktop -- -D warnings` one crate at a time.
- [ ] Run the allowed root Worker gate. If the sandbox prevents it from starting, record `ENV-BLOCKED` and do not change scripts or bypass the gate.
- [ ] Audit the working diff, confirm no process/write DTO widened, fill every report evidence section and exit code, and record controller-only desktop E2E as not run by design.
