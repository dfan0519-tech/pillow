# Step 1d — Brand identity + frosted glass visual upgrade

**Date:** 2026-08-08  
**Worker:** grok-step1d  
**Controller:** Claude  
**Signal:** `STEP1D-BRAND-GLASS-COMPLETE`

## Summary

Pillow desktop chrome now carries the Concept 08 «Mirage Loop» brand mark (provisional 2D SVG) and a layered dark palette based on BLACK INK / GRAPHITE / LILAC HAZE, with container-level frosted glass on sidebar, context resource pane, floating composer, and modal card. Reading surfaces (conversation timeline / stage body) stay solid and unblurred.

## Brand mark implementation

### Files

| Path | Role |
| --- | --- |
| `apps/desktop/src/components/BrandMark.tsx` | `BrandSymbol`, `BrandWordmark`, `BrandMark` |
| `apps/desktop/src/styles/brand-mark.css` | Wordmark weight/tracking, lilac tittle, tagline |
| `apps/desktop/src/components/icons.tsx` | `IconPillowMark` updated to compact möbius ribbon |
| `apps/desktop/src/features/projects/AppSidebar.tsx` | Brand row uses `<BrandMark size={22} showWordmark />` |
| `apps/desktop/index.html` | Title `pillow` + provisional SVG data-URI favicon |

### Symbol

- Continuous horizontal infinity / möbius-style **band** (filled path with thickness, not a single stroke).
- Linear gradient LILAC HAZE → OFF-WHITE → LILAC HAZE, plus soft Gaussian glow and a center twist highlight.
- Unique gradient/filter IDs per mount instance to avoid SVG id collisions.

### Wordmark

- Lowercase `pillow`, system sans stack only (no webfonts).
- `font-weight: 300`, `letter-spacing: 0.1em`.
- **i-dot memory point:** Latin small letter dotless *i* (`ı`) + absolute lilac tittle with soft glow.
- Accessible name via `aria-label="pillow"` on the wordmark (decorative glyph tree is `aria-hidden`).

### Tagline

- Styles for `SOFTWARE HARNESS TOOL` exist on `BrandMark` (`showTagline`).
- **Not shown in the sidebar** (compact chrome only). Available for a future about surface.
- Optional slogan `Flow with memory.` was not wired (no about page in scope).

### Non-final asset declaration (required)

> **The design source mark is a 3D-rendered ribbon. The inline SVG is a clean 2D vector interpretation (gradient + soft-glow approximation) for product chrome only.**  
> **Final 3D identity assets must be exported from design source and replace** `BrandMark.tsx` / `IconPillowMark` / the favicon data URI. **Do not treat these paths as production-final brand art.**

No Tauri application icon PNGs were generated under `src-tauri/icons/**` (optional; needs multi-size raster export from design).

## Palette mapping (old → new)

Brand baseline (authoritative):

| Name | Hex |
| --- | --- |
| LILAC HAZE | `#DCCBFF` |
| OFF-WHITE | `#FAF9F7` |
| SMOKE GRAY | `#C7C7CE` |
| GRAPHITE | `#3E3E46` |
| BLACK INK | `#0E0E10` |

Token assignments:

| Token | Old | New | Notes |
| --- | --- | --- | --- |
| `--bg-app` | `#0d0d0d` | `#0e0e10` | BLACK INK |
| `--bg-sidebar` / solid | `#101010` | `rgba(20,20,24,0.72)` / `#141416` | Glass + solid fallback |
| `--bg-main` | `#151515` | `#161618` | Slight GRAPHITE lift off pure black |
| `--bg-aside` | `#101010` | glass rgba (same family) | Context column |
| `--bg-elevated` | `#1a1a1a` | `#1c1c20` | Composer / elevated cards |
| `--bg-hover` / `--bg-active` | white washes | lilac washes | Brand accent family |
| `--text-primary` | `rgba(255,255,255,0.92)` | `#faf9f7` | OFF-WHITE |
| `--text-secondary` | `rgba(255,255,255,0.68)` | `#c7c7ce` | SMOKE GRAY |
| `--text-tertiary` | `rgba(255,255,255,0.5)` | `rgba(199,199,206,0.72)` | Smoke muted |
| `--text-inverse` / `--accent-fg` | `#0d0d0d` | `#0e0e10` | BLACK INK |
| `--accent` | `#8aa4ff` (blue) | `#dccbff` | LILAC HAZE |
| `--accent-hover` | `#a0b6ff` | `#ebe0ff` | Lighter lilac |
| `--accent-muted` | blue rgba | lilac rgba | |
| `--border-focus` | blue rgba | lilac rgba | |
| `--glass-*` | *(none)* | new | Container glass only |

Primitive tokens `--brand-lilac|off-white|smoke|graphite|ink` are also exposed for brand-mark CSS.

## Frosted glass surfaces

| Surface | Selector | Technique |
| --- | --- | --- |
| Left sidebar | `.app-sidebar` | `backdrop-filter: blur(18px) saturate(1.15)`, translucent `--glass-bg`, fine border, inset highlight |
| Context / resource pane | `.resource-pane` | Same container glass |
| Floating composer | `.composer-wrap--float .composer` | Stronger glass fill + blur + soft shadow |
| Modal card | `.modal-card` | Glass elevated card |
| Utility class | `.surface-glass` | Shared helper in `tokens.css` |

**Not glassed (readability / performance):**

- `.stage-body`, `.conversation-surface`, timeline message rows
- Scroll list rows (`.project-row`, `.session-row`, `.ctx-resource-*` rows)
- Agent activity / run-activity row chrome

**Reduced transparency:** `@media (prefers-reduced-transparency: reduce)` disables `backdrop-filter` and falls back to solid sidebar / elevated fills.

## Contrast (measured)

WCAG relative luminance contrast on solid approximate backgrounds (Python sRGB formula). Glass is translucent; solid fallbacks `#141416` / `#1c1c20` / `#161618` are the conservative floors for body text.

| Pair | Ratio | Pass |
| --- | --- | --- |
| `--text-primary` `#FAF9F7` on `--bg-main` `#161618` | **17.17:1** | ≥ 4.5 body |
| `--text-secondary` `#C7C7CE` on `#161618` | **10.75:1** | ≥ 4.5 body |
| Tertiary approx (0.72 smoke over `#161618` → ~`#95959b`) | **~6.2:1** | ≥ 4.5 body |
| Primary / secondary on glass solid `#141416` | **17.49 / 10.94** | pass |
| Primary / secondary on elevated `#1c1c20` | **16.14 / 10.10** | pass |
| Accent `#DCCBFF` on `#0E0E10` (send / primary buttons) | **12.88:1** | ≥ 3 large / ≥ 4.5 UI |

Large-type / UI chrome uses accent and secondary well above 3:1.

## Performance

- `backdrop-filter` only on **three container surfaces** (+ modal when open), not on list rows.
- Blur radius capped at **18px** with moderate saturate.
- Reading column remains opaque solid `--bg-main` so scroll cost is unchanged.
- Reduced-transparency media query avoids blur work when the user prefers it.

## Red-line verification

| Constraint | How confirmed |
| --- | --- |
| No `data-testid` delete/rename | AppSidebar retains `app-sidebar`, `nav-open-project`, `project-conversations`, `conversation-row`. Added only `brand-mark` (new). Vitest green (E2E not run). |
| Accessible names / copy | Unchanged: `导航`, `搜索`, `新建对话`, `打开项目`, project `aria-label`s, `项目对话`. No workbench/resource label edits. |
| Layout geometry | CSS only on existing shell selectors; grid remains `sidebar-width | 1fr` and single `.app-sidebar`. No structural dual-rail. |
| No new runtime deps | Inline SVG only. |
| No webfonts | System stack in tokens + brand-mark CSS. |
| Forbidden paths | Did **not** edit `src/lib/**`, `features/workbench/**`, `features/execution/**`, `crates/**`, `scripts/**`. |

### Whitelist exception (documented)

`apps/desktop/tests/visual-shell-contract.test.tsx` asserted `--bg-main: #151515` and `--accent: #8aa4ff`. Those values are intentionally replaced by this step; the test was updated so the required vitest gate can pass. **Controller should accept this as a brand-token contract refresh, not a scope creep into product logic.**

## THIRD_PARTY_NOTICES.md

Updated the `tokens.css` row:

- **Before:** Claimed adaptation of Grok App `[data-theme="light"]` soft neutrals and `--accent: #3d5fd9`.
- **After:** Structure/token *names* still follow the Grok App tokens layout pattern; **color values are Pillow-original** (Mirage Loop five-color brand). Explicitly states `#3d5fd9` / `#8aa4ff` are **not** claimed. Glass tokens are Pillow-original.
- `workbench.css` row notes glass fills may use Pillow glass tokens while control grammar remains upstream-adapted.

## Gate results

| Command | Exit code |
| --- | --- |
| `corepack pnpm --filter @pillow/desktop exec vitest run --no-cache` | **0** (30 files / 377 tests) |
| `corepack pnpm --filter @pillow/desktop typecheck` | **0** |
| `corepack pnpm --filter @pillow/desktop build` | **0** |
| `node scripts/check-secrets.mjs` | **0** |
| `git diff --check` | **0** (CRLF conversion warnings only) |
| `git diff --cached --name-only` | empty (**0**) |

Desktop E2E **not run** (controller-owned).

## Files touched (this step)

**New**

- `apps/desktop/src/components/BrandMark.tsx`
- `apps/desktop/src/styles/brand-mark.css`
- `docs/execution/2026-08-08-step1d-brand-glass-report.md`

**Modified**

- `apps/desktop/src/styles/tokens.css`
- `apps/desktop/src/styles/workbench.css`
- `apps/desktop/src/styles/workbench-shell.css`
- `apps/desktop/src/styles/workbench-chrome.css`
- `apps/desktop/src/styles/agent-activity.css` (accent/elevated fallbacks)
- `apps/desktop/src/styles/execution.css` (accent/elevated fallbacks)
- `apps/desktop/src/styles/write-approval.css` (accent/elevated fallbacks)
- `apps/desktop/src/styles/resource-pane.css` / `resource-preview.css` (elevated fallbacks)
- `apps/desktop/src/components/icons.tsx`
- `apps/desktop/src/features/projects/AppSidebar.tsx`
- `apps/desktop/index.html`
- `THIRD_PARTY_NOTICES.md`
- `apps/desktop/tests/visual-shell-contract.test.tsx` (whitelist exception)

Working tree also contains **other workers’** dirty files (`crates/**`, git gateway, etc.); those were not modified by this step.

## Residual risk

1. **2D mark ≠ 3D design source** — visual QA may ask for design-exported assets before release marketing surfaces.
2. **Glass over animated content** — WebView2 blur cost depends on what’s behind the sidebar/composer; if screenshots show lag, lower `--glass-blur` or solidify sidebar.
3. **Send button now lilac** (was near-white on dark) — intentional brand accent; controller screenshot pass should confirm contrast/readability of the circular send control.
4. **Wordmark “Pillow” → “pillow”** in the brand row only (project names unchanged). Confirm no human-facing copy freeze required uppercase product name in chrome.
5. Visual contract test now locks brand hexes; future palette tweaks must update that test.

## Not done

- Application icon multi-size set under `apps/desktop/src-tauri/icons/**`
- About page with tagline / slogan `Flow with memory.`
- Design-exported 3D mark replacement
- Desktop E2E / screenshot capture (controller)
- Light theme (explicitly out of scope — dark remains default)

---

`STEP1D-BRAND-GLASS-COMPLETE`
