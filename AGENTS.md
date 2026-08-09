# Pillow Agent Rules

This repository is the clean successor to the legacy Vault Keeper resource
manager. Pillow is a separate product codebase: it does not require an external
session service, vault checkout, or sibling monorepo layout to build or test.

## Product Order

1. Industrial local AI IDE.
2. Governed Obsidian knowledge integration.
3. One-person-company project operations.

Do not pull later-stage concerns into an earlier stage without an accepted
design change.

## Repository Boundary

- Pillow is independently buildable from this repository alone.
- Optional external references (a legacy resource manager, upstream UI pins,
  conceptual interaction clones) may exist out of tree for maintainers; treat
  them as read-only when present. Never copy them wholesale into product code.
- Port behavior through typed contracts and focused tests.
- Never commit credentials, account tokens, machine-specific paths, or runtime
  session data.

## Engineering Rules

- Domain state belongs in SQLite, not ad hoc JSON files.
- The desktop shell must not own agent-domain logic.
- Agent runtimes connect through explicit adapters.
- Approval, audit, and secret boundaries are enforced below the UI.
- Every durable object is project-scoped.
- UI work follows the accepted Grok App interaction baseline until a Pillow
  design system supersedes it.
- Upstream code reuse must retain required license and attribution notices.
- New modules require focused tests and must remain independently replaceable.

## Multi-runtime isolation (principle)

When running more than one provider CLI or agent profile on one machine, keep
credential stores and config roots isolated per profile. Do not point a second
profile at another profile’s auth files by only changing a home-style
environment variable. Verify the resolved config source before treating a
session as authenticated. Never print raw authentication files or
token-bearing fields.

Maintainer-only slot pools, claim/release flows, and local session bridges are
**not** part of the public product contract. See `.superpowers/RUNBOOK.md` on
machines that use them.

## Required Gates

- Typecheck, lint, unit tests, integration tests, and desktop smoke must pass.
- Agent adapters require protocol fixture tests.
- Write-capable tools require explicit approval-path tests.
- UI changes require desktop screenshots at supported Windows sizes.
- Performance budgets are release gates, not optional diagnostics.

Primary contributor check entry points:

- `pnpm check` (when present at the repo root)
- `node scripts/run-worker-gate.mjs` — see
  `docs/verification/worker-gate-entry.md`
