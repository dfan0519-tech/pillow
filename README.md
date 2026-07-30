# Keeper IDE

Keeper IDE is a local-first, project-scoped AI development environment.

The product roadmap is intentionally ordered:

1. build a reliable industrial AI IDE;
2. connect the IDE to a governed Obsidian knowledge layer;
3. extend the project model into a one-person-company operating system.

The legacy Keeper repository at `<workspace>/VaultSystem\keeper` is a read-only
migration source. New product code belongs in this repository.

## Current Status

**Slice B1 (conversation kernel) is accepted.** The desktop shell owns a
project-scoped conversation service with durable SQLite authority, a
provider-neutral runtime host, and the deterministic fake adapter path. Every
runtime event follows `validate -> persist -> commit -> emit`.

**Slice B2 (real Grok ACP adapter) is accepted.** The controlled live gate
proved plaintext Grok ACP execution, cancellation cleanup, provider-tool
shutdown, and public-error redaction behind the B1 protocol boundary.

**Wave B3 (governed read tool foundation) is active.** The next product
boundary is a Keeper-owned, project-scoped read tool gateway. Provider-native
tools, arbitrary MCP servers, file writes, and shell execution remain disabled.

## Design Authority

- `docs/superpowers/specs/2026-07-29-keeper-ide-first-design.md`
- `docs/migration/legacy-capability-inventory.md`
- `docs/references/upstream-baseline.md`
- `docs/decisions/ADR-0001-clean-successor.md`
