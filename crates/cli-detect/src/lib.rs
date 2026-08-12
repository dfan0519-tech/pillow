//! Pure discovery and read-only account-status probing for local agent CLIs.
//!
//! This crate never starts a conversation, never writes CLI config, and never
//! returns token / key / cookie material. Probe failures are always
//! [`AuthState::Unknown`], never [`AuthState::NotAuthenticated`].

mod discover;
mod parse;
mod probe;
mod profiles;
mod sanitize;
mod types;

pub use discover::{discover, discover_all, discover_in};
pub use parse::{parse_auth_output, parse_version_output};
pub use probe::{
    DEFAULT_PROBE_TIMEOUT, auth_state_from_probe_failure, detect, detect_all,
    detect_all_with_timeout, detect_with_timeout, probe_found,
};
pub use profiles::{ALL_KINDS, AgentCliKind, status_command_summary, version_command_summary};
pub use sanitize::sanitize_account_label;
pub use types::{AuthState, CliDetection, Discovery, ProbeError};
