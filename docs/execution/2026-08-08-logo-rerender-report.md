# Logo re-render — Concept 08 «Mirage Loop» geometry

**Date:** 2026-08-08  
**Worker:** grok-worker  
**Controller:** Claude  
**Signal:** `LOGO-RERENDER-BLOCKED` (logo deliverable ready; sole worker gate red on unrelated env flakes)

## Summary

Replaced the flat symmetric ∞ outline in `BrandMark` with a layered SVG
approximation of the Mirage Loop ribbon: continuous band width, asymmetric
lobes, möbius face values, explicit over/under at the cross with contact
shadow, path hue, specular rim, and soft lilac bloom. Props, `data-testid`,
accessible names, and layout box geometry are unchanged.

> **Non-final asset.** Final 3D identity should be exported from design source
> and replace this inline SVG. Do not treat these paths as production-final
> brand art.

## Files touched (whitelist only)

| Path | Role |
| --- | --- |
| `apps/desktop/src/components/BrandMark.tsx` | Layered Mirage Loop symbol + size variants |
| `apps/desktop/src/styles/brand-mark.css` | Bloom opacity ladders; wordmark unchanged |
| `docs/execution/2026-08-08-logo-rerender-report.md` | This report |

No changes to `tokens.css`, `crates/**`, `scripts/**`, call sites, or test ids.

## Geometry spec → implementation map

### 1.1 Form — continuous closed ribbon, asymmetric lobes, variable width

| Spec | Implementation |
| --- | --- |
| Continuous closed band (not a hairline stroke) | Six centerline segments with **stroke widths 5.8–9.2** (viewBox height 55 → full band ≈ **14–18%** of mark height). Round caps join into one continuous ribbon topology. |
| Horizontal double-leaf ∞, **asymmetric** | Left lobe apex ~`x=17`, rounder vertical span; right lobe apex ~`x=90`, longer horizontal span, flatter vertical (`PATH.leftTop` / `leftBottom` vs `rightTop` / `rightBottom`). Glow silhouette matches asymmetry. |
| Width varies (face-on wide, side narrow) | Face strokes `w.face` (~8.6–9.2); cross/side `w.cross` / `w.side` (~5.8–6.8). Perspective contraction at the crossing. |

Topology (one closed möbius loop):

```text
left apex → left top → overpass (NW→SE) → right bottom → right apex
         → right top → underpass (NE→SW) → left bottom → left apex
```

### 1.2 Crossing — front/back + contact shadow

| Spec | Implementation |
| --- | --- |
| Clear over/under | Paint order: **underpass** body first, lobe faces next, **overpass** last (`brand-symbol__overpass`). |
| Shadow on under strand | `brand-symbol__cross-shadow`: same underpass path, wider stroke `#4A4458` (purple-deep grey, not pure black), soft blur filter, drawn under the underpass body. |

### 1.3 Twist and dual faces

| Spec | Implementation |
| --- | --- |
| Different face values | **Light face:** `leftTop` + overpass → gradients `#F5F2FF` / `#FAF9F7` / `#E8E4F0`. **Dark face:** `leftBottom` + `rightTop` + underpass → `#B8A8D0` → `#8E869C` → `#6E6878`. **Silver path:** `rightBottom` → smoke `#C7C7CE` family. |
| Transition near outer arc tops | Face assignment flips at left/right **apexes** (segment boundaries at lobe tips), not mid-cross. Specular rims sit on those outer arcs. |

### 1.4 Light and color

| Spec | Implementation |
| --- | --- |
| Hue path lilac → near-white → smoke → lilac | `linearGradient` `pathHue` (`#DCCBFF` → `#FAF9F7` → `#C7C7CE` → `#DCCBFF`); used as compact hue-wash; full detail uses per-face gradients in the same palette. |
| Specular rim | `rimLeft` / `rimRight` white strokes (~0.45–0.55) on outer arc tops; overpass center highlight (`overpass-spec`). |
| Soft purple bloom | Filled asymmetric silhouette + `feGaussianBlur` + lilac `feColorMatrix` tint (`g.bloom`), low opacity. |
| No pure-black shadow | Cross shadow `#4A4458`; dark face greys are purple-tinted. |

## Size ladders and legibility

| Size | Detail | Strategy |
| --- | --- | --- |
| ~22px (sidebar) | `compact` | Underpass + shadow, left/right lobe loops (2 faces), overpass on top, hue wash. **No fine rim** (would dissolve). Over/under retained — does not collapse to flat ∞. |
| ~64px (about) | `standard` | Full 6 face segments, contact shadow, dual rims, moderate bloom. |
| ~128px (app icon) | `hero` | Same geometry + stronger bloom blur, light-face glow merge, stronger rim opacity. |

Thresholds: `size < 28` → compact; `< 80` → standard; else hero.

## Wordmark

Unchanged contract:

- lowercase `pillow`, system stack, `font-weight: 300`, `letter-spacing: 0.1em`
- **i-dot** lilac `#DCCBFF` via dotless `ı` + `.brand-wordmark__tittle`
- no webfonts

## Interface / a11y / geometry freeze

| Check | Result |
| --- | --- |
| Props `size` / `showWordmark` / `showTagline` / `title` / `className` | Preserved |
| Call sites | Untouched (`AppSidebar` still `<BrandMark size={22} showWordmark />`) |
| `data-testid="brand-mark"` | Present, not renamed |
| `aria-label` / decorative `aria-hidden` / `role="img"` when titled | Same pattern |
| Wordmark `aria-label="pillow"` | Same |
| Layout box | Still `width: size`, `height: size * 0.55` (viewBox now `100×55`, same aspect) |
| No desktop E2E | Not run (controller-owned) |
| No git write ops | Not used |

## Commands

| Command | Exit | Notes |
| --- | --- | --- |
| `corepack pnpm --filter @pillow/desktop build` | **0** | Vite production build green |
| `corepack pnpm typecheck` (via gate) | **0** | `@pillow/desktop` tsc green every attempt |
| `corepack pnpm test` / vitest (via gate) | **0** on successful runs | One load-flake timeout in `conversation-fix-pass2` under heavy parallel load; subsequent full packages passed |
| `node scripts/run-worker-gate.mjs` | **1** (multiple retries) | Always fails at `cargo-test-workspace` for **unrelated** env issues (see below) |
| `node scripts/run-cargo.mjs test -p keeper-runtime-host --test governed_process_bridge slow_output_…` | **0** | Isolated re-run of the flaky test passes |

### Worker gate failure pattern (not logo-related)

Repeated `PILLOW_WORKER_GATE: FAILED at step "cargo-test-workspace"`:

1. **LNK1104** — `link.exe` cannot open `target\debug\deps\governed_process_bridge-*.exe` / `fake_runtime-*.exe` / `host_run_ownership-*.exe` (Windows file lock; antivirus or leftover test process).
2. **Flaky process timing** — under full workspace parallel load, `slow_output_is_visible_live_and_reopens_after_waiter_consumes_terminal` sometimes panics on live output; same test is green alone.
3. Occasional adapter process cleanup flake (`shutdown_kills_descendant…`) under load.

**None of these crates or tests were modified.** Whitelist touch set is renderer-only (`BrandMark.tsx`, `brand-mark.css`, this report).

## Residual risk

1. **2D approximation only** — true 3D torsion/occlusion cannot match a rendered mesh; design export should replace this when available.
2. **Stroke-ribbon vs flat band mesh** — faces are segment color changes along a thick stroke, not a filled developable surface; at large sizes a mesh/export will read richer.
3. **Filter cost** — bloom/shadow blurs are light; if sidebar paint jank appears, compact already reduces blur stdDeviation.
4. **Keeper boot** — worker bridge boot returned `400 parentSessionId` (controller session); work proceeded from task contract alone.
5. **`IconPillowMark` in `icons.tsx`** — out of whitelist; still the older flat mark. Controller may schedule a follow-up if the compact icon must match.
6. **Sole gate red** — controller should re-run `node scripts/run-worker-gate.mjs` when `target\debug\deps` is unlocked / machine quieter; logo code does not need further changes for gate health.

## Signal

`LOGO-RERENDER-BLOCKED: worker gate cargo-test-workspace fails on Windows LNK1104 / unrelated process-bridge flakes (logo TS/CSS + desktop build + typecheck + vitest green)`
