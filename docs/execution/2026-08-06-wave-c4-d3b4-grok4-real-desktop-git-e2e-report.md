# C4 D3B4 Round 3 — Grok4 real desktop Git E2E report

Date: 2026-08-06  
Worker: actual Grok CLI slot `grok4`  
Task: `c4-d3b4-real-desktop-git-e2e`  
Brief: user-provided C4 D3B4 Round 3 real desktop Git E2E brief  
Design: `docs/superpowers/specs/2026-08-05-c4-git-changes-design.md`  
Plan: `docs/superpowers/plans/2026-08-05-c4-git-changes-implementation.md`

## Keeper boot

- Attempted worker boot once:  
  `POST local session bridge boot (optional; out of tree)`  
  `{"project":"pillow","task":"C4 D3B4 Round 3 real desktop Git E2E","agentName":"grok4","agentRole":"worker"}`
- Result: **HTTP 400** `{"error":"worker claims require local parent session id"}`.
- Did not impersonate controller. Did not run through Keeper orchestration.
- Keeper used for record attempt only; no Vault query, prompt rewrite, or completion judgment.

## Worker / profile identity

| Field | Value |
| --- | --- |
| Slot | `grok4` |
| Effective profile | Grok slot home (USERPROFILE under slot isolation) |
| Host OS user profile | `HOMEDRIVE`+`HOMEPATH` (host Rust toolchain) |
| Host Git (fixture + `PILLOW_HOST_GIT`) | Resolved host `git.exe` (well-known path / PATH) |
| Runtime composition | Production Tauri/WebView2 + production Grok ACP path with `fake_acp_agent` (not `PILLOW_RUNTIME=fake`) |

Harness build env may set `RUSTUP_HOME`/`CARGO_HOME` to the OS-user rustup when the slot profile has no active toolchain. Product launch env does not use that for Git authority beyond accepted `PILLOW_HOST_GIT`.

## Exact changed files (allowlist only)

| Path | Action |
| --- | --- |
| `scripts/lib/c4-git-changes-acceptance.mjs` | **Create** — public UI helpers for Git Changes |
| `scripts/wave-c4-git-changes-acceptance.mjs` | **Create** — real Tauri/WebView2 orchestrator |
| `apps/desktop/tests/e2e/c4-git-changes.spec.ts` | **Create** — thin serial E2E wrapper |
| `apps/desktop/package.json` | Modify — add `test:e2e:c4:git`; append spec to serial `test:e2e` |
| `docs/execution/2026-08-06-wave-c4-d3b4-grok4-real-desktop-git-e2e-report.md` | **Create** — this report |

No App/Workbench/Tauri commands/gateways/Git executor/lockfiles/existing E2E/supervision libraries/plans were modified.

Generated evidence only under ignored `.superpowers/c4-git-acceptance/` and OS temp dirs.

## Runtime path proof

```
UI (project select + resource-pane Changes)
  → TauriGitGateway / invoke names
  → GitCommandState (discover_git_repositories / get_git_status / open_git_diff)
  → GitReadExecutor (fixed templates, real host git via PILLOW_HOST_GIT / host resolve)
```

- No direct Git for UI assertions after fixture setup.
- No `PILLOW_RUNTIME=fake`, no renderer injection, no private React callbacks.
- Isolated temp project roots + isolated app data / WebView2 profile.
- Launcher-owned `ProcessSupervisor` + fresh-handle cleanup; no numeric-PID kill / process-name-wide kill.

## Scenario matrix

| # | Scenario | Result |
| --- | --- | --- |
| 1 | `project-root-clean` | **passed** — Project B single root repo, clean status, branch/HEAD |
| 2 | `nested-repository` | **passed** — portable ids `.` + `vendor/nested`; nested clean |
| 3 | `modified-untracked` | **passed** — 修改 + 未跟踪 |
| 4 | `rename` | **passed** — 重命名 with old/new portable paths |
| 5 | `conflict` | **passed** — 冲突 observed without resolve/mutate |
| 6 | `detached-head` | **passed** — 分离 HEAD + head short |
| 7 | `text-diff` | **passed** — one bounded unified text diff |
| 8 | `binary-oversized-diff` | **passed** — binary summary; oversized as unsupported/truncated without invented full text |
| 9 | `unsafe-config-denial` | **passed** — filter config → fixed path-free public error |
| 10 | `generation-drift` | **passed** — identity re-init + refresh drops stale paths/diff |
| 11 | `project-switch-isolation` | **passed** — A/B never cross-render markers |
| 12 | `restart-recovery` | **passed** — normal close + relaunch rediscovers; no mutation |

### Fixture notes (fail-closed portable substitutes)

- Nested `vendor/` is **gitignored** on Project A so root porcelain does not emit trailing-slash directory entries that map to host `git_path_rejected` on status. Nested discovery still walks the filesystem independently of gitignore.
- Unsafe denial uses local `filter.evil.clean` (no elevation). Public UI may show `Git repository was not found.` (unavailable discovery) or `Git read capability is unavailable.` — both fixed path-free copies.
- Direct Git is fixture setup + independent hash inspection only.

## Commands, exit codes, counts, wall time

```powershell
node --check scripts/lib/c4-git-changes-acceptance.mjs
# exit 0

node --check scripts/wave-c4-git-changes-acceptance.mjs
# exit 0

pnpm --filter @pillow/desktop exec node --test --test-concurrency=1 tests/e2e/c4-git-changes.spec.ts
# exit 0; tests 1; pass 1; fail 0; wall ~252s (outer node:test)
# orchestrator wallTimeMs: 248962

pnpm --filter @pillow/desktop typecheck
# exit 0

pnpm --filter @pillow/desktop build
# exit 0

git diff --check -- scripts/lib/c4-git-changes-acceptance.mjs scripts/wave-c4-git-changes-acceptance.mjs apps/desktop/tests/e2e/c4-git-changes.spec.ts apps/desktop/package.json docs/execution/2026-08-06-wave-c4-d3b4-grok4-real-desktop-git-e2e-report.md
# exit 0

git diff --cached --name-only
# (empty)
```

Completion record fields (from orchestrator stdout / evidence):

- `status: passed`
- `ipcBoundary: real-tauri-webview`
- `runtimePath: production-tauri-webview-real-host-git`
- `gitPath: GitCommandState->GitReadExecutor`
- `processCleanup / normalClose / restartRecovery: passed`
- `residualProcesses: 0`
- `viewports: 1440x960, 1024x720`
- 12/12 required scenarios

## Screenshots / evidence paths

Under `.superpowers/c4-git-acceptance/` (gitignored):

| File | Role |
| --- | --- |
| `git-changes-1440x960.png` | Repository selector + compact status/diff |
| `git-changes-1024x720.png` | Conversation-dominant + contextual Changes |
| `restart-before-1440x960.png` | Pre-restart dirty marker |
| `restart-after-1440x960.png` | Post normal restart rediscovery |
| `c4-git-acceptance-record.json` | Compact pass record |
| `hash-manifest.json` | Fixture/scenario immutability hashes |

Privacy: body text/HTML and screenshots asserted free of drive roots, fixture temp roots, UUID/session/run/proposal ids, executable/argv/cwd/env, tokens, raw RPC, debug invoke names.

## Immutability

- Before/after manifests captured per read-only scenario (HEAD, index, config, attributes, hooks metadata, tracked file hashes, porcelain status digest).
- Read-only UI paths did not mutate fixture identity except intentional fixture-setup steps between observations (conflict setup, detach, re-init for generation drift, etc.).
- Compact reference: `.superpowers/c4-git-acceptance/hash-manifest.json`.

## Cleanup / residual counts

| Check | Result |
| --- | --- |
| Launcher-owned Pillow descendants | 0 |
| `pillow.exe` residual | 0 |
| Port 1420 listeners (suite end) | 0 |
| Real user WebView2 profile | Unchanged (snapshot compare) |

## Product blockers and residual risk

- **No BLOCKED product defect** for this allowlist run. Nested-directory porcelain paths with trailing slash can make **root** status fail closed with `git_path_rejected` when a nested worktree is untracked and not ignored; the acceptance fixture uses portable `vendor/` gitignore so discovery still proves nested selection without weakening containment.
- Residual risk: multi-hour full serial desktop suite still a controller gate; this worker ran the focused C4 Git E2E only.
- Harness-only slot Rust toolchain routing is an environment accommodation, not product code.

## Staging

```text
git diff --cached --name-only
# empty
```

Nothing staged or committed by this worker.

## Completion

C4 D3B4 Round 3 real desktop Git E2E delivered for controller source review and acceptance gates.

`C4-D3B4-GROK4-REAL-DESKTOP-GIT-E2E-COMPLETE`
