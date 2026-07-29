# Keeper IDE-First Clean Successor Design

Date: 2026-07-29

Status: Approved direction; implementation planning gate

Owner: Keeper controller

## 1. Product Decision

Keeper will be rebuilt as an industrial, local-first AI IDE.

The product sequence is locked:

```text
Stage 1: Industrial AI IDE
Stage 2: Governed Obsidian knowledge integration
Stage 3: One-person-company project operating system
```

Stage 1 must be independently useful. It cannot depend on a vault, autonomous
company roles, or the legacy Keeper service.

## 2. Primary User Journey

```text
launch Keeper IDE
  -> open or create a project workspace
  -> choose or resume an Agent conversation
  -> ask for work
  -> watch the plan, tools, files, terminal and sub-agent activity
  -> approve consequential operations
  -> inspect diffs and artifacts
  -> accept, revert or continue
  -> close and later resume without losing state
```

The project is always visible. A conversation, run, terminal, diff, artifact,
or approval cannot exist without a project owner.

## 3. Stage 1 Scope

### 3.1 Workspace

- open an existing local folder;
- initialize a new folder;
- trust and permission decision on first open;
- file tree with search and recent files;
- text editor with syntax highlighting;
- integrated terminal;
- Git status, diff, branch, worktree, commit and restore surfaces;
- artifact preview for Markdown, images, video, PDF and common documents.

### 3.2 Agent Workbench

- persistent conversations grouped by project;
- streaming messages and tool activity;
- cancel, interrupt, follow-up queue, retry, fork and archive;
- plan and task progress;
- sub-agent collaboration displayed inside the owning conversation;
- per-run files changed, commands, tests, artifacts and completion report;
- context and model information shown through progressive disclosure.

### 3.3 Runtime Connections

The first accepted adapters are:

1. Grok Build through ACP;
2. Codex through its app-server or supported stdio protocol;
3. OpenCode through its server/SDK surface.

The IDE owns project, conversation, permission, and audit state. A connected
runtime owns model inference and runtime-specific tool execution only.

### 3.4 Permissions And Safety

- Ask is the default permission mode;
- read-only operations may be allowed by project policy;
- write, process, network, credential, delete, publish and destructive Git
  operations are classified separately;
- approval decisions are scoped to one action, session, project, or explicit
  policy rule;
- secret values never enter renderer state, logs, model context, or SQLite;
- process launch uses an allow-listed environment;
- every write-capable tool emits an auditable receipt.

### 3.5 Persistence

SQLite is the only operational authority.

Required durable objects:

- project;
- workspace;
- conversation;
- message;
- turn;
- run;
- run event;
- tool call;
- approval;
- artifact;
- file change;
- terminal session metadata;
- runtime connection;
- resource health observation;
- audit record.

Secrets are stored in the operating-system credential store and referenced by
opaque IDs.

## 4. Explicit Non-Goals

Stage 1 will not:

- integrate Obsidian;
- operate a company or business workflow;
- autonomously discover and control unrelated projects;
- reproduce every VS Code feature;
- implement a proprietary model gateway;
- scrape credentials from other applications;
- expose internal scheduler records as primary navigation;
- preserve legacy route or JSON compatibility inside the new core;
- use cinematic dashboards, particle effects, or decorative AI faces;
- depend on the old Keeper service at runtime.

## 5. Interaction Architecture

The initial visual and interaction baseline follows Grok App.

### 5.1 Desktop Layout

```text
+----------------------+-----------------------------------------------+
| Projects             | Active project / branch / runtime            |
|  project A           +-----------------------------------------------+
|  project B           | Conversation timeline                         |
|                      |  user message                                 |
| Conversations        |  agent reasoning summary                      |
|  current             |  plan and sub-agent activity                  |
|  recent              |  tool calls and approvals                     |
|  archived            |  response                                     |
|                      +-----------------------------------------------+
| Settings             | Composer / attachments / mode / send          |
+----------------------+-----------------------------------------------+
                         Contextual resource pane: files/diff/terminal
```

The resource pane is contextual and collapsible. It must not permanently take
space from the conversation when no resource is open.

### 5.2 UI Baseline Rules

- Default theme is neutral light.
- Dark theme is supported but not the design source of truth.
- The conversation is the dominant surface.
- Borders are restrained and communicate structure, not decoration.
- Status color is reserved for state and risk.
- Runtime names, IDs, tokens, and route metadata are secondary.
- Project and conversation labels use human-readable names.
- Long histories are virtualized.
- Streaming updates do not shift the composer or resize navigation.
- All essential workflows are keyboard accessible.

### 5.3 UI Reuse Boundary

Grok App components may be adapted under its MIT license when they materially
reduce risk. Every adapted file must:

- retain required copyright and license notice;
- record original repository, commit and source path;
- be isolated from Keeper domain state;
- pass Keeper accessibility and Windows geometry gates;
- use Keeper naming and icons only after adaptation.

No Grok branding, account-specific behavior, or runtime assumptions enter the
Keeper domain.

## 6. Technical Architecture

### 6.1 Technology

- Tauri 2 desktop shell;
- Rust 2024 kernel and runtime host;
- React 19 and TypeScript;
- Vite;
- SQLite with versioned migrations;
- SQLx or rusqlite selected during the first implementation plan;
- ACP for Grok Build;
- JSON-RPC/stdio transport for adapter protocols;
- MCP for external tools;
- OS keychain through a maintained Rust credential library;
- Vitest for renderer units;
- Rust unit and integration tests;
- Playwright for desktop-webview interaction tests.

### 6.2 Repository Shape

```text
keeper-ide/
  apps/
    desktop/                 Tauri application and React renderer
  crates/
    kernel-project/          projects and workspaces
    kernel-session/          conversations, turns and run lifecycle
    kernel-policy/           permissions and approvals
    kernel-audit/            append-only receipts
    kernel-resource/         runtime connection health
    storage-sqlite/          schema, migrations and repositories
    runtime-host/            child processes, cancellation and limits
    agent-protocol/          runtime-neutral events and commands
    adapter-grok-acp/        Grok Build adapter
    adapter-codex/           Codex adapter
    adapter-opencode/        OpenCode adapter
  packages/
    ui/                      reusable renderer primitives
    protocol/                generated TypeScript command/event types
  tests/
    fixtures/                protocol and legacy import fixtures
    desktop/                 end-to-end desktop scenarios
  docs/
```

Crates communicate through typed interfaces. The renderer cannot directly
open SQLite, launch processes, or read secrets.

### 6.3 Command And Event Flow

```text
React UI
  -> typed Tauri command
  -> application service
  -> policy decision
  -> domain transaction
  -> runtime adapter or local tool
  -> append-only event and receipt
  -> projected UI event
```

Commands are idempotent where retries are possible. Long-running work returns
a stable run ID and streams events. The host remains authoritative if the
renderer reloads.

### 6.4 Runtime Adapter Contract

Every adapter must provide:

- `probe`: installation, version and authentication health;
- `capabilities`: supported tools, media and session operations;
- `start_session`;
- `resume_session`;
- `send_turn`;
- `cancel_turn`;
- `stream_events`;
- `shutdown`;
- `export_diagnostics`.

Provider-specific payloads stay inside the adapter. The application receives a
normalized event vocabulary.

## 7. Reliability And Performance

### 7.1 Required Behavior

- crash-safe SQLite transactions;
- restart recovery for non-terminal runs;
- adapter heartbeat and stall detection;
- bounded process and session concurrency;
- cancellation with process-tree cleanup;
- renderer reload without terminating active streams;
- diagnostic bundles that exclude secrets;
- versioned data migrations with backup and rollback.

### 7.2 Initial Budgets

- cold launch to usable project launcher: at most 2.5 seconds on the target
  Windows workstation;
- resumed project shell visible: at most 1.5 seconds;
- conversation switch with cached data: at most 100 milliseconds;
- input response while streaming: no frame longer than 50 milliseconds;
- idle renderer CPU: under 2 percent on the target workstation;
- idle memory after one project: under 350 MB;
- 10,000-message conversation remains scrollable through virtualization.

Performance tests report measured values. A release cannot replace them with
subjective statements.

## 8. Security Model

Surface classes:

| Surface | Rule |
| --- | --- |
| Locked | policy engine, audit schema, migration history, release verification |
| Editable by Agent | trusted project files within the approved workspace |
| Append-only | run events, approvals, tool receipts, audit records |
| Human controlled | credentials, destructive operations, publish, deploy, final merge |

Project trust is not a sandbox. Runtime adapters must declare their actual
sandbox guarantees. The UI cannot claim isolation that the runtime does not
provide.

## 9. Migration Strategy

Migration uses five gates:

1. inventory and freeze legacy behavior;
2. implement new typed contract with failing successor tests;
3. port the minimum algorithm or rewrite it;
4. replay sanitized legacy fixtures against the new contract;
5. accept parity or explicitly retire the behavior.

The first import tool is read-only. It converts a copied legacy backup into a
new database and writes an import report. It never mutates the source.

## 10. Delivery Slices

### Slice A: Desktop Foundation

- clean monorepo and build;
- Tauri window and Grok App-based shell;
- project launcher;
- SQLite project records;
- logging, diagnostics and release metadata.

### Slice B: Real Agent Conversation

- runtime-neutral session model;
- Grok ACP adapter;
- streaming conversation;
- cancellation and restart recovery;
- tool event timeline.

### Slice C: IDE Resources

- file tree and editor;
- terminal;
- Git status and diff;
- contextual artifact previews;
- permission bar.

### Slice D: Runtime Expansion

- Codex adapter;
- OpenCode adapter;
- connection settings and health;
- routing chosen explicitly by the user, not hidden fallback.

### Slice E: Industrial Hardening

- import compatibility;
- updater and packaging;
- performance gates;
- security review;
- crash recovery and diagnostic bundle;
- public alpha documentation.

Obsidian work begins only after Slice E acceptance.

## 11. Stage 1 Acceptance

Stage 1 is complete only when a clean Windows installation can:

1. open two independent projects;
2. maintain isolated conversations in each;
3. execute a real task through at least Grok Build and Codex;
4. show the plan, tools, sub-agent activity, changes and completion evidence;
5. edit files, run a terminal and inspect Git diffs;
6. enforce approvals for consequential operations;
7. survive application and adapter restart without corrupting state;
8. package diagnostics without secrets;
9. import the retained legacy project/conversation subset from a backup;
10. pass unit, integration, desktop, security and performance gates.

## 12. Decisions Deferred To Implementation Planning

The first implementation plan must resolve:

- SQLx versus rusqlite;
- Monaco versus CodeMirror for the first editor;
- whether Grok App UI code is adapted directly or recreated component by
  component;
- exact Tauri capability allow-list;
- protocol code generation boundary;
- Windows signing and updater channel for public alpha.

These decisions do not change the product boundary in this specification.

