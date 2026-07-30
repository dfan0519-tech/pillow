//! Adapter configuration and hard limits.

use std::path::PathBuf;
use std::time::Duration;

use keeper_agent_protocol::RuntimeError;

/// Production ACP launch argv. Fixed; not overridable from untrusted product input.
pub const PRODUCTION_ACP_ARGV: &[&str] = &["agent", "--no-leader", "stdio"];

/// Default per-request timeout (legacy ACP client: 3 minutes).
pub const DEFAULT_REQUEST_TIMEOUT: Duration = Duration::from_secs(180);

/// Default max NDJSON line size (1 MiB).
pub const DEFAULT_MAX_LINE_BYTES: usize = 1_048_576;

/// Default max concatenated assistant text for one prompt.
pub const DEFAULT_MAX_RESPONSE_CHARS: usize = 12_000;

/// Default max public tool activity observations per prompt.
pub const DEFAULT_MAX_TOOL_ACTIVITY: usize = 24;

/// Configuration for launching and talking to a Grok ACP child.
#[derive(Debug, Clone)]
pub struct GrokAcpConfig {
    /// Path to the Grok CLI (or fake child binary in tests).
    pub executable: PathBuf,
    /// Per JSON-RPC request timeout.
    pub request_timeout: Duration,
    /// Hard cap on buffered/incomplete stdout line bytes.
    pub max_line_bytes: usize,
    /// Cap on concatenated assistant text for one prompt.
    pub max_response_chars: usize,
    /// Optional isolated process home (`HOME` / `USERPROFILE` override).
    pub honor_isolated_home: Option<PathBuf>,
    /// Extra allow-listed env pairs for tests (never secrets).
    pub extra_env: Vec<(String, String)>,
    /// Grace period after cooperative cancel before kill-tree.
    pub cancel_grace: Duration,
    /// Test-only argv override. `None` means production `agent --no-leader stdio`.
    ///
    /// Must never be populated from untrusted product configuration.
    argv_override: Option<Vec<String>>,
}

impl GrokAcpConfig {
    /// Build a config with industrial defaults for the given executable.
    pub fn new(executable: impl Into<PathBuf>) -> Self {
        Self {
            executable: executable.into(),
            request_timeout: DEFAULT_REQUEST_TIMEOUT,
            max_line_bytes: DEFAULT_MAX_LINE_BYTES,
            max_response_chars: DEFAULT_MAX_RESPONSE_CHARS,
            honor_isolated_home: None,
            extra_env: Vec::new(),
            cancel_grace: Duration::from_millis(250),
            argv_override: None,
        }
    }

    /// Launch argv for the child process.
    ///
    /// Production is always [`PRODUCTION_ACP_ARGV`]. Overrides exist only via
    /// [`Self::with_test_argv`] for the fake ACP child.
    pub fn launch_argv(&self) -> Vec<String> {
        match &self.argv_override {
            Some(args) => args.clone(),
            None => PRODUCTION_ACP_ARGV
                .iter()
                .map(|s| (*s).to_string())
                .collect(),
        }
    }

    /// Override launch argv for deterministic fake-child tests.
    ///
    /// This is intentionally explicit and test-oriented. Product wiring must
    /// not call this with untrusted input.
    pub fn with_test_argv(mut self, args: Vec<String>) -> Self {
        self.argv_override = Some(args);
        self
    }

    /// Validate paths and limits without launching a process.
    pub fn validate(&self) -> Result<(), RuntimeError> {
        if self.executable.as_os_str().is_empty() {
            return Err(RuntimeError::Internal {
                message: "grok executable path is empty".into(),
            });
        }
        if self.max_line_bytes == 0 {
            return Err(RuntimeError::Internal {
                message: "max_line_bytes must be > 0".into(),
            });
        }
        if self.max_response_chars == 0 {
            return Err(RuntimeError::Internal {
                message: "max_response_chars must be > 0".into(),
            });
        }
        if self.request_timeout.is_zero() {
            return Err(RuntimeError::Internal {
                message: "request_timeout must be > 0".into(),
            });
        }
        if let Some(home) = &self.honor_isolated_home {
            validate_local_path(home, "isolated home")?;
        }
        Ok(())
    }
}

fn validate_local_path(path: &std::path::Path, label: &str) -> Result<(), RuntimeError> {
    let s = path.to_string_lossy();
    if s.is_empty() {
        return Err(RuntimeError::Internal {
            message: format!("{label} is empty"),
        });
    }
    if s.starts_with(r"\\") || s.starts_with("//") {
        return Err(RuntimeError::Internal {
            message: format!("{label} must be a local path"),
        });
    }
    for component in path.components() {
        if matches!(component, std::path::Component::ParentDir) {
            return Err(RuntimeError::Internal {
                message: format!("{label} must not traverse directories"),
            });
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_match_security_budget() {
        let cfg = GrokAcpConfig::new("grok");
        assert_eq!(cfg.max_line_bytes, DEFAULT_MAX_LINE_BYTES);
        assert_eq!(cfg.max_response_chars, DEFAULT_MAX_RESPONSE_CHARS);
        assert_eq!(cfg.request_timeout, DEFAULT_REQUEST_TIMEOUT);
        assert_eq!(cfg.launch_argv(), vec!["agent", "--no-leader", "stdio"]);
        assert_eq!(PRODUCTION_ACP_ARGV, &["agent", "--no-leader", "stdio"]);
    }

    #[test]
    fn test_argv_override_is_explicit() {
        let cfg = GrokAcpConfig::new("fake").with_test_argv(vec![]);
        assert!(cfg.launch_argv().is_empty());
        let production = GrokAcpConfig::new("grok");
        assert_eq!(production.launch_argv().len(), 3);
    }

    #[test]
    fn rejects_zero_limits() {
        let mut cfg = GrokAcpConfig::new("grok");
        cfg.max_line_bytes = 0;
        assert!(cfg.validate().is_err());
    }

    #[test]
    fn rejects_unc_isolated_home() {
        let mut cfg = GrokAcpConfig::new("grok");
        cfg.honor_isolated_home = Some(PathBuf::from(r"\\server\share"));
        assert!(cfg.validate().is_err());
    }
}
