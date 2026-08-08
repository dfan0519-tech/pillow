//! Portable path surface for Git layers.
//!
//! Grammar lives in [`keeper_project`]. This module re-exports the public API
//! and keeps the crate-local `PathRejectDetail` alias so porcelain / runner
//! mapping to [`crate::status::ParseError`] stays unchanged.

/// Maximum full path length (`str::len` bytes).
pub use keeper_project::PORTABLE_PATH_MAX_LEN;

/// Maximum single path component length.
pub use keeper_project::PORTABLE_COMPONENT_MAX_LEN;

/// Reject detail for internal mapping to parse / read errors.
pub(crate) type PathRejectDetail = keeper_project::PathRejectDetail;

/// Accept only relative portable paths (forward slashes, no traversal/devices).
///
/// Repository ids and diff targets use this form. The repository root id `.`
/// is accepted; a trailing directory separator is not.
pub(crate) fn validate_portable_path(path: &str) -> Result<(), PathRejectDetail> {
    keeper_project::validate_portable_path(path)
}

/// Portable-path rules plus the single trailing separator porcelain v2 uses for
/// a collapsed untracked/ignored directory (`vendor/`).
///
/// Every layer that handles a status entry path must use this rather than
/// [`validate_portable_path`]: the parser produces the directory form, so a
/// stricter downstream check would reject an entry the parser just accepted and
/// fail the whole repository status. Containment is unchanged — the stem runs
/// through the identical rules.
pub(crate) fn validate_portable_path_or_directory(path: &str) -> Result<(), PathRejectDetail> {
    keeper_project::validate_portable_path_or_directory(path)
}

/// Public bool form of [`validate_portable_path`] for command gates and tests.
pub fn is_portable_path(path: &str) -> bool {
    keeper_project::is_portable_path(path)
}

/// Public bool form of [`validate_portable_path_or_directory`].
pub fn is_portable_entry_path(path: &str) -> bool {
    keeper_project::is_portable_entry_path(path)
}

#[cfg(test)]
mod tests {
    use super::{is_portable_entry_path, is_portable_path};

    #[test]
    fn file_form_accepts_repo_id_and_nested_files() {
        assert!(is_portable_path("."));
        assert!(is_portable_path("a.txt"));
        assert!(is_portable_path("src/lib.rs"));
    }

    #[test]
    fn file_form_rejects_directory_and_traversal() {
        assert!(!is_portable_path("vendor/"));
        assert!(!is_portable_path("../escape"));
        assert!(!is_portable_path("a\\b"));
        assert!(!is_portable_path("CON"));
    }

    #[test]
    fn entry_form_accepts_collapsed_directory() {
        assert!(is_portable_entry_path("vendor/"));
        assert!(is_portable_entry_path("a.txt"));
        assert!(!is_portable_entry_path("vendor//"));
        assert!(!is_portable_entry_path("./"));
    }

    #[test]
    fn rejects_superscript_device_names() {
        for name in [
            "COM\u{00b9}",
            "COM\u{00b2}",
            "COM\u{00b3}",
            "LPT\u{00b9}",
            "LPT\u{00b2}",
            "LPT\u{00b3}",
        ] {
            assert!(!is_portable_path(name), "git-executor must reject {name}");
            assert!(
                !is_portable_entry_path(name),
                "git-executor entry form must reject {name}"
            );
        }
    }

    #[test]
    fn rejects_com0_lpt0() {
        assert!(!is_portable_path("COM0"));
        assert!(!is_portable_path("LPT0"));
    }
}
