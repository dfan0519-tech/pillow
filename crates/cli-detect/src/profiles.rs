/// Agent CLIs this crate knows how to discover and (when possible) probe.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum AgentCliKind {
    Grok,
    Claude,
    Codex,
    OpenCode,
    Gemini,
}

/// Stable iteration order for product surfaces.
pub const ALL_KINDS: &[AgentCliKind] = &[
    AgentCliKind::Grok,
    AgentCliKind::Claude,
    AgentCliKind::Codex,
    AgentCliKind::OpenCode,
    AgentCliKind::Gemini,
];

impl AgentCliKind {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Grok => "grok",
            Self::Claude => "claude",
            Self::Codex => "codex",
            Self::OpenCode => "opencode",
            Self::Gemini => "gemini",
        }
    }

    /// Primary executable leaf names searched on PATH and install roots.
    pub(crate) const fn binary_names(self) -> &'static [&'static str] {
        match self {
            Self::Grok => &["grok"],
            Self::Claude => &["claude"],
            Self::Codex => &["codex"],
            Self::OpenCode => &["opencode"],
            Self::Gemini => &["gemini"],
        }
    }

    /// Version probe argv (no login side effects expected for these flags).
    pub(crate) const fn version_args(self) -> &'static [&'static str] {
        match self {
            // `gemini -v` is the short form; both avoid starting the TUI.
            Self::Gemini => &["--version"],
            _ => &["--version"],
        }
    }

    /// Read-only status command argv, or `None` when no reliable headless signal exists.
    pub(crate) const fn status_args(self) -> Option<&'static [&'static str]> {
        match self {
            // Emits explicit login / not-login banners; does not start a chat.
            Self::Grok => Some(&["models"]),
            // Official: exit 0 = logged in, exit 1 = not logged in.
            Self::Claude => Some(&["auth", "status", "--text"]),
            // Official: prints "Logged in using …" or a not-logged message.
            Self::Codex => Some(&["login", "status"]),
            // Lists local credential providers only (not remote session validity).
            Self::OpenCode => Some(&["auth", "list"]),
            // No documented headless status; interactive `/auth` only.
            Self::Gemini => None,
        }
    }
}

/// Human-readable version command used in reports (no process launch).
pub fn version_command_summary(kind: AgentCliKind) -> &'static str {
    match kind {
        AgentCliKind::Grok => "grok --version",
        AgentCliKind::Claude => "claude --version",
        AgentCliKind::Codex => "codex --version",
        AgentCliKind::OpenCode => "opencode --version",
        AgentCliKind::Gemini => "gemini --version",
    }
}

/// Human-readable status command used in reports (no process launch).
pub fn status_command_summary(kind: AgentCliKind) -> &'static str {
    match kind {
        AgentCliKind::Grok => "grok models",
        AgentCliKind::Claude => "claude auth status --text",
        AgentCliKind::Codex => "codex login status",
        AgentCliKind::OpenCode => "opencode auth list",
        AgentCliKind::Gemini => "(none — no reliable headless status)",
    }
}
