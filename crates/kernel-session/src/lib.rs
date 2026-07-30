//! Runtime-neutral conversation, run and event domain for Pillow.
//!
//! This crate defines durable session vocabulary and pure lifecycle rules.
//! Persistence lives in `keeper-storage-sqlite`. No provider, process, secret
//! or network concerns belong here.

mod domain;
mod transition;

pub use domain::{
    Conversation, ConversationId, EventId, Message, MessageId, MessageRole, NewConversation,
    NewRun, NewRunEvent, Run, RunEvent, RunId, RunStatus, RuntimeEventKind, SessionError, Timeline,
};
pub use transition::validate_run_transition;

use keeper_project::ProjectId;

// ---------------------------------------------------------------------------
// Repository contract
// ---------------------------------------------------------------------------

pub trait ConversationRepository {
    fn create_conversation(&self, input: NewConversation) -> Result<Conversation, SessionError>;
    fn list_conversations(&self, project_id: ProjectId) -> Result<Vec<Conversation>, SessionError>;
    fn create_run(&self, input: NewRun) -> Result<Run, SessionError>;
    fn append_event(&self, event: NewRunEvent) -> Result<RunEvent, SessionError>;
    fn load_timeline(&self, conversation_id: ConversationId) -> Result<Timeline, SessionError>;
    fn transition_run(&self, run_id: RunId, next: RunStatus) -> Result<Run, SessionError>;
    fn mark_inflight_interrupted(&self) -> Result<usize, SessionError>;
}
