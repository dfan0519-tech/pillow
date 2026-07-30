//! Child process launch, environment policy, and tree cleanup.

mod child;
mod env;
#[cfg(windows)]
mod job_windows;

pub(crate) use child::process_gone;
pub use child::{ChildHandle, spawn_child};
pub use env::build_child_env;
