# Wave C3 — zbbody Collaboration & Editor Interaction Research

**Date:** 2026-08-01
**Role:** read-only collaboration / editor interaction researcher (`grok2`)
**Reference checkout:** `zbbody reference clone (out of tree)`
**Verified HEAD:** `00c293414333b6fcc4e4598389fa3952c8d0b09b` (`fix(conversation): avoid forbidden extra updates`)
**Worktree note:** independently dirty (`M AGENTS.md`, `M CLAUDE.md`, `M packages/desktop/electron.vite.config.ts`, `?? .cursor/`). Not inspected as product source of truth; not modified.
**Pillow product order (context):** industrial local AI IDE first; UI follows accepted Grok App interaction baseline until a Pillow design system supersedes it.
**Prior conceptual reference:** `docs/research/2026-07-30-zbbody-reference-analysis.md`
**Pillow in-tree baselines used for contrast:** C2-C2 `AgentActivityDisclosure`, C2-C1 `ResourcePane`, C2-C3 project-scoped `ResourcePreview`.

Keeper Bridge boot (`local session bridge boot (optional)` and claim fallback) returned HTTP 400 in this session; work continued under the written brief with vault QUERY only.

---

## 1. License finding

| Fact | Evidence |
|------|----------|
| Declared license | **Apache License 2.0** |
| `package.json` | `"license": "Apache-2.0"`, product name `AionUi` / ZBBody packaging |
| Full license text | `zbbody reference clone (out of tree)\LICENSE` (Apache 2.0, standard terms) |
| Upstream provenance | `readme.md` L159: ZBBody continues [AionUi](https://github.com/iOfficeAI/AionUi) under Apache-2.0 |
| Per-file SPDX | Widespread `SPDX-License-Identifier: Apache-2.0` and `Copyright … AionUi (aionui.com)` headers (e.g. message/permission/tool components) |
| NOTICE file | No top-level `NOTICE` found in the checkout |

### Reuse posture for Pillow

Apache-2.0 **permits** use, modification, and distribution of source **if** license text, copyright notices, and (if present) NOTICE obligations are retained for any **copied** source.

Pillow rules require: **no wholesale copy**, typed contracts, focused tests, and provenance for any later code port. Combined with prior C2 decisions (“zbbody conceptual-only”) and the product boundary (Tauri/SQLite authority, no Electron string IPC, no provider-owned always-allow):

| Mode | Allowed now? |
|------|----------------|
| Interaction patterns, state machines, disclosure levels (design contracts) | **Yes — conceptual adopt** |
| Line-by-line UI/source port into Pillow | **No** without a later brief naming path, commit, NOTICE/attribution, adaptation boundary, and acceptance tests |
| Security/policy semantics (`proceed_always`, YOLO ids, workspace-as-root) | **Reject** as product boundary even if license allows the code |

**Finding:** Treat zbbody as **conceptual-only** for C3. License is clear and permissive; provenance and Pillow architecture — not license blockage — set the reuse ceiling.

---

## 2. Evidence table (paths / lines)

| Topic | Local path | Lines (approx.) | What it shows |
|-------|------------|-----------------|---------------|
| License header pattern | `packages/desktop/src/renderer/pages/conversation/Messages/acp/MessageAcpPermission.tsx` | 1–5 | Apache-2.0 SPDX on UI surfaces |
| Conversation context isolation | `…/hooks/context/ConversationContext.tsx` | 14–65, 77–102 | Provider binds `conversation_id`, optional `workspace`, type, skills/MCP snapshots |
| Message type union | `…/common/chat/chatLib.ts` | 444–475 | `text`, tools, permissions, plan, thinking, subagent lifecycle; `IConfirmation` |
| Message switch / disclosure surface | `…/Messages/MessageList.tsx` | 235–264, 314–413, 811–840 | Per-type render; tool/file summaries; turn process group; plan rail + inspector |
| Turn collapse (process group) | `…/MessageTurnProcessGroup/index.tsx` | 25–88 | Finished turn folds process steps; keyboard expand |
| Turn grouping rules | `…/MessageTurnProcessGroup/turnGrouping.ts` | 46–61 | Position-based turn boundaries; streaming tail stays live |
| Tool progressive disclosure | `…/Messages/components/MessageToolCall.tsx` | 20–116 | Status badge; collapsed by default; expand input/output; replace → diff panel |
| Tool group summary | `…/Messages/components/MessageToolGroupSummary.tsx` | 17–78, 96+ | Category breakdown (`Read ×n · Command ×n`); nested detail load |
| Normalize tool status | `…/common/chat/normalizeToolCall.ts` | 4–84 | `pending/running/completed/error/canceled`; edit/exec/info/mcp descriptions |
| Subagent card + disclosure | `…/Messages/components/MessageSubagentCard.tsx` | 52–165 | Always start collapsed; force-collapse on fail/cancel; activity only when open |
| Subagent live activity store | `…/Messages/subagent/subagentActivityStore.ts` | 7–76 | Transient stream **not** in message list; conversation-keyed; sessionStorage mirror |
| Plan inline vs floating | `…/Messages/components/MessagePlan.tsx` | 8–54 | Inline plan collapsed by default (avoids double checklist) |
| Plan progress rail / overlay | `…/Messages/MessageList.tsx` | 83–106, 467–567, 822–840 | Resident rail when wide; overlay when narrow; bootstrap plan while active |
| Session inspector KPIs | `…/Messages/components/SessionInspectorPanel.tsx` | 14–92 | Duration/steps/tools/model + touched files (dashboard risk) |
| Thinking disclosure | `…/Messages/components/MessageThinking.tsx` | 42–99 | Peek/collapsed default; auto-collapse on done; Enter/Space toggle |
| File changes in chat | `…/Messages/MessageFileChanges.tsx` | 23–90 | Merge diffs; open file vs open diff via preview launcher |
| File changes panel UI | `…/components/base/FileChangesPanel.tsx` | 16–61, 52–155 | `defaultExpanded = true`; +/− stats click → diff; preview button |
| Workspace snapshot changes | `…/Workspace/hooks/useFileChanges.ts` | 30–149 | Per-`workspace` init/compare/dispose; stage/unstage/discard/reset |
| Workspace change list + empty | `…/Workspace/components/FileChangeList.tsx` | 17–120 | Staged/unstaged; expandable diffs; Arco `Empty`/`Spin` |
| Diff viewer | `…/Preview/components/viewers/DiffViewer.tsx` | 20–119 | diff2html; side-by-side; source/preview modes |
| Preview dirty close confirm | `…/Preview/…/PreviewConfirmModals.tsx` | 15–80 | Save / discard / cancel on tab close |
| Preview keyboard | `…/Preview/hooks/usePreviewKeyboardShortcuts.ts` | 27–48 | Ctrl/Cmd+S save when dirty |
| Permission (generic) | `…/Messages/components/MessagePermission.tsx` | 28–123 | Radio options; `always_allow` when `proceed_always`; confirm IPC |
| Permission (ACP) | `…/Messages/acp/MessageAcpPermission.tsx` | 21–147 | Tool title/kind icons; `conversation.confirmMessage.invoke` |
| Pending confirmation recovery | `…/Messages/usePendingConfirmationsRecovery.ts` | 12–82 | Rehydrate pending confirmations into timeline on enter |
| Approval store (session memory) | `…/common/chat/approval/ApprovalStore.ts` | 11–81 | Session “always allow” map by action/identifier |
| Permission outcomes enum | `…/renderer/utils/common.ts` | 20–26 | `ProceedOnce`, `ProceedAlways*`, `Cancel`, etc. |
| Runtime approval mode in sendbox | `…/platforms/acp/AcpSendBox.tsx` | 258–303, 678–807 | Runtime config option for approval/permission mode |
| Runtime view state | `…/runtime/conversationRuntimeViewStore.ts` | 9–99 | Per-conversation hydrate: processing, pending confirmations, canSend |
| Workspace collapse policy | `…/hooks/useWorkspaceCollapse.ts` | 37–120 | Default collapsed; preference key; mobile force-collapsed |
| Chat + workspace + preview layout | `…/components/ChatLayout/index.tsx` | 31–159 | Resizable workspace px + chat/preview ratio; collapse hook |
| Layout floors | `…/utils/layoutCalc.ts` | 1–80 | Min chat 360px, preview 340px, workspace 220–500px default 260 |
| Global layout sider | `…/components/layout/Layout.tsx` | 108–444 | Collapsible left nav; mobile full-hide sider |
| Empty history | `…/GroupedHistory/index.tsx` | ~252 | `Empty` no history |
| Minimap empty / no match | `…/ConversationTitleMinimap/index.tsx` | ~134–145 | Empty + no-match states |
| Agent status / error | `…/Messages/components/MessageAgentStatus.tsx` | 29–75 | Connecting…error badges; feedback on error |
| Focus hygiene | `…/renderer/utils/ui/focus.ts` | 1–19 | Blur active element on route change (mobile keyboard) |
| Escape closes context menu | `…/Workspace/hooks/useWorkspaceEvents.ts` | 219–236 | Escape → close workspace context menu |
| YOLO / bypass surface (reject) | `…/common/adapter/ipcBridge.ts` | ~917–934 | `yolo_id` in agent type surface |
| Built-in MCP bypassPermissions | `…/process/resources/builtinMcp/grokMediaCore.ts` | ~114 | Provider-side permission bypass naming |

---

## 3. Interaction and state-flow findings

### 3.1 Conversation-owned agent steps, tools, roles, output disclosure

**Ownership model (UI):**

```text
ConversationProvider(conversation_id, workspace?, type, …)
  → MessageList(useMessageList scoped by conversation)
  → optional process grouping (turn)
  → tool_summary / file_summary / subagent_lifecycle / plan / thinking
  → ConversationRuntimeView(conversation_id) for busy / pending confirmations
```

- **Message stream is the durable UI authority** for tools, plans, permissions, and subagent lifecycle (`chatLib` union; `MessageList` switch).
- **High-frequency subagent activity is deliberately not durable messages** (`subagentActivityStore` docs L7–14): streamed tokens/tool pings land in a module store keyed by `(conversationId, subagentId)`, with sessionStorage survival across HMR/soft reload, and eviction caps.
- **Progressive disclosure is the dominant pattern:**
  - Tools: one-line status + name/description; expand for Input/Output (`MessageToolCall` L59–116).
  - Tool groups: collapsed category summary, then per-tool expand with optional full reload (`MessageToolGroupSummary`).
  - Thinking: peek while streaming, collapse when done (`MessageThinking` L42–61).
  - Subagents: always start collapsed; fail/cancel force-collapse so raw dumps do not stick open (`MessageSubagentCard` L134–145).
  - Finished turns: fold process behind `Process · N steps · duration`; keep final assistant text visible (`MessageTurnProcessGroup`, `turnGrouping`).
- **Roles:** subagent cards surface persona/type + model display name, not raw session IDs in the summary line. Tool summaries map provider names to categories (read/command/edit/search/…).
- **Output disclosure levels observed:**
  1. Collapsed status line (default for finished process/tools/thinking/subagents).
  2. Expanded structured detail (timeline, input/output, process list).
  3. File/diff launch into preview (separate column), not inline dump of full patches by default (except replace-tool `FileChangesPanel` with `defaultExpanded={true}`).

**Contrast with Pillow C2-C2:** Pillow already requires parent-supplied, conversation-filtered records; details closed by default; no raw session/run IDs; concise success disclosure. zbbody’s turn-fold and tool-category summary **complement** that baseline; zbbody’s permanent plan rail + session inspector KPIs **conflict** with C2 “no permanent card stack” lessons.

### 3.2 Editor, file-change, and diff presentation

**Two parallel surfaces:**

| Surface | Binding | Presentation |
|---------|---------|--------------|
| **In-chat file summary** | Derived from tool messages / write diffs in the conversation list | `FileChangesPanel` list with +/−; click file → preview with optional editable content from diff; click stats → pure diff preview |
| **Workspace changes** | `workspace` path string via `useFileChanges` + `fileSnapshot` IPC | Staged/unstaged lists, stage/discard/reset, expandable inline diff, open full diff |

**Editor/preview stack:**

- Preview is a first-class resizable column beside chat (`ChatLayout` + `PreviewPanel`).
- Diff uses **diff2html** with line-by-line or side-by-side (`DiffViewer`).
- Code editing via CodeMirror in preview editors; dirty tab close requires save/discard modal (`PreviewConfirmModals`).
- Ctrl/Cmd+S saves when dirty (`usePreviewKeyboardShortcuts`).

**State flow (chat file change → preview):**

```text
tool_call / tool_group result_display.file_diff
  → MessageList aggregates file_summary
  → MessageFileChanges.parseDiff
  → launchPreview({ relativePath | diffContent, contentType })
  → PreviewPanel / DiffViewer or CodeEditor
```

**State flow (workspace snapshot):**

```text
workspace change
  → fileSnapshot.init(workspace)
  → compare → staged/unstaged
  → dispose on workspace leave
```

**Pillow gap:** C2-C3 preview is **read-only project-scoped DTO** (`preview_project_resource`); no write, no workspace-root snapshot, no staged/unstaged. C3 should not import zbbody’s workspace-string snapshot model; it may adopt the **disclosure shape** (list → open file vs open diff) with portable relative paths only.

### 3.3 Approval / confirmation interactions

**In-timeline permission cards (primary collaboration pattern):**

1. Backend emits confirmation / permission message (or pending list on recover).
2. UI shows card: title, optional command, radio options, Confirm.
3. User selects option → IPC:
   - Generic: `conversation.confirmation.confirm` with optional `always_allow` when value is `proceed_always` (`MessagePermission` L44–51).
   - ACP: `conversation.confirmMessage` with `confirm_key` (`MessageAcpPermission` L54–66).
4. Card shows success strip; further confirm disabled (`hasResponded`).
5. On conversation re-entry: `usePendingConfirmationsRecovery` lists pending confirmations and injects synthetic permission messages if missing.

**Session-level “always allow” memory:** `BaseApprovalStore` (in-memory Map by action/identifier) — **not** durable SQLite policy.

**Mode selector:** AcpSendBox exposes runtime approval/permission mode options from agent config (`runtimeApprovalMode`), including team propagation for leaders.

**Other confirms (not agent-policy):** paste into workspace (`PasteConfirmModal`), dirty preview tab close, renames, quit guard (app-level).

**Pillow must implement independently:** durable approval proposals under Project→Conversation→Run, policy below UI, audit receipts, **no** YOLO / `proceed_always` as a self-granted path, opaque errors, explicit write-capable approval tests (AGENTS.md gates).

### 3.4 Project / conversation isolation and restored state

**What zbbody isolates:**

- React context and message list hooks are **conversation_id-scoped**.
- Runtime view Map is keyed by conversation (`conversationRuntimeViewStore`).
- Subagent activity keys include conversationId.
- Workspace panel preference: `workspace-preference-${preferenceKey}` (conversation or team id).
- Plan “last known entries” cleared when `conversationId` changes (`MessageList` L499–507).
- Auto-scroll `resetKey` is conversation id.

**What zbbody does *not* match Pillow’s durable model:**

- Isolation unit is largely **conversation + workspace string**, not **Project** as first-class durable root.
- Competing authorities (renderer preferences in `localStorage`/`sessionStorage`, message DB, file snapshots) — prior research already rejects this for Pillow.
- Temporary workspace auto-expand quirks documented in `useWorkspaceCollapse` (files added while away may not auto-expand).

**Restored state patterns worth conceptual adoption:**

- Runtime hydrate + pending confirmation re-injection on enter.
- Plan pin / lastKnownPlanEntries for pagination safety.
- Subagent activity survival across conversation switches for **running** agents (UX continuity) — but Pillow should store durable tool/approval/file events in SQLite, not sessionStorage as authority.

### 3.5 Layout: conversation central, auxiliaries collapsible

**Three-region workbench (desktop):**

```text
[ Left nav (collapsible) | Chat (flex, min 360px) | Preview (optional) | Workspace (collapsible, 220–500px) ]
```

Plus optional **plan progress rail** (~292px) beside the message column when width allows; otherwise floating overlay.

**Collapse rules that keep chat central:**

- Workspace **starts collapsed**; expands on real files only with preference override (`useWorkspaceCollapse`).
- Mobile: workspace and left sider forced out of the way of chat.
- Chat absorbs viewport growth; workspace is fixed px, not percentage (`layoutCalc` comment L37–39).
- Preview split constrained so chat never drops below `MIN_CHAT_PANEL_PX`.

**Clutter tension:** “Task rail is *always* resident on desktop once wide enough — not gated on having plan/subagent content” (`MessageList` L565–567) plus Session Inspector KPIs under it. This fights “conversation-centred, no permanent dashboard cards” and should be **avoided** as a default in Pillow C3.

---

## 4. Answers to the five questions

### Q1 — Collaboration patterns that complement the Grok App / Pillow baseline

Adopt as **interaction contracts** (not code):

1. **Conversation-local progressive disclosure** for tools, thinking, and subagents (collapsed default; expand for detail).
2. **Turn process fold**: after a turn finishes, hide intermediate process behind one summary line; keep final answer visible.
3. **Tool category summary** (`Read ×3 · Edit ×1`) instead of a card per tool at rest.
4. **In-timeline permission cards** with explicit confirm, disabled after respond, and **recovery of pending approvals** on re-open.
5. **File change list → open file vs open diff** as two distinct actions.
6. **Collapsible workspace/resource column** default closed; chat min-width floor; resizable only when open.
7. **Runtime view per conversation** (busy, canSend, pendingConfirmations, hydrate) reconciled from authoritative backend — conceptually matches Pillow’s SQLite-first event model.
8. **Keyboard affordances** on disclosure toggles (Enter/Space), Escape to dismiss menus, Ctrl/Cmd+S for dirty editor, focus blur on conversation switch.

### Q2 — UI patterns that create dashboard/card clutter (reject)

| Pattern | Why reject for Pillow C3 |
|---------|---------------------------|
| Always-resident plan progress rail + progress ring celebration | Permanent chrome beside chat; “alwaysShow” even empty/bootstrap |
| Bootstrap fake plan checklist while processing | Invents tasks not owned by the conversation record |
| Session Inspector “This turn” KPI rows (duration/steps/tools/model) as permanent rail | Dashboard metrics, not collaboration content |
| Floating overlay plan card on narrow widths *as default product* | Competes with timeline focus (overlay may be OK as temporary affordance, not KPI stack) |
| Agent status banners for every connect/auth success | Noise; hide historical disconnected already hints at this |
| `FileChangesPanel` defaultExpanded=true for every replace | Inflates mid-stream card height; prefer collapsed count line |
| Team-mode multi-agent home as primary nav | Prior zbbody analysis; not Stage-1 Pillow |
| Permanent multi-card stacks of successful tool runs | Contradicts C2-C2 concise success disclosure |

### Q3 — Proposal / diff / approval state by disclosure level

Recommended levels for Pillow C3 (aligned with zbbody’s good layers, filtered through Pillow authority):

| Level | Visible by default | On expand / secondary action | Never in renderer text |
|-------|--------------------|------------------------------|------------------------|
| **L0 Timeline summary** | Role · status · current step (or “Process · N steps”); approval **needs action** badge if pending | — | sessionId, runId, host paths, raw argv |
| **L1 Activity body** | Ordered steps; tool titles + status; subagent roles + status; portable relative artifact paths | Tool input/output excerpts (truncated + notice); subagent stream peek | Absolute paths, secrets, full env |
| **L2 Proposal / file change** | Collapsed “N files changed (+a/−d)” or single path + op (create/modify/delete) | File list with +/−; open **preview** vs open **diff** | Full patch inline unless user opens L3 |
| **L3 Diff / editor** | Side-by-side or unified diff in preview panel; dirty indicator | Save / discard / cancel; keyboard save | Unbounded binary dump; model-provided `file://` HTML with permissive WebView |
| **Approval L0** | Inline blocking card or sticky “awaiting approval” for the active run only | — | Auto-grant UI |
| **Approval L1** | Title, action kind (read/edit/exec/mcp), short command/path display (portable), options | Risk detail, full args (redacted), always-ask history | “Always allow” as durable product default |
| **Approval terminal** | Chosen outcome label + timestamp (audit-backed) | Link to receipt / diagnostics export | YOLO mode names as trust signals |

zbbody maps roughly: L0/L1 → MessageTool* / TurnProcess / SubagentCard; L2 → FileChangesPanel / MessageFileChanges; L3 → DiffViewer/CodeEditor; Approval → MessagePermission* + recovery. Pillow must back each visible state with **SQLite-durable** events before paint.

### Q4 — Focus, keyboard, empty, and error states for Windows desktop

From zbbody, prioritized for Pillow Windows desktop:

| State | Mattering behavior |
|-------|--------------------|
| **Focus** | Blur active editor when switching conversation/project so keystrokes do not hit the wrong sendbox or workspace tree; restore focus to chat composer after approval resolve when safe |
| **Keyboard** | Enter/Space toggle disclosure; Escape closes menus/modals; Ctrl+S save dirty preview; Ctrl+F search/minimap if offered; avoid trapping Tab inside collapsed rails |
| **Empty conversation** | Stable scroller shell (zbbody always mounts the same scroller to avoid pin races); clear emptySlot CTA for first message — no fake plan cards |
| **Empty resource/workspace** | Collapsed by default; empty tree without error flash (zbbody guards empty tree flicker in `useWorkspaceTree`) |
| **Empty history list** | Explicit empty description (GroupedHistory pattern) |
| **Loading** | Skeleton that does not replace scroller identity; no inventing tools while loading |
| **Runtime error** | Agent status error + feedback path; opaque messages (Pillow: no machine paths) |
| **Permission error / timeout** | Distinct copy for confirmation timeout / busy / command ack (`ChatConversation` / AcpSendBox error keys) |
| **Pending approval after restart** | Rehydrate list and show cards — critical for Windows kill/restart |
| **Dirty editor leave** | Modal: save / discard / cancel (PreviewConfirmModals) |
| **High-DPI / narrow** | Prefer collapse rail over three permanent columns; keep min chat width |

### Q5 — What Pillow must implement independently

1. **Project-scoped durable domain** (SQLite): conversation, turn, run, tool, approval, file-change, artifact, receipt hierarchy — not workspace strings as roots.
2. **Typed Tauri commands/events** — not Electron generic IPC / local HTTP discovery.
3. **Policy and approval authority below UI** — providers propose; host decides; no session `ApprovalStore` as truth; no YOLO / bypassPermissions product path.
4. **Write-capable tools** only with approval-path tests and audit.
5. **Portable relative paths only** at the UI boundary (already C2-C1/C2-C2/C2-C3).
6. **Agent activity projection** already started in C2-C2 — extend with turn-fold, file-change, and approval disclosure without permanent KPI rails.
7. **Preview** remains project-scoped read-only until a separate write/edit design; if edit is added, host-owned save + dirty confirm, not zbbody snapshot stage/discard as default.
8. **Desktop shell must not own agent-domain logic** (Pillow AGENTS.md).
9. **Windows desktop smoke + screenshots** at supported sizes for any C3 UI.
10. **Attribution package** only if a later brief copies Apache-2.0 source (LICENSE + copyright + NOTICE process).

---

## 5. Adopt / adapt / avoid decisions

| Decision | Item | Rationale |
|----------|------|-----------|
| **ADOPT (concept)** | Progressive disclosure defaults (tools, thinking, subagents, turn process) | Matches Grok App / C2-C2; reduces card stack |
| **ADOPT (concept)** | In-timeline approval card + pending recovery | Correct collaboration UX; maps to durable confirmations |
| **ADOPT (concept)** | File list → open file vs open diff | Clear dual affordance without dumping patches |
| **ADOPT (concept)** | Collapsible resource/workspace column; chat min width; default collapsed | Conversation-central layout |
| **ADOPT (concept)** | Per-conversation runtime hydrate flags (busy, pending approvals) | Clean sendbox gating |
| **ADAPT** | Tool category summary line | Keep labels neutral; map Pillow tool names only |
| **ADAPT** | Diff presentation | Use host-provided unified diff text in project-scoped preview; do not adopt full workspace snapshot stage machine |
| **ADAPT** | Subagent role display | Parent-supplied roles; no color-hash identity theater required |
| **ADAPT** | Keyboard/focus matrix | Windows Ctrl primary; Tauri webview focus rules |
| **AVOID** | Always-on plan rail / Session Inspector KPI dashboard | Clutter; invents metrics chrome |
| **AVOID** | Bootstrap fake plan entries | Fabricates work |
| **AVOID** | Session always-allow / YOLO / proceed_always product defaults | Security boundary |
| **AVOID** | Workspace path as project root; model-supplied roots | Isolation model |
| **AVOID** | sessionStorage/localStorage as authority for activity or policy | SQLite authority |
| **AVOID** | Source copy without attribution brief | Provenance |
| **AVOID** | Teams/multi-agent as Stage-1 primary IA | Product order |

---

## 6. Recommended C3 UI contract (Pillow)

### 6.1 Layout contract

```text
Project-scoped shell
  ├── Conversation timeline (primary, always mounted scroller)
  ├── AgentActivityDisclosure (parent-fed, conversation-filtered; details closed)
  ├── Approval strip/card (only when run has pending decision)
  ├── Optional Resource pane (closed default; C2-C1)
  └── Optional ResourcePreview (read-only; C2-C3) opened from path or file-change row
```

- No permanent plan/KPI rail.
- No third permanent dashboard column.
- Preview/resource may split chat width but must honor a min chat width (~360px conceptual).

### 6.2 Disclosure contract

```text
L0 summary  → always visible when activity exists
L1 details  → <details> closed by default; open shows steps/tools/subagents
L2 files    → collapsed file-change summary on the owning turn/run
L3 preview  → explicit user open; projectId + relativePath only
Approval    → L0 blocking when pending; outcome recorded after decide
```

Successful completed runs: **one concise disclosure** (C2-C2), optionally with “N steps · duration” like turn process — not a stack of green cards.

### 6.3 Approval contract (UI only; host owns decision)

- Show: kind, short title, portable target, options from host, Confirm/Cancel.
- Disable after submit; show terminal outcome from durable state (not optimistic-only).
- On conversation restore: re-list pending approvals for that conversation only.
- Never expose always-allow as the recommended path in C3.

### 6.4 File-change contract

- Events: create / modify / delete with optional +/− counts from host.
- Actions: Open preview | Open diff (read-only in C3 unless write design lands).
- Paths: portable relative only; opaque on error.

### 6.5 Empty / error / focus (minimum)

| Condition | UI |
|-----------|-----|
| No messages | emptySlot CTA; no skeleton tools |
| Loading history | skeleton inside stable scroller |
| No pending activity | render nothing for activity (not empty cards) |
| Pending approval | single high-priority card, focusable Confirm |
| Run failed | failure disclosure remains open-capable; role=alert for error text |
| Conversation switch | clear foreign activity; blur previous focus |

### 6.6 Independence clause

All of §6 is a **Pillow-owned contract**. zbbody evidence informs shape only. Implementation lives under Pillow packages/tests; no zbbody modules as dependencies.

---

## 7. Risks

| Risk | Severity | Mitigation |
|------|----------|------------|
| Accidental source port from dirty zbbody worktree | High | Conceptual-only; no copy; never stage zbbody |
| Importing always-allow / YOLO UX | High | Explicit avoid list; approval-path tests |
| Plan rail / inspector reintroduced as “productivity” | Medium | C3 contract forbids permanent KPI rails |
| sessionStorage activity store as authority | Medium | Durable SQLite events first; UI projection second |
| Workspace string isolation ≠ Project isolation | High | Keep projectId root in Tauri commands |
| Diff2html / full editor stack scope creep | Medium | Stay on C2-C3 read-only preview until write design |
| License/attribution skip if later code adapt | Medium | Require NOTICE + SPDX on any accepted port |
| Windows focus traps (WebView2) | Medium | Explicit blur/focus tests on conversation switch |
| Pending approval lost after kill | High | Recovery list like zbbody, but from SQLite |
| Bootstrap / invented plan content | Medium | Only display host-authored plan entries |

---

## 8. Trace summary (state machines)

### 8.1 Tool disclosure

```text
tool event persisted → MessageList tool_summary
  → collapsed: status + name [+ category counts]
  → expand: input/output (truncated optional)
  → if file_diff: file_summary → FileChangesPanel → preview/diff
```

### 8.2 Subagent disclosure

```text
lifecycle message (durable) → MessageSubagentCard (collapsed)
  + activity stream (ephemeral store) → only while expanded & subscribed
  → terminal: mark finished; force-collapse on fail/cancel
```

### 8.3 Approval

```text
pending confirmation (host)
  → timeline card OR recovery inject
  → user select option → confirm IPC
  → optional always_allow (zbbody only; Pillow rejects as product default)
  → responded UI / remove on confirmation.remove
```

### 8.4 Layout collapse

```text
enter conversation → workspace collapsed
  → HAS_FILES (user workspace or mid-session write) → maybe expand
  → user preference overrides
  → mobile: force collapsed
```

---

## 9. Relation to prior Pillow waves

| Wave | Already locked | C3 should extend with (concept from zbbody) |
|------|----------------|-----------------------------------------------|
| C2-C2 activity disclosure | Conversation filter, closed details, no session IDs | Turn process fold; tool category summary; failure remains visible |
| C2-C1 resource pane | Collapsed default, projectId, opaque errors | Same collapse discipline when wiring workbench |
| C2-C3 preview | Project-scoped read-only DTO | Dual open file / open diff entry points from activity |
| B2 security research | Approval policy host-owned | In-timeline approval card UX only |

---

## 10. Non-actions taken

- Did **not** modify `zbbody reference clone (out of tree)` or its Git state.
- Did **not** modify Pillow product code, tests, or other docs.
- Did **not** stage or commit.
- Wrote **only** this file under `docs/research/`.

---

COMPLETION: DONE
