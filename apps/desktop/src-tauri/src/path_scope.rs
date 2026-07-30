//! Project-scoped canonical path capabilities minted by the native host.
//! Adapted from Grok App path_scope under MIT.
//! Upstream: src-tauri/src/path_scope.rs at
//! cbbbdf17bb8db1588b6707f3f42f6b00397e27f9.

use std::collections::HashMap;
#[cfg(windows)]
use std::path::Prefix;
use std::path::{Component, Path, PathBuf};
use std::sync::Mutex;
use std::time::{Duration, Instant};

use keeper_project::ProjectId;
#[cfg(windows)]
use std::os::windows::fs::MetadataExt;

#[derive(Default)]
struct CapabilityState {
    picker_grants: Vec<PickerGrant>,
    project_grants: HashMap<(String, ProjectId), PathBuf>,
}

struct PickerGrant {
    window_id: String,
    canonical_root: PathBuf,
    expires_at: Instant,
}

pub(crate) struct PickerSelection {
    window_id: String,
    canonical_root: PathBuf,
}

impl PickerSelection {
    pub(crate) fn canonical_root(&self) -> &Path {
        &self.canonical_root
    }
}

pub struct PathCapabilities {
    state: Mutex<CapabilityState>,
    picker_ttl: Duration,
}

impl Default for PathCapabilities {
    fn default() -> Self {
        Self {
            state: Mutex::new(CapabilityState::default()),
            picker_ttl: Duration::from_secs(5 * 60),
        }
    }
}

impl PathCapabilities {
    #[cfg(test)]
    fn with_picker_ttl(picker_ttl: Duration) -> Self {
        Self {
            state: Mutex::new(CapabilityState::default()),
            picker_ttl,
        }
    }

    pub(crate) fn register_picker_selection(
        &self,
        window_id: &str,
        path: &Path,
    ) -> Result<PathBuf, PathScopeError> {
        validate_picker_path(path)?;
        let canonical_root = canonical_directory(path)?;
        let mut state = self.state.lock().map_err(|_| PathScopeError::NotAllowed)?;
        let now = Instant::now();
        state.picker_grants.retain(|grant| grant.expires_at > now);
        state
            .picker_grants
            .retain(|grant| grant.window_id != window_id);
        state.picker_grants.push(PickerGrant {
            window_id: window_id.to_owned(),
            canonical_root: canonical_root.clone(),
            expires_at: now + self.picker_ttl,
        });
        const MAX_PICKER_GRANTS: usize = 64;
        if state.picker_grants.len() > MAX_PICKER_GRANTS {
            let drain = state.picker_grants.len() - MAX_PICKER_GRANTS;
            state.picker_grants.drain(0..drain);
        }
        Ok(canonical_root)
    }

    pub(crate) fn consume_picker_selection(
        &self,
        window_id: &str,
        path: &Path,
    ) -> Result<PickerSelection, PathScopeError> {
        validate_picker_path(path)?;
        let canonical_root = canonical_directory(path)?;
        let mut state = self.state.lock().map_err(|_| PathScopeError::NotAllowed)?;
        let now = Instant::now();
        state.picker_grants.retain(|grant| grant.expires_at > now);
        let Some(grant_index) = state.picker_grants.iter().position(|grant| {
            grant.window_id == window_id && grant.canonical_root == canonical_root
        }) else {
            return Err(PathScopeError::NotAllowed);
        };
        state.picker_grants.remove(grant_index);
        Ok(PickerSelection {
            window_id: window_id.to_owned(),
            canonical_root,
        })
    }

    pub(crate) fn bind_project(
        &self,
        selection: PickerSelection,
        project_id: ProjectId,
        expected_root: &Path,
    ) -> Result<(), PathScopeError> {
        let canonical_root = canonical_directory(expected_root)?;
        if selection.canonical_root != canonical_root {
            return Err(PathScopeError::NotAllowed);
        }
        let mut state = self.state.lock().map_err(|_| PathScopeError::NotAllowed)?;
        state
            .project_grants
            .retain(|(window_id, _), _| window_id != &selection.window_id);
        state
            .project_grants
            .insert((selection.window_id, project_id), canonical_root);
        Ok(())
    }

    pub(crate) fn derive_child_selection(
        &self,
        selection: PickerSelection,
        child: &Path,
    ) -> Result<PathBuf, PathScopeError> {
        #[cfg(windows)]
        reject_reparse_components(child)?;
        let canonical_child = canonical_directory(child)?;
        if canonical_child.parent() != Some(selection.canonical_root.as_path()) {
            return Err(PathScopeError::NotAllowed);
        }
        let mut state = self.state.lock().map_err(|_| PathScopeError::NotAllowed)?;
        state
            .picker_grants
            .retain(|grant| grant.window_id != selection.window_id);
        state.picker_grants.push(PickerGrant {
            window_id: selection.window_id,
            canonical_root: canonical_child.clone(),
            expires_at: Instant::now() + self.picker_ttl,
        });
        Ok(canonical_child)
    }

    #[cfg(test)]
    fn bind_picker_selection(
        &self,
        window_id: &str,
        project_id: ProjectId,
        expected_root: &Path,
    ) -> Result<(), PathScopeError> {
        let selection = self.consume_picker_selection(window_id, expected_root)?;
        self.bind_project(selection, project_id, expected_root)
    }

    pub(crate) fn require_project_root(
        &self,
        window_id: &str,
        project_id: ProjectId,
        expected_root: &Path,
    ) -> Result<PathBuf, PathScopeError> {
        let canonical_root = canonical_directory(expected_root)?;
        let state = self.state.lock().map_err(|_| PathScopeError::NotAllowed)?;
        match state
            .project_grants
            .get(&(window_id.to_owned(), project_id))
        {
            Some(granted_root) if granted_root == &canonical_root => Ok(canonical_root),
            _ => Err(PathScopeError::NotAllowed),
        }
    }

    pub(crate) fn activate_registered_project(
        &self,
        window_id: &str,
        project_id: ProjectId,
        expected_root: &Path,
    ) -> Result<PathBuf, PathScopeError> {
        #[cfg(windows)]
        reject_reparse_components(expected_root)?;
        let canonical_root = canonical_directory(expected_root)?;
        let mut state = self.state.lock().map_err(|_| PathScopeError::NotAllowed)?;
        state
            .project_grants
            .retain(|(grant_window, _), _| grant_window != window_id);
        state
            .project_grants
            .insert((window_id.to_owned(), project_id), canonical_root.clone());
        Ok(canonical_root)
    }
}

fn validate_picker_path(path: &Path) -> Result<(), PathScopeError> {
    if !path.is_absolute() {
        return Err(PathScopeError::NotAllowed);
    }
    if path
        .components()
        .any(|component| component == Component::ParentDir)
    {
        return Err(PathScopeError::NotAllowed);
    }
    #[cfg(windows)]
    {
        let mut components = path.components();
        match components.next() {
            Some(Component::Prefix(prefix)) if matches!(prefix.kind(), Prefix::Disk(_)) => {}
            _ => return Err(PathScopeError::NotAllowed),
        }
        if components.any(|component| {
            matches!(
                component,
                Component::Normal(value) if value.to_string_lossy().contains(':')
            )
        }) {
            return Err(PathScopeError::NotAllowed);
        }
        reject_reparse_components(path)?;
    }
    Ok(())
}

#[cfg(windows)]
fn reject_reparse_components(path: &Path) -> Result<(), PathScopeError> {
    const FILE_ATTRIBUTE_REPARSE_POINT: u32 = 0x400;

    let mut current = PathBuf::new();
    for component in path.components() {
        current.push(component.as_os_str());
        if matches!(component, Component::Prefix(_) | Component::RootDir) {
            continue;
        }
        let metadata =
            std::fs::symlink_metadata(&current).map_err(|_| PathScopeError::NotAllowed)?;
        if metadata.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0 {
            return Err(PathScopeError::NotAllowed);
        }
    }
    Ok(())
}

fn canonical_directory(path: &Path) -> Result<PathBuf, PathScopeError> {
    let canonical = path
        .canonicalize()
        .map_err(|_| PathScopeError::NotAllowed)?;
    if !canonical.is_dir() {
        return Err(PathScopeError::NotAllowed);
    }
    #[cfg(windows)]
    return normalize_windows_canonical(canonical);
    #[cfg(not(windows))]
    Ok(canonical)
}

#[cfg(windows)]
fn normalize_windows_canonical(canonical: PathBuf) -> Result<PathBuf, PathScopeError> {
    let mut components = canonical.components();
    let Some(Component::Prefix(prefix)) = components.next() else {
        return Err(PathScopeError::NotAllowed);
    };
    match prefix.kind() {
        Prefix::Disk(_) => Ok(canonical),
        Prefix::VerbatimDisk(drive) => {
            let mut normalized = PathBuf::from(format!("{}:\\", char::from(drive)));
            for component in components {
                if component != Component::RootDir {
                    normalized.push(component.as_os_str());
                }
            }
            Ok(normalized)
        }
        _ => Err(PathScopeError::NotAllowed),
    }
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
            Self::NotAllowed => {
                "The selected folder is not authorized for this project and window."
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use keeper_project::ProjectId;
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
    fn project_capability_rejects_cross_project_lookup() {
        let root = temp_dir("cross-project");
        let capabilities = PathCapabilities::default();
        let project_a = ProjectId::new();
        let project_b = ProjectId::new();

        let canonical = capabilities
            .register_picker_selection("window-a", &root)
            .expect("native picker selection should be accepted");
        capabilities
            .bind_picker_selection("window-a", project_a, &root)
            .expect("picker grant should bind to the owning project");

        assert_eq!(
            capabilities
                .require_project_root("window-a", project_b, &canonical)
                .unwrap_err(),
            PathScopeError::NotAllowed
        );

        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn picker_selection_rejects_parent_traversal_spelling() {
        let root = temp_dir("traversal");
        let nested = root.join("nested");
        fs::create_dir_all(&nested).expect("nested directory");
        let capabilities = PathCapabilities::default();

        assert_eq!(
            capabilities
                .register_picker_selection("window-a", &nested.join(".."))
                .unwrap_err(),
            PathScopeError::NotAllowed
        );

        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn expired_picker_selection_cannot_bind_project_authority() {
        let root = temp_dir("expired");
        let capabilities = PathCapabilities::with_picker_ttl(std::time::Duration::ZERO);
        capabilities
            .register_picker_selection("window-a", &root)
            .expect("picker selection");

        assert_eq!(
            capabilities
                .bind_picker_selection("window-a", ProjectId::new(), &root)
                .unwrap_err(),
            PathScopeError::NotAllowed
        );

        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn newer_picker_selection_revokes_prior_selection_in_same_window() {
        let first = temp_dir("picker-first");
        let second = temp_dir("picker-second");
        let capabilities = PathCapabilities::default();
        capabilities
            .register_picker_selection("window-a", &first)
            .expect("first picker selection");
        capabilities
            .register_picker_selection("window-a", &second)
            .expect("second picker selection");

        assert!(matches!(
            capabilities.consume_picker_selection("window-a", &first),
            Err(PathScopeError::NotAllowed)
        ));

        let _ = fs::remove_dir_all(&first);
        let _ = fs::remove_dir_all(&second);
    }

    #[test]
    fn binding_new_project_revokes_prior_project_in_same_window() {
        let first = temp_dir("project-first");
        let second = temp_dir("project-second");
        let capabilities = PathCapabilities::default();
        let project_a = ProjectId::new();
        let project_b = ProjectId::new();
        capabilities
            .register_picker_selection("window-a", &first)
            .expect("first picker");
        capabilities
            .bind_picker_selection("window-a", project_a, &first)
            .expect("first project");
        capabilities
            .register_picker_selection("window-a", &second)
            .expect("second picker");
        capabilities
            .bind_picker_selection("window-a", project_b, &second)
            .expect("second project");

        assert_eq!(
            capabilities
                .require_project_root("window-a", project_a, &first)
                .unwrap_err(),
            PathScopeError::NotAllowed
        );

        let _ = fs::remove_dir_all(&first);
        let _ = fs::remove_dir_all(&second);
    }

    #[cfg(windows)]
    #[test]
    fn windows_escape_path_syntax_is_rejected_fail_closed() {
        let rejected = [
            r"\\server\share\project",
            r"\\?\C:\project",
            r"\\.\C:\project",
            r"C:\project:stream",
            r"C:\project\..\escape",
        ];

        for candidate in rejected {
            assert_eq!(
                validate_picker_path(Path::new(candidate)),
                Err(PathScopeError::NotAllowed),
                "{candidate} must be rejected"
            );
        }
    }

    #[cfg(windows)]
    #[test]
    fn picker_selection_rejects_reparse_point_components() {
        use std::os::windows::fs::symlink_dir;

        let root = temp_dir("reparse");
        let target = root.join("target");
        let link = root.join("link");
        fs::create_dir_all(&target).expect("target directory");
        symlink_dir(&target, &link).expect("directory symlink");
        let capabilities = PathCapabilities::default();

        assert_eq!(
            capabilities
                .register_picker_selection("window-a", &link)
                .unwrap_err(),
            PathScopeError::NotAllowed
        );

        let _ = fs::remove_dir_all(&root);
    }
}
