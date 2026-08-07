# Wave C4 D3B4 Controller Acceptance (Round 4)

Date: 2026-08-07
Controller session: `<session-id>`
Branch / HEAD: `main` / `62149be9eddd2288eb45fd9b8278641cf64c7a5f` (unchanged)
Staging: empty. Nothing staged or committed by this acceptance.

Verdict: **D3B4 read-only Git Changes ACCEPTED**, after one Critical regression
was found by this gate and repaired. C4 checkpoint is **NOT** taken — see §7.

This is a controller record. It is not a copy of any worker report.

## 1. What Round 4 actually found

Round 3 delivered the real-desktop Git E2E harness and self-reported 12/12. The
worker explicitly ran only the focused Git E2E and left the full serial suite to
the controller. That gap mattered: the first full serial desktop E2E run on this
tree was **7/10**, and two of the three failures traced to a Critical product
regression that no prior gate had exercised.

Round 4 therefore consists of the source review, three defect repairs, and the
fresh aggregate gates below.

## 2. Round 3 harness source review (controller, personally)

Reviewed all five allowlist files. No Critical, no Important.

- **No PID/name-wide kill.** Zero `kill`/`taskkill`/`process.kill` in the
  orchestrator or helper. Teardown runs only through the accepted
  `ProcessSupervisor`, which terminates only the root child handle; descendants
  are never killed and must be *proven* dead or cleanup is refused
  (`scripts/lib/desktop-process.mjs:592`). `retainTrackedPids` only deletes
  records, so the over-broad WebView2 branch in `isAcceptanceManagedProcess`
  can only retain more — the fail-closed direction. Dead code, not a defect.
- **No fake runtime.** `delete env.PILLOW_RUNTIME` at
  `scripts/wave-c4-git-changes-acceptance.mjs:633`; production Grok ACP with a
  fake child binary. Independently corroborated by `Pillow runtime selection:
  grok-acp` in the harness stderr.
- **No host pollution.** Isolated `CARGO_TARGET_DIR`, app-data and WebView2
  profile; explicit refusal to launch the repository's own
  `target/debug/pillow.exe`; `assertPathUnchangedSince` on the real
  LocalAppData WebView2 profile at suite end.
- **Direct Git confined to fixture setup/hash.** Exactly one Git spawn site
  (`:920`). Every UI assertion goes through testids into real
  Tauri → `GitCommandState` → `GitReadExecutor`.
- **Hash manifest does not mask mutation.** `equal: true` is emitted only where
  a real `assertManifestEqual` ran. `scenarioGenerationDrift` honestly omits the
  equality assertion (identity intentionally changes) and records
  `identityChanged` plus an explicit note instead of faking a pass.
- **Evidence chain is genuine.** The retained `c4-git-acceptance-failure.json`
  (00:55) failed on the real assertion at
  `scripts/lib/c4-git-changes-acceptance.mjs:665` ("conversation stage must not
  be empty placeholder"); the later pass record (01:00) followed the
  `seedConversation` repair. Different runs, different times. Retained
  deliberately as an audit chain; read it by timestamp, do not delete it.
- **Serial preserved.** `test:e2e` keeps `--test-concurrency=1` and only appends.
- `.superpowers/` is ignored at `.gitignore:33`; no evidence can be committed.

Harness-only notes, not product code: `resolveHostGit` hardcodes
a developer-specific Git install path before falling back to `where git`; `fixtureGitEnv` spreads
`process.env` without `GIT_CONFIG_GLOBAL` isolation, so fixture setup can read
the user's global gitconfig. Neither writes user config (`git config` calls are
local, no `--global`).

## 3. Defect 1 (Critical, product) — Git Changes displaced the C2 resource browser

Found by this gate. `App.tsx` built a non-null `gitChanges` model whenever a
project and gateway existed, **without checking whether any repository was
discovered**; `ResourcePane` then selected the Changes body ahead of
Files/Preview. The accepted C2 capability (tree, search, recents, bounded
preview) became unreachable in the UI, with no way back.

RED evidence, full serial desktop E2E 7/10:

- C2-E: `locator.waitFor: Timeout 20000ms exceeded — waiting for
  getByRole('region', { name: '项目资源' })`
- C3-F: `approve-modify: opened resource must reference notes/base.txt` → `false !== true`

Controller design ruling (issued before any work started): Changes must be an
**explicitly selected** mode, never an ambient default that outranks the
resource browser; default is resources; Terminal and governed-write Diff keep
absolute priority.

Repaired by worker `grok3` under an exact allowlist. Controller review confirms
`showGitChanges` now requires `bodyMode === "changes"` with default
`"resources"`, the switcher is a keyboard-reachable `role="tablist"`, the
Round 2B ownership guard is retained (`App.tsx:563`, `!==` early-return form),
and Git activation is additionally scoped by `gitChangesRequestProjectId`.
Worker stayed inside the allowlist; `scripts/wave-c4-git-changes-acceptance.mjs`
was not touched.

## 4. Defect 2 (Important, product) — untracked directories broke whole-repo status

Found by this gate; repaired by the controller.

`git status --porcelain=v2` emits `? vendor/` for any collapsed untracked
directory (verified empirically against a scratch repo). The trailing separator
produced an empty path component, which `validate_portable_path` rejected, and
`status.rs:175-181` propagated that with `?` straight out of the parse loop — so
**one** untracked directory failed the entire repository status as
`git_path_rejected`. Any repo with `build/`, `dist/`, `target/` or a new folder
was affected. The Round 3 fixture sidestepped it by gitignoring `vendor/`.

This was an oversight, not a decision: the only untracked test case covered a
file (`? notes/todo.md`) and every rejection case was a hostile path.

Fixed at `parse_simple_path_record`, which handles only `?`/`!` records — the
only two forms that legitimately carry a directory. The stem is validated by the
identical rules, so traversal/absolute/device containment is unchanged, and
`tracked_records_still_reject_a_trailing_separator` proves no global loosening.
Clicking a directory entry remains fail-closed through the existing
`is_portable_path` check, returning the existing fixed path-free
`git_path_rejected` — no new error code, no DTO change.

## 5. Defect 3 (harness) — `pnpm check` could not reach exit 0 on a cold build

`run-cargo.mjs` refused cleanup with "observation-only PIDs remain" whenever a
real compile preceded it. Root cause: `CLEANUP_ATTEMPTS = 120 × 50ms = 6s` of
grace, while MSVC's detached `vctip.exe` telemetry process survives ~6s after
compilation (observed live). The supervisor already has purpose-built handling
for exactly this process; `run-cargo.mjs` simply did not enable it, unlike the
E2E spec wrapper.

Enabled `allowMsvcVctipTelemetry: true`. The allowance is identity-gated (exact
MSVC tools-path regex, strict `=== true`, non-root record, and only once the
root is confirmed dead) and covered by negative tests for a string `"true"` and
a wrong executable path. No cleanup logic was weakened;
`desktop-process-safety.test.mjs` remains 46/46 including "still-live observed
descendant after natural root exit fails observation-only".

## 6. Fresh controller gates (all run by the controller after the repairs)

| Gate | Result |
| --- | --- |
| `pnpm check` | exit 0 — renderer **365/365** across 29 files, Rust **731**, Clippy `-D warnings` clean |
| `pnpm --filter @pillow/desktop build` | exit 0, 84 modules |
| **Complete serial desktop E2E** | **10/10**, exit 0 |
| Focused renderer (6 suites) | 100/100 |
| Focused native `git_commands` | 14/14 |
| `keeper-git-executor` | 20/20 (5 new regression tests) |
| `desktop-process-safety` | 46/46 |
| `pnpm --filter @pillow/desktop typecheck` | exit 0 |
| `cargo fmt --all -- --check` | exit 0 |
| `git diff --check` | exit 0 |
| Staging | empty |
| Residual `pillow.exe` / 1420 listeners | 0 / 0 |

Focused Git E2E record (controller run): 12/12 scenarios, residual 0/0,
232036 ms, `ipcBoundary: real-tauri-webview`,
`gitPath: GitCommandState->GitReadExecutor`.

Environment accommodation, not product code: `pnpm` is absent from this shell's
PATH, so gates ran through a session-local shim resolving corepack's pinned
pnpm 10.14.0 — the same version as the `packageManager` field. Nothing was
written into the repository or the system Node install.

## 7. Unresolved risks — why the C4 checkpoint is NOT taken

1. **D3B3 `cancel-tree` is a live race, not a cleared failure.** It failed the
   first full serial run (`composer-cancel` never visible within 60s) and passed
   the second **with no code change**. A single green run does not clear a known
   race. Read-only diagnosis (worker `grok1`,
   `docs/execution/2026-08-07-wave-c4-d3b3-cancel-tree-diagnosis.md`) identifies
   a structural cause with medium-high confidence: `conversations_loaded`
   replaces the whole conversation list and the `listConversations` effect has
   no generation guard, so a late stale snapshot can drop the selected
   conversation, null the active run, and unmount the cancel control. **That
   race is still present in the code.** It must be fixed before C4 is
   checkpointed.
2. **Process lifecycle events emit only after execution returns.**
   `crates/adapter-grok-acp/src/prompt.rs:713-742` emits
   `process_proposal_decided` / `process_started` / `process_terminal_outcome`
   only after `execute_governed_process` returns, so during a long-running
   approved process the timeline stays at 待批准 and never shows 运行中. The
   acceptance harness currently swallows this with
   `.catch(() => undefined)`. This is a real observability gap inside accepted
   D3B3 scope.
3. **Contextual pane header labels are redundant.** With the new mode switcher,
   the pane shows 变更 three times at 1024x720 (h2 heading, active tab, and the
   view's own section label). Cosmetic, but the design requires this pane to be
   compact.
4. Harness portability notes from §2 (hardcoded Git path, no
   `GIT_CONFIG_GLOBAL` isolation in fixture setup).

## 8. Visual check (controller, on the final artifacts)

Re-inspected all four screenshots regenerated by the final 10/10 run, not the
worker's originals. Conversation surface dominates at both 1440x960 and
1024x720; Changes is a compact collapsible right pane with a close affordance,
not a dashboard; progressive disclosure runs repository selector → branch/HEAD/
count → status list. Repository ids are portable (`项目根仓库`, `vendor/nested`),
paths are project-relative, HEAD is short form only. No drive roots, temp paths,
UUIDs, session/run/proposal ids, argv/cwd/env, tokens, raw RPC or debug
metadata. Restart before/after are identical, showing clean rediscovery with no
stale generation. The only visual defect is the redundant labelling in §7.3.

## 9. Scope unchanged

Stage/unstage, commit, checkout, branch, merge, rebase, reset, discard, remote
operations, credentials, terminal, arbitrary Git commands, diff editing and
patch application all remain out of scope and were not implemented.
