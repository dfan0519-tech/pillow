//! Typed Tauri resource commands — project-scoped, read-only bridge over
//! `keeper-resource` and durable recents. No mutation, no renderer filesystem.

use base64::{Engine as _, engine::general_purpose::STANDARD as BASE64};
use keeper_resource::{
    MAX_LIST_ENTRIES, MAX_READ_BYTES, MAX_SEARCH_RESULTS, ProjectResourceCapability,
    ResourceContentKind, ResourceEntry, ResourceEntryKind, ResourceError, ResourceFile,
    ResourceListRequest, ResourceReadRequest, ResourceReader, ResourceSearchMatch,
    ResourceSearchRequest,
};
use keeper_storage_sqlite::{MAX_RECENT_LIST, ResourceRecent};
use serde::Serialize;
use tauri::State;

use crate::app_state::{AppState, CommandError};

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ResourceEntryDto {
    pub path: String,
    pub kind: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub size_bytes: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ResourceFileDto {
    pub path: String,
    pub content_kind: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub content: Option<String>,
    pub bytes_returned: u64,
    pub truncated: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ResourceSearchResultDto {
    pub path: String,
    pub line: u64,
    pub column: u64,
    pub preview: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RecentResourceDto {
    pub path: String,
    pub opened_at: String,
}

/// Bounded preview DTO for the renderer. Never serializes raw `bytes`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ResourcePreviewDto {
    pub path: String,
    pub kind: String,
    pub media_type: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub content: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data_url: Option<String>,
    pub bytes_returned: u64,
    pub truncated: bool,
}

impl From<ResourceEntry> for ResourceEntryDto {
    fn from(entry: ResourceEntry) -> Self {
        Self {
            path: entry.path,
            kind: entry_kind_label(entry.kind).to_owned(),
            size_bytes: entry.size_bytes,
        }
    }
}

impl From<ResourceSearchMatch> for ResourceSearchResultDto {
    fn from(hit: ResourceSearchMatch) -> Self {
        Self {
            path: hit.path,
            line: hit.line,
            column: hit.column,
            preview: hit.preview,
        }
    }
}

impl From<ResourceRecent> for RecentResourceDto {
    fn from(recent: ResourceRecent) -> Self {
        Self {
            path: recent.path,
            opened_at: recent.opened_at.to_rfc3339(),
        }
    }
}

fn map_resource_file(file: ResourceFile) -> ResourceFileDto {
    // Never forward raw binary bytes or unbounded content to the renderer.
    match file.content_kind {
        ResourceContentKind::Utf8 => ResourceFileDto {
            path: file.path,
            content_kind: "utf8".to_owned(),
            content: file.content,
            bytes_returned: file.bytes_returned,
            truncated: file.truncated,
        },
        ResourceContentKind::Binary => ResourceFileDto {
            path: file.path,
            content_kind: "binary".to_owned(),
            content: None,
            bytes_returned: file.bytes_returned,
            truncated: file.truncated,
        },
    }
}

/// Lower-case portable file extension from a slash-separated relative path.
fn portable_extension(path: &str) -> Option<String> {
    let name = path.rsplit('/').next().unwrap_or(path);
    let (stem, ext) = name.rsplit_once('.')?;
    if stem.is_empty() || ext.is_empty() {
        return None;
    }
    Some(ext.to_ascii_lowercase())
}

fn matches_png(bytes: &[u8]) -> bool {
    bytes.starts_with(&[0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A])
}

fn matches_jpeg(bytes: &[u8]) -> bool {
    bytes.starts_with(&[0xFF, 0xD8, 0xFF])
}

fn matches_gif(bytes: &[u8]) -> bool {
    bytes.starts_with(b"GIF87a") || bytes.starts_with(b"GIF89a")
}

fn matches_webp(bytes: &[u8]) -> bool {
    bytes.len() >= 12 && &bytes[0..4] == b"RIFF" && &bytes[8..12] == b"WEBP"
}

/// Image only when complete (not truncated), extension allow-listed, and signature agrees.
fn classify_image(ext: &str, bytes: &[u8], truncated: bool) -> Option<&'static str> {
    if truncated {
        return None;
    }
    match ext {
        "png" if matches_png(bytes) => Some("image/png"),
        "jpg" | "jpeg" if matches_jpeg(bytes) => Some("image/jpeg"),
        "gif" if matches_gif(bytes) => Some("image/gif"),
        "webp" if matches_webp(bytes) => Some("image/webp"),
        _ => None,
    }
}

fn is_image_extension(ext: &str) -> bool {
    matches!(ext, "png" | "jpg" | "jpeg" | "gif" | "webp")
}

fn map_resource_preview(file: ResourceFile) -> ResourcePreviewDto {
    let extension = portable_extension(&file.path);

    // Recover raw bytes for signature checks. Some image payloads (GIF/WebP) can
    // be valid UTF-8; allow-listed extensions must still go through image rules.
    let raw_bytes: Vec<u8> = match file.content_kind {
        ResourceContentKind::Utf8 => file
            .content
            .as_ref()
            .map(|s| s.as_bytes().to_vec())
            .unwrap_or_default(),
        ResourceContentKind::Binary => file.bytes.unwrap_or_default(),
    };

    if let Some(ext) = extension.as_deref()
        && is_image_extension(ext)
    {
        if let Some(mime) = classify_image(ext, &raw_bytes, file.truncated) {
            return ResourcePreviewDto {
                path: file.path,
                kind: "image".to_owned(),
                media_type: mime.to_owned(),
                content: None,
                data_url: Some(format!("data:{mime};base64,{}", BASE64.encode(&raw_bytes))),
                bytes_returned: file.bytes_returned,
                truncated: file.truncated,
            };
        }
        // Extension allow-listed but truncated, signature mismatch, or empty.
        return ResourcePreviewDto {
            path: file.path,
            kind: "binary".to_owned(),
            media_type: "application/octet-stream".to_owned(),
            content: None,
            data_url: None,
            bytes_returned: file.bytes_returned,
            truncated: file.truncated,
        };
    }

    match file.content_kind {
        ResourceContentKind::Utf8 => {
            let is_markdown = extension.as_deref() == Some("md");
            ResourcePreviewDto {
                path: file.path,
                kind: if is_markdown {
                    "markdown".to_owned()
                } else {
                    "text".to_owned()
                },
                media_type: if is_markdown {
                    "text/markdown; charset=utf-8".to_owned()
                } else {
                    "text/plain; charset=utf-8".to_owned()
                },
                // Prefer original UTF-8 content; fall back to decoded raw bytes.
                content: file.content.or_else(|| String::from_utf8(raw_bytes).ok()),
                data_url: None,
                bytes_returned: file.bytes_returned,
                truncated: file.truncated,
            }
        }
        ResourceContentKind::Binary => ResourcePreviewDto {
            path: file.path,
            kind: "binary".to_owned(),
            media_type: "application/octet-stream".to_owned(),
            content: None,
            data_url: None,
            bytes_returned: file.bytes_returned,
            truncated: file.truncated,
        },
    }
}

fn entry_kind_label(kind: ResourceEntryKind) -> &'static str {
    match kind {
        ResourceEntryKind::File => "file",
        ResourceEntryKind::Directory => "directory",
        ResourceEntryKind::Symlink => "symlink",
        ResourceEntryKind::Other => "other",
    }
}

fn map_resource_error(error: ResourceError) -> CommandError {
    let (code, message) = match error {
        ResourceError::InvalidProjectRoot => (
            "resource_unavailable",
            "Project resource root is unavailable.",
        ),
        ResourceError::PathRejected => ("resource_path_rejected", "Resource path was rejected."),
        ResourceError::InvalidSearchQuery => (
            "resource_search_query_rejected",
            "Resource search query was rejected.",
        ),
        ResourceError::NotFound => ("resource_not_found", "Resource was not found."),
        ResourceError::NotADirectory => (
            "resource_not_a_directory",
            "Resource target is not a directory.",
        ),
        ResourceError::NotAFile => ("resource_not_a_file", "Resource target is not a file."),
        ResourceError::LimitExceeded => (
            "resource_limit_exceeded",
            "Resource operation exceeded a bound.",
        ),
        ResourceError::InternalError => (
            "resource_unavailable",
            "Resource storage is temporarily unavailable.",
        ),
    };
    CommandError {
        code,
        message: message.to_owned(),
    }
}

fn resolve_capability(
    state: &AppState,
    project_id: &str,
) -> Result<ProjectResourceCapability, CommandError> {
    let project = state.require_registered_project(project_id)?;
    ProjectResourceCapability::new(project.id, &project.root_path).map_err(map_resource_error)
}

#[tauri::command]
pub fn list_project_resources(
    state: State<'_, AppState>,
    project_id: String,
    relative_path: Option<String>,
) -> Result<Vec<ResourceEntryDto>, CommandError> {
    state.list_project_resources(project_id, relative_path)
}

#[tauri::command]
pub fn read_project_resource(
    state: State<'_, AppState>,
    project_id: String,
    relative_path: String,
) -> Result<ResourceFileDto, CommandError> {
    state.read_project_resource(project_id, relative_path)
}

#[tauri::command]
pub fn search_project_resources(
    state: State<'_, AppState>,
    project_id: String,
    query: String,
    relative_path: Option<String>,
) -> Result<Vec<ResourceSearchResultDto>, CommandError> {
    state.search_project_resources(project_id, query, relative_path)
}

#[tauri::command]
pub fn list_recent_project_resources(
    state: State<'_, AppState>,
    project_id: String,
) -> Result<Vec<RecentResourceDto>, CommandError> {
    state.list_recent_project_resources(project_id)
}

#[tauri::command]
pub fn preview_project_resource(
    state: State<'_, AppState>,
    project_id: String,
    relative_path: String,
) -> Result<ResourcePreviewDto, CommandError> {
    state.preview_project_resource(project_id, relative_path)
}

impl AppState {
    /// Resolve a registered project by id only — never fall back to cwd or last-opened.
    pub fn require_registered_project(
        &self,
        project_id: &str,
    ) -> Result<keeper_project::Project, CommandError> {
        self.get_registered_project(project_id)
    }

    pub fn list_project_resources(
        &self,
        project_id: String,
        relative_path: Option<String>,
    ) -> Result<Vec<ResourceEntryDto>, CommandError> {
        let capability = resolve_capability(self, &project_id)?;
        let path = relative_path.unwrap_or_else(|| ".".to_owned());
        let directory = ResourceReader
            .list(
                &capability,
                ResourceListRequest {
                    path,
                    max_entries: MAX_LIST_ENTRIES,
                },
            )
            .map_err(map_resource_error)?;
        Ok(directory
            .entries
            .into_iter()
            .map(ResourceEntryDto::from)
            .collect())
    }

    pub fn read_project_resource(
        &self,
        project_id: String,
        relative_path: String,
    ) -> Result<ResourceFileDto, CommandError> {
        let capability = resolve_capability(self, &project_id)?;
        let file = ResourceReader
            .read(
                &capability,
                ResourceReadRequest {
                    path: relative_path,
                    max_bytes: MAX_READ_BYTES,
                },
            )
            .map_err(map_resource_error)?;
        let dto = map_resource_file(file);
        // Successful reads update only this project's recent list.
        self.record_recent_resource(&project_id, &dto.path)?;
        Ok(dto)
    }

    pub fn search_project_resources(
        &self,
        project_id: String,
        query: String,
        relative_path: Option<String>,
    ) -> Result<Vec<ResourceSearchResultDto>, CommandError> {
        let capability = resolve_capability(self, &project_id)?;
        let path = relative_path.unwrap_or_else(|| ".".to_owned());
        let results = ResourceReader
            .search(
                &capability,
                ResourceSearchRequest {
                    path,
                    query,
                    case_sensitive: false,
                    max_results: MAX_SEARCH_RESULTS,
                },
            )
            .map_err(map_resource_error)?;
        Ok(results
            .matches
            .into_iter()
            .map(ResourceSearchResultDto::from)
            .collect())
    }

    pub fn list_recent_project_resources(
        &self,
        project_id: String,
    ) -> Result<Vec<RecentResourceDto>, CommandError> {
        // Fail closed on unknown project before touching recents.
        let _project = self.require_registered_project(&project_id)?;
        let recents = self
            .list_resource_recents(&project_id, MAX_RECENT_LIST)
            .map_err(map_recent_error)?;
        Ok(recents.into_iter().map(RecentResourceDto::from).collect())
    }

    /// Bounded preview: classify UTF-8 / allow-listed images / binary metadata only.
    /// Raw `bytes` never leave the host DTO.
    pub fn preview_project_resource(
        &self,
        project_id: String,
        relative_path: String,
    ) -> Result<ResourcePreviewDto, CommandError> {
        let capability = resolve_capability(self, &project_id)?;
        let file = ResourceReader
            .read(
                &capability,
                ResourceReadRequest {
                    path: relative_path,
                    max_bytes: MAX_READ_BYTES,
                },
            )
            .map_err(map_resource_error)?;
        let dto = map_resource_preview(file);
        // Successful previews update only this project's recent list.
        self.record_recent_resource(&project_id, &dto.path)?;
        Ok(dto)
    }

    fn record_recent_resource(&self, project_id: &str, path: &str) -> Result<(), CommandError> {
        self.record_resource_recent(project_id, path)
            .map_err(map_recent_error)
    }
}

fn map_recent_error(error: keeper_storage_sqlite::ResourceRecentError) -> CommandError {
    use keeper_storage_sqlite::ResourceRecentError;
    match error {
        ResourceRecentError::InvalidProjectId => CommandError {
            code: "project_not_found",
            message: "Project not found.".to_owned(),
        },
        ResourceRecentError::InvalidPath => CommandError {
            code: "resource_path_rejected",
            message: "Resource path was rejected.".to_owned(),
        },
        ResourceRecentError::Sqlite | ResourceRecentError::LockPoisoned => CommandError {
            code: "resource_unavailable",
            message: "Resource storage is temporarily unavailable.".to_owned(),
        },
    }
}
