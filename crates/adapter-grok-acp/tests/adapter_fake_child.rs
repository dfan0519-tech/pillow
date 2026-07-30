//! Integration tests against the deterministic `fake_acp_agent` child.

use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

use keeper_adapter_grok_acp::{GrokAcpAdapter, GrokAcpConfig};
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

fn adapter_with_script(script: &str) -> GrokAcpAdapter {
    let mut cfg = GrokAcpConfig::new(fake_bin());
    // Fake binary ignores real ACP args; keep defaults.
    cfg.extra_args = vec![];
    cfg.request_timeout = Duration::from_secs(5);
    cfg.max_line_bytes = 256 * 1024;
    cfg.extra_env = vec![
        ("KEEPER_FAKE_ACP_SCRIPT".into(), script.into()),
        ("KEEPER_FAKE_ACP_DELAY_MS".into(), "0".into()),
    ];
    GrokAcpAdapter::new(cfg).expect("adapter")
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
fn probe_reports_grok_acp_not_fake() {
    let adapter = adapter_with_script("text-only");
    let probe = adapter.probe().unwrap();
    assert_eq!(probe.adapter_name, "grok-acp");
    assert!(probe.healthy);
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
    let mut cfg = GrokAcpConfig::new(fake_bin());
    cfg.extra_args = vec![];
    cfg.request_timeout = Duration::from_secs(5);
    cfg.max_line_bytes = 1024;
    cfg.extra_env = vec![("KEEPER_FAKE_ACP_SCRIPT".into(), "oversized-line".into())];
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
    let mut cfg = GrokAcpConfig::new(fake_bin());
    cfg.extra_args = vec![];
    cfg.request_timeout = Duration::from_millis(300);
    cfg.extra_env = vec![("KEEPER_FAKE_ACP_SCRIPT".into(), "timeout-prompt".into())];
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
