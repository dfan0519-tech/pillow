# Wave C1 Desktop Visual Acceptance

Date: 2026-07-31

Scope: Track C only. This gate neither changes the desktop renderer nor claims
that the concurrent renderer work has merged.

## Run

```powershell
node --test --test-concurrency=1 apps/desktop/tests/e2e/wave-c1-visual-acceptance.spec.ts
```

The runner writes nine deterministic PNGs and `report.json` to
`artifacts/wave-c1-visual/`. It uses the screenshot fixture, a system Chrome or
Edge browser, `deviceScaleFactor: 1`, and no user profile or Tauri state.
`CHROME_PATH` or `EDGE_PATH` can override browser discovery.

## C1 Matrix

| Pillow state | Grok reference | Required desktop captures | Pass criteria | Fail criteria |
| --- | --- | --- | --- | --- |
| Empty workbench | supplied reference 2: quiet new-chat canvas, one left rail, floating composer | `empty-1280x800.png`, `empty-1440x900.png`, `empty-1920x1080.png` | Exactly one navigation rail; wide unblocked conversation canvas; composer remains inside the canvas; no page scroll | launcher/dashboard card grid, dual persistent rails, clipped composer, horizontal/page scroll |
| Selected project | supplied reference 2: project hierarchy remains in the left rail while the center stays calm | `selected-project-1280x800.png`, `selected-project-1440x900.png`, `selected-project-1920x1080.png` | Project is selected in the rail, conversation list is nested below it, selected canvas stays dominant | project selection opens a dashboard/admin panel, nested list is clipped, rail steals more than 40% of the viewport |
| Active conversation | supplied reference 3: nested conversation selection, broad timeline, fixed floating composer | `active-conversation-1280x800.png`, `active-conversation-1440x900.png`, `active-conversation-1920x1080.png` | User and assistant timeline entries render; timeline owns overflow; composer stays stable at bottom of the stage | timeline/composer overlap, document scroll trap, conversation content leaks into a second rail, clipped focus target |

Reference 1 is an account/settings surface and is used only for the shared
dark, restrained, low-contrast chrome: it must not pull account-dashboard
cards, quota charts, or settings navigation into the C1 workbench.

## Automated Assertions

- Each PNG is captured at exactly 1280x800, 1440x900, and 1920x1080.
- Sidebar, workbench, stage body, and composer have nonzero rectangles fully
  inside the viewport. The main canvas is at least 60% of viewport width.
- The document cannot scroll in either axis. The stage body, rather than the
  composer or document, owns timeline overflow.
- Only one `[data-testid="app-sidebar"]` may exist. Known dashboard/dual-rail
  selectors (`.dashboard`, `.metric-card`, `.project-launcher`,
  `.project-sidebar`, `.conversation-sidebar`) fail the gate.
- The composer input has a programmatic label, the rail and send button have
  accessible names, and keyboard focus can reach an unclipped composer input
  with a visible outline.
- Under `prefers-reduced-motion: reduce`, the composer transition duration is
  at most 1 ms.

The structural dashboard rejection is intentional: a passing screenshot that
contains a generic admin workbench is a failed C1 acceptance, even where boxes
do not overlap.

## Integration Contract

The renderer merge must preserve these stable test surfaces:

| Purpose | Contract |
| --- | --- |
| Shell/rail | `[data-testid="app-sidebar"]`, `[data-testid="workbench"]`, accessible navigation label |
| Empty state | `[data-testid="empty-workbench"]` |
| Selected state | `[data-testid="selected-workbench"]`, `[data-testid="active-project-name"]` |
| Conversation | accessible `新建对话` button, `#work-request`, `[data-testid="timeline-user-message"]`, `[data-testid="timeline-assistant-message"]` |
| Composer | `[data-testid="composer"]`, `[data-testid="composer-send"]`, labelled `#work-request` |

If a UI refactor needs different names, update this gate and the renderer test
contract in the same merge. Do not solve a failing visual gate by dropping the
state, accessibility, geometry, or dashboard-rejection assertion.

## Current Baseline Status

PASS on the current concurrent renderer baseline: all nine captures completed
with `status: "passed"`. The gate verified no geometry overflow, document
scroll traps, duplicate rails, known dashboard surfaces, missing labels,
unreachable focus, or composer motion under reduced-motion emulation.

The final post-merge rerun must include the command above, plus
`pnpm --filter @pillow/desktop test:e2e`, and a human side-by-side check of the
nine generated images against the three supplied references. The existing
package E2E command has an explicit file list, so it will not run this new C1
specification until that command is intentionally expanded by its owner.
