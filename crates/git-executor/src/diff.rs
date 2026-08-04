//! Bounded one-file Git diff views: text, binary, or oversized summaries.

use crate::status::{GitRepositoryId, ParseError, validate_portable_path};

/// Maximum bytes accepted as a single text diff body (256 KiB).
pub const DIFF_TEXT_BYTE_CAP: usize = 256 * 1024;

/// Typed view of one path's diff. Binary and oversized cases never return
/// partial text content.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GitDiffView {
    Text {
        path: String,
        content: String,
        byte_count: usize,
    },
    Binary {
        path: String,
        size_bytes: usize,
    },
    Oversized {
        path: String,
        size_bytes: usize,
        limit_bytes: usize,
    },
}

/// Generation-bearing diff snapshot for native/renderer stale rejection.
///
/// Every successful `GitReadExecutor::diff` response carries the repository
/// identity and the exact caller-supplied generation so later layers can drop
/// stale results without re-parsing content.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GitDiffSnapshot {
    pub repository: GitRepositoryId,
    pub generation: u64,
    pub view: GitDiffView,
}

/// Classify and bound a single Git diff payload for one portable path.
///
/// - NUL byte anywhere ⇒ [`GitDiffView::Binary`] (no text body).
/// - Length above [`DIFF_TEXT_BYTE_CAP`] ⇒ [`GitDiffView::Oversized`] (no partial text).
/// - Otherwise UTF-8 text within the cap ⇒ [`GitDiffView::Text`].
pub fn bound_git_diff(path: &str, raw: &[u8]) -> Result<GitDiffView, ParseError> {
    validate_portable_path(path)?;
    let path = path.to_string();
    let size_bytes = raw.len();

    if raw.contains(&0) {
        return Ok(GitDiffView::Binary { path, size_bytes });
    }

    if size_bytes > DIFF_TEXT_BYTE_CAP {
        return Ok(GitDiffView::Oversized {
            path,
            size_bytes,
            limit_bytes: DIFF_TEXT_BYTE_CAP,
        });
    }

    let content = std::str::from_utf8(raw)
        .map_err(|_| ParseError::Utf8)?
        .to_string();
    Ok(GitDiffView::Text {
        path,
        content,
        byte_count: size_bytes,
    })
}
