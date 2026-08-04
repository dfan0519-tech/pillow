//! Single owner of conversation repository + runtime host.
//!
//! Authoritative order for every runtime event:
//! 1. validate
//! 2. persist event + resulting run status in SQLite
//! 3. commit
//! 4. emit to the renderer
//!
//! Never emit an event that has not been durably committed.
//!
//! Runtime composition: production callers inject `Arc<dyn AgentRuntimeAdapter>`
//! via [`ConversationService::open_with_adapter`]. The fake constructors
//! (`open` / `open_with_config`) remain for tests and explicit development
//! startup only — they are not the production Grok path.

mod ids;
mod pipeline;
mod project_cwd;
mod reconciliation;
mod service;
mod types;
mod write_service;

#[cfg(test)]
mod tests;

pub use service::ConversationService;
pub use types::ConversationCommandError;
#[cfg(test)]
pub use types::PumpRegistrationHooks;
pub use write_service::{WriteDiffDto, WriteProposalDto};
