# Theme persistence + light tertiary contrast — completion report

Date: 2026-08-08  
Status: **THEME-PERSISTENCE-COMPLETE**  
Worker: grok-worker (settings-layer second consumer)

## 1. Purpose

Prove the S0 settings layer can host a second config class (UI theme), not only
runtime adapter paths — without a parallel store. Also strengthen light-skin
`--text-tertiary` to body-size contrast ≥ 4.5:1.

## 2. `ui.theme` reuses existing key / scope / precedence

**No parallel theme store.** Theme rows live in the same `settings_entries`
table as runtime settings.

| Concern | Implementation |
| --- | --- |
| Key | `ui.theme` (`KEY_UI_THEME`) — same `upsert` / `get` / `delete` / `list` API |
| Value kind | `enum` (same as `runtime.adapter`) |
| Values | `dark` \| `light` only; default `dark`; **no `system`** |
| Precedence | `env → project → global → default` via `resolve_theme` |
| Env allow-list | `PILLOW_THEME` (`ENV_UI_THEME`) in `process_env_map()` |
| Fail-closed | `SettingsError::InvalidThemeSelection` → code `invalid_theme_selection` |
| Write validation | `validate_known_key_write` accepts only known keys including `ui.theme` |

Product UI exposes **global only** for theme edits (`set_ui_theme` always writes
global). Resolution still walks the full precedence chain so the storage
machinery is proven reusable; project rows are not offered in the UI.

Proof tests:

- `theme_uses_same_settings_entries_table_not_parallel_store`
- `theme_precedence_default_global_project_env`
- `theme_env_wins_over_stored_global`
- `theme_survives_repository_reopen`

## 3. `PILLOW_THEME` override

- Allow-listed in `process_env_map` alongside the three runtime env names.
- Presence wins even over stored global/project values; `source = env`.
- DTO surfaces `envVar: "PILLOW_THEME"` when source is env.
- UI: SettingsPanel shows “已被环境变量覆盖”, field disabled, note
  `data-testid="settings-theme-env-override"`.

## 4. Fail-closed error code

| Layer | Behavior |
| --- | --- |
| Parse | `UiThemeSetting::parse` rejects `system`, empty, unknown |
| Stored row | `parse_stored_theme` errors; resolve does **not** fall through or default |
| Env | Illegal `PILLOW_THEME` → resolve `Err` |
| Code | `invalid_theme_selection` (stable, mirrored for adapter as `invalid_runtime_selection`) |

Never silently returns `Ok(Dark)` for illegal input.

## 5. Anti-flash (FOUC)

How first paint avoids a dark→light jump:

1. **CSS product default is dark** (`:root` + `[data-theme="dark"]`).
2. **FOUC guard** in `tokens.css`:
   `html:not([data-theme-ready]) { visibility: hidden; }`
3. **App** loads theme via `settingsGateway.getThemeSettings` in
   `useLayoutEffect`, then `applyDocumentTheme(theme)` which sets
   `data-theme` **and** `data-theme-ready` together.
4. First **visible** paint therefore has the resolved theme (including
   `PILLOW_THEME=light` for deterministic light screenshots).
5. Unit tests stamp `data-theme-ready` in `tests/setup.ts` so
   `toBeVisible` remains meaningful without weakening production FOUC.

Theme changes from Settings save call `applyDocumentTheme` immediately
(no restart required for theme; runtime paths still restart).

## 6. Light tertiary contrast

| Item | Value |
| --- | --- |
| Background | `--bg-main` light = `#eeebe6` |
| Before | `#7a7a84` → contrast **3.57:1** (fails body 4.5:1) |
| After | `#686872` → contrast **≈ 4.633:1** |
| Dark tertiary | Unchanged (`rgba(199, 199, 206, 0.72)`, already ≥ 4.5:1) |

### Calculation (WCAG 2.x relative luminance)

For sRGB channel \(c \in [0,1]\):

\[
c_\text{lin} = \begin{cases}
c/12.92 & c \le 0.03928 \\
((c+0.055)/1.055)^{2.4} & \text{otherwise}
\end{cases}
\]

\[
L = 0.2126 R_\text{lin} + 0.7152 G_\text{lin} + 0.0722 B_\text{lin}
\]

\[
\text{contrast} = (L_\text{lighter} + 0.05) / (L_\text{darker} + 0.05)
\]

For `#686872` vs `#eeebe6`:

- \(L(\#eeebe6) \approx 0.840\)
- \(L(\#686872) \approx 0.142\)
- ratio \(\approx (0.840 + 0.05) / (0.142 + 0.05) \approx 4.633\)

Token name count and names remain identical across skins. Visual contract
locks `#686872` and asserts ratio ≥ 4.5; forbids regression to `#7a7a84`.

## 7. Six required tests — coverage map

| # | Requirement | Where |
| --- | --- | --- |
| 1 | `ui.theme` precedence + `source` | `settings_repository.rs`: `theme_precedence_*` |
| 2 | `PILLOW_THEME` beats storage | storage + `settings_commands_tests`: env wins |
| 3 | Illegal fail-closed | storage + unit + command tests (`invalid_theme_selection`) |
| 4 | Restart restore | `theme_survives_repository_reopen` |
| 5 | Light tertiary numeric contrast | `visual-shell-contract` + `theme-settings.test.tsx` |
| 6 | UI env override read-only | `settings-panel.test.tsx` theme env test |

Additional UI: save light theme, no `system` option, App FOUC stamp.

## 8. Geometry / testid / a11y

- **No existing `data-testid` deleted or renamed.**
- New testids only: `settings-theme`, `settings-theme-env-override`.
- Existing accessible names preserved; theme field uses `aria-label="外观主题"`.
- Layout geometry of shell chrome untouched (settings form adds one field only).
- Existing visual contract assertions kept; tertiary assertion **strengthened**.

## 9. Commands

| Command | Exit code |
| --- | --- |
| `node scripts/run-worker-gate.mjs` | **0** (`PILLOW_WORKER_GATE: OK`) |
| `corepack pnpm --filter @pillow/desktop build` | **0** |

Gate included: workspace check, secrets, `cargo fmt --check`, typecheck, pnpm
tests (393 desktop), cargo tests (workspace), ACP drift check.

## 10. Whitelist note (minimal wiring outside brief list)

Strict write list did not include re-export / invoke registration. Mechanical
wiring required for the feature to function:

- `crates/storage-sqlite/src/lib.rs` — re-export `KEY_UI_THEME`, `ENV_UI_THEME`,
  `UiThemeSetting`
- `apps/desktop/src-tauri/src/lib.rs` — register `get_theme_settings`,
  `set_ui_theme`

No domain logic in those files beyond export/register. No `Cargo.toml` changes.

## 11. Not touched (explicit)

- `scripts/**` (controller-owned screenshots / acceptance)
- `crates/runtime-host/**`, `git-executor/**`, `adapter-grok-acp/**`
- `apps/desktop/src/features/**` outside `settings/`
- `brand-mark.css`
- Any `Cargo.toml`
- No git add/commit/stash/reset/clean/checkout
- No desktop E2E (`test:e2e*`, wave acceptance scripts)
- No secret / key storage (S1)
- No new runtime deps, network fonts, or image backgrounds

## 12. Residual risks

1. **Async theme resolve**: FOUC guard hides until IPC/mock resolves. If
   `get_theme_settings` hangs, UI stays hidden — same class of failure as any
   blocked boot command.
2. **Illegal theme at boot**: shell applies product default dark so the app
   becomes visible; settings API remains fail-closed (command returns error).
3. **Desktop E2E light screenshots** still controller-owned via `PILLOW_THEME`;
   this worker did not capture acceptance images.
4. **Project-scoped theme rows** can still be written via storage API (proves
   dual-scope store); product UI never writes them.

## 13. Completion token

**THEME-PERSISTENCE-COMPLETE**
