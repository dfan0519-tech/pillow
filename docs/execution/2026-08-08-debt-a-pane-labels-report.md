# DEBT-A — Contextual pane label redundancy

Date: 2026-08-08  
Worker: grok1  
Controller: Claude  
Scope: checkpoint open risk §5.4 (`docs/verification/2026-08-07-wave-c4-checkpoint.md`)  
Signal: **DEBT-A-PANE-LABELS-COMPLETE**

## 1. Defect

After D3B4 mode switcher, a 1024×720 narrow contextual pane could show the same
mode word three times in Changes mode:

1. `ResourcePane` h2 (`heading` →「变更」)
2. Selected tab (`aria-label` / visible text →「变更」)
3. `GitChangesView` section title h3 →「变更」

Resources mode had the same class of redundancy for「资源」(h2 + selected tab).
That contradicts the C4 design placement: Changes is a compact collapsible
context mode, not a second dashboard.

## 2. Fix (and why it is best for accessibility)

### Approach

| Surface | Change |
| --- | --- |
| Pane h2 when mode switcher is visible | Stable generic title **「上下文」** (names the pane, not the mode) |
| Pane h2 when switcher is hidden | Unchanged:「资源」/「变更」/「终端输出」/「写入差异」 |
| Mode tabs | **Unchanged** — keep visible text, `aria-label`, `role="tab"`, `aria-selected`, testids |
| `GitChangesView` | Remove the visual h3「变更」title only |
| `GitChangesView` section | Keep `aria-label="工作区变更"` so the body region stays named for AT |
| CSS | Drop unused `.git-changes-view__title` rules |

No DTO/prop changes on `GitChangesView`. No mode-priority or default-mode change.
No testid deleted or renamed.

### Accessibility rationale

WAI-ARIA **Tabs** pattern places mode identity on the tab control
(`role="tab"` + accessible name + `aria-selected`). Screen readers already
announce the selected mode when focus moves through the tablist
(`aria-label="上下文面板模式"`).

Repeating the same mode word as:

- a document heading (h2),
- the selected tab name, and
- an inner section heading (h3)

is noisy for both sighted users (especially at 1024×720) and AT heading/tab
navigation. The better split is:

1. **Pane heading** → stable object name「上下文」(when dual modes exist)
2. **Tabs** → sole mode names「资源」/「变更」(keyboard + SR mode switch)
3. **Body region** → `aria-label="工作区变更"` (or resource regions) without a
   second visible mode title; identity content for Changes is the branch / HEAD
   / dirty-count line, which is more informative than another「变更」

When the switcher is absent (no Git capability, or Terminal / write Diff
surfaces), the specific h2 labels remain so the single-mode pane still has a
clear heading and no tablist.

## 3. Files changed (whitelist only)

| File | Role |
| --- | --- |
| `apps/desktop/src/features/workbench/ResourcePane.tsx` | Generic h2 when `showModeSwitcher` |
| `apps/desktop/src/features/execution/GitChangesView.tsx` | Remove redundant h3 title |
| `apps/desktop/src/styles/execution.css` | Remove unused title styles |
| `apps/desktop/tests/resource-pane.test.tsx` | Heading expectations + new single-label a11y test |
| `apps/desktop/tests/c4-git-changes-integration.test.tsx` | Heading/tab expectations in composition tests |

Not modified: `App.tsx`, `Workbench.tsx`, any `crates/**`, any `scripts/**`,
`.superpowers/**`. Staging left empty (no `git add`).

## 4. Tests

### New / extended coverage

- `shows each mode name once while tabs keep accessible names and testids`
  (resource-pane workbench suite):
  - Resources mode: exact text「资源」once; tabs retain accessible names and
    testids; pane/close/switcher testids still locate.
  - Changes mode: exact text「变更」once; no heading named「变更」; selected
    tab accessible name preserved; all existing Git Changes testids still
    locate.
- Composition test asserts single「变更」and generic「上下文」heading after
  explicit Changes selection.

### Existing suites updated

- Heading expectations updated from mode-specific h2 to「上下文」wherever the
  mode switcher is present.
- No-switcher path still expects h2「资源」.

### Run results

```text
corepack pnpm --filter @pillow/desktop exec vitest run \
  tests/resource-pane.test.tsx tests/c4-git-changes-integration.test.tsx --no-cache
```

| File | Tests | Result |
| --- | --- | --- |
| `tests/resource-pane.test.tsx` | 19 | pass |
| `tests/c4-git-changes-integration.test.tsx` | 13 | pass |
| **Total** | **32** | **pass** |

Exit code: **0**

## 5. Required gates

| Command | Exit code |
| --- | --- |
| vitest focused suite (above) | **0** |
| `corepack pnpm --filter @pillow/desktop typecheck` | **0** |
| `node scripts/check-secrets.mjs` | **0** (`secret gate ok — 9 patterns self-tested, no findings in tracked files`) |
| `git diff --cached --name-only` | **0**, empty output |

Not run (hard forbid): desktop E2E / `test:e2e*` / wave acceptance scripts.

## 6. Residual risk

- **Visual only until desktop E2E lane is free.** Unit/integration cover DOM
  labels and testids; no 1024×720 screenshot gate was run in this worker
  (E2E channel exclusive to another worker).
- **Nested region names** `aria-label="工作区变更"` still appear on both the
  ResourcePane host section and `GitChangesView` root section. Pre-existing
  nesting; out of scope for label-redundancy debt A (would need host-only or
  view-only naming, not a simple title drop).
- **Unrelated dirty scripts** may exist in the working tree
  (`scripts/wave-c4-*.mjs`, etc.). This worker did not edit them; they are
  outside the write whitelist.

## 7. Completion signal

**DEBT-A-PANE-LABELS-COMPLETE**
