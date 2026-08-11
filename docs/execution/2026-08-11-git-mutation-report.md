# Git Mutation Backend Execution Report

Date: 2026-08-11

Implementation status: COMPLETE

Workspace gate status: PARTIAL — the scoped Git crate gates pass. The
workspace-wide gates are not green: `pnpm check` and one desktop test are
`ENV-BLOCKED`, while desktop clippy has out-of-scope concurrent failures.

Scope: structured backend Git mutation operations and Tauri command
registration only. No renderer implementation is included.

All Cargo commands below used the isolated `target-git-mut/` target directory.
No command output was piped through `tail` or another exit-code-masking command.

## 1. Exit-code table

| Gate | Exit code | Result | Evidence |
| --- | ---: | --- | --- |
| Initial mutation integration RED | 101 | EXPECTED RED | Missing semantic types and `stage` / `unstage` / `commit` / `push` methods. |
| Initial mutation integration GREEN | 0 | PASS | 7/7 temporary-repository tests passed. |
| Tauri command-layer RED | 101 | EXPECTED RED | Missing confirmation DTO, state methods, and command registrations. |
| Focused Tauri command-layer GREEN | 0 | PASS | 18/18 `git_commands_tests` passed. |
| Descendant-reparse regression RED | 101 | EXPECTED RED | A directory pathspec could reach a descendant junction. |
| Descendant-reparse regression GREEN | 0 | PASS | Junction is rejected as `PathRejected` before `git add`. |
| Fixed push-side-effect template RED | 101 | EXPECTED RED | Template lacked explicit no-signed/no-tags/no-mirror/no-submodule flags. |
| Fixed push-side-effect template GREEN | 0 | PASS | Exact argv test and real contained-remote push both passed. |
| Descendant-repository isolation RED | 101 | EXPECTED RED | Directory stage accepted a nested repository marker and could create a gitlink. |
| Descendant-repository isolation GREEN | 0 | PASS | A descendant `.git` marker is rejected before `git add`; the selected repository root remains allowed. |
| Local-remote target-shape RED | 101 | EXPECTED RED | Relative, non-bare worktree, and non-bare gitdir targets reached real temporary pushes. |
| Local-remote target-shape GREEN | 0 | PASS | Local push now requires an absolute, contained target with no `.git` marker and an explicit `core.bare=true` declaration. |
| Indirect-config denial RED | 101 | EXPECTED RED | URL-scoped credential helpers, alternate-ref commands, GC hooks, external metadata, and protocol overrides were not all denied. |
| Indirect-config denial GREEN | 0 | PASS | Those execution/escape surfaces are denied; command-valued `false` is not misread as boolean disable. |
| Full fixed-argv/protocol policy RED | 101 | EXPECTED RED | Exact vectors lacked the closed protocol and maintenance policy. |
| Full fixed-argv/protocol policy GREEN | 0 | PASS | Full vectors, including the URL probe, are pinned exactly; only HTTPS/file transports are admitted internally. |
| Reserved branch-component RED | 101 | EXPECTED RED | `topic.lock/main` passed the validator. |
| Reserved branch-component GREEN | 0 | PASS | Every ref component now rejects the reserved `.lock` suffix. |
| `cargo test -p keeper-git-executor` | 0 | PASS | Fresh closeout run: 102 tests passed, 0 failed. |
| `cargo clippy -p keeper-git-executor --all-targets -- -D warnings` | 0 | PASS | No warnings. |
| `cargo test -p pillow-desktop git_commands_tests::` | 0 | PASS | 18/18 focused command tests passed. |
| `cargo test -p pillow-desktop` | 101 | ENV-BLOCKED | 168 passed; the sole failure was the existing reparse test receiving Windows error 1314 while creating a directory symlink. No workaround was attempted. |
| `cargo clippy -p pillow-desktop --all-targets -- -D warnings` | 101 | OUT-OF-SCOPE FAIL | Diagnostics are in concurrent runtime-slot files and an existing runtime-composition test, not this task's files. |
| `pnpm check` | N/A | ENV-BLOCKED | PowerShell execution policy rejected `pnpm.ps1` before pnpm started; `$LASTEXITCODE` remained unset. The enclosing PowerShell invocation returning 0 is not treated as a green gate. |
| Scoped `rustfmt --check` | 0 | PASS | Only this task's Rust write set was checked, avoiding concurrent worker files. |
| `git diff --check` | 0 | PASS | No whitespace errors. |

## 2. Structured Git safety evidence

### 2.1 No caller-supplied argv, flags, or `-c`

- `crates/git-executor/src/mutation.rs:49-235` defines private validated value
  types and a crate-visible but opaque `PreparedGitMutation`. Its private
  operation enum has exactly four variants. The runner cannot construct a
  variant or insert an unvalidated value.
- `crates/git-executor/src/mutation.rs:111-165,207-235` is the sole mutation argv
  constructor. Every flag and every internal `-c` pair is a literal. The only
  inserted values come from validated path, remote, and branch wrappers. The
  fixed prefix denies unspecified protocols, admits only HTTPS/file, disables
  auto-maintenance, and the clean environment disables lazy fetch.
- `crates/git-executor/src/runner.rs:428-506` exposes only semantic methods;
  none accepts argv, flags, configuration, cwd, environment, executable, or a
  raw command name.
- `crates/git-executor/tests/git_read_integration.rs:1699-1755` pins the public
  method signatures. `crates/git-executor/src/mutation.rs:369-489` pins every
  complete argv vector, including the remote probe, and proves commit text is
  absent from argv.

### 2.2 Portable, project-scoped paths

- `crates/git-executor/src/mutation.rs:236-290` validates every input with the
  existing portable entry-path grammar both before and after nested-repository
  prefix reduction. Empty selections, excessive count/bytes, traversal,
  native separators, control characters, device names, and leading-dash
  repo-relative pathspecs are rejected.
- Every path is placed after a fixed `--` separator.
- `crates/git-executor/src/runner.rs:848-971` revalidates existing components
  with no-follow metadata checks. Directory pathspecs receive a bounded
  recursive scan (100,000 entries, 128 levels) so descendant symlinks,
  junctions, reparse points, and nested repository markers fail closed before
  Git traverses them. A missing leaf remains allowed so deletions can be staged.
- `apps/desktop/src-tauri/src/git_commands.rs:577-590` derives a fresh
  `ProjectCapability` from the registered project record. The state methods at
  `:258-353` additionally require membership in the latest project/repository/
  generation discovery authority. Discovery and mutations share one serial
  gate, and authorization is rechecked after acquiring it. No command accepts
  a native project root.

### 2.3 Validated commit messages, branch names, and remote names

- Bounds are 4,096 bytes for commit messages, 255 for branch names, and 64 for
  remote names (`crates/git-executor/src/mutation.rs:13-21`).
- The validators at `crates/git-executor/src/mutation.rs:293-367` reject empty
  values, leading `-`, control/NUL characters, over-limit values, and unsafe
  ref/name grammar, including `.lock` on every ref component. The branch
  allowlist cannot encode a refspec or option.
- Commit uses fixed `commit --no-verify --no-gpg-sign -F -`; message bytes are
  written only to the supervised child's stdin
  (`crates/git-executor/src/mutation.rs:124-165`,
  `crates/git-executor/src/runner.rs:719-793`). Raw Git stderr is never mapped
  into a public error.
- Before push, the runner obtains the effective push URL with a fixed probe and
  parses exactly one control-free output record. It accepts only HTTPS or an
  absolute, no-follow, same-project local repository proven bare by both shape
  and `core.bare=true`. Local hooks/config are inspected; relative/scp paths,
  non-bare targets, multiple URLs, and helper/URI forms such as `ext::` are
  rejected (`crates/git-executor/src/runner.rs:795-830,973-1068`).
- The local config classifier denies hooks/filters, URL-scoped credential
  helpers, remote programs, protocol overrides, alternate-ref commands, GC
  hooks, and external attributes/excludes/worktree paths. Command-valued
  `false` remains a command, not a disable signal
  (`crates/git-executor/src/runner.rs:1425-1500`).
- Fixed push arguments disable signed pushes, follow-tags, mirror behavior,
  hooks, and recursive submodule pushes, keeping the effect to the validated
  `refs/heads/<branch>:refs/heads/<branch>` refspec.

### 2.4 No arbitrary Git-command entry point

- The only new executor operations are `Stage`, `Unstage`, `Commit`, and
  `Push`. There is no string-command variant and no public template/builder.
- The only new Tauri registrations are `stage_git_paths`,
  `unstage_git_paths`, `commit_git_changes`, and `push_git_branch`
  (`apps/desktop/src-tauri/src/git_commands.rs:453-498`,
  `apps/desktop/src-tauri/src/lib.rs:114-120`).
- The registration test explicitly keeps arbitrary `run_git`, checkout,
  discard, and hard-reset surfaces absent
  (`apps/desktop/src-tauri/src/git_commands_tests.rs:747-777`).
- Unstage is the fixed path-scoped `reset -- <paths>` form. No `--hard` token
  or destructive worktree operation is represented by the type.

## 3. Adversarial and positive-path tests

| Case | Expected result | Evidence |
| --- | --- | --- |
| Branch `--upload-pack=malicious` | `BranchName` rejection / opaque `git_branch_rejected` | `crates/git-executor/tests/git_mutation_integration.rs:226-236`; command mapping at `apps/desktop/src-tauri/src/git_commands_tests.rs:616-665` |
| Message beginning with `-` | `CommitMessage` rejection / opaque `git_message_rejected` | `crates/git-executor/tests/git_mutation_integration.rs:214-224` |
| Path `../outside.txt` | `PathRejected` / opaque `git_path_rejected` | `crates/git-executor/tests/git_mutation_integration.rs:166-174` |
| NUL/newline/bell controls | Rejected for message, branch, remote, and probed target record | `crates/git-executor/tests/git_mutation_integration.rs:214-248`; parser test at `crates/git-executor/src/runner.rs:1765-1786` |
| `ext::` remote helper | Rejected before push | `crates/git-executor/tests/git_mutation_integration.rs:392-405` |
| Descendant junction under staged directory | Rejected before `git add` | `crates/git-executor/tests/git_mutation_integration.rs:176-194` |
| Descendant nested repository | Rejected before outer `git add` can create a gitlink | `crates/git-executor/tests/git_mutation_integration.rs:196-212` |
| Relative/non-bare local remotes | Rejected before push | `crates/git-executor/tests/git_mutation_integration.rs:336-390` |
| Normal stage → unstage → stage → commit | Real index and HEAD changes succeed | `crates/git-executor/tests/git_mutation_integration.rs:250-301` |
| Normal confirmed push | Real branch reaches a contained temporary bare remote | `crates/git-executor/tests/git_mutation_integration.rs:303-334` |
| Command DTO and opaque errors | Exact positive DTO allowlists plus negative mappings | `apps/desktop/src-tauri/src/git_commands_tests.rs:546-745` |

All positive Git writes occur in temporary repositories. No test or product
command performs a Git write against the Pillow working repository. The suite
therefore pins both rejection and real success; it is not rejection-only green.

## 4. Push confirmation design

`GitPushConfirmationInput` is a closed `deny_unknown_fields` input containing
`confirmed`, `remoteName`, and `branchName`
(`apps/desktop/src-tauri/src/git_commands.rs:94-103`). The target is inside the
same object as the marker, so there is no separately supplied target that can
diverge from what was confirmed. `GitCommandState::push` rejects
`confirmed == false` before resolving or running Git (`:323-353`). The executor
then independently validates both names and the effective remote target.

On success, the response reports the validated `remoteName` and `branchName`.
The command test proves false is rejected, true succeeds, the reported target
matches, and an unknown `argv` input key is denied
(`apps/desktop/src-tauri/src/git_commands_tests.rs:668-745`).

## 5. Write-boundary audit

This worker's write set is limited to:

- `crates/git-executor/Cargo.toml`
- `crates/git-executor/src/lib.rs`
- `crates/git-executor/src/mutation.rs`
- `crates/git-executor/src/runner.rs`
- `crates/git-executor/tests/git_mutation_integration.rs`
- `crates/git-executor/tests/git_read_integration.rs`
- `apps/desktop/src-tauri/src/git_commands.rs`
- `apps/desktop/src-tauri/src/git_commands_tests.rs`
- `apps/desktop/src-tauri/src/lib.rs` (imports and four registrations only)
- this report

No `apps/desktop/src/**`, script, dependency lockfile, or unrelated crate file
was written by this worker. `Cargo.lock` is unchanged and no dependency was
added. The live worktree does contain concurrent renderer, runtime-slot, and
storage changes from other workers; those were preserved and are not claimed
as this task's output.

This worker ran only read-only Git commands against the Pillow repository
(`status`, `diff`, and path audits). It did not stage, commit, reset, push, or
otherwise mutate repository state.

## 6. Residual risks

- The sole `pnpm check` entry could not start because PowerShell script
  execution is disabled. This is `ENV-BLOCKED`, not a green workspace gate.
- Desktop-wide test is `ENV-BLOCKED` by Windows symlink privilege. Desktop
  clippy is red in concurrent runtime-slot files plus one existing test. The
  focused Git command suite and Git executor crate gates are green.
- This task intentionally adds no renderer gateway. The UI task must admit the
  exact new DTO fields in its closed allowlists; otherwise the new commands
  will remain unreachable from the renderer.
- Push intentionally supports HTTPS and same-project absolute bare remotes
  only. Relative local paths, non-bare repositories, SSH/scp, Git protocol,
  out-of-project file transport, custom remote helpers, and multiple push URLs
  fail closed. The clean environment also disables ambient credential helpers
  and lazy fetch; authenticated HTTPS needs a future governed credential
  integration rather than inheriting host secrets.
- The positive push test uses a contained local bare remote. No external
  network push was performed.
- Discovery authority updates and mutation execution are serialized inside
  this command state, but a separate local process could still race-edit Git
  config or the worktree
  between no-follow validation and process access. The runner rechecks on every
  operation and fails closed where observable, but it does not hold an OS-level
  snapshot/lock across arbitrary external writers.
- `discard`, checkout, clean, and hard reset remain unimplemented by design.
- Desktop E2E was not run, as explicitly prohibited for this task.
