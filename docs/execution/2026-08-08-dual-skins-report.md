# Dual skins report — light + dark token sets (no persistence)

**Date:** 2026-08-08  
**Signal:** `DUAL-SKINS-COMPLETE`  
**Scope:** Pure CSS dual theme tokens + visual contract extension. No settings storage, no settings UI, no `app_settings`.

## Summary

Delivered two complete, same-name token sets under `[data-theme="dark"]` (default via bare `:root` as well) and `[data-theme="light"]`. Dark keeps the existing BLACK INK / GRAPHITE layering and LILAC HAZE accent. Light uses OFF-WHITE canvas, GRAPHITE body text, SMOKE-derived borders/secondary hierarchy, and LILAC accent for fills only. `color-scheme` switches with the theme. Toggle is non-persistent: set/remove `data-theme` on `<html>`.

## Files touched (whitelist only)

| Path | Change |
| --- | --- |
| `apps/desktop/src/styles/tokens.css` | Dual theme token sets + docs for switch |
| `apps/desktop/tests/visual-shell-contract.test.tsx` | Extended dual-skin lock; existing dark asserts kept |
| `docs/execution/2026-08-08-dual-skins-report.md` | This report |

**Not touched:** `brand-mark.css`, `BrandMark.tsx`, `App.tsx`, `features/**`, `crates/**`, `scripts/**`, any storage/settings path.

## How to switch (dev / visual QA only)

```js
// Light
document.documentElement.setAttribute("data-theme", "light");

// Dark (explicit)
document.documentElement.setAttribute("data-theme", "dark");

// Default dark (no attribute)
document.documentElement.removeAttribute("data-theme");
```

No localStorage / sessionStorage / SQLite / settings panel.

## Token对照表 (semantic set — same names both skins)

Layout metrics (`--titlebar-height`, `--sidebar-width`, `--radius-*`, `--space-*`, `--text-sm`) and brand primitives (`--brand-*`) live on `:root` and are theme-invariant. Semantic color/surface tokens below appear in **both** theme blocks with equal names and counts.

| Token | Dark | Light |
| --- | --- | --- |
| `color-scheme` | `dark` | `light` |
| `--bg-app` | `#0e0e10` | `#faf9f7` |
| `--bg-sidebar` | `rgba(20, 20, 24, 0.72)` | `rgba(238, 237, 240, 0.78)` |
| `--bg-sidebar-solid` | `#141416` | `#eeedf0` |
| `--bg-main` | `#161618` | `#f5f4f1` |
| `--bg-aside` | `rgba(20, 20, 24, 0.72)` | `rgba(238, 237, 240, 0.78)` |
| `--bg-elevated` | `#1c1c20` | `#f7f6f3` |
| `--bg-hover` | `rgba(220, 203, 255, 0.06)` | `rgba(62, 62, 70, 0.06)` |
| `--bg-active` | `rgba(220, 203, 255, 0.1)` | `rgba(220, 203, 255, 0.32)` |
| `--bg-input` | `#121214` | `#fffeff` |
| `--bg-code` | `#0a0a0c` | `#ebeae6` |
| `--bg-user-bubble` | `#2a2a30` | `#ebe4f7` |
| `--border-subtle` | `rgba(199, 199, 206, 0.1)` | `rgba(62, 62, 70, 0.1)` |
| `--border-strong` | `rgba(199, 199, 206, 0.2)` | `rgba(62, 62, 70, 0.18)` |
| `--border-focus` | `rgba(220, 203, 255, 0.55)` | `rgba(140, 110, 200, 0.55)` |
| `--text-primary` | `#faf9f7` | `#3e3e46` |
| `--text-secondary` | `#c7c7ce` | `#5c5c66` |
| `--text-tertiary` | `rgba(199, 199, 206, 0.72)` | `#7a7a84` |
| `--text-inverse` | `#0e0e10` | `#faf9f7` |
| `--accent` | `#dccbff` | `#dccbff` |
| `--accent-muted` | `rgba(220, 203, 255, 0.14)` | `rgba(220, 203, 255, 0.4)` |
| `--accent-hover` | `#ebe0ff` | `#c9b4f5` |
| `--accent-fg` | `#0e0e10` | `#0e0e10` |
| `--success` | `#3ecf8e` | `#187a50` |
| `--danger` | `#f07178` | `#b83a42` |
| `--warning` | `#f0993d` | `#a85a10` |
| `--glass-bg` | `rgba(20, 20, 24, 0.68)` | `rgba(250, 249, 247, 0.72)` |
| `--glass-bg-strong` | `rgba(28, 28, 32, 0.78)` | `rgba(247, 246, 243, 0.84)` |
| `--glass-border` | `rgba(199, 199, 206, 0.14)` | `rgba(62, 62, 70, 0.1)` |
| `--glass-highlight` | `inset 0 1px 0 rgba(250, 249, 247, 0.06)` | `inset 0 1px 0 rgba(255, 255, 255, 0.72)` |
| `--glass-glow` | `inset 0 0 0 1px rgba(220, 203, 255, 0.04)` | `inset 0 0 0 1px rgba(220, 203, 255, 0.14)` |
| `--glass-blur` | `18px` | `20px` |
| `--glass-saturate` | `1.15` | `1.2` |
| `--shadow-composer` | dark multi-layer black | light multi-layer graphite |
| `--canvas` | `var(--bg-app)` | `var(--bg-app)` |
| `--panel` | `var(--bg-main)` | `var(--bg-main)` |
| `--panel-muted` | `var(--bg-sidebar-solid)` | `var(--bg-sidebar-solid)` |
| `--line` | `var(--border-subtle)` | `var(--border-subtle)` |
| `--line-strong` | `var(--border-strong)` | `var(--border-strong)` |
| `--text` | `var(--text-primary)` | `var(--text-primary)` |
| `--muted` | `var(--text-secondary)` | `var(--text-secondary)` |
| `--accent-soft` | `var(--accent-muted)` | `var(--accent-muted)` |

Brand constants (both skins): LILAC `#dccbff`, OFF-WHITE `#faf9f7`, SMOKE `#c7c7ce`, GRAPHITE `#3e3e46`, BLACK INK `#0e0e10`.

## Contrast (measured WCAG relative luminance)

Body target ≥ 4.5:1; large text ≥ 3:1. Lilac never used as light body/small text.

### Dark

| Pair | Ratio | Role |
| --- | --- | --- |
| `#faf9f7` on `#161618` (`--text-primary` / `--bg-main`) | **17.17:1** | Body |
| `#c7c7ce` on `#161618` (`--text-secondary` / `--bg-main`) | **10.75:1** | Secondary |
| `#faf9f7` on `#0e0e10` (`--text-primary` / `--bg-app`) | **18.33:1** | Body on app |
| `#0e0e10` on `#dccbff` (`--accent-fg` / `--accent`) | **12.88:1** | On-accent control label |

### Light

| Pair | Ratio | Role |
| --- | --- | --- |
| `#3e3e46` on `#faf9f7` (`--text-primary` / `--bg-app`) | **10.07:1** | Body |
| `#3e3e46` on `#f5f4f1` (`--text-primary` / `--bg-main`) | **9.63:1** | Body on stage |
| `#5c5c66` on `#f5f4f1` (`--text-secondary` / `--bg-main`) | **6.01:1** | Secondary (≥4.5) |
| `#7a7a84` on `#f5f4f1` (`--text-tertiary` / `--bg-main`) | **~3.8:1** | Tertiary / meta (large-text band) |
| `#0e0e10` on `#dccbff` (`--accent-fg` / `--accent`) | **12.88:1** | On-accent control label |
| `#187a50` on `#f5f4f1` (`--success`) | **4.85:1** | Status text |
| `#b83a42` on `#f5f4f1` (`--danger`) | **5.13:1** | Status text |
| `#a85a10` on `#f5f4f1` (`--warning`) | **4.62:1** | Status text |

**Rejected for light body:** raw SMOKE `#c7c7ce` on OFF-WHITE ≈ **1.60:1**; LILAC `#dccbff` on OFF-WHITE ≈ **1.42:1**. Secondary was deepened to `#5c5c66`; status hues darkened for light only.

Light canvas is OFF-WHITE / warm gray (`#faf9f7`, `#f5f4f1`, `#eeedf0`, `#f7f6f3`) — not pure `#ffffff` as the app base.

## Frosted glass surfaces

Container-level only (existing rules; tokens themed):

| Surface | Mechanism |
| --- | --- |
| Left sidebar `.app-sidebar` | `--glass-bg`, blur/saturate, highlight |
| Resource / context pane `.resource-pane` | same glass tokens |
| Floating composer `.composer-wrap--float .composer` | `--glass-bg-strong` |
| Modal card `.modal-card` | `--glass-bg-strong` |
| Utility class `.surface-glass` | shared glass recipe |

**Not applied:** per-row blur on scroll lists; conversation reading stage remains solid `--bg-main`.  
`prefers-reduced-transparency: reduce` still flattens glass to solid tokens.

## Visual contract test extension

`apps/desktop/tests/visual-shell-contract.test.tsx`:

1. **Kept** all prior dark locks (`color-scheme: dark`, `--bg-main: #161618`, `--bg-app: #0e0e10`, `--accent: #dccbff`, anti-blue negatives, nav ghost, circular send, single left rail, composer radius/focus, run-activity quiet disclosure).
2. **Added** dual-skin block: requires both `[data-theme="dark"]` and `[data-theme="light"]`; parses both rule bodies; asserts identical custom-property name sets; locks light `--bg-app: #faf9f7`, `--text-primary: #3e3e46`, `--accent: #dccbff`; forbids pure-white app/main; locks light `color-scheme: light`; forbids storage APIs in tokens source.
3. **Added** runtime assert: setting `document.documentElement` `data-theme` switches computed `--bg-app` / `--text-primary` / `color-scheme` between light and dark.

## Hard constraints — confirmation

| Constraint | How verified |
| --- | --- |
| No `data-testid` deleted/renamed | Diff limited to CSS + visual test + report |
| Accessible names unchanged | No component/feature edits |
| Layout geometry unchanged | No shell/feature CSS geometry edits; existing geometry asserts still pass |
| No settings storage | Tokens only; test asserts no localStorage/sessionStorage/indexedDB in tokens.css |
| No new runtime deps / network fonts | CSS only |
| No git destructive ops | None performed |
| No desktop E2E | Not run (controller-owned) |
| No `.superpowers/` / auth files | Not accessed |

## Gate results

| Command | Exit code | Notes |
| --- | --- | --- |
| `node scripts/run-worker-gate.mjs` | **0** | `PILLOW_WORKER_GATE: OK` (final successful run) |
| `corepack pnpm --filter @pillow/desktop build` | **0** | Vite production build |

Earlier attempts hit environment flakes unrelated to this slice:

- MSVC `LNK1104` (cannot open `*.exe` under `target/debug/deps` — file lock / AV)
- Occasional timing flake in `governed_process_bridge::slow_output_...`
- One vitest 5s timeout in `conversation-fix-pass2` race test

Package-level vitest (including dual-skin visual contract, 8/8) and typecheck were green throughout successful runs. Final full worker gate passed.

## Residual risk

1. **No live pixel screenshots in this slice** — visual QA of light glass/stacking is manual via `data-theme="light"`; desktop E2E remains controller-owned.
2. **Hardcoded dark-ish fallbacks** in some feature CSS (`var(--x, #dark-fallback)`) only apply if the custom property is missing; with dual tokens on `:root` they do not affect normal themed rendering.
3. **Modal dimmer** in chrome still uses a fixed dark translucent overlay (`rgba(20, 20, 24, 0.4)`); acceptable for both skins but not tokenized.
4. **Theme preference persistence** intentionally absent — product settings/`app_settings` is a later slice.
5. **Windows cargo linker/file-lock flakes** can still fail the gate under parallel load; not caused by CSS skins.

## Completion signal

`DUAL-SKINS-COMPLETE`
