# Keeper IDE Stage 1 Plan Comparison

Date: 2026-07-30

Status: Controller-approved execution baseline

## Product Goal

Keeper IDE first becomes an industrial, local-first AI IDE. Obsidian
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
| Runtime | Adapters behind neutral contract | Not implemented | Slice B1/B2 |
| Conversation | Persistent, streaming, cancellable | Visual placeholders only | Slice B1 |
| Files/terminal/Git | Contextual IDE resources | Stub resource pane only | Slice C |
| Knowledge | Obsidian after IDE | Not implemented | Correctly deferred |
| Company OS | Project teams and orchestration | Not implemented | Correctly deferred |

## Completed Baseline

Slice A provides:

- clean Tauri/Rust/React workspace;
- native Windows release without a console window;
- SQLite project authority and canonical-root deduplication;
- native folder picker and scoped project directory creation;
- always-visible project shell;
- deterministic renderer, Rust, release and startup gates;
- pinned MIT provenance for adapted Grok App surfaces.

Slice A does not provide:

- a conversation database;
- a runtime-neutral event protocol;
- a child-process host;
- a real Grok/Codex/OpenCode connection;
- message streaming, cancellation, recovery or tool activity;
- a real file tree, terminal or Git surface.

## Corrected Delivery Sequence

### Slice B1: Conversation Kernel Vertical Slice

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

Replace the fake adapter at the runtime boundary:

- probe Grok Build installation and authentication;
- start/resume ACP session;
- normalize streaming, tool, plan and sub-agent events;
- cancel the process tree;
- recover or mark interrupted after restart;
- produce redacted diagnostics.

### Slice C: IDE Resources

Add file tree, editor, terminal, Git diff, artifacts and permission bar only
after a real conversation can run and recover.

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

