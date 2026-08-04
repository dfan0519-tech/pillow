//! Project-scoped Git repository discovery (no process launch helpers here
//! beyond pure path probes). Linked worktrees whose real path leaves the
//! project root are rejected by the runner after gitdir resolution.

use std::fs;
use std::path::{Path, PathBuf};

use crate::runner::GitReadError;
use crate::status::validate_portable_path;

/// Maximum directory depth walked under the project root when searching for
/// nested repositories.
pub const DISCOVERY_MAX_DEPTH: usize = 8;

/// Maximum repositories returned from one discovery pass.
pub const DISCOVERY_MAX_REPOS: usize = 32;

/// One discovered repository identity (portable project-relative path only).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DiscoveredRepository {
    pub project_relative: String,
    /// `false` when the repository is visible but not safe to read (for example
    /// an outside-root linked worktree). Callers must not invoke status/diff
    /// against unavailable entries.
    pub available: bool,
}

/// Bounded discovery result for one project root.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GitDiscovery {
    pub repositories: Vec<DiscoveredRepository>,
}

/// Pure filesystem probe: find `.git` markers under `project_root` without
/// following reparse points / symlinks out of the project.
pub(crate) fn walk_git_markers(project_root: &Path) -> Result<Vec<PathBuf>, GitReadError> {
    let root = canonicalize_dir(project_root)?;
    let mut found = Vec::new();
    walk_inner(&root, &root, 0, &mut found)?;
    Ok(found)
}

fn walk_inner(
    project_root: &Path,
    dir: &Path,
    depth: usize,
    found: &mut Vec<PathBuf>,
) -> Result<(), GitReadError> {
    if found.len() >= DISCOVERY_MAX_REPOS || depth > DISCOVERY_MAX_DEPTH {
        return Ok(());
    }

    let git_marker = dir.join(".git");
    if path_exists_no_follow(&git_marker) {
        found.push(dir.to_path_buf());
        if found.len() >= DISCOVERY_MAX_REPOS {
            return Ok(());
        }
        // Do not walk into `.git` content; still may walk siblings.
    }

    if depth == DISCOVERY_MAX_DEPTH {
        return Ok(());
    }

    let entries = match fs::read_dir(dir) {
        Ok(e) => e,
        Err(_) => return Ok(()),
    };

    for entry in entries.flatten() {
        if found.len() >= DISCOVERY_MAX_REPOS {
            break;
        }
        let name = entry.file_name();
        let name_str = name.to_string_lossy();
        if name_str == ".git" || name_str == "." || name_str == ".." {
            continue;
        }
        // Skip common heavy/irrelevant trees.
        if matches!(
            name_str.as_ref(),
            "node_modules" | "target" | ".cargo" | "dist" | "build" | ".venv" | "venv"
        ) {
            continue;
        }
        let path = entry.path();
        // Reject Windows junctions / reparse points and POSIX symlinks before
        // any existence or directory probe that would follow them.
        let meta = match fs::symlink_metadata(&path) {
            Ok(m) => m,
            Err(_) => continue,
        };
        if meta.file_type().is_symlink() || is_reparse_point(&meta) {
            continue;
        }
        if !meta.is_dir() {
            continue;
        }
        // Prove the child remains inside the project after realpath.
        let Ok(canon) = fs::canonicalize(&path) else {
            continue;
        };
        let canon = strip_verbatim_prefix(canon);
        if canon != project_root && !canon.starts_with(project_root) {
            continue;
        }
        walk_inner(project_root, &canon, depth + 1, found)?;
    }
    Ok(())
}

/// Convert an absolute worktree path into a portable project-relative path.
pub(crate) fn project_relative_path(
    project_root: &Path,
    worktree: &Path,
) -> Result<String, GitReadError> {
    let root = canonicalize_dir(project_root)?;
    let tree = canonicalize_dir(worktree)?;
    if tree == root {
        validate_portable_path(".").map_err(|_| GitReadError::PathRejected)?;
        return Ok(".".to_string());
    }
    let rel = tree
        .strip_prefix(&root)
        .map_err(|_| GitReadError::RepoOutsideProject)?;
    let portable = rel
        .components()
        .map(|c| c.as_os_str().to_string_lossy())
        .collect::<Vec<_>>()
        .join("/");
    if portable.is_empty() {
        return Ok(".".to_string());
    }
    validate_portable_path(&portable).map_err(|_| GitReadError::PathRejected)?;
    Ok(portable)
}

/// Ensure `candidate` is the same as or a strict subdirectory of `project_root`.
pub(crate) fn ensure_under_project(
    project_root: &Path,
    candidate: &Path,
) -> Result<PathBuf, GitReadError> {
    let root = canonicalize_dir(project_root)?;
    let path = if candidate.is_dir() {
        canonicalize_dir(candidate)?
    } else {
        let canon = fs::canonicalize(candidate).map_err(|_| GitReadError::PathRejected)?;
        strip_verbatim_prefix(canon)
    };
    if path == root || path.starts_with(&root) {
        Ok(path)
    } else {
        Err(GitReadError::RepoOutsideProject)
    }
}

/// Ensure an already-resolved absolute path stays under the project root.
pub(crate) fn ensure_resolved_under_project(
    project_root: &Path,
    resolved: &Path,
) -> Result<PathBuf, GitReadError> {
    let root = canonicalize_dir(project_root)?;
    let path = strip_verbatim_prefix(resolved.to_path_buf());
    if path == root || path.starts_with(&root) {
        Ok(path)
    } else {
        Err(GitReadError::RepoOutsideProject)
    }
}

pub(crate) fn resolve_repo_workdir(
    project_root: &Path,
    project_relative: &str,
) -> Result<PathBuf, GitReadError> {
    validate_portable_path(project_relative).map_err(|_| GitReadError::PathRejected)?;
    let root = canonicalize_dir(project_root)?;
    if project_relative == "." {
        return Ok(root);
    }
    resolve_under_root_no_reparse(&root, project_relative)
}

/// Resolve a portable relative directory under `root` without following
/// symlinks or Windows reparse points/junctions. Every component is checked
/// before descent.
pub(crate) fn resolve_under_root_no_reparse(
    root: &Path,
    relative: &str,
) -> Result<PathBuf, GitReadError> {
    validate_portable_path(relative).map_err(|_| GitReadError::PathRejected)?;
    let root = canonicalize_dir(root)?;
    if relative == "." {
        return Ok(root);
    }

    let mut current = root.clone();
    let components: Vec<&str> = relative.split('/').collect();
    for (idx, component) in components.iter().enumerate() {
        current.push(component);
        let meta = fs::symlink_metadata(&current).map_err(|_| GitReadError::PathRejected)?;
        if meta.file_type().is_symlink() || is_reparse_point(&meta) {
            return Err(GitReadError::PathRejected);
        }
        let is_last = idx + 1 == components.len();
        if is_last {
            if !meta.is_dir() {
                return Err(GitReadError::RepoNotFound);
            }
        } else if !meta.is_dir() {
            return Err(GitReadError::PathRejected);
        }

        // Canonicalize each existing real component and re-check authority.
        let canon = fs::canonicalize(&current).map_err(|_| GitReadError::PathRejected)?;
        let canon = strip_verbatim_prefix(canon);
        if canon != root && !canon.starts_with(&root) {
            return Err(GitReadError::PathRejected);
        }
        current = canon;
    }
    Ok(current)
}

pub(crate) fn canonicalize_dir(path: &Path) -> Result<PathBuf, GitReadError> {
    let meta = fs::symlink_metadata(path).map_err(|_| GitReadError::PathRejected)?;
    if meta.file_type().is_symlink() || is_reparse_point(&meta) {
        return Err(GitReadError::PathRejected);
    }
    let canon = fs::canonicalize(path).map_err(|_| GitReadError::PathRejected)?;
    if !canon.is_dir() {
        return Err(GitReadError::PathRejected);
    }
    Ok(strip_verbatim_prefix(canon))
}

/// Windows `\\?\` / `//?/` prefix strip for stable prefix comparisons.
pub(crate) fn strip_verbatim_prefix(path: PathBuf) -> PathBuf {
    let s = path.to_string_lossy();
    if let Some(rest) = s.strip_prefix(r"\\?\") {
        return PathBuf::from(rest);
    }
    if let Some(rest) = s.strip_prefix("//?/") {
        return PathBuf::from(rest);
    }
    path
}

pub(crate) fn path_exists_no_follow(path: &Path) -> bool {
    fs::symlink_metadata(path).is_ok()
}

#[allow(dead_code)] // retained for discovery call sites / future probes
pub(crate) fn is_symlink_or_reparse(path: &Path) -> bool {
    match fs::symlink_metadata(path) {
        Ok(meta) => meta_is_symlink_or_reparse(&meta),
        Err(_) => false,
    }
}

/// Windows `FILE_ATTRIBUTE_REPARSE_POINT` (0x400). Junctions and mount points
/// set this bit even when Rust's `is_symlink()` is false.
#[cfg(windows)]
pub(crate) fn is_reparse_point(metadata: &fs::Metadata) -> bool {
    use std::os::windows::fs::MetadataExt;
    const FILE_ATTRIBUTE_REPARSE_POINT: u32 = 0x400;
    metadata.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0
}

#[cfg(not(windows))]
pub(crate) fn is_reparse_point(_metadata: &fs::Metadata) -> bool {
    false
}

/// Byte cap for repository metadata files (config, attributes, gitfile, commondir).
pub(crate) const METADATA_FILE_BYTE_CAP: usize = 256 * 1024;

/// True when metadata is a symlink or Windows reparse point (junction, mount, …).
pub(crate) fn meta_is_symlink_or_reparse(metadata: &fs::Metadata) -> bool {
    metadata.file_type().is_symlink() || is_reparse_point(metadata)
}

/// `symlink_metadata` for `path`, mapping IO errors to `PathRejected`.
pub(crate) fn symlink_metadata_nofollow(path: &Path) -> Result<fs::Metadata, GitReadError> {
    fs::symlink_metadata(path).map_err(|_| GitReadError::PathRejected)
}

/// Reject symlink / reparse; require regular file or directory as requested.
///
/// Returns `Ok(None)` when the path is missing. Canonicalizes and proves the
/// resolved path stays under `project_root`. Outside-project resolves preserve
/// [`GitReadError::RepoOutsideProject`] so callers can map to linked-worktree
/// unavailability; reparse/symlink always yields [`GitReadError::PathRejected`].
pub(crate) fn open_contained_entry(
    project_root: &Path,
    path: &Path,
    expect_dir: bool,
) -> Result<Option<PathBuf>, GitReadError> {
    let meta = match fs::symlink_metadata(path) {
        Ok(m) => m,
        Err(_) => return Ok(None),
    };
    if meta_is_symlink_or_reparse(&meta) {
        return Err(GitReadError::PathRejected);
    }
    if expect_dir {
        if !meta.is_dir() {
            return Err(GitReadError::PathRejected);
        }
    } else if !meta.is_file() {
        return Err(GitReadError::PathRejected);
    }
    let canon = fs::canonicalize(path).map_err(|_| GitReadError::PathRejected)?;
    let canon = strip_verbatim_prefix(canon);
    ensure_resolved_under_project(project_root, &canon)?;
    Ok(Some(canon))
}

/// Resolve `relative` (portable `/`-separated) under an already-contained
/// `base` without following symlinks or reparse points on any component —
/// including intermediate ancestors.
///
/// `Ok(None)` means the final component is missing (parents existed and were
/// safe). Mid-path missing ⇒ `PathRejected`.
pub(crate) fn resolve_under_base_no_follow(
    project_root: &Path,
    base: &Path,
    relative: &str,
    expect_dir: bool,
) -> Result<Option<PathBuf>, GitReadError> {
    if relative.is_empty() || relative == "." {
        return open_contained_entry(project_root, base, true);
    }
    // Base must already be a real, contained directory.
    let mut current =
        open_contained_entry(project_root, base, true)?.ok_or(GitReadError::PathRejected)?;

    let components: Vec<&str> = relative.split('/').filter(|c| !c.is_empty()).collect();
    if components.is_empty() {
        return Ok(Some(current));
    }
    for (idx, component) in components.iter().enumerate() {
        if *component == "." {
            continue;
        }
        if *component == ".." {
            // Explicit parent traversal is only legal when the parent remains
            // under the project root after resolve.
            current = current
                .parent()
                .map(Path::to_path_buf)
                .ok_or(GitReadError::PathRejected)?;
            let meta = symlink_metadata_nofollow(&current)?;
            if meta_is_symlink_or_reparse(&meta) || !meta.is_dir() {
                return Err(GitReadError::PathRejected);
            }
            let canon = fs::canonicalize(&current).map_err(|_| GitReadError::PathRejected)?;
            current = strip_verbatim_prefix(canon);
            ensure_resolved_under_project(project_root, &current)?;
            continue;
        }
        if component.contains('\\') || component.contains('\0') {
            return Err(GitReadError::PathRejected);
        }
        current.push(component);
        let is_last = idx + 1 == components.len();
        let meta = match fs::symlink_metadata(&current) {
            Ok(m) => m,
            Err(_) => {
                // Missing component ⇒ the relative path does not exist.
                // (Including mid-path: `info/attributes` when `info` is absent.)
                return Ok(None);
            }
        };
        if meta_is_symlink_or_reparse(&meta) {
            return Err(GitReadError::PathRejected);
        }
        if is_last {
            if expect_dir {
                if !meta.is_dir() {
                    return Err(GitReadError::PathRejected);
                }
            } else if !meta.is_file() {
                return Err(GitReadError::PathRejected);
            }
        } else if !meta.is_dir() {
            return Err(GitReadError::PathRejected);
        }
        let canon = fs::canonicalize(&current).map_err(|_| GitReadError::PathRejected)?;
        current = strip_verbatim_prefix(canon);
        ensure_resolved_under_project(project_root, &current)?;
    }
    Ok(Some(current))
}

/// Read a regular, non-reparse metadata file under project authority.
///
/// Oversized content or invalid UTF-8 fails closed. Missing path → `Ok(None)`.
pub(crate) fn read_contained_metadata_file(
    project_root: &Path,
    path: &Path,
) -> Result<Option<String>, GitReadError> {
    let Some(canon) = open_contained_entry(project_root, path, false)? else {
        return Ok(None);
    };
    read_metadata_bytes_as_utf8(&canon)
}

/// Read a relative metadata file under a contained base (component-wise).
pub(crate) fn read_metadata_under_base(
    project_root: &Path,
    base: &Path,
    relative: &str,
) -> Result<Option<String>, GitReadError> {
    let Some(canon) = resolve_under_base_no_follow(project_root, base, relative, false)? else {
        return Ok(None);
    };
    read_metadata_bytes_as_utf8(&canon)
}

fn read_metadata_bytes_as_utf8(path: &Path) -> Result<Option<String>, GitReadError> {
    // Re-check immediately before the read (no-follow).
    let meta = symlink_metadata_nofollow(path)?;
    if meta_is_symlink_or_reparse(&meta) || !meta.is_file() {
        return Err(GitReadError::PathRejected);
    }
    let len = meta.len();
    if len > METADATA_FILE_BYTE_CAP as u64 {
        return Err(GitReadError::MalformedConfiguration);
    }
    let bytes = fs::read(path).map_err(|_| GitReadError::PathRejected)?;
    if bytes.len() > METADATA_FILE_BYTE_CAP {
        return Err(GitReadError::MalformedConfiguration);
    }
    // Invalid UTF-8 metadata fails closed.
    let text = String::from_utf8(bytes)
        .map_err(|_| GitReadError::Parse(crate::status::ParseError::Utf8))?;
    Ok(Some(text))
}

/// Read a `.git` file (gitdir pointer) and return the absolute gitdir path
/// without treating a missing target as a successful resolve.
///
/// The gitfile itself must already have been proven non-reparse by the caller.
pub(crate) fn parse_gitfile(project_root: &Path, git_file: &Path) -> Result<PathBuf, GitReadError> {
    let text = read_contained_metadata_file(project_root, git_file)?
        .ok_or(GitReadError::NotARepository)?;
    for line in text.lines() {
        let line = line.trim();
        if let Some(rest) = line.strip_prefix("gitdir:") {
            let raw = rest.trim();
            if raw.is_empty() {
                return Err(GitReadError::NotARepository);
            }
            let base = git_file.parent().unwrap_or_else(|| Path::new("."));
            if Path::new(raw).is_absolute() {
                match open_contained_entry(project_root, Path::new(raw), true) {
                    Ok(Some(p)) => return Ok(p),
                    Ok(None) => return Err(GitReadError::NotARepository),
                    Err(GitReadError::RepoOutsideProject) => {
                        return Err(GitReadError::LinkedWorktreeOutsideProject);
                    }
                    Err(e) => return Err(e),
                }
            }
            // Relative gitdir: resolve component-wise from the gitfile parent.
            let portable = raw.replace('\\', "/");
            match resolve_under_base_no_follow(project_root, base, &portable, true) {
                Ok(Some(p)) => return Ok(p),
                Ok(None) => return Err(GitReadError::NotARepository),
                Err(GitReadError::RepoOutsideProject) => {
                    return Err(GitReadError::LinkedWorktreeOutsideProject);
                }
                Err(e) => return Err(e),
            }
        }
    }
    Err(GitReadError::NotARepository)
}

/// Map a repository-relative portable path into a project-relative path.
pub(crate) fn map_repo_path_to_project(
    repo_project_relative: &str,
    repo_relative: &str,
) -> Result<String, GitReadError> {
    validate_portable_path(repo_relative).map_err(|_| GitReadError::PathRejected)?;
    if repo_project_relative == "." {
        return Ok(repo_relative.to_string());
    }
    let combined = format!("{repo_project_relative}/{repo_relative}");
    validate_portable_path(&combined).map_err(|_| GitReadError::PathRejected)?;
    Ok(combined)
}
