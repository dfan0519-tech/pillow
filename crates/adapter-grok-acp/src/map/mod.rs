//! Wire → provider-neutral event mapping.

mod events;
mod permission;
mod session;
mod terminal;

pub use events::{MappedEvent, map_session_update};
pub use permission::{ToolRegistry, decide_permission, observe_tool_update};
pub use session::SessionMap;
pub use terminal::{map_stop_reason, terminal_for_error};
