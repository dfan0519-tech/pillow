# First-run onboarding — worker report

Date: 2026-08-09  
Worker: grok (firstrun-onboarding)  
Controller: Claude  
Work directory: `<repo>`  
`CARGO_TARGET_DIR`: `<repo>/target-firstrun`

## Signal

**FIRSTRUN-ONBOARDING-COMPLETE**

(Full `node scripts/run-worker-gate.mjs` did not go green: **BLOCKED-BY-CONCURRENT** on
`crates/adapter-grok-acp/src/prompt.rs` rustfmt only — out of whitelist; not modified.
Scoped verification of this ticket’s surface is green — see §7.)

## 1. Research restatement

Research §3: first-run guidance was **zero**. Strangers landed on an empty workbench
with no next step. Existing surfaces covered **misconfiguration** (degraded banner)
and **auth-class send failure** (`grok login` guidance), not **never configured**.

## 2. What shipped

### 2.1 Trigger matrix (never-configured vs misconfigured)

| Case | Signal | UX |
| --- | --- | --- |
| First run, no projects, not dismissed, runtime ready | `projects.length === 0` + `dismissed === false` + not degraded | Neutral `role="region"` card: open project + Grok readiness tip |
| Configured user (negative control) | `projects.length ≥ 1` | **No** first-run card (even if `dismissed` still false) |
| Dismissed / skipped | settings `ui.onboarding.dismissed = true` | **No** card (survives remount / restart via settings) |
| Misconfigured paths | `!conversationAvailable && degradedCode` | **Only** existing `runtime-degraded-banner` (`role="alert"`); first-run hidden |

Pure function: `shouldShowFirstRunOnboarding` in
`apps/desktop/src/features/onboarding/shouldShowFirstRunOnboarding.ts`.

### 2.2 Dismiss persistence (settings layer reuse — proven)

| Layer | Path |
| --- | --- |
| Key (host storage, pre-landed by controller) | `ui.onboarding.dismissed` (`KEY_ONBOARDING_DISMISSED`) |
| Semantics | Absent → `false` (show once); only literals `true`/`false`; **no env override** |
| Host API | `SqliteSettingsRepository::resolve_onboarding_dismissed` / `upsert_onboarding_dismissed` |
| Tauri commands | `get_onboarding_settings` / `set_onboarding_dismissed` in `settings_commands.rs` |
| Handler registration | `apps/desktop/src-tauri/src/lib.rs` (minimal mount; same pattern as theme) |
| Renderer gateway | `SettingsGateway.getOnboardingSettings` / `setOnboardingDismissed` |
| Mock | `MockSettingsGateway` in-memory flag (same contract as Tauri) |

**No** second store (no localStorage, no JSON beside `keeper.db`, no new tables).

Dismiss actions: **跳过**, close (`aria-label="关闭引导"`), and successful **打开项目**
(primary completion path) all call `setOnboardingDismissed(true)`.

### 2.3 Copy / ADR-0003 claim

Grok body (fixed string `FIRSTRUN_GROK_BODY`):

> 对话需要本机已安装 Grok CLI。Grok 登录由 Grok CLI 管理，Pillow 不存储该登录。请在终端运行 grok login 后再发消息。

Asserted in tests: contains `不存储` + `grok login`; `guidanceLooksSafe`; no
Pillow-stores-login implication; no auth/token leakage patterns.

### 2.4 Visual discipline

- Full-width bar (same placement class as degraded/login banners) — **does not**
  alter sidebar width / single continuous left rail.
- `role="region"` — **not** `role="alert"` (never-configured ≠ error).
- Body text: `color: var(--text-primary)` on elevated mix; dual-skin contrast ≥ 4.5:1
  (token math in tests).
- Lilac only on primary CTA / focus; no blur, no network fonts/images, no telemetry.

## 3. Files touched (whitelist + required mount)

| Path | Change |
| --- | --- |
| `apps/desktop/src/features/onboarding/**` | New: copy, trigger, card |
| `apps/desktop/src/features/settings/settingsTypes.ts` | `OnboardingSettingsView` + gateway methods |
| `apps/desktop/src/features/settings/settingsGateway.ts` | Tauri + Mock get/set |
| `apps/desktop/src/App.tsx` | Load flag, mount card, dismiss |
| `apps/desktop/src/styles/onboarding.css` | New styles (`brand-mark.css` untouched) |
| `apps/desktop/src-tauri/src/settings_commands.rs` | DTO + get/set commands |
| `apps/desktop/src-tauri/src/lib.rs` | Register two commands (mount) |
| `apps/desktop/tests/firstrun-onboarding.test.tsx` | Six required scenarios + extras |
| `docs/execution/2026-08-09-firstrun-onboarding-report.md` | This report |

**Not touched:** `crates/**`, `scripts/**`, `tests/fixtures/**`, other `docs/**`,
`.superpowers/**` (except reading), git history operations, desktop E2E,
auth/token files, existing `data-testid` values / accessible names / geometry
assertions, `visual-shell-contract` (not weakened).

## 4. Six tests (coverage map)

| # | Requirement | How covered |
| --- | --- | --- |
| 1 | First run, no projects, not skipped → shows | Mock empty projects + default `dismissed=false` → `firstrun-onboarding` visible |
| 2 | Dismiss → gone after remount | Click dismiss; assert gateway `dismissed=true`; unmount + remount same gateway → still gone |
| 3 | **Negative control** configured user | Non-empty projects + `dismissed=false` → card absent |
| 4 | Config error → degraded only | `setDegradedStartup` + empty projects → `runtime-degraded-banner` only |
| 5 | ADR claim / no Pillow stores login | Body asserts + `guidanceLooksSafe` |
| 6 | Dual-skin body contrast ≥ 4.5:1 | Token `--text-primary` vs `--bg-elevated` dark+light; CSS pins body to primary |

Extra: close control same path; open-settings reaches settings panel.

**Result:** `vitest run tests/firstrun-onboarding.test.tsx` → **12/12 passed**.

Related suites (login-guidance, settings-panel, project-workbench, visual-shell,
theme-settings) remained green in the broader desktop run.

## 5. Geometry / testid / a11y / Fake

- **No** existing `data-testid` deleted or renamed; only **new** firstrun testids.
- **No** existing accessible names changed (`打开设置`, sidebar, workbench, etc.).
- Onboarding is an extra full-width bar above `.project-shell` — same pattern as
  degraded/login; does not split the left rail or change rail geometry tokens.
- Degrade still never falls back to Fake (host composition unchanged).

## 6. Distinction proof: 没配过 vs 配错了

| | 没配过 (first-run) | 配错了 (degraded) |
| --- | --- | --- |
| Surface | `firstrun-onboarding` | `runtime-degraded-banner` |
| Role | `region` (status guidance) | `alert` (error) |
| Headline | 欢迎使用 Pillow | 对话运行时不可用 |
| Mutual exclusion | Hidden when degraded | Shown only when degraded |

## 7. Commands / exit codes

| Command | Exit | Notes |
| --- | --- | --- |
| `export CARGO_TARGET_DIR=<repo>/target-firstrun` | 0 | PowerShell `$env:CARGO_TARGET_DIR=...` |
| `corepack pnpm --filter @pillow/desktop typecheck` | **0** | |
| `vitest run tests/firstrun-onboarding.test.tsx` | **0** (12 pass) | PowerShell may surface stderr noise as non-zero wrapper; vitest summary green |
| `corepack pnpm --filter @pillow/desktop build` | **0** | Vite production build |
| `cargo check -p pillow-desktop` | **0** | Uses storage helpers; commands compile |
| `node scripts/run-worker-gate.mjs` | **1** | Fails at `cargo-fmt` on **`crates/adapter-grok-acp/src/prompt.rs`** only — concurrent worker file; **not** this ticket. Reported as **BLOCKED-BY-CONCURRENT**. |

## 8. What this worker did **not** touch

- `crates/**` (storage key already landed by controller; not re-edited)
- Concurrent worker files under `adapter-grok-acp` / `runtime-host` / `knowledge-indexer`
- `scripts/**`, `tests/fixtures/**`
- Desktop E2E (controller-owned)
- Git: no `add` / `commit` / `stash` / `reset` / `clean` / `checkout` / `restore`
- Auth/token files
- WebView2 bootstrapper / Fake adapter policy / existing login-guidance path

## 9. Residual risk

- Full workspace gate remains red until concurrent `adapter-grok-acp` fmt is resolved
  by that worker or a controller reformat pass.
- Dismiss write failure: session hides guidance; next cold start may re-show if SQLite
  write failed (logged only).
- “Grok not ready” on first-run is **educational** copy on the card when conversation
  runtime is available; true path/exe failures still use degraded only.
- Auto-dismiss on successful open-project is intentional completion; canceling the
  folder picker does **not** dismiss.
