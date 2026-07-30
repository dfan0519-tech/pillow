//! Integration tests against the deterministic `fake_acp_agent` child.

use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

use keeper_adapter_grok_acp::{
    GrokAcpAdapter, GrokAcpConfig, PRODUCTION_ACP_ARGV, process_gone_for_tests,
};
use keeper_agent_protocol::{
    AgentRuntimeAdapter, EventSink, RunId, RuntimeError, RuntimeEvent, RuntimeEventKind, SendTurn,
    StartSession,
};

struct CollectingSink {
    events: Vec<RuntimeEvent>,
}

impl CollectingSink {
    fn new() -> Self {
        Self { events: Vec::new() }
    }

    fn kinds(&self) -> Vec<RuntimeEventKind> {
        self.events.iter().map(|e| e.kind).collect()
    }
}

impl EventSink for CollectingSink {
    fn emit(&mut self, event: RuntimeEvent) -> Result<(), RuntimeError> {
        // Ensure no forbidden provider keys leak.
        let dump = event.payload.to_string().to_ascii_lowercase();
        assert!(
            !dump.contains("api_key")
                && !dump.contains("authorization")
                && !dump.contains("acp_session")
                && !dump.contains("raw_provider")
                && !dump.contains("x.ai"),
            "provider field leaked: {}",
            event.payload
        );
        self.events.push(event);
        Ok(())
    }
}

fn fake_bin() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_fake_acp_agent"))
}

fn base_config(script: &str) -> GrokAcpConfig {
    let mut cfg = GrokAcpConfig::new(fake_bin()).with_test_argv(vec![]);
    cfg.request_timeout = Duration::from_secs(5);
    cfg.max_line_bytes = 256 * 1024;
    cfg.extra_env = vec![
        ("KEEPER_FAKE_ACP_SCRIPT".into(), script.into()),
        ("KEEPER_FAKE_ACP_DELAY_MS".into(), "0".into()),
    ];
    cfg
}

fn adapter_with_script(script: &str) -> GrokAcpAdapter {
    GrokAcpAdapter::new(base_config(script)).expect("adapter")
}

fn cwd() -> String {
    std::env::temp_dir().to_string_lossy().into_owned()
}

fn start(adapter: &GrokAcpAdapter) -> keeper_agent_protocol::RuntimeSession {
    adapter
        .start_session(StartSession {
            conversation_key: "conv-test".into(),
            working_directory: Some(cwd()),
        })
        .expect("start_session")
}

#[test]
fn production_argv_is_fixed() {
    assert_eq!(PRODUCTION_ACP_ARGV, &["agent", "--no-leader", "stdio"]);
    let cfg = GrokAcpConfig::new("grok");
    assert_eq!(cfg.launch_argv(), vec!["agent", "--no-leader", "stdio"]);
}

#[test]
fn probe_not_healthy_from_executable_presence_alone() {
    let adapter = adapter_with_script("text-only");
    let probe = adapter.probe().unwrap();
    assert_eq!(probe.adapter_name, "grok-acp");
    assert!(!probe.healthy, "file presence must not report healthy");
    assert_eq!(probe.detail.as_deref(), Some("unavailable"));
    // Adapter package version until initialize observes agent metadata.
    assert_eq!(probe.version, env!("CARGO_PKG_VERSION"));
    adapter.shutdown().unwrap();
}

#[test]
fn initialize_is_transport_ready_not_healthy() {
    let adapter = adapter_with_script("text-only");
    let _ = start(&adapter);
    let probe = adapter.probe().unwrap();
    assert!(!probe.healthy);
    assert_eq!(
        probe.detail.as_deref(),
        Some("transport_ready_auth_unverified")
    );
    assert_eq!(probe.version, "0.0.0-fake");
    adapter.shutdown().unwrap();
}

#[test]
fn successful_prompt_establishes_healthy() {
    let adapter = adapter_with_script("text-only");
    let session = start(&adapter);
    let run_id = RunId::new();
    adapter.arm_turn(run_id).unwrap();
    let mut sink = CollectingSink::new();
    adapter
        .send_turn(
            SendTurn {
                session_id: session.session_id,
                run_id,
                content: "hello".into(),
            },
            &mut sink,
        )
        .unwrap();
    assert_eq!(sink.kinds().last(), Some(&RuntimeEventKind::TurnCompleted));
    let probe = adapter.probe().unwrap();
    assert!(probe.healthy);
    assert_eq!(probe.detail.as_deref(), Some("healthy"));
    assert_eq!(probe.version, "0.0.0-fake");
    adapter.shutdown().unwrap();
}

#[test]
fn auth_failure_clears_healthy_with_stable_detail() {
    let adapter = adapter_with_script("auth-failed");
    let session = start(&adapter);
    // Transport ready after initialize.
    let probe = adapter.probe().unwrap();
    assert_eq!(
        probe.detail.as_deref(),
        Some("transport_ready_auth_unverified")
    );

    let run_id = RunId::new();
    let mut sink = CollectingSink::new();
    adapter
        .send_turn(
            SendTurn {
                session_id: session.session_id,
                run_id,
                content: "need auth".into(),
            },
            &mut sink,
        )
        .unwrap();
    assert_eq!(sink.kinds().last(), Some(&RuntimeEventKind::TurnFailed));
    let msg = sink.events.last().unwrap().payload["message"]
        .as_str()
        .unwrap_or("");
    assert!(
        !msg.contains("token=") && !msg.contains("Bearer") && !msg.contains("auth.json"),
        "secret-shaped text in detail/message: {msg}"
    );
    let probe = adapter.probe().unwrap();
    assert!(!probe.healthy);
    assert_eq!(probe.detail.as_deref(), Some("authentication_failed"));
    adapter.shutdown().unwrap();
}

#[test]
fn text_only_turn_maps_reasoning_and_assistant() {
    let adapter = adapter_with_script("text-only");
    let session = start(&adapter);
    let run_id = RunId::new();
    adapter.arm_turn(run_id).unwrap();
    let mut sink = CollectingSink::new();
    adapter
        .send_turn(
            SendTurn {
                session_id: session.session_id,
                run_id,
                content: "hello".into(),
            },
            &mut sink,
        )
        .unwrap();
    let kinds = sink.kinds();
    assert_eq!(kinds.first(), Some(&RuntimeEventKind::RunStarted));
    assert!(kinds.contains(&RuntimeEventKind::ReasoningSummaryDelta));
    assert!(kinds.contains(&RuntimeEventKind::AssistantDelta));
    assert_eq!(kinds.last(), Some(&RuntimeEventKind::TurnCompleted));
    // Exactly one terminal.
    assert_eq!(kinds.iter().filter(|k| k.is_terminal()).count(), 1);
    adapter.shutdown().unwrap();
}

#[test]
fn tools_and_vendor_noise_do_not_leak() {
    let adapter = adapter_with_script("with-tools");
    let session = start(&adapter);
    let run_id = RunId::new();
    let mut sink = CollectingSink::new();
    adapter
        .send_turn(
            SendTurn {
                session_id: session.session_id,
                run_id,
                content: "tools".into(),
            },
            &mut sink,
        )
        .unwrap();
    assert!(sink.kinds().contains(&RuntimeEventKind::ToolCallStarted));
    assert!(sink.kinds().contains(&RuntimeEventKind::ToolCallCompleted));
    assert_eq!(sink.kinds().last(), Some(&RuntimeEventKind::TurnCompleted));
    adapter.shutdown().unwrap();
}

#[test]
fn permission_read_allows_and_completes() {
    let adapter = adapter_with_script("permission-read");
    let session = start(&adapter);
    let run_id = RunId::new();
    let mut sink = CollectingSink::new();
    adapter
        .send_turn(
            SendTurn {
                session_id: session.session_id,
                run_id,
                content: "read".into(),
            },
            &mut sink,
        )
        .unwrap();
    assert_eq!(sink.kinds().last(), Some(&RuntimeEventKind::TurnCompleted));
    adapter.shutdown().unwrap();
}

#[test]
fn permission_write_rejected_is_cancelled_terminal() {
    let adapter = adapter_with_script("permission-write");
    let session = start(&adapter);
    let run_id = RunId::new();
    let mut sink = CollectingSink::new();
    adapter
        .send_turn(
            SendTurn {
                session_id: session.session_id,
                run_id,
                content: "write".into(),
            },
            &mut sink,
        )
        .unwrap();
    assert_eq!(sink.kinds().last(), Some(&RuntimeEventKind::RunCancelled));
    adapter.shutdown().unwrap();
}

#[test]
fn cancel_mid_stream_emits_run_cancelled() {
    let adapter = Arc::new(adapter_with_script("hang-until-cancel"));
    let session = start(&adapter);
    let run_id = RunId::new();
    adapter.arm_turn(run_id).unwrap();

    let adapter_c = Arc::clone(&adapter);
    let cancel_thread = thread::spawn(move || {
        thread::sleep(Duration::from_millis(80));
        adapter_c.cancel_turn(run_id).unwrap();
    });

    let mut sink = CollectingSink::new();
    adapter
        .send_turn(
            SendTurn {
                session_id: session.session_id,
                run_id,
                content: "long".into(),
            },
            &mut sink,
        )
        .unwrap();
    cancel_thread.join().unwrap();
    assert_eq!(sink.kinds().last(), Some(&RuntimeEventKind::RunCancelled));
    adapter.shutdown().unwrap();
}

#[test]
fn malformed_output_fails_turn() {
    let adapter = adapter_with_script("malformed");
    let session = start(&adapter);
    let run_id = RunId::new();
    let mut sink = CollectingSink::new();
    adapter
        .send_turn(
            SendTurn {
                session_id: session.session_id,
                run_id,
                content: "x".into(),
            },
            &mut sink,
        )
        .unwrap();
    assert_eq!(sink.kinds().last(), Some(&RuntimeEventKind::TurnFailed));
    adapter.shutdown().unwrap();
}

#[test]
fn oversized_line_fails_closed() {
    let mut cfg = base_config("oversized-line");
    cfg.max_line_bytes = 1024;
    let adapter = GrokAcpAdapter::new(cfg).unwrap();
    let session = start(&adapter);
    let run_id = RunId::new();
    let mut sink = CollectingSink::new();
    adapter
        .send_turn(
            SendTurn {
                session_id: session.session_id,
                run_id,
                content: "x".into(),
            },
            &mut sink,
        )
        .unwrap();
    assert_eq!(sink.kinds().last(), Some(&RuntimeEventKind::TurnFailed));
    let msg = sink.events.last().unwrap().payload["message"]
        .as_str()
        .unwrap_or("");
    assert!(
        msg.contains("acp_output_too_large") || msg.contains("too_large") || msg.contains("acp_"),
        "message={msg}"
    );
    adapter.shutdown().unwrap();
}

#[test]
fn early_child_exit_fails_turn() {
    let adapter = adapter_with_script("exit-early");
    // start_session may fail on initialize or exit.
    let result = adapter.start_session(StartSession {
        conversation_key: "c".into(),
        working_directory: Some(cwd()),
    });
    assert!(result.is_err(), "expected start failure on exit-early");
    adapter.shutdown().unwrap();
}

#[test]
fn load_replay_suppressed_until_prompt() {
    let adapter = adapter_with_script("load-replay");
    // First create a session id via new, then load with known id.
    let _ = start(&adapter);
    let loaded = adapter
        .load_session(
            StartSession {
                conversation_key: "conv-load".into(),
                working_directory: Some(cwd()),
            },
            "fake-loaded-session",
        )
        .expect("load_session");
    // Allow reader to drain load-replay notifications while not live.
    thread::sleep(Duration::from_millis(50));
    let run_id = RunId::new();
    let mut sink = CollectingSink::new();
    adapter
        .send_turn(
            SendTurn {
                session_id: loaded.session_id,
                run_id,
                content: "after load".into(),
            },
            &mut sink,
        )
        .unwrap();
    // Live text only — no "old assistant replay".
    let texts: Vec<_> = sink
        .events
        .iter()
        .filter(|e| e.kind == RuntimeEventKind::AssistantDelta)
        .filter_map(|e| {
            e.payload
                .get("text")
                .and_then(|t| t.as_str())
                .map(str::to_owned)
        })
        .collect();
    assert!(
        texts.iter().any(|t| t.contains("live-after-load")),
        "texts={texts:?}"
    );
    assert!(
        texts.iter().all(|t| !t.contains("old assistant")),
        "replay leaked: {texts:?}"
    );
    adapter.shutdown().unwrap();
}

#[test]
fn timeout_prompt_fails_or_cancels() {
    let mut cfg = base_config("timeout-prompt");
    cfg.request_timeout = Duration::from_millis(300);
    let adapter = GrokAcpAdapter::new(cfg).unwrap();
    let session = start(&adapter);
    let run_id = RunId::new();
    let mut sink = CollectingSink::new();
    adapter
        .send_turn(
            SendTurn {
                session_id: session.session_id,
                run_id,
                content: "slow".into(),
            },
            &mut sink,
        )
        .unwrap();
    let last = *sink.kinds().last().unwrap();
    assert!(
        last == RuntimeEventKind::TurnFailed || last == RuntimeEventKind::RunCancelled,
        "got {last:?}"
    );
    adapter.shutdown().unwrap();
}

#[test]
fn shutdown_cleans_up_child() {
    let adapter = adapter_with_script("text-only");
    let _ = start(&adapter);
    adapter.shutdown().unwrap();
    // Second shutdown is idempotent.
    adapter.shutdown().unwrap();
}

#[test]
fn refuses_cancel_request_method_on_rpc() {
    // Covered by RpcClient unit behavior via notify ban — exercise adapter cancel path only.
    let adapter = adapter_with_script("hang-until-cancel");
    let session = start(&adapter);
    let run_id = RunId::new();
    adapter.arm_turn(run_id).unwrap();
    let adapter = Arc::new(adapter);
    let a2 = Arc::clone(&adapter);
    let t = thread::spawn(move || {
        thread::sleep(Duration::from_millis(50));
        a2.cancel_turn(run_id).unwrap();
    });
    let mut sink = CollectingSink::new();
    adapter
        .send_turn(
            SendTurn {
                session_id: session.session_id,
                run_id,
                content: "c".into(),
            },
            &mut sink,
        )
        .unwrap();
    t.join().unwrap();
    adapter.shutdown().unwrap();
}

#[test]
fn shutdown_kills_parent_and_descendant() {
    let stamp = format!(
        "{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0)
    );
    let parent_pid_path = std::env::temp_dir().join(format!("keeper-fake-parent-{stamp}.pid"));
    let descendant_pid_path = std::env::temp_dir().join(format!("keeper-fake-desc-{stamp}.pid"));

    let mut cfg = base_config("spawn-descendant");
    cfg.extra_env.push((
        "KEEPER_FAKE_ACP_PARENT_PID_FILE".into(),
        parent_pid_path.to_string_lossy().into_owned(),
    ));
    cfg.extra_env.push((
        "KEEPER_FAKE_ACP_DESCENDANT_PID_FILE".into(),
        descendant_pid_path.to_string_lossy().into_owned(),
    ));
    let adapter = GrokAcpAdapter::new(cfg).unwrap();
    let _ = start(&adapter);

    // Wait for the fake child to record PIDs.
    let parent_pid = wait_for_pid_file(&parent_pid_path, Duration::from_secs(3));
    let descendant_pid = wait_for_pid_file(&descendant_pid_path, Duration::from_secs(3));
    assert!(
        !process_gone_for_tests(parent_pid),
        "parent should be alive before shutdown"
    );
    assert!(
        !process_gone_for_tests(descendant_pid),
        "descendant should be alive before shutdown"
    );

    adapter.shutdown().unwrap();

    // Prove tree cleanup: direct child and descendant must both be gone.
    assert!(
        process_gone_for_tests(parent_pid),
        "parent pid {parent_pid} still alive after shutdown"
    );
    assert!(
        process_gone_for_tests(descendant_pid),
        "descendant pid {descendant_pid} still alive after shutdown (job/tree kill failed)"
    );

    let _ = std::fs::remove_file(&parent_pid_path);
    let _ = std::fs::remove_file(&descendant_pid_path);
}

fn wait_for_pid_file(path: &std::path::Path, timeout: Duration) -> u32 {
    let deadline = std::time::Instant::now() + timeout;
    loop {
        if let Ok(text) = std::fs::read_to_string(path)
            && let Ok(pid) = text.trim().parse::<u32>()
            && pid > 0
        {
            return pid;
        }
        if std::time::Instant::now() >= deadline {
            panic!("timed out waiting for pid file {}", path.display());
        }
        thread::sleep(Duration::from_millis(20));
    }
}

/// Shared sink for concurrent cancel registration tests.
#[allow(dead_code)]
struct LockedSink {
    inner: Arc<Mutex<CollectingSink>>,
}

impl EventSink for LockedSink {
    fn emit(&mut self, event: RuntimeEvent) -> Result<(), RuntimeError> {
        self.inner.lock().unwrap().emit(event)
    }
}
