//! C4 structured Git Tauri commands.
//!
//! Composes the closed `keeper_git_executor::GitReadExecutor` surface. The renderer never
//! supplies a native root, Git executable, argv, cwd, or environment. Project
//! roots resolve exclusively through `AppState::get_registered_project`.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use keeper_git_executor::{
    GIT_READ_TIMEOUT, GitDiffSnapshot, GitDiffView as ExecutorDiffView, GitHead, GitInputKind,
    GitMutationKind, GitMutationReceipt, GitReadError, GitReadExecutor, GitRepositoryId,
    GitStatusEntry, GitStatusKind, GitStatusSnapshot, is_portable_entry_path, is_portable_path,
};
use keeper_tool_executor::ProjectCapability;
use serde::{Deserialize, Serialize};
use tauri::State;

use crate::app_state::{AppState, CommandError};

/// Largest integer still exact in IEEE-754 / JS Number.
const MAX_JS_SAFE_INTEGER: u64 = 9_007_199_254_740_991;

// ---------------------------------------------------------------------------
// Public DTOs (exact wire fields accepted by gitGateway.ts — no extras)
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct GitRepositoryDto {
    pub repository_id: String,
    pub available: bool,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct GitDiscoveryDto {
    pub generation: u64,
    pub repositories: Vec<GitRepositoryDto>,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct GitStatusEntryDto {
    pub path: String,
    pub kind: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub original_path: Option<String>,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct GitStatusDto {
    pub repository_id: String,
    pub generation: u64,
    pub branch: Option<String>,
    pub detached: bool,
    pub head_short: String,
    pub dirty_count: u32,
    pub files: Vec<GitStatusEntryDto>,
    pub truncated: bool,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct GitDiffDto {
    pub repository_id: String,
    pub generation: u64,
    pub path: String,
    pub kind: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub unified: Option<String>,
    pub truncated: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub line_count: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub byte_count: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub original_path: Option<String>,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct GitMutationDto {
    pub repository_id: String,
    pub generation: u64,
    pub operation: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub remote_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub branch_name: Option<String>,
}

/// Push target and explicit confirmation travel in one closed input object, so
/// the backend cannot confirm one target and execute another.
#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct GitPushConfirmationInput {
    pub confirmed: bool,
    pub remote_name: String,
    pub branch_name: String,
}

// ---------------------------------------------------------------------------
// Per-project discovery authority
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
struct ProjectGitAuthority {
    generation: u64,
    /// Portable repository id → availability from the latest discovery.
    repositories: HashMap<String, bool>,
}

/// Managed host Git authority: one resolved executor + per-project snapshots.
pub struct GitCommandState {
    /// Host-resolved executor. `None` means Git is unavailable (fail-closed).
    executor: Option<GitReadExecutor>,
    authority: Mutex<HashMap<String, ProjectGitAuthority>>,
    mutation_gate: Mutex<()>,
}

impl GitCommandState {
    /// Production construction: resolve and canonicalize a trusted Git binary.
    pub fn new() -> Self {
        Self {
            executor: resolve_host_git_executor(),
            authority: Mutex::new(HashMap::new()),
            mutation_gate: Mutex::new(()),
        }
    }

    /// Test seam only — never exposed as a Tauri command or renderer input.
    #[cfg(test)]
    pub(crate) fn with_git_executable(git_executable: Option<PathBuf>) -> Self {
        let executor = git_executable.and_then(|path| {
            let canonical = canonicalize_git_path(&path)?;
            GitReadExecutor::new(canonical, GIT_READ_TIMEOUT).ok()
        });
        Self {
            executor,
            authority: Mutex::new(HashMap::new()),
            mutation_gate: Mutex::new(()),
        }
    }

    /// Whether the host currently has a usable Git executor.
    #[cfg(test)]
    pub(crate) fn git_available(&self) -> bool {
        self.executor.is_some()
    }

    fn require_executor(&self) -> Result<&GitReadExecutor, CommandError> {
        self.executor
            .as_ref()
            .ok_or_else(|| git_error("git_unavailable", "Git read capability is unavailable."))
    }

    /// Discover repositories under the registered project root.
    pub fn discover(
        &self,
        app: &AppState,
        project_id: &str,
    ) -> Result<GitDiscoveryDto, CommandError> {
        let _guard = self.mutation_gate.lock().map_err(|_| mutation_failed())?;
        let capability = resolve_project_capability(app, project_id)?;
        let executor = self.require_executor()?;

        let discovery = executor
            .discover(capability.canonical_root())
            .map_err(map_read_error)?;

        let repositories: Vec<GitRepositoryDto> = discovery
            .repositories
            .into_iter()
            .map(|repo| GitRepositoryDto {
                repository_id: repo.project_relative,
                available: repo.available,
            })
            .collect();

        let mut map = self
            .authority
            .lock()
            .map_err(|_| git_error("git_unavailable", "Git read capability is unavailable."))?;

        let previous = map.get(project_id).map(|a| a.generation).unwrap_or(0);
        let generation = next_generation(previous);

        let mut index = HashMap::with_capacity(repositories.len());
        for repo in &repositories {
            index.insert(repo.repository_id.clone(), repo.available);
        }

        map.insert(
            project_id.to_owned(),
            ProjectGitAuthority {
                generation,
                repositories: index,
            },
        );

        Ok(GitDiscoveryDto {
            generation,
            repositories,
        })
    }

    /// Bounded porcelain status for one available repository at the current generation.
    pub fn status(
        &self,
        app: &AppState,
        project_id: &str,
        repository_id: &str,
        generation: u64,
    ) -> Result<GitStatusDto, CommandError> {
        let capability = resolve_project_capability(app, project_id)?;
        let executor = self.require_executor()?;
        self.authorize_repo(project_id, repository_id, generation)?;

        let repo = GitRepositoryId {
            project_relative: repository_id.to_owned(),
        };
        let snap = executor
            .status(capability.canonical_root(), &repo, generation)
            .map_err(map_read_error)?;
        Ok(map_status_dto(snap))
    }

    /// Bounded one-path diff for one available repository at the current generation.
    pub fn diff(
        &self,
        app: &AppState,
        project_id: &str,
        repository_id: &str,
        generation: u64,
        path: &str,
    ) -> Result<GitDiffDto, CommandError> {
        let capability = resolve_project_capability(app, project_id)?;
        let executor = self.require_executor()?;
        self.authorize_repo(project_id, repository_id, generation)?;

        // Reject non-portable paths at the command layer before the executor.
        if !is_portable_path(path) {
            return Err(git_error("git_path_rejected", "Git path was rejected."));
        }

        let repo = GitRepositoryId {
            project_relative: repository_id.to_owned(),
        };
        let snap = executor
            .diff(capability.canonical_root(), &repo, path, generation)
            .map_err(map_read_error)?;
        Ok(map_diff_dto(snap))
    }

    pub fn stage(
        &self,
        app: &AppState,
        project_id: &str,
        repository_id: &str,
        generation: u64,
        paths: &[String],
    ) -> Result<GitMutationDto, CommandError> {
        validate_command_paths(paths)?;
        let _guard = self.mutation_gate.lock().map_err(|_| mutation_failed())?;
        let capability = resolve_project_capability(app, project_id)?;
        let executor = self.require_executor()?;
        self.authorize_repo(project_id, repository_id, generation)?;
        let repo = GitRepositoryId {
            project_relative: repository_id.to_owned(),
        };
        let receipt = executor
            .stage(capability.canonical_root(), &repo, paths)
            .map_err(map_mutation_error)?;
        Ok(map_mutation_dto(receipt, repository_id, generation))
    }

    pub fn unstage(
        &self,
        app: &AppState,
        project_id: &str,
        repository_id: &str,
        generation: u64,
        paths: &[String],
    ) -> Result<GitMutationDto, CommandError> {
        validate_command_paths(paths)?;
        let _guard = self.mutation_gate.lock().map_err(|_| mutation_failed())?;
        let capability = resolve_project_capability(app, project_id)?;
        let executor = self.require_executor()?;
        self.authorize_repo(project_id, repository_id, generation)?;
        let repo = GitRepositoryId {
            project_relative: repository_id.to_owned(),
        };
        let receipt = executor
            .unstage(capability.canonical_root(), &repo, paths)
            .map_err(map_mutation_error)?;
        Ok(map_mutation_dto(receipt, repository_id, generation))
    }

    pub fn commit(
        &self,
        app: &AppState,
        project_id: &str,
        repository_id: &str,
        generation: u64,
        message: &str,
    ) -> Result<GitMutationDto, CommandError> {
        let _guard = self.mutation_gate.lock().map_err(|_| mutation_failed())?;
        let capability = resolve_project_capability(app, project_id)?;
        let executor = self.require_executor()?;
        self.authorize_repo(project_id, repository_id, generation)?;
        let repo = GitRepositoryId {
            project_relative: repository_id.to_owned(),
        };
        let receipt = executor
            .commit(capability.canonical_root(), &repo, message)
            .map_err(map_mutation_error)?;
        Ok(map_mutation_dto(receipt, repository_id, generation))
    }

    pub fn push(
        &self,
        app: &AppState,
        project_id: &str,
        repository_id: &str,
        generation: u64,
        confirmation: &GitPushConfirmationInput,
    ) -> Result<GitMutationDto, CommandError> {
        if !confirmation.confirmed {
            return Err(git_error(
                "git_push_confirmation_required",
                "Git push requires explicit confirmation.",
            ));
        }
        let _guard = self.mutation_gate.lock().map_err(|_| mutation_failed())?;
        let capability = resolve_project_capability(app, project_id)?;
        let executor = self.require_executor()?;
        self.authorize_repo(project_id, repository_id, generation)?;
        let repo = GitRepositoryId {
            project_relative: repository_id.to_owned(),
        };
        let receipt = executor
            .push(
                capability.canonical_root(),
                &repo,
                &confirmation.remote_name,
                &confirmation.branch_name,
            )
            .map_err(map_mutation_error)?;
        Ok(map_mutation_dto(receipt, repository_id, generation))
    }

    /// Exact project + repository + generation membership against latest discovery.
    fn authorize_repo(
        &self,
        project_id: &str,
        repository_id: &str,
        generation: u64,
    ) -> Result<(), CommandError> {
        let map = self
            .authority
            .lock()
            .map_err(|_| git_error("git_unavailable", "Git read capability is unavailable."))?;

        let authority = map.get(project_id).ok_or_else(|| {
            git_error(
                "git_generation_stale",
                "Git result is stale; refresh changes.",
            )
        })?;

        if generation != authority.generation {
            if generation > authority.generation {
                return Err(git_error(
                    "git_generation_mismatch",
                    "Git repository identity changed; refresh changes.",
                ));
            }
            return Err(git_error(
                "git_generation_stale",
                "Git result is stale; refresh changes.",
            ));
        }

        match authority.repositories.get(repository_id) {
            Some(true) => Ok(()),
            Some(false) => Err(git_error(
                "git_repository_not_found",
                "Git repository was not found.",
            )),
            None => Err(git_error(
                "git_repository_not_found",
                "Git repository was not found.",
            )),
        }
    }

    /// Latest generation for a project (test inspection only).
    #[cfg(test)]
    pub(crate) fn latest_generation(&self, project_id: &str) -> Option<u64> {
        self.authority
            .lock()
            .ok()
            .and_then(|m| m.get(project_id).map(|a| a.generation))
    }
}

impl Default for GitCommandState {
    fn default() -> Self {
        Self::new()
    }
}

// ---------------------------------------------------------------------------
// Tauri commands
// ---------------------------------------------------------------------------

#[tauri::command]
pub fn discover_git_repositories(
    app: State<'_, AppState>,
    git: State<'_, GitCommandState>,
    project_id: String,
) -> Result<GitDiscoveryDto, CommandError> {
    git.discover(&app, &project_id)
}

#[tauri::command]
pub fn get_git_status(
    app: State<'_, AppState>,
    git: State<'_, GitCommandState>,
    project_id: String,
    repository_id: String,
    generation: u64,
) -> Result<GitStatusDto, CommandError> {
    git.status(&app, &project_id, &repository_id, generation)
}

#[tauri::command]
pub fn open_git_diff(
    app: State<'_, AppState>,
    git: State<'_, GitCommandState>,
    project_id: String,
    repository_id: String,
    generation: u64,
    path: String,
) -> Result<GitDiffDto, CommandError> {
    git.diff(&app, &project_id, &repository_id, generation, &path)
}

#[tauri::command]
pub fn stage_git_paths(
    app: State<'_, AppState>,
    git: State<'_, GitCommandState>,
    project_id: String,
    repository_id: String,
    generation: u64,
    paths: Vec<String>,
) -> Result<GitMutationDto, CommandError> {
    git.stage(&app, &project_id, &repository_id, generation, &paths)
}

#[tauri::command]
pub fn unstage_git_paths(
    app: State<'_, AppState>,
    git: State<'_, GitCommandState>,
    project_id: String,
    repository_id: String,
    generation: u64,
    paths: Vec<String>,
) -> Result<GitMutationDto, CommandError> {
    git.unstage(&app, &project_id, &repository_id, generation, &paths)
}

#[tauri::command]
pub fn commit_git_changes(
    app: State<'_, AppState>,
    git: State<'_, GitCommandState>,
    project_id: String,
    repository_id: String,
    generation: u64,
    message: String,
) -> Result<GitMutationDto, CommandError> {
    git.commit(&app, &project_id, &repository_id, generation, &message)
}

#[tauri::command]
pub fn push_git_branch(
    app: State<'_, AppState>,
    git: State<'_, GitCommandState>,
    project_id: String,
    repository_id: String,
    generation: u64,
    confirmation: GitPushConfirmationInput,
) -> Result<GitMutationDto, CommandError> {
    git.push(&app, &project_id, &repository_id, generation, &confirmation)
}

// ---------------------------------------------------------------------------
// Host Git resolution (never renderer-controlled)
// ---------------------------------------------------------------------------

fn resolve_host_git_executor() -> Option<GitReadExecutor> {
    let path = locate_host_git()?;
    let canonical = canonicalize_git_path(&path)?;
    GitReadExecutor::new(canonical, GIT_READ_TIMEOUT).ok()
}

fn locate_host_git() -> Option<PathBuf> {
    // Optional host override for controlled environments only (not a product API).
    if let Ok(override_path) = std::env::var("PILLOW_HOST_GIT") {
        let p = PathBuf::from(override_path);
        if p.is_file() {
            return Some(p);
        }
    }

    // Standard install locations only. Never encode a developer-specific path;
    // non-standard installs resolve through PATH below or `PILLOW_HOST_GIT`.
    let candidates = [
        r"C:\Program Files\Git\cmd\git.exe",
        r"C:\Program Files (x86)\Git\cmd\git.exe",
    ];
    for c in candidates {
        let p = PathBuf::from(c);
        if p.is_file() {
            return Some(p);
        }
    }

    if let Ok(path) = std::env::var("PATH") {
        for dir in std::env::split_paths(&path) {
            let p = dir.join("git.exe");
            if p.is_file() {
                return Some(p);
            }
            let p2 = dir.join("git");
            if p2.is_file() {
                return Some(p2);
            }
        }
    }
    None
}

fn canonicalize_git_path(path: &Path) -> Option<PathBuf> {
    let canonical = path.canonicalize().ok()?;
    if canonical.is_file() {
        Some(canonical)
    } else {
        None
    }
}

// ---------------------------------------------------------------------------
// Project + error mapping
// ---------------------------------------------------------------------------

fn resolve_project(
    app: &AppState,
    project_id: &str,
) -> Result<keeper_project::Project, CommandError> {
    match app.get_registered_project(project_id) {
        Ok(project) => Ok(project),
        Err(err) if err.code == "project_not_found" => Err(git_error(
            "git_project_not_found",
            "Git project was not found.",
        )),
        Err(_) => Err(git_error(
            "git_unavailable",
            "Git read capability is unavailable.",
        )),
    }
}

fn resolve_project_capability(
    app: &AppState,
    project_id: &str,
) -> Result<ProjectCapability, CommandError> {
    let project = resolve_project(app, project_id)?;
    let registered_id = project.id.0.to_string();
    if registered_id != project_id {
        return Err(git_error(
            "git_project_not_found",
            "Git project was not found.",
        ));
    }
    ProjectCapability::new(registered_id, &project.root_path)
        .map_err(|_| git_error("git_project_not_found", "Git project was not found."))
}

fn git_error(code: &'static str, message: &'static str) -> CommandError {
    CommandError {
        code,
        message: message.to_owned(),
    }
}

fn map_read_error(error: GitReadError) -> CommandError {
    match error {
        GitReadError::PathRejected => git_error("git_path_rejected", "Git path was rejected."),
        GitReadError::Parse(keeper_git_executor::ParseError::PathRejected { .. }) => {
            git_error("git_path_rejected", "Git path was rejected.")
        }
        GitReadError::NotARepository
        | GitReadError::RepoNotFound
        | GitReadError::RepoOutsideProject
        | GitReadError::LinkedWorktreeOutsideProject => {
            git_error("git_repository_not_found", "Git repository was not found.")
        }
        GitReadError::ExecutableUnavailable => {
            git_error("git_unavailable", "Git read capability is unavailable.")
        }
        // Unsafe config, timeouts, process failures, malformed output: opaque unavailable.
        _ => git_error("git_unavailable", "Git read capability is unavailable."),
    }
}

fn validate_command_paths(paths: &[String]) -> Result<(), CommandError> {
    if paths.is_empty() || paths.iter().any(|path| !is_portable_entry_path(path)) {
        return Err(git_error("git_path_rejected", "Git path was rejected."));
    }
    Ok(())
}

fn map_mutation_error(error: GitReadError) -> CommandError {
    match error {
        GitReadError::PathRejected
        | GitReadError::Parse(keeper_git_executor::ParseError::PathRejected { .. }) => {
            git_error("git_path_rejected", "Git path was rejected.")
        }
        GitReadError::InputRejected {
            kind: GitInputKind::CommitMessage,
        } => git_error("git_message_rejected", "Git commit message was rejected."),
        GitReadError::InputRejected {
            kind: GitInputKind::BranchName,
        } => git_error("git_branch_rejected", "Git branch name was rejected."),
        GitReadError::InputRejected {
            kind: GitInputKind::RemoteName | GitInputKind::RemoteTarget,
        } => git_error("git_remote_rejected", "Git remote was rejected."),
        GitReadError::NotARepository
        | GitReadError::RepoNotFound
        | GitReadError::RepoOutsideProject
        | GitReadError::LinkedWorktreeOutsideProject => {
            git_error("git_repository_not_found", "Git repository was not found.")
        }
        GitReadError::ExecutableUnavailable => {
            git_error("git_unavailable", "Git capability is unavailable.")
        }
        _ => mutation_failed(),
    }
}

fn mutation_failed() -> CommandError {
    git_error("git_mutation_failed", "Git operation failed.")
}

fn next_generation(previous: u64) -> u64 {
    if previous >= MAX_JS_SAFE_INTEGER {
        1
    } else {
        previous.saturating_add(1)
    }
}

// ---------------------------------------------------------------------------
// DTO projection
// ---------------------------------------------------------------------------

fn map_mutation_dto(
    receipt: GitMutationReceipt,
    repository_id: &str,
    generation: u64,
) -> GitMutationDto {
    let operation = match receipt.operation {
        GitMutationKind::Stage => "stage",
        GitMutationKind::Unstage => "unstage",
        GitMutationKind::Commit => "commit",
        GitMutationKind::Push => "push",
    };
    GitMutationDto {
        repository_id: repository_id.to_owned(),
        generation,
        operation: operation.to_owned(),
        remote_name: receipt.remote_name,
        branch_name: receipt.branch_name,
    }
}

fn map_status_dto(snap: GitStatusSnapshot) -> GitStatusDto {
    let (branch, detached, head_short) = match &snap.head {
        GitHead::Branch { name, oid } => (Some(name.clone()), false, short_oid(oid)),
        GitHead::Detached { oid } => (None, true, short_oid(oid)),
        GitHead::Unknown => (None, false, String::new()),
    };

    let files: Vec<GitStatusEntryDto> = snap
        .entries
        .into_iter()
        .filter(|e| e.kind != GitStatusKind::Ignored)
        .map(map_status_entry)
        .collect();
    let dirty_count = files.len() as u32;

    GitStatusDto {
        repository_id: snap.repository.project_relative,
        generation: snap.generation,
        branch,
        detached,
        head_short,
        dirty_count,
        files,
        truncated: snap.truncated,
    }
}

fn map_status_entry(entry: GitStatusEntry) -> GitStatusEntryDto {
    GitStatusEntryDto {
        path: entry.path,
        kind: status_kind_wire(entry.kind).to_owned(),
        original_path: entry.old_path,
    }
}

fn status_kind_wire(kind: GitStatusKind) -> &'static str {
    match kind {
        GitStatusKind::Modified => "modified",
        GitStatusKind::Added => "added",
        GitStatusKind::Deleted => "deleted",
        GitStatusKind::Untracked => "untracked",
        GitStatusKind::Renamed => "renamed",
        GitStatusKind::Copied => "copied",
        GitStatusKind::Typechange => "typechange",
        GitStatusKind::Conflict => "conflict",
        GitStatusKind::Submodule => "submodule",
        GitStatusKind::Ignored => "ignored",
        GitStatusKind::Unknown => "unknown",
    }
}

fn map_diff_dto(snap: GitDiffSnapshot) -> GitDiffDto {
    let repository_id = snap.repository.project_relative;
    let generation = snap.generation;
    match snap.view {
        ExecutorDiffView::Text {
            path,
            content,
            byte_count,
        } => {
            let line_count = count_lines(&content);
            GitDiffDto {
                repository_id,
                generation,
                path,
                kind: "text".to_owned(),
                unified: Some(content),
                truncated: false,
                line_count: Some(line_count),
                byte_count: Some(byte_count as u32),
                original_path: None,
            }
        }
        ExecutorDiffView::Binary {
            path,
            size_bytes: _,
        } => GitDiffDto {
            repository_id,
            generation,
            path,
            kind: "binary".to_owned(),
            unified: None,
            truncated: false,
            line_count: None,
            byte_count: None,
            original_path: None,
        },
        ExecutorDiffView::Oversized {
            path,
            size_bytes: _,
            limit_bytes: _,
        } => GitDiffDto {
            repository_id,
            generation,
            path,
            kind: "unsupported".to_owned(),
            unified: None,
            truncated: true,
            line_count: None,
            byte_count: None,
            original_path: None,
        },
    }
}

fn short_oid(oid: &str) -> String {
    if oid.is_empty() || oid == "(initial)" {
        return oid.to_owned();
    }
    oid.chars().take(7).collect()
}

fn count_lines(text: &str) -> u32 {
    if text.is_empty() {
        return 0;
    }
    let n = if text.ends_with('\n') {
        text.split('\n').count().saturating_sub(1)
    } else {
        text.split('\n').count()
    };
    n as u32
}

/// Public error codes only (for tests and documentation).
#[cfg(test)]
pub(crate) fn is_git_public_error(code: &str) -> bool {
    matches!(
        code,
        "git_project_not_found"
            | "git_path_rejected"
            | "git_repository_not_found"
            | "git_unavailable"
            | "git_generation_stale"
            | "git_generation_mismatch"
            | "git_message_rejected"
            | "git_branch_rejected"
            | "git_remote_rejected"
            | "git_push_confirmation_required"
            | "git_mutation_failed"
    )
}
