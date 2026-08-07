# C4 D3B3 Task 5B-R — Grok4 Visual Proof Repair Brief

## Controller finding

The real desktop focused E2E passed, but controller visual inspection rejected
`conversation-dominant-1024x720.png`: it shows an empty conversation and the Resource pane. It does not prove the required in-conversation Agent process activity with an activity/output contextual pane open.

This is a harness false-green. Repair the proof without changing production code.

## Exact write allowlist

- `scripts/lib/c4-governed-process-acceptance.mjs`
- `scripts/wave-c4-governed-process-acceptance.mjs`
- `apps/desktop/tests/e2e/c4-governed-process.spec.ts`
- `docs/execution/2026-08-05-wave-c4-d3b3-grok4-visual-proof-repair-report.md` (create)
- generated ignored evidence only under `.superpowers/c4-process-acceptance/`

Do not modify any production React, Tauri, runtime-host, policy, executor, adapter, package, roadmap, plan, or unrelated dirty file. Do not stage, commit, reset, restore, clean, or broadly format.

## Required repair

1. Capture the 1024x720 proof while still on a conversation that visibly contains a completed governed-process activity.
2. Require the process activity row, its meaningful status/details, and the terminal output contextual pane to be visible before capture. Do not swallow a failed `openCommandOutput` call.
3. Keep the conversation visually dominant and the contextual pane subordinate. Preserve privacy assertions and the single-approval-slot constraint.
4. Strengthen automated semantic assertions so an empty conversation or Resource pane cannot produce a green screenshot again.
5. Keep the strict terminal completion record contract unchanged.

## Required verification

Run at minimum:

```powershell
node --check scripts/lib/c4-governed-process-acceptance.mjs
node --check scripts/wave-c4-governed-process-acceptance.mjs
pnpm --filter @pillow/desktop exec node --test --test-concurrency=1 tests/e2e/c4-governed-process.spec.ts
pnpm --filter @pillow/desktop typecheck
git diff --check -- scripts/lib/c4-governed-process-acceptance.mjs scripts/wave-c4-governed-process-acceptance.mjs apps/desktop/tests/e2e/c4-governed-process.spec.ts docs/execution/2026-08-05-wave-c4-d3b3-grok4-visual-proof-repair-report.md
```

Write the repair report with changed files, RED reason, GREEN evidence, screenshot path, remaining risks, and `completion: complete|blocked`. Do not claim controller acceptance.
