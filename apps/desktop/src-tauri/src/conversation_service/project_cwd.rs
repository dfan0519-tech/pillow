//! Resolve a conversation's project root for adapter `working_directory`.
//!
//! Project identity and root_path come from SQLite. Roots that are missing,
//! not a directory, or not registered fail closed before the adapter spawns.

use std::path::{Path, PathBuf};

use keeper_session::{ConversationId, ConversationRepository};
use keeper_storage_sqlite::{
    ProjectStoreError, SqliteConversationRepository, SqliteProjectRepository,
};

use super::ids::map_session_error;
use super::types::ConversationCommandError;

/// Resolve the canonical existing project root for a conversation.
///
/// Steps:
/// 1. Load the conversation (project-scoped) from SQLite.
/// 2. Load the registered project row.
/// 3. Require the root to exist and be a directory; canonicalize it.
pub(super) fn resolve_working_directory(
    conversations: &SqliteConversationRepository,
    projects: &SqliteProjectRepository,
    conversation_id: ConversationId,
) -> Result<String, ConversationCommandError> {
    let timeline = conversations
        .load_timeline(conversation_id)
        .map_err(map_session_error)?;
    let project = projects
        .get(timeline.conversation.project_id)
        .map_err(map_project_store_error)?
        .ok_or_else(|| {
            ConversationCommandError::new(
                "project_not_found",
                "Conversation project is not registered.",
            )
        })?;

    canonicalize_existing_project_root(&project.root_path)
}

/// Validate and canonicalize a project root path.
///
/// Rejects empty paths, missing paths, and non-directories. Does not inspect
/// auth files or secret material.
pub(super) fn canonicalize_existing_project_root(
    root_path: &str,
) -> Result<String, ConversationCommandError> {
    let trimmed = root_path.trim();
    if trimmed.is_empty() {
        return Err(ConversationCommandError::new(
            "invalid_project_root",
            "Project folder cannot be empty.",
        ));
    }

    let path = Path::new(trimmed);
    if !path.exists() {
        return Err(ConversationCommandError::new(
            "invalid_project_root",
            "The project folder does not exist or cannot be opened.",
        ));
    }
    if !path.is_dir() {
        return Err(ConversationCommandError::new(
            "invalid_project_root",
            "The project root must be an existing directory.",
        ));
    }

    let canonical = path.canonicalize().map_err(|source| {
        ConversationCommandError::new(
            "invalid_project_root",
            format!(
                "Failed to canonicalize project root `{}`: {source}",
                display_path(path)
            ),
        )
    })?;

    if !canonical.is_dir() {
        return Err(ConversationCommandError::new(
            "invalid_project_root",
            "The project root must be an existing directory.",
        ));
    }

    path_to_string(canonical)
}

fn path_to_string(path: PathBuf) -> Result<String, ConversationCommandError> {
    path.into_os_string().into_string().map_err(|_| {
        ConversationCommandError::new(
            "invalid_project_root",
            "Project root path is not valid UTF-8.",
        )
    })
}

fn display_path(path: &Path) -> String {
    path.to_string_lossy().into_owned()
}

fn map_project_store_error(error: ProjectStoreError) -> ConversationCommandError {
    eprintln!("Keeper IDE project store error while resolving cwd: {error}");
    match error {
        ProjectStoreError::Canonicalize { .. } => ConversationCommandError::new(
            "invalid_project_root",
            "The project folder does not exist or cannot be opened.",
        ),
        ProjectStoreError::DuplicateRoot => ConversationCommandError::new(
            "project_root_exists",
            "A project already exists for this folder.",
        ),
        ProjectStoreError::Sqlite(_) | ProjectStoreError::LockPoisoned => {
            ConversationCommandError::new(
                "project_store_unavailable",
                "Project storage is temporarily unavailable.",
            )
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    use keeper_project::CreateProject;
    use tempfile::tempdir;

    #[test]
    fn rejects_missing_root() {
        let err = canonicalize_existing_project_root(
            std::env::temp_dir()
                .join("keeper-ide-missing-root-does-not-exist")
                .to_string_lossy()
                .as_ref(),
        )
        .unwrap_err();
        assert_eq!(err.code, "invalid_project_root");
    }

    #[test]
    fn rejects_file_as_root() {
        let temp = tempdir().unwrap();
        let file = temp.path().join("not-a-dir");
        fs::write(&file, b"x").unwrap();
        let err = canonicalize_existing_project_root(file.to_str().unwrap()).unwrap_err();
        assert_eq!(err.code, "invalid_project_root");
        assert!(err.message.contains("directory"));
    }

    #[test]
    fn accepts_existing_directory() {
        let temp = tempdir().unwrap();
        let root = temp.path().join("proj");
        fs::create_dir_all(&root).unwrap();
        let canonical = canonicalize_existing_project_root(root.to_str().unwrap()).unwrap();
        let expected = root.canonicalize().unwrap();
        assert_eq!(Path::new(&canonical), expected.as_path());
    }

    #[test]
    fn resolve_requires_registered_project() {
        let temp = tempdir().unwrap();
        let db = temp.path().join("keeper.db");
        let conversations = SqliteConversationRepository::open(&db).unwrap();
        let projects = SqliteProjectRepository::open(&db).unwrap();
        let root = temp.path().join("registered");
        fs::create_dir_all(&root).unwrap();
        let project = projects
            .create(CreateProject {
                name: "registered".into(),
                root_path: root.to_string_lossy().into_owned(),
            })
            .unwrap();
        let conversation = conversations
            .create_conversation(keeper_session::NewConversation {
                project_id: project.id,
                title: Some("c".into()),
            })
            .unwrap();

        let cwd = resolve_working_directory(&conversations, &projects, conversation.id).unwrap();
        assert_eq!(Path::new(&cwd), root.canonicalize().unwrap().as_path());

        // Unregistered: conversation id that never existed.
        let missing = resolve_working_directory(
            &conversations,
            &projects,
            ConversationId(uuid::Uuid::new_v4()),
        )
        .unwrap_err();
        assert_eq!(missing.code, "conversation_not_found");
    }
}
