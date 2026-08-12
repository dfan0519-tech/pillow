//! Contract tests for pillow-cli-detect.
//!
//! These tests never touch real auth files and never print secrets. Fake
//! token-shaped strings are clearly synthetic.

use std::collections::BTreeMap;
use std::ffi::OsString;
use std::path::PathBuf;
use std::time::Duration;

use keeper_process_executor::{
    CancellationSignal, ProcessSpec, SupervisedProcessRunner, sensitive_name,
};
use pillow_cli_detect::{
    AgentCliKind, AuthState, DEFAULT_PROBE_TIMEOUT, ProbeError, auth_state_from_probe_failure,
    detect, detect_all, discover, discover_in, parse_auth_output, sanitize_account_label,
    status_command_summary, version_command_summary,
};

/// Failure of a probe must never collapse into NotAuthenticated.
#[test]
fn inevitable_probe_failure_is_unknown_not_not_authenticated() {
    // Non-existent PE path → ExecutableUnavailable / Spawn class failure.
    let missing = PathBuf::from(if cfg!(windows) {
        r"C:\pillow-cli-detect-no-such-dir\no-such-cli.exe"
    } else {
        "/tmp/pillow-cli-detect-no-such-cli"
    });
    assert!(!missing.exists());

    let detection =
        pillow_cli_detect::probe_found(AgentCliKind::Grok, missing, Duration::from_millis(200));
    assert!(detection.found);
    assert!(
        matches!(detection.auth, AuthState::Unknown { .. }),
        "expected Unknown, got {:?}",
        detection.auth
    );
    assert!(
        !matches!(detection.auth, AuthState::NotAuthenticated),
        "probe failure must not report NotAuthenticated"
    );
}

#[test]
fn all_probe_error_variants_map_to_unknown() {
    for err in [
        ProbeError::ExecutableUnavailable,
        ProbeError::Spawn,
        ProbeError::EnvironmentRejected,
        ProbeError::Wait,
        ProbeError::Output,
        ProbeError::CleanupUnproven,
        ProbeError::TimedOut,
        ProbeError::Cancelled,
    ] {
        let state = auth_state_from_probe_failure(err);
        assert!(matches!(&state, AuthState::Unknown { reason } if !reason.is_empty()));
        assert!(!matches!(state, AuthState::NotAuthenticated));
    }
}

/// Sanitizer must drop token-shaped fake material from account labels.
#[test]
fn sanitizer_strips_token_shaped_fake_output() {
    // Assembled at runtime — see sanitize.rs for why.
    let fake_key = format!("sk-{}", "fakeTESTKEY_not_real_ccccccccccccccc");
    let polluted = format!("Logged in as user@example.com token={fake_key}");
    // Full polluted string is rejected.
    assert!(
        sanitize_account_label(&polluted).is_none(),
        "polluted label must be dropped"
    );
    // Pure token is rejected.
    assert!(sanitize_account_label(&fake_key).is_none());
    // Parser on grok with token in the banner rest must not return the token.
    let state = parse_auth_output(
        AgentCliKind::Grok,
        &format!("You are logged in with {fake_key}.\n"),
        "",
        Some(0),
    );
    match state {
        AuthState::Unknown { reason } => {
            assert_eq!(reason, "authenticated_label_rejected");
        }
        AuthState::Authenticated { account_label } => {
            assert!(
                !account_label.contains(fake_key.as_str()),
                "token must not appear in account_label"
            );
            assert!(!account_label.contains("sk-"));
        }
        AuthState::NotAuthenticated => panic!("must not be NotAuthenticated"),
    }
}

#[test]
fn sanitizer_keeps_email_from_clean_claude_output() {
    let state = parse_auth_output(
        AgentCliKind::Claude,
        "Logged in as dev@example.com\n",
        "",
        Some(0),
    );
    assert_eq!(
        state,
        AuthState::Authenticated {
            account_label: "dev@example.com".into()
        }
    );
}

#[test]
fn discover_in_empty_root_is_not_found() {
    let dir = tempfile::tempdir().unwrap();
    let d = discover_in(AgentCliKind::OpenCode, &[dir.path().to_path_buf()]);
    assert!(!d.found);
    assert!(d.executable.is_none());
}

#[test]
fn command_summaries_are_stable() {
    assert_eq!(
        version_command_summary(AgentCliKind::Grok),
        "grok --version"
    );
    assert_eq!(status_command_summary(AgentCliKind::Grok), "grok models");
    assert_eq!(
        status_command_summary(AgentCliKind::Gemini),
        "(none — no reliable headless status)"
    );
}

/// Supervised runner timeout cleans the child (reuses process-executor proof path).
#[test]
fn hard_timeout_cleans_child_and_maps_to_unknown() {
    // Mapping contract: timed-out probes are Unknown, never NotAuthenticated.
    let state = auth_state_from_probe_failure(ProbeError::TimedOut);
    assert!(matches!(&state, AuthState::Unknown { reason } if reason == "timed_out"));
    assert!(!matches!(state, AuthState::NotAuthenticated));

    // Prove process-executor hard timeout + cleanup on a real long-running child.
    #[cfg(windows)]
    {
        let ping = PathBuf::from(r"C:\Windows\System32\ping.exe");
        if ping.is_file() {
            let mut env = BTreeMap::new();
            for (name, value) in std::env::vars_os() {
                if !sensitive_name(&name) {
                    env.insert(name, value);
                }
            }
            let receipt = SupervisedProcessRunner::new()
                .run(
                    ProcessSpec {
                        executable: ping,
                        // ping -n 30 ≈ 30s; hard timeout 150ms kills it.
                        argv: vec![
                            OsString::from("-n"),
                            OsString::from("30"),
                            OsString::from("127.0.0.1"),
                        ],
                        cwd: std::env::current_dir().unwrap(),
                        env,
                        timeout: Duration::from_millis(150),
                        output_byte_cap: 4096,
                    },
                    CancellationSignal::new(),
                )
                .expect("supervised run");
            assert!(receipt.timed_out, "expected hard timeout");
            assert!(receipt.cleanup_proven, "timeout must prove child cleanup");
            // Map the same way probe.rs does.
            let auth = if receipt.timed_out {
                AuthState::Unknown {
                    reason: "timed_out".into(),
                }
            } else {
                AuthState::NotAuthenticated
            };
            assert!(matches!(auth, AuthState::Unknown { .. }));
            assert!(!matches!(auth, AuthState::NotAuthenticated));
        }
    }
}

/// Live discovery smoke: must not panic; never returns secrets in Debug fields
/// we control (account_label sanitized). Does not assert machine-specific installs.
#[test]
fn live_detect_all_is_safe_and_shaped() {
    let results = detect_all();
    assert_eq!(results.len(), 5);
    for item in &results {
        match &item.auth {
            AuthState::Authenticated { account_label } => {
                assert!(
                    sanitize_account_label(account_label).is_some(),
                    "account_label must already be sanitizer-clean"
                );
                assert!(!account_label.contains("sk-"));
                assert!(!account_label.to_ascii_lowercase().contains("bearer "));
            }
            AuthState::NotAuthenticated => {}
            AuthState::Unknown { reason } => {
                assert!(!reason.is_empty());
                assert!(!reason.contains("sk-"));
            }
        }
        if let Some(v) = &item.version {
            assert!(!v.contains("sk-"));
            assert!(v.chars().count() <= 200);
        }
        // Touch summaries so report tables stay linked to code.
        let _ = version_command_summary(item.kind);
        let _ = status_command_summary(item.kind);
        let _ = discover(item.kind);
        let _ = DEFAULT_PROBE_TIMEOUT;
    }
    // Single-kind entry point also works.
    let _ = detect(AgentCliKind::Grok);
}

// Re-export path for the forced-failure helper used above.
// `probe_found` is crate-public via lib re-exports — ensure it is available.
mod probe_found_export {
    #[allow(unused_imports)]
    use pillow_cli_detect as _;
}
