# Uniform Slot Model — Execution Report

**Status:** complete  
**Complete signal:** `UNIFORM-SLOT-COMPLETE`  
**Date:** 2026-08-12

## 0. Goal

One slot record, one create command surface, auth as a field, role as a field.
Governance is derived from auth (never independently stored).

## 1. Design decisions

### 1.1 Second Controller

**Choice: reject** the second `Controller` in a project with a clear error
(`runtime_slot_controller_already_exists` / store `controller_conflict`).

Rationale: silent demotion of the prior Controller can change who is allowed to
drive conversation without an explicit operator action. Fail-closed is safer for
a governance-sensitive role. Enforced by a partial unique index
`idx_runtime_slots_one_controller` plus typed create/update validation.

### 1.2 Governance derivation

| Auth | Governance |
| --- | --- |
| `ApiKey` | `Full` |
| `GrokLogin` | `Full` |
| `LocalCli` | `None` |

Contradiction between kind and governance is unrepresentable: the `governance`
column was removed in migration 0017.

### 1.3 Controller must be governed

`LocalCli` + `Controller` is rejected at create/update validation
(`runtime_slot_controller_requires_governed_auth`).
Positive path: `ApiKey` / `GrokLogin` may be `Controller`.

### 1.4 Migration defaults

All existing rows → `role = Custom("未分配")`.
`secret_ref` is copied byte-for-byte (no re-encrypt).

### 1.5 IPC surface

Domain service exposes unified `create_slot` / `update_slot` / `delete`.
Tauri keeps thin wrappers for the three legacy create commands (host registration
in `lib.rs` is outside this ticket’s write boundary) and adds `create_slot` /
`update_slot` for the controller to register.

### 1.6 Wire DTO

DTO adds `role` + `roleCustomName`. `kind` / `governance` / `authMode` remain
as **derived** closed labels for existing UI (not independent storage).

Renderer closed-key allowlist in `runtimeSlotsGateway.ts` was widened in the
same change. Parse accepts the new shape and rejects missing/bad role enums and
ungoverned Controllers.

### 1.7 Extensibility

`SlotAuth` is an open enum shape (documented as extensible; new variants may be
added). This ticket only flattens the three existing auths — no OAuth work.

## 2. Exit codes

| Check | EXIT | Notes |
| --- | --- | --- |
| `cargo test -p keeper-storage-sqlite -p pillow-runtime-slots --no-run` | **0** | Compiles `#[cfg(test)]` paths |
| `cargo test -p keeper-storage-sqlite -p pillow-runtime-slots` | **0** | All suites green |
| `corepack pnpm typecheck` | **2** | Out-of-boundary desktop test fixtures lack `role` / `roleCustomName` (see residual risks) |

Commands used the PowerShell form `cmd > log 2>&1; echo EXIT=$LASTEXITCODE`
(not piped through `tail` for the exit code).

## 3. Migration round-trip evidence

Test: `legacy_slot_rows_migrate_round_trip_preserving_secret_ref`

- Writes pre-0016 rows: `GovernedGrok{Login}`, `GovernedGrok{ApiKey}`, `ExternalCli`
- Opens repository (applies 0016 + 0017)
- Asserts:
  - Login → `auth = GrokLogin{None}`, `role = Custom("未分配")`, governance Full
  - ApiKey → `auth = ApiKey{…}`, **same `secret_ref` string**, role 未分配
  - External → `auth = LocalCli{…}`, governance None, role 未分配
- Re-open: `secret_ref` still equal (no re-encrypt)

```
test legacy_slot_rows_migrate_round_trip_preserving_secret_ref ... ok
test result: ok. 1 passed; 0 failed; … 15 filtered out
```

## 4. Governance derivation test evidence

Tests:

- unit: `runtime_slot::governance_derivation_tests::{api_key,grok_login,local_cli}_maps_*`
- integration: `governance_derivation_maps_all_three_auth_variants`

```
test governance_derivation_maps_all_three_auth_variants ... ok
test runtime_slot::governance_derivation_tests::api_key_maps_to_full ... ok
test runtime_slot::governance_derivation_tests::grok_login_maps_to_full ... ok
test runtime_slot::governance_derivation_tests::local_cli_maps_to_none ... ok
```

## 5. Controller-must-be-governed test evidence

Positive + negative (storage):

```
test controller_requires_governed_auth_positive_and_negative ... ok
```

- ApiKey + Controller → accepted
- LocalCli + Controller → `runtime_slot_controller_requires_governed_auth`
- GrokLogin + Controller → accepted after first controller deleted

Service layer:

```
test create_slot_rejects_local_cli_as_controller ... ok
test create_slot_accepts_governed_auth_as_controller ... ok
```

Also: `second_controller_is_rejected` pins unique-Controller policy.

## 6. Write-boundary audit

Files actually modified (within allowed paths only):

| Path | Change |
| --- | --- |
| `crates/storage-sqlite/migrations/0017_uniform_slot_model.sql` | **new** schema rebuild |
| `crates/storage-sqlite/src/runtime_slot.rs` | SlotAuth / SlotRole / derived governance |
| `crates/storage-sqlite/src/migrations.rs` | wire 0017 + safe probe order |
| `crates/storage-sqlite/src/lib.rs` | re-exports |
| `crates/storage-sqlite/tests/runtime_slot_repository.rs` | migration + controller + governance tests |
| `crates/runtime-slots/src/lib.rs` | `create_slot` / `update_slot` |
| `crates/runtime-slots/tests/slot_service.rs` | auth-based assertions + controller tests |
| `apps/desktop/src-tauri/src/runtime_slot_commands.rs` | DTO role fields + create_slot/update_slot |
| `apps/desktop/src/features/settings/runtimeSlotsTypes.ts` | role types + CreateSlotInput |
| `apps/desktop/src/features/settings/runtimeSlotsGateway.ts` | allowlist + parse + mock |
| `apps/desktop/src/features/settings/RuntimeSlotsSection.tsx` | role badge |
| `docs/execution/2026-08-12-uniform-slot-model-report.md` | this report |

**Not touched:** `adapter-grok-acp`, `kernel-policy`, `workbench`, `scripts`,
`lib.rs` (command registration), git.

## 7. Residual risks

1. **Host command registration** — `create_slot` / `update_slot` are implemented
   in `runtime_slot_commands.rs` but **not registered** in
   `apps/desktop/src-tauri/src/lib.rs` (outside write boundary). Legacy
   create/update commands remain registered and route through the unified
   domain path. Controller must register the new commands when ready.

2. **Desktop typecheck / fixture tests** — `apps/desktop/tests/runtime-slots*.ts(x)`
   and related fixtures construct `RuntimeSlotView` without `role` /
   `roleCustomName`. Those files are outside this ticket’s write boundary.
   `corepack pnpm typecheck` exits **2** until fixtures add:
   `role: "custom", roleCustomName: "未分配"` (or equivalent).
   Gateway parse tests that assert exact key sets must also admit the two
   new keys and reject missing/bad role.

3. **Tauri command unit tests** —
   `apps/desktop/src-tauri/src/runtime_slot_commands_tests.rs` is outside
   the write boundary; it may need updates for role fields / new commands.

4. **OAuth / subscription login** — intentionally untouched; shape reserved
   for the parallel research brief.

5. **LocalCli still ungoverned** — property preserved (derived `None`); still
   cannot `prepare_grok` / start as conversation runtime. Now expressed as
   auth property rather than a second slot kind.

---

## UNIFORM-SLOT-COMPLETE
