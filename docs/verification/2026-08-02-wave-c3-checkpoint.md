# Pillow Wave C3 checkpoint

Date: 2026-08-02

State: **accepted working-tree checkpoint; unstaged and uncommitted**

## Product boundary now available

Pillow now has the Stage 1 capability chain:

`project-scoped conversation -> real runtime -> governed read -> read-only
resource workbench -> governed single-file write/diff/approve-once`

C3 adds a human-governed write surface without granting the provider filesystem
or approval authority. Only one bounded UTF-8 create/modify proposal is in
scope. The host owns diff construction, base-hash CAS, atomic apply, durable
metadata-only status and terminal receipt projection. Project switching,
cancellation, expiry and restart invalidate authority instead of widening it.

## Accepted evidence

- controller source verdict: Critical 0 / Important 0;
- `pnpm check`: 246/246 desktop tests plus Rust workspace/doctests and Clippy;
- production UI build: passed;
- real focused C3 desktop: 1/1 with all eight adversarial scenarios;
- complete serial desktop E2E: 8/8;
- process safety: 41/41;
- format/diff/cached-diff gates: passed;
- relevant Pillow/fake-ACP/Cargo/rustc residuals: zero;
- branch/HEAD unchanged; staging empty.

Detailed evidence is recorded in
`docs/verification/2026-08-02-wave-c3-acceptance.md`.

## Still excluded

- arbitrary file writes, rename, delete, directories or binary mutation;
- allow-session, auto-approve or provider-selected approval authority;
- terminal and general process execution;
- Git status/diff/branch/worktree/commit/restore/publish;
- network tools, credentials or key management;
- Obsidian governance and company-operation features;
- Codex/OpenCode or other runtime expansion;
- arbitrary MCP/provider-native tools.

## Next boundary

C4 terminal and Git is the next **planning** boundary only. It requires a new
research/design/authority brief and explicit user authorization before any
implementation. Stage 2 and Stage 3 remain inactive.

This checkpoint document does not authorize staging or committing the existing
C1-C3 working tree.
