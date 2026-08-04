use std::path::{Path, PathBuf};

use keeper_project::{CreateProject, Project, ProjectId};
use keeper_session::ConversationId;
use keeper_storage_sqlite::{
    ProjectStoreError, ResourceRecent, ResourceRecentError, SqliteProjectRepository,
    SqliteResourceRecentRepository,
};
use serde::Serialize;
use uuid::Uuid;

use crate::directory_ops::project_name_from_path;
use crate::path_scope;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectDto {
    pub id: String,
    pub name: String,
    pub root_path: String,
    pub trust: keeper_project::ProjectTrust,
    pub created_at: String,
    pub updated_at: String,
}

impl From<Project> for ProjectDto {
    fn from(project: Project) -> Self {
        Self {
            id: project.id.0.to_string(),
            name: project.name,
            root_path: project.root_path,
            trust: project.trust,
            created_at: project.created_at.to_rfc3339(),
            updated_at: project.updated_at.to_rfc3339(),
        }
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CommandError {
    pub code: &'static str,
    pub message: String,
}

pub struct AppState {
    projects: SqliteProjectRepository,
    resource_recents: SqliteResourceRecentRepository,
    path_capabilities: path_scope::PathCapabilities,
}

impl AppState {
    pub fn open(path: impl AsRef<Path>) -> Result<Self, ProjectStoreError> {
        let path = path.as_ref();
        // Projects open applies all migrations (including resource_recents).
        let projects = SqliteProjectRepository::open(path)?;
        let resource_recents =
            SqliteResourceRecentRepository::open(path).map_err(map_recent_open_error)?;
        Ok(Self {
            projects,
            resource_recents,
            path_capabilities: path_scope::PathCapabilities::default(),
        })
    }

    /// Look up a registered project by id only. Never falls back to cwd or last-opened.
    pub fn get_registered_project(&self, project_id: &str) -> Result<Project, CommandError> {
        let id = parse_project_id(project_id)?;
        self.projects
            .get(id)
            .map_err(map_store_error)?
            .ok_or_else(project_not_found)
    }

    pub(crate) fn record_resource_recent(
        &self,
        project_id: &str,
        path: &str,
    ) -> Result<(), ResourceRecentError> {
        self.resource_recents.record(project_id, path)
    }

    pub(crate) fn list_resource_recents(
        &self,
        project_id: &str,
        limit: u32,
    ) -> Result<Vec<ResourceRecent>, ResourceRecentError> {
        self.resource_recents.list(project_id, limit)
    }

    pub fn register_picker_selection(
        &self,
        window_id: &str,
        path: &Path,
    ) -> Result<String, CommandError> {
        self.path_capabilities
            .register_picker_selection(window_id, path)
            .map(|canonical| canonical.to_string_lossy().into_owned())
            .map_err(map_path_scope_error)
    }

    pub(crate) fn begin_picker_operation(&self, window_id: &str) -> path_scope::PickerOperation {
        self.path_capabilities.begin_picker_operation(window_id)
    }

    pub(crate) fn complete_picker_operation(
        &self,
        operation: path_scope::PickerOperation,
        path: Option<&Path>,
    ) -> Result<Option<String>, CommandError> {
        self.path_capabilities
            .complete_picker_operation(operation, path)
            .map(|root| root.map(|root| root.to_string_lossy().into_owned()))
            .map_err(map_path_scope_error)
    }

    pub fn create_project_from_picker_for_window(
        &self,
        window_id: &str,
        name: String,
        root_path: String,
    ) -> Result<ProjectDto, CommandError> {
        if name.trim().is_empty() {
            return Err(command_error(
                "invalid_project_name",
                "Project name cannot be empty.",
            ));
        }
        let path = PathBuf::from(root_path.trim());
        let picker_selection = self
            .path_capabilities
            .consume_picker_selection(window_id, &path)
            .map_err(map_path_scope_error)?;
        let canonical_root = picker_selection.canonical_root().to_path_buf();
        let project = self.projects.create(CreateProject {
            name,
            root_path: canonical_root.to_string_lossy().into_owned(),
        });
        let project = match project {
            Ok(project) => project,
            Err(error) => {
                self.path_capabilities
                    .restore_picker_selection(picker_selection);
                return Err(map_store_error(error));
            }
        };
        self.path_capabilities
            .bind_project(picker_selection, project.id);
        Ok(project.into())
    }

    pub fn list_projects(&self) -> Result<Vec<ProjectDto>, CommandError> {
        self.projects
            .list()
            .map(|projects| projects.into_iter().map(ProjectDto::from).collect())
            .map_err(map_store_error)
    }

    pub fn select_project(&self, project_id: String) -> Result<ProjectDto, CommandError> {
        self.select_project_for_window("main", project_id)
    }

    pub fn select_project_for_window(
        &self,
        window_id: &str,
        project_id: String,
    ) -> Result<ProjectDto, CommandError> {
        let id = Uuid::parse_str(&project_id)
            .map(ProjectId)
            .map_err(|_| project_not_found())?;
        let project = self
            .projects
            .get(id)
            .map_err(map_store_error)?
            .ok_or_else(project_not_found)?;
        self.path_capabilities
            .activate_registered_project(window_id, id, Path::new(&project.root_path))
            .map_err(map_path_scope_error)?;

        Ok(project.into())
    }

    pub(crate) fn begin_conversation_for_window(
        &self,
        window_id: &str,
        project_id: String,
    ) -> Result<path_scope::ConversationBootstrap, CommandError> {
        let id = parse_project_id(&project_id)?;
        self.path_capabilities
            .begin_conversation(window_id, id)
            .map_err(map_path_scope_error)
    }

    pub(crate) fn restore_conversation_bootstrap(
        &self,
        bootstrap: path_scope::ConversationBootstrap,
    ) {
        self.path_capabilities
            .restore_conversation_bootstrap(bootstrap);
    }

    pub(crate) fn bind_new_conversation(
        &self,
        bootstrap: path_scope::ConversationBootstrap,
        conversation_id: ConversationId,
    ) {
        self.path_capabilities
            .bind_new_conversation(bootstrap, conversation_id);
    }

    pub(crate) fn permit_existing_conversations_for_window(
        &self,
        window_id: &str,
        project_id: ProjectId,
        conversation_ids: &[ConversationId],
    ) -> Result<(), CommandError> {
        let project = self
            .projects
            .get(project_id)
            .map_err(map_store_error)?
            .ok_or_else(project_not_found)?;
        self.path_capabilities
            .permit_existing_conversations(
                window_id,
                project_id,
                Path::new(&project.root_path),
                conversation_ids,
            )
            .map_err(map_path_scope_error)
    }

    pub(crate) fn bind_existing_conversation_for_window(
        &self,
        window_id: &str,
        project_id: ProjectId,
        conversation_id: ConversationId,
        expected_root: &Path,
    ) -> Result<(), CommandError> {
        self.path_capabilities
            .bind_existing_conversation(window_id, project_id, conversation_id, expected_root)
            .map_err(map_path_scope_error)
    }

    pub(crate) fn require_conversation_root_for_window(
        &self,
        window_id: &str,
        project_id: ProjectId,
        conversation_id: ConversationId,
        expected_root: &Path,
    ) -> Result<PathBuf, CommandError> {
        self.path_capabilities
            .require_conversation_root(window_id, project_id, conversation_id, expected_root)
            .map_err(map_path_scope_error)
    }

    pub fn revoke_window(&self, window_id: &str) {
        self.path_capabilities.revoke_window(window_id);
    }

    /// Open an existing registered project for `root_path`, or register it.
    /// Project name is derived from the folder leaf when registering.
    pub fn open_or_register_project(&self, root_path: String) -> Result<ProjectDto, CommandError> {
        self.open_or_register_project_for_window("main", root_path)
    }

    pub fn open_or_register_project_for_window(
        &self,
        window_id: &str,
        root_path: String,
    ) -> Result<ProjectDto, CommandError> {
        let trimmed = root_path.trim();
        if trimmed.is_empty() {
            return Err(command_error(
                "invalid_project_root",
                "Project folder cannot be empty.",
            ));
        }

        let path = PathBuf::from(trimmed);
        if !path.is_dir() {
            return Err(command_error(
                "invalid_project_root",
                "The project folder does not exist or cannot be opened.",
            ));
        }

        let picker_selection = self
            .path_capabilities
            .consume_picker_selection(window_id, &path)
            .map_err(map_path_scope_error)?;
        let canonical_root = picker_selection.canonical_root().to_path_buf();
        let canonical_root_string = canonical_root.to_string_lossy().into_owned();

        let project_result = (|| {
            if let Some(existing) = self
                .projects
                .find_by_canonical_root(&canonical_root_string)
                .map_err(map_store_error)?
            {
                return Ok(existing);
            }
            let name = project_name_from_path(&canonical_root)?;
            self.projects
                .create(CreateProject {
                    name,
                    root_path: canonical_root_string,
                })
                .map_err(map_store_error)
        })();
        let project = match project_result {
            Ok(project) => project,
            Err(error) => {
                self.path_capabilities
                    .restore_picker_selection(picker_selection);
                return Err(error);
            }
        };

        self.path_capabilities
            .bind_project(picker_selection, project.id);
        Ok(project.into())
    }

    /// Disabled until Windows-safe handle-relative directory creation exists.
    pub fn create_project_with_folder(
        &self,
        name: String,
        parent_path: String,
    ) -> Result<ProjectDto, CommandError> {
        self.create_project_with_folder_for_window("main", name, parent_path)
    }

    pub fn create_project_directory_from_picker_for_window(
        &self,
        _window_id: &str,
        _parent_path: String,
        _name: String,
    ) -> Result<String, CommandError> {
        Err(project_folder_creation_disabled())
    }

    pub fn create_project_with_folder_for_window(
        &self,
        _window_id: &str,
        _name: String,
        _parent_path: String,
    ) -> Result<ProjectDto, CommandError> {
        Err(project_folder_creation_disabled())
    }
}

fn parse_project_id(project_id: &str) -> Result<ProjectId, CommandError> {
    Uuid::parse_str(project_id)
        .map(ProjectId)
        .map_err(|_| project_not_found())
}

fn map_path_scope_error(error: path_scope::PathScopeError) -> CommandError {
    command_error(error.code(), error.message())
}

fn map_store_error(error: ProjectStoreError) -> CommandError {
    eprintln!("Pillow project store error: {error}");
    match error {
        ProjectStoreError::DuplicateRoot => command_error(
            "project_root_exists",
            "A project already exists for this folder.",
        ),
        ProjectStoreError::Canonicalize { .. } => command_error(
            "invalid_project_root",
            "The project folder does not exist or cannot be opened.",
        ),
        ProjectStoreError::Sqlite(_) | ProjectStoreError::LockPoisoned => command_error(
            "project_store_unavailable",
            "Project storage is temporarily unavailable.",
        ),
    }
}

fn project_not_found() -> CommandError {
    command_error("project_not_found", "Project not found.")
}

fn project_folder_creation_disabled() -> CommandError {
    command_error(
        "project_folder_creation_disabled",
        "Creating project folders is disabled until handle-relative creation is available.",
    )
}

fn command_error(code: &'static str, message: &'static str) -> CommandError {
    CommandError {
        code,
        message: message.to_owned(),
    }
}

fn map_recent_open_error(error: ResourceRecentError) -> ProjectStoreError {
    // Projects already opened the same path; recents failure is treated as
    // store unavailability without constructing a rusqlite::Error here.
    match error {
        ResourceRecentError::LockPoisoned => ProjectStoreError::LockPoisoned,
        ResourceRecentError::Sqlite
        | ResourceRecentError::InvalidProjectId
        | ResourceRecentError::InvalidPath => ProjectStoreError::LockPoisoned,
    }
}
