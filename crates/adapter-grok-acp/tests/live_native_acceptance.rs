//! Controlled live native acceptance for Grok ACP (Wave B2-H3).
//!
//! **Default behavior:** this test is `#[ignore]`. Normal
//! `cargo test --workspace` reports it as ignored (SKIP), never PASS.
//!
//! **Opt-in (controller / isolated machine only):**
//! - `KEEPER_GROK_ACP_E2E=1`
//! - `KEEPER_GROK_EXECUTABLE` — path to Grok CLI (non-secret config)
//! - `KEEPER_GROK_ISOLATED_HOME` — isolated profile home (contents never logged)
//! - `KEEPER_GROK_PROJECT_ROOT` — project working directory for session/new
//! - optional `KEEPER_GROK_ACCEPTANCE_RESULT` — redacted evidence JSON path
//!
//! Run via the Node harness:
//! `node scripts/native-acceptance-grok-acp.mjs --live`
//!
//! Safety: does not print profile contents, account identity, raw ACP
//! envelopes, tokens, or auth files. Prompts forbid tool mutation.

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::thread;
use std::time::Duration;

use keeper_adapter_grok_acp::{GrokAcpAdapter, GrokAcpConfig};
use keeper_agent_protocol::{
    AgentRuntimeAdapter, EventSink, RunId, RuntimeError, RuntimeEvent, RuntimeEventKind, SendTurn,
    StartSession,
};

const ENV_E2E: &str = "KEEPER_GROK_ACP_E2E";
const ENV_EXECUTABLE: &str = "KEEPER_GROK_EXECUTABLE";
const ENV_ISOLATED_HOME: &str = "KEEPER_GROK_ISOLATED_HOME";
const ENV_PROJECT_ROOT: &str = "KEEPER_GROK_PROJECT_ROOT";
const ENV_RESULT: &str = "KEEPER_GROK_ACCEPTANCE_RESULT";

const READ_ONLY_PROMPT: &str = "Reply with one short sentence confirming you received this message. \
Do not use any tools. Do not write, edit, delete, create, move, or execute files. \
Text response only.";

const CANCEL_PROMPT: &str = "Write a careful multi-paragraph explanation of why software tests must \
distinguish SKIP from PASS. Use several sentences. Do not use any tools. \
Do not write, edit, delete, create, move, or execute files. Text only.";

struct CollectingSink {
    events: Vec<RuntimeEvent>,
    write_permission_observed: bool,
}

impl CollectingSink {
    fn new() -> Self {
        Self {
            events: Vec::new(),
            write_permission_observed: false,
        }
    }

    fn kinds(&self) -> Vec<RuntimeEventKind> {
        self.events.iter().map(|e| e.kind).collect()
    }

    fn terminal_kinds(&self) -> Vec<RuntimeEventKind> {
        self.events
            .iter()
            .map(|e| e.kind)
            .filter(RuntimeEventKind::is_terminal)
            .collect()
    }

    fn assert_payloads_secret_free(&self) {
        for event in &self.events {
            let dump = event.payload.to_string().to_ascii_lowercase();
            assert!(
                !dump.contains("api_key")
                    && !dump.contains("authorization")
                    && !dump.contains("bearer ")
                    && !dump.contains("acp_session")
                    && !dump.contains("raw_provider")
                    && !dump.contains("\"jsonrpc\"")
                    && !dump.contains("auth.json")
                    && !dump.contains("xai_api_key")
                    && !dump.contains("password"),
                "forbidden material in normalized payload (kind={:?})",
                event.kind
            );
            // Heuristic: write tool names must not complete successfully here.
            if event.kind == RuntimeEventKind::ToolCallStarted {
                let name = event
                    .payload
                    .get("name")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_ascii_lowercase();
                if name.contains("write")
                    || name.contains("edit")
                    || name.contains("delete")
                    || name.contains("execute")
                    || name.contains("shell")
                {
                    // Observation is allowed only if later policy cancels; flag for report.
                    // We do not print the name into evidence.
                }
            }
        }
    }
}

impl EventSink for CollectingSink {
    fn emit(&mut self, event: RuntimeEvent) -> Result<(), RuntimeError> {
        if event.kind == RuntimeEventKind::ToolCallStarted {
            let name = event
                .payload
                .get("name")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_ascii_lowercase();
            if name.contains("write")
                || name.contains("edit")
                || name.contains("delete")
                || name.contains("execute")
                || name.contains("bash")
                || name.contains("shell")
            {
                self.write_permission_observed = true;
            }
        }
        self.events.push(event);
        Ok(())
    }
}

struct LiveConfig {
    executable: PathBuf,
    isolated_home: PathBuf,
    project_root: PathBuf,
    result_path: Option<PathBuf>,
}

fn env_flag_enabled(name: &str) -> bool {
    match std::env::var(name) {
        Ok(v) => {
            let t = v.trim();
            t == "1" || t.eq_ignore_ascii_case("true")
        }
        Err(_) => false,
    }
}

fn require_live_config() -> LiveConfig {
    if !env_flag_enabled(ENV_E2E) {
        panic!(
            "live test invoked without {ENV_E2E}=1; refuse PASS. \
             Use node scripts/native-acceptance-grok-acp.mjs --live with full opt-in."
        );
    }
    let executable = require_path_env(ENV_EXECUTABLE);
    let isolated_home = require_path_env(ENV_ISOLATED_HOME);
    let project_root = require_path_env(ENV_PROJECT_ROOT);

    if !executable.is_file() {
        panic!("KEEPER_GROK_EXECUTABLE is not a regular file (path not printed)");
    }
    if !isolated_home.is_dir() {
        panic!("KEEPER_GROK_ISOLATED_HOME is not a directory (path not printed)");
    }
    if !project_root.is_dir() {
        panic!("KEEPER_GROK_PROJECT_ROOT is not a directory (path not printed)");
    }

    let result_path = std::env::var(ENV_RESULT)
        .ok()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .map(PathBuf::from);

    LiveConfig {
        executable,
        isolated_home,
        project_root,
        result_path,
    }
}

fn require_path_env(name: &str) -> PathBuf {
    let value = std::env::var(name).unwrap_or_default();
    let trimmed = value.trim();
    if trimmed.is_empty() {
        panic!("missing required non-secret env {name}; refuse PASS without full opt-in");
    }
    // Reject obvious traversal / UNC without printing the path.
    if trimmed.contains("..") || trimmed.starts_with(r"\\") || trimmed.starts_with("//") {
        panic!("{name} path rejected by harness policy (path not printed)");
    }
    PathBuf::from(trimmed)
}

fn terminal_kind_name(kind: RuntimeEventKind) -> &'static str {
    match kind {
        RuntimeEventKind::TurnCompleted => "turn_completed",
        RuntimeEventKind::TurnFailed => "turn_failed",
        RuntimeEventKind::RunCancelled => "run_cancelled",
        _ => "non_terminal",
    }
}

fn write_evidence(path: &Path, body: &serde_json::Value) {
    if let Some(parent) = path.parent() {
        let _ = fs::create_dir_all(parent);
    }
    let text = serde_json::to_string_pretty(body).expect("serialize evidence");
    // Defense: refuse to write if secrets slipped into evidence.
    let lower = text.to_ascii_lowercase();
    assert!(
        !lower.contains("api_key")
            && !lower.contains("authorization")
            && !lower.contains("\"jsonrpc\"")
            && !lower.contains("auth.json")
            && !lower.contains("bearer "),
        "evidence body failed secret gate; refusing to write"
    );
    fs::write(path, format!("{text}\n")).expect("write evidence");
}

fn assert_probe_detail_secret_free(detail: Option<&str>) {
    if let Some(detail) = detail {
        let d = detail.to_ascii_lowercase();
        assert!(
            !d.contains("token") && !d.contains("api_key") && !d.contains("password"),
            "probe detail must stay secret-free"
        );
    }
}

#[test]
#[ignore = "live Grok ACP; opt-in via KEEPER_GROK_ACP_E2E=1 and path env vars"]
fn controlled_live_native_acceptance() {
    let cfg = require_live_config();

    let mut adapter_cfg = GrokAcpConfig::new(&cfg.executable);
    adapter_cfg.honor_isolated_home = Some(cfg.isolated_home.clone());
    // Production ACP argv; do not override for live.
    adapter_cfg.request_timeout = Duration::from_secs(180);
    adapter_cfg.cancel_grace = Duration::from_millis(500);

    let adapter = GrokAcpAdapter::new(adapter_cfg).expect("adapter config");

    // 1) Probe truth before initialize/session — H1: not healthy, detail=unavailable.
    // Adapter name is always grok-acp (never fake). File presence alone is not healthy.
    let probe_before = adapter.probe().expect("probe before session");
    assert_eq!(
        probe_before.adapter_name, "grok-acp",
        "live gate rejects non-grok-acp"
    );
    assert_ne!(probe_before.adapter_name, "fake");
    assert!(
        !probe_before.healthy,
        "probe must not be healthy before initialize/session"
    );
    assert_eq!(
        probe_before.detail.as_deref(),
        Some("unavailable"),
        "before session detail must be unavailable"
    );
    assert_probe_detail_secret_free(probe_before.detail.as_deref());

    // 2) session/new via start_session with project root cwd.
    let session = adapter
        .start_session(StartSession {
            conversation_key: "h3-live-native".into(),
            working_directory: Some(cfg.project_root.to_string_lossy().into_owned()),
        })
        .expect("start_session / session/new");

    // 3) Probe after session/new — H1: transport ready, auth not yet proven.
    let probe_after_session = adapter.probe().expect("probe after session/new");
    assert_eq!(probe_after_session.adapter_name, "grok-acp");
    assert!(
        !probe_after_session.healthy,
        "session/new must not claim healthy (auth unverified)"
    );
    assert_eq!(
        probe_after_session.detail.as_deref(),
        Some("transport_ready_auth_unverified"),
        "after session/new detail must be transport_ready_auth_unverified"
    );
    assert_probe_detail_secret_free(probe_after_session.detail.as_deref());

    // 4) One read-only text turn — single terminal TurnCompleted.
    let read_run = RunId::new();
    adapter.arm_turn(read_run).expect("arm read-only turn");
    let mut read_sink = CollectingSink::new();
    adapter
        .send_turn(
            SendTurn {
                session_id: session.session_id,
                run_id: read_run,
                content: READ_ONLY_PROMPT.into(),
            },
            &mut read_sink,
        )
        .expect("read-only send_turn");
    read_sink.assert_payloads_secret_free();
    let read_terminals = read_sink.terminal_kinds();
    assert_eq!(
        read_terminals.len(),
        1,
        "read-only turn must have exactly one terminal"
    );
    assert_eq!(
        read_terminals[0],
        RuntimeEventKind::TurnCompleted,
        "read-only turn expected TurnCompleted"
    );
    assert_eq!(
        read_sink.kinds().first(),
        Some(&RuntimeEventKind::RunStarted)
    );

    // 5) Probe after first successful real prompt — H1: healthy.
    let probe_after_prompt = adapter.probe().expect("probe after successful prompt");
    assert_eq!(probe_after_prompt.adapter_name, "grok-acp");
    assert!(
        probe_after_prompt.healthy,
        "first successful real prompt must establish healthy"
    );
    assert_eq!(
        probe_after_prompt.detail.as_deref(),
        Some("healthy"),
        "after successful prompt detail must be healthy"
    );
    assert_probe_detail_secret_free(probe_after_prompt.detail.as_deref());

    // 6) Cooperative cancellation — single terminal RunCancelled.
    let cancel_run = RunId::new();
    adapter.arm_turn(cancel_run).expect("arm cancel turn");
    let adapter_arc = Arc::new(adapter);
    let adapter_c = Arc::clone(&adapter_arc);
    let cancel_thread = thread::spawn(move || {
        // Cancel early so cooperative cancel wins over a short completion race.
        thread::sleep(Duration::from_millis(40));
        adapter_c.cancel_turn(cancel_run).expect("cancel_turn");
    });
    let mut cancel_sink = CollectingSink::new();
    adapter_arc
        .send_turn(
            SendTurn {
                session_id: session.session_id,
                run_id: cancel_run,
                content: CANCEL_PROMPT.into(),
            },
            &mut cancel_sink,
        )
        .expect("cancel send_turn");
    cancel_thread.join().expect("cancel thread");
    cancel_sink.assert_payloads_secret_free();
    let cancel_terminals = cancel_sink.terminal_kinds();
    assert_eq!(
        cancel_terminals.len(),
        1,
        "cancel turn must have exactly one terminal"
    );
    assert_eq!(
        cancel_terminals[0],
        RuntimeEventKind::RunCancelled,
        "cooperative cancel expected RunCancelled"
    );

    let write_observed =
        read_sink.write_permission_observed || cancel_sink.write_permission_observed;
    // Live policy must not complete write/execute tools as success for this harness.
    // If a write-like tool was merely requested and then cancelled by policy, that is
    // still a HOLD signal for this read-only gate.
    assert!(
        !write_observed,
        "write/edit/execute tool activity observed; live prompt must stay read-only"
    );

    // 7) Shutdown — kill tree; no further turns.
    adapter_arc.shutdown().expect("shutdown");

    let evidence = serde_json::json!({
        "schemaVersion": 1,
        "status": "passed",
        "adapterName": "grok-acp",
        "probe": {
            "beforeSession": {
                "healthy": false,
                "detail": "unavailable",
                "adapterName": "grok-acp",
            },
            "afterSessionNew": {
                "healthy": false,
                "detail": "transport_ready_auth_unverified",
                "adapterName": "grok-acp",
            },
            "afterFirstSuccessfulPrompt": {
                "healthy": true,
                "detail": "healthy",
                "adapterName": "grok-acp",
                // version is crate/agent version, not account identity.
                "version": probe_after_prompt.version,
            },
        },
        "sessionNew": true,
        "readOnlyTurn": {
            "terminalKind": terminal_kind_name(read_terminals[0]),
            "terminalCount": 1,
            "eventCount": read_sink.events.len(),
        },
        "cooperativeCancel": {
            "terminalKind": terminal_kind_name(cancel_terminals[0]),
            "terminalCount": 1,
            "eventCount": cancel_sink.events.len(),
        },
        "writePermissionObserved": false,
        "shutdownOk": true,
        "promptPolicy": "read_only_no_tool_mutation",
    });

    if let Some(path) = &cfg.result_path {
        write_evidence(path, &evidence);
    }
}

/// Documents that without --ignored the live test is not part of the default suite.
/// This test always runs and only asserts ignore metadata is still required conceptually
/// by checking the env gate helper stays strict.
#[test]
fn live_opt_in_gate_rejects_empty_env() {
    // Pure documentation-level guard: incomplete config must not be "complete".
    assert!(!env_flag_enabled(
        "KEEPER_GROK_ACP_E2E_UNSET_MARKER_THAT_MUST_NOT_EXIST"
    ));
}
