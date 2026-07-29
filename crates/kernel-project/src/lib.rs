use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ProjectId(pub Uuid);

impl ProjectId {
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }
}

impl Default for ProjectId {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProjectTrust {
    Untrusted,
    Trusted,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Project {
    pub id: ProjectId,
    pub name: String,
    pub root_path: String,
    pub trust: ProjectTrust,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CreateProject {
    pub name: String,
    pub root_path: String,
}

pub trait ProjectRepository {
    type Error;

    fn create(&self, input: CreateProject) -> Result<Project, Self::Error>;
    fn list(&self) -> Result<Vec<Project>, Self::Error>;
    fn get(&self, id: ProjectId) -> Result<Option<Project>, Self::Error>;
    fn find_by_canonical_root(&self, root_path: &str) -> Result<Option<Project>, Self::Error>;
}
