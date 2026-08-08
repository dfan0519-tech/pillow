//! Single source of truth for portable project-relative path grammar.
//!
//! All Git layers (porcelain parse, discovery mapping, command gate, and the
//! mirrored TypeScript DTO checks) must use this rule set. Directory form
//! (`vendor/`) is only valid for status *entry* paths; repository ids and
//! diff targets stay file-form via [`is_portable_path`].

/// Maximum full path length (`str::len` bytes).
pub const PORTABLE_PATH_MAX_LEN: usize = 4_096;

/// Maximum single path component length.
pub const PORTABLE_COMPONENT_MAX_LEN: usize = 255;

/// Reject detail for internal mapping to parse / read errors.
pub(crate) type PathRejectDetail = &'static str;

/// Accept only relative portable paths (forward slashes, no traversal/devices).
///
/// Repository ids and diff targets use this form. The repository root id `.`
/// is accepted; a trailing directory separator is not.
pub(crate) fn validate_portable_path(path: &str) -> Result<(), PathRejectDetail> {
    if path == "." {
        return Ok(());
    }
    if path.is_empty() || path.len() > PORTABLE_PATH_MAX_LEN {
        return Err("empty or too long");
    }
    if path.starts_with('/') || path.starts_with('\\') || path.contains('\\') || path.contains(':')
    {
        return Err("absolute or non-portable separator");
    }
    for component in path.split('/') {
        if component.is_empty() || component == "." || component == ".." {
            return Err("empty or traversal component");
        }
        if component.len() > PORTABLE_COMPONENT_MAX_LEN {
            return Err("component too long");
        }
        if component.ends_with('.') || component.ends_with(' ') {
            return Err("trailing dot or space");
        }
        if component
            .chars()
            .any(|c| c.is_control() || matches!(c, '<' | '>' | '"' | '|' | '?' | '*'))
        {
            return Err("illegal character");
        }
        if is_windows_device_component(component) {
            return Err("windows device name");
        }
    }
    Ok(())
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
    let Some(stem) = path.strip_suffix('/') else {
        return validate_portable_path(path);
    };
    // `/`, `vendor//` and `./` are never emitted by git and stay rejected.
    if stem.is_empty() || stem == "." || stem.ends_with('/') {
        return Err("empty or repeated trailing separator");
    }
    validate_portable_path(stem)
}

/// Public bool form of [`validate_portable_path`] for command gates and tests.
pub fn is_portable_path(path: &str) -> bool {
    validate_portable_path(path).is_ok()
}

/// Public bool form of [`validate_portable_path_or_directory`].
pub fn is_portable_entry_path(path: &str) -> bool {
    validate_portable_path_or_directory(path).is_ok()
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
    bytes.len() == 4 && matches!(&bytes[..3], b"COM" | b"LPT") && matches!(bytes[3], b'1'..=b'9')
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
}
