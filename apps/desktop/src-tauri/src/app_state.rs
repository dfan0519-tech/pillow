use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use keeper_project::{CreateProject, Project, ProjectId};
use keeper_storage_sqlite::{ProjectStoreError, SqliteProjectRepository};
use serde::Serialize;
use uuid::Uuid;

use crate::directory_ops::{create_child_directory, project_name_from_path};
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
    selected_projects: Mutex<HashMap<String, ProjectId>>,
    path_capabilities: path_scope::PathCapabilities,
}

impl AppState {
    pub fn open(path: impl AsRef<Path>) -> Result<Self, ProjectStoreError> {
        Ok(Self {
            projects: SqliteProjectRepository::open(path)?,
            selected_projects: Mutex::new(HashMap::new()),
            path_capabilities: path_scope::PathCapabilities::default(),
        })
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
        let project = self
            .projects
            .create(CreateProject {
                name,
                root_path: canonical_root.to_string_lossy().into_owned(),
            })
            .map_err(map_store_error)?;
        self.path_capabilities
            .bind_project(picker_selection, project.id, &canonical_root)
            .map_err(map_path_scope_error)?;
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
        let mut selected = self.selected_projects.lock().map_err(|_| {
            command_error(
                "project_state_unavailable",
                "Project selection is temporarily unavailable.",
            )
        })?;
        selected.insert(window_id.to_owned(), id);

        Ok(project.into())
    }

    pub fn require_project_root_for_window(
        &self,
        window_id: &str,
        project_id: String,
        expected_root: &Path,
    ) -> Result<PathBuf, CommandError> {
        let id = Uuid::parse_str(&project_id)
            .map(ProjectId)
            .map_err(|_| project_not_found())?;
        self.path_capabilities
            .require_project_root(window_id, id, expected_root)
            .map_err(map_path_scope_error)
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

        let project = if let Some(existing) = self
            .projects
            .find_by_canonical_root(&canonical_root_string)
            .map_err(map_store_error)?
        {
            existing
        } else {
            let name = project_name_from_path(&canonical_root)?;
            self.projects
                .create(CreateProject {
                    name,
                    root_path: canonical_root_string,
                })
                .map_err(map_store_error)?
        };

        self.path_capabilities
            .bind_project(picker_selection, project.id, Path::new(&project.root_path))
            .map_err(map_path_scope_error)?;
        Ok(project.into())
    }

    /// Create `parent/name`, register the project, and return the DTO.
    /// Parent must have a live one-use native-picker capability.
    pub fn create_project_with_folder(
        &self,
        name: String,
        parent_path: String,
    ) -> Result<ProjectDto, CommandError> {
        self.create_project_with_folder_for_window("main", name, parent_path)
    }

    pub fn create_project_directory_from_picker_for_window(
        &self,
        window_id: &str,
        parent_path: String,
        name: String,
    ) -> Result<String, CommandError> {
        let parent = PathBuf::from(parent_path.trim());
        let picker_selection = self
            .path_capabilities
            .consume_picker_selection(window_id, &parent)
            .map_err(map_path_scope_error)?;
        let child = create_child_directory(&parent, &name)?;
        self.path_capabilities
            .derive_child_selection(picker_selection, &child)
            .map(|_| child.to_string_lossy().into_owned())
            .map_err(map_path_scope_error)
    }

    pub fn create_project_with_folder_for_window(
        &self,
        window_id: &str,
        name: String,
        parent_path: String,
    ) -> Result<ProjectDto, CommandError> {
        let parent = PathBuf::from(parent_path.trim());
        let picker_selection = self
            .path_capabilities
            .consume_picker_selection(window_id, &parent)
            .map_err(map_path_scope_error)?;
        let child = create_child_directory(picker_selection.canonical_root(), &name)?;
        let canonical_child = child
            .canonicalize()
            .map_err(|_| map_path_scope_error(path_scope::PathScopeError::NotAllowed))?;
        let project = self
            .projects
            .create(CreateProject {
                name: name.trim().to_owned(),
                root_path: canonical_child.to_string_lossy().into_owned(),
            })
            .map_err(map_store_error)?;
        self.path_capabilities
            .activate_registered_project(window_id, project.id, &canonical_child)
            .map_err(map_path_scope_error)?;
        Ok(project.into())
    }
}

fn map_path_scope_error(error: path_scope::PathScopeError) -> CommandError {
    command_error(error.code(), error.message())
}

fn map_store_error(error: ProjectStoreError) -> CommandError {
    eprintln!("Keeper IDE project store error: {error}");
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

fn command_error(code: &'static str, message: &'static str) -> CommandError {
    CommandError {
        code,
        message: message.to_owned(),
    }
}
