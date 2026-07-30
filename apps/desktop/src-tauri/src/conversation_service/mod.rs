//! Single owner of conversation repository + deterministic runtime host.
//!
//! Authoritative order for every runtime event:
//! 1. validate
//! 2. persist event + resulting run status in SQLite
//! 3. commit
//! 4. emit to the renderer
//!
//! Never emit an event that has not been durably committed.

mod ids;
mod pipeline;
mod service;
mod types;

#[cfg(test)]
mod tests;

pub use service::ConversationService;
pub use types::ConversationCommandError;
#[cfg(test)]
pub use types::PumpRegistrationHooks;
