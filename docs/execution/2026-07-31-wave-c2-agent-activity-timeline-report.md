# Wave C2 Task 3 — Real Conversation Agent Activity Timeline — Completion Report

**Date**: 2026-07-31
**Status**: `DONE`
**Worker**: grok-c2-activity-timeline (Grok Build)
**Controller session (handoff)**: `<keeper-session>`
**Worker bridge session**: `<keeper-session>`
**Dependent-tests worker**: grok-wave-c2-tests (session `<keeper-session>`)
**Baseline HEAD**: `7b7f70f7360131cd1cfd984a8b19bdfb5725c21b`
**Commit policy**: not staged, not committed (per brief)

---

## Status

`DONE`

Task 3 implementation and the dependent-test migration are complete under their
respective exclusive write whitelists. The three formerly failing suites now
query the public Agent activity disclosure surface; focused and dependent gates
are green.

---

## Exact changed files (whitelist only)

| Path | Role |
|------|------|
| `apps/desktop/src/features/conversations/agentActivityFromTimeline.ts` | **New** — pure `agentActivityFromRun` + `resolveResourceFromActivity` |
| `apps/desktop/src/features/conversations/ConversationTimeline.tsx` | **Modified** — mount `AgentActivityDisclosure` at former `RunActivity` site |
| `apps/desktop/src/features/conversations/AgentActivityDisclosure.tsx` | **Modified** — render run notices + per-tool output truncation |
| `apps/desktop/src/features/conversations/agentActivityTypes.ts` | **Modified** — notices, truncation fields, cancelling/interrupted, `recordKey` |
| `apps/desktop/src/features/conversations/agentActivityProjection.ts` | **Modified** — status labels, notices projection, truncation notices, terminal rules |
| `apps/desktop/tests/agent-activity-timeline-integration.test.tsx` | **New** — TDD integration suite (15 cases) |
| `apps/desktop/tests/conversation-workbench.test.tsx` | **Modified** — timeline mount expectations + isolation markers |
| `docs/execution/2026-07-31-wave-c2-agent-activity-timeline-report.md` | This report |

**Not modified** (read-only): `RunActivity.tsx`, reducers, App, Workbench,
resource code, Rust/Tauri, package files, vault/keeper.

`RunActivity.tsx` remains in-tree for focused legacy unit tests that render it
directly.

---

## RED / GREEN evidence

### RED (before production mapper/mount)

```text
pnpm --filter @pillow/desktop exec vitest run tests/agent-activity-timeline-integration.test.tsx
```

| Exit | Outcome |
|------|---------|
| 1 | `Failed to resolve import "../src/features/conversations/agentActivityFromTimeline"` — suite failed before any test body |

Exact failure (import analysis):

```text
Error: Failed to resolve import "../src/features/conversations/agentActivityFromTimeline"
from "tests/agent-activity-timeline-integration.test.tsx". Does the file exist?
```

### GREEN (after implementation)

| Command | Exit | Count / outcome |
|---------|------|-----------------|
| `vitest run tests/agent-activity-timeline-integration.test.tsx` | 0 | **15** passed |
| `vitest run tests/agent-activity-disclosure.test.tsx tests/agent-activity-timeline-integration.test.tsx tests/conversation-workbench.test.tsx` | 0 | **45** passed (9 + 15 + 21) |
| `pnpm --filter @pillow/desktop typecheck` | 0 | Clean |
| `pnpm --filter @pillow/desktop build:ui` | 0 | Vite production build — **68** modules |
| `git diff --check` | 0 | No whitespace errors |

C2-C2 nine focused disclosure tests remain green (compatible extension).

---

## Contract mapping table

| `RunActivityView` source | `AgentActivityRecord` / UI |
|--------------------------|----------------------------|
| owning `timeline.conversationId` | `record.conversationId` (ownership filter) |
| `runId` | internal `runId` key only — never DOM text/attrs |
| `status` (`pending`…`interrupted`) | real status + Chinese labels (`取消中` / `已中断` included) |
| active plan step (`status === "active"`) | `currentStep` (omitted if none) |
| `plan.steps[]` order | `steps[]` titles/statuses |
| `tools[]` order | `tools[]` title/name/status; completed/running → bounded `output`; failed/cancelled → `error` (opaque path) |
| `tools[].outputTruncated` / `omittedOutputChars` | per-tool `truncationNotice` (“输出已截断，省略 N 个字符”) |
| `subagents[]` order | title/status/progress only — **no invented `role`** |
| tool `resources[]` then `artifacts[]` | portable `artifacts[]` only (drop abs / `\` / URL / `..` / empty segment) |
| assistant / reasoning truncation + omission counters | `notices[]` summaries (reasoning **text never shown**) |
| structured tests | **empty** `testResults` — never inferred from tool names |
| portable artifact open | disclosure `onOpenArtifact(path)` → `resolveResourceFromActivity` → existing `onOpenResource(ResourceReferenceView)` |

---

## Timeline mount contract

- Chronological item construction, timestamps, pagination, visible-range
  freeze, and follow-bottom behavior are unchanged.
- Each activity item builds **exactly one** `AgentActivityRecord` via
  `agentActivityFromRun(timeline.conversationId, activity)` and renders
  **only** `AgentActivityDisclosure` (not both components).
- Public `ConversationTimeline.onOpenResource` preserved; portable paths only.
- Path-only artifacts without embedded resources get a **minimal**
  `ResourceReferenceView` (path + leaf label, no fabricated content).
- Foreign records remain filtered by disclosure ownership; remount uses
  `key={timeline.conversationId}`.

---

## Provenance

| Source | License | Use |
|--------|---------|-----|
| Pillow C2-C2 `AgentActivityDisclosure` slice | Project-owned | Extended for notices / truncation / timeline mount |
| Local `zbbody` (Apache-2.0) | Apache-2.0 | Conceptual progressive disclosure only — no source copy |
| Grok App `cbbbdf17` | MIT | Interaction baseline only (compact summary + progressive disclosure); no direct component adaptation |
| Legacy `RunActivity.tsx` | Project-owned | Read-only reference for truncation/omission wording and status labels; **not** mounted from the timeline |

No upstream file was copied wholesale into the whitelist.

---

## Self-review findings

1. **Stable internal keys** use source IDs as React keys only; no
   `data-run-id` / tool-call / resource id attributes on the activity surface.
2. **Reasoning body** is intentionally absent; only truncation counts appear.
3. **Status copy** for completed runs is disclosure’s `已完成` (not legacy
   RunActivity `思考完成`) — workbench chronological wait updated accordingly.
4. **Dependent-test migration (follow-up whitelist)** closed the three suites
   that still selected `run-activity` — see section below.
5. Foreign-ownership integration case primarily asserts the pure mapper +
   timeline pairing; deep foreign-record injection remains covered by C2-C2
   disclosure suite.

---

## Dependent tests migration (2026-07-31 follow-up)

**Whitelist (tests + this report only; no production edits):**

| Path | Change |
|------|--------|
| `apps/desktop/tests/conversation-gateway-stream.test.tsx` | Replace `run-activity` / plan / tool / sub-agent / `run-status` selectors with `agent-activity-*`; expand outer + nested tool output before asserting streamed content; completed/cancelled use `已完成` / `已取消` |
| `apps/desktop/tests/conversation-fix-pass2.test.tsx` | Completed wait and cancelled-run assertions use `agent-activity-summary` / `agent-activity-disclosure`; keep no-running-child checks |
| `apps/desktop/tests/resource-workbench-integration.test.tsx` | Open outer `agent-activity-details` and nested artifacts disclosure (`HTMLDetailsElement.open`) before “打开 Result report”; preserve close/reopen resurrection and gateway-omission fallback |
| `docs/execution/2026-07-31-wave-c2-agent-activity-timeline-report.md` | This append |

**Behavioral purpose preserved:**

- Streamed plan / tool / sub-agent data visible only after progressive disclosure
- Completed vs cancelled not confused
- Conversation artifact callbacks still open accepted resource pane
- Close/reopen preview resurrection protections still asserted
- `resourceGateway` omission still preserves conversation-reference fallback

**No production compatibility IDs added. No snapshot rewrites.**

### GREEN evidence (dependent migration)

| Command | Exit | Outcome |
|---------|------|---------|
| `vitest run tests/conversation-gateway-stream.test.tsx tests/conversation-fix-pass2.test.tsx tests/resource-workbench-integration.test.tsx` | 0 | **32** passed (8 + 8 + 16) |
| `vitest run tests/agent-activity-disclosure.test.tsx tests/agent-activity-timeline-integration.test.tsx tests/conversation-workbench.test.tsx` | 0 | **45** passed (9 + 15 + 21) |
| `pnpm --filter @pillow/desktop typecheck` | 0 | Clean (after casting details to `HTMLDetailsElement`) |
| `git diff --check` | 0 | No whitespace errors |

### Staging / process cleanup (dependent migration)

| Check | Value |
|-------|-------|
| Staging count | **0** |
| Commit | none |
| Vitest process residual from this worker | none (commands exited cleanly) |

---

## Staging / residual process

| Check | Value |
|-------|-------|
| Staging count | **0** |
| Residual Pillow/Cargo/Rust/Node-vitest/Playwright worker processes | **0** (spot check of this worker’s test runs) |
| Commit | none |

---

## Residual risks

1. Minimal path-only open references use `resourceId: "path:<relative>"` for
   the existing resource pane type contract; the activity DOM never surfaces
   that string.
2. `AgentActivityDisclosure` remains CSS self-imported; shell graph still does
   not need a second stylesheet registration for this mount.
3. Direct `RunActivity.tsx` unit mounts (if any remain elsewhere) still use
   legacy `run-activity` IDs by design — only the timeline mount site was
   replaced.
4. Controller must personally re-run focused + chosen regression gates before
   Codex2 review — worker completion is not acceptance.

---

## Completion message

Wave C2 Task 3 is **implementation-complete**, and the **dependent-test
migration is green**: the timeline mounts conversation-owned
`AgentActivityDisclosure` from real `RunActivityView` data; the three suites
that still selected `run-activity` now use the public disclosure surface with
progressive disclosure before nested assertions. Gates: dependent **32/32**,
focused disclosure/timeline/workbench **45/45**, typecheck, `git diff --check`.
Staging **0**, no commit. Worker completion is not controller acceptance.

**DONE**

---

## Controller final acceptance — 2026-07-31

**Verdict**: `ACCEPTED`

The controller personally inspected the mapper, projection, disclosure,
timeline mount, exact resource resolver, and all new/updated tests. The final
Codex2 review returned:

- Spec Compliance: compliant
- Code Quality: approved
- Critical: 0
- Important: 0

The controller then ran fresh acceptance gates:

| Command | Exit | Outcome |
|---------|------|---------|
| six activity/resource suites | 0 | **85/85** passed |
| complete desktop Vitest | 0 | **204/204** passed across 17 files |
| `pnpm --filter @pillow/desktop typecheck` | 0 | Clean |
| `pnpm --filter @pillow/desktop build:ui` | 0 | Production build, **68 modules** |
| `git diff --check` | 0 | Clean |
| `git diff --cached --name-only` | 0 | **Staging count 0** |

Expected negative-path stderr remained limited to tests that deliberately
inject invalid runtime events, timeline refresh failures, or network failure.
The earlier duplicate React-key warning is gone.

Final state:

- HEAD remains `7b7f70f7360131cd1cfd984a8b19bdfb5725c21b`.
- No files are staged and no commit was created.
- Residual Pillow/Grok/Node/Cargo/Rust/Playwright task processes: **0**.
- Keeper Grok active instances: **0**; all leases from this task were released.
- `RunActivity.tsx` remains read-only in the tree but is no longer mounted by
  the real conversation timeline.

Task 3 is accepted. The next authorized unit is Task 4: two-project
isolation/restart desktop acceptance. This acceptance does not authorize
staging or committing.

---

## Review-fix pass (Codex2 Important × 6) — 2026-07-31

**Status**: `DONE`
**Worker**: grok-c2-fix (session `<keeper-session>`)
**Controller session (handoff)**: `<keeper-session>`
**Commit policy**: not staged, not committed

### Exact changes (whitelist only)

| Path | Change |
|------|--------|
| `agentActivityTypes.ts` | Optional `sourceToken` on artifact input/view; `onOpenArtifact(path, sourceToken?)` |
| `agentActivityProjection.ts` | Stable `recordKey` (`identity::index`, no status); sourceToken on views; defensive ID sanitization of labels/titles/progress/notices |
| `agentActivityFromTimeline.ts` | Source tokens for tool-resource / run-artifact; no path-only dedupe; path-mismatched embedded resource → leaf label + minimal open ref; token-first resolve |
| `AgentActivityDisclosure.tsx` | `key={projection.recordKey}`; pass sourceToken only via in-memory click callback |
| `ConversationTimeline.tsx` | Forward `sourceToken` into `resolveResourceFromActivity` |
| `agent-activity-timeline-integration.test.tsx` | Regressions for open-state stability, exact identity, real ownership, DOM chronology, adversarial IDs (20 cases) |
| This report | Append |

### Fix mapping

1. **Stable disclosure identity** — list items use `projection.recordKey` only (React-internal). Rerender same run with new status/current step keeps `<details open>`.
2. **Exact artifact/resource identity** — all portable tool resources then run artifacts preserved; non-DOM `sourceToken`; resolve token-first with path-only fallback; mismatched `artifact.resource.path` never borrows label/content.
3. **Ownership / chronology** — disclosure-level owned+foreign assert; expand both activities and assert early < mid message < late DOM order.
4. **Defensive ID non-disclosure** — strip exact/whole-token echoes of associated run/step/tool/sub-agent/artifact IDs from display fields; portable path to open callback unmutated.

### GREEN gates (review-fix)

| Command | Exit | Count / outcome |
|---------|------|-----------------|
| `vitest run tests/agent-activity-timeline-integration.test.tsx` | 0 | **20** passed |
| `vitest run tests/agent-activity-disclosure.test.tsx tests/agent-activity-timeline-integration.test.tsx tests/conversation-workbench.test.tsx` | 0 | **50** passed (9 + 20 + 21) |
| `vitest run tests/conversation-gateway-stream.test.tsx tests/conversation-fix-pass2.test.tsx tests/resource-workbench-integration.test.tsx` | 0 | **32** passed (8 + 8 + 16) |
| `pnpm --filter @pillow/desktop typecheck` | 0 | Clean |
| `pnpm --filter @pillow/desktop build:ui` | 0 | Vite production — **68** modules |
| `git diff --check` | 0 | No whitespace errors |
| `git diff --cached --name-only` | 0 | **Staging count 0** |

### Residual risks

1. Path-only open (legacy one-arg / missing token) remains first-match ambiguous when multiple sources share a portable path; token path is authoritative for new UI opens.
2. `recordKey` includes filtered list index; reordering multi-record disclosures without stable run/session identity can remount a row (single-run timeline mount is unaffected).
3. Sanitization only redacts IDs present on the same record; unrelated ordinary user text is preserved.
4. Worker completion is not controller acceptance; controller must re-run gates and re-submit Codex2 review.

### Completion message (review-fix)

Wave C2 Task 3 **Codex2 review-fix complete** under the exclusive whitelist: stable `recordKey` disclosure identity, exact artifact source tokens, real ownership/chronology assertions, and defensive ID non-disclosure. Gates: timeline **20/20**, focused **50/50**, dependent **32/32**, typecheck, `build:ui` (68 modules), `git diff --check`. Staging **0**, no commit. Worker completion is not controller acceptance.

**DONE**

---

## Review-fix 2 pass (Codex2 Important × 2 remaining) — 2026-07-31

**Status**: `DONE`
**Worker**: grok-worker (session `<keeper-session>`)
**Controller session (handoff)**: `<keeper-session>`
**Commit policy**: not staged, not committed

### Confirmed remaining Important findings (closed)

1. `recordKey = identity::index` changed under array reorder / earlier insert.
2. `artifact.resource.resourceId` and `tool.receipt.receiptId` discarded before
   projection, so labels echoing those IDs could not be sanitized.

### Exact changes (whitelist only)

| Path | Change |
|------|--------|
| `agentActivityTypes.ts` | Optional internal `recordIdentity`; optional internal `forbiddenDisplayIds` carrier (never DOM) |
| `agentActivityFromTimeline.ts` | Populate `recordIdentity` from real `runId`; collect full forbidden-ID set (run, plan-step, tool-call, receipt, tool resource, sub-agent, run artifact, embedded artifact resource) |
| `agentActivityProjection.ts` | Prefer `recordIdentity` → `runId` → `sessionId` for `recordKey` (no index when stable); legacy `legacy-record::index` only without identity; merge carrier into forbidden set; sanitize path-leaf fallback label → neutral `产物` chrome |
| `AgentActivityDisclosure.tsx` | No code change required (already `key={projection.recordKey}`) |
| `agent-activity-timeline-integration.test.tsx` | Reorder open-state regression; receipt-ID label sanitization; embedded-resource-ID label sanitization (artifact ID differs); mapper carrier assertions (23 cases) |
| This report | Append |

### Fix mapping

1. **Stable record identity under array reorder** — real mapper sets
   `recordIdentity = runId`. Projection uses that (then run/session) as
   React-only `recordKey` without appending list index. Array index is only
   the legacy fallback. Identity never enters text, attributes, titles, or
   test IDs. Regression: two distinct runs, open Beta, reverse array, Beta
   stays open.
2. **Complete internal-ID sanitization carrier** — mapper fills
   `forbiddenDisplayIds` with every actual run-side ID including discarded
   receipt and embedded resource IDs. Projection merges the carrier and never
   renders it. Path-leaf display labels are sanitized; if the leaf itself
   echoes a forbidden ID, neutral `产物` chrome is used while the portable
   callback path stays exact.

### GREEN gates (review-fix 2)

| Command | Exit | Count / outcome |
|---------|------|-----------------|
| `vitest run tests/agent-activity-timeline-integration.test.tsx` | 0 | **23** passed |
| `vitest run tests/agent-activity-disclosure.test.tsx tests/agent-activity-timeline-integration.test.tsx tests/conversation-workbench.test.tsx` | 0 | **53** passed (9 + 23 + 21) |
| `pnpm --filter @pillow/desktop typecheck` | 0 | Clean |
| `git diff --check` | 0 | No whitespace errors |
| `git diff --cached --name-only` | 0 | **Staging count 0** |

### Residual risks

1. Synthetic multi-record fixtures that reuse the same `runId` (e.g. C2-C2
   disclosure terminal-status matrix) produce a React key warning because
   stable identity no longer suffixes list index. Real timeline runs are
   unique per `runId` and are unaffected.
2. Path-only open (legacy one-arg / missing token) remains first-match
   ambiguous when multiple sources share a portable path; token path is
   authoritative for new UI opens.
3. Receipt *resources* are still not projected as separate artifact rows
   (only their IDs enter the sanitizer carrier). Tool resources beside a
   receipt remain the open surface for portable receipt-adjacent paths.
4. Worker completion is not controller acceptance; controller must re-run
   gates and re-submit Codex2 review.

### Completion message (review-fix 2)

Wave C2 Task 3 **Codex2 review-fix 2 complete** under the exclusive
whitelist: stable `recordIdentity`/`recordKey` under activity-array reorder,
and complete internal-ID sanitization via `forbiddenDisplayIds` (receipt +
embedded resource IDs included) with path-leaf fallback hardening. Gates:
timeline **23/23**, focused **53/53**, typecheck, `git diff --check`.
Staging **0**, no commit. Worker completion is not controller acceptance.

**DONE**

---

## Duplicate-key test fixture cleanup — 2026-07-31

**Status**: `DONE` (worker)
**Worker**: grok-wave-c2-t3 (session `<keeper-session>`)
**Controller session (handoff)**: `<keeper-session>`
**Commit policy**: not staged, not committed

### Confirmed problem

Production review approved (no Critical / Important). One legacy multi-record
disclosure test called `baseRecord` twice without overriding the shared
default `runId` (`run-raw-id-do-not-show`). After stable `recordKey` stopped
suffixing list index, React correctly emitted a duplicate-key warning while
assertions still passed. Real timeline records remain unique via
`activitiesByRunId`.

### Exact change (whitelist only)

| Path | Change |
|------|--------|
| `apps/desktop/tests/agent-activity-disclosure.test.tsx` | In `keeps terminal failure and cancelled states visible and accessible`, assign distinct stable `runId` values: `run-terminal-failed` and `run-terminal-cancelled` |
| `docs/execution/2026-07-31-wave-c2-agent-activity-timeline-report.md` | This append |

**Not changed:** product behavior, assertions, visible copy, snapshot
expectations, `console.error` handling, or any production file.

### Residual risk closed

Review-fix 2 residual risk #1 (synthetic multi-record fixtures reusing the
same `runId` → React key warning) is closed by this fixture-only fix.

### GREEN gates

| Command | Exit | Count / outcome |
|---------|------|-----------------|
| `vitest run tests/agent-activity-disclosure.test.tsx` | 0 | **9** passed |
| `vitest run tests/agent-activity-disclosure.test.tsx tests/agent-activity-timeline-integration.test.tsx tests/conversation-workbench.test.tsx` | 0 | **53** passed (9 + 23 + 21) |
| `git diff --check` | 0 | No whitespace errors |
| `git diff --cached --name-only` | 0 | **Staging count 0** |

### Staging / process

| Check | Value |
|-------|-------|
| Staging count | **0** |
| Commit | none |
| Production files touched | none |

### Completion message

Wave C2 Task 3 **duplicate-key test fixture cleanup complete** under the
exclusive whitelist: the multi-record terminal failure/cancelled disclosure
fixture now uses distinct stable `runId` identities so React list keys no
longer collide. Product behavior, assertions, and visible copy unchanged.
Gates: disclosure **9/9**, focused trio **53/53**, `git diff --check`.
Staging **0**, no commit. Worker completion is not controller acceptance.

**DONE**

## Controller acceptance — final record

Task 3 is accepted and closed. The owning conversation renders governed,
progressively disclosed Agent activity from real run/event/tool/sub-agent data,
with exact resource targeting and stable non-DOM identity handling.

Final controller evidence after all review fixes:

- focused timeline integration: **23/23**;
- six related desktop suites: **85/85**;
- full desktop Vitest: **204/204**;
- desktop typecheck: passed;
- production UI build: passed (**68 modules**);
- `git diff --check`: passed (line-ending warnings only);
- staging count: **0**;
- HEAD: `7b7f70f7360131cd1cfd984a8b19bdfb5725c21b`;
- residual Pillow/Grok/Node/Cargo/Rust/Playwright task processes: **0**;
- Codex2 independent final review: **Critical 0, Important 0**.

No files were staged or committed. The next controlled unit is C2 Task 4:
project-switch isolation, restart recovery, and desktop acceptance.
