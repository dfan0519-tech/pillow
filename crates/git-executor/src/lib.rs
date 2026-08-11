//! Git porcelain parser, bounds, and hardened structured command runner.
//!
//! Parsing is pure. Process launch uses fixed Git templates over
//! [`keeper_process_executor::SupervisedProcessRunner`] and never exposes an
//! arbitrary argv API.

mod diff;
mod discovery;
mod mutation;
mod portable_path;
mod runner;
mod status;

pub use diff::{DIFF_TEXT_BYTE_CAP, GitDiffSnapshot, GitDiffView, bound_git_diff};
pub use discovery::{DISCOVERY_MAX_DEPTH, DISCOVERY_MAX_REPOS, DiscoveredRepository, GitDiscovery};
pub use mutation::{
    BRANCH_NAME_MAX_LEN, COMMIT_MESSAGE_MAX_LEN, GitInputKind, GitMutationKind, GitMutationReceipt,
    REMOTE_NAME_MAX_LEN,
};
pub use portable_path::{
    PORTABLE_COMPONENT_MAX_LEN, PORTABLE_PATH_MAX_LEN, is_portable_entry_path, is_portable_path,
};
pub use runner::{GIT_READ_TIMEOUT, GitReadError, GitReadExecutor, UnsafeConfigKind};
pub use status::{
    GitHead, GitRepositoryId, GitStatusEntry, GitStatusKind, GitStatusSnapshot, ParseError,
    STATUS_ENTRY_CAP, STATUS_TRANSPORT_CAP, parse_porcelain_v2_z,
};
