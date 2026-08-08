# Settings S0b Report — UI + composition wiring + parser convergence

Date: 2026-08-08  
Worker: Grok (S0b)  
Controller: Claude  
Work directory: `<repo>`

## Signal

**SETTINGS-S0B-COMPLETE**

## 1. One-line outcome

S0a SQLite settings are product-wired: desktop composition reads
`resolve_runtime` (env still wins), the runtime adapter parser is a single
source of truth, Tauri commands + settings UI ship with env-override
visibility, and the worker gate + desktop build are green.

## 2. Parser convergence (§3.1)

### How it was converged

| Before | After |
| --- | --- |
| `RuntimeAdapterSetting::parse` in `storage-sqlite` | **Sole authority** for accepted labels / empty-string / fail-closed |
| `RuntimeSelection::parse` in `runtime_composition.rs` (verbatim match) | **Delegates** to `RuntimeAdapterSetting::parse`, then maps enum variants |

Desktop mapping:

- `RuntimeAdapterSetting::GrokAcp` → `RuntimeSelection::GrokAcp`
- `RuntimeAdapterSetting::Fake` → `RuntimeSelection::Fake`
- `SettingsError::InvalidRuntimeSelection` → `ConversationCommandError` with code **`invalid_runtime_selection`** (same message shape as before)

Accepted labels unchanged: `"" | "grok" | "grok-acp" | "acp"` → GrokAcp; `"fake" | "fixture"` → Fake; anything else fails closed.

### Cross-layer mechanical drift capture

Test: `runtime_composition::tests::runtime_selection_parse_matches_storage_adapter_parse`

Walks a fixed label set (empty, aliases, case variants, rejects) and asserts
for every label:

1. Both parsers `Ok` with the same mapped selection, **or**
2. Both parsers `Err` with the same code `invalid_runtime_selection`
3. Neither path can yield `Ok(Fake)` on a reject label

If someone re-implements parse on either side, this fails without human review.

## 3. Composition wiring (§3.2)

`open_conversation_service(db_path, emitter)` now:

1. Opens `SqliteSettingsRepository` on the same DB
2. Calls `resolve_runtime(None, &process_env_map())` — process startup is global scope
3. Maps adapter → `RuntimeSelection`
4. For GrokAcp, builds config via `discover_grok_config_with_overrides` from
   **already-resolved** path strings (env consulted once inside resolve)

### Env priority (E2E life-line)

- `process_env_map()` only reads allow-listed names (`PILLOW_RUNTIME`,
  `PILLOW_GROK_EXECUTABLE`, `PILLOW_GROK_PROFILE`)
- Presence of a key in that map makes `SettingSource::Env` win over project /
  global / default — same contract as S0a
- Proof tests:
  - `env_adapter_wins_over_stored_value`
  - `env_path_overrides_win_over_stored_paths`
  - `open_service_respects_env_fake_over_stored_grok`
  - `settings_commands_tests::env_map_injection_documents_e2e_lifeline`
  - `settings_commands_tests::env_override_wins_over_stored_and_marks_source_env`

**E2E life-line intact:** `PILLOW_RUNTIME=fake` still forces Fake even when SQLite
stores `grok-acp`. No stored value can suppress env.

### No storage → same as today

`no_stored_settings_behaves_like_env_only_default`: unset env + empty store →
GrokAcp / Default sources / path `None` (PATH + home discovery still apply for
real Grok open, as before).

### Fail-closed

- Illegal env / parse labels → `invalid_runtime_selection`, never Fake
- Tests: `illegal_env_adapter_fails_closed_never_fake`,
  `illegal_label_parse_never_returns_fake`,
  settings command mirror tests

## 4. Tauri command layer (§3.3)

New module `settings_commands.rs` + `settings_commands_tests.rs`.

| Command | Role |
| --- | --- |
| `get_runtime_settings` | Resolve with source attribution |
| `set_runtime_adapter` | Global / project write |
| `set_grok_executable` | Path write |
| `set_grok_profile` | Path write |
| `clear_runtime_setting` | Delete one known key |

DTO: `RuntimeSettingsDto` — fields only
`adapter` / `grokExecutable` / `grokProfile` / `projectId`, each setting
`value` + `source` + optional `envVar`. **No secret fields.**

Explicit DTO assertion: `runtime_settings_dto_has_no_secret_fields` (forbidden
substrings + structural allow-list on JSON keys). Renderer
`assertNoSecretFields` rejects secret-shaped keys on the wire.

`lib.rs`: mod + command registration + `app.manage(SqliteSettingsRepository)`.

## 5. Settings UI (§3.4–3.5)

`apps/desktop/src/features/settings/`:

- `SettingsPanel.tsx` — form: constrained `<select>` for adapter, path inputs
- `settingsGateway.ts` — Tauri + Mock gateways
- `settingsTypes.ts` — DTO types (no secrets)

Mount: `App.tsx` fixed “设置” button (`data-testid="nav-open-settings"`) + dialog.
Does **not** alter sidebar rail structure. Tauri gateway auto-selected when
`__TAURI_INTERNALS__` / `__TAURI__` present; tests use mock.

### Env override UI (ADR-0003)

When `source === "env"`:

- Field is **disabled / read-only**
- Badge text: **「已被环境变量覆盖」**
- Note names the env var (`PILLOW_RUNTIME` / …)
- Test: `settings-panel.test.tsx` → `shows env override state and keeps the field read-only`

### Visual discipline

- Styles in `styles/settings.css` only (not `brand-mark.css`)
- Existing tokens only (`--bg-input`, `--text-primary`, `--accent-muted`,
  `--warning` for override chrome, etc.)
- Modal shell reuses container-level glass (`.modal-card`); **form controls are
  opaque** — no `backdrop-filter` on fields/rows
- Radius: `--radius-sm` / `--radius-md` (6–8px) on form controls
- Lilac only via existing accent tokens for active scope + CTA save

### Dual-skin contrast (body ≥ 4.5:1)

Unchanged product tokens (asserted by existing
`visual-shell-contract.test.tsx`):

| Skin | Pair | Contract |
| --- | --- | --- |
| dark | `--text-primary` / `--bg-main` | ≥ 4.5:1 |
| dark | `--text-secondary` / `--bg-main` | ≥ 4.5:1 |
| light | `--text-primary` / `--bg-main` | ≥ 4.5:1 |
| light | `--text-secondary` / `--bg-main` | ≥ 4.5:1 |

Settings form uses the same text/surface tokens; no new body colors.

## 6. Six required tests — coverage map

| # | Requirement | Where |
| --- | --- | --- |
| 1 | Cross-layer parser consistency | `runtime_selection_parse_matches_storage_adapter_parse` |
| 2 | Env wins over storage | composition + settings_commands tests (map + live process_env) |
| 3 | No storage = today | `no_stored_settings_behaves_like_env_only_default` / `no_stored_values_resolve_to_defaults` |
| 4 | Illegal fail-closed, no Fake | parse + env fail-closed tests (composition + commands) |
| 5 | DTO no secret fields | `runtime_settings_dto_has_no_secret_fields` + TS `assertNoSecretFields` |
| 6 | UI env override read-only | `settings-panel.test.tsx` |

## 7. testid / a11y / geometry

- **No existing `data-testid` deleted or renamed**
- **No existing accessible names changed**
- New only: `nav-open-settings`, `settings-panel`, `settings-adapter`,
  `settings-*-env-override`, path fields, save, etc.
- Layout: single continuous left rail unchanged; settings is a fixed button +
  modal overlay (does not add a second rail or alter workbench grid)
- `visual-shell-contract.test.tsx` still green (not weakened)

## 8. Gates (exit codes)

| Command | Exit |
| --- | --- |
| `node scripts/run-worker-gate.mjs` | **0** (`PILLOW_WORKER_GATE: OK`) |
| `corepack pnpm --filter @pillow/desktop build` | **0** |

Gate includes `cargo fmt --check`, typecheck, vitest, workspace cargo tests,
desktop lib tests (124), ACP drift check.

## 9. What was **not** touched

- `crates/storage-sqlite/src/**` (S0a frozen)
- `crates/runtime-host/**`, `git-executor/**`, `adapter-grok-acp/**`
- `scripts/**`, any `Cargo.toml`
- `brand-mark.css` / BrandMark
- Desktop E2E / acceptance scripts (controller-owned)
- No git add/commit/stash/reset/clean/checkout
- No secrets UI, keychain, auth file reads
- No new runtime deps, network fonts, or image backgrounds

## 10. Residual risks

1. **Process-level composition is startup-only.** Settings writes update SQLite
   immediately; adapter/path take effect for the **next** process start. UI
   states this explicitly after save.
2. **Empty `PILLOW_GROK_PROFILE` via resolve path:** S0a `resolve_runtime` rejects
   empty path env as `settings_invalid_path`. Legacy `discover_grok_config`
   (env-only test seam) still fell through empty profile to home. Production
   open goes through resolve first — empty profile env now fails closed
   (aligned with S0a; E2E does not rely on empty profile).
3. **Startup project scope:** composition resolves with `project_id = None`
   (global + env). Project-scoped adapter applies in UI resolution when a
   project is selected; process adapter selection remains global until a
   future design scopes process open per project.
4. **Keeper bridge boot** returned HTTP 400 in this environment; work proceeded
   per contract without session recording.

## 11. Files written / modified (whitelist)

**New**

- `apps/desktop/src-tauri/src/settings_commands.rs`
- `apps/desktop/src-tauri/src/settings_commands_tests.rs`
- `apps/desktop/src/features/settings/**`
- `apps/desktop/src/styles/settings.css`
- `apps/desktop/tests/settings-panel.test.tsx`
- `docs/execution/2026-08-08-settings-s0b-report.md`

**Modified**

- `apps/desktop/src-tauri/src/runtime_composition.rs`
- `apps/desktop/src-tauri/src/lib.rs` (mod + commands + manage settings repo)
- `apps/desktop/src/App.tsx` (mount entry only)

## Completion token

**SETTINGS-S0B-COMPLETE**
