# Wave C2-D — Resource Workbench Integration — Completion Report

**Date**: 2026-07-31
**Status**: `CONTROLLER ACCEPTED` — resource workbench integration accepted; C2 checkpoint not yet accepted
**Worker**: grok-c2d-worker (`<keeper-session>`)
**Controller session**: `<keeper-session>`
**Baseline HEAD**: `7b7f70f7360131cd1cfd984a8b19bdfb5725c21b`
**Commit**: not staged, not committed (per brief)

---

## Objective delivered

Mounted the accepted project resource browser and bounded preview into the
existing right contextual region of the Pillow workbench. Conversation remains
the dominant column (`minmax(0, 1fr)`). All browse/preview results are discarded
on project change via generation counters. No new resource capabilities were
added.

---

## Changed files (whitelist only)

| Path | Role |
|------|------|
| `apps/desktop/src/main.tsx` | Inject exactly one `TauriResourceGateway` into `App` |
| `apps/desktop/src/App.tsx` | Optional `resourceGateway?: ResourceGateway` → `Workbench` |
| `apps/desktop/src/features/workbench/Workbench.tsx` | Pass gateway; disable outer toggle without project; portable conversation paths → preview request |
| `apps/desktop/src/features/workbench/ResourcePane.tsx` | Integration host: browser + preview + conversation-reference fallback; `PreviewScope` generation discard |
| `apps/desktop/src/features/resources/ResourcePane.tsx` | Controlled `open` / `onOpenChange` / `showToggle` (default true; uncontrolled closed-by-default preserved) |
| `apps/desktop/src/styles/workbench-chrome.css` | Scrollable right region; preview section inline status styles |
| `apps/desktop/src/styles/resource-pane.css` | Host embed: no top border when inner toggle hidden |
| `apps/desktop/tests/resource-pane.test.tsx` | Controlled-mode + uncontrolled regression (10 total) |
| `apps/desktop/tests/resource-workbench-integration.test.tsx` | **New** — focused C2-D integration suite (10 cases) |
| `docs/execution/2026-07-31-wave-c2d-resource-workbench-integration-report.md` | This report |

**Not modified by this worker**: Rust, Tauri commands, DTO contracts, SQLite,
preview classification, conversation timeline/activity projection,
`<workspace>\VaultSystem\keeper`.

**Pre-existing dirty tree preserved**: other C1/B3/C2 working-tree files
untouched. `apps/desktop/tests/project-workbench.test.tsx` already carried three
composer-context label assertions from prior work; not authored in this task.

**Staging count**: `0` (nothing staged).

---

## Integration contract

### Wiring

- `main.tsx` → single `TauriResourceGateway` → `App.resourceGateway` →
  `Workbench.resourceGateway` → workbench `ResourcePane`.
- When gateway **or** active project is absent: no resource-gateway methods
  are invoked from the integration host.
- Outer stage-header resource toggle: closed by default; **disabled** with no
  project; opens/closes the one existing right pane without mutating project
  or conversation state.

### Controlled project browser

```ts
type ResourcePaneProps = {
  projectId: string;
  gateway: ResourceGateway;
  onOpenResource: (relativePath: string) => void;
  open?: boolean;                 // undefined => internal state
  onOpenChange?: (open: boolean) => void;
  showToggle?: boolean;           // default true
};
```

Workbench host uses `open={true}` and `showToggle={false}` so the stage-header
icon remains the only outer disclosure control.

### Preview generation discard

```ts
type PreviewScope = { projectId: string; generation: number };
```

Before every preview: increment generation → clear prior preview/error →
capture scope → `previewProjectResource` → commit only if active project and
generation still match. On project change, close, or unmount: increment and
clear selection/loading/error so late A results cannot paint under B.

Portable paths from tree, search, recents, and compatible conversation
references share the same preview function. Omitting `resourceGateway`
preserves the prior conversation-reference fallback UI.

---

## TDD evidence

### RED (before production wiring)

| Command | Exit | Outcome |
|---------|------|---------|
| `vitest run tests/resource-workbench-integration.test.tsx tests/resource-pane.test.tsx` | 1 | Integration: 8 failed (no gateway wiring, toggle not disabled, no browser/preview mount). Controlled mode: 1 failed (`showToggle={false}` still rendered toggle). Uncontrolled suite mostly green. |

### GREEN (after implementation)

| Command | Exit | Outcome |
|---------|------|---------|
| `pnpm --filter @pillow/desktop exec vitest run tests/resource-workbench-integration.test.tsx tests/resource-pane.test.tsx tests/project-workbench.test.tsx tests/conversation-workbench.test.tsx` | 0 | **50** passed (10 integration + 10 pane + 9 project-workbench + 21 conversation-workbench) |
| `pnpm --filter @pillow/desktop typecheck` | 0 | Clean |
| `git diff --check` | 0 | No whitespace errors |

### Final test counts

| Suite | Passed |
|-------|--------|
| `resource-workbench-integration.test.tsx` | 10 |
| `resource-pane.test.tsx` | 10 |
| `project-workbench.test.tsx` | 9 |
| `conversation-workbench.test.tsx` | 21 |
| **Total required gate** | **50** |

### Behavior covered

- Panel closed by default; zero gateway calls before open
- No active project → outer toggle disabled; zero gateway calls
- Open project A → list/recents only with A; no duplicate inner disclosure
- Tree / recent / search portable paths → `previewProjectResource` for A + accepted preview renderer
- Pending A preview cleared before B can show it; late A success/failure discarded under B
- Open B + select B path → only project B preview calls
- Close/reopen does not resurrect stale preview
- `resourceGateway` omission preserves conversation-reference ResourcePane fallback
- No machine root / raw-bytes / second-dashboard markers

---

## Residual processes

- Worker residual processes: **0** (vitest completed and exited).
- Staging count: **0**.
- No commit performed.

---

## Remaining risks

1. **Controller adversarial acceptance** still required: project-switch races under
   real Tauri gateway latency, and full two-project restart E2E remain outside
   this slice.
2. **Conversation reference UI** is hidden when a gateway is present (preview
   path replaces fallback chrome for portable paths). Non-portable conversation
   paths will not trigger preview; they still open the right pane.
3. **`resourceTypes.ts` not updated** (outside whitelist). Public controlled
   props are exported from `features/resources/ResourcePane.tsx`; the older
   `ResourcePaneProps` in `resourceTypes.ts` remains the C2-C1 snapshot shape.
4. **CSS `:has()`** used only for border polish when the inner toggle is hidden;
   layout does not depend on it for correctness.

---

## Controller handoff

Worker completion is **not** acceptance. Controller should:

1. Inspect the complete unstaged diff on the whitelist paths.
2. Execute adversarial project-switch checks.
3. Rerun the three required gates personally.
4. Decide whether the task may proceed to independent Codex2 review.
5. Release the worker lease after capture.

---

## Controller acceptance fix (2026-07-31) — stale `previewPathRequest` resurrection

**Controller session**: `<keeper-session>`
**Finding**: Important — portable conversation open left `Workbench.previewPathRequest`
populated after panel close; reopen with `open=true` re-fired the request and
asynchronously resurrected the closed preview.
**Status**: Fix landed in whitelist; gates re-run green. **Worker/fix completion is
still not formal acceptance** until the controller independently revalidates both
close paths manually.

### Root cause

Close paths only cleared `resourcePaneScope`. Child `ResourcePane` correctly
invalidated local preview on `open=false`, but the host kept
`previewPathRequest`. Reopen re-ran the conversation-path effect and issued
`previewProjectResource` again without a new user selection.

### Fix

`Workbench.closeResourcePane()` synchronously clears:

- `resourcePaneScope`
- `resourceSelection`
- `previewPathRequest`

Used by:

- stage-header toggle close branch
- contextual pane close button (`onClose`)

Open remains project-scoped and does not clear unrelated conversation/project
state. Project-switch clearing is unchanged.

### Changed files (this fix only)

| Path | Change |
|------|--------|
| `apps/desktop/src/features/workbench/Workbench.tsx` | `closeResourcePane` helper; toggle + pane close |
| `apps/desktop/tests/resource-workbench-integration.test.tsx` | Async conversation-path resurrection regression (both close paths) |
| `docs/execution/2026-07-31-wave-c2d-resource-workbench-integration-report.md` | This section |

### RED evidence (pre-fix, new test only)

| Command | Exit | Outcome |
|---------|------|---------|
| `pnpm --filter @pillow/desktop exec vitest run tests/resource-workbench-integration.test.tsx -t "conversation portable path"` | 1 | Failed at async flush after toggle reopen: `expected "spy" to be called 1 times, but got 2 times` — proves asynchronous resurrection, not a sync DOM snapshot miss. |

### GREEN evidence (post-fix)

| Command | Exit | Outcome |
|---------|------|---------|
| `pnpm --filter @pillow/desktop exec vitest run tests/resource-workbench-integration.test.tsx tests/resource-pane.test.tsx tests/project-workbench.test.tsx tests/conversation-workbench.test.tsx` | 0 | **51** passed (11 integration + 10 pane + 9 project-workbench + 21 conversation-workbench) |
| `pnpm --filter @pillow/desktop typecheck` | 0 | Clean |
| `git diff --check` | 0 | No whitespace errors |

### Final counts (required gate)

| Suite | Passed |
|-------|--------|
| `resource-workbench-integration.test.tsx` | **11** (+1 conversation-path async resurrection) |
| `resource-pane.test.tsx` | 10 |
| `project-workbench.test.tsx` | 9 |
| `conversation-workbench.test.tsx` | 21 |
| **Total required gate** | **51** |

### Regression coverage

New case
`conversation portable path: close then reopen does not async-resurrect preview`:

1. Renders `Workbench` with real mock `resourceGateway` + timeline portable resource
2. Opens resource; awaits first preview
3. Closes via **stage-header toggle**; reopens without selection; flushes microtasks/timers
4. Asserts preview call count unchanged and body does not reappear
5. Reopens resource; closes via **pane close button**; reopens without selection; same async flush + assertions

### Staging / residual

- **Staging count**: `0` (nothing staged; no commit)
- **Residual processes**: 0 (vitest exited)
- **Residual risk**:
  1. Formal controller revalidation of both close paths in a live desktop session still required for acceptance.
  2. Tree-selection close/reopen remains a sync-style guard; conversation-path case is the async call-count proof.
  3. Non-portable conversation paths still open the pane without preview (unchanged).

---

## Controller acceptance fix (2026-07-31) — project selection last-request-wins race

**Controller session**: `<keeper-session>`
**Source**: Codex2 independent review; controller confirmed
**Finding**: Important — `App.selectProject` / `activateProject` committed every
async `gateway.selectProject` success and failure without proving the request
was still the newest selection. A delayed project A result could overwrite a
completed project B selection, after which the mounted resource gateway
legitimately scoped to A.
**Status**: Fix landed in whitelist; gates re-run green. **C2-D remains rejected
until controller independent revalidation** of the race tests and full gates.

### Root cause

`busy`-disabled project buttons shrink the ordinary UI race window but are not
an authority or concurrency boundary. Batched discrete events, default
selection, tests, and future callers can still start two selections before the
busy DOM update. Without an activation generation guard, late A success
`setActiveProject(A)` and late A failure `setError(...)` mutate state owned by B.

### Fix

Independent `projectActivationGenRef` in `App`:

- Every `selectProject` and `activateProject` captures `generation =
  ++projectActivationGenRef.current` before the gateway call.
- Only the current generation may:
  - `upsertProject`
  - `setActiveProject`
  - set or clear the project error
  - clear `busy` (select path)
- Stale success returns silently.
- Stale failure returns silently (select) or is swallowed (activate) so
  `openProject` cannot paint a superseded error banner.
- Gateway contracts and store shape unchanged.

### Changed files (this fix only)

| Path | Change |
|------|--------|
| `apps/desktop/src/App.tsx` | `projectActivationGenRef`; guards on `selectProject` + `activateProject` |
| `apps/desktop/tests/project-workbench.test.tsx` | Delayed success + delayed stale failure last-request-wins cases |
| `apps/desktop/tests/resource-workbench-integration.test.tsx` | Same races + B-scoped list/recents/preview only |
| `docs/execution/2026-07-31-wave-c2d-resource-workbench-integration-report.md` | This section |

### RED evidence (pre-fix, race tests only)

Command:

```powershell
pnpm --filter @pillow/desktop exec vitest run tests/project-workbench.test.tsx tests/resource-workbench-integration.test.tsx -t "last-request-wins"
```

| Suite case | Exit | Exact failure |
|------------|------|---------------|
| project-workbench: delayed select success A cannot overwrite B | 1 | `expect(element).toHaveTextContent()` Expected `Project B` Received `Project A` at `project-workbench.test.tsx:287` |
| project-workbench: delayed select failure A cannot error banner over B | 1 | `expect(...).not.toBeInTheDocument()` found `<span>STALE-A-SELECT-FAILED</span>` at `project-workbench.test.tsx:356` |
| resource-integration: delayed A success cannot remount resource scope under A | 1 | `expect(element).toHaveTextContent()` Expected `Project B` Received `Project A` at `resource-workbench-integration.test.tsx:673` |
| resource-integration: delayed A failure cannot error banner or poison B resources | 1 | `expect(...).not.toBeInTheDocument()` found `<span>STALE-A-SELECT-FAILED</span>` at `resource-workbench-integration.test.tsx:773` |

**Summary**: 4 failed / 20 skipped. Proves late A success rewrites active project and
late A failure paints a stale project error banner over B.

### GREEN evidence (post-fix)

| Command | Exit | Outcome |
|---------|------|---------|
| `pnpm --filter @pillow/desktop exec vitest run tests/project-workbench.test.tsx tests/resource-workbench-integration.test.tsx` | 0 | **24** passed (11 project + 13 integration) |
| `pnpm --filter @pillow/desktop exec vitest run tests/resource-pane.test.tsx tests/resource-preview.test.tsx tests/resource-gateway.test.ts tests/conversation-workbench.test.tsx` | 0 | **70** passed (10 + 18 + 21 + 21) |
| `pnpm --filter @pillow/desktop typecheck` | 0 | Clean |
| `pnpm --filter @pillow/desktop build:ui` | 0 | Vite production build OK |
| `git diff --check` (whitelist paths) | 0 | No whitespace errors |

### Final counts (after this fix)

| Suite | Passed |
|-------|--------|
| `resource-workbench-integration.test.tsx` | **13** (+2 selection last-request-wins) |
| `resource-pane.test.tsx` | 10 |
| `resource-preview.test.tsx` | 18 |
| `resource-gateway.test.ts` | 21 |
| `project-workbench.test.tsx` | **11** (+2 selection last-request-wins) |
| `conversation-workbench.test.tsx` | 21 |

### Regression coverage

1. **Delayed success**: start A then B in one `act` batch (`fireEvent` clicks);
   resolve B first → B active; resolve A later → B remains; open resources →
   list/recents/preview calls are B-scoped only; no A content.
2. **Delayed stale failure**: start A then B; resolve B; reject A with
   `STALE-A-SELECT-FAILED` → B remains; no error banner; B resources usable.
3. Existing single-selection and open/register project flows remain green.

### Staging / residual (this fix)

- **Staging count**: `0` (nothing staged; no commit)
- **Residual processes**: 0 (vitest / build exited)
- **Residual risk**:
  1. Controller must independently re-run the four race tests and full gates
     before any C2-D acceptance.
  2. `openProject` still clears `busy` in its own `finally` without an
     activation-gen check; concurrent superseding is only practical once
     another path can start while `busy` is true (default-select / future
     callers). Select path busy clearing is generation-guarded.
  3. Conversation-scope `projectScopeGenRef` remains separate; this fix does
     not merge the two counters.

---

## Controller rejection follow-up (2026-07-31) — open/register last-request-wins

**Controller session**: `<keeper-session>`
**Worker session**: `<keeper-session>`
**Source**: Controller rejection of selection-only generation guard as incomplete
**Finding**: Important — `openProject()` captured generation only inside
`activateProject()` after `pickDirectory` / `openOrRegisterProject`, and wrote
`setError` / `setBusy(false)` unconditionally in outer `catch` / `finally`.
A delayed open success could late-call `selectProject(opened.id)` and overwrite
B; a delayed open failure could paint a stale error banner over B; a stale
finally could clear busy owned by a newer activation.
**Status**: Fix landed in whitelist; gates re-run green. **Worker completion is
not acceptance** — controller must independently inspect and re-run gates.

### Root cause

1. Generation was allocated at final activate, not at the user action start.
2. Outer `catch` always painted project errors from superseded open failures.
3. Outer `finally` always cleared `busy`, including when a newer selection owned it.
4. Disabled UI (`busy`) is not an authority boundary for concurrent open vs select.

### Fix

- `openProject` captures `generation = ++projectActivationGenRef.current`
  **before** `pickDirectory()` / any async gateway call.
- That generation is passed into `activateProject(project, generation)` so
  activate does **not** allocate a newer generation after open/register returns.
- Only the current generation may:
  - call final `gateway.selectProject` for the opened project
  - upsert / set active project / clear project error
  - set project error from open/register failure
  - clear busy owned by this open activation
- Stale open success returns before final select (no late `selectProject(opened.id)`).
- Stale open failure returns silently (no error banner).
- Current picker cancellation (`!path`) still clears its own busy when current
  and preserves the active project.
- Direct-selection guards and conversation/resource scope protections preserved.
- No gateway contract changes; no global store.

### Changed files (this fix only)

| Path | Change |
|------|--------|
| `apps/desktop/src/App.tsx` | Early open generation; pass-through to activate; guarded catch/finally |
| `apps/desktop/tests/project-workbench.test.tsx` | Delayed open success + delayed open failure vs later B |
| `apps/desktop/tests/resource-workbench-integration.test.tsx` | Same races + B-scoped list/recents/preview only |
| `docs/execution/2026-07-31-wave-c2d-resource-workbench-integration-report.md` | This section |

### RED evidence (pre-fix, open race tests only)

Command:

```powershell
pnpm --filter @pillow/desktop exec vitest run tests/project-workbench.test.tsx tests/resource-workbench-integration.test.tsx -t "delayed open|last-request-wins open"
```

| Suite case | Exit | Exact failure |
|------------|------|---------------|
| project-workbench: delayed open success cannot overwrite later B | 1 | `expect(element).toHaveTextContent()` Expected `Project B` Received `stale-opened` at `project-workbench.test.tsx:430` |
| project-workbench: delayed open failure cannot error banner over later B | 1 | `expect(...).not.toBeInTheDocument()` found `<span>STALE-OPEN-REGISTER-FAILED</span>` at `project-workbench.test.tsx:503` |
| resource-integration: delayed open success cannot remount under opened | 1 | `expect(element).toHaveTextContent()` Expected `Project B` Received `stale-opened` at `resource-workbench-integration.test.tsx:862` |
| resource-integration: delayed open failure cannot error banner or poison B | 1 | `expect(...).not.toBeInTheDocument()` found `<span>STALE-OPEN-REGISTER-FAILED</span>` at `resource-workbench-integration.test.tsx:973` |

**Summary**: 4 failed / 24 skipped. Proves late open success rewrites active project
to `stale-opened` and late open failure paints a stale project error banner over B.

### GREEN evidence (post-fix)

| Command | Exit | Outcome |
|---------|------|---------|
| `pnpm --filter @pillow/desktop exec vitest run tests/project-workbench.test.tsx tests/resource-workbench-integration.test.tsx` | 0 | **28** passed (13 project + 15 integration) |
| `pnpm --filter @pillow/desktop exec vitest run tests/resource-pane.test.tsx tests/resource-preview.test.tsx tests/resource-gateway.test.ts tests/conversation-workbench.test.tsx` | 0 | **70** passed (10 + 18 + 21 + 21) |
| `pnpm --filter @pillow/desktop typecheck` | 0 | Clean |
| `pnpm --filter @pillow/desktop build:ui` | 0 | Vite production build OK |
| `git diff --check` | 0 | No whitespace errors |

### Final counts (after this fix)

| Suite | Passed |
|-------|--------|
| `resource-workbench-integration.test.tsx` | **15** (+2 open last-request-wins) |
| `resource-pane.test.tsx` | 10 |
| `resource-preview.test.tsx` | 18 |
| `resource-gateway.test.ts` | 21 |
| `project-workbench.test.tsx` | **13** (+2 open last-request-wins) |
| `conversation-workbench.test.tsx` | 21 |

### Regression coverage

1. **Delayed open success**: batch Open Project + select B; picker returns path;
   open/register stays deferred; resolve B → B active; resolve open → no
   `selectProject(opened.id)`, B remains, no error; resource list/recents/preview
   B-scoped only.
2. **Delayed open failure**: same race; reject open/register with
   `STALE-OPEN-REGISTER-FAILED` → B remains; no error banner; B resources usable.
3. Existing open/register success, selection last-request-wins, and resource
   integration tests remain green.

### Staging / residual (this fix)

- **Staging count**: `0` (nothing staged; no commit)
- **Residual processes**: 0 (vitest / typecheck / build exited)
- **Residual risk**:
  1. Controller must independently re-inspect the open path and re-run all
     required gates before any C2-D acceptance.
  2. `openOrRegisterProject` may still execute after a newer selection has
     started (only final select + state commits are generation-guarded). That
     matches “stale success returns silently” without blocking registration
     side effects already in flight.
  3. Conversation-scope `projectScopeGenRef` remains separate from
     `projectActivationGenRef`.

---

## Final controller acceptance

**Verdict**: `ACCEPT C2-D`
**Controller**: Codex Desktop
**Date**: 2026-07-31
**Stage/commit authority**: none; all work remains unstaged and uncommitted

The controller personally inspected the final `App` activation flow and the
public-behavior race tests. Acceptance covers:

- resource browser and bounded preview mounted in the existing contextual pane;
- portable conversation paths do not resurrect previews after close/reopen;
- direct project selection is last-request-wins for success, failure, error,
  active project, and busy ownership;
- an older picker cannot register a project after a newer selection wins;
- an already in-flight registration may finish at the gateway boundary, but
  its late success/failure cannot select, activate, surface errors, clear newer
  busy state, or remount the resource scope;
- tree, search, recents, and preview calls remain scoped to the winning
  `projectId`.

### Controller-run evidence

| Gate | Result |
|------|--------|
| `vitest ... -t "last-request-wins\|stale picker"` | **10/10 passed** |
| Six C2-D UI suites | **100/100 passed** |
| `pnpm --filter @pillow/desktop typecheck` | Passed |
| `pnpm --filter @pillow/desktop build:ui` | Passed; 65 modules transformed |
| `git diff --check` | Passed |

### Independent final review

Codex2 final re-review: **clean** — Critical 0, Important 0, Minor 0. It
independently reran the 10 adversarial cases, typecheck, diff check, and cached
diff check. Staging remained empty.

### Residual boundary

An `openOrRegisterProject` request that was already dispatched before a newer
selection cannot be cancelled at the current gateway contract. Its durable
registration side effect may complete, but the renderer discards all late
state effects. Cancellation of an already-dispatched registration is outside
C2-D and does not weaken active-project or resource isolation.

---

## Controller acceptance fix (2026-07-31) — stale picker registration

**Controller session**: `<keeper-session>`
**Worker session**: `<keeper-session>`
**Source**: Codex2 final review; controller confirmed Important
**Finding**: Important — `openProject()` captured generation before
`pickDirectory()`, but after a delayed non-empty picker result it called
`openOrRegisterProject(path)` without proving the generation was still current.
Preventing only the final activation was insufficient because registration is
itself a durable metadata mutation.
**Status**: Fix landed in whitelist; gates re-run green. **Worker completion is
not acceptance** — controller must independently revalidate.

### Root cause

```ts
const path = await gateway.pickDirectory();
if (!path) return;
// BUG: no generation check here
const project = await gateway.openOrRegisterProject(path);
if (generation !== projectActivationGenRef.current) return;
```

An older Open Project could keep the picker pending while a newer project B
selection won. When the old picker later returned a valid path, registration
still ran under B’s ownership.

### Fix

Immediately after a non-empty picker result and **before**
`openOrRegisterProject(path)`, return silently if the captured generation is no
longer current. Preserve the separate post-open/register generation check for
requests that become stale while registration is already in flight. Preserve
picker cancellation, error ownership, busy ownership, direct-selection guards,
conversation clearing, and resource-scope protections.

```ts
const path = await gateway.pickDirectory();
if (!path) return;
if (generation !== projectActivationGenRef.current) return;
const project = await gateway.openOrRegisterProject(path);
if (generation !== projectActivationGenRef.current) return;
await activateProject(project, generation);
```

### Changed files (this fix only)

| Path | Change |
|------|--------|
| `apps/desktop/src/App.tsx` | Pre-register generation guard after non-empty pick |
| `apps/desktop/tests/project-workbench.test.tsx` | Deferred-picker RED/GREEN case; in-flight open races re-established via default-select after register starts |
| `apps/desktop/tests/resource-workbench-integration.test.tsx` | Same stale-picker case + B-scoped list/recents/preview; in-flight open races preserved |
| `docs/execution/2026-07-31-wave-c2d-resource-workbench-integration-report.md` | This section |

### RED evidence (pre-fix, stale picker tests only)

Command:

```powershell
pnpm --filter @pillow/desktop exec vitest run tests/project-workbench.test.tsx tests/resource-workbench-integration.test.tsx -t "stale picker"
```

| Suite case | Exit | Exact failure |
|------------|------|---------------|
| project-workbench: delayed non-empty pickDirectory must not call openOrRegister after B wins | 1 | `expected "spy" to not be called at all, but actually been called 1 times` with `["F:\\work\\stale-opened"]` at `project-workbench.test.tsx:577` |
| resource-integration: delayed non-empty pickDirectory must not register or poison B resources | 1 | `expected "spy" to not be called at all, but actually been called 1 times` with `["F:\\secret\\projects\\stale-opened"]` at `resource-workbench-integration.test.tsx:1062` |

**Summary**: 2 failed / 28 skipped. Proves a delayed non-empty picker still
invoked `openOrRegisterProject` after project B was already active.

### GREEN evidence (post-fix)

| Command | Exit | Outcome |
|---------|------|---------|
| `pnpm --filter @pillow/desktop exec vitest run tests/project-workbench.test.tsx tests/resource-workbench-integration.test.tsx` | 0 | **30** passed (14 project + 16 integration) |
| `pnpm --filter @pillow/desktop exec vitest run tests/resource-pane.test.tsx tests/resource-preview.test.tsx tests/resource-gateway.test.ts tests/conversation-workbench.test.tsx` | 0 | **70** passed (10 + 18 + 21 + 21) |
| `pnpm --filter @pillow/desktop typecheck` | 0 | Clean |
| `pnpm --filter @pillow/desktop build:ui` | 0 | Vite production build OK |
| `git diff --check` | 0 | No whitespace errors (CRLF warnings only on other dirty paths) |

### Final counts (after this fix)

| Suite | Passed |
|-------|--------|
| `resource-workbench-integration.test.tsx` | **16** (+1 stale picker registration) |
| `resource-pane.test.tsx` | 10 |
| `resource-preview.test.tsx` | 18 |
| `resource-gateway.test.ts` | 21 |
| `project-workbench.test.tsx` | **14** (+1 stale picker registration) |
| `conversation-workbench.test.tsx` | 21 |

### Regression coverage

1. **Stale picker (new)**: deferred `pickDirectory`; start Open + select B in one
   batch; resolve B → B active; resolve older picker with valid path →
   `openOrRegisterProject` never called; B remains; no error; busy ownership
   restored so Open Project / B / resource toggle stay usable; resource suite
   proves list/recents/preview are B-scoped only.
2. **In-flight open success/failure (preserved)**: Open starts with immediate
   pick so `openOrRegister` is already pending; concurrent B via public
   `defaultSelectedProjectId` harness path; resolve B then late
   resolve/reject register → B remains, no stale activation/error, B resources
   usable.
3. All eight existing direct-selection and in-flight-open race tests remain
   green (four selection + four open across both suites).

### Staging / residual (this fix)

- **Staging count**: `0` (nothing staged; no commit)
- **Residual processes**: 0 (vitest / typecheck / build exited)
- **Residual risk**:
  1. Controller must independently re-run the stale-picker and in-flight-open
     tests plus full gates before any C2-D acceptance.
  2. Busy-disabled project rows still block ordinary UI clicks while open is
     pending; concurrent superseding after register starts is proven via the
     public default-select path (and still via batched pre-busy clicks for
     selection races).
  3. Conversation-scope `projectScopeGenRef` remains separate from
     `projectActivationGenRef`.
