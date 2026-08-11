//! Hardened Git runner: fixed semantic templates over `SupervisedProcessRunner`.
//!
//! Callers never supply Git argv. The host resolves one trusted `git.exe`.

use std::collections::BTreeMap;
use std::ffi::OsString;
use std::fs;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::thread;
use std::time::Duration;
use std::time::Instant;

use keeper_process_executor::{
    BoundedOutput, CancellationSignal, ProcessError, ProcessSpec, SupervisedProcessRunner,
};

use crate::diff::{DIFF_TEXT_BYTE_CAP, GitDiffSnapshot, GitDiffView, bound_git_diff};
use crate::discovery::{
    DISCOVERY_MAX_REPOS, DiscoveredRepository, GitDiscovery, ensure_resolved_under_project,
    ensure_under_project, map_repo_path_to_project, meta_is_symlink_or_reparse,
    open_contained_entry, parse_gitfile, path_exists_no_follow, project_relative_path,
    read_metadata_under_base, resolve_repo_workdir, resolve_under_base_no_follow,
    strip_verbatim_prefix, walk_git_markers,
};
use crate::mutation::{GitInputKind, GitMutationReceipt, PreparedGitMutation};
use crate::status::{
    GitRepositoryId, GitStatusSnapshot, ParseError, STATUS_TRANSPORT_CAP, parse_porcelain_v2_z,
    validate_portable_path,
};

/// Default wall-clock budget for a single Git read invocation.
pub const GIT_READ_TIMEOUT: Duration = Duration::from_secs(30);

/// Opaque configuration denial classes (no native paths in messages).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UnsafeConfigKind {
    Hook,
    Filter,
    Textconv,
    ExtDiff,
    Fsmonitor,
    SshCommand,
    ExecutableConfig,
    /// Local `include.path` / `includeIf.*.path` pulls external config.
    Include,
    /// Configured program or helper that a network operation could execute.
    RemoteCommand,
}

/// Typed Git read failures (public surface; no native roots / raw stderr).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GitReadError {
    ExecutableUnavailable,
    NotARepository,
    RepoNotFound,
    RepoOutsideProject,
    LinkedWorktreeOutsideProject,
    UnsafeConfiguration {
        kind: UnsafeConfigKind,
    },
    /// Config wire form was present but not a well-formed `key\\nvalue\\0` list.
    MalformedConfiguration,
    /// Capture truncated, lossy UTF-8, or otherwise not trustworthy to parse/publish.
    OutputUnreliable,
    PathRejected,
    InputRejected {
        kind: GitInputKind,
    },
    Parse(ParseError),
    Process(ProcessError),
    CommandFailed {
        exit_code: Option<i32>,
    },
    TimedOut,
    Cancelled,
}

impl std::fmt::Display for GitReadError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::ExecutableUnavailable => write!(f, "git executable unavailable"),
            Self::NotARepository => write!(f, "not a git repository"),
            Self::RepoNotFound => write!(f, "repository not found"),
            Self::RepoOutsideProject => write!(f, "repository outside project"),
            Self::LinkedWorktreeOutsideProject => {
                write!(f, "linked worktree outside project")
            }
            Self::UnsafeConfiguration { kind } => {
                write!(f, "unsafe git configuration: {kind:?}")
            }
            Self::MalformedConfiguration => write!(f, "malformed git configuration"),
            Self::OutputUnreliable => write!(f, "git output unreliable"),
            Self::PathRejected => write!(f, "path rejected"),
            Self::InputRejected { kind } => write!(f, "git input rejected: {kind:?}"),
            Self::Parse(e) => write!(f, "git parse error: {e}"),
            Self::Process(e) => write!(f, "process error: {e}"),
            Self::CommandFailed { exit_code } => {
                write!(f, "git command failed (exit={exit_code:?})")
            }
            Self::TimedOut => write!(f, "git command timed out"),
            Self::Cancelled => write!(f, "git command cancelled"),
        }
    }
}

impl std::error::Error for GitReadError {}

impl From<ParseError> for GitReadError {
    fn from(value: ParseError) -> Self {
        Self::Parse(value)
    }
}

impl From<ProcessError> for GitReadError {
    fn from(value: ProcessError) -> Self {
        match value {
            ProcessError::ExecutableUnavailable => Self::ExecutableUnavailable,
            other => Self::Process(other),
        }
    }
}

/// Fixed internal Git command templates (never exposed as a public argv API).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum GitTemplate {
    /// `rev-parse --is-inside-work-tree`
    IsInsideWorkTree,
    /// `rev-parse --show-toplevel`
    ShowToplevel,
    /// `status --porcelain=v2 -z --branch --untracked-files=normal --no-optional-locks`
    StatusPorcelainV2,
    /// `diff --no-color --no-ext-diff --no-textconv -- <path>`
    DiffTracked,
    /// `diff --no-color --no-ext-diff --no-textconv --no-index -- /dev/null <path>`
    DiffUntracked,
    /// `config --local --list --null`
    ConfigLocalList,
}

impl GitTemplate {
    fn argv(self, path: Option<&str>) -> Vec<OsString> {
        // Fixed host template only. Never accept caller-controlled segments
        // except a single validated portable path after `--`.
        //
        // Leading safety flags (not user-controlled):
        //   --no-pager
        //   -c core.fsmonitor=     disable fsmonitor hook process
        //   -c core.pager=cat      belt-and-suspenders with --no-pager
        //   -c credential.helper=  no credential helper prompts
        // Global options must precede the subcommand.
        let mut args: Vec<OsString> = vec![
            OsString::from("--no-pager"),
            OsString::from("--no-optional-locks"),
            OsString::from("-c"),
            OsString::from("core.fsmonitor="),
            OsString::from("-c"),
            OsString::from("core.pager=cat"),
            OsString::from("-c"),
            OsString::from("credential.helper="),
        ];
        match self {
            Self::IsInsideWorkTree => {
                args.extend([
                    OsString::from("rev-parse"),
                    OsString::from("--is-inside-work-tree"),
                ]);
            }
            Self::ShowToplevel => {
                args.extend([
                    OsString::from("rev-parse"),
                    OsString::from("--show-toplevel"),
                ]);
            }
            Self::StatusPorcelainV2 => {
                args.extend([
                    OsString::from("status"),
                    OsString::from("--porcelain=v2"),
                    OsString::from("-z"),
                    OsString::from("--branch"),
                    OsString::from("--untracked-files=normal"),
                ]);
            }
            Self::DiffTracked => {
                let path = path.expect("diff path");
                args.extend([
                    OsString::from("diff"),
                    OsString::from("--no-color"),
                    OsString::from("--no-ext-diff"),
                    OsString::from("--no-textconv"),
                    OsString::from("--"),
                    OsString::from(path),
                ]);
            }
            Self::DiffUntracked => {
                let path = path.expect("diff path");
                args.extend([
                    OsString::from("diff"),
                    OsString::from("--no-color"),
                    OsString::from("--no-ext-diff"),
                    OsString::from("--no-textconv"),
                    OsString::from("--no-index"),
                    OsString::from("--"),
                    OsString::from("/dev/null"),
                    OsString::from(path),
                ]);
            }
            Self::ConfigLocalList => {
                // Fixed no-includes probe: do not let Git follow include.path /
                // includeIf during the local list. Raw contained config still
                // classifies include directives independently.
                args.extend([
                    OsString::from("config"),
                    OsString::from("--local"),
                    OsString::from("--list"),
                    OsString::from("--null"),
                    OsString::from("--no-includes"),
                ]);
            }
        }
        args
    }
}

/// Resolved git metadata paths that stay inside the project boundary.
struct ContainedGitDirs {
    /// Actual gitdir (`.git` directory or linked worktree gitdir).
    gitdir: PathBuf,
    /// Common dir (same as gitdir for ordinary repos; main `.git` for linked).
    common_dir: PathBuf,
}

/// Hardened structured Git executor with a closed semantic operation surface.
#[derive(Debug, Clone)]
pub struct GitReadExecutor {
    git_executable: PathBuf,
    timeout: Duration,
    runner: SupervisedProcessRunner,
}

impl GitReadExecutor {
    /// Construct from a host-resolved trusted `git.exe` path.
    ///
    /// Fails with [`GitReadError::ExecutableUnavailable`] when the path is not
    /// a regular file. Does not search `PATH`.
    pub fn new(git_executable: PathBuf, timeout: Duration) -> Result<Self, GitReadError> {
        if timeout.is_zero() || !git_executable.is_file() {
            return Err(GitReadError::ExecutableUnavailable);
        }
        Ok(Self {
            git_executable,
            timeout,
            runner: SupervisedProcessRunner::new(),
        })
    }

    /// Discover Git worktrees under `project_root` (bounded depth/count).
    pub fn discover(&self, project_root: &Path) -> Result<GitDiscovery, GitReadError> {
        let root = ensure_under_project(project_root, project_root)?;
        let markers = walk_git_markers(&root)?;
        let mut repositories = Vec::new();

        for worktree in markers {
            if repositories.len() >= DISCOVERY_MAX_REPOS {
                break;
            }
            match self.classify_worktree(&root, &worktree) {
                Ok(repo) => repositories.push(repo),
                Err(GitReadError::LinkedWorktreeOutsideProject)
                | Err(GitReadError::RepoOutsideProject) => {
                    // Outside linked worktree / outside gitdir: surface as
                    // unavailable only when the worktree directory itself is
                    // still under the project.
                    if let Ok(rel) = project_relative_path(&root, &worktree) {
                        repositories.push(DiscoveredRepository {
                            project_relative: rel,
                            available: false,
                        });
                    }
                }
                Err(GitReadError::NotARepository) | Err(GitReadError::RepoNotFound) => {
                    continue;
                }
                Err(other) => return Err(other),
            }
        }

        repositories.sort_by(|a, b| a.project_relative.cmp(&b.project_relative));
        Ok(GitDiscovery { repositories })
    }

    /// Read a bounded porcelain v2 status snapshot for one repository.
    pub fn status(
        &self,
        project_root: &Path,
        repository: &GitRepositoryId,
        generation: u64,
    ) -> Result<GitStatusSnapshot, GitReadError> {
        let workdir = self.prepare_repo(project_root, repository)?;
        self.deny_unsafe_configuration(project_root, &workdir)?;

        let receipt = self.run_git(
            &workdir,
            GitTemplate::StatusPorcelainV2,
            None,
            STATUS_TRANSPORT_CAP.saturating_add(4096),
        )?;
        // status exits 0 on success even with dirty trees.
        if receipt.exit_code != Some(0) {
            return Err(GitReadError::CommandFailed {
                exit_code: receipt.exit_code,
            });
        }
        // Fail closed: never publish lossy or truncated status.
        require_reliable_publishable(&receipt.stdout)?;
        if receipt.stdout.bytes_seen > STATUS_TRANSPORT_CAP {
            return Err(GitReadError::OutputUnreliable);
        }

        let bytes = receipt.stdout.text.into_bytes();
        let mut snap = parse_porcelain_v2_z(&bytes, repository.clone(), generation)?;
        if snap.truncated {
            return Err(GitReadError::OutputUnreliable);
        }
        // Map repo-relative paths to project-relative portable paths.
        for entry in &mut snap.entries {
            entry.path = map_repo_path_to_project(&repository.project_relative, &entry.path)?;
            if let Some(old) = entry.old_path.take() {
                entry.old_path = Some(map_repo_path_to_project(
                    &repository.project_relative,
                    &old,
                )?);
            }
        }
        Ok(snap)
    }

    /// Bounded one-path diff against HEAD (tracked) or as an untracked add.
    ///
    /// The response is a [`GitDiffSnapshot`] carrying repository identity and
    /// the exact caller-supplied `generation` so later layers can reject stale
    /// results.
    pub fn diff(
        &self,
        project_root: &Path,
        repository: &GitRepositoryId,
        path: &str,
        generation: u64,
    ) -> Result<GitDiffSnapshot, GitReadError> {
        validate_portable_path(path).map_err(|_| GitReadError::PathRejected)?;
        let workdir = self.prepare_repo(project_root, repository)?;
        self.deny_unsafe_configuration(project_root, &workdir)?;

        let repo_rel = strip_repo_prefix(&repository.project_relative, path)?;
        // Reject option-injection shaped paths.
        if repo_rel.starts_with('-') {
            return Err(GitReadError::PathRejected);
        }

        // Prefer tracked diff; fall back to untracked --no-index when empty
        // and the path exists in the worktree as a contained regular file.
        let cap = DIFF_TEXT_BYTE_CAP.saturating_add(8192);
        let tracked = self.run_git(&workdir, GitTemplate::DiffTracked, Some(&repo_rel), cap)?;

        let (raw, bytes_seen, truncated, lossy) = if tracked.exit_code == Some(0)
            && !tracked.stdout.text.is_empty()
        {
            require_reliable_or_oversized(&tracked.stdout)?;
            (
                stdout_bytes(&tracked),
                tracked.stdout.bytes_seen,
                tracked.stdout.truncated,
                tracked.stdout.lossy_utf8,
            )
        } else if tracked.exit_code == Some(0) && tracked.stdout.text.is_empty() {
            // Unchanged tracked or untracked — try untracked presentation
            // only after proving the path is a regular file under authority.
            // Symlink / reparse components fail closed (never empty success).
            match probe_contained_file(project_root, &workdir, &repo_rel)? {
                Some(_abs) => {
                    let untracked =
                        self.run_git(&workdir, GitTemplate::DiffUntracked, Some(&repo_rel), cap)?;
                    // --no-index returns 1 when files differ; 0 when identical.
                    if matches!(untracked.exit_code, Some(0) | Some(1)) {
                        require_reliable_or_oversized(&untracked.stdout)?;
                        (
                            stdout_bytes(&untracked),
                            untracked.stdout.bytes_seen,
                            untracked.stdout.truncated,
                            untracked.stdout.lossy_utf8,
                        )
                    } else {
                        return Err(GitReadError::CommandFailed {
                            exit_code: untracked.exit_code,
                        });
                    }
                }
                None => (Vec::new(), 0, false, false),
            }
        } else if matches!(tracked.exit_code, Some(0) | Some(1)) {
            require_reliable_or_oversized(&tracked.stdout)?;
            (
                stdout_bytes(&tracked),
                tracked.stdout.bytes_seen,
                tracked.stdout.truncated,
                tracked.stdout.lossy_utf8,
            )
        } else {
            return Err(GitReadError::CommandFailed {
                exit_code: tracked.exit_code,
            });
        };

        // Lossy UTF-8 is never publishable as text. Oversized truncated captures
        // map to the typed Oversized summary instead of partial text.
        if lossy {
            return Err(GitReadError::Parse(ParseError::Utf8));
        }
        let view = classify_diff(path, &raw, bytes_seen, truncated)?;
        Ok(GitDiffSnapshot {
            repository: repository.clone(),
            generation,
            view,
        })
    }

    /// Stage a bounded set of portable paths. Callers cannot supply flags or argv.
    pub fn stage(
        &self,
        project_root: &Path,
        repository: &GitRepositoryId,
        paths: &[String],
    ) -> Result<GitMutationReceipt, GitReadError> {
        let prepared = PreparedGitMutation::stage(&repository.project_relative, paths)?;
        self.execute_mutation(project_root, repository, &prepared)
    }

    /// Remove a bounded set of portable paths from the index without touching the worktree.
    pub fn unstage(
        &self,
        project_root: &Path,
        repository: &GitRepositoryId,
        paths: &[String],
    ) -> Result<GitMutationReceipt, GitReadError> {
        let prepared = PreparedGitMutation::unstage(&repository.project_relative, paths)?;
        self.execute_mutation(project_root, repository, &prepared)
    }

    /// Commit the current index with a validated single-line message sent on stdin.
    pub fn commit(
        &self,
        project_root: &Path,
        repository: &GitRepositoryId,
        message: &str,
    ) -> Result<GitMutationReceipt, GitReadError> {
        let prepared = PreparedGitMutation::commit(message)?;
        self.execute_mutation(project_root, repository, &prepared)
    }

    /// Push one validated local branch to the same branch on one validated remote.
    pub fn push(
        &self,
        project_root: &Path,
        repository: &GitRepositoryId,
        remote_name: &str,
        branch_name: &str,
    ) -> Result<GitMutationReceipt, GitReadError> {
        let prepared = PreparedGitMutation::push(remote_name, branch_name)?;
        self.execute_mutation(project_root, repository, &prepared)
    }

    fn execute_mutation(
        &self,
        project_root: &Path,
        repository: &GitRepositoryId,
        prepared: &PreparedGitMutation,
    ) -> Result<GitMutationReceipt, GitReadError> {
        let workdir = self.prepare_repo(project_root, repository)?;
        self.deny_unsafe_configuration(project_root, &workdir)?;

        let mut inspected_entries = 0usize;
        for pathspec in prepared.pathspecs() {
            ensure_mutation_path_contained(
                project_root,
                &workdir,
                pathspec,
                &mut inspected_entries,
            )?;
        }
        if prepared.push_url_probe_argv().is_some() {
            self.validate_push_targets(project_root, &workdir, prepared)?;
        }

        let exit_code = if let Some(stdin) = prepared.stdin_bytes() {
            self.run_mutation_with_stdin(&workdir, prepared, stdin)?
        } else {
            self.run_mutation(&workdir, prepared)?
        };
        if exit_code != Some(0) {
            return Err(GitReadError::CommandFailed { exit_code });
        }
        Ok(prepared.receipt())
    }

    fn classify_worktree(
        &self,
        project_root: &Path,
        worktree: &Path,
    ) -> Result<DiscoveredRepository, GitReadError> {
        let worktree = ensure_under_project(project_root, worktree)?;
        let git_marker = worktree.join(".git");
        if !path_exists_no_follow(&git_marker) {
            return Err(GitReadError::NotARepository);
        }

        // Linked worktree: `.git` is a file pointing at a gitdir. Outside
        // gitdir/common-dir ⇒ unavailable (do not read outside metadata).
        let meta = fs::symlink_metadata(&git_marker).map_err(|_| GitReadError::NotARepository)?;
        if meta_is_symlink_or_reparse(&meta) {
            return Err(GitReadError::PathRejected);
        }
        let dirs = if meta.file_type().is_file() {
            let dirs = resolve_contained_git_dirs(project_root, &worktree)?;
            // Filesystem metadata before any git process.
            match deny_filesystem_metadata(project_root, &worktree, &dirs) {
                Ok(()) => {}
                Err(GitReadError::UnsafeConfiguration { .. }) => {
                    let rel = project_relative_path(project_root, &worktree)?;
                    return Ok(DiscoveredRepository {
                        project_relative: rel,
                        available: false,
                    });
                }
                Err(other) => return Err(other),
            }
            let toplevel = self.show_toplevel(&worktree)?;
            ensure_under_project(project_root, &toplevel)?;
            if strip_verbatim_prefix(toplevel) != worktree {
                return Err(GitReadError::LinkedWorktreeOutsideProject);
            }
            dirs
        } else if meta.is_dir() {
            let dirs = resolve_contained_git_dirs(project_root, &worktree)?;
            match deny_filesystem_metadata(project_root, &worktree, &dirs) {
                Ok(()) => {}
                Err(GitReadError::UnsafeConfiguration { .. }) => {
                    let rel = project_relative_path(project_root, &worktree)?;
                    return Ok(DiscoveredRepository {
                        project_relative: rel,
                        available: false,
                    });
                }
                Err(other) => return Err(other),
            }
            let toplevel = self.show_toplevel(&worktree)?;
            ensure_under_project(project_root, &toplevel)?;
            dirs
        } else {
            return Err(GitReadError::NotARepository);
        };

        // Full safety scan (includes git config --list --no-includes probe).
        match self.deny_unsafe_configuration(project_root, &worktree) {
            Ok(()) => {}
            Err(GitReadError::UnsafeConfiguration { .. }) => {
                let rel = project_relative_path(project_root, &worktree)?;
                return Ok(DiscoveredRepository {
                    project_relative: rel,
                    available: false,
                });
            }
            Err(other) => return Err(other),
        }
        let _ = dirs;

        let rel = project_relative_path(project_root, &worktree)?;
        Ok(DiscoveredRepository {
            project_relative: rel,
            available: true,
        })
    }

    fn prepare_repo(
        &self,
        project_root: &Path,
        repository: &GitRepositoryId,
    ) -> Result<PathBuf, GitReadError> {
        validate_portable_path(&repository.project_relative)
            .map_err(|_| GitReadError::PathRejected)?;
        let workdir = resolve_repo_workdir(project_root, &repository.project_relative)?;
        let git_marker = workdir.join(".git");
        if !path_exists_no_follow(&git_marker) {
            return Err(GitReadError::NotARepository);
        }

        // Contained gitdir/common-dir required before any config/hooks/attributes.
        let dirs = resolve_contained_git_dirs(project_root, &workdir)?;
        // Filesystem metadata scan must run *before* any git process that would
        // load config/includes/attributes (whitespace include, linked config, …).
        deny_filesystem_metadata(project_root, &workdir, &dirs)?;

        // Confirm Git agrees this is a work tree.
        let inside = self.run_git(&workdir, GitTemplate::IsInsideWorkTree, None, 64)?;
        require_reliable_publishable(&inside.stdout)?;
        if inside.exit_code != Some(0) || inside.stdout.text.trim() != "true" {
            return Err(GitReadError::NotARepository);
        }

        let toplevel = self.show_toplevel(&workdir)?;
        ensure_under_project(project_root, &toplevel)?;
        Ok(workdir)
    }

    fn show_toplevel(&self, workdir: &Path) -> Result<PathBuf, GitReadError> {
        let receipt = self.run_git(workdir, GitTemplate::ShowToplevel, None, 4096)?;
        require_reliable_publishable(&receipt.stdout)?;
        if receipt.exit_code != Some(0) {
            return Err(GitReadError::NotARepository);
        }
        let text = receipt.stdout.text.trim();
        if text.is_empty() {
            return Err(GitReadError::NotARepository);
        }
        let path = PathBuf::from(text);
        let canon = fs::canonicalize(&path).map_err(|_| GitReadError::NotARepository)?;
        Ok(strip_verbatim_prefix(canon))
    }

    fn deny_unsafe_configuration(
        &self,
        project_root: &Path,
        workdir: &Path,
    ) -> Result<(), GitReadError> {
        let dirs = resolve_contained_git_dirs(project_root, workdir)?;

        // 1–2 + 4) Filesystem metadata (hooks, raw config, attributes) first.
        deny_filesystem_metadata(project_root, workdir, &dirs)?;

        // 3) Ask git for local list — fail closed on any probe uncertainty.
        // Wire form is the real Git form: `key\nvalue\0`. `--no-includes`
        // prevents Git from following include.path during this probe.
        let list = self.run_git(workdir, GitTemplate::ConfigLocalList, None, 256 * 1024)?;
        if list.exit_code != Some(0) {
            return Err(GitReadError::CommandFailed {
                exit_code: list.exit_code,
            });
        }
        if list.stdout.truncated {
            return Err(GitReadError::OutputUnreliable);
        }
        if list.stdout.lossy_utf8 {
            return Err(GitReadError::Parse(ParseError::Utf8));
        }
        if let Some(kind) = parse_config_null_list(&list.stdout.text)? {
            return Err(GitReadError::UnsafeConfiguration { kind });
        }

        Ok(())
    }

    fn run_git(
        &self,
        cwd: &Path,
        template: GitTemplate,
        path: Option<&str>,
        output_byte_cap: usize,
    ) -> Result<keeper_process_executor::ProcessReceipt, GitReadError> {
        let env = clean_git_env(&self.git_executable)?;
        let spec = ProcessSpec {
            executable: self.git_executable.clone(),
            argv: template.argv(path),
            cwd: cwd.to_path_buf(),
            env,
            timeout: self.timeout,
            output_byte_cap,
        };
        let receipt = self
            .runner
            .run(spec, CancellationSignal::new())
            .map_err(GitReadError::from)?;
        if receipt.timed_out {
            return Err(GitReadError::TimedOut);
        }
        if receipt.cancelled {
            return Err(GitReadError::Cancelled);
        }
        Ok(receipt)
    }

    fn run_mutation(
        &self,
        cwd: &Path,
        prepared: &PreparedGitMutation,
    ) -> Result<Option<i32>, GitReadError> {
        let spec = ProcessSpec {
            executable: self.git_executable.clone(),
            argv: prepared.argv(),
            cwd: cwd.to_path_buf(),
            env: clean_git_env(&self.git_executable)?,
            timeout: self.timeout,
            output_byte_cap: 64 * 1024,
        };
        let receipt = self
            .runner
            .run(spec, CancellationSignal::new())
            .map_err(GitReadError::from)?;
        if receipt.timed_out {
            return Err(GitReadError::TimedOut);
        }
        if receipt.cancelled {
            return Err(GitReadError::Cancelled);
        }
        Ok(receipt.exit_code)
    }

    /// Use the shared executor's owned supervised child for `commit -F -`.
    /// The same Job Object / stable child handle, timeout cadence, and
    /// kill-tree proof as ordinary reads remain authoritative.
    fn run_mutation_with_stdin(
        &self,
        cwd: &Path,
        prepared: &PreparedGitMutation,
        stdin_bytes: &[u8],
    ) -> Result<Option<i32>, GitReadError> {
        let spec = ProcessSpec {
            executable: self.git_executable.clone(),
            argv: prepared.argv(),
            cwd: cwd.to_path_buf(),
            env: clean_git_env(&self.git_executable)?,
            timeout: self.timeout,
            output_byte_cap: 64 * 1024,
        };
        let mut child = self.runner.spawn_piped(spec).map_err(GitReadError::from)?;
        let stdout = match child.take_stdout() {
            Some(stdout) => stdout,
            None => {
                child
                    .kill_tree(Duration::from_secs(2))
                    .map_err(GitReadError::from)?;
                return Err(GitReadError::Process(ProcessError::Setup));
            }
        };
        let drain = thread::spawn(move || -> std::io::Result<()> {
            let mut stdout = stdout;
            let mut buffer = [0_u8; 8_192];
            loop {
                if stdout.read(&mut buffer)? == 0 {
                    return Ok(());
                }
            }
        });

        let mut stdin = match child.take_stdin() {
            Some(stdin) => stdin,
            None => {
                child
                    .kill_tree(Duration::from_secs(2))
                    .map_err(GitReadError::from)?;
                join_discarded_stdout(drain)?;
                return Err(GitReadError::Process(ProcessError::Setup));
            }
        };
        if stdin.write_all(stdin_bytes).is_err() {
            drop(stdin);
            child
                .kill_tree(Duration::from_secs(2))
                .map_err(GitReadError::from)?;
            join_discarded_stdout(drain)?;
            return Err(GitReadError::Process(ProcessError::Output));
        }
        drop(stdin);

        let started = Instant::now();
        let exit_code = loop {
            if started.elapsed() >= self.timeout {
                child
                    .kill_tree(Duration::from_secs(2))
                    .map_err(GitReadError::from)?;
                join_discarded_stdout(drain)?;
                return Err(GitReadError::TimedOut);
            }
            if let Some(exit_code) = child.try_wait().map_err(GitReadError::from)? {
                break Some(exit_code);
            }
            thread::sleep(Duration::from_millis(10));
        };

        child
            .kill_tree(Duration::from_secs(2))
            .map_err(GitReadError::from)?;
        join_discarded_stdout(drain)?;
        Ok(exit_code)
    }

    fn validate_push_targets(
        &self,
        project_root: &Path,
        workdir: &Path,
        prepared: &PreparedGitMutation,
    ) -> Result<(), GitReadError> {
        let argv = prepared
            .push_url_probe_argv()
            .ok_or(GitReadError::InputRejected {
                kind: GitInputKind::RemoteTarget,
            })?;
        let spec = ProcessSpec {
            executable: self.git_executable.clone(),
            argv,
            cwd: workdir.to_path_buf(),
            env: clean_git_env(&self.git_executable)?,
            timeout: self.timeout,
            output_byte_cap: 64 * 1024,
        };
        let receipt = self
            .runner
            .run(spec, CancellationSignal::new())
            .map_err(GitReadError::from)?;
        if receipt.timed_out {
            return Err(GitReadError::TimedOut);
        }
        if receipt.cancelled {
            return Err(GitReadError::Cancelled);
        }
        if receipt.exit_code != Some(0) {
            return Err(GitReadError::CommandFailed {
                exit_code: receipt.exit_code,
            });
        }
        require_reliable_publishable(&receipt.stdout)?;
        let target = parse_single_push_target(&receipt.stdout.text)?;
        validate_push_target(project_root, target)?;
        Ok(())
    }
}

fn join_discarded_stdout(
    drain: thread::JoinHandle<std::io::Result<()>>,
) -> Result<(), GitReadError> {
    drain
        .join()
        .map_err(|_| GitReadError::Process(ProcessError::Output))?
        .map_err(|_| GitReadError::Process(ProcessError::Output))
}

/// Prove each existing path component is a real in-project entry. A missing
/// leaf is allowed so deletion pathspecs can still be staged/unstaged; the
/// remaining lexical components have already passed the portable grammar.
fn ensure_mutation_path_contained(
    project_root: &Path,
    workdir: &Path,
    pathspec: &str,
    inspected_entries: &mut usize,
) -> Result<(), GitReadError> {
    let directory_requested = pathspec.ends_with('/');
    let pathspec = pathspec.strip_suffix('/').unwrap_or(pathspec);
    let canonical_workdir = fs::canonicalize(workdir).map_err(|_| GitReadError::PathRejected)?;
    ensure_resolved_under_project(project_root, &canonical_workdir)?;
    if pathspec == "." {
        return scan_mutation_directory(
            project_root,
            &canonical_workdir,
            &canonical_workdir,
            inspected_entries,
        );
    }

    let components: Vec<&str> = pathspec.split('/').collect();
    let mut current = canonical_workdir.clone();
    let mut final_is_directory = false;
    for (index, component) in components.iter().enumerate() {
        current.push(component);
        let metadata = match fs::symlink_metadata(&current) {
            Ok(metadata) => metadata,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
            Err(_) => return Err(GitReadError::PathRejected),
        };
        if meta_is_symlink_or_reparse(&metadata) {
            return Err(GitReadError::PathRejected);
        }
        if index + 1 < components.len() && !metadata.is_dir() {
            return Err(GitReadError::PathRejected);
        }
        final_is_directory = metadata.is_dir();
        let canonical = fs::canonicalize(&current).map_err(|_| GitReadError::PathRejected)?;
        if canonical != canonical_workdir && !canonical.starts_with(&canonical_workdir) {
            return Err(GitReadError::PathRejected);
        }
        ensure_resolved_under_project(project_root, &canonical)?;
        if metadata.is_dir() {
            ensure_not_descendant_repository(&canonical_workdir, &canonical)?;
        }
        current = canonical;
    }
    if directory_requested && !final_is_directory {
        return Err(GitReadError::PathRejected);
    }
    if final_is_directory {
        scan_mutation_directory(
            project_root,
            &canonical_workdir,
            &current,
            inspected_entries,
        )?;
    }
    Ok(())
}

/// `git add <directory>` recursively walks descendants. Mirror that authority
/// boundary without following any symlink/reparse entry before Git sees it.
fn scan_mutation_directory(
    project_root: &Path,
    canonical_workdir: &Path,
    directory: &Path,
    inspected_entries: &mut usize,
) -> Result<(), GitReadError> {
    const MUTATION_TREE_ENTRY_MAX: usize = 100_000;
    const MUTATION_TREE_DEPTH_MAX: usize = 128;

    let mut pending = vec![(directory.to_path_buf(), 0usize)];
    while let Some((current, depth)) = pending.pop() {
        if depth > MUTATION_TREE_DEPTH_MAX {
            return Err(GitReadError::PathRejected);
        }
        ensure_not_descendant_repository(canonical_workdir, &current)?;
        let entries = fs::read_dir(&current).map_err(|_| GitReadError::PathRejected)?;
        for entry in entries {
            let entry = entry.map_err(|_| GitReadError::PathRejected)?;
            *inspected_entries = inspected_entries
                .checked_add(1)
                .ok_or(GitReadError::PathRejected)?;
            if *inspected_entries > MUTATION_TREE_ENTRY_MAX {
                return Err(GitReadError::PathRejected);
            }
            let path = entry.path();
            let metadata = fs::symlink_metadata(&path).map_err(|_| GitReadError::PathRejected)?;
            if meta_is_symlink_or_reparse(&metadata) {
                return Err(GitReadError::PathRejected);
            }

            // Git treats repository metadata specially and never stages it.
            // Inspect the entry itself above, then omit its descendants.
            if entry.file_name() == ".git" {
                continue;
            }

            let canonical = fs::canonicalize(&path).map_err(|_| GitReadError::PathRejected)?;
            if canonical != canonical_workdir && !canonical.starts_with(canonical_workdir) {
                return Err(GitReadError::PathRejected);
            }
            ensure_resolved_under_project(project_root, &canonical)?;
            if metadata.is_dir() {
                pending.push((canonical, depth.saturating_add(1)));
            }
        }
    }
    Ok(())
}

fn ensure_not_descendant_repository(
    canonical_workdir: &Path,
    directory: &Path,
) -> Result<(), GitReadError> {
    if directory == canonical_workdir {
        return Ok(());
    }
    match fs::symlink_metadata(directory.join(".git")) {
        Ok(_) => Err(GitReadError::PathRejected),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(_) => Err(GitReadError::PathRejected),
    }
}

fn parse_single_push_target(output: &str) -> Result<&str, GitReadError> {
    let record = output
        .strip_suffix("\r\n")
        .or_else(|| output.strip_suffix('\n'))
        .unwrap_or(output);
    if record.is_empty() || record.chars().any(char::is_control) {
        return Err(remote_target_rejected());
    }
    Ok(record)
}

fn validate_push_target(project_root: &Path, target: &str) -> Result<(), GitReadError> {
    const REMOTE_TARGET_MAX_LEN: usize = 2_048;
    if target.is_empty()
        || target.len() > REMOTE_TARGET_MAX_LEN
        || target.starts_with('-')
        || target.chars().any(char::is_control)
    {
        return Err(remote_target_rejected());
    }

    if target.starts_with("https://") {
        return if safe_https_target(target) {
            Ok(())
        } else {
            Err(remote_target_rejected())
        };
    }

    // All other URI/helper/scp forms are denied. A local filesystem remote is
    // accepted only as an absolute, resolved, in-project bare repository. The
    // absolute-path requirement keeps scp-like `host:path` syntax out of this
    // branch on every platform.
    if target.contains("://") || target.contains("::") {
        return Err(remote_target_rejected());
    }
    let raw = PathBuf::from(target);
    if !raw.is_absolute() {
        return Err(remote_target_rejected());
    }
    let candidate = raw;
    let remote_root =
        open_contained_entry(project_root, &candidate, true)?.ok_or_else(remote_target_rejected)?;
    // A non-bare repository keeps receive hooks/config below `.git`; treating
    // its worktree root as a bare gitdir would scan the wrong authority. Reject
    // every `.git` marker (directory, gitfile, symlink, or reparse point) rather
    // than risk launching an uninspected receive hook.
    match fs::symlink_metadata(remote_root.join(".git")) {
        Ok(_) => return Err(remote_target_rejected()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(_) => return Err(remote_target_rejected()),
    }
    let remote_dirs = ContainedGitDirs {
        gitdir: remote_root.clone(),
        common_dir: remote_root.clone(),
    };
    deny_filesystem_metadata(project_root, &remote_root, &remote_dirs)?;
    let config = read_metadata_under_base(project_root, &remote_root, "config")?
        .ok_or_else(remote_target_rejected)?;
    if !config_declares_bare_repository(&config)? {
        return Err(remote_target_rejected());
    }
    Ok(())
}

fn config_declares_bare_repository(text: &str) -> Result<bool, GitReadError> {
    let mut section = String::new();
    let mut declared_bare = false;
    for raw in text.lines() {
        let line = raw.trim();
        if line.is_empty() || line.starts_with('#') || line.starts_with(';') {
            continue;
        }
        if line.starts_with('[') {
            if !line.ends_with(']') || line.len() < 2 {
                return Err(GitReadError::MalformedConfiguration);
            }
            section = line[1..line.len() - 1].trim().to_ascii_lowercase();
            continue;
        }
        let (key, value) = parse_config_assignment_line(line)?;
        let is_core_bare =
            (section == "core" && key == "bare") || (section.is_empty() && key == "core.bare");
        if !is_core_bare {
            continue;
        }
        if !matches!(
            value.to_ascii_lowercase().as_str(),
            "true" | "yes" | "on" | "1"
        ) {
            return Ok(false);
        }
        declared_bare = true;
    }
    Ok(declared_bare)
}

fn safe_https_target(target: &str) -> bool {
    let Some(rest) = target.strip_prefix("https://") else {
        return false;
    };
    let authority_end = rest.find(['/', '?', '#']).unwrap_or(rest.len());
    let authority = &rest[..authority_end];
    !authority.is_empty()
        && !authority.contains('@')
        && !authority.starts_with('-')
        && !authority.ends_with('.')
        && authority.chars().all(|character| {
            character.is_ascii_alphanumeric() || matches!(character, '.' | '-' | ':' | '[' | ']')
        })
        && !rest.contains('\\')
        && !rest
            .chars()
            .any(|character| character.is_control() || character.is_whitespace())
}

fn remote_target_rejected() -> GitReadError {
    GitReadError::InputRejected {
        kind: GitInputKind::RemoteTarget,
    }
}

fn clean_git_env(git_executable: &Path) -> Result<BTreeMap<OsString, OsString>, GitReadError> {
    let mut env = BTreeMap::new();
    // Minimal Windows / POSIX base — never inherit parent secrets.
    for key in ["SYSTEMROOT", "WINDIR", "COMSPEC", "PATHEXT", "TEMP", "TMP"] {
        if let Ok(v) = std::env::var(key) {
            env.insert(OsString::from(key), OsString::from(v));
        }
    }
    // Isolated PATH: only the directory containing trusted git + System32.
    let mut path_dirs = Vec::new();
    if let Some(parent) = git_executable.parent() {
        path_dirs.push(parent.to_path_buf());
        // Git for Windows often needs mingw/bin and usr/bin beside cmd/.
        if let Some(grand) = parent.parent() {
            for rel in ["mingw64/bin", "mingw32/bin", "usr/bin", "bin"] {
                let extra = grand.join(rel);
                if extra.is_dir() {
                    path_dirs.push(extra);
                }
            }
        }
    }
    if let Ok(system_root) = std::env::var("SYSTEMROOT") {
        let system32 = PathBuf::from(&system_root).join("System32");
        if system32.is_dir() {
            path_dirs.push(system32);
        }
    }
    let path_value =
        std::env::join_paths(&path_dirs).map_err(|_| GitReadError::ExecutableUnavailable)?;
    env.insert(OsString::from("PATH"), path_value);

    // Prompt / pager / system config suppression. Do not use GIT_CONFIG_KEY_*
    // env vars: process-executor rejects names containing "KEY".
    env.insert(OsString::from("GIT_TERMINAL_PROMPT"), OsString::from("0"));
    env.insert(OsString::from("GIT_PAGER"), OsString::from("cat"));
    env.insert(OsString::from("PAGER"), OsString::from("cat"));
    env.insert(OsString::from("GIT_CONFIG_NOSYSTEM"), OsString::from("1"));
    env.insert(OsString::from("GIT_NO_LAZY_FETCH"), OsString::from("1"));
    env.insert(OsString::from("LC_ALL"), OsString::from("C"));
    env.insert(OsString::from("LANG"), OsString::from("C"));

    // Intentionally omit HOME / USERPROFILE / APPDATA so global config and
    // user credential stores are not consulted.
    Ok(env)
}

/// Hooks + raw config + gitattributes without launching Git.
///
/// Must run before any `git` process that would load repository config so
/// include directives and linked metadata are rejected without following them.
fn deny_filesystem_metadata(
    project_root: &Path,
    workdir: &Path,
    dirs: &ContainedGitDirs,
) -> Result<(), GitReadError> {
    deny_active_hooks(project_root, dirs)?;
    if let Some(kind) = scan_git_config_files(project_root, dirs)? {
        return Err(GitReadError::UnsafeConfiguration { kind });
    }
    if let Some(kind) = scan_gitattributes(project_root, workdir, dirs)? {
        return Err(GitReadError::UnsafeConfiguration { kind });
    }
    Ok(())
}

/// Resolve gitdir + common-dir and prove both remain under the project root.
fn resolve_contained_git_dirs(
    project_root: &Path,
    workdir: &Path,
) -> Result<ContainedGitDirs, GitReadError> {
    let git_marker = workdir.join(".git");
    let meta = fs::symlink_metadata(&git_marker).map_err(|_| GitReadError::NotARepository)?;
    // Never follow a reparse-point `.git` marker.
    if meta_is_symlink_or_reparse(&meta) {
        return Err(GitReadError::PathRejected);
    }

    let (gitdir, common_dir) = if meta.file_type().is_file() {
        let gitdir = parse_gitfile(project_root, &git_marker)?;
        let common = common_dir_from_gitdir(project_root, &gitdir)?;
        (gitdir, common)
    } else if meta.is_dir() {
        let gitdir = open_contained_entry(project_root, &git_marker, true)?
            .ok_or(GitReadError::NotARepository)?;
        (gitdir.clone(), gitdir)
    } else {
        return Err(GitReadError::NotARepository);
    };

    ensure_resolved_under_project(project_root, &gitdir)
        .map_err(|_| GitReadError::LinkedWorktreeOutsideProject)?;
    ensure_resolved_under_project(project_root, &common_dir)
        .map_err(|_| GitReadError::LinkedWorktreeOutsideProject)?;

    Ok(ContainedGitDirs { gitdir, common_dir })
}

fn common_dir_from_gitdir(project_root: &Path, gitdir: &Path) -> Result<PathBuf, GitReadError> {
    // Explicit `commondir` file — never follow a linked/reparse commondir.
    if let Some(text) = read_metadata_under_base(project_root, gitdir, "commondir")? {
        let raw = text.trim();
        if raw.is_empty() {
            return Err(GitReadError::NotARepository);
        }
        let portable = raw.replace('\\', "/");
        if Path::new(raw).is_absolute() {
            // Absolute common dir: open as directory, no-follow, contained.
            match open_contained_entry(project_root, Path::new(raw), true) {
                Ok(Some(p)) => return Ok(p),
                Ok(None) => return Err(GitReadError::NotARepository),
                Err(GitReadError::RepoOutsideProject) => {
                    return Err(GitReadError::LinkedWorktreeOutsideProject);
                }
                Err(e) => return Err(e),
            }
        }
        // Relative to gitdir; component-wise no-follow including `..`.
        match resolve_under_base_no_follow(project_root, gitdir, &portable, true) {
            Ok(Some(p)) => return Ok(p),
            Ok(None) => return Err(GitReadError::NotARepository),
            Err(GitReadError::RepoOutsideProject) => {
                return Err(GitReadError::LinkedWorktreeOutsideProject);
            }
            Err(e) => return Err(e),
        }
    }
    // Conventional linked worktree layout: …/.git/worktrees/<name>
    if let Some(parent) = gitdir.parent()
        && parent.file_name().and_then(|s| s.to_str()) == Some("worktrees")
        && let Some(common) = parent.parent()
    {
        match open_contained_entry(project_root, common, true) {
            Ok(Some(p)) => return Ok(p),
            Ok(None) => return Err(GitReadError::NotARepository),
            Err(GitReadError::RepoOutsideProject) => {
                return Err(GitReadError::LinkedWorktreeOutsideProject);
            }
            Err(e) => return Err(e),
        }
    }
    Ok(gitdir.to_path_buf())
}

fn deny_active_hooks(project_root: &Path, dirs: &ContainedGitDirs) -> Result<(), GitReadError> {
    // Hooks directory: no-follow on the directory and every ancestor component.
    let Some(hooks_dir) =
        resolve_under_base_no_follow(project_root, &dirs.common_dir, "hooks", true)?
    else {
        return Ok(());
    };
    let entries = fs::read_dir(&hooks_dir).map_err(|_| GitReadError::PathRejected)?;
    for entry in entries.flatten() {
        let name = entry.file_name();
        let name = name.to_string_lossy();
        if name.ends_with(".sample") || name.starts_with('.') {
            continue;
        }
        let path = entry.path();
        // No-follow on each hook entry — reparse/symlink counts as active hook.
        let meta = match fs::symlink_metadata(&path) {
            Ok(m) => m,
            Err(_) => continue,
        };
        if meta_is_symlink_or_reparse(&meta) {
            return Err(GitReadError::UnsafeConfiguration {
                kind: UnsafeConfigKind::Hook,
            });
        }
        if meta.is_file() {
            // Prove the resolved file remains under the project.
            let _ = open_contained_entry(project_root, &path, false)?;
            return Err(GitReadError::UnsafeConfiguration {
                kind: UnsafeConfigKind::Hook,
            });
        }
    }
    Ok(())
}

fn scan_git_config_files(
    project_root: &Path,
    dirs: &ContainedGitDirs,
) -> Result<Option<UnsafeConfigKind>, GitReadError> {
    // Relative names under common_dir / gitdir — no-follow each component.
    let relatives = [
        (&dirs.common_dir, "config"),
        (&dirs.gitdir, "config"),
        (&dirs.gitdir, "config.worktree"),
        (&dirs.common_dir, "config.worktree"),
    ];
    for (base, rel) in relatives {
        let Some(text) = read_metadata_under_base(project_root, base, rel)? else {
            continue;
        };
        if let Some(kind) = classify_config_ini(&text)? {
            return Ok(Some(kind));
        }
    }
    Ok(None)
}

/// Parse Git's real `git config --local --list --null` wire form: `key\\nvalue\\0`.
///
/// Returns `Ok(Some(kind))` when an unsafe key is present, `Ok(None)` when the
/// list is well-formed and safe, and a typed error on malformed records.
///
/// An alternate `key=value\\0` form is **not** accepted: Git on this platform
/// emits `key\\nvalue\\0`, and silently tolerating another shape would hide
/// probe failures.
fn parse_config_null_list(null_separated: &str) -> Result<Option<UnsafeConfigKind>, GitReadError> {
    // Empty output is valid (no local keys beyond defaults that git still lists
    // as records — but an entirely empty buffer is fine).
    if null_separated.is_empty() {
        return Ok(None);
    }

    // Split on NUL; a trailing empty segment after the final NUL is ignored.
    let mut records = null_separated.split('\0').peekable();
    while let Some(entry) = records.next() {
        // Trailing empty after final NUL.
        if entry.is_empty() {
            if records.peek().is_none() {
                break;
            }
            // Empty record in the middle is malformed.
            return Err(GitReadError::MalformedConfiguration);
        }
        // Exact wire form: key\nvalue (value may be empty).
        let (key, value) = match entry.split_once('\n') {
            Some((k, v)) => (k, v),
            None => return Err(GitReadError::MalformedConfiguration),
        };
        // Key must be non-empty and must not contain additional control
        // separators that would indicate a truncated/corrupt capture.
        if key.is_empty() || key.contains('\n') {
            return Err(GitReadError::MalformedConfiguration);
        }
        // Value must not embed NUL (split already removed them) or look like a
        // glued next-key from a missing terminator: additional bare newlines
        // beyond the first key/value split are part of the value in Git, so
        // allow them. Reject `=`-only legacy misparse attempts by requiring the
        // newline form exclusively (already enforced).
        if let Some(kind) = classify_config_key_value(key, value) {
            return Ok(Some(kind));
        }
    }
    Ok(None)
}

/// Classify raw local config INI text.
///
/// Accepts both `key = value` and valid whitespace-separated `key value`
/// (no `=`) forms. Malformed or ambiguous lines fail closed.
fn classify_config_ini(text: &str) -> Result<Option<UnsafeConfigKind>, GitReadError> {
    let mut section = String::new();
    for raw in text.lines() {
        let line = raw.trim();
        if line.is_empty() || line.starts_with('#') || line.starts_with(';') {
            continue;
        }
        if line.starts_with('[') {
            if !line.ends_with(']') || line.len() < 2 {
                return Err(GitReadError::MalformedConfiguration);
            }
            section = line[1..line.len() - 1].trim().to_ascii_lowercase();
            // Normalize: section "diff \"evil\"" / includeIf "…" -> head token.
            if let Some(head) = section.split_whitespace().next() {
                section = head.trim_matches('"').to_string();
            }
            if section.is_empty() {
                return Err(GitReadError::MalformedConfiguration);
            }
            continue;
        }
        let (key, value) = parse_config_assignment_line(line)?;
        let full = if section.is_empty() {
            key
        } else {
            format!("{section}.{key}")
        };
        if let Some(kind) = classify_config_key_value(&full, &value) {
            return Ok(Some(kind));
        }
    }
    Ok(None)
}

/// Parse one non-section config assignment line.
///
/// Supports `key = value`, `key=value`, and whitespace-separated `key value`
/// without `=`. Bare `key` is treated as boolean true. Empty key or otherwise
/// ambiguous shape fails closed.
fn parse_config_assignment_line(line: &str) -> Result<(String, String), GitReadError> {
    if let Some((k, v)) = line.split_once('=') {
        let key = k.trim();
        if key.is_empty() || key.chars().any(|c| c.is_whitespace()) {
            return Err(GitReadError::MalformedConfiguration);
        }
        if !config_key_token_ok(key) {
            return Err(GitReadError::MalformedConfiguration);
        }
        let value = v.trim().trim_matches('"').to_string();
        return Ok((key.to_ascii_lowercase(), value));
    }

    // Whitespace-separated form (no `=`): first token is key, remainder value.
    let mut parts = line.split_whitespace();
    let key = parts.next().ok_or(GitReadError::MalformedConfiguration)?;
    if !config_key_token_ok(key) {
        return Err(GitReadError::MalformedConfiguration);
    }
    let rest: Vec<&str> = parts.collect();
    let value = if rest.is_empty() {
        // Bare key ⇒ boolean true (Git semantics).
        "true".to_string()
    } else {
        let joined = rest.join(" ");
        joined.trim_matches('"').to_string()
    };
    Ok((key.to_ascii_lowercase(), value))
}

fn config_key_token_ok(key: &str) -> bool {
    !key.is_empty()
        && key
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_' || c == '.')
}

fn classify_config_key_value(key: &str, value: &str) -> Option<UnsafeConfigKind> {
    let key = key.trim().to_ascii_lowercase();
    let value = value.trim();
    // Empty assignments disable the feature.
    if value.is_empty() {
        return None;
    }
    if key == "core.hookspath" {
        return Some(UnsafeConfigKind::Hook);
    }
    if key == "core.fsmonitor" {
        // This key is boolean-or-command: only boolean false disables it.
        return (!value.eq_ignore_ascii_case("false")).then_some(UnsafeConfigKind::Fsmonitor);
    }
    if key == "core.sshcommand" {
        return Some(UnsafeConfigKind::SshCommand);
    }
    if key == "diff.external" || key == "interactive.difffilter" {
        return Some(UnsafeConfigKind::ExtDiff);
    }
    if key == "gpg.program"
        || key == "gpg.ssh.program"
        || key == "core.askpass"
        || key == "core.gitproxy"
        || key == "core.alternaterefscommand"
        || key == "gc.recentobjectshook"
        || key == "core.attributesfile"
        || key == "core.excludesfile"
        || key == "core.worktree"
        || key == "credential.helper"
        || (key.starts_with("credential.") && key.ends_with(".helper"))
    {
        return Some(UnsafeConfigKind::ExecutableConfig);
    }
    // include.path / includeIf.<cond>.path pull external configuration.
    if key == "include.path" || (key.starts_with("includeif.") && key.ends_with(".path")) {
        return Some(UnsafeConfigKind::Include);
    }

    // filter.<name>.clean|smudge|process
    if key.starts_with("filter.")
        && (key.ends_with(".clean") || key.ends_with(".smudge") || key.ends_with(".process"))
    {
        return Some(UnsafeConfigKind::Filter);
    }
    // diff.<driver>.textconv / command
    if key.starts_with("diff.") && key.ends_with(".textconv") {
        return Some(UnsafeConfigKind::Textconv);
    }
    if key.starts_with("diff.") && key.ends_with(".command") {
        return Some(UnsafeConfigKind::ExtDiff);
    }
    // merge.<driver>.driver can execute programs during merges; deny for read.
    if key.starts_with("merge.") && key.ends_with(".driver") {
        return Some(UnsafeConfigKind::ExecutableConfig);
    }
    // A named remote is still unsafe if config can select a program/helper.
    if key.starts_with("remote.")
        && (key.ends_with(".receivepack") || key.ends_with(".uploadpack") || key.ends_with(".vcs"))
    {
        return Some(UnsafeConfigKind::RemoteCommand);
    }
    if key.starts_with("remote.") && key.ends_with(".mirror") {
        return (!value.eq_ignore_ascii_case("false")).then_some(UnsafeConfigKind::RemoteCommand);
    }
    if key == "protocol.allow" || (key.starts_with("protocol.") && key.ends_with(".allow")) {
        // Repository config must not reopen a transport disabled by the fixed
        // command-line protocol policy.
        return Some(UnsafeConfigKind::RemoteCommand);
    }
    if key == "push.pushoption" {
        return Some(UnsafeConfigKind::RemoteCommand);
    }
    None
}

fn scan_gitattributes(
    project_root: &Path,
    workdir: &Path,
    dirs: &ContainedGitDirs,
) -> Result<Option<UnsafeConfigKind>, GitReadError> {
    // Worktree attributes + validated gitdir/common-dir info/attributes.
    // Never assume workdir/.git is a directory (linked worktrees use a file).
    // Every path is opened with symlink_metadata + no reparse ancestors.
    let sources: [(&Path, &str); 3] = [
        (workdir, ".gitattributes"),
        (&dirs.gitdir, "info/attributes"),
        (&dirs.common_dir, "info/attributes"),
    ];
    for (base, rel) in sources {
        let Some(text) = read_metadata_under_base(project_root, base, rel)? else {
            continue;
        };
        if let Some(kind) = classify_gitattributes_text(&text) {
            return Ok(Some(kind));
        }
    }
    Ok(None)
}

fn classify_gitattributes_text(text: &str) -> Option<UnsafeConfigKind> {
    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        for token in line.split_whitespace().skip(1) {
            let token = token.trim_start_matches('-').trim_start_matches('!');
            if let Some(name) = token.strip_prefix("filter=")
                && !name.is_empty()
            {
                // Even lfs is an external filter — deny fail-closed.
                return Some(UnsafeConfigKind::Filter);
            }
            if let Some(name) = token.strip_prefix("diff=") {
                // Built-in diff drivers are short identifiers; any custom
                // driver may bind textconv. Deny all non-empty to be safe
                // except common built-ins we treat as pure.
                if !name.is_empty()
                    && !matches!(
                        name,
                        "astextplain"
                            | "ada"
                            | "cpp"
                            | "csharp"
                            | "css"
                            | "dts"
                            | "elixir"
                            | "fortran"
                            | "html"
                            | "java"
                            | "kotlin"
                            | "markdown"
                            | "matlab"
                            | "objc"
                            | "pascal"
                            | "perl"
                            | "php"
                            | "python"
                            | "ruby"
                            | "rust"
                            | "scheme"
                            | "tex"
                            | "word"
                    )
                {
                    return Some(UnsafeConfigKind::ExtDiff);
                }
            }
        }
    }
    None
}

/// Probe a repo-relative path under workdir without following links.
///
/// - `Ok(Some(abs))` — regular file fully contained under project + workdir
/// - `Ok(None)` — path does not exist (safe empty diff)
/// - `Err(PathRejected)` — symlink, reparse/junction, or authority escape
fn probe_contained_file(
    project_root: &Path,
    workdir: &Path,
    repo_rel: &str,
) -> Result<Option<PathBuf>, GitReadError> {
    validate_portable_path(repo_rel).map_err(|_| GitReadError::PathRejected)?;
    if repo_rel == "." || repo_rel.is_empty() {
        return Err(GitReadError::PathRejected);
    }

    let workdir = ensure_under_project(project_root, workdir)?;
    let mut current = workdir.clone();
    let components: Vec<&str> = repo_rel.split('/').collect();
    for (idx, component) in components.iter().enumerate() {
        current.push(component);
        let meta = match fs::symlink_metadata(&current) {
            Ok(m) => m,
            Err(_) => {
                // Missing: only legal when this is the final component
                // (parent chain already proven real). Mid-path missing ⇒ reject.
                if idx + 1 == components.len() {
                    return Ok(None);
                }
                return Err(GitReadError::PathRejected);
            }
        };
        if meta_is_symlink_or_reparse(&meta) {
            return Err(GitReadError::PathRejected);
        }
        let is_last = idx + 1 == components.len();
        if is_last {
            if !meta.is_file() {
                return Err(GitReadError::PathRejected);
            }
        } else if !meta.is_dir() {
            return Err(GitReadError::PathRejected);
        }
        let canon = fs::canonicalize(&current).map_err(|_| GitReadError::PathRejected)?;
        let canon = strip_verbatim_prefix(canon);
        if canon != workdir && !canon.starts_with(&workdir) {
            return Err(GitReadError::PathRejected);
        }
        ensure_resolved_under_project(project_root, &canon)?;
        current = canon;
    }
    Ok(Some(current))
}

fn strip_repo_prefix(
    repo_project_relative: &str,
    project_path: &str,
) -> Result<String, GitReadError> {
    if repo_project_relative == "." {
        return Ok(project_path.to_string());
    }
    let prefix = format!("{repo_project_relative}/");
    if let Some(rest) = project_path.strip_prefix(&prefix) {
        validate_portable_path(rest).map_err(|_| GitReadError::PathRejected)?;
        return Ok(rest.to_string());
    }
    if project_path == repo_project_relative {
        return Err(GitReadError::PathRejected);
    }
    // Allow callers to pass already-repo-relative paths.
    validate_portable_path(project_path).map_err(|_| GitReadError::PathRejected)?;
    Ok(project_path.to_string())
}

fn stdout_bytes(receipt: &keeper_process_executor::ProcessReceipt) -> Vec<u8> {
    // Prefer retained captured bytes; BoundedOutput stores lossy text, so use
    // the UTF-8 view which is sufficient for porcelain and unified diffs when
    // lossy_utf8 is false (enforced by callers).
    receipt.stdout.text.as_bytes().to_vec()
}

/// Status / config / small probes: any truncation or lossy UTF-8 is denial.
fn require_reliable_publishable(out: &BoundedOutput) -> Result<(), GitReadError> {
    if out.truncated {
        return Err(GitReadError::OutputUnreliable);
    }
    if out.lossy_utf8 {
        return Err(GitReadError::Parse(ParseError::Utf8));
    }
    Ok(())
}

/// Diff probes may truncate when over the product text budget; that maps to
/// Oversized rather than partial text. Lossy UTF-8 is still denial.
fn require_reliable_or_oversized(out: &BoundedOutput) -> Result<(), GitReadError> {
    if out.lossy_utf8 {
        return Err(GitReadError::Parse(ParseError::Utf8));
    }
    // Truncation is only acceptable when the observed size exceeds the text
    // budget (typed Oversized). Truncation under the budget is uncertain.
    if out.truncated && out.bytes_seen <= DIFF_TEXT_BYTE_CAP {
        return Err(GitReadError::OutputUnreliable);
    }
    Ok(())
}

fn classify_diff(
    path: &str,
    raw: &[u8],
    bytes_seen: usize,
    truncated: bool,
) -> Result<GitDiffView, GitReadError> {
    // Git's binary notice has no NUL; detect explicitly.
    let text_probe = std::str::from_utf8(raw).unwrap_or("");
    if text_probe.contains("Binary files ") || text_probe.contains("differ\nBinary") {
        return Ok(GitDiffView::Binary {
            path: path.to_string(),
            size_bytes: bytes_seen.max(raw.len()),
        });
    }
    if truncated || bytes_seen > DIFF_TEXT_BYTE_CAP || raw.len() > DIFF_TEXT_BYTE_CAP {
        return Ok(GitDiffView::Oversized {
            path: path.to_string(),
            size_bytes: bytes_seen.max(raw.len()),
            limit_bytes: DIFF_TEXT_BYTE_CAP,
        });
    }
    bound_git_diff(path, raw).map_err(GitReadError::from)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn config_null_wire_detects_unsafe_key_newline_value() {
        // Exact Git wire form: key\nvalue\0
        let wire = "user.name\ntest\0filter.evil.clean\nevil-tool\0core.filemode\nfalse\0";
        let kind = parse_config_null_list(wire)
            .expect("parse")
            .expect("unsafe");
        assert_eq!(kind, UnsafeConfigKind::Filter);
    }

    #[test]
    fn config_null_wire_detects_include_textconv_fsmonitor() {
        let wire = concat!(
            "include.path\n../outside.inc\0",
            "diff.evil.textconv\nevil-tc\0",
            "core.fsmonitor\nevil-fsm\0",
        );
        let kind = parse_config_null_list(wire)
            .expect("parse")
            .expect("unsafe");
        assert_eq!(kind, UnsafeConfigKind::Include);

        let wire2 = "diff.evil.textconv\nevil-tc\0";
        assert_eq!(
            parse_config_null_list(wire2).unwrap(),
            Some(UnsafeConfigKind::Textconv)
        );

        let wire3 = "core.fsmonitor\nevil-fsm\0";
        assert_eq!(
            parse_config_null_list(wire3).unwrap(),
            Some(UnsafeConfigKind::Fsmonitor)
        );
    }

    #[test]
    fn config_null_wire_denies_remote_program_and_push_expansion_settings() {
        for wire in [
            "remote.origin.receivepack\nevil\0",
            "remote.origin.vcs\nevil\0",
            "remote.origin.mirror\ntrue\0",
            "push.pushoption\nserver-side-option\0",
            "protocol.ext.allow\nalways\0",
        ] {
            assert_eq!(
                parse_config_null_list(wire),
                Ok(Some(UnsafeConfigKind::RemoteCommand)),
                "wire={wire:?}"
            );
        }
    }

    #[test]
    fn push_target_probe_requires_exactly_one_control_free_record() {
        assert_eq!(
            parse_single_push_target("https://example.test/repo.git\n"),
            Ok("https://example.test/repo.git")
        );
        assert_eq!(
            parse_single_push_target("https://example.test/repo.git\r\n"),
            Ok("https://example.test/repo.git")
        );
        for output in [
            "",
            "https://one.test/repo\nhttps://two.test/repo\n",
            "https://example.test/repo\n\n",
            "https://example.test/repo\u{0007}\n",
        ] {
            assert!(
                parse_single_push_target(output).is_err(),
                "output={output:?}"
            );
        }
    }

    #[test]
    fn config_null_wire_denies_indirect_programs_and_external_metadata() {
        for wire in [
            "credential.https://example.test.helper\n!evil\0",
            "core.alternaterefscommand\nevil\0",
            "gc.recentobjectshook\nevil\0",
            "core.attributesfile\n../outside-attributes\0",
            "core.excludesfile\n../outside-ignore\0",
            "core.worktree\n../outside-worktree\0",
        ] {
            assert_eq!(
                parse_config_null_list(wire),
                Ok(Some(UnsafeConfigKind::ExecutableConfig)),
                "wire={wire:?}"
            );
        }
    }

    #[test]
    fn command_valued_false_is_not_treated_as_boolean_disable() {
        assert_eq!(
            parse_config_null_list("core.hookspath\nfalse\0"),
            Ok(Some(UnsafeConfigKind::Hook))
        );
        assert_eq!(
            parse_config_null_list("filter.evil.clean\nfalse\0"),
            Ok(Some(UnsafeConfigKind::Filter))
        );
        assert_eq!(parse_config_null_list("core.fsmonitor\nfalse\0"), Ok(None));
        assert_eq!(
            parse_config_null_list("remote.origin.mirror\nfalse\0"),
            Ok(None)
        );
    }

    #[test]
    fn config_null_wire_safe_list_is_ok_none() {
        let wire = "core.filemode\nfalse\0user.name\nPillow\0user.email\nf@x\0";
        assert_eq!(parse_config_null_list(wire).unwrap(), None);
    }

    #[test]
    fn config_null_wire_rejects_equals_form_as_malformed() {
        // Legacy incorrect assumption key=value\0 must not be silently accepted.
        let wire = "filter.evil.clean=evil-tool\0";
        let err = parse_config_null_list(wire).expect_err("equals form");
        assert_eq!(err, GitReadError::MalformedConfiguration);
    }

    #[test]
    fn config_null_wire_rejects_missing_newline_malformed() {
        let wire = "filter.evil.clean\0";
        let err = parse_config_null_list(wire).expect_err("no value sep");
        assert_eq!(err, GitReadError::MalformedConfiguration);
    }

    #[test]
    fn config_null_wire_rejects_empty_middle_record() {
        let wire = "user.name\ntest\0\0core.bare\nfalse\0";
        let err = parse_config_null_list(wire).expect_err("empty middle");
        assert_eq!(err, GitReadError::MalformedConfiguration);
    }

    #[test]
    fn config_null_wire_empty_value_is_safe() {
        // Empty value disables; still well-formed.
        let wire = "core.fsmonitor\n\0core.hookspath\n\0";
        assert_eq!(parse_config_null_list(wire).unwrap(), None);
    }

    #[test]
    fn reliable_output_helpers_fail_closed() {
        let truncated = BoundedOutput {
            text: "partial".into(),
            bytes_seen: 10,
            bytes_retained: 7,
            truncated: true,
            lossy_utf8: false,
        };
        assert_eq!(
            require_reliable_publishable(&truncated),
            Err(GitReadError::OutputUnreliable)
        );
        // Diff: truncation under text budget is unreliable.
        assert_eq!(
            require_reliable_or_oversized(&truncated),
            Err(GitReadError::OutputUnreliable)
        );

        let lossy = BoundedOutput {
            text: "x".into(),
            bytes_seen: 1,
            bytes_retained: 1,
            truncated: false,
            lossy_utf8: true,
        };
        assert!(matches!(
            require_reliable_publishable(&lossy),
            Err(GitReadError::Parse(ParseError::Utf8))
        ));

        let over = BoundedOutput {
            text: "x".repeat(100),
            bytes_seen: DIFF_TEXT_BYTE_CAP + 10,
            bytes_retained: 100,
            truncated: true,
            lossy_utf8: false,
        };
        // Oversized truncation is allowed through; classify_diff maps it.
        assert!(require_reliable_or_oversized(&over).is_ok());
    }

    #[test]
    fn classify_diff_maps_truncation_to_oversized() {
        let view =
            classify_diff("big.txt", b"partial", DIFF_TEXT_BYTE_CAP + 1, true).expect("classify");
        match view {
            GitDiffView::Oversized {
                path,
                size_bytes,
                limit_bytes,
            } => {
                assert_eq!(path, "big.txt");
                assert!(size_bytes > DIFF_TEXT_BYTE_CAP);
                assert_eq!(limit_bytes, DIFF_TEXT_BYTE_CAP);
            }
            other => panic!("expected Oversized, got {other:?}"),
        }
    }

    #[test]
    fn config_ini_whitespace_include_without_equals_is_denied() {
        // Valid whitespace-separated include form (no `=`) must classify as
        // Include before any outside file would be read by Git.
        let text = "[include]\n\tpath ../outside.inc\n";
        let kind = classify_config_ini(text).expect("parse").expect("unsafe");
        assert_eq!(kind, UnsafeConfigKind::Include);

        let text2 = "[includeIf \"gitdir:**\"]\npath /tmp/evil.inc\n";
        let kind2 = classify_config_ini(text2).expect("parse").expect("unsafe");
        assert_eq!(kind2, UnsafeConfigKind::Include);
    }

    #[test]
    fn config_ini_equals_include_still_denied() {
        let text = "[include]\n\tpath = ../outside.inc\n";
        assert_eq!(
            classify_config_ini(text).unwrap(),
            Some(UnsafeConfigKind::Include)
        );
    }

    #[test]
    fn config_ini_malformed_section_fails_closed() {
        let text = "[include\npath = x\n";
        let err = classify_config_ini(text).expect_err("malformed section");
        assert_eq!(err, GitReadError::MalformedConfiguration);
    }

    #[test]
    fn config_ini_empty_key_equals_fails_closed() {
        let text = "[core]\n= value\n";
        let err = classify_config_ini(text).expect_err("empty key");
        assert_eq!(err, GitReadError::MalformedConfiguration);
    }
}
