//! Pure parsers for version and auth status command output.
//!
//! Deterministic signals only. Unrecognized shapes become
//! [`AuthState::Unknown`] — never a false [`AuthState::NotAuthenticated`].

use crate::profiles::AgentCliKind;
use crate::sanitize::sanitize_account_label;
use crate::types::AuthState;

/// Parse a version probe's combined stdout/stderr into a short version label.
pub fn parse_version_output(_kind: AgentCliKind, stdout: &str, stderr: &str) -> Option<String> {
    let combined = first_nonempty_line(stdout).or_else(|| first_nonempty_line(stderr))?;
    let line = combined.trim();
    if line.is_empty() || line.chars().count() > 200 {
        return None;
    }
    if line_looks_secret(line) {
        return None;
    }
    if looks_like_version_line(line) {
        Some(truncate_chars(line, 120))
    } else {
        None
    }
}

fn looks_like_version_line(line: &str) -> bool {
    // Must mention the tool or look like "x.y.z".
    let lower = line.to_ascii_lowercase();
    lower.contains("version")
        || lower.contains("grok")
        || lower.contains("claude")
        || lower.contains("codex")
        || lower.contains("opencode")
        || lower.contains("gemini")
        || line.chars().any(|c| c.is_ascii_digit())
}

fn line_looks_secret(line: &str) -> bool {
    let lower = line.to_ascii_lowercase();
    lower.contains("sk-")
        || lower.contains("bearer ")
        || lower.contains("api_key")
        || lower.contains("access_token")
        || line.contains("eyJ")
        || (line.chars().count() >= 40
            && !line.contains([' ', '@'])
            && line
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '+' | '/' | '=')))
}

/// Parse a status command receipt into [`AuthState`].
///
/// `exit_code` is the process exit code when the process completed normally.
/// Callers must map timeout / spawn / cleanup failures to `Unknown` *before*
/// calling this function.
pub fn parse_auth_output(
    kind: AgentCliKind,
    stdout: &str,
    stderr: &str,
    exit_code: Option<i32>,
) -> AuthState {
    let combined = join_outputs(stdout, stderr);
    match kind {
        AgentCliKind::Grok => parse_grok_models(&combined),
        AgentCliKind::Claude => parse_claude_auth_status(&combined, exit_code),
        AgentCliKind::Codex => parse_codex_login_status(&combined, exit_code),
        AgentCliKind::OpenCode => parse_opencode_auth_list(&combined, exit_code),
        AgentCliKind::Gemini => AuthState::Unknown {
            reason: "no_headless_status_command".into(),
        },
    }
}

fn parse_grok_models(output: &str) -> AuthState {
    // Deterministic banners from the grok-build CLI (verified against binary
    // string table and live logged-in output on a maintainer machine).
    const LOGGED_IN_PREFIX: &str = "You are logged in with ";
    const NOT_LOGGED_IN: &str = "Not logged in. Run `grok login`.";

    if let Some(line) = output.lines().find(|l| l.contains(LOGGED_IN_PREFIX))
        && let Some(rest) = line.split(LOGGED_IN_PREFIX).nth(1)
    {
        let label_src = rest.trim().trim_end_matches('.').trim();
        if let Some(label) = sanitize_account_label(label_src) {
            return AuthState::Authenticated {
                account_label: label,
            };
        }
        // Banner matched but label sanitizer rejected → authenticated
        // without a displayable label is still not "not logged in".
        return AuthState::Unknown {
            reason: "authenticated_label_rejected".into(),
        };
    }
    if output.contains(NOT_LOGGED_IN) {
        return AuthState::NotAuthenticated;
    }
    AuthState::Unknown {
        reason: "unrecognized_grok_models_output".into(),
    }
}

fn parse_claude_auth_status(output: &str, exit_code: Option<i32>) -> AuthState {
    // Official contract: exit 0 = logged in, exit 1 = not logged in.
    // Any other exit is Unknown (never force NotAuthenticated).
    match exit_code {
        Some(0) => {
            if let Some(label) = extract_claude_label(output) {
                AuthState::Authenticated {
                    account_label: label,
                }
            } else {
                // Logged in per exit code; no safe label → still authenticated.
                AuthState::Authenticated {
                    account_label: "authenticated".into(),
                }
            }
        }
        Some(1) => AuthState::NotAuthenticated,
        Some(code) => AuthState::Unknown {
            reason: format!("unexpected_claude_exit_code:{code}"),
        },
        None => AuthState::Unknown {
            reason: "missing_claude_exit_code".into(),
        },
    }
}

fn extract_claude_label(output: &str) -> Option<String> {
    // Prefer an email-shaped token if present.
    for token in output.split_whitespace() {
        let t = token.trim_matches(|c: char| matches!(c, ',' | ';' | '"' | '\'' | '`' | ':'));
        if t.contains('@')
            && let Some(label) = sanitize_account_label(t)
        {
            return Some(label);
        }
    }
    // Common text forms: "Logged in as X" / "Account: X"
    for line in output.lines() {
        let line = line.trim();
        for prefix in ["Logged in as ", "Account: ", "Email: ", "User: "] {
            if let Some(rest) = line.strip_prefix(prefix)
                && let Some(label) = sanitize_account_label(rest)
            {
                return Some(label);
            }
        }
    }
    None
}

fn parse_codex_login_status(output: &str, exit_code: Option<i32>) -> AuthState {
    // Live signal (maintainer machine): "Logged in using ChatGPT"
    // Also accept "Logged in using API key" style method labels.
    const LOGGED_IN_USING: &str = "Logged in using ";
    const NOT_LOGGED_MARKERS: &[&str] = &[
        "Not logged in",
        "not logged in",
        "You are not logged in",
        "No credentials found",
    ];

    if let Some(line) = output.lines().find(|l| l.contains(LOGGED_IN_USING))
        && let Some(rest) = line.split(LOGGED_IN_USING).nth(1)
    {
        let method = rest.trim().trim_end_matches('.').trim();
        if let Some(label) = sanitize_account_label(method) {
            return AuthState::Authenticated {
                account_label: label,
            };
        }
        return AuthState::Unknown {
            reason: "authenticated_label_rejected".into(),
        };
    }
    if NOT_LOGGED_MARKERS.iter().any(|m| output.contains(m)) {
        // Prefer explicit text over exit code alone.
        return AuthState::NotAuthenticated;
    }
    // Codex has been observed printing status on stderr while still exiting 0.
    // Without a deterministic marker, do not invent NotAuthenticated.
    match exit_code {
        Some(0) => AuthState::Unknown {
            reason: "unrecognized_codex_login_status_output".into(),
        },
        Some(code) => AuthState::Unknown {
            reason: format!("unrecognized_codex_login_status_exit:{code}"),
        },
        None => AuthState::Unknown {
            reason: "missing_codex_exit_code".into(),
        },
    }
}

fn parse_opencode_auth_list(output: &str, exit_code: Option<i32>) -> AuthState {
    // `opencode auth list` documents listing providers stored in the local
    // credentials file. Empty list ⇒ no local credentials; non-empty ⇒ local
    // record present (not remote session proof).
    if exit_code.is_some_and(|c| c != 0) {
        // Non-zero without a documented empty-list contract → Unknown.
        // (Do not treat command failure as NotAuthenticated.)
        return AuthState::Unknown {
            reason: format!("opencode_auth_list_exit:{}", exit_code.unwrap_or_default()),
        };
    }

    let providers = extract_opencode_providers(output);
    if providers.is_empty() {
        // Empty successful list is the only deterministic not-auth signal.
        if output_looks_like_empty_auth_list(output) {
            return AuthState::NotAuthenticated;
        }
        return AuthState::Unknown {
            reason: "unrecognized_opencode_auth_list_output".into(),
        };
    }
    let joined = providers.join(", ");
    match sanitize_account_label(&joined) {
        Some(label) => AuthState::Authenticated {
            account_label: label,
        },
        None => AuthState::Unknown {
            reason: "authenticated_label_rejected".into(),
        },
    }
}

fn extract_opencode_providers(output: &str) -> Vec<String> {
    let mut out = Vec::new();
    for line in output.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        let lower = line.to_ascii_lowercase();
        // Skip decorative / header lines.
        if lower.contains("credential") && lower.contains("list") {
            continue;
        }
        if lower.starts_with("no ") || lower.contains("not authenticated") {
            continue;
        }
        // Provider ids are short tokens; reject secret-shaped lines.
        if let Some(label) = sanitize_account_label(line) {
            // Prefer single-token provider ids.
            if !label.contains('@') && label.chars().count() <= 64 {
                out.push(label);
            }
        }
    }
    out
}

fn output_looks_like_empty_auth_list(output: &str) -> bool {
    let trimmed = output.trim();
    if trimmed.is_empty() {
        return true;
    }
    let lower = trimmed.to_ascii_lowercase();
    lower.contains("no credentials")
        || lower.contains("not logged")
        || lower.contains("no providers")
        || lower == "none"
        || lower.contains("empty")
}

fn join_outputs(stdout: &str, stderr: &str) -> String {
    match (stdout.is_empty(), stderr.is_empty()) {
        (false, false) => format!("{stdout}\n{stderr}"),
        (false, true) => stdout.to_string(),
        (true, false) => stderr.to_string(),
        (true, true) => String::new(),
    }
}

fn first_nonempty_line(text: &str) -> Option<&str> {
    text.lines().map(str::trim).find(|l| !l.is_empty())
}

fn truncate_chars(s: &str, max: usize) -> String {
    let count = s.chars().count();
    if count <= max {
        s.to_string()
    } else {
        s.chars().take(max).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn grok_logged_in_banner_is_authenticated() {
        let state = parse_auth_output(
            AgentCliKind::Grok,
            "You are logged in with grok.com.\nDefault model: grok-4.5\n",
            "",
            Some(0),
        );
        assert_eq!(
            state,
            AuthState::Authenticated {
                account_label: "grok.com".into()
            }
        );
    }

    #[test]
    fn grok_not_logged_in_banner_is_not_authenticated() {
        let state = parse_auth_output(
            AgentCliKind::Grok,
            "Not logged in. Run `grok login`.\n",
            "",
            Some(0),
        );
        assert_eq!(state, AuthState::NotAuthenticated);
    }

    #[test]
    fn grok_garbled_output_is_unknown_not_logged_out() {
        let state = parse_auth_output(AgentCliKind::Grok, "models unavailable???", "", Some(1));
        assert!(matches!(state, AuthState::Unknown { .. }));
        assert!(!matches!(state, AuthState::NotAuthenticated));
    }

    #[test]
    fn claude_exit_codes_are_authoritative() {
        assert_eq!(
            parse_auth_output(
                AgentCliKind::Claude,
                "Logged in as dev@example.com",
                "",
                Some(0)
            ),
            AuthState::Authenticated {
                account_label: "dev@example.com".into()
            }
        );
        assert_eq!(
            parse_auth_output(AgentCliKind::Claude, "Not logged in", "", Some(1)),
            AuthState::NotAuthenticated
        );
        assert!(matches!(
            parse_auth_output(AgentCliKind::Claude, "boom", "", Some(99)),
            AuthState::Unknown { .. }
        ));
    }

    #[test]
    fn codex_logged_in_using_method() {
        assert_eq!(
            parse_auth_output(
                AgentCliKind::Codex,
                "Logged in using ChatGPT\n",
                "",
                Some(0)
            ),
            AuthState::Authenticated {
                account_label: "ChatGPT".into()
            }
        );
    }

    #[test]
    fn codex_unrecognized_is_unknown() {
        let state = parse_auth_output(AgentCliKind::Codex, "something odd", "", Some(0));
        assert!(matches!(state, AuthState::Unknown { .. }));
    }

    #[test]
    fn gemini_always_unknown_status() {
        let state = parse_auth_output(AgentCliKind::Gemini, "", "", Some(0));
        assert!(matches!(
            state,
            AuthState::Unknown {
                reason
            } if reason == "no_headless_status_command"
        ));
    }

    #[test]
    fn version_parser_keeps_short_tool_lines() {
        let v = parse_version_output(AgentCliKind::Grok, "grok 1.0.0 (3cd0d0cbce) [stable]", "");
        assert!(v.unwrap().contains("1.0.0"));
    }

    #[test]
    fn version_parser_drops_token_lines() {
        // Assembled at runtime — a secret-shaped literal here trips the repo
        // secret scanner. The input under test is unchanged.
        let fake = format!("sk-{}", "fakeTESTKEY_not_real_bbbbbbbbbbbbbbbb");
        assert!(parse_version_output(AgentCliKind::Codex, &fake, "").is_none());
    }
}
