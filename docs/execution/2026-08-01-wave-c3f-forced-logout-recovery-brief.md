# Wave C3-F Forced-Logout Recovery Brief

Owner: Grok3 harness worker; controller owns review and acceptance

Controller session: `<keeper-session>`

Worker session: `<keeper-session>`

## Starting Truth

The previous C3-F worker was terminated by the machine logout while its first
focused Tauri acceptance run was in progress. It left the harness source in the
working tree but produced no completion record, screenshots or integration
report. Nothing from that run is accepted evidence.

Existing C3-F candidate files must be reviewed as untrusted partial work:

- `apps/desktop/tests/e2e/c3-governed-write.spec.ts`
- `scripts/wave-c3-governed-write-acceptance.mjs`
- `scripts/lib/c3-governed-write-acceptance.mjs`
- the C3 entry in `apps/desktop/package.json`
- narrowly necessary fake-ACP scenario additions in the C3-D fixture files

HEAD is `7b7f70f7360131cd1cfd984a8b19bdfb5725c21b`, staging is empty, and
all accepted C1/C2/C3-A-E working-tree changes must be preserved.

## Task

1. Read the active C3-F brief and inspect every partial harness/fake-agent
   change before running it. Do not assume the interrupted worker was correct.
2. Validate Node syntax and the fake-ACP adapter fixture tests.
3. Run the focused real Tauri/WebView2 C3-F acceptance serially. Fix only
   harness or fake-fixture defects within the original C3-F whitelist.
4. If the failure is in product renderer/native/runtime code, stop and document
   a minimal deterministic reproduction; do not edit product code.
5. When focused C3-F is green, run the complete desktop E2E command and all
   remaining gates named by the active brief.
6. Produce the required screenshots/JSON evidence and
   `docs/execution/2026-08-01-wave-c3-integration-report.md` with exact commands,
   counts, hashes, residual process checks, changed files and residual risks.

## Non-negotiable Boundaries

- Use real production Tauri commands and public WebView2 UI.
- Never call the write executor directly to simulate approval.
- Never mutate target files except isolated pre-run fixtures and the explicit
  post-proposal base-drift setup.
- No product implementation edits, old Keeper writes, reset/restore, staging,
  commit, packaging or publishing.
- End with `COMPLETION: DONE` or `COMPLETION: BLOCKED`, then exit. Worker output
  is not controller acceptance.
