# C4 D3B Grok4 brief — governed process UI integration map

Read this file first. It is the complete task contract.

You are the actual Keeper `grok4` CLI worker. Boot Keeper as a `pillow` worker
for task `c4-d3b-process-ui-integration-research`. Work only in
`<workspace>\pillow`; Keeper and all reference repositories are read-only. Preserve
HEAD `7b7f70f7360131cd1cfd984a8b19bdfb5725c21b`, empty staging and all unrelated
dirty work. No stage, commit, reset, restore, clean, package or publish.

This is a preparation task that may run while the controller closes the final
D3AR2 integration gate. Do not implement product code yet.

## Objective

Produce an evidence-backed D3B implementation map for carrying the accepted
governed process proposal, decision, live output and terminal events through
the Tauri/TypeScript bridge into conversation-owned UI with progressive
disclosure.

Inspect the current Pillow Rust/Tauri/TS/React flow and the local references
`<workspace>\VaultSystem\references\grok-app` (or its verified current location)
and `<workspace>\zbbody`. Extract only relevant patterns for conversation-owned
agent activity, expandable steps, role/status display and output/file/test
detail. Record provenance and licenses; do not copy code.

The report must provide:

1. Exact current source-to-render data flow and missing seams.
2. Proposed typed DTOs and command/event names, with project/conversation/run/
   proposal ownership boundaries.
3. File-level implementation slices with non-overlapping write allowlists for
   at least three workers.
4. Test matrix: Rust command tests, TS gateway tests, React interaction tests,
   project-switch isolation, restart/durable-only output and desktop E2E.
5. UI interaction model: conversation-centered summary row -> activity detail
   -> output/tests/files; no second dashboard, raw session IDs or debug tables.
6. Risks and explicit out-of-scope items. D3B must not broaden process
   authority, add a second executor, expose grants, or persist transcripts.

## Exact write allowlist

- `docs/research/2026-08-04-c4-d3b-grok4-process-ui-integration-map.md`

No other file may be changed.

Run `git diff --cached --name-only` and `git rev-parse HEAD`. Write the report,
post a Keeper completion message with its path and key recommendations, release
only your exact lease, then exit. End the report with `COMPLETION: complete` or
an accurate blocked state. Controller integration and implementation decisions
remain separate.
