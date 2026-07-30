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
use keeper_session::ConversationId;
#[cfg(windows)]
use std::os::windows::fs::MetadataExt;
use uuid::Uuid;

#[derive(Default)]
struct CapabilityState {
    window_instances: HashMap<String, Uuid>,
    picker_grants: Vec<PickerGrant>,
    project_bootstraps: HashMap<Uuid, ProjectBootstrapGrant>,
    conversation_permits: HashMap<(Uuid, ConversationId), ProjectRootAuthority>,
    conversation_grants: HashMap<(Uuid, ConversationId), ProjectRootAuthority>,
}

struct PickerGrant {
    window_instance: Uuid,
    canonical_root: PathBuf,
    expires_at: Instant,
}

#[derive(Debug)]
pub(crate) struct PickerSelection {
    window_instance: Uuid,
    canonical_root: PathBuf,
    expires_at: Instant,
}

impl PickerSelection {
    pub(crate) fn canonical_root(&self) -> &Path {
        &self.canonical_root
    }
}

#[derive(Clone, Debug)]
struct ProjectRootAuthority {
    project_id: ProjectId,
    canonical_root: PathBuf,
}

#[derive(Debug)]
struct ProjectBootstrapGrant {
    authority: ProjectRootAuthority,
    nonce: Uuid,
}

#[derive(Debug)]
pub(crate) struct ConversationBootstrap {
    window_instance: Uuid,
    grant: ProjectBootstrapGrant,
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
        let mut state = self.lock_state();
        let now = Instant::now();
        let window_instance = window_instance(&mut state, window_id);
        state.picker_grants.retain(|grant| grant.expires_at > now);
        state
            .picker_grants
            .retain(|grant| grant.window_instance != window_instance);
        state.picker_grants.push(PickerGrant {
            window_instance,
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
        let mut state = self.lock_state();
        let now = Instant::now();
        let Some(window_instance) = state.window_instances.get(window_id).copied() else {
            return Err(PathScopeError::NotAllowed);
        };
        state.picker_grants.retain(|grant| grant.expires_at > now);
        let Some(grant_index) = state.picker_grants.iter().position(|grant| {
            grant.window_instance == window_instance && grant.canonical_root == canonical_root
        }) else {
            return Err(PathScopeError::NotAllowed);
        };
        let grant = state.picker_grants.remove(grant_index);
        Ok(PickerSelection {
            window_instance,
            canonical_root,
            expires_at: grant.expires_at,
        })
    }

    pub(crate) fn revoke_picker_selection(&self, window_id: &str) {
        let mut state = self.lock_state();
        let Some(window_instance) = state.window_instances.get(window_id).copied() else {
            return;
        };
        state
            .picker_grants
            .retain(|grant| grant.window_instance != window_instance);
    }

    pub(crate) fn restore_picker_selection(&self, selection: PickerSelection) {
        let mut state = self.lock_state();
        if !is_live_window_instance(&state, selection.window_instance)
            || selection.expires_at <= Instant::now()
        {
            return;
        }
        state
            .picker_grants
            .retain(|grant| grant.window_instance != selection.window_instance);
        state.picker_grants.push(PickerGrant {
            window_instance: selection.window_instance,
            canonical_root: selection.canonical_root,
            expires_at: selection.expires_at,
        });
    }

    pub(crate) fn bind_project(&self, selection: PickerSelection, project_id: ProjectId) {
        let mut state = self.lock_state();
        if !is_live_window_instance(&state, selection.window_instance) {
            return;
        }
        revoke_instance_authority(&mut state, selection.window_instance);
        state.project_bootstraps.insert(
            selection.window_instance,
            ProjectBootstrapGrant {
                authority: ProjectRootAuthority {
                    project_id,
                    canonical_root: selection.canonical_root,
                },
                nonce: Uuid::new_v4(),
            },
        );
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
        let mut state = self.lock_state();
        let window_instance = window_instance(&mut state, window_id);
        revoke_instance_authority(&mut state, window_instance);
        state.project_bootstraps.insert(
            window_instance,
            ProjectBootstrapGrant {
                authority: ProjectRootAuthority {
                    project_id,
                    canonical_root: canonical_root.clone(),
                },
                nonce: Uuid::new_v4(),
            },
        );
        Ok(canonical_root)
    }

    pub(crate) fn begin_conversation(
        &self,
        window_id: &str,
        project_id: ProjectId,
    ) -> Result<ConversationBootstrap, PathScopeError> {
        let mut state = self.lock_state();
        let Some(window_instance) = state.window_instances.get(window_id).copied() else {
            return Err(PathScopeError::NotAllowed);
        };
        let Some(grant) = state.project_bootstraps.remove(&window_instance) else {
            return Err(PathScopeError::NotAllowed);
        };
        if grant.authority.project_id != project_id {
            state.project_bootstraps.insert(window_instance, grant);
            return Err(PathScopeError::NotAllowed);
        }
        Ok(ConversationBootstrap {
            window_instance,
            grant,
        })
    }

    pub(crate) fn restore_conversation_bootstrap(&self, bootstrap: ConversationBootstrap) {
        let mut state = self.lock_state();
        if !is_live_window_instance(&state, bootstrap.window_instance) {
            return;
        }
        state
            .project_bootstraps
            .entry(bootstrap.window_instance)
            .or_insert(bootstrap.grant);
    }

    pub(crate) fn bind_new_conversation(
        &self,
        bootstrap: ConversationBootstrap,
        conversation_id: ConversationId,
    ) {
        let mut state = self.lock_state();
        if !is_live_window_instance(&state, bootstrap.window_instance) {
            return;
        }
        let _consumed_nonce = bootstrap.grant.nonce;
        let next_bootstrap = ProjectBootstrapGrant {
            authority: bootstrap.grant.authority.clone(),
            nonce: Uuid::new_v4(),
        };
        state.conversation_grants.insert(
            (bootstrap.window_instance, conversation_id),
            bootstrap.grant.authority,
        );
        state
            .project_bootstraps
            .insert(bootstrap.window_instance, next_bootstrap);
    }

    pub(crate) fn permit_existing_conversations(
        &self,
        window_id: &str,
        project_id: ProjectId,
        expected_root: &Path,
        conversation_ids: &[ConversationId],
    ) -> Result<(), PathScopeError> {
        let canonical_root = canonical_directory(expected_root)?;
        let mut state = self.lock_state();
        let Some(window_instance) = state.window_instances.get(window_id).copied() else {
            return Err(PathScopeError::NotAllowed);
        };
        let Some(bootstrap) = state.project_bootstraps.get(&window_instance) else {
            return Err(PathScopeError::NotAllowed);
        };
        if bootstrap.authority.project_id != project_id
            || bootstrap.authority.canonical_root != canonical_root
        {
            return Err(PathScopeError::NotAllowed);
        }
        let authority = bootstrap.authority.clone();
        for conversation_id in conversation_ids {
            state
                .conversation_permits
                .insert((window_instance, *conversation_id), authority.clone());
        }
        Ok(())
    }

    pub(crate) fn bind_existing_conversation(
        &self,
        window_id: &str,
        project_id: ProjectId,
        conversation_id: ConversationId,
        expected_root: &Path,
    ) -> Result<(), PathScopeError> {
        let canonical_root = canonical_directory(expected_root)?;
        let mut state = self.lock_state();
        let Some(window_instance) = state.window_instances.get(window_id).copied() else {
            return Err(PathScopeError::NotAllowed);
        };
        if matches!(
            state
                .conversation_grants
                .get(&(window_instance, conversation_id)),
            Some(authority)
                if authority.project_id == project_id
                    && authority.canonical_root == canonical_root
        ) {
            return Ok(());
        }
        let Some(authority) = state
            .conversation_permits
            .remove(&(window_instance, conversation_id))
        else {
            return Err(PathScopeError::NotAllowed);
        };
        if authority.project_id != project_id || authority.canonical_root != canonical_root {
            return Err(PathScopeError::NotAllowed);
        }
        state
            .conversation_grants
            .insert((window_instance, conversation_id), authority);
        Ok(())
    }

    pub(crate) fn require_conversation_root(
        &self,
        window_id: &str,
        project_id: ProjectId,
        conversation_id: ConversationId,
        expected_root: &Path,
    ) -> Result<PathBuf, PathScopeError> {
        let canonical_root = canonical_directory(expected_root)?;
        let state = self.lock_state();
        let Some(window_instance) = state.window_instances.get(window_id).copied() else {
            return Err(PathScopeError::NotAllowed);
        };
        match state
            .conversation_grants
            .get(&(window_instance, conversation_id))
        {
            Some(authority)
                if authority.project_id == project_id
                    && authority.canonical_root == canonical_root =>
            {
                Ok(canonical_root)
            }
            _ => Err(PathScopeError::NotAllowed),
        }
    }

    pub(crate) fn revoke_window(&self, window_id: &str) {
        let mut state = self.lock_state();
        let Some(window_instance) = state.window_instances.remove(window_id) else {
            return;
        };
        state
            .picker_grants
            .retain(|grant| grant.window_instance != window_instance);
        revoke_instance_authority(&mut state, window_instance);
    }

    fn lock_state(&self) -> std::sync::MutexGuard<'_, CapabilityState> {
        self.state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }
}

fn window_instance(state: &mut CapabilityState, window_id: &str) -> Uuid {
    *state
        .window_instances
        .entry(window_id.to_owned())
        .or_insert_with(Uuid::new_v4)
}

fn is_live_window_instance(state: &CapabilityState, instance: Uuid) -> bool {
    state
        .window_instances
        .values()
        .any(|candidate| *candidate == instance)
}

fn revoke_instance_conversations(state: &mut CapabilityState, instance: Uuid) {
    state
        .conversation_grants
        .retain(|(window_instance, _), _| *window_instance != instance);
}

fn revoke_instance_authority(state: &mut CapabilityState, instance: Uuid) {
    state.project_bootstraps.remove(&instance);
    state
        .conversation_permits
        .retain(|(window_instance, _), _| *window_instance != instance);
    revoke_instance_conversations(state, instance);
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
    use keeper_session::ConversationId;
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

    fn bind_project(
        capabilities: &PathCapabilities,
        window_id: &str,
        project_id: ProjectId,
        root: &Path,
    ) {
        let selection = capabilities
            .consume_picker_selection(window_id, root)
            .expect("picker selection");
        capabilities.bind_project(selection, project_id);
    }

    #[test]
    fn conversation_capability_rejects_cross_project_lookup() {
        let root = temp_dir("cross-project");
        let capabilities = PathCapabilities::default();
        let project_a = ProjectId::new();
        let project_b = ProjectId::new();
        let conversation_id = ConversationId::new();

        capabilities
            .register_picker_selection("window-a", &root)
            .expect("native picker selection should be accepted");
        bind_project(&capabilities, "window-a", project_a, &root);
        let bootstrap = capabilities
            .begin_conversation("window-a", project_a)
            .expect("conversation bootstrap");
        capabilities.bind_new_conversation(bootstrap, conversation_id);

        assert_eq!(
            capabilities
                .require_conversation_root("window-a", project_b, conversation_id, &root)
                .unwrap_err(),
            PathScopeError::NotAllowed
        );

        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn project_bootstrap_is_consumed_into_one_conversation() {
        let root = temp_dir("conversation-bound");
        let capabilities = PathCapabilities::default();
        let project_id = ProjectId::new();
        let first_conversation = ConversationId::new();
        let second_conversation = ConversationId::new();

        capabilities
            .register_picker_selection("window-a", &root)
            .expect("native picker selection should be accepted");
        bind_project(&capabilities, "window-a", project_id, &root);
        let bootstrap = capabilities
            .begin_conversation("window-a", project_id)
            .expect("project bootstrap should be consumable once");
        let first_nonce = bootstrap.grant.nonce;
        capabilities.bind_new_conversation(bootstrap, first_conversation);

        assert!(
            capabilities
                .require_conversation_root("window-a", project_id, first_conversation, &root)
                .is_ok()
        );
        assert_eq!(
            capabilities
                .require_conversation_root("window-a", project_id, second_conversation, &root)
                .unwrap_err(),
            PathScopeError::NotAllowed
        );
        let second_bootstrap = capabilities
            .begin_conversation("window-a", project_id)
            .expect("the host should mint a fresh bootstrap for another conversation");
        assert_ne!(
            first_nonce, second_bootstrap.grant.nonce,
            "a consumed bootstrap nonce must never be reused"
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
                .consume_picker_selection("window-a", &root)
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
    fn opening_or_cancelling_a_new_picker_revokes_the_prior_selection() {
        let root = temp_dir("picker-cancel");
        let capabilities = PathCapabilities::default();
        capabilities
            .register_picker_selection("window-a", &root)
            .expect("picker selection");

        capabilities.revoke_picker_selection("window-a");

        assert!(matches!(
            capabilities.consume_picker_selection("window-a", &root),
            Err(PathScopeError::NotAllowed)
        ));

        let _ = fs::remove_dir_all(&root);
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
        bind_project(&capabilities, "window-a", project_a, &first);
        capabilities
            .register_picker_selection("window-a", &second)
            .expect("second picker");
        bind_project(&capabilities, "window-a", project_b, &second);

        assert_eq!(
            capabilities
                .begin_conversation("window-a", project_a)
                .unwrap_err(),
            PathScopeError::NotAllowed
        );

        let _ = fs::remove_dir_all(&first);
        let _ = fs::remove_dir_all(&second);
    }

    #[test]
    fn existing_conversation_permit_cannot_authorize_another_conversation() {
        let root = temp_dir("existing-conversation");
        let capabilities = PathCapabilities::default();
        let project_id = ProjectId::new();
        let permitted = ConversationId::new();
        let denied = ConversationId::new();
        capabilities
            .activate_registered_project("window-a", project_id, &root)
            .expect("project activation");
        capabilities
            .permit_existing_conversations("window-a", project_id, &root, &[permitted])
            .expect("host permit");

        assert_eq!(
            capabilities
                .bind_existing_conversation("window-a", project_id, denied, &root)
                .unwrap_err(),
            PathScopeError::NotAllowed
        );
        capabilities
            .bind_existing_conversation("window-a", project_id, permitted, &root)
            .expect("matching permit");
        assert!(
            capabilities
                .require_conversation_root("window-a", project_id, permitted, &root)
                .is_ok()
        );

        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn failed_conversation_persist_can_restore_the_same_bootstrap() {
        let root = temp_dir("conversation-rollback");
        let capabilities = PathCapabilities::default();
        let project_id = ProjectId::new();
        capabilities
            .activate_registered_project("window-a", project_id, &root)
            .expect("project activation");
        let bootstrap = capabilities
            .begin_conversation("window-a", project_id)
            .expect("conversation bootstrap");
        let nonce = bootstrap.grant.nonce;

        capabilities.restore_conversation_bootstrap(bootstrap);
        let retried = capabilities
            .begin_conversation("window-a", project_id)
            .expect("failed persistence must restore the bootstrap");

        assert_eq!(retried.grant.nonce, nonce);

        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn window_close_revokes_authority_and_label_reuse_does_not_inherit_it() {
        let root = temp_dir("window-reuse");
        let capabilities = PathCapabilities::default();
        let project_id = ProjectId::new();
        let conversation_id = ConversationId::new();
        capabilities
            .activate_registered_project("window-a", project_id, &root)
            .expect("project activation");
        let bootstrap = capabilities
            .begin_conversation("window-a", project_id)
            .expect("conversation bootstrap");
        capabilities.bind_new_conversation(bootstrap, conversation_id);

        capabilities.revoke_window("window-a");
        capabilities
            .register_picker_selection("window-a", &root)
            .expect("reused label receives a fresh window instance");

        assert_eq!(
            capabilities
                .require_conversation_root("window-a", project_id, conversation_id, &root)
                .unwrap_err(),
            PathScopeError::NotAllowed
        );

        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn delayed_post_persist_bind_cannot_target_a_reused_window_label() {
        let root = temp_dir("delayed-bind");
        let capabilities = PathCapabilities::default();
        let project_id = ProjectId::new();
        capabilities
            .register_picker_selection("window-a", &root)
            .expect("picker selection");
        let selection = capabilities
            .consume_picker_selection("window-a", &root)
            .expect("persistence reservation");

        capabilities.revoke_window("window-a");
        capabilities
            .register_picker_selection("window-a", &root)
            .expect("new window instance");
        capabilities.bind_project(selection, project_id);

        assert_eq!(
            capabilities
                .begin_conversation("window-a", project_id)
                .unwrap_err(),
            PathScopeError::NotAllowed
        );

        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn delayed_conversation_bind_after_close_stays_revoked() {
        let root = temp_dir("delayed-conversation");
        let capabilities = PathCapabilities::default();
        let project_id = ProjectId::new();
        let conversation_id = ConversationId::new();
        capabilities
            .activate_registered_project("window-a", project_id, &root)
            .expect("project activation");
        let bootstrap = capabilities
            .begin_conversation("window-a", project_id)
            .expect("conversation persistence reservation");

        capabilities.revoke_window("window-a");
        capabilities
            .register_picker_selection("window-a", &root)
            .expect("new window instance");
        capabilities.bind_new_conversation(bootstrap, conversation_id);

        assert_eq!(
            capabilities
                .require_conversation_root("window-a", project_id, conversation_id, &root)
                .unwrap_err(),
            PathScopeError::NotAllowed
        );

        let _ = fs::remove_dir_all(&root);
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
