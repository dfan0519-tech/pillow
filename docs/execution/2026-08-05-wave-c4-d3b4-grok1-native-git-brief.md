# C4 D3B4 Round 1A — Grok1 native Tauri Git brief

You are the real Grok CLI `grok1` implementation worker. Compose the accepted
read-only `keeper-git-executor` into the Pillow Tauri host. Do not implement UI
or any Git mutation.

## Startup

1. Work only in `<repo>`.
2. Read `AGENTS.md`,
   `docs/superpowers/specs/2026-08-05-c4-git-changes-design.md`, and this brief.
3. Boot Keeper once as record-only: project `pillow`, task
   `c4-d3b4-native-git`, agentName `grok1`, agentRole `worker`. Do not ask
   Keeper to generate prompts, execute project work, judge completion, or
   ingest Vault content.
4. Inspect the dirty tree and preserve every unrelated change. Do not stage,
   commit, reset, clean, restore, or reformat unrelated files.

## Write allowlist

- Create `apps/desktop/src-tauri/src/git_commands.rs`
- Create `apps/desktop/src-tauri/src/git_commands_tests.rs`
- Modify `apps/desktop/src-tauri/src/lib.rs`
- Modify `apps/desktop/src-tauri/Cargo.toml`
- Create `docs/execution/2026-08-05-wave-c4-d3b4-grok1-native-git-report.md`

No other file may be changed.

## Required contract

- Add managed `GitCommandState` with per-project latest discovery authority:
  generation plus discovered portable repository ids/availability.
- Register exact Tauri commands `discover_git_repositories`, `get_git_status`,
  and `open_git_diff`.
- Resolve `projectId` only through `AppState::get_registered_project`; never
  accept a root, executable, argv, cwd, environment or native path from the
  renderer.
- Compose only `GitReadExecutor` fixed templates. Git executable resolution is
  host-only, canonicalized and fail-closed; missing Git maps to
  `git_unavailable`. Test seams must not become renderer authority.
- A discovery atomically replaces that project's allowed repositories and
  advances a JS-safe generation. Status/diff require exact current project,
  repository and generation; never merge drifted results.
- Serialize the exact DTO fields accepted by `gitGateway.ts` and no more.
- Map failures only to the six public `git_*` codes, with path-free messages.
- Preserve nested repo containment, unavailable repo semantics, unsafe-config
  denial, output bounds and portable project-relative paths.

Use focused deterministic Rust tests for registered/missing project, host Git
unavailable, discovery generation, stale/mismatch, missing/unavailable repo,
portable path rejection, nested repositories, path-free error serialization
and command registration. Prefer temporary fixtures and the existing real Git
test approach; do not read or mutate the Pillow repository through product
commands.

## Gates

```powershell
node scripts/run-cargo.mjs test -p pillow-desktop git_commands -- --nocapture
node scripts/run-cargo.mjs clippy -p pillow-desktop --all-targets -- -D warnings
node scripts/run-cargo.mjs fmt --all -- --check
git diff --check -- apps/desktop/src-tauri/src/git_commands.rs apps/desktop/src-tauri/src/git_commands_tests.rs apps/desktop/src-tauri/src/lib.rs apps/desktop/src-tauri/Cargo.toml docs/execution/2026-08-05-wave-c4-d3b4-grok1-native-git-report.md
```

If the focused test name filter cannot reach integration tests, run the
smallest correct `pillow-desktop` test command and document it. Do not run the
full desktop E2E.

## Completion

The report must contain changed files, architecture, public error mapping,
tests/exit codes, residual risk and explicit tree hygiene. End the final worker
response with:

`C4-D3B4-GROK1-NATIVE-GIT-COMPLETE`

