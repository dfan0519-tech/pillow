# ZBBody / AionUi Reference Analysis

Date: 2026-07-30

Reference repository: `zbbody reference clone (out of tree)`

Reference commit: `00c293414333b6fcc4e4598389fa3952c8d0b09b`

Upstream remote: `https://gitee.com/hwblack_admin/zbbody.git`

Declared license: Apache-2.0

Decision: use as a read-only interaction and lifecycle reference, not as the
Pillow application or security foundation.

## Executive Decision

The repository is a large AionUi-derived Electron application. It contains
useful, mature examples for conversation navigation, workspace panes, preview
dispatch, agent activity, runtime reconnect, and broad desktop testing.

Pillow must retain its existing Tauri/Rust boundaries:

- projects are durable domain objects rather than workspace strings attached
  to conversations;
- SQLite is the only operational authority;
- renderer access remains a narrow typed Tauri command and event surface;
- providers propose work but never grant themselves authority;
- tool execution, policy, approval, and receipts remain Pillow-owned.

No source file will be copied from this repository unless a later brief names
the exact source path, commit, license obligation, adaptation boundary, and
focused acceptance tests.

## Adopt As Design Patterns

1. A three-region workbench: compact project/conversation navigation, dominant
   conversation timeline, and a collapsible contextual resource pane.
2. Conversation-local plan, tool, sub-agent, file, terminal, and artifact
   activity with progressive disclosure.
3. Runtime snapshot plus ordered live-event reconciliation.
4. Workspace file tree, diff, and preview interaction patterns.
5. Explicit runtime lifecycle states, reconnect behavior, and visible
   cancellation.
6. Protocol fixtures, fake child processes, and opt-in live acceptance.

## Reject As Product Or Security Boundaries

1. Renderer discovery of a local HTTP/WebSocket backend port.
2. Generic string-based IPC channels in place of typed commands.
3. Provider-specific fields inside the core conversation schema.
4. JSON history, SQLite, and backend storage acting as competing authorities.
5. Renderer-defined executable, argv, environment, cwd, MCP URL, or headers.
6. YOLO, bypass-permission, or provider-defined always-allow decisions.
7. Runtime installation through drifting `@latest` packages.
8. Model-provided workspace paths used directly as execution roots.
9. Previewing model-generated `file://` HTML with permissive WebView settings.
10. Team modes as a primary navigation model for new users.

## Keeper Information Architecture

The durable ownership hierarchy remains:

```text
Project
  -> Conversation
    -> Turn
      -> Run
        -> activity, approval, file change, terminal, artifact, receipt
```

An agent or sub-agent belongs to a run. It is not an independent top-level
project or conversation.

## Immediate Consequences

- Slice B2 remains accepted and its provider profile continues to expose no
  native provider tools or MCP servers.
- The next slice begins with a project-bound path capability, not with an
  arbitrary MCP client or shell.
- The first real tools are Pillow-owned, read-only filesystem operations.
- Tool and approval events are durable before the renderer sees them.
- Existing activity UI will be evolved to preserve per-run history instead of
  replacing the previous run's structured activity.

## Local Repository Handling

`zbbody reference clone (out of tree)` is a reference repository under Keeper governance. Its
pre-existing worktree changes are not part of Pillow and must not be
cleaned, staged, committed, or reused implicitly.

