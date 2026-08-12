use std::path::PathBuf;

use thiserror::Error;

use crate::profiles::AgentCliKind;

/// Three-state account probe. Probe failure is never collapsed into
/// [`AuthState::NotAuthenticated`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AuthState {
    /// Deterministic authenticated signal; `account_label` is already sanitized.
    Authenticated { account_label: String },
    /// Deterministic not-authenticated signal from the CLI itself.
    NotAuthenticated,
    /// Timeout, spawn failure, unparseable output, missing status command, etc.
    Unknown { reason: String },
}

/// Filesystem discovery result (no process launch).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Discovery {
    pub kind: AgentCliKind,
    pub found: bool,
    /// Absolute path to a candidate executable when found.
    pub executable: Option<PathBuf>,
}

/// Full detection: discovery + optional version + account status.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CliDetection {
    pub kind: AgentCliKind,
    pub found: bool,
    pub executable: Option<PathBuf>,
    /// Best-effort short version string (sanitized); never secret-bearing.
    pub version: Option<String>,
    pub auth: AuthState,
}

/// Errors from the low-level supervised probe runner (tests / advanced callers).
#[derive(Debug, Error, Clone, Copy, PartialEq, Eq)]
pub enum ProbeError {
    #[error("probe executable unavailable")]
    ExecutableUnavailable,
    #[error("probe process failed to start")]
    Spawn,
    #[error("probe process environment rejected")]
    EnvironmentRejected,
    #[error("probe process wait failed")]
    Wait,
    #[error("probe output capture failed")]
    Output,
    #[error("probe cleanup could not be proven")]
    CleanupUnproven,
    #[error("probe timed out")]
    TimedOut,
    #[error("probe cancelled")]
    Cancelled,
}
