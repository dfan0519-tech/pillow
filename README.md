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

**Slice B2 (real Grok ACP adapter) is in progress.** Live provider execution
behind the accepted B1 protocol boundary is the active wave. B1 must not be
cited as Grok connectivity evidence.

## Design Authority

- `docs/superpowers/specs/2026-07-29-keeper-ide-first-design.md`
- `docs/migration/legacy-capability-inventory.md`
- `docs/references/upstream-baseline.md`
- `docs/decisions/ADR-0001-clean-successor.md`
