# ADR-0002: Separate Pillow From The Legacy Keeper

Date: 2026-07-30

Status: Accepted

## Context

The legacy Keeper service (out of tree; not part of this repository) still
manages shared knowledge, API key inventory, project registration, and Agent
coordination on maintainer machines that keep it. The new industrial IDE had
been developed under the temporary name `Keeper IDE`, which made product
ownership and project status ambiguous.

## Decision

The successor IDE is named **Pillow** and its canonical repository is this
repository (`pillow`).

The legacy Keeper remains an optional external system and may be used by Pillow
only through explicit governance, knowledge, and resource management interfaces.
Pillow builds and runs without it.

Public Pillow identity includes:

- product and window title: `Pillow`;
- npm workspace package: `@pillow/desktop`;
- Rust desktop package: `pillow-desktop`;
- desktop executable: `pillow`;
- Tauri identifier: `com.pillow.desktop`;
- environment prefix: `PILLOW_`.

Existing internal Rust crates with `keeper-*` package names are transitional
implementation identifiers. They do not import or modify the legacy Keeper
repository. Renaming them is deferred until a dedicated package-boundary wave
can preserve downstream compatibility and release history.

## Consequences

- Keeper and Pillow appear as separate projects in the project registry.
- New IDE work, releases, issues, and documentation use the Pillow name.
- Historical worktrees and commits remain valid evidence even when their
  filesystem paths retain the former temporary name.
- The legacy Keeper can continue operating while Pillow evolves independently.
