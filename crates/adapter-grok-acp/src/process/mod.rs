//! Child process launch, environment policy, and tree cleanup.

mod child;
mod env;

pub use child::{ChildHandle, spawn_child};
pub use env::build_child_env;
