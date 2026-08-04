//! Project-scoped read-only resource core.
//!
//! Provides deterministic directory listing, bounded text/binary reads, and
//! bounded text search. No write, process, Git, network, credential, Obsidian,
//! MCP, or runtime-launch capability is exposed.

use std::fs::{self, File, Metadata};
use std::io::{BufRead, BufReader, Read};
use std::path::{Path, PathBuf};

use keeper_project::ProjectId;
use serde::{Deserialize, Serialize};
use thiserror::Error;

/// Hard cap on directory list entries (caller limit is clamped to this).
pub const MAX_LIST_ENTRIES: u32 = 1_000;
/// Hard cap on a single read (1 MiB).
pub const MAX_READ_BYTES: u64 = 1_048_576;
/// Hard cap on search result matches (caller limit is clamped).
pub const MAX_SEARCH_RESULTS: u32 = 1_000;
/// Maximum candidate regular files visited during a search walk.
pub const MAX_SEARCH_FILES: usize = 100_000;
/// Lines longer than this are skipped during search (no unbounded read).
const MAX_SEARCH_LINE_BYTES: usize = 65_536;
/// UTF-8 preview bound for a search match.
const MAX_SEARCH_PREVIEW_BYTES: usize = 2_048;

/// Public resource errors never embed absolute machine paths or file contents.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum ResourceError {
    #[error("project resource root could not be established")]
    InvalidProjectRoot,
    #[error("resource path was rejected")]
    PathRejected,
    #[error("resource search query was rejected")]
    InvalidSearchQuery,
    #[error("resource target was not found")]
    NotFound,
    #[error("resource target is not a directory")]
    NotADirectory,
    #[error("resource target is not a regular file")]
    NotAFile,
    #[error("resource operation exceeded a bound")]
    LimitExceeded,
    #[error("resource operation failed")]
    InternalError,
}

/// Captures a canonical existing project directory and owning [`ProjectId`].
#[derive(Debug, Clone)]
pub struct ProjectResourceCapability {
    project_id: ProjectId,
    canonical_root: PathBuf,
}

impl ProjectResourceCapability {
    pub fn new(project_id: ProjectId, root: impl AsRef<Path>) -> Result<Self, ResourceError> {
        let canonical_root = root
            .as_ref()
            .canonicalize()
            .map_err(|_| ResourceError::InvalidProjectRoot)?;
        if !canonical_root.is_dir() {
            return Err(ResourceError::InvalidProjectRoot);
        }
        Ok(Self {
            project_id,
            canonical_root,
        })
    }

    pub fn project_id(&self) -> ProjectId {
        self.project_id
    }

    pub fn canonical_root(&self) -> &Path {
        &self.canonical_root
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ResourceEntryKind {
    File,
    Directory,
    Symlink,
    Other,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ResourceContentKind {
    Utf8,
    Binary,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ResourceEntry {
    pub path: String,
    pub kind: ResourceEntryKind,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub size_bytes: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ResourceDirectory {
    pub entries: Vec<ResourceEntry>,
    pub truncated: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ResourceFile {
    pub path: String,
    pub content_kind: ResourceContentKind,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub content: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bytes: Option<Vec<u8>>,
    pub bytes_returned: u64,
    pub truncated: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ResourceSearchMatch {
    pub path: String,
    /// One-based line number.
    pub line: u64,
    /// One-based byte column within the line.
    pub column: u64,
    pub preview: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ResourceSearchResults {
    pub matches: Vec<ResourceSearchMatch>,
    pub truncated: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ResourceListRequest {
    pub path: String,
    pub max_entries: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ResourceReadRequest {
    pub path: String,
    pub max_bytes: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ResourceSearchRequest {
    pub path: String,
    pub query: String,
    pub case_sensitive: bool,
    pub max_results: u32,
}

/// Stateless read-only resource reader.
#[derive(Debug, Default, Clone, Copy)]
pub struct ResourceReader;

impl ResourceReader {
    pub fn list(
        &self,
        capability: &ProjectResourceCapability,
        request: ResourceListRequest,
    ) -> Result<ResourceDirectory, ResourceError> {
        let root = revalidate_root(capability)?;
        let directory = resolve_target(&root, &request.path)?;
        let metadata = directory
            .symlink_metadata()
            .map_err(|_| ResourceError::PathRejected)?;
        if metadata.file_type().is_symlink() || is_reparse_point(&metadata) {
            return Err(ResourceError::PathRejected);
        }
        if !metadata.is_dir() {
            return Err(ResourceError::NotADirectory);
        }

        let limit = request.max_entries.min(MAX_LIST_ENTRIES) as usize;
        let mut children = fs::read_dir(&directory)
            .map_err(|_| ResourceError::InternalError)?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|_| ResourceError::InternalError)?;
        children.sort_by(|left, right| {
            left.file_name()
                .to_string_lossy()
                .cmp(&right.file_name().to_string_lossy())
        });

        let mut entries = Vec::new();
        let mut truncated = false;
        for child in children {
            if entries.len() >= limit {
                truncated = true;
                break;
            }
            let child_path = child.path();
            let metadata = child_path
                .symlink_metadata()
                .map_err(|_| ResourceError::InternalError)?;
            let kind = entry_kind(&metadata);
            let size_bytes = if kind == ResourceEntryKind::File {
                Some(metadata.len())
            } else {
                None
            };
            entries.push(ResourceEntry {
                path: portable_relative(&root, &child_path)?,
                kind,
                size_bytes,
            });
        }

        Ok(ResourceDirectory { entries, truncated })
    }

    pub fn read(
        &self,
        capability: &ProjectResourceCapability,
        request: ResourceReadRequest,
    ) -> Result<ResourceFile, ResourceError> {
        let root = revalidate_root(capability)?;
        let path = resolve_target(&root, &request.path)?;
        let metadata = path.symlink_metadata().map_err(map_path_io)?;
        if metadata.file_type().is_symlink() || is_reparse_point(&metadata) {
            return Err(ResourceError::PathRejected);
        }
        if !metadata.is_file() {
            return Err(ResourceError::NotAFile);
        }

        let max_bytes = request.max_bytes.min(MAX_READ_BYTES);
        let mut file = File::open(&path).map_err(map_path_io)?;
        // Re-check after open: refuse if the leaf became a reparse/symlink.
        let reopen_meta = path.symlink_metadata().map_err(map_path_io)?;
        if reopen_meta.file_type().is_symlink() || is_reparse_point(&reopen_meta) {
            return Err(ResourceError::PathRejected);
        }
        let length = reopen_meta.len();
        let mut bytes = Vec::with_capacity(max_bytes.min(65_536) as usize);
        file.by_ref()
            .take(max_bytes)
            .read_to_end(&mut bytes)
            .map_err(|_| ResourceError::InternalError)?;

        let bytes_returned = bytes.len() as u64;
        let truncated = length > bytes_returned;

        match String::from_utf8(bytes) {
            Ok(content) => Ok(ResourceFile {
                path: request.path,
                content_kind: ResourceContentKind::Utf8,
                content: Some(content),
                bytes: None,
                bytes_returned,
                truncated,
            }),
            Err(error) => Ok(ResourceFile {
                path: request.path,
                content_kind: ResourceContentKind::Binary,
                content: None,
                bytes: Some(error.into_bytes()),
                bytes_returned,
                truncated,
            }),
        }
    }

    pub fn search(
        &self,
        capability: &ProjectResourceCapability,
        request: ResourceSearchRequest,
    ) -> Result<ResourceSearchResults, ResourceError> {
        if request.query.trim().is_empty() {
            return Err(ResourceError::InvalidSearchQuery);
        }
        let root = revalidate_root(capability)?;
        let target = resolve_target(&root, &request.path)?;
        let mut files = Vec::new();
        collect_files(&root, &target, &mut files)?;
        files.sort_by(|left, right| {
            portable_relative(&root, left)
                .unwrap_or_default()
                .cmp(&portable_relative(&root, right).unwrap_or_default())
        });

        let max_results = request.max_results.min(MAX_SEARCH_RESULTS) as usize;
        let needle = if request.case_sensitive {
            request.query.clone()
        } else {
            request.query.to_lowercase()
        };

        let mut matches = Vec::new();
        let mut truncated = false;

        'files: for path in files {
            let file = File::open(&path).map_err(|_| ResourceError::InternalError)?;
            let portable_path = portable_relative(&root, &path)?;
            // Refuse if the path no longer resolves as a regular non-reparse file.
            let current = resolve_target(&root, &portable_path)?;
            if current != path {
                return Err(ResourceError::PathRejected);
            }
            let mut reader = BufReader::new(file);
            let mut line_number = 0_u64;
            while let Some((mut bytes, overflowed)) = read_bounded_line(&mut reader)? {
                line_number += 1;
                if overflowed {
                    continue;
                }
                while matches!(bytes.last(), Some(b'\r')) {
                    bytes.pop();
                }
                let Ok(line) = std::str::from_utf8(&bytes) else {
                    continue;
                };
                let haystack = if request.case_sensitive {
                    line.to_owned()
                } else {
                    line.to_lowercase()
                };
                let mut start = 0;
                while let Some(found) = haystack[start..].find(&needle) {
                    if matches.len() >= max_results {
                        truncated = true;
                        break 'files;
                    }
                    let byte_index = start + found;
                    matches.push(ResourceSearchMatch {
                        path: portable_path.clone(),
                        line: line_number,
                        column: byte_index as u64 + 1,
                        preview: bounded_preview(line),
                    });
                    start = byte_index + needle.len().max(1);
                }
            }
        }

        Ok(ResourceSearchResults { matches, truncated })
    }
}

fn revalidate_root(capability: &ProjectResourceCapability) -> Result<PathBuf, ResourceError> {
    let root = capability
        .canonical_root
        .canonicalize()
        .map_err(|_| ResourceError::PathRejected)?;
    if root != capability.canonical_root || !root.is_dir() {
        return Err(ResourceError::PathRejected);
    }
    Ok(root)
}

/// Accept only relative portable paths. Root is exclusively `.`.
fn validate_portable_path(path: &str) -> Result<(), ResourceError> {
    if path == "." {
        return Ok(());
    }
    if path.is_empty() || path.len() > 4_096 {
        return Err(ResourceError::PathRejected);
    }
    if path.starts_with('/') || path.starts_with('\\') || path.contains('\\') || path.contains(':')
    {
        return Err(ResourceError::PathRejected);
    }
    for component in path.split('/') {
        if component.is_empty() || component == "." || component == ".." {
            return Err(ResourceError::PathRejected);
        }
        if component.len() > 255 {
            return Err(ResourceError::PathRejected);
        }
        if component.ends_with('.') || component.ends_with(' ') {
            return Err(ResourceError::PathRejected);
        }
        if component.chars().any(|character| {
            character.is_control() || matches!(character, '<' | '>' | '"' | '|' | '?' | '*')
        }) {
            return Err(ResourceError::PathRejected);
        }
        if is_windows_device_component(component) {
            return Err(ResourceError::PathRejected);
        }
    }
    Ok(())
}

fn is_windows_device_component(component: &str) -> bool {
    let stem = component
        .split('.')
        .next()
        .unwrap_or(component)
        .trim_end_matches([' ', '.'])
        .to_ascii_uppercase();
    if matches!(
        stem.as_str(),
        "CON" | "PRN" | "AUX" | "NUL" | "CLOCK$" | "CONIN$" | "CONOUT$"
    ) {
        return true;
    }
    let bytes = stem.as_bytes();
    (bytes.len() == 4 && matches!(&bytes[..3], b"COM" | b"LPT") && matches!(bytes[3], b'1'..=b'9'))
        || matches!(
            stem.as_str(),
            "COM\u{00b9}"
                | "COM\u{00b2}"
                | "COM\u{00b3}"
                | "LPT\u{00b9}"
                | "LPT\u{00b2}"
                | "LPT\u{00b3}"
        )
}

/// Resolve a portable relative path under `root`, rejecting symlinks/reparse
/// points at every component and refusing targets outside the root.
fn resolve_target(root: &Path, relative: &str) -> Result<PathBuf, ResourceError> {
    validate_portable_path(relative)?;
    let mut current = root.to_owned();
    if relative != "." {
        for component in relative.split('/') {
            current.push(component);
            let metadata = current.symlink_metadata().map_err(map_path_io)?;
            if metadata.file_type().is_symlink() || is_reparse_point(&metadata) {
                return Err(ResourceError::PathRejected);
            }
            let canonical = current.canonicalize().map_err(map_path_io)?;
            if !canonical.starts_with(root) {
                return Err(ResourceError::PathRejected);
            }
            current = canonical;
        }
    }
    Ok(current)
}

fn collect_files(
    root: &Path,
    target: &Path,
    output: &mut Vec<PathBuf>,
) -> Result<(), ResourceError> {
    let metadata = target.symlink_metadata().map_err(map_path_io)?;
    if metadata.file_type().is_symlink() || is_reparse_point(&metadata) {
        return Err(ResourceError::PathRejected);
    }
    if metadata.is_file() {
        if output.len() >= MAX_SEARCH_FILES {
            return Err(ResourceError::LimitExceeded);
        }
        output.push(target.to_owned());
        return Ok(());
    }
    if !metadata.is_dir() {
        return Err(ResourceError::PathRejected);
    }

    let mut children = fs::read_dir(target)
        .map_err(|_| ResourceError::InternalError)?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|_| ResourceError::InternalError)?;
    children.sort_by(|left, right| {
        left.file_name()
            .to_string_lossy()
            .cmp(&right.file_name().to_string_lossy())
    });
    for child in children {
        let path = child.path();
        let metadata = path
            .symlink_metadata()
            .map_err(|_| ResourceError::InternalError)?;
        // Symlinks / reparse points are never followed during search.
        if metadata.file_type().is_symlink() || is_reparse_point(&metadata) {
            continue;
        }
        let canonical = path
            .canonicalize()
            .map_err(|_| ResourceError::InternalError)?;
        if !canonical.starts_with(root) {
            return Err(ResourceError::PathRejected);
        }
        collect_files(root, &canonical, output)?;
    }
    Ok(())
}

fn read_bounded_line<R: BufRead>(reader: &mut R) -> Result<Option<(Vec<u8>, bool)>, ResourceError> {
    let mut output = Vec::new();
    let mut saw_data = false;
    let mut overflowed = false;
    loop {
        let buffer = reader
            .fill_buf()
            .map_err(|_| ResourceError::InternalError)?;
        if buffer.is_empty() {
            return if saw_data {
                Ok(Some((output, overflowed)))
            } else {
                Ok(None)
            };
        }
        saw_data = true;
        let newline = buffer.iter().position(|byte| *byte == b'\n');
        let consumed = newline.map_or(buffer.len(), |index| index + 1);
        let content_length = newline.unwrap_or(buffer.len());
        if !overflowed {
            let remaining = MAX_SEARCH_LINE_BYTES.saturating_sub(output.len());
            let copied = content_length.min(remaining);
            output.extend_from_slice(&buffer[..copied]);
            if copied < content_length
                || (newline.is_none() && output.len() == MAX_SEARCH_LINE_BYTES)
            {
                overflowed = true;
            }
        }
        reader.consume(consumed);
        if newline.is_some() {
            return Ok(Some((output, overflowed)));
        }
    }
}

fn bounded_preview(line: &str) -> String {
    let mut preview = String::new();
    for character in line.chars() {
        let character = if character.is_control() && character != '\t' {
            ' '
        } else {
            character
        };
        if preview.len() + character.len_utf8() > MAX_SEARCH_PREVIEW_BYTES {
            break;
        }
        preview.push(character);
    }
    preview
}

fn portable_relative(root: &Path, path: &Path) -> Result<String, ResourceError> {
    let relative = path
        .strip_prefix(root)
        .map_err(|_| ResourceError::PathRejected)?;
    let mut parts = Vec::new();
    for component in relative.components() {
        let text = component
            .as_os_str()
            .to_str()
            .ok_or(ResourceError::PathRejected)?;
        parts.push(text);
    }
    Ok(if parts.is_empty() {
        ".".into()
    } else {
        parts.join("/")
    })
}

fn entry_kind(metadata: &Metadata) -> ResourceEntryKind {
    if metadata.file_type().is_symlink() || is_reparse_point(metadata) {
        ResourceEntryKind::Symlink
    } else if metadata.is_file() {
        ResourceEntryKind::File
    } else if metadata.is_dir() {
        ResourceEntryKind::Directory
    } else {
        ResourceEntryKind::Other
    }
}

fn map_path_io(error: std::io::Error) -> ResourceError {
    match error.kind() {
        std::io::ErrorKind::NotFound => ResourceError::NotFound,
        _ => ResourceError::PathRejected,
    }
}

#[cfg(windows)]
fn is_reparse_point(metadata: &Metadata) -> bool {
    use std::os::windows::fs::MetadataExt;
    const FILE_ATTRIBUTE_REPARSE_POINT: u32 = 0x400;
    metadata.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0
}

#[cfg(not(windows))]
fn is_reparse_point(_: &Metadata) -> bool {
    false
}
