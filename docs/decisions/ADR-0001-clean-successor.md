# ADR-0001: Build Pillow As A Clean Successor

Date: 2026-07-29

Status: Accepted

## Context

The legacy Keeper evolved from a knowledge dashboard into a project studio,
agent harness, key manager, and desktop shell. Its main server and renderer
became monolithic, while JSON projections and compatibility routes accumulated
multiple meanings for projects, sessions, tasks, and runtime status.

Continuing to patch that implementation would preserve the wrong boundaries.

## Decision

Create `<repo>` as an independent Git repository.

The legacy repository is frozen as a read-only migration source. Pillow
will reimplement accepted behavior behind typed boundaries and use selected
legacy tests as conformance evidence.

The first product is an industrial local AI IDE. Obsidian integration and
one-person-company orchestration are later products built on top of the IDE
kernel.

## Consequences

- No legacy file is copied wholesale.
- SQLite is authoritative from the first executable milestone.
- Agent runtimes are adapters, not the application core.
- The initial UI follows Grok App's workbench interaction and visual baseline.
- Migration is incremental and reversible.
- The legacy service remains available until successor acceptance proves data
  import, feature parity for retained capabilities, and rollback.

