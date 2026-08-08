//! Portable project-relative path grammar (single source of truth).
//!
//! Used by Git layers (via `keeper-git-executor` re-export), resource core,
//! and mirrored by the TypeScript DTO checks. Directory form (`vendor/`) is
//! only valid for status *entry* paths; repository ids and diff targets stay
//! file-form via [`is_portable_path`].
//!
//! Device-name policy is **strict**: ASCII `COM0`–`COM9` / `LPT0`–`LPT9` plus
//! Windows superscript device stems `COM¹`–`COM³` / `LPT¹`–`LPT³` (U+00B9/B2/B3).

/// Maximum full path length (`str::len` bytes).
pub const PORTABLE_PATH_MAX_LEN: usize = 4_096;

/// Maximum single path component length.
pub const PORTABLE_COMPONENT_MAX_LEN: usize = 255;

/// Reject detail for callers that map onto typed errors (`ParseError`, etc.).
pub type PathRejectDetail = &'static str;

/// Accept only relative portable paths (forward slashes, no traversal/devices).
///
/// Repository ids and diff targets use this form. The repository root id `.`
/// is accepted; a trailing directory separator is not.
pub fn validate_portable_path(path: &str) -> Result<(), PathRejectDetail> {
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
pub fn validate_portable_path_or_directory(path: &str) -> Result<(), PathRejectDetail> {
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

/// Windows reserved device names (stem before first `.`).
///
/// Includes:
/// - `CON`, `PRN`, `AUX`, `NUL`, `CLOCK$`, `CONIN$`, `CONOUT$`
/// - `COM0`–`COM9` / `LPT0`–`LPT9` (ASCII digits)
/// - `COM¹` / `COM²` / `COM³` / `LPT¹` / `LPT²` / `LPT³` (ISO-8859-1
///   superscripts U+00B9/B2/B3 — treated as digits by Windows device parsing)
///
/// `COM0`/`LPT0` are rejected under a fail-closed policy: Microsoft Learn
/// "Naming Files, Paths, and Namespaces" lists COM1–COM9 and the superscript
/// forms but omits COM0/LPT0; secondary sources and Microsoft 365 invalid-name
/// tables include COM0–COM9 / LPT0–LPT9. Cost of rejecting an odd filename is
/// far lower than accepting a device alias.
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
    if bytes.len() == 4 && matches!(&bytes[..3], b"COM" | b"LPT") && bytes[3].is_ascii_digit() {
        return true;
    }
    matches!(
        stem.as_str(),
        "COM\u{00b9}"
            | "COM\u{00b2}"
            | "COM\u{00b3}"
            | "LPT\u{00b9}"
            | "LPT\u{00b2}"
            | "LPT\u{00b3}"
    )
}

#[cfg(test)]
mod tests {
    use super::{is_portable_entry_path, is_portable_path, is_windows_device_component};

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
    fn rejects_ascii_com_lpt_including_zero() {
        for name in [
            "COM0", "COM1", "COM9", "LPT0", "LPT1", "LPT9", "com0", "lpt0.txt",
        ] {
            assert!(
                is_windows_device_component(name),
                "expected device reject for {name}"
            );
            assert!(!is_portable_path(name), "file form must reject {name}");
        }
    }

    #[test]
    fn rejects_superscript_com_lpt_device_stems() {
        // U+00B9 ¹, U+00B2 ², U+00B3 ³ — Windows device digit aliases.
        let cases = [
            "COM\u{00b9}",
            "COM\u{00b2}",
            "COM\u{00b3}",
            "LPT\u{00b9}",
            "LPT\u{00b2}",
            "LPT\u{00b3}",
            "com\u{00b9}.txt",
            "src/LPT\u{00b2}",
        ];
        for name in cases {
            assert!(
                !is_portable_path(name),
                "file form must reject superscript device {name:?}"
            );
            assert!(
                !is_portable_entry_path(name),
                "entry form must reject superscript device {name:?}"
            );
        }
        // Directory form: stem still runs through the same device check.
        assert!(!is_portable_entry_path("COM\u{00b9}/"));
    }

    #[test]
    fn directory_form_is_entry_only() {
        assert!(!is_portable_path("vendor/"));
        assert!(is_portable_entry_path("vendor/"));
    }
}
