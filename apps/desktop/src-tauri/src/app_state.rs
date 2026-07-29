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
    selected_project: Mutex<Option<ProjectId>>,
}

impl AppState {
    pub fn open(path: impl AsRef<Path>) -> Result<Self, ProjectStoreError> {
        Ok(Self {
            projects: SqliteProjectRepository::open(path)?,
            selected_project: Mutex::new(None),
        })
    }

    pub fn create_project(
        &self,
        name: String,
        root_path: String,
    ) -> Result<ProjectDto, CommandError> {
        if name.trim().is_empty() {
            return Err(command_error(
                "invalid_project_name",
                "Project name cannot be empty.",
            ));
        }
        if root_path.trim().is_empty() {
            return Err(command_error(
                "invalid_project_root",
                "Project folder cannot be empty.",
            ));
        }

        self.projects
            .create(CreateProject { name, root_path })
            .map(ProjectDto::from)
            .map_err(map_store_error)
    }

    pub fn list_projects(&self) -> Result<Vec<ProjectDto>, CommandError> {
        self.projects
            .list()
            .map(|projects| projects.into_iter().map(ProjectDto::from).collect())
            .map_err(map_store_error)
    }

    pub fn select_project(&self, project_id: String) -> Result<ProjectDto, CommandError> {
        let id = Uuid::parse_str(&project_id)
            .map(ProjectId)
            .map_err(|_| project_not_found())?;
        let project = self
            .projects
            .get(id)
            .map_err(map_store_error)?
            .ok_or_else(project_not_found)?;
        let mut selected = self.selected_project.lock().map_err(|_| {
            command_error(
                "project_state_unavailable",
                "Project selection is temporarily unavailable.",
            )
        })?;
        *selected = Some(id);

        Ok(project.into())
    }

    /// Open an existing registered project for `root_path`, or register it.
    /// Project name is derived from the folder leaf when registering.
    pub fn open_or_register_project(&self, root_path: String) -> Result<ProjectDto, CommandError> {
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

        path_scope::grant_path(&path);

        if let Some(existing) = self
            .projects
            .find_by_canonical_root(trimmed)
            .map_err(map_store_error)?
        {
            return Ok(existing.into());
        }

        let name = project_name_from_path(&path)?;
        self.create_project(name, trimmed.to_owned())
    }

    /// Create `parent/name`, register the project, and return the DTO.
    pub fn create_project_with_folder(
        &self,
        name: String,
        parent_path: String,
    ) -> Result<ProjectDto, CommandError> {
        let parent = PathBuf::from(parent_path.trim());
        let child = create_child_directory(&parent, &name)?;
        path_scope::grant_path(&child);
        let root_path = child.to_string_lossy().into_owned();
        self.create_project(name.trim().to_owned(), root_path)
    }
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
