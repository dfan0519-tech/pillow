//! Controller → role assignment domain for Pillow.
//!
//! This crate translates a controller-initiated dispatch into a **structured
//! process proposal** shape. It does **not** approve, spawn, store, or talk to
//! providers.
//!
//! ## Authority boundary
//!
//! Process execution in Pillow already has a governance chain
//! (proposal → one-shot human approval → host execute → durable receipt).
//! Role dispatch **reuses** that chain: an [`Assignment`] carries a
//! [`ProposedCommand`] that a host can hand to the existing process path.
//!
//! There is **no** second approval mechanism, no executor, and no path that
//! runs a process from this crate.
//!
//! ## Non-goals
//!
//! - Network, process spawn, config/file I/O
//! - Persistence (types only; storage is a later controller wiring step)
//! - UI / approval grants / secret material
//! - Elevating [`GovernanceCapability::None`] (LocalCli) to governed

mod assignment;
mod command;
mod error;
mod result;
mod transition;
mod types;

pub use assignment::{
    Assignment, AssignmentId, AssignmentTarget, BuildAssignmentInput, NewAssignment,
    build_assignment,
};
pub use command::{
    ProposedCommand, TargetSlotRecord, build_proposed_command, reject_shell_joined_command,
    task_contains_shell_metacharacters,
};
pub use error::{AttributeError, ProposeError, TransitionError};
pub use result::{
    AssignmentOutcome, AttributeInput, ProcessOutputRecord, attribute_outcome,
    attribute_outcome_for_assignment,
};
pub use transition::{
    apply_assignment_transition, is_assignment_terminal, validate_assignment_transition,
};
pub use types::{
    AssignmentState, GOVERNED_CATALOG_PROGRAM, GovernanceCapability, SlotAuthShape, SlotId,
    SlotRole, TASK_ARGV_FLAG,
};
