# Wave C2-C2 — In-Conversation Agent Activity Disclosure — Completion Report

**Date**: 2026-07-31
**Status**: `DONE`
**Worker**: grok (Grok CLI, controller)
**Scope**: Isolated renderer slice only; workbench / timeline wiring deferred.
**Commit**: not staged, not committed (per brief).

---

## Changed files (whitelist only)

| Path | Role |
|------|------|
| `apps/desktop/src/features/conversations/AgentActivityDisclosure.tsx` | **New** — progressive disclosure UI for conversation-owned activity |
| `apps/desktop/src/features/conversations/agentActivityProjection.ts` | **New** — pure projection: ownership filter, summary, portable paths, opaque errors |
| `apps/desktop/src/features/conversations/agentActivityTypes.ts` | **New** — input record / props / projected view types |
| `apps/desktop/src/styles/agent-activity.css` | **New** — `agent-activity-*` styles (no collision with `run-activity`) |
| `apps/desktop/tests/agent-activity-disclosure.test.tsx` | **New** — focused TDD suite (9 cases) |
| `docs/execution/2026-07-31-wave-c2c2-agent-activity-report.md` | This report |

**Not modified**: `ConversationTimeline.tsx`, `RunActivity.tsx`, App/workbench wiring, existing CSS, Tauri, provider/runtime, C2-A/B code, package files.

---

## Contract delivered

`AgentActivityDisclosure` props:

- `conversationId: string`
- `activity: AgentActivityRecord | readonly AgentActivityRecord[] | null | undefined`
- `onOpenArtifact?: (relativePath: string) => void`

Behavior:

1. **Ownership filter** — only records whose `conversationId` matches the prop are projected; foreign / empty / null inputs render nothing (no invented tools or sub-agents).
2. **Compact summary** — human-readable `role · status · current step`; raw `sessionId` / `runId` never enter the DOM.
3. **Closed by default** — outer `<details>` starts closed; nested output / artifact / test sections also start closed.
4. **Ordered actual work** — expanded body shows plan steps, then tools, then sub-agents in input order; entries only from supplied arrays.
5. **Terminal accessibility** — failed / cancelled remain visible on the summary line and expand to opaque error detail; successful completed history is a single concise disclosure (no permanent card stack, no `agent-activity-card` chrome).
6. **Portable artifacts only** — non-portable paths (absolute, Windows roots, `..`, backslashes) are dropped; `onOpenArtifact` receives portable relative paths only.
7. **Opaque errors** — path-bearing host messages become `无法完成该操作。`; machine paths never surface.

---

## TDD evidence

### RED (implementation absent)

| Command | Exit | Outcome |
|---------|------|---------|
| `pnpm --filter @pillow/desktop exec vitest run tests/agent-activity-disclosure.test.tsx` | 1 | `Failed to resolve import "../src/features/conversations/AgentActivityDisclosure"` — suite failed before any test body |

Test file was authored **before** production modules.

### GREEN (after implementation)

| Command | Exit | Outcome |
|---------|------|---------|
| `pnpm --filter @pillow/desktop exec vitest run tests/agent-activity-disclosure.test.tsx` | 0 | **9** tests passed |
| `pnpm --filter @pillow/desktop typecheck` | 0 | Clean |
| `git diff --check` (scoped whitelist paths) | 0 | No whitespace errors |

### Final verification

```
pnpm --filter @pillow/desktop exec vitest run tests/agent-activity-disclosure.test.tsx
  → 9 passed

pnpm --filter @pillow/desktop typecheck
  → pass

git diff --check -- apps/desktop/src/features/conversations/AgentActivityDisclosure.tsx \
  apps/desktop/src/features/conversations/agentActivityProjection.ts \
  apps/desktop/src/features/conversations/agentActivityTypes.ts \
  apps/desktop/src/styles/agent-activity.css \
  apps/desktop/tests/agent-activity-disclosure.test.tsx
  → pass
```

### Behavior covered by tests

- Ownership filtering (foreign conversation activity omitted)
- Missing / empty / foreign-only inputs → no invented work
- Compact summary without raw session/run IDs
- Closed-default disclosure; expand shows ordered steps / tools / sub-agents
- Tool and sub-agent entries only from supplied records
- Terminal failure and cancelled remain visible and expandable
- Successful history is one concise closed disclosure (no permanent cards)
- Artifact callback portable-relative only; absolute / escape paths dropped
- Opaque errors with no machine path leakage

---

## Provenance

| Source | License | Use |
|--------|---------|-----|
| Local `zbbody` (`zbbody reference clone (out of tree)`, Apache-2.0) | Apache-2.0 | **Conceptual only** — conversation-local progressive disclosure of plan / tool / sub-agent activity; no source files copied |
| Grok App commit `cbbbdf17bb8db1588b6707f3f42f6b00397e27f9` | MIT | **Interaction baseline only** — compact summary + progressive disclosure pattern; design tokens reused via existing Pillow `tokens.css` (already attributed). **No direct source adaptation** of Grok App components for this slice |

This slice is Pillow-authored TypeScript/CSS. Existing `RunActivity` remains the wired run projection; this component is a parallel, unmounted contract for conversation-scoped disclosure until the controller wires it.

---

## Residual risks / known limitations

1. **Not wired into the workbench or timeline** — controller must pass conversation-owned activity and optional artifact open callback after preview/wiring waves.
2. **CSS self-imported** — `AgentActivityDisclosure` imports `agent-activity.css`; avoid double-import if later added to the shell stylesheet graph.
3. **Does not replace `RunActivity`** — both may coexist until a later consolidation; this slice does not edit `RunActivity.tsx`.
4. **No polling** — activity is fully parent-supplied; no global background-task table or session registry.
5. **Multi-record rendering** — each owned record becomes one closed disclosure; successful records stay concise but are not auto-merged across runs.

---

## Staging / commit

- **Not staged**
- **Not committed**
- Whitelist-only intentional edits; pre-existing dirty tree left as found
