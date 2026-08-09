# Settings self-brick fix + Fake ADR gate — execution report

**Date:** 2026-08-08  
**Status:** `SELFBRICK-FIX-COMPLETE`  
**Worker:** grok-worker (Keeper boot required `local parent session id`; work continued under total-control brief)

## Problem (do not re-argue)

Saving a bad Grok executable/profile path only checked non-empty + length. After restart, `require_executable_file` / profile checks failed inside `open_conversation_service` during Tauri `setup`, aborting the whole app. The only UI that could fix the path lived inside that dead app. Release builds are windowed (no console) → near-silent brick.

Separately, Fake was always listed in the adapter dropdown, contrary to ADR-0003 Q2 (dev builds only).

## Layer 1 — Save-time validation (hard reject)

**Where:** `crates/storage-sqlite/src/settings.rs` → `validate_path_setting_value`

| Key | Rule |
| --- | --- |
| `runtime.grok.executable` | Must be an **existing file** |
| `runtime.grok.profile` | Must be an **existing directory** |

- Empty / length > 4096 still use `SettingsError::InvalidPathValue` → code `settings_invalid_path`.
- Existence failures use `SettingsError::Coded { code: "settings_invalid_path", message: ... }` with **actionable** text (expected vs current value; what to do next).
- **Hard reject only** — no soft warning path. Cannot “save path then install Grok”; acceptable vs product lockout.

## Layer 2 — Startup degradation (real fix)

**Where:** `runtime_composition.rs` + `lib.rs` (minimal)

### Composition API

- `open_conversation_service_or_degrade` → `RuntimeOpenOutcome::{Ready, Degraded}`
- Degradable codes **only**:
  - `runtime_executable_missing`
  - `runtime_profile_invalid`
  - `runtime_profile_missing`
  - `runtime_fake_not_allowed` (release + stored Fake; settings-fixable)
- Everything else (e.g. `settings_sqlite`) stays **fatal `Err`**.

### `lib.rs` setup

1. `AppState::open` / settings store open — still fatal (SQLite not user-fixable in Settings).
2. `open_conversation_service_or_degrade`:
   - **Ready** → manage `Arc<ConversationService>`, recover in-flight runs.
   - **Degraded** → log; **do not** manage ConversationService; **do not** open Fake.
3. Always manage `Arc<RuntimeStartupState>` for `get_runtime_startup`.
4. Settings + shell still managed.

### UI

- Banner `data-testid="runtime-degraded-banner"` with guidance pointing at Settings fields + “打开设置”.
- Conversation remains unavailable until user fixes settings and restarts (existing “restart to apply” semantics).

### How we prove degrade ≠ Fake

| Check | Proof |
| --- | --- |
| Outcome type | `Degraded` has **no** `selection`; Ready with Fake only from explicit Fake open |
| Message | Asserts no “using fake” / “falling back” |
| Service | ConversationService **not** managed in degraded branch |
| Unit | `bad_stored_executable_degrades_not_fake_and_not_fatal`, `bad_stored_profile_degrades_not_fake`, `degrade_outcome_is_never_fake_selection`, frontend banner text rejects Fake fallback copy |

## Fake ADR-0003 Q2 gate

| Surface | Behavior |
| --- | --- |
| Host flag | `fake_adapter_selectable()` = `cfg!(debug_assertions)` |
| DTO | `RuntimeSettingsDto.fake_adapter_selectable` → renderer filters options |
| UI | `adapterOptionsForBuild(flag)` — **no** frontend env hard-code |
| Write | `set_runtime_adapter` rejects Fake when not selectable (`runtime_fake_not_allowed`) |
| Stored Fake in release | Open fails closed with `runtime_fake_not_allowed` → **degrade** path; **user data not rewritten** |
| Env Fake | `SettingSource::Env` always allowed → `PILLOW_RUNTIME=fake` still opens Ready/Fake |

### Confidence `PILLOW_RUNTIME=fake` life-line is intact

1. Existing tests still pass: `env_adapter_wins_over_stored_value`, `open_service_respects_env_fake_over_stored_grok`, `process_env_map_still_wins_when_live`, `env_map_injection_documents_e2e_lifeline`.
2. New test `env_fake_still_opens_ready_not_degraded` asserts Ready + `probe.adapter_name == "fake"`.
3. Gate only blocks **stored** Fake when `!debug_assertions`; env source bypasses the block in `open_conversation_service_with_resolved`.
4. Resolve precedence unchanged: env → project → global → default.

## Seven required tests — coverage map

| # | Requirement | Coverage |
| --- | --- | --- |
| 1 | Bad exe → app still starts, settings reachable | `bad_stored_executable_degrades_not_fake_and_not_fatal` + App banner test (open settings from degrade) |
| 2 | Bad profile → same | `bad_stored_profile_degrades_not_fake` |
| 3 | Save rejects missing file/dir, actionable error | storage unit + `upsert_rejects_missing_*` integration tests |
| 4 | Degrade is not Fake | explicit asserts in composition + `RuntimeStartupState` + banner copy |
| 5 | `PILLOW_RUNTIME=fake` still wins | `env_fake_still_opens_ready_not_degraded` + prior env-wins tests |
| 6 | Prod: Fake not in options; dev: is | `adapterOptionsForBuild` + vitest host-flag tests; host `fake_adapter_selectable_matches_debug_assertions` |
| 7 | SQLite failure still fatal | `sqlite_open_failure_remains_fatal_not_degraded` |

## Commands & exit codes

| Command | Exit |
| --- | --- |
| `node scripts/run-worker-gate.mjs` | **0** (`PILLOW_WORKER_GATE: OK`) |
| `corepack pnpm --filter @pillow/desktop build` | **0** |

(No desktop E2E run — total-control exclusive.)

## Touched files (whitelist only)

- `crates/storage-sqlite/src/settings.rs`
- `crates/storage-sqlite/tests/settings_repository.rs`
- `apps/desktop/src-tauri/src/runtime_composition.rs`
- `apps/desktop/src-tauri/src/settings_commands.rs`
- `apps/desktop/src-tauri/src/settings_commands_tests.rs`
- `apps/desktop/src-tauri/src/lib.rs` (degrade startup only)
- `apps/desktop/src/features/settings/**`
- `apps/desktop/src/App.tsx` (degrade banner only)
- `apps/desktop/src/styles/settings.css`
- `apps/desktop/tests/settings-panel.test.tsx`
- `docs/execution/2026-08-08-settings-selfbrick-fix-report.md`

## Explicitly not touched

- `scripts/**`
- `crates/runtime-host/**`, `crates/git-executor/**`, `crates/adapter-grok-acp/**`
- `apps/desktop/src/features/` outside `settings/`
- `brand-mark.css`, any `Cargo.toml`
- No git add/commit/stash/reset/clean/checkout/restore
- No desktop E2E
- No auth/token files read
- Theme persistence (`ui.theme`, `--text-tertiary`, `data-theme-ready`) left intact

## Residual risks

1. **Pre-existing bad paths** already in SQLite still load; users get degrade + Settings (by design). New saves cannot re-introduce them.
2. **Release Fake gate** is only fully exercised via `cfg!(debug_assertions)` dual-path logic + unit tests of the flag; a true `--release` open of stored Fake is not run in this worker gate (release binary not built here). Logic is compile-time `cfg!` + source-based env bypass.
3. **Degraded conversation commands** fail because `ConversationService` is not managed (Tauri state missing). Banner + Settings CTA is the recovery UX; no stub Fake adapter.
4. **Stored Fake on release** still appears as a temporary select option when the current value is `fake` so the control is not blank; user can switch to Grok ACP. Value is not auto-rewritten.
5. Local session bridge boot (optional) failed without `local parent session id` (controller-owned claim path).

## Completion marker

**SELFBRICK-FIX-COMPLETE**
