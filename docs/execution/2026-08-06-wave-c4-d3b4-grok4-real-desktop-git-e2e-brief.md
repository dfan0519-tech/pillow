# C4 D3B4 Round 3 — Grok4 real desktop Git E2E brief

Date: 2026-08-06  
Implementer: actual Grok CLI slot `grok4`  
Controller: Codex Desktop performs final source review and all acceptance gates

## Read first

1. `<repo>\AGENTS.md`
2. `<repo>\docs\superpowers\specs\2026-08-05-c4-git-changes-design.md`
3. `<repo>\docs\superpowers\plans\2026-08-05-c4-git-changes-implementation.md`
4. `<repo>\docs\execution\2026-08-05-wave-c4-d3b4-grok1-native-git-report.md`
5. `<repo>\docs\execution\2026-08-05-wave-c4-d3b4-grok2-git-bridge-report.md`
6. `<repo>\docs\execution\2026-08-05-wave-c4-d3b4-grok3-git-pane-report.md`
7. `<repo>\docs\execution\2026-08-05-wave-c4-d3b4-grok4-git-workbench-report.md`
8. accepted supervision references:
   `scripts/wave-c4-governed-process-acceptance.mjs`,
   `scripts/lib/desktop-process.mjs`, and
   `apps/desktop/tests/e2e/c4-governed-process.spec.ts`

Round 1, Round 2A and repaired Round 2B are controller-accepted. This task
adds production-composition acceptance only; do not redesign product code.

Keeper is record-only. Attempt worker boot once. If worker boot is rejected
because no parent session id is available, record that result and continue;
do not impersonate controller and do not run through Keeper orchestration.

## Exact write allowlist

- Create `scripts/lib/c4-git-changes-acceptance.mjs`
- Create `scripts/wave-c4-git-changes-acceptance.mjs`
- Create `apps/desktop/tests/e2e/c4-git-changes.spec.ts`
- Modify `apps/desktop/package.json`
- Create `docs/execution/2026-08-06-wave-c4-d3b4-grok4-real-desktop-git-e2e-report.md`

Generated evidence may be written only under ignored
`.superpowers/c4-git-acceptance/` and temporary OS directories. Do not modify
App, Workbench, Tauri commands, gateways, Git executor, lockfiles, existing E2E
files, supervision libraries, plans, or unrelated dirty work. Do not stage or
commit.

If real execution exposes a product defect that cannot be repaired inside this
allowlist, stop with `BLOCKED`, preserve diagnostic evidence, and report the
smallest required product-file expansion. Do not silently edit outside scope.

## Required runtime truth

- Use the production Tauri/WebView2 application and the real host Git
  executable through the accepted `GitCommandState -> GitReadExecutor` path.
- Never call Git directly to satisfy a UI assertion after fixture setup. Direct
  Git is allowed only to create fixtures and independently hash/inspect them.
- Do not use `PILLOW_RUNTIME=fake`, inject renderer state, call private React
  callbacks, or bypass the public project selection/resource-pane UI.
- Use isolated temporary project roots and isolated app data. Never point the
  test at `<repo>` or another user working tree.
- Reuse the accepted launcher-owned `ProcessSupervisor` and fresh-handle
  cleanup pattern. No numeric-PID kill, process-name-wide kill, or unowned
  descendant cleanup.

## Fixture and scenario matrix

Build deterministic temporary Git repositories with fixed local author data;
do not rely on global/system Git config. Capture before/after hashes for HEAD,
index, config, attributes/hooks-related metadata used by the executor, and the
fixture working-tree files.

Prove through the visible desktop UI:

1. project-root repository discovery and clean status;
2. nested repository discovery/selection with portable repository ids only;
3. ordinary modified and untracked entries;
4. rename represented with old/new portable paths;
5. conflict status without mutating or resolving it;
6. detached HEAD label and identity;
7. one bounded text diff rendered in the contextual pane;
8. binary and oversized/unsupported/truncated summaries without invented
   unified text;
9. unsafe repository configuration/metadata denial with a fixed path-free
   public error and no external path disclosure;
10. discovery generation drift: a stale prior generation cannot paint or open
    a diff after fixture repository identity changes;
11. project switch isolation: Project B never renders Project A repository,
    path, status, diff, or error state;
12. restart recovery: after normal desktop restart the current project is
    rediscovered/refreshed, no stale pre-restart generation is reused, and no
    mutation occurs.

Where Windows/Git cannot create a scenario reliably without elevated
permissions, fail closed and report the exact portable substitute. Do not
weaken containment assertions or follow symlink/reparse paths.

## Visual and privacy proof

Capture privacy-safe screenshots at both supported sizes already used by the
accepted process harness:

- `1440x960`: repository selector + compact status/diff;
- `1024x720`: conversation-dominant layout with the contextual Changes pane.

Assert body text/HTML and screenshots expose none of: drive-root/native paths,
fixture temp roots, UUID/session/run/proposal ids, executable/argv/cwd/env,
tokens, raw RPC, or debug metadata. Conversation remains the dominant surface;
there is no second dashboard or permanent card stack.

## Immutability and cleanup gates

- Compare the captured before/after fixture hashes. Read-only Git operations
  must not change working tree, index, HEAD, config, hooks/attributes metadata,
  or repository identity except for scenario changes explicitly performed by
  fixture setup between observations.
- Finish with zero launcher-owned Pillow descendants, zero `pillow.exe`, and
  zero port 1420 listeners attributable to this acceptance run.
- Preserve the real user WebView2 profile and unrelated processes.

## Package script

Add `test:e2e:c4:git` for the new spec and append the spec to the existing
serial `test:e2e` command. Do not reorder or parallelize existing specs.

## Worker gates

Run serially:

```powershell
node --check scripts/lib/c4-git-changes-acceptance.mjs
node --check scripts/wave-c4-git-changes-acceptance.mjs
pnpm --filter @pillow/desktop exec node --test --test-concurrency=1 tests/e2e/c4-git-changes.spec.ts
pnpm --filter @pillow/desktop typecheck
pnpm --filter @pillow/desktop build
git diff --check -- scripts/lib/c4-git-changes-acceptance.mjs scripts/wave-c4-git-changes-acceptance.mjs apps/desktop/tests/e2e/c4-git-changes.spec.ts apps/desktop/package.json docs/execution/2026-08-06-wave-c4-d3b4-grok4-real-desktop-git-e2e-report.md
git diff --cached --name-only
```

The real E2E may run longer than ten minutes. There is no artificial turn or
wall-clock cutoff. Scenario-specific waits must remain bounded and diagnostic;
do not add a global task timeout that truncates valid execution.

## Report and completion

The report must include:

- actual worker/profile identity and Keeper boot result;
- exact changed files;
- runtime path proof and scenario matrix;
- commands, exit codes, counts, and wall time;
- screenshot/evidence paths;
- before/after immutability hashes or a compact manifest reference;
- cleanup/residual counts;
- any product blocker and residual risk;
- staging-empty confirmation.

End the report and final stdout with:

`C4-D3B4-GROK4-REAL-DESKTOP-GIT-E2E-COMPLETE`

Worker completion is not controller acceptance.
