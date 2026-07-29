use std::path::Path;
use std::sync::Arc;

use keeper_project::{CreateProject, ProjectId};
use keeper_storage_sqlite::SqliteProjectRepository;

use crate::conversation_events::RecordingEmitter;
use crate::conversation_service::ConversationService;

pub(super) fn temp_db() -> (tempfile::TempDir, std::path::PathBuf) {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("keeper.db");
    (temp, path)
}

pub(super) fn seed_project(path: &Path, name: &str) -> ProjectId {
    let projects = SqliteProjectRepository::open(path).unwrap();
    let root = path.parent().unwrap().join(name);
    std::fs::create_dir_all(&root).unwrap();
    projects
        .create(CreateProject {
            name: name.into(),
            root_path: root.to_string_lossy().into_owned(),
        })
        .unwrap()
        .id
}

pub(super) fn open_service(path: &Path) -> ConversationService {
    ConversationService::open(path, Arc::new(RecordingEmitter::new()), false).unwrap()
}
