//! Path allowlist for absolute filesystem access granted by the host.
//! Adapted from Grok App path_scope under MIT.
//! Upstream: src-tauri/src/path_scope.rs at
//! cbbbdf17bb8db1588b6707f3f42f6b00397e27f9.

use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};

fn grants() -> &'static Mutex<Vec<PathBuf>> {
    static GRANTS: OnceLock<Mutex<Vec<PathBuf>>> = OnceLock::new();
    GRANTS.get_or_init(|| Mutex::new(Vec::new()))
}

/// Grant a one-off absolute path (e.g. user-picked folder).
/// Parent directory of a file is granted so re-reads of the same file work.
pub fn grant_path(path: &Path) {
    let canonical = path.canonicalize().unwrap_or_else(|_| path.to_path_buf());
    let grant = if canonical.is_file() {
        canonical
            .parent()
            .map(|parent| parent.to_path_buf())
            .unwrap_or(canonical)
    } else {
        canonical
    };

    let Ok(mut locked) = grants().lock() else {
        return;
    };
    if locked.iter().any(|existing| existing == &grant) {
        return;
    }
    locked.push(grant);
    const MAX_GRANTS: usize = 256;
    if locked.len() > MAX_GRANTS {
        let drain = locked.len() - MAX_GRANTS;
        locked.drain(0..drain);
    }
}

/// True when `path` sits under a granted root (after canonicalize when possible).
pub fn is_allowed(path: &Path) -> bool {
    let candidate = path.canonicalize().unwrap_or_else(|_| path.to_path_buf());
    let Ok(locked) = grants().lock() else {
        return false;
    };
    locked.iter().any(|root| path_under_root(&candidate, root))
}

/// Reject when `path` is outside the allowlist (used by write-capable create paths).
pub fn require_allowed(path: &Path) -> Result<(), PathScopeError> {
    if is_allowed(path) {
        return Ok(());
    }
    Err(PathScopeError::NotAllowed)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PathScopeError {
    NotAllowed,
}

impl PathScopeError {
    pub const fn code(self) -> &'static str {
        match self {
            Self::NotAllowed => "path_not_allowed",
        }
    }

    pub const fn message(self) -> &'static str {
        match self {
            Self::NotAllowed => "The selected folder is outside the granted path allowlist.",
        }
    }
}

fn path_under_root(path: &Path, root: &Path) -> bool {
    if path == root {
        return true;
    }
    let mut path_comps = path.components();
    for root_comp in root.components() {
        match path_comps.next() {
            Some(path_comp) if path_comp == root_comp => {}
            _ => return false,
        }
    }
    true
}

#[cfg(test)]
pub fn clear_grants_for_tests() {
    if let Ok(mut locked) = grants().lock() {
        locked.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn temp_dir(label: &str) -> PathBuf {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock")
            .as_nanos();
        let root = std::env::temp_dir().join(format!(
            "keeper-ide-path-scope-{label}-{}-{nonce}",
            std::process::id()
        ));
        fs::create_dir_all(&root).expect("temp");
        root
    }

    #[test]
    fn grants_allow_the_folder_and_its_children() {
        clear_grants_for_tests();
        let root = temp_dir("grant");
        let child = root.join("nested");
        fs::create_dir_all(&child).unwrap();
        grant_path(&root);
        assert!(is_allowed(&root));
        assert!(is_allowed(&child));
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn ungranted_paths_are_not_allowed() {
        clear_grants_for_tests();
        let root = temp_dir("deny");
        assert!(!is_allowed(&root));
        assert_eq!(
            require_allowed(&root).unwrap_err(),
            PathScopeError::NotAllowed
        );
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn require_allowed_accepts_granted_parent() {
        clear_grants_for_tests();
        let root = temp_dir("require-ok");
        grant_path(&root);
        assert!(require_allowed(&root).is_ok());
        let _ = fs::remove_dir_all(&root);
    }
}
