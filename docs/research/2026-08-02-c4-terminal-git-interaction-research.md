# Wave C4 Research — Terminal and Git Interaction

**Date**: 2026-08-02
**Agent**: grok3 (worker)
**Role**: conversation-centred terminal / Git interaction research
**Scope**: interaction contracts, state flows, progressive disclosure, layout, keyboard, acceptance scenarios
**Not in scope**: process lifecycle authority (grok1), Git protocol/governance (grok2), threat model gates (grok4), product implementation

**Pillow product order**: industrial local AI IDE first; conversation remains visual centre; no second task dashboard.
**Keeper boot**: `POST /api/agent-bridge/boot` succeeded; session `<keeper-session>`, project `VK-058` / pillow.

---

## 1. Executive summary

C4 must extend the **accepted C1–C3 conversation workbench** so that:

1. **One-shot command runs** (Agent-proposed or user-initiated) leave a durable, conversation-owned summary in the **owning timeline**.
2. **Streaming / large output** opens a **contextual terminal surface** that collapses by default, never a permanent multi-project terminal dashboard.
3. **Git status / branch / file diffs** live as a **mode of the existing collapsible resource pane**, reusing C2 file/preview and C3 write-diff placement—not a new rail.
4. **Terminal and Git mutations** reuse the C3 **single approval bar** pattern (one pending governed action, Escape = reject, focus trap), with C4-specific summary fields.
5. **Progressive disclosure** is the only path from summary → output tail → full log → tests/artifacts → Git evidence.

Pinned references inspected:

| Source | Pin | License | Use for C4 interaction |
|--------|-----|---------|------------------------|
| Pillow C1–C3 dirty tree | HEAD `7b7f70f` + accepted working tree | Pillow | Authority for UI placement, scope guards, redaction |
| Grok App | `cbbbdf17bb8db1588b6707f3f42f6b00397e27f9` | MIT | Conceptual baseline: tool-in-timeline, Changes pane, permission bar, layout floors |
| zbbody / AionUi | `00c293414333b6fcc4e4598389fa3952c8d0b09b` | Apache-2.0 | Conceptual: command step expand, category summary, turn fold |

**Critical finding:** Grok App at the pinned commit has **no dedicated interactive PTY terminal panel**. Shell work is modelled as **bash/shell tool steps** woven into the assistant timeline, with optional expanded detail. Workspace Git lives in `ResourceViewer` **Changes** mode (`files | changes | plan`). Pillow must therefore **design** a contextual terminal surface (not copy a Grok App PTY shell) while **adopting** Grok’s conversation-dominant layout and zbbody’s command progressive disclosure.

---

## 2. Evidence base

### 2.1 Accepted Pillow C1–C3 UI (implementation paths)

| Surface | Path | Interaction contract already accepted |
|---------|------|----------------------------------------|
| Workbench stage | `apps/desktop/src/features/workbench/Workbench.tsx` | Conversation column dominant; resource toggle in stage header; `has-resources` class when pane open; project/conversation generation scope for async results |
| Conversation timeline | `…/conversations/ConversationTimeline.tsx` | Messages + run activity; open resource from conversation |
| Run activity | `…/conversations/RunActivity.tsx` | `<details>` open while run active; auto-close when terminal; summary line + nested blocks |
| Agent activity disclosure | `…/conversations/AgentActivityDisclosure.tsx` | Closed-by-default progressive disclosure; nested tool output; concise success; no raw session/run IDs |
| Activity projection | `…/conversations/agentActivityProjection.ts` | Conversation filter; opaque errors; portable relative artifacts only; truncation notices |
| Terminal activity normalizer | `…/conversations/normalizeTerminalActivity.ts` | On run terminal, close open plan/tool/sub-agent child rows so UI never shows “running” after terminal |
| Write approval bar | `…/conversations/WriteApprovalBar.tsx` | Single bar; Approve once / Reject / Review diff; Esc → reject; Tab trap; grant never in React state text |
| Write gateway redaction | `apps/desktop/src/lib/writeApprovalGateway.ts` | Strip `approvalGrant` for display; path-free terminal messages |
| Contextual resource browser | `…/resources/ResourcePane.tsx` | Closed by default; recents + tree; project switch clears via generation counters; opaque errors |
| Workbench resource host | `…/workbench/ResourcePane.tsx` | Embeds browser + preview + **write-diff** target; close invalidates in-flight preview/diff |
| Write diff preview | `…/workbench/WriteDiffPreview.tsx` | Host-generated bounded unified diff only |
| Layout / screenshots | C3 acceptance | Captures at **1440×960** and **1024×720**; conversation centre; one approval bar; no second dashboard |

Checkpoint boundaries (must not regress):

- C1: governed read receipts into owning conversation; no shell/Git.
- C2: collapsible resources + progressive agent activity; portable paths; opaque errors.
- C3: one pending write proposal; approve-once only; grant memory-only; resource-pane diff review; project/conversation switch invalidates.

### 2.2 Grok App — component paths and state flows

**Pin**: `<workspace>\VaultSystem\references\grok-app` @ `cbbbdf17…` (MIT).

| Topic | Path | Lines / API | What the code does |
|-------|------|-------------|--------------------|
| Layout floors | `src/lib/layout.ts` | `MAIN_CHAT_MIN_WIDTH=400`, `ASIDE_WIDTH_MIN=320`, default `asideCollapsed=true` | Chat never squeezed below 400px; right pane soft-grows |
| Live tool line | `src/components/lobe-chat/AgentActivity.tsx` | 1–50 | Only **latest running** tool title; no stack |
| Timeline tool row | `src/components/lobe-chat/TimelineToolRow.tsx` | 18–154 | One-line shell/edit/read summary; quiet fail mark; ≥3 context tools collapsible |
| Tool kind registry | `src/lib/toolDisplay.ts` | 41–136 | bash/shell/terminal → `bash`; summary prefers first detail line for shell |
| Tool detail tail | `src/lib/toolDisplay.ts` | 138–148 | Expanded activity uses **last N lines** (default 8) — virtualization-friendly |
| Session changes | `src/lib/sessionChanges.ts` | 10–218 | Upsert by path from edit tools; newest first; not a permanent card stack |
| Unified diff helper | `src/lib/sessionChanges.ts` | 224+ | Client Myers-ish unified diff for pane |
| Resource pane modes | `src/components/ResourceViewer.tsx` | 105–109, 150, 279+ | `SideMode = files \| changes \| plan`; `ResourceOpenTarget` includes `changes` |
| Workspace Git model | `src/lib/workspaceGit.ts` | 1–200 | Porcelain kind classification; filter; badge letters; path normalize |
| Diff load cascade | `ResourceViewer.tsx` | 452–595 | payload before/after → `git_file_diff` → HEAD+after → after-only |
| Workspace status load | `ResourceViewer.tsx` | 400–431 | Prefetch `git_status` on project change; non-git drops Changes mode |
| Host Git commands | `src-tauri/src/commands.rs` | ~4069–608 | `git_file_diff`, `git_status`, `git_show_file`, worktree list/add/remove/gc |
| FE Git API | `src/lib/api.ts` | ~444–608 | Typed invokes for status/diff/show/worktrees |
| Permission bar | `src/App.tsx` | ~12281–12350 | Above composer; title + summary + optional preview; once / session / deny |
| Multi-session park | `src/App.tsx` | ~3304–3344 | Pending permission parked by session; bar only if viewing session |
| Escape priority | `src/lib/escapeStop.ts` | (C3 research) | Overlay → permission → menus → stop generation |

**Grok App terminal model (fact):** shell is a **tool kind**, not an OS terminal panel. There is no xterm/PTY React surface under `src/`. Interactive free-form shells are **out of scope of the pinned baseline** and must be designed as a Pillow product surface.

**Grok App Git model (fact):**

```text
Agent edit tools → sessionChangesById[session]
Project open     → git_status → workspaceFiles + branch
User opens Changes → ResourceViewer sideMode=changes
  section: session changes
  section: workspace git files
  select path → loadChangeDiff / loadWorkspaceDiff
Worktrees        → composer/project chrome (git_worktrees_*), not a permanent dashboard
```

### 2.3 zbbody — conceptual patterns and license

**Pin**: `<workspace>\zbbody` @ `00c29341…`
**License**: Apache-2.0 (`LICENSE`, package `"license": "Apache-2.0"`, SPDX headers on UI files).
**Reuse ceiling**: **conceptual-only** (matches C2/C3 decision). No line-by-line UI port without a later provenance brief + `THIRD_PARTY_NOTICES.md`.

| Topic | Path | Interaction lesson |
|-------|------|--------------------|
| Command progressive disclosure | `…/Messages/components/MessageToolCall.tsx` | Status badge + one-line name/description; expand Input/Output only on demand |
| Normalize command | `…/common/chat/normalizeToolCall.ts` | `exec` confirmation surfaces **command** as description |
| Tool category summary | `…/Messages/components/MessageToolGroupSummary.tsx` | Maps `run_terminal_command` / `Bash` → **Command ×n** in collapsed header |
| Turn process fold | `…/MessageTurnProcessGroup/*` | Finished turn folds process steps; final answer stays visible |
| File changes → preview | `…/MessageFileChanges.tsx`, `FileChangesPanel.tsx` | Open file vs open diff as distinct actions |
| Workspace changes | `…/Workspace/hooks/useFileChanges.ts` | Staged/unstaged list with expand — **do not** import workspace-string authority |
| Reject | Session inspector KPIs, always-resident plan rail | Dashboard clutter vs Pillow C2 “no permanent card stack” |

zbbody ships `@xterm/headless` for non-UI plumbing; it is **not** a product interactive terminal workbench to clone.

### 2.4 Layout targets and keyboard baseline

| Constraint | Source | C4 implication |
|------------|--------|----------------|
| 1024×720 | C3 real desktop acceptance screenshots | Resource + terminal surfaces must leave conversation ≥ ~60% width; collapse secondary chrome |
| 1440×960 | C3 acceptance | Comfortable dual-pane: chat + open resource/terminal |
| Chat min width | Grok `MAIN_CHAT_MIN_WIDTH=400` | Keep; refuse auto-grow that collapses chat |
| Resource default closed | Pillow C2-C1 + Workbench toggle | Terminal/Git modes inherit closed-by-default |
| Keyboard | C3 WriteApprovalBar + Grok perm bar | Esc deny; Tab trap; primary approve focus; disclosure Enter/Space via native `<details>` |
| Output volume | Grok `toolDetailTail`, Pillow truncation notices | Never dump unbounded raw logs into timeline DOM |

---

## 3. Interaction architecture (recommended)

### 3.1 Ownership hierarchy (UI authority)

```text
Project (selected)
  └── Conversation (selected, visual centre)
        ├── Timeline messages
        ├── Run / Agent activity disclosure  ← one-shot command summaries live here
        ├── Approval bar (0..1 pending)      ← terminal + Git mutations when required
        └── Composer
  └── Contextual resource pane (collapsible)
        ├── Files / search / recents / preview   (C2)
        ├── Write proposal diff                 (C3)
        ├── Git: branch + status + file list    (C4)
        └── Terminal output / live log host     (C4, optional mode)
```

**Rule:** Domain state remains SQLite / host (not renderer JSON). UI only projects conversation-owned DTOs. Renderer never invents tools, commands, or Git truth.

### 3.2 Surface map (what goes where)

| Content | Primary surface | Secondary surface | Forbidden |
|---------|-----------------|-------------------|-----------|
| One-shot command summary (`pnpm test` exit 1, 12s) | Owning conversation timeline row | — | Permanent card stack of successful runs |
| Live single running command | Mid-stream one-line (RunActivity / live edge) | — | Multi-line log dump mid-stream |
| Full / streaming stdout+stderr | Contextual terminal pane (opened on demand) | Nested `<details>` for small capped tail only | Global always-on IDE terminal across projects |
| Test results / artifacts | Nested activity disclosure → open portable path | Resource preview | Raw absolute paths |
| Git branch badge | Resource pane header when Git mode open | Optional quiet subtitle in stage header only if non-dashboard | Permanent multi-metric status strip |
| Git status list + file diff | Resource pane Git/Changes mode | Timeline link “查看变更” | Separate Git dashboard column |
| Manual free-form shell input | Contextual terminal pane (origin=user) | — | Provider-owned unrestricted shell |
| Approval for mutations | Single bar above composer | Review opens resource pane | Stacked approval cards; YOLO / allow-session as default |

### 3.3 State-flow diagrams

#### A. Agent-proposed one-shot command (conversation-owned)

```text
Provider proposes shell tool
  → Host policy: allow-read-only vs requires_approval vs deny
  → If requires_approval:
        emit pending proposal DTO (command summary, cwd portable, risk class)
        UI: WriteApprovalBar-shaped TerminalApprovalBar
        Approve once → host executes under project root
        Reject / Esc / expiry → terminal receipt cancelled/expired
  → If auto-safe (read-only policy — product gate, not UI):
        host executes; no bar
  → Runtime events stream to owning conversation/run
  → Timeline: summary line (status · short command · exit/duration)
  → User expands:
        level-1: last N output lines + truncation notice
        level-2: "打开完整输出" → contextual terminal pane (virtualized)
        level-3: artifacts / tests open via portable paths into resource preview
  → Terminal status: completed | failed | cancelled | interrupted
        normalizeTerminalActivity closes child "running" rows
```

#### B. Contextual terminal pane open / collapse / focus

```text
Triggers to open (any one):
  1. User clicks "完整输出" / "在终端查看" on a command summary
  2. User clicks stage resource chrome → Terminal mode (manual)
  3. Approval bar "查看命令" for a pending terminal proposal (read-only preview of sealed argv)

Open rules:
  - Pane is conversation-scoped AND project-scoped
  - Opening Terminal mode does not invent a second left rail
  - At 1024×720: prefer bottom-of-stage strip (max ~40% height) OR replace
    resource-pane body; never both full-height Git tree AND full terminal
  - At 1440×960: resource pane may host Terminal | Git | Files tabs
  - Focus moves to terminal viewport (or read-only log region) only when opened
    by explicit user action; auto-open mid-stream is forbidden by default

Collapse rules:
  - Toggle close clears live selection but retains durable timeline summary
  - Closing pane does not cancel a running command (cancel is explicit)
  - Project switch: hard close pane + invalidate generation (C2/C3 pattern)
  - Conversation switch: hard close pane; background run continues in host;
    reopening conversation restores summary, not stolen focus unless user opens

Focus retention:
  - While terminal pane focused, Esc first collapses pane (or stops input
    focus) before global stop-generation — match escape priority chain
  - While approval bar owns focus, Esc = reject (C3), not terminal close
```

#### C. Progressive disclosure ladder

```text
L0 Compact summary (always in timeline when event exists)
    role · command short · status · duration/exit
L1 Nested details (default closed for success; open for running/fail optional)
    last ≤ N lines (recommend N=24 virtualized or 8–24 capped)
    truncation notice if omitted
L2 Contextual terminal pane
    virtualized full log; copy redacted selection; cancel/retry controls
L3 Linked resources
    portable artifact paths → ResourcePreview
    test summary rows → expand or open log path
L4 Git evidence
    "查看工作区变更" → resource pane Git mode focused on touched paths
```

Successful completed commands collapse to **one concise disclosure** (C2-C2 `conciseSuccess`). Failed/cancelled remain visible with quiet risk mark.

#### D. Manual terminal vs Agent-proposed

| Dimension | Agent-proposed | Manual (user) |
|-----------|----------------|---------------|
| Origin badge | `代理` / role label from activity | `用户` |
| Summary owner | Run/tool under conversation run | Conversation-owned “manual terminal” activity OR project-scoped session still **projected into selected conversation** as an explicit user step |
| Approval | Policy-driven bar when mutation/risk | Optional confirm for high-risk templates; free-form still host-sandbox |
| Visual chrome | Timeline tool row style (quiet) | Same summary shape + distinct origin chip (not a second color theme system) |
| Output pane | Same contextual terminal surface | Same surface with `origin=user` |
| Never | Provider-looking card that hides origin | Permanent global terminal tab disconnected from conversation |

**Decision:** One terminal surface component; **origin is a first-class DTO field**, not two separate UIs.

#### E. Git placement (resource pane)

Extend workbench resource host modes:

```text
Side modes (conceptual):
  files     — C2 tree / search / recents / preview
  changes   — C3 write-diff OR C4 Git status/diff (mutually exclusive body)
  terminal  — C4 live/history log host (optional tab)
```

Git body (read-first C4 UI, mutations behind approval):

1. Header: **branch name** (display-safe), dirty count (number only), refresh.
2. Sections (Grok-inspired, conversation-safe):
   - **This conversation** — files touched by agent tools / write proposals in the selected conversation.
   - **Working tree** — project-scoped status list (kinds M/A/D/U/… as badges).
3. Select path → host-built unified diff (never renderer `git` spawn).
4. Empty / non-repo: opaque “未检测到 Git 仓库” without native roots.
5. Mutations (stage/commit/checkout/restore/publish) are **later capability slices** still projected through approval bar; this research places **evidence** only.

Do **not** place Git worktree management as a permanent sidebar dashboard (Grok’s worktree menus are optional advanced chrome; C4 default is status+diff only).

#### F. Approval bar content (terminal + Git)

Reuse `WriteApprovalBar` layout contract; specialize fields:

| Field | Terminal mutation | Git mutation |
|-------|-------------------|--------------|
| Title | `命令批准` | `Git 批准` |
| Primary identity | Short command (redacted) | Action + branch / path count |
| Mode / risk | e.g. `执行 · 写入风险` | e.g. `提交` / `切换分支` / `还原` |
| Expiry | Same deadline UX as C3 | Same |
| Summary | Cwd as portable relative or omit; no absolute path | Repo-relative paths only; file count |
| Actions | 批准一次 · 拒绝 · 查看命令 | 批准一次 · 拒绝 · 查看差异 |
| Grant | Memory-only opaque grant (C3) | Same |
| Forbidden | Allow for session as product default; raw argv with secrets; full environment dump |

Review actions:

- **查看命令** → open terminal pane in **read-only sealed preview** (no execute).
- **查看差异** → open resource Git/write diff mode (C3 path).

Only **one** pending bar globally for the selected conversation (oldest pending or host-selected, matching C3 `selectOldestPendingProposal`).

#### G. Project / conversation / restart / terminal-ended

| Event | Live terminal pane | Timeline summary | Approval bar | Running process (host, not UI) |
|-------|--------------------|------------------|--------------|--------------------------------|
| Conversation switch | Close + drop selection | Restored from durable events when re-entered | Clear local grant map; re-list for new conversation | Continues if host allows background; no focus steal |
| Project switch | Close + generation++ (C2/C3) | Not shown (wrong project) | Clear all grants | Cancel or park per process policy (grok1); UI shows interrupted if cancelled |
| App restart | Closed | Recover durable receipts; non-terminal → interrupted (C3 write precedent) | No auto-approve; pending invalidated | Host reaps orphans; UI never claims running without host proof |
| Terminal ended (exit) | Pane may stay open on last output; mark status chip | Summary becomes terminal status | N/A | N/A |
| Terminal cancelled | Status cancelled; output retained if sealed | Quiet risk mark | Pending cleared if cancel during wait | Host kill path (grok1) |

#### H. Redaction, secrets, native paths, raw IDs, copy

| Class | Display | Copy behavior |
|-------|---------|---------------|
| Portable relative path | Allowed in Git list / artifacts | Copy relative path |
| Native absolute path | Never in product UI (C2 opaque) | Copy button either disabled or copies portable only |
| `sessionId` / `runId` / `toolCallId` / `proposalId` | Never rendered; React keys only | Not copyable from UI |
| `approvalGrant` | Never | Never |
| Env secrets / tokens in output | Host redaction stream preferred; UI applies secondary mask for known patterns | Copy uses **currently displayed redacted** text |
| Full raw provider wire JSON | Never | Never |
| Command argv | Summary may show redacted command line | Copy command uses host-provided redacted string |

Projection already has patterns in `agentActivityProjection.ts` (`OPAQUE_ERROR_MESSAGE`, portable artifacts, forbidden ID strip). C4 terminal output projection must plug into the same sanitizers.

#### I. Loading / cancellation / failure / conflict / stale-diff

| State | Timeline | Terminal pane | Resource Git |
|-------|----------|---------------|--------------|
| Loading | Live status “运行中” + one-line | Spinner / “正在加载输出…” | “正在刷新状态…” |
| Streaming | Live edge one line; no multi-card | Append virtualized; autoscroll only if user at bottom | N/A |
| Cancel in flight | Status cancelling → cancelled | Freeze buffer; mark cancelled | Refresh may show partial file changes |
| Failure (non-zero exit) | Risk mark + exit code in summary | Full log available via L2 | Unchanged unless tools wrote files |
| Approval expired | Terminal outcome expired (path-free) | Closed review | Diff target cleared (C3 expiry) |
| Base conflict (write) | C3 receipt conflict | N/A | Diff shows stale; re-list |
| Stale Git diff | — | — | Generation guard: discard if project/conversation/refresh epoch changed; show “状态已过期，请刷新” |
| Concurrent edit | — | — | Selecting path re-fetches host diff; never trust client-only patch cache as truth |

---

## 4. Required decisions (locked for design handoff)

### D1 — Where one-shot command summaries appear

**In the owning conversation timeline**, as part of run/agent activity progressive disclosure (`RunActivity` / `AgentActivityDisclosure` lineage), not as a global Tasks dashboard.

Summary line shape:

```text
[origin chip] [status]  short-command  ·  exit/duration
```

Mid-stream: **one** live running command line at stream edge (Grok `LiveToolText` conceptual).

### D2 — Contextual terminal pane open / collapse / focus

- **Default closed.**
- Opens only from explicit user action (summary link, resource chrome Terminal mode, approval review).
- Hosted as a **mode of the right contextual pane** at ≥1440 width; as a **bottom-of-stage collapsible strip** (≤40% height) at 1024×720 when dual full panes would violate chat dominance.
- Closing pane ≠ cancelling process.
- Scope generation invalidates async output loads (C2/C3).

### D3 — Progressive disclosure

Four levels L0–L3 in §3.3.C. Nested tool output remains capped; full log only in virtualized terminal surface.

### D4 — Manual vs Agent-proposed visual distinction

Single component family; **origin field** drives chip (`代理` / `用户`). Same status colors and disclosure chrome. No separate “Terminal app” brand panel.

### D5 — Git placement

**Existing resource pane**, new mode conceptually aligned with Grok `changes` (`files | changes | terminal`). Branch + status list + on-demand host diff. Conversation-touched files section first, working tree second. No permanent Git dashboard.

### D6 — Approval bar for terminal and Git

C3 bar contract generalized:

- One pending proposal for selected conversation.
- Actions: approve once · reject · review (command or diff).
- Esc = reject when bar owns focus.
- Grant memory-only; never allow-session as default product path for C4 mutations.

### D7 — Switch / restart / ended

§3.3.G. Conversation switch parks UI; project switch hard-clears; restart invalidates non-terminal authority; ended runs keep durable summaries.

### D8 — Redaction and copy

§3.3.H. Displayed text is the only copy source; portable paths only; opaque errors; no raw IDs/grants/native roots.

### D9 — Loading / cancel / fail / conflict / stale

§3.3.I. Generation counters + host re-fetch; stale UI shows refresh, never silent wrong diff.

### D10 — No second dashboard

Reject:

- Always-on multi-session terminal strip.
- Permanent KPI rail (duration/steps/tokens) beside chat (zbbody Session Inspector anti-pattern).
- Stack of successful shell cards.
- Global Tasks page as primary command surface.
- Git worktree manager as home chrome.

---

## 5. UI acceptance scenarios and screenshots

### 5.1 Structural acceptance (must pass)

| ID | Scenario | Expected |
|----|----------|----------|
| T1 | Agent command completes successfully | One concise timeline disclosure; no permanent card; optional nested tail closed |
| T2 | Agent command fails (exit ≠ 0) | Visible failed summary; expand shows capped tail; “完整输出” opens terminal pane |
| T3 | Streaming command mid-run | Exactly one live running line; no multi-tool stack |
| T4 | Open terminal pane from summary | Conversation remains centre; pane/strip open; focus in output region |
| T5 | Collapse terminal pane | Summary remains; process not cancelled |
| T6 | Manual command origin | Origin chip `用户`; same disclosure family |
| T7 | Terminal approval required | Single bar above composer; Esc rejects; grant never visible |
| T8 | Review command from bar | Read-only sealed command view; no execute |
| T9 | Git status in resource pane | Branch + list; portable paths; no native root |
| T10 | Open Git file diff | Host unified diff in pane; chat min width preserved |
| T11 | Conversation switch during live command | Pane closes; no approval/diff/output leak into other conversation |
| T12 | Project switch during live command | Pane closes; generation invalidate; no cross-project output |
| T13 | Restart with non-terminal command | UI shows interrupted/recovered durable state; no auto re-approve |
| T14 | Cancel running command | Status cancelled; output retained if sealed; no running child rows |
| T15 | Stale Git refresh | Epoch mismatch discards old diff; empty/error opaque |
| T16 | Secret-looking output | Redacted display; copy matches display |
| T17 | 1024×720 with terminal open | No horizontal page scroll; chat usable; no dual full panes fighting |
| T18 | 1440×960 with Git + timeline | Conversation dominant; resource mode Git; one approval bar max |
| T19 | Keyboard | Tab trap in approval bar; Esc priority; disclosure toggle via summary/details |
| T20 | Output virtualization | Large log (e.g. ≥5k lines fixture) does not freeze UI; only viewport rows mounted |

### 5.2 Screenshot matrix (desktop)

Capture at **deviceScaleFactor 1**, Windows sizes:

| Size | Required captures |
|------|-------------------|
| **1024×720** | (a) command summary only; (b) terminal strip open + failed command; (c) Git mode open + conversation dominant; (d) approval bar for terminal |
| **1440×960** | (a) live running one-line; (b) terminal mode in resource pane with virtualized output; (c) Git status + unified diff; (d) manual vs agent origin chips side-by-side in one conversation; (e) approval bar + review command |

**Pass criteria (shared with C1/C3 visual gates):**

- Conversation is visual centre; no dashboard/metric card grid.
- At most one left project/conversation rail.
- At most one approval bar.
- Resource/terminal are contextual, collapsible, and project-scoped.
- No native roots, grants, or raw session/run IDs in pixels.
- Document does not scroll as a whole; stage body owns overflow.

**Suggested artifact paths (implementation phase):**

```text
artifacts/wave-c4-visual/
  cmd-summary-1024x720.png
  terminal-open-fail-1024x720.png
  git-mode-1024x720.png
  terminal-approval-1024x720.png
  live-run-1440x960.png
  terminal-virtualized-1440x960.png
  git-diff-1440x960.png
  origin-chips-1440x960.png
  terminal-approval-review-1440x960.png
  report.json
```

### 5.3 Test seams (for later implementation)

- Pure projectors for command summary DTOs (IDs stripped).
- Terminal pane generation counters (projectId + conversationId + outputEpoch).
- Virtualized list unit tests with 10k line fixture.
- Approval bar keyboard tests (reuse C3 patterns).
- E2E: open/close pane, switch conversation isolation, screenshot matrix.
- Protocol fixtures for command receipts (owned by process/governance research).

---

## 6. Provenance and reuse decisions

| Source | Allowed now | Not allowed |
|--------|-------------|-------------|
| Pillow C1–C3 | Extend patterns, CSS prefixes, scope guards | Regress conversation dominance or opaque path rules |
| Grok App MIT `cbbbdf17…` | Conceptual: timeline tools, Changes mode, layout floors, permission bar | Whole App.tsx; YOLO; allow-session default; raw path display habits; worktree manager as home |
| zbbody Apache-2.0 `00c29341…` | Conceptual: command expand, category summary, turn fold | Source copy; permanent inspector KPI rail; workspace-string authority; YOLO ids |

Any later **source adaptation** requires commit, path, license notice, and `THIRD_PARTY_NOTICES.md` entry (upstream-baseline policy).

---

## 7. Explicit exclusions (interaction layer)

- Permanent multi-tab IDE terminal dashboard unbound from conversation.
- Provider self-approval of shell or Git.
- Rendering full unredacted logs in the timeline by default.
- Competitive Git “source control” product clone (commit UI may come later under approval).
- Network tools, credentials, Obsidian, company ops.
- Implementation of process kill trees or Git CAS (other C4-R slots).

---

## 8. Residual risks

1. **No pinned Grok PTY panel** — Pillow must invent a contextual terminal surface; risk of over-building a second IDE. Mitigate with L0–L2 disclosure and closed-by-default pane.
2. **1024×720 dual-pane pressure** — Git + terminal + chat can collide; require exclusive body mode or height-capped strip (T17).
3. **Background runs after conversation switch** — UX may surprise users if host keeps processes; status must rehydrate without focus steal (depends on grok1 process policy).
4. **Output virtualization + WebView2** — large logs need measurement against desktop smoke; budget is a release gate per AGENTS.md.
5. **Manual terminal identity** — if manual sessions are project-global, projecting them into “selected conversation” needs a durable ownership rule in design (not invented here as SQLite schema).
6. **Parallel research slots** — process lifecycle (grok1) and Git governance (grok2) may constrain which approvals and which Git actions exist; this document places UI only. Controller must reconcile if process policy forbids background after switch.

---

## 9. Controller handoff notes

Design doc for C4 should lock:

1. DTO for `CommandActivity` / `TerminalOutputChunk` / `GitStatusView` with portable paths only.
2. Resource pane mode enum including Git + Terminal.
3. Approval proposal kinds: `terminal.execute`, `git.*` (exact set from grok2).
4. Escape priority chain including terminal focus.
5. Screenshot + E2E matrix from §5 as acceptance gates.
6. No implementation until explicit user authorization (C4-R research only).

---

## Completion

**Status: DONE**

**Changed files (truth):**

- Created: `docs/research/2026-08-02-c4-terminal-git-interaction-research.md`

**No other files modified.** No stage, commit, source, test, or vault mutation.

**Residual risks:** see §8 (especially no Grok PTY baseline; layout collision at 1024×720; background process UX depends on process research).

**Keeper signal:**
`completion | pillow/c4-terminal-git-interaction-research | DONE — conversation-owned command summaries + contextual terminal pane + Git in resource pane; progressive disclosure; C3 approval bar extension; 1024x720/1440x960 acceptance matrix; Grok App cbbbdf17 and zbbody 00c29341 conceptual only; no implementation.`
