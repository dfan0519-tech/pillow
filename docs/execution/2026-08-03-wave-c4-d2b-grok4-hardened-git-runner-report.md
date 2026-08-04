# C4 D2B Grok4 — hardened read-only Git runner development report

Date: 2026-08-03
Worker: real Grok CLI slot `grok4` (model Grok 4.5)
Worker session: `<keeper-session>`
Controller parent session: `<keeper-session>`
Task: `c4-d2b-hardened-git-runner`
Status: **COMPLETE** (not controller-accepted)

## Objective

Implement `GitReadExecutor::discover`, `status`, and `diff` with fixed internal
Git command templates over `SupervisedProcessRunner`. Resolve one trusted
`git.exe` from host configuration; never expose an arbitrary argv API. Enforce
clean environment, prompt/pager suppression, porcelain v2 `-z`,
`--no-ext-diff`, `--no-textconv`, and accepted status/diff bounds. Deny unsafe
local executable configuration and outside-root linked worktrees.

## Public boundary delivered

| Symbol | Role |
| --- | --- |
| `GitReadExecutor` | `new`, `discover`, `status`, `diff` |
| `GitReadError` | Typed fail-closed errors (no native roots / raw stderr) |
| `UnsafeConfigKind` | `Hook`, `Filter`, `Textconv`, `ExtDiff`, `Fsmonitor`, `SshCommand`, `ExecutableConfig` |
| `GitDiscovery` / `DiscoveredRepository` | Bounded project-relative discovery |
| `DISCOVERY_MAX_DEPTH` | **8** |
| `DISCOVERY_MAX_REPOS` | **32** |
| `GIT_READ_TIMEOUT` | default 30s constant |

Retained from D1 parser: `parse_porcelain_v2_z`, `bound_git_diff`, status/diff
DTO caps (5_000 entries / 2 MiB status / 256 KiB text diff).

## Exact fixed Git templates

Every invocation is:

```text
<trusted-git.exe>
  --no-pager
  --no-optional-locks
  -c core.fsmonitor=
  -c core.pager=cat
  -c credential.helper=
  <subcommand…>
```

| Semantic op | Subcommand argv after the fixed prefix |
| --- | --- |
| Is-inside-work-tree | `rev-parse --is-inside-work-tree` |
| Show toplevel | `rev-parse --show-toplevel` |
| Status | `status --porcelain=v2 -z --branch --untracked-files=normal` |
| Diff tracked | `diff --no-color --no-ext-diff --no-textconv -- <repo-rel-path>` |
| Diff untracked | `diff --no-color --no-ext-diff --no-textconv --no-index -- /dev/null <repo-rel-path>` |
| Local config probe | `config --local --list --null` |

Rules:

- No public argv API; only `discover` / `status` / `diff`.
- Portable path is the only caller-controlled string, and only after `--`.
- Paths starting with `-` are rejected (option-injection).
- `--no-optional-locks` is a **global** flag (before the subcommand).

## Clean environment

Built from scratch (`env_clear` in process-executor) with only:

| Name | Source |
| --- | --- |
| `SYSTEMROOT`, `WINDIR`, `COMSPEC`, `PATHEXT`, `TEMP`, `TMP` | Host, when present |
| `PATH` | Directory of trusted `git.exe` + Git mingw/usr bins + `SYSTEMROOT\System32` |
| `GIT_TERMINAL_PROMPT` | `0` |
| `GIT_PAGER` / `PAGER` | `cat` |
| `GIT_CONFIG_NOSYSTEM` | `1` |
| `LC_ALL` / `LANG` | `C` |

Omitted on purpose: `HOME`, `USERPROFILE`, `APPDATA`, `LOCALAPPDATA`, and any
name that would trip process-executor’s sensitive-name denylist (so no
`GIT_CONFIG_KEY_*` env overrides).

## Configuration denials

Before claiming a safe read, the runner denies when any of the following is
present:

| Class | Detection |
| --- | --- |
| **Hook** | Non-`.sample` file under resolved hooks dir; or `core.hooksPath` non-empty |
| **Filter** | `filter.*.clean` / `.smudge` / `.process` in local config; or `.gitattributes` `filter=` |
| **Textconv** | `diff.*.textconv` in local config |
| **ExtDiff** | `diff.external`, `interactive.diffFilter`, `diff.*.command`; custom `diff=` attribute drivers |
| **Fsmonitor** | `core.fsmonitor` non-empty / non-false (including `true`) |
| **SshCommand** | `core.sshCommand` non-empty |
| **ExecutableConfig** | `gpg.program`, `merge.*.driver` non-empty |

Local config is scanned from `.git/config` (+ worktree/common config for linked
worktrees) and re-checked via `git config --local --list --null`.

## Ownership / path denials

| Condition | Error |
| --- | --- |
| Trusted `git.exe` missing / not a file | `ExecutableUnavailable` |
| No `.git` / not inside work tree | `NotARepository` |
| Portable repo id does not resolve under project | `RepoNotFound` / `PathRejected` |
| `rev-parse --show-toplevel` leaves project root | `RepoOutsideProject` / `LinkedWorktreeOutsideProject` |
| Symlink/reparse components on resolve | `PathRejected` |
| Outside linked worktree not under project walk | not discovered (only in-tree markers) |
| Path `..` / absolute / non-portable | `PathRejected` |

All DTO paths are portable project-relative (nested repos prefix
`vendor/nested/…` onto repo-relative porcelain paths).

## Changed files (allowlist only)

| Path | Action |
| --- | --- |
| `crates/git-executor/Cargo.toml` | **Modified** — depend on `keeper-process-executor` |
| `crates/git-executor/src/lib.rs` | **Modified** — export runner + discovery |
| `crates/git-executor/src/runner.rs` | **Created** — `GitReadExecutor` + templates + safety |
| `crates/git-executor/src/discovery.rs` | **Created** — bounded walk + path mapping |
| `crates/git-executor/tests/git_read_integration.rs` | **Created** — 13 real-repo integration tests |
| `docs/execution/2026-08-03-wave-c4-d2b-grok4-hardened-git-runner-report.md` | **Created** — this report |

**Not modified by intent:** workspace `Cargo.toml`, protocol, policy,
process-executor sources, storage, tool-executor, runtime-host, Tauri,
renderer, scripts. No stage / commit / reset / restore / clean / package /
publish.

**Note:** `cargo` may refresh the already-dirty workspace `Cargo.lock` when
resolving the path dependency. This worker did not hand-edit lock or workspace
members. Controller owns any intentional lock merge.

## RED → GREEN evidence

### RED (pre-implementation)

Integration test module compiled against the missing `GitReadExecutor` surface
(expected missing types/modules). After scaffolding, first executable RED was
`CommandFailed { exit_code: Some(129) }` caused by placing
`--no-optional-locks` after `status` (status does not accept that flag; it is a
global option). Fixed by moving it before the subcommand.

### GREEN

```text
cargo test -p keeper-git-executor --test git_read_integration -- --nocapture
# 13 passed; 0 failed

cargo test -p keeper-git-executor --tests
# lib 0 + bounded_diff 6 + git_read_integration 13 + status_porcelain 15
# = 34 passed; 0 failed

cargo check --workspace --all-targets
# exit 0

cargo clippy -p keeper-git-executor --all-targets -- -D warnings
# exit 0

cargo fmt -p keeper-git-executor -- --check
# exit 0

cargo fmt --all --check
# exit 0 (this session)

git diff --check -- crates/git-executor
# exit 0
```

### Integration coverage map

| Requirement | Test |
| --- | --- |
| Non-repo / ordinary discovery | `discover_non_repository_returns_empty`, `discover_ordinary_repository_at_project_root` |
| Nested isolation + outside worktree | `discover_nested_repository_isolation`, `linked_worktree_outside_project_is_denied` |
| Ordinary / untracked / rename / binary / conflict / detached | `status_ordinary_modified_and_untracked`, `status_rename_binary_conflict_detached` |
| Bounded diff + oversized/binary + generation | `diff_text_binary_oversized_and_generation` |
| Prompt/pager/ext-diff/textconv suppression | `prompt_pager_ext_diff_textconv_are_suppressed` |
| Unsafe hook/filter/textconv/fsmonitor | `unsafe_hook_filter_textconv_and_executable_config_are_denied` |
| Trusted exe + typed process failure | `trusted_executable_resolution_failure_is_typed`, `typed_process_failure_for_missing_repo_path` |
| No mutation + no arbitrary argv | `read_ops_do_not_mutate_worktree`, `no_public_arbitrary_git_argv_surface` |

## Staging / HEAD / residuals

| Item | State |
| --- | --- |
| HEAD | `7b7f70f7360131cd1cfd984a8b19bdfb5725c21b` preserved |
| Staging | empty for this worker’s deliverables (allowlist untracked/modified only) |
| Residual processes | no owned cargo/rustc/pillow residuals retained after gates; no numeric-PID kill |
| Mutation verbs | not present in product templates (fixture setup only uses host `git` for temp repos) |

## Residual risks (controller)

1. **Git version floor:** templates assume Git for Windows 2.x with porcelain v2
   `-z`, `--no-textconv`, and global `--no-optional-locks`. Older Git may fail
   typed.
2. **Binary diff detection:** relies on Git’s `Binary files …` message and/or
   NUL scan via `bound_git_diff`. Some exotic binary encodings may still show as
   short text notices — still bounded.
3. **Capture encoding:** process-executor returns lossy UTF-8 text; porcelain
   paths are expected UTF-8. Non-UTF-8 paths fail closed at parse.
4. **Workspace lock:** if controller requires a clean lock commit, regenerate
   after merging all D2 crates; this worker did not stage lock.
5. **Hooks directory samples:** only non-`.sample` hook files deny; empty hooks
   dir is allowed.
6. **Not controller-accepted.** Gateway/UI wiring is Task 10+; this crate is the
   G0 runner foundation only.

## Keeper signal

`COMPLETION: complete` — C4-D2B hardened Git runner: fixed templates over
SupervisedProcessRunner; discover/status/diff; unsafe config denials; 34/34
package tests green; clippy -D warnings 0; fmt 0; scoped diff-check 0;
workspace check 0; HEAD 7b7f70f; staging empty; no commit.
