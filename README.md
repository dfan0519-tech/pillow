# Pillow

Pillow is a local-first, project-scoped AI development environment.

The product roadmap is intentionally ordered:

1. build a reliable industrial AI IDE;
2. connect the IDE to a governed Obsidian knowledge layer;
3. extend the project model into a one-person-company operating system.

Pillow is the clean successor to an earlier resource-manager codebase. New IDE
product code belongs in this repository. Optional external tools (session
recording, shared knowledge vaults, reference UI clones) may be used by
maintainers out of tree; they are **not** required to build, test, or run
Pillow from a public checkout.

## Build and check

Requires a recent Node.js/pnpm toolchain and a Rust toolchain suitable for the
workspace crates (including the Tauri desktop shell).

```bash
pnpm install
pnpm check
# or the worker-oriented gate:
node scripts/run-worker-gate.mjs
```

Details: `docs/verification/worker-gate-entry.md`, root `AGENTS.md`.

## Current status (Stage 1)

Stage 1 focuses on a governed local IDE: conversation kernel, real provider
adapters behind a strict protocol boundary, project-scoped read tools, a
read-only workbench, governed write/diff/approval, bounded process execution,
and read-only Git inspection.

High-level acceptance evidence lives under `docs/verification/`. Design
authority and ADRs live under `docs/superpowers/specs/` and
`docs/decisions/`. A short map of the docs tree is in `docs/README.md`.

**C4 is checkpointed** through D3B4 (governed process + read-only Git Changes).
Interactive shell and Git mutation have not been started. See
`docs/verification/2026-08-07-wave-c4-checkpoint.md`.

## Design authority

- `docs/superpowers/specs/2026-07-29-pillow-first-design.md`
- `docs/migration/legacy-capability-inventory.md`
- `docs/references/upstream-baseline.md`
- `docs/decisions/`
- `docs/verification/`
- `docs/roadmap/2026-07-31-stage1-overall-status.md`

## License

Apache-2.0. See `LICENSE` and `THIRD_PARTY_NOTICES.md`.
