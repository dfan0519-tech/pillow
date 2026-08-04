# Wave C3 Research — Grok App Interaction & State Flow

**Date**: 2026-08-01
**Agent**: grok1 (worker)
**Role**: read-only interaction / state-flow research
**Reference root**: `<workspace>\VaultSystem\references\grok-app`
**Pinned commit**: `cbbbdf17bb8db1588b6707f3f42f6b00397e27f9` (verified `git rev-parse HEAD`)
**License**: MIT
**Pillow baseline policy**: `docs/references/upstream-baseline.md`
**Keeper boot**: bridge returned HTTP 400 for both `/api/agent-bridge/boot` and `/api/agent-sessions/claim` at research time; work continued read-only under project rules.

---

## 1. Executive summary

Grok App keeps the **conversation column dominant** and treats permissions, tool activity, plan review, and file diffs as **session-scoped gates and contextual panes**, not permanent card stacks.

For Pillow C3, the high-value adopt set is:

1. Host-owned session FSM with `awaiting_permission` as a first-class busy state.
2. A single permission bar (allow once / allow session / deny) with Escape → deny, focus trap, optional auto-deny timeout.
3. Tool steps woven into the assistant timeline (summary rows + collapse of context tools); live mid-stream shows **one** running tool line.
4. Session Changes list + on-demand unified diff in the **right resource pane** (closed by default; chat min-width protected).
5. Multi-session gates parked per session; only the viewed session surfaces UI; background sessions toast/notify.
6. Escape priority chain: overlay → permission → ask-user → menus → stop generation.

Avoid cloning Grok Build–specific option-id heuristics, YOLO / `bypassPermissions`, download auto-allow shell parsing, SuperGrok branding, and raw preview dumps of full ACP permission JSON in the bar.

---

## 2. Evidence table (source paths / lines)

| Topic | Path | Lines | What the code does |
|------|------|-------|--------------------|
| Host session states | `src-tauri/src/session_fsm.rs` | 11–18, 181–199 | `Idle \| Connecting \| Ready \| Streaming \| AwaitingPermission \| Disconnected`; `await_permission` only from Streaming; resolve → Streaming |
| Multi-session process policy | `src-tauri/src/session_manager.rs` | 1–13 | Switching chats never cancels Streaming / AwaitingPermission; demotes to background pump |
| Permission request emit | `src-tauri/src/session_manager.rs` | 3239–3387 | On `AcpEvent::PermissionRequest`: auto-allow / auto-deny / else `session://permission` with preview ≤2000 chars |
| Permission resolve | `src-tauri/src/session_manager.rs` | 5422–5471 | `resolve_permission` targets **raising session’s ACP child**; `allow_session` writes `allow_cache` by `scope_key` |
| Resolve IPC | `src-tauri/src/commands.rs` | 185–196 | `session_resolve_permission(rpc_id, decision, option_id, scope_key, session_id)` |
| Policy model | `src-tauri/src/permission.rs` | 10–56, 446–495 | Ask / AllowOnce / AllowForSession / DontAsk / AcceptEdits / Deny / AlwaysApprove; outside-project never session-cached except YOLO |
| Scope key + path safety | `src-tauri/src/permission.rs` | 91–184, 145–184 | `scope_key = tool:normalized_target`; lexical normalize blocks `..` escape |
| UI permission DTO | `src-tauri/src/session_manager.rs` | 167–178 | `UiPermissionRequest { rpcId, sessionId, toolCallId, toolName, title, preview, scopeKey, options }` |
| Frontend PermissionPayload | `src/lib/session.ts` | 1089–1098 | Mirrors host DTO for bar |
| Map ACP options → buttons | `src/lib/permissionOptions.ts` | 69–138 | Always three decisions; prefer real `optionId`; fallbacks `allow_once` / `allow_always` / `reject` |
| Permission listener + bg park | `src/App.tsx` | 3304–3344 | Park in `pendingPermBySessionRef`; bar only if viewing session; else toast + desktop notify |
| Resolve + Escape deny | `src/App.tsx` | 9113–9182 | Host invoke; Escape → deny; Tab trap; focus primary allow |
| Auto-deny timeout | `src/lib/permissionTimeout.ts` | 1–101; `App.tsx` 9184–9209 | localStorage seconds; 0=off; countdown; same path as Deny |
| Perm bar chrome | `src/App.tsx` | 12281–12350 | `role="dialog"` bar above composer: title, summary, optional preview `<pre>`, three actions |
| Tool event → timeline | `src/lib/session.ts` | 579–668 | `applyToolEvent`: upsert `tool_step` row + embed segment on current-turn assistant |
| Filter inlined tools | `src/lib/session.ts` | 302–328 | Transcript paint drops journal rows already in assistant segments |
| Live one-line tool | `src/components/lobe-chat/AgentActivity.tsx` | 1–50 | Only latest **running** tool title; no stack |
| Timeline tool row | `src/components/lobe-chat/TimelineToolRow.tsx` | 18–154, 166–199 | Quiet fail mark; ≥3 context tools collapsible group |
| Tool kind display | `src/lib/toolDisplay.ts` | 6–136 | bash/read/edit/search/subagent/fallback; path basename summaries |
| Turn activity aggregate | `src/lib/turnActivity.ts` | 27–223 | Segments, errorCount, modifiedPaths, `shouldExpand` on errors |
| session://tool handler | `src/App.tsx` | 2969–3013 | `applyToolEvent` + `mergeSessionChange` for edit tools |
| Session change model | `src/lib/sessionChanges.ts` | 10–218 | Upsert by path; terminal statuses; rebuild from messages |
| Unified diff helper | `src/lib/sessionChanges.ts` | 224–234+ | Client-side Myers-ish unified diff for pane preview |
| Resource open targets | `src/components/ResourceViewer.tsx` | 104–109, 150, 293 | `file \| url \| changes`; SideMode `files \| changes \| plan` |
| Diff load cascade | `src/components/ResourceViewer.tsx` | 452–595 | payload before/after → git_file_diff → HEAD+after → after-only |
| Open changes from chat | `src/App.tsx` | 12234–12245 | `onOpenSessionChanges` / `onOpenModifiedPath` → open aside + `ResourceOpenTarget` |
| Layout: chat dominance | `src/lib/layout.ts` | 48–73, 128–146 | `MAIN_CHAT_MIN_WIDTH=400`; aside default collapsed; clamp preserves chat floor |
| View focus epoch | `src/lib/viewFocus.ts` | 1–54 | Draft navigation epoch prevents async connect yanking workbench |
| Escape priority | `src/lib/escapeStop.ts` | 9–43 | Permission / ask-user / overlays win over stop |
| Focus trap helpers | `src/lib/a11yFocus.ts` | 46–104 | Tab trap; prefer `.perm-bar__btn--allow` |
| Error deck | `src/lib/errorDeck.ts` | 11–150 | Structured problem/cause/primary/secondary for CLI/auth/crash/stall… |
| Plan gate UI | `src/components/PlanReviewPanel.tsx` | 1–119; `src/lib/planBody.ts` 56–66 | Review expands when `rpcId`; progress collapsed by default |
| Plan resolve IPC | `src/lib/api.ts` | 246–250; `session_manager.rs` 5487–5490 | `approved \| cancelled \| abandoned` |
| Ask-user gate | `src/components/AskUserModal.tsx` | 1–25; `session.ts` 1113–1119 | Questionnaire modal; blocked turn like permission |
| Live multi-session map | `src/lib/sessionLiveStore.ts` | 10–26, 65–100 | Per-session busy / awaitingPermission / liveToolTitle projection |
| Activity panel rows | `src/lib/agentActivity.ts` | 17–115 | Aggregate streaming / awaiting_permission / connecting for Tasks |
| Deferred prompt complete | `src-tauri/src/session_manager.rs` | 3211–3227 | Do not Ready while tools/permission/plan/ask_user open |
| Upstream baseline class | `docs/references/upstream-baseline.md` (pillow) | 9–45 | Grok App = conceptual UI baseline; commit pin; no whole-app copy |

---

## 3. State-flow diagram (text)

### 3.1 Permission / approval (in conversation)

```
[Streaming]
    │ ACP PermissionRequest
    ▼
Host: fsm.await_permission()  →  state = AwaitingPermission
    │
    ├─ may_auto_allow(policy, allow_cache, scope_key, outside?, edit?, download?)
    │     → respond_permission(Selected optionId) → permission_resolved_continue() → Streaming
    │
    ├─ may_auto_deny(DontAsk|Deny)
    │     → respond_permission(reject*) → continue Streaming (agent may end turn)
    │
    └─ else emit session://permission {rpcId, sessionId, tool…, scopeKey, options, preview}
              │
              ├─ viewingSessionId == sessionId
              │     → setPerm(p); focus trap; optional countdown auto-deny
              │     → UI: Allow once | Allow for session | Deny
              │           │
              │           ├─ allow_once  → session_resolve_permission(decision, optionId, sessionId)
              │           ├─ allow_session → same + Host allow_cache.allow(scope_key)
              │           └─ deny / Esc / timeout → deny optionId
              │                 → Host permission_resolved_continue()
              │                 → try_finish_deferred_prompt_complete
              │
              └─ other session
                    → pendingPermBySessionRef.set(sessionId, p)
                    → toast + optional desktop notify
                    → on reopen chat: rehydrate bar from map
```

Related gates (same busy family):

- `session://ask_user` → `AskUserModal` (blocked turn; Esc owned by modal).
- `session://plan` + `rpcId` → Plan review in resource pane / plan bar (`approved` / revise / abandon).

Composer send is blocked while `awaiting_permission` (see `App.tsx` queue blocked toast around 6040).

### 3.2 Tool progress → completion / failure → Changes

```
ACP ToolCall / session://tool
    │
    ▼
applyToolEvent(messages, payload)
    ├─ upsert ChatMessage role=tool, marker=tool_step, streaming=running?
    └─ upsertToolInSegments on current-turn assistant (stream order)
    │
    ├─ UI live edge: pickRunningTurnTool → single LiveToolText line
    ├─ UI history: TimelineToolRow / TimelineContextGroup (≥3 reads/searches collapsed)
    │     failed → is-error + one-line fail hint (not a card stack)
    │
    └─ if isEditToolKind(kind) && path
          mergeSessionChange(sessionChangesById[sid])
                status: in_progress | completed | failed | cancelled…
                optional before/after from payload

User opens modified path / Changes
    → openAsidePane(); ResourceOpenTarget { type:"changes", path? }
    → ResourceViewer sideMode=changes
    → loadChangeDiff cascade:
         payload before+after → git_file_diff → HEAD+after → after-only
```

### 3.3 Conversation dominance vs resource pane

```
[ Left: projects/sessions ] [ Center: ConversationThread ≥400px ] [ Right: ResourceViewer ]
                                      dominant surface                 default asideCollapsed=true
                                                                           files | changes | plan
                                                                           soft-grow width; never
                                                                           squeeze chat below min

Async navigation: ViewFocus {sessionId, epoch}
  capture epoch before await → shouldAdoptView / isViewingSendTarget before writing UI
```

### 3.4 Terminal / cancel / expiry

| Outcome | Mechanism |
|---------|-----------|
| Allow once | ACP Selected; no cache write |
| Allow session | ACP Selected + `allow_cache` by `scope_key` |
| Deny / Esc / timeout | Same deny path; countdown from `permissionTimeoutSec` |
| Stop generation | Esc only if no higher owner (`escapeStop.ts`); Tasks “Stop all” for busy sessions |
| Turn cancelled | `session://turn_marker` `turn_cancelled`; clear pending gates |
| Crash / disconnect | FSM → Disconnected + Error deck actions |
| Stream stall | Soft stall banner (keep waiting / cancel); tiers pre_token / tools / post_output |
| Process limit | `PROCESS_LIMIT` error deck → open runtime settings |
| Plan abandon | `session_resolve_plan` decision `abandoned` |

---

## 4. Answers to research questions

### Q1 — Interaction primitives Pillow should adopt for C3

| Primitive | Grok App source | Pillow C3 use |
|-----------|-----------------|---------------|
| **Session FSM busy states** including `awaiting_permission` | `session_fsm.rs`, `session.ts` SessionState | Domain-owned state (SQLite / host), UI only projects |
| **Single permission bar** (not stacked cards) above composer | `App.tsx` perm-bar | Primary write/tool approval UX |
| **Three decisions**: once / session-scope / deny | `permissionOptions.ts` + Host cache | Map to Pillow approval policy IDs; store audit below UI |
| **Park gates by conversation id** | `pendingPermBySessionRef` | Multi-run / multi-agent without stealing focus |
| **Timeline-woven tool steps** + collapse context tools | `applyToolEvent`, `TimelineToolRow` | Extend C2-C2 activity disclosure; no permanent tool cards |
| **One live tool line** at stream edge | `AgentActivity.tsx` LiveToolText | Mid-stream disclosure only |
| **Changes list + on-demand diff pane** | `sessionChanges.ts`, ResourceViewer | C3 proposal → expand in resource pane (C2-C1/C2-C3) |
| **Chat min-width + aside collapsed default** | `layout.ts` | Preserve conversation dominance |
| **Escape priority chain** | `escapeStop.ts`, perm focus trap | Desktop acceptance matrix |
| **Deferred “turn ready” while gates open** | deferred prompt_complete | Prevent false idle while approval pending |
| **Structured error deck** | `errorDeck.ts` | Recoverable failure UX (adapt codes to Pillow) |
| **ViewFocus epoch** for async navigation | `viewFocus.ts` | Prevent race when opening resources/connect |

### Q2 — Unsafe or too provider-specific behaviors

| Behavior | Why avoid / adapt | Source |
|----------|-------------------|--------|
| **YOLO / AlwaysApprove / bypassPermissions** as product default | Unattended full auto-approve is unsafe for industrial local IDE | `permission.rs` AlwaysApprove |
| **AcceptEdits auto-allow all edit tools** | Convenient but weak audit boundary; Pillow requires approval-path tests for write tools | `may_auto_allow` + AcceptEdits |
| **Download shell auto-allow** (curl/wget/aria2 heuristics) | Shell parsing is brittle; false negatives/positives; provider-specific | `is_download_command`, `may_auto_allow_download` |
| **Grok optionId heuristics** (`allow_command_always`, underscore vs hyphen) | Tightly coupled to Grok Build CLI option catalogs | `session_manager.rs` 3319–3327 |
| **Raw permission `preview` as JSON dump** in `<pre>` | Leaks noisy wire payloads; poor UX | `App.tsx` 12315–12317 |
| **`_x.ai/exit_plan_mode` / `_x.ai/ask_user_question` tool names** | xAI extension tools, not portable ACP core | Plan/AskUser modules |
| **localStorage-only permission timeout** | Not audit-grade; fine as UX pref, not security control | `permissionTimeout.ts` |
| **Session allow_cache without durable audit** | Memory-only session cache; Pillow needs project-scoped durable approval records | `resolve_permission` allow_cache |
| **SuperGrok / account tier chrome** in workbench | Branding noise | App.tsx composer brand blocks |
| **Whole App.tsx monolith reuse** | Upstream baseline forbids whole-app copy | `upstream-baseline.md` |

### Q3 — Proposal summary → diff → approval → result without permanent card stacks

Grok App pattern to mirror (conceptually):

1. **Compact timeline row** (tool summary / “edited N files”) lives **inside** the assistant turn — not a floating card stack (`TimelineToolRow`, turn activity segments).
2. **Click path / “Changes”** opens **contextual right pane** (`ResourceOpenTarget type: "changes"`), not a second permanent conversation column of cards.
3. **Diff is demand-loaded** for the selected path (payload → git → HEAD); closing pane or switching chat clears selection without leaving orphan cards (`ResourceViewer` selection drop effect ~434–450).
4. **Approval is a single bar** (or plan panel actions) while `awaiting_permission` / plan `rpcId`; resolution removes the bar (`setPerm(null)`), does not archive a stack of permission cards.
5. **Result** returns to timeline: tool status completed/failed (quiet red mark) + optional toast; Changes list updates by path upsert (newest first), still not a card deck in the transcript.

Pillow C3 recommendation:

```
ProposalSummary (timeline, 1 line)
  → expand in-place details OR open ResourcePane(diff|file)
  → ApprovalBar (once / session-policy / deny) if write-capable
  → ResultChip on same timeline row (accepted | rejected | cancelled | expired | failed)
```

Never: permanent vertical stack of proposal cards in the transcript.

### Q4 — Timeline vs contextual resource pane

| Belongs in **conversation timeline** | Belongs in **resource / context pane** |
|--------------------------------------|----------------------------------------|
| User / assistant prose | Full file preview / edit buffer |
| Interleaved tool **summaries** (status, short path, fail hint) | Unified **diff** body |
| Live single running tool line | File tree / search / recents (C2-C1) |
| Permission / plan **decision outcomes** (short) | Plan markdown body + step list when reviewing |
| Turn cancelled / compact / error records | Multi-tab open files, external URL preview |
| Links that *open* resources | Workspace git status list |
| Activity disclosure closed-by-default (C2-C2) | Session Changes aggregate across tools |

**Not** in either surface as primary: provider wire dumps, raw `sessionId`/`runId`, absolute machine paths in Pillow product UI (Pillow C2 already enforces portable paths + opaque errors — keep that over Grok App’s freer path display).

### Q5 — Keyboard / focus / error states desktop acceptance must cover

| ID | State | Expected behavior |
|----|-------|-------------------|
| K1 | Permission bar open | Focus primary Allow; Tab cycles only inside bar; Esc = Deny |
| K2 | Permission timeout > 0 | Countdown visible; expiry = Deny path; bar dismisses |
| K3 | Ask-user modal open | Esc cancels questionnaire; does **not** stop turn via global Esc until closed |
| K4 | Streaming, no overlay | Esc stops generation |
| K5 | Streaming + permission | Esc denies permission (not stop) |
| K6 | Slash / menus / find open | Esc closes overlay first (`escapeStop`) |
| K7 | Background session permission | No bar; toast/notify; reopen chat restores bar from parked map |
| K8 | Resolve with wrong sessionId | Must not hang other ACP child (regression: always pass raising `sessionId`) |
| K9 | Tool fail | Timeline quiet error mark + optional expand; no silent drop |
| K10 | Turn cancelled | Marker/chip; pending gates cleared |
| K11 | Stream stall | Banner: keep waiting / cancel; tiered copy not “first token” after tools |
| K12 | Error deck codes | Primary/secondary actions work (reconnect, doctor, account, dismiss…) |
| K13 | Aside open on narrow window | Chat width ≥ floor; aside clamps; pane close restores dominance |
| K14 | Async open/connect race | ViewFocus epoch: navigating away does not steal workbench |
| K15 | Awaiting permission | Composer send disabled / queued with clear reason |
| K16 | Plan review gate | Approve / request changes / dismiss; expanded when `rpcId` present |

---

## 5. Adopt / Adapt / Avoid decisions

### ADOPT (conceptual reference — implement independently in Pillow)

- Host-owned FSM with explicit permission-wait state projected to UI.
- Single permission/approval bar with once / session / deny.
- Park multi-conversation gates; surface only for viewed conversation.
- Timeline-inline tool steps; collapse context-only tool runs; one live tool line.
- Changes aggregate by path; open diff in contextual resource pane.
- Aside collapsed by default; protect conversation min-width.
- Escape ownership hierarchy; permission focus trap; optional auto-deny as **UX** pref.
- Do not mark turn idle while tools/permission/plan/ask-user still open.
- Structured recoverable error surfaces (problem / cause / actions).

### ADAPT (focused port with material change + provenance)

| Grok App piece | Pillow change |
|----------------|---------------|
| `mapPermissionButtons` / `PermissionPayload` | Map to Pillow approval protocol + SQLite audit; no Grok optionId catalog hardcode |
| `sessionChanges` + unified diff | Project-scoped portable paths only; opaque errors (align C2-A/B/C1) |
| `layout.ts` aside clamps | Reuse numbers/ideas under Pillow CSS tokens; Windows sizes from acceptance |
| `errorDeck` codes | Remap to Pillow agent adapter error taxonomy |
| `PlanReviewPanel` progressive expand | Only if Pillow plan gate exists; generic name, not `_x.ai/*` |
| `SessionAllowCache` | Durable project/session policy store with explicit tests |
| `turnActivity` / `toolDisplay` | Fold into C2-C2 `AgentActivityDisclosure` projection, not a second UI language |

Provenance level per `upstream-baseline.md`: **adapted implementation** requires source commit, path, MIT notice, and change note in `THIRD_PARTY_NOTICES.md` when code is copied; pure reimplementation stays **conceptual reference**.

### AVOID (do not reuse)

- Direct copy of `App.tsx` permission/workbench monolith.
- YOLO / AlwaysApprove as default or lightly gated chip.
- Download-command auto-allow heuristics.
- Provider-specific optionId pick lists as the only resolution path.
- Permanent proposal/tool card stacks in the transcript.
- Surfacing absolute paths / raw ACP JSON in default UI.
- Remote IM / Feishu / SuperGrok / wallpaper / voice overlays for C3 scope.
- Treating Grok App Host as Pillow’s agent domain (desktop shell must not own agent logic — Pillow AGENTS.md).

---

## 6. Provenance classification

| Area | Classification | Notes |
|------|----------------|-------|
| Conversation-dominant workbench chrome | **Conceptual reference** | Baseline already accepted in `upstream-baseline.md` Grok App UI Baseline |
| Permission bar UX (once/session/deny, Esc) | **Conceptual reference** | Reimplement against Pillow approval domain |
| Host FSM `AwaitingPermission` | **Conceptual reference** | Mirror states in Pillow host/kernel; do not vendor Rust FSM blindly |
| `permissionOptions.ts` / `permissionTimeout.ts` | **Adapted** if ported | Small pure modules; easy to reimplement; if copied, record MIT + commit |
| `sessionChanges.ts` / `buildUnifiedDiff` | **Adapted** if ported | Diff preview logic; path policy must change for portable paths |
| `ResourceViewer.tsx` whole file | **Avoid direct reuse** | Large Codex-inspired surface; steal interaction contracts only |
| `TimelineToolRow` / activity collapse | **Conceptual / adapted** | Align with existing C2-C2 disclosure |
| ACP golden fixtures under grok-app tests | **Conceptual** | Protocol shapes may inform adapter fixtures; verify against Pillow adapters |
| Grok Build CLI option catalogs | **Avoid** | Provider-specific |
| Package dependency on grok-app | **Not applicable** | No direct dependency; fixed local MIT tree only |

**Direct dependency**: none recommended for C3 UI. Consume protocols via Pillow adapters, not by importing Grok App.

---

## 7. Recommended C3 UI contract (Pillow)

### 7.1 Surfaces

```
Workbench
├── Conversation (dominant)
│   ├── Timeline messages
│   ├── ToolStepSummary rows (inline; collapsible context groups)
│   ├── LiveToolLine (at most one, running only)
│   ├── ProposalSummary (1-line; expands details in-place optional)
│   └── ApprovalBar | AskUserDialog | ErrorBanner (ephemeral gates)
└── ResourceContextPane (secondary, default collapsed)
    ├── Files / tree / preview (C2-C1/C2-C3)
    ├── DiffView (selected proposal/path)
    └── PlanReview (optional gate body)
```

### 7.2 Domain objects (UI projection only)

```ts
// Conceptual contract — implement in Pillow types, not copied from Grok App
type ApprovalDecision = "allow_once" | "allow_session" | "deny";

type ApprovalRequest = {
  requestId: string;
  conversationId: string;
  toolCallId: string;
  toolName: string;
  summary: string;          // human one-liner (path/command clipped)
  risk: "read" | "edit" | "shell" | "network" | "unknown";
  scopeKey: string;         // durable cache key (project-relative)
  options: { id: string; decision: ApprovalDecision; label: string }[];
  createdAt: string;
  expiresAt?: string | null;
};

type ToolStepProjection = {
  toolCallId: string;
  title: string;
  kind: string;
  status: "running" | "completed" | "failed" | "cancelled";
  path?: string;            // portable relative only in product UI
  failHint?: string;        // opaque if needed
};

type FileChangeProjection = {
  path: string;             // portable
  status: string;
  toolKind: string;
  updatedAt: string;
};

type ProposalUiState =
  | "proposed"
  | "diff_open"
  | "awaiting_approval"
  | "accepted"
  | "rejected"
  | "cancelled"
  | "expired"
  | "failed"
  | "applied";
```

### 7.3 Transitions (must be tested)

1. `proposed` → user opens diff → `diff_open` (pane) without leaving conversation.
2. Write-capable tool → `awaiting_approval` + ApprovalBar; composer send blocked.
3. `allow_once` / `allow_session` / `deny` / Esc / expiry → terminal decision + audit record.
4. Tool `running` → LiveToolLine; terminal → line hides; timeline row keeps quiet status.
5. Edit tools → `FileChangeProjection` upsert; open path → DiffView.
6. Switch conversation mid-gate → park request; return → rehydrate bar.
7. Cancel turn → clear gates; timeline shows cancelled; no stuck busy badge.

### 7.4 Non-goals for C3

- YOLO full bypass as default.
- Permanent multi-card proposal stacks.
- Provider branding or Grok CLI install/doctor chrome.
- Desktop shell implementing approval authority (authority stays below UI).

---

## 8. Risks

| Risk | Impact | Mitigation |
|------|--------|------------|
| Cloning Grok App optionId map breaks non-Grok adapters | Permissions hang or wrong outcome | Negotiate option ids from adapter protocol; fixture tests per adapter |
| Session allow cache without audit | Unreviewed repeated writes | SQLite approval ledger; project trust gates |
| Auto-deny timeout as security control | Silent deny confuses users / fails automation | Default off; label as convenience; never sole safety |
| Diff from tool payload only | Incomplete when agent omits before/after | Cascade like ResourceViewer but fail closed with opaque error |
| Showing absolute paths (Grok App does) | Leaks machine layout; breaks portable project model | Keep C2 portable-path + opaque error rules |
| False “ready” while approval open | User sends overlapping turns | Defer idle until gates clear (prompt_complete pattern) |
| Background permission resolved on wrong process | Stuck agent forever | Always route by `conversationId` / raising session id |
| Over-building Plan/AskUser xAI tools | Scope creep | Only generic questionnaire + plan gate if protocol supports |
| Card-stack regression in C2-C2 wiring | Timeline clutter | Enforce single summary + pane expansion in acceptance |
| Whole-file ResourceViewer port | License/attribution + unmaintainable merge | Reimplement contracts; record adapted snippets only |

---

## 9. Mapping to Pillow Stage-1 waves already done

| Prior wave | Relationship to this research |
|------------|-------------------------------|
| C2-C1 resource pane | Target host for C3 diffs/files; keep closed-by-default |
| C2-C2 agent activity | Timeline disclosure foundation; extend with proposal/approval states |
| C2-C3 preview | File preview path for open-from-timeline |
| C2-E activity driver | Live tool/run projection source of truth below UI |
| B3 / approval-path tests | Write tools must keep explicit approval tests when C3 wires gates |
| `upstream-baseline.md` | Grok App interaction baseline remains until Pillow design system supersedes |

---

## 10. File index (primary trace order)

1. `src-tauri/src/session_fsm.rs` — states
2. `src-tauri/src/permission.rs` — policy, scope, auto-allow
3. `src-tauri/src/session_manager.rs` — permission/tool/plan event pumps
4. `src-tauri/src/commands.rs` — `session_resolve_permission`
5. `src/lib/session.ts` — payloads, `applyToolEvent`, segments
6. `src/lib/permissionOptions.ts` / `permissionTimeout.ts`
7. `src/lib/sessionChanges.ts` / `toolDisplay.ts` / `turnActivity.ts`
8. `src/lib/layout.ts` / `viewFocus.ts` / `escapeStop.ts` / `a11yFocus.ts`
9. `src/lib/errorDeck.ts` / `sessionLiveStore.ts` / `agentActivity.ts`
10. `src/components/lobe-chat/TimelineToolRow.tsx` / `AgentActivity.tsx`
11. `src/components/ResourceViewer.tsx` / `PlanReviewPanel.tsx` / `AskUserModal.tsx`
12. `src/App.tsx` — listeners, bar UI, open-aside wiring

---

## 11. Completion

Research limited to fixed local Grok App tree at commit `cbbbdf17bb8db1588b6707f3f42f6b00397e27f9`. No application code, other docs, Git staging, commits, or Keeper product directory modifications were made. Output is this file only.

**COMPLETION: DONE**
