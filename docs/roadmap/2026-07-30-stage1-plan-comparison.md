# Pillow Stage 1 Plan Comparison

> 本文件已被 `docs/roadmap/2026-07-31-stage1-overall-status.md` 取代。

Date: 2026-07-30

Status: Historical comparison; current state moved to
`2026-07-31-stage1-overall-status.md`

## Product Goal

Pillow first becomes an industrial, local-first AI IDE. Obsidian
integration begins only after the IDE passes Stage 1 acceptance. One-person
company orchestration remains the third stage.

## Plan Versus Reality

| Area | Approved plan | Current repository truth | Decision |
| --- | --- | --- | --- |
| Repository | Clean successor, legacy frozen | New independent repository exists; legacy is untouched | Keep |
| Desktop | Tauri 2 + React | Release binary and native smoke pass | Keep |
| Project entry | Blocking project launcher | Always-visible Grok-style shell with native open/create | Update plan |
| Persistence | SQLite authority | `rusqlite`, migration 0001, canonical project roots | Keep |
| UI baseline | Grok App-derived | Pinned Grok light shell, single sidebar and floating composer | Accept |
| Runtime | Adapters behind neutral contract | Grok ACP adapter and the C1 provider-to-B3 governed read-only loop accepted | Keep read-only boundary; expand runtimes only in Slice D |
| Conversation | Persistent, streaming, cancellable | Project-scoped SQLite conversation, cancellation, recovery and timeline projection accepted | Keep |
| Files/terminal/Git | Contextual IDE resources | C2 read-only core/bridge plus unmounted resource and Agent-activity renderer slices accepted | Finish preview and product wiring; terminal and Git stay deferred |
| Knowledge | Obsidian after IDE | Not implemented | Correctly deferred |
| Company OS | Project teams and orchestration | Not implemented | Correctly deferred |

## Historical Slice A Baseline

Slice A provides:

- clean Tauri/Rust/React workspace;
- native Windows release without a console window;
- SQLite project authority and canonical-root deduplication;
- native folder picker and scoped project directory creation;
- always-visible project shell;
- deterministic renderer, Rust, release and startup gates;
- pinned MIT provenance for adapted Grok App surfaces.

At Slice A acceptance, the repository did not yet provide:

- a conversation database;
- a runtime-neutral event protocol;
- a child-process host;
- a real Grok/Codex/OpenCode connection;
- message streaming, cancellation, recovery or tool activity;
- a real file tree, terminal or Git surface.

## Corrected Delivery Sequence

### Slice B1: Conversation Kernel Vertical Slice

Status: accepted.

Deliver one complete deterministic path:

```text
project
  -> create conversation
  -> send turn
  -> fake runtime emits normalized events
  -> SQLite persists messages, run and ordered events
  -> UI streams timeline
  -> cancel or complete
  -> restart and restore
```

The fake adapter is a test instrument, not a product runtime.

### Slice B2: Real Grok ACP

Status: accepted.

Replace the fake adapter at the runtime boundary:

- probe Grok Build installation and authentication;
- start/resume ACP session;
- normalize streaming, tool, plan and sub-agent events;
- cancel the process tree;
- recover or mark interrupted after restart;
- produce redacted diagnostics.

### Slice C: IDE Resources

The original slice name is now executed as staged C2 work. C2 begins only with
file tree, search, recent files, read-only editor and contextual previews after
the accepted C1 conversation and governed-read baseline. Terminal, Git,
write/diff approval and additional runtime work remain later boundaries.

Status: in progress. C2-A resource core, C2-B bridge, C2-C1 resource panel, and
C2-C2 Agent activity disclosure have focused acceptance. Preview, mounting,
real projection wiring, project-switch/restart E2E, full gates, and independent
review remain before checkpoint.

### Slice D: Runtime Expansion

Add Codex and OpenCode through the same contract. No hidden provider fallback.

### Slice E: Industrial Hardening

Importer, packaging, updater, security review, performance distribution,
crash recovery, diagnostics and public alpha documentation.

## Controller Gates

No slice advances on screenshots alone. Each gate requires:

1. domain and persistence tests;
2. protocol fixture tests;
3. renderer behavior tests;
4. real desktop smoke where native boundaries change;
5. process cleanup evidence for runtime work;
6. a committed report with residual risks;
7. independent spec and code review.
