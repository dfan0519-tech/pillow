# Permission Mode Execution Report

Date: 2026-08-11  
Worker: codex1  
Acceptance owner: controller  
Completion state: WORKER IMPLEMENTATION COMPLETE; CONTROLLER ACCEPTANCE PENDING; WORKER GATE ENV-BLOCKED

## Scope and outcome

Pillow now has the three requested installation-wide permission modes:

| Mode | Writes without a matching rule | Processes without a matching rule |
| --- | --- | --- |
| `default` | Ask | Ask |
| `accept_edits` | Automatically approve | Ask |
| `bypass` | Automatically approve | Automatically approve |

Permission-rule evaluation remains first. A matching deny rejects in every mode. Mode approval is a Pillow-host decision and retains the durable proposal, decision, execution lifecycle, and a separate mode-attribution receipt. No mode is forwarded to Grok.

## Change inventory

- Storage and settings:
  - Added the typed `PermissionMode` setting and a global-only repository method.
  - Added migration `0014_permission_modes.sql`, including database triggers that reject project-scoped mode rows.
  - Added write/process mode-receipt tables and atomic repository decision methods.
  - Invalid stored kinds, JSON, or enum labels resolve to `default` with source `default`.
- Runtime host:
  - Reads the mode only after rule evaluation returns `RequireConfirmation`.
  - Automatically approves writes in `accept_edits` and `bypass`, and processes only in `bypass`.
  - Uses the existing apply/spawn and lifecycle paths; mode-owned pending decisions never expose a human approval grant/action.
  - Keeps rule and mode attribution distinct.
- Desktop host and renderer:
  - Added global `get_permission_mode` and `set_permission_mode` commands.
  - Added a closed-key `{ mode, source }` gateway contract.
  - Added the three-option settings selector, project-scope lockout, persistent non-default workbench indicator, and one-click reset.
- Tests:
  - Added focused storage, migration, runtime, desktop-contract, and UI coverage for both allow and deny directions.

All Cargo commands used the required isolated target directory. It is represented as `<repo>/target-perm-mode` here so this report contains no machine-specific absolute path. Commands were run directly, without `tail` or another exit-code-masking pipeline.

## Exit-code table

| Command | Exit code | Result | Evidence note |
| --- | ---: | --- | --- |
| `cargo test -p keeper-storage-sqlite --test settings_repository permission_mode -- --nocapture` | 0 | PASS | 4 focused settings tests passed. |
| `cargo test -p keeper-storage-sqlite --test migrations permission_mode_migration_is_idempotent_and_installs_global_scope_guards -- --exact --nocapture` | 0 | PASS | Migration and both global-only triggers verified. |
| `cargo test -p keeper-storage-sqlite` | 0 | PASS | Full storage crate passed, including 19 migration, 27 settings, 29 write-proposal, and 31 exec-proposal tests. |
| `cargo test -p keeper-tool-executor` | 0 | PASS | Full tool-executor crate passed. |
| `cargo test -p keeper-runtime-host` | 0 | PASS | Full runtime-host crate passed after the implementation refactor. |
| `cargo test -p keeper-runtime-host --test governed_write_bridge accept_edits_and_bypass_auto_apply_writes_with_durable_mode_receipts -- --exact --nocapture` | 0 | PASS | Real auto-apply, hidden human grant, and durable mode attribution. |
| `cargo test -p keeper-runtime-host --test governed_process_bridge bypass_auto_executes_process_with_full_lifecycle_and_durable_mode_receipt -- --exact --nocapture` | 0 | PASS | Real process start/success, hidden approval action, complete lifecycle, and durable mode attribution. |
| `cargo test -p keeper-adapter-grok-acp session_profile_enables_no_approval_or_dangerous_capabilities -- --nocapture` | 0 | PASS | Existing forbidden-provider-flag test passed unchanged. |
| `cargo test -p pillow-desktop settings_commands_tests::permission_mode_dto_accepts_exact_keys_and_global_values_round_trip -- --exact --nocapture` | 0 | PASS | Rust DTO accept direction passed. |
| `cargo test -p pillow-desktop settings_commands_tests::permission_mode_command_parser_rejects_unknown_value_without_selecting_bypass -- --exact --nocapture` | 0 | PASS | Rust command reject direction passed. |
| `pnpm --dir apps/desktop typecheck` | 0 | PASS | Renderer typecheck passed. |
| `pnpm --dir apps/desktop exec vitest run tests/settings-panel.test.tsx` | 0 | PASS | 17 focused UI/closed-validator tests passed. |
| `pnpm --dir apps/desktop test` | 0 | PASS | 35 files and 454 tests passed. Expected test-harness stderr did not affect the exit code. |
| `pnpm --dir apps/desktop build` | 0 | PASS | Production renderer build passed. |
| `cargo fmt --all -- --check` | 0 | PASS | Formatting passed. |
| `cargo clippy -p keeper-storage-sqlite -- -D warnings` | 0 | PASS | Strict per-crate clippy passed. |
| `cargo clippy -p keeper-tool-executor -- -D warnings` | 0 | PASS | Strict per-crate clippy passed. |
| `cargo clippy -p keeper-runtime-host -- -D warnings` | 1 then 0 | FIXED/PASS | First run found two 8-argument bind functions. Governance repositories were grouped in a typed internal dependency; rerun passed without a lint allowance. |
| `cargo clippy -p keeper-adapter-grok-acp -- -D warnings` | 0 | PASS | Strict per-crate clippy passed. |
| `cargo clippy -p pillow-desktop -- -D warnings` | 0 | PASS | Strict per-crate clippy passed. |
| `cargo test -p pillow-desktop` | 1 | ENVIRONMENT-LIMITED | 154/155 passed. The existing reparse-point test could not create a directory symlink because Windows returned OS error 1314 (required privilege unavailable). |
| `node scripts/run-worker-gate.mjs` | 1 | **ENV-BLOCKED** | Workspace and secret checks passed; the unchanged Cargo wrapper then failed when the sandbox denied `Get-CimInstance Win32_Process`. No script was modified and no bypass was attempted. |
| `git diff --check` | 0 | PASS | No whitespace errors; only line-ending notices. |

Pre-implementation focused tests failed for the expected missing APIs/UI and were used only as TDD red evidence. Any zero-match filtered invocation was discarded and is not used as acceptance evidence.

## Required invariant evidence

### 1. Mode approval preserves the durable ledger

- Positive: `accept_edits_and_bypass_auto_apply_writes_with_durable_mode_receipts` proves both non-default modes emit one requested proposal, actually write the file, finish `Succeeded`, and persist the exact approving mode. `bypass_auto_executes_process_with_full_lifecycle_and_durable_mode_receipt` proves `requested -> decided -> started`, a real `started` receipt, `Succeeded`, and a durable `bypass` receipt.
- Negative: `default_mode_cannot_create_a_write_approval_or_receipt` leaves the proposal pending with no decision row or mode receipt. `default_and_accept_edits_cannot_create_process_approval_receipts` rejects both modes at the storage boundary for process auto-approval.
- Receipt identity is fixed by database constraints: `decision=approve_by_mode`, `actor=pillow_permission_mode`, and `permission_mode` is the exact approving mode.

### 2. Deny rules always take precedence

- Positive/no-deny direction: the two runtime auto-approval tests above prove that no-match operations genuinely reach write application or process execution; these are not reject-only tests.
- Negative/deny direction: `bypass_write_still_obeys_project_deny_and_never_creates_mode_receipt` rejects without creating the file. `bypass_process_still_obeys_deny_and_never_reaches_started` rejects without a started event. Both retain the deny-rule receipt and create no mode receipt.

### 3. Approval authority remains inside Pillow

- Positive: host tests resolve the SQLite mode, use Pillow's existing governed decision path, and persist the `pillow_permission_mode` actor. During the requested callback, mode-owned decisions are visible for audit but expose no clickable human grant/action.
- Negative: the unchanged `session_profile_enables_no_approval_or_dangerous_capabilities` test serializes provider session parameters and rejects `yolomode`, `automode`, and `always-approve`; it passed. The provider method source has no worktree diff.

### 4. Project directories cannot set permission mode

- Positive: `permission_mode_defaults_and_global_values_roundtrip` persists global `accept_edits` and `bypass`, reopens the database, and resolves source `global`. The Tauri command has no project-shaped argument.
- Negative: `permission_mode_project_write_is_rejected_by_repository` rejects a project-scoped generic upsert. `permission_mode_project_row_is_rejected_by_sqlite_guard` proves a raw SQL insert is also rejected. The renderer disables the selector and shows a global-only notice in project scope.

### 5. Non-default mode remains visible and resets in one action

- Positive: parameterized UI tests independently load `accept_edits` and `bypass`, require the persistent workbench indicator to be visible with the correct label, click `恢复默认`, and verify the durable mock setting becomes `default`.
- Negative/default direction: the same tests require the indicator to disappear after the successful reset. The project-scope test proves a project cannot use the selector.
- Approval-bar behavior: the host withholds `approval_grant`/`decision_available` while a new mode-owned decision is pending, so a transient auto-approval cannot present a clickable approval bar. A proposal that was already awaiting a person before a mode change remains host-authoritative and actionable rather than being hidden or retroactively approved.

## Invalid-enum fail-closed evidence

`invalid_stored_permission_mode_fails_closed_to_default` inserts the unknown stored label `never-ask`, reopens through `SqliteSettingsRepository`, and asserts value `default`, source `default`, and explicitly not `accept_edits` or `bypass`. The renderer's closed validator separately rejects missing keys, extra keys, and the invalid label `always-approve`; the Rust command parser also rejects an unknown label before storage.

## DTO correspondence

The write/process proposal DTOs and their closed renderer gateways were not widened. Mode attribution is held in its own durable storage receipt, so no new authority-bearing key was added to `tauriExecGateway`, `tauriWriteApprovalGateway`, or conversation event payloads.

One new settings DTO crosses the host/renderer boundary, with both sides landed together:

| Rust `PermissionModeSettingsDto` key | Renderer closed allowlist | Accepted values | Verification |
| --- | --- | --- | --- |
| `mode` | exact key `mode` in `parsePermissionModeSettings` | `default`, `accept_edits`, `bypass` | Rust exact-key roundtrip plus TypeScript accept/reject test |
| `source` | exact key `source` in `parsePermissionModeSettings` | `global`, `default` | Rust exact-key roundtrip plus TypeScript accept/reject test |

`set_permission_mode` accepts only `mode`; neither `projectId` nor a scope key exists. The TypeScript test accepts the exact two-key payload and rejects an extra native-path key, a missing key, and an unknown mode, preventing the reject-everything false-green pattern.

## Write-boundary audit

- Product writes are limited to relative paths under:
  - `crates/storage-sqlite/`
  - `crates/tool-executor/`
  - `crates/runtime-host/`
  - `apps/desktop/src-tauri/`
  - `apps/desktop/src/`
- Test writes are limited to the corresponding crate test directories and `apps/desktop/tests/settings-panel.test.tsx`.
- Documentation writes are this report and the implementation plan under `docs/`.
- `scripts/` is unchanged.
- `crates/adapter-grok-acp/src/rpc/methods.rs` is unchanged.
- No credentials, runtime sessions, provider automatic-approval flags, or machine-specific paths were added to product code or this report.
- No Git write operation, staging, commit, reset, or checkout was performed.
- No desktop E2E was run.

## Residual risks and controller-only acceptance

1. The official worker gate is **ENV-BLOCKED** in this sandbox at Windows process enumeration. It must be rerun by the controller in an environment where the unchanged process supervisor can query process identity.
2. The controller should rerun the full `pillow-desktop` Rust suite with symlink privilege; the worker result is 154 passed and one environment-limited existing reparse-point test.
3. Desktop E2E, supported-size Windows screenshots, and the live provider probe remain controller-only. The live acceptance predicate remains conjunctive: the command reaches the client and the proxy does not execute it itself.
4. Permission mode is evaluated for each newly reserved proposal after rules. Existing human-pending proposals are intentionally not retroactively approved when the global setting changes.

PERMISSION-MODE-COMPLETE
