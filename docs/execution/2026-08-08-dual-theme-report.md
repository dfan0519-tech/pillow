# Dual Theme (Light + Dark) — Worker Report

**Date:** 2026-08-08  
**Worker:** grok-worker (dual-theme)  
**Controller:** Claude  
**Working directory:** `<repo>`  
**Signal:** `DUAL-THEME-BLOCKED: no existing project/settings storage path for UI theme preference`

---

## 1. Outcome

**Blocked before implementation.** Dual skins, theme toggle, visual-contract extension, and contrast work were **not landed**. The task requires selection to **persist and restore across restart** via an **existing** project/settings storage pathway, and forbids inventing ad hoc JSON. Inventory shows **no such pathway** in the current product surface or in this worker’s write whitelist.

No application source under the dual-theme whitelist was modified. No gates were run (no code change to verify).

---

## 2. Persistence inventory (why blocked)

### 2.1 What “existing pathway” would mean

Per task + `AGENTS.md`:

- Domain state belongs in **SQLite**, not ad hoc JSON files.
- Do **not** invent a new store if none exists.
- Write whitelist for this worker **excludes** `crates/**` and `scripts/**`, so a new migration / Tauri command cannot be added here.

### 2.2 SQLite today (`crates/storage-sqlite`)

Migrations present:

| Migration | Domain |
| --- | --- |
| `0001_projects.sql` | projects (id, name, paths, trust, timestamps) |
| `0002_conversations.sql` | conversations |
| `0003_tool_calls.sql` | tool calls |
| `0004_resource_recents.sql` | resource recents |
| `0005_write_proposals.sql` | write proposals |
| `0006_run_events_write_kinds.sql` | run event kinds (write) |
| `0007_exec_proposals.sql` | exec proposals |
| `0008_run_events_process_kinds.sql` | run event kinds (process) |

**No** `app_settings` / `user_preferences` / `ui_theme` table.  
**No** free-form key-value settings repository in `storage-sqlite`.

Projects row has only identity/path/trust/timestamps — no JSON prefs blob, no theme column.

### 2.3 Desktop / Tauri command surface

Renderer gateways under `apps/desktop/src/lib/`:

- `projectGateway` — list / create / select / pick / open-or-register  
- `conversationGateway` — conversation runtime  
- `resourceGateway`, `writeApprovalGateway`, `execGateway`, `gitGateway`  

**No** settings/preference gateway.  
**No** `get_setting` / `set_setting` / `get_ui_theme` Tauri commands.  
`package.json` has only `@tauri-apps/api` — **no** `tauri-plugin-store` or similar.

Lifecycle paths:

- `keeper.db` — product SQLite (domain objects above)  
- `desktop-ready.json` — smoke/acceptance readiness only (not a user prefs API; inventing theme JSON here would violate the task)

### 2.4 Explicit non-options (would be “inventing storage”)

| Approach | Why rejected |
| --- | --- |
| `localStorage` / `sessionStorage` | Task forbids ad hoc client store; research treats browser storage as non-authority for product state; c4 tests already guard some paths against these APIs |
| New JSON under app-data | Explicitly forbidden by task + AGENTS.md |
| Piggyback on `projects` / conversations / recents | Wrong domain; no schema field; would require `crates/**` migration anyway |
| In-memory only toggle | Fails “重启后恢复” |
| Worker-owned SQLite migration + Tauri API | Requires `crates/**` + `src-tauri` (outside whitelist) |

### 2.5 Keeper Bridge

Boot POST to `http://127.0.0.1:7890/api/agent-bridge/boot` returned **400 Bad Request** in this environment. Not used as a substitute prefs store (and AGENTS.md: Keeper is record-only for Pillow project work).

---

## 3. Dual skins / toggle (not implemented — blocked)

Planned design (for controller unblock + next worker), **not applied**:

### 3.1 Token structure

- Shared brand primitives on `:root` (LILAC HAZE / OFF-WHITE / SMOKE / GRAPHITE / BLACK INK).
- Semantic tokens duplicated under:
  - `:root, [data-theme="dark"]` — current dark values (default)
  - `[data-theme="light"]` — OFF-WHITE base, GRAPHITE text, LILAC accent with safe contrast
- `color-scheme: dark | light` on each theme selector.
- Same token **names and count** on both themes (no theme-only tokens).

### 3.2 Default

- Dark remains default (`data-theme="dark"` or equivalent `:root` dark when unset).
- No change to current users’ first-paint look once persistence lands with default `dark`.

### 3.3 Toggle placement (whitelist)

- Entry in workbench chrome (`features/workbench/**`) or shell chrome wired from `App.tsx`.
- Must not change existing `data-testid`s or accessible names listed in the task.
- Geometry of left rail / stage must stay unchanged (toggle is chrome-only).

### 3.4 Persistence (blocked — required unlock)

Ideal product path after controller authorizes storage work:

1. **Migration** e.g. `0009_app_settings.sql`:

   ```sql
   CREATE TABLE IF NOT EXISTS app_settings (
     key TEXT PRIMARY KEY NOT NULL,
     value TEXT NOT NULL,
     updated_at TEXT NOT NULL
   );
   ```

2. **Repository** in `crates/storage-sqlite` (`get` / `set` for key `ui.theme` ∈ `{dark,light}`).
3. **Tauri commands** e.g. `get_app_setting` / `set_app_setting` (or dedicated `get_ui_theme` / `set_ui_theme`).
4. **TS gateway** (new or extended) used only from `App.tsx` for load-on-boot + save-on-toggle.
5. Default when missing key: `dark`.

Until that exists, dual-theme UI cannot honestly claim “restart restore.”

---

## 4. Contrast plan (not measured in product — blocked)

Target when light tokens are authored:

| Pair (light, planned) | Role | WCAG target |
| --- | --- | --- |
| Body text on `--bg-main` | body | ≥ 4.5:1 |
| Secondary text on `--bg-main` | secondary | ≥ 4.5:1 if used as body-like copy |
| Large title / controls | large text | ≥ 3:1 |
| Accent as **text** on OFF-WHITE | danger zone | LILAC HAZE `#DCCBFF` on `#FAF9F7` is **far below** 4.5:1 — accent must not be body text color on light; use GRAPHITE/BLACK INK for body, lilac for fill/border/highlight only |

Dark (current, from step1d baseline; approximate):

| Pair | Approx contrast |
| --- | --- |
| OFF-WHITE `#FAF9F7` on BLACK INK `#0E0E10` | ~18:1 (body OK) |
| SMOKE `#C7C7CE` on `#161618` (`--bg-main`) | ~9:1+ (secondary OK) |
| LILAC `#DCCBFF` as **fill** with `--accent-fg` ink | button text contrast is ink-on-lilac, not lilac-on-dark as body |

**No runtime contrast metering was run** because light tokens were not landed. Controller should require measured values in a follow-up report after implementation.

---

## 5. Glass plan (not applied)

- Keep glass on **container chrome only**: sidebar, resource pane, floating composer, modal — already the step1d rule.
- Light theme: slightly higher glass opacity / lower blur if needed so panels don’t “milk” reading columns.
- Conversation / stage reading surfaces stay solid (no backdrop blur on timeline body).

---

## 6. Visual contract tests (not extended)

File: `apps/desktop/tests/visual-shell-contract.test.tsx`

Current locks (must **keep and strengthen**, not delete):

- `color-scheme: dark` (today on `:root`)
- `--bg-main: #161618`, `--bg-app: #0e0e10`, `--accent: #dccbff`
- Negative: no old blue accents `#1769aa` / `#8aa4ff`

Planned extension after skins land:

- Assert both `[data-theme="dark"]` and `[data-theme="light"]` blocks define the **same** semantic token keys.
- Lock dark values as today under dark selector.
- Lock light values (e.g. `--bg-app` OFF-WHITE family, text GRAPHITE/ink, accent still LILAC HAZE).
- Assert `color-scheme` follows each theme.
- Keep all existing negative blue assertions.

---

## 7. Hard-prohibition checklist (this run)

| Rule | Status |
| --- | --- |
| No delete/change of `data-testid` | N/A — no UI edits |
| No accessible-name changes | N/A |
| No layout geometry changes | N/A |
| No new runtime deps / network fonts | N/A |
| No git add/commit/stash/reset/clean/checkout/restore | Honored |
| No desktop E2E | Honored |
| No `.superpowers/` | Honored |
| No auth/token files | Honored |
| Whitelist only; no `crates/**`, `BrandMark`, `brand-mark.css` | Honored (zero product edits) |

---

## 8. Commands / exit codes

| Command | Exit | Notes |
| --- | --- | --- |
| Keeper Bridge boot | HTTP 400 | Environment; not used as store |
| `node scripts/run-worker-gate.mjs` | **not run** | No code change; would not unblock storage |
| `corepack pnpm --filter @pillow/desktop build` | **not run** | Same |

---

## 9. Residual risk / unlock for controller

**Blocker is structural, not stylistic.** Skins and toggle can be done entirely inside the desktop whitelist **once** a durable settings API exists.

### Recommended controller follow-up (out of this worker’s whitelist)

1. Accept a small **app settings** slice:
   - SQLite migration + repository tests in `crates/storage-sqlite`
   - Tauri get/set commands + desktop lifecycle open of same `keeper.db`
   - Thin TS gateway (may need whitelist expansion beyond current dual-theme list)
2. Re-dispatch dual-theme worker with:
   - tokens split to dark/light
   - `App.tsx` load/save theme
   - workbench toggle
   - visual-contract dual locks + contrast table with measured ratios
3. Keep default `dark`.

### What this worker did **not** do (by design)

- Did not invent localStorage/JSON prefs.
- Did not partially land dual CSS that would diverge from “restart restore” acceptance.
- Did not touch brand-mark worker paths.

---

## 10. Signal

```
DUAL-THEME-BLOCKED: no existing project/settings storage path for UI theme preference (SQLite has no app_settings/prefs; no settings Tauri/gateway; whitelist forbids crates/**; ad hoc JSON/localStorage forbidden)
```
