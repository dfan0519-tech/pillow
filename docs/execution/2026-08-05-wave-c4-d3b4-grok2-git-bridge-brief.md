# C4 D3B4 Round 1B — Grok2 typed Git bridge brief

You are the real Grok CLI `grok2` implementation worker. Harden and prove the
renderer-side typed Tauri bridge for the accepted read-only Git slice. Do not
mount UI and do not modify Rust.

## Startup

1. Work only in `<repo>`.
2. Read `AGENTS.md`,
   `docs/superpowers/specs/2026-08-05-c4-git-changes-design.md`, and this brief.
3. Boot Keeper once as record-only: project `pillow`, task
   `c4-d3b4-git-bridge`, agentName `grok2`, agentRole `worker`. Do not ask
   Keeper to execute or judge project work.
4. Inspect the dirty tree and preserve unrelated work. Do not stage, commit,
   reset, clean, restore, or reformat unrelated paths.

## Write allowlist

- Modify `apps/desktop/src/lib/gitGateway.ts`
- Modify `apps/desktop/src/lib/tauriGitGateway.ts`
- Create `apps/desktop/tests/tauri-git-gateway.test.ts`
- Create `docs/execution/2026-08-05-wave-c4-d3b4-grok2-git-bridge-report.md`

No other file may be changed.

## Required contract

- Keep `gitGateway.ts` as the only renderer wire-contract source.
- Use exact invokes `discover_git_repositories`, `get_git_status`, and
  `open_git_diff` with only projectId/repositoryId/generation/path arguments.
- Strictly reject unknown fields, native/absolute/backslash/traversal paths,
  invalid enum values, unsafe integers, oversized lists/diffs, inconsistent
  byte/line counts and malformed generation/repository echoes.
- Keep repository id `.` valid and other ids portable project-relative.
- Preserve text/binary/unsupported distinctions; binary/unsupported responses
  never contain invented unified text.
- Normalize only the six accepted public errors. Unknown/malformed Tauri
  responses become path-free `git_unavailable`; generation mismatch/stale from
  strict validation must retain their specific code.
- Do not add loading, stale UI flags, repository selectors, React state or
  product mounting in this task.

Tests must assert exact invoke payloads and hostile/malformed responses,
including path leakage, extra fields, list/diff bounds, stale generation,
repository mismatch, line/byte inconsistency, binary text injection and public
error normalization.

## Gates

```powershell
pnpm --filter @pillow/desktop test -- tests/tauri-git-gateway.test.ts
pnpm --filter @pillow/desktop typecheck
git diff --check -- apps/desktop/src/lib/gitGateway.ts apps/desktop/src/lib/tauriGitGateway.ts apps/desktop/tests/tauri-git-gateway.test.ts docs/execution/2026-08-05-wave-c4-d3b4-grok2-git-bridge-report.md
```

Do not run desktop E2E.

## Completion

The report must list changed files, contract decisions, tests/exit codes,
remaining risk and tree hygiene. End the final worker response with:

`C4-D3B4-GROK2-GIT-BRIDGE-COMPLETE`

