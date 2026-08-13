//! `GrokAcpAdapter`: `AgentRuntimeAdapter` over a supervised ACP child.

use std::fs;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use chrono::Utc;
use keeper_agent_protocol::{
    AgentRuntimeAdapter, EventSink, RunId, RunStartedPayload, RuntimeError, RuntimeEvent,
    RuntimeEventKind, RuntimeProbe, RuntimeSession, RuntimeSessionId, SendTurn, StartSession,
};
use keeper_governance_port::GovernedExecutionPort;

use crate::config::GrokAcpConfig;
use crate::error::internal;
use crate::health::ProbeState;
use crate::map::{SessionMap, map_stop_reason};
use crate::prompt::run_prompt_loop;
use crate::rpc::{Incoming, session_cancel_params, session_load_params, session_new_params};
use crate::state::{Inner, ensure_connection, initialize_if_needed, terminate_connection};
use crate::turn::TurnRegistry;

/// Grok Build ACP adapter. Provider wire state stays private to this type.
pub struct GrokAcpAdapter {
    config: GrokAcpConfig,
    governance: Arc<dyn GovernedExecutionPort>,
    /// ACP has one global event sink and accepts one prompt at a time.
    operation_gate: Mutex<()>,
    inner: Mutex<Inner>,
    shut_down: AtomicBool,
}

impl GrokAcpAdapter {
    pub fn new(
        config: GrokAcpConfig,
        governance: Arc<dyn GovernedExecutionPort>,
    ) -> Result<Self, RuntimeError> {
        config.validate()?;
        Ok(Self {
            config,
            governance,
            operation_gate: Mutex::new(()),
            inner: Mutex::new(Inner {
                connection: None,
                sessions: SessionMap::default(),
                turns: TurnRegistry::default(),
                probe_state: ProbeState::Unavailable,
                agent_version: None,
            }),
            shut_down: AtomicBool::new(false),
        })
    }

    /// Non-secret PID for controlled acceptance evidence while the provider is owned.
    #[doc(hidden)]
    pub fn owned_provider_root_pid_for_acceptance(&self) -> Option<u32> {
        let inner = self.inner.lock().ok()?;
        inner
            .connection
            .as_ref()
            .map(|connection| connection.child.pid())
    }

    fn ensure_alive(&self) -> Result<(), RuntimeError> {
        if self.shut_down.load(Ordering::SeqCst) {
            Err(RuntimeError::ShutDown)
        } else {
            Ok(())
        }
    }

    /// Load an existing ACP session. Replay updates are ignored until a prompt.
    pub fn load_session(
        &self,
        request: StartSession,
        acp_session_id: &str,
    ) -> Result<RuntimeSession, RuntimeError> {
        self.ensure_alive()?;
        let _operation = self.operation_gate.lock().expect("operation gate");
        let cwd = canonical_project_root(
            request
                .working_directory
                .clone()
                .ok_or_else(|| internal("working_directory is required for session/load"))?,
        )?;
        let mut inner = self.inner.lock().expect("inner lock");
        ensure_connection(&mut inner, &self.config)?;
        initialize_if_needed(&mut inner)?;
        let conn = inner.connection.as_ref().expect("connection");
        if !conn.load_session {
            return Err(internal("acp_load_unsupported"));
        }
        conn.rpc.set_event_sink(None);
        let rpc = Arc::clone(&conn.rpc);
        let _ = rpc.request("session/load", session_load_params(acp_session_id, &cwd))?;
        let runtime_id = RuntimeSessionId::new();
        inner.sessions.insert(
            runtime_id,
            request.conversation_key.clone(),
            acp_session_id.to_string(),
            cwd,
        );
        Ok(RuntimeSession {
            session_id: runtime_id,
            conversation_key: request.conversation_key,
        })
    }

    fn emit_started(
        run_id: RunId,
        content: &str,
        sequence: u64,
        sink: &mut dyn EventSink,
    ) -> Result<(), RuntimeError> {
        let preview: String = content.chars().take(120).collect();
        let event = RuntimeEvent::new(
            run_id,
            sequence,
            RuntimeEventKind::RunStarted,
            RunStartedPayload {
                prompt_preview: Some(preview),
            },
            Utc::now(),
        )?;
        sink.emit(event)
    }

    fn build_probe(inner: &Inner, executable_present: bool) -> RuntimeProbe {
        let state = if !executable_present {
            ProbeState::Unavailable
        } else {
            inner.probe_state
        };
        let version = inner
            .agent_version
            .clone()
            .unwrap_or_else(|| env!("CARGO_PKG_VERSION").into());
        RuntimeProbe {
            healthy: state.is_healthy(),
            adapter_name: "grok-acp".into(),
            version,
            detail: Some(state.detail_code().into()),
        }
    }
}

impl AgentRuntimeAdapter for GrokAcpAdapter {
    fn probe(&self) -> Result<RuntimeProbe, RuntimeError> {
        self.ensure_alive()?;
        let executable_present = self.config.executable.is_file();
        let inner = self.inner.lock().expect("inner lock");
        Ok(Self::build_probe(&inner, executable_present))
    }

    fn start_session(&self, request: StartSession) -> Result<RuntimeSession, RuntimeError> {
        self.ensure_alive()?;
        let _operation = self.operation_gate.lock().expect("operation gate");
        let cwd = canonical_project_root(
            request
                .working_directory
                .clone()
                .ok_or_else(|| internal("working_directory is required for Grok ACP sessions"))?,
        )?;
        let mut inner = self.inner.lock().expect("inner lock");
        ensure_connection(&mut inner, &self.config)?;
        initialize_if_needed(&mut inner)?;
        let conn = inner.connection.as_ref().expect("connection");
        conn.rpc.set_event_sink(None);
        let rpc = Arc::clone(&conn.rpc);
        let result = rpc.request("session/new", session_new_params(&cwd))?;
        let acp_session_id = result
            .get("sessionId")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .trim()
            .to_string();
        if acp_session_id.is_empty() {
            return Err(internal("acp_session_missing"));
        }
        let runtime_id = RuntimeSessionId::new();
        inner.sessions.insert(
            runtime_id,
            request.conversation_key.clone(),
            acp_session_id,
            cwd,
        );
        Ok(RuntimeSession {
            session_id: runtime_id,
            conversation_key: request.conversation_key,
        })
    }

    fn arm_turn(&self, run_id: RunId) -> Result<(), RuntimeError> {
        self.ensure_alive()?;
        self.inner.lock().expect("inner lock").turns.arm(run_id)
    }

    fn disarm_turn(&self, run_id: RunId) -> Result<(), RuntimeError> {
        self.inner.lock().expect("inner lock").turns.disarm(run_id);
        Ok(())
    }

    fn send_turn(&self, request: SendTurn, sink: &mut dyn EventSink) -> Result<(), RuntimeError> {
        self.ensure_alive()?;
        let _operation = self.operation_gate.lock().expect("operation gate");

        let (acp_session_id, cancel, rpc, rx) = {
            let mut inner = self.inner.lock().expect("inner lock");
            let record = inner
                .sessions
                .get(&request.session_id)
                .ok_or(RuntimeError::SessionNotFound {
                    session_id: request.session_id,
                })?
                .clone();

            let cancel = inner.turns.take_cancel_token(request.run_id);
            if cancel.load(Ordering::SeqCst) {
                let event = map_stop_reason(request.run_id, 1, "cancelled", true, Utc::now())?;
                sink.emit(event)?;
                return Ok(());
            }

            ensure_connection(&mut inner, &self.config)?;
            initialize_if_needed(&mut inner)?;

            let (tx, rx) = mpsc::channel::<Incoming>();
            let conn = inner.connection.as_mut().expect("connection");
            conn.rpc.set_event_sink(Some(tx));
            let rpc = Arc::clone(&conn.rpc);

            inner.turns.begin_prompt(
                request.run_id,
                record.acp_session_id.clone(),
                record.working_directory.clone(),
                Arc::clone(&cancel),
            )?;
            {
                let active = inner.turns.active_mut().expect("active");
                active.sequence = 1;
            }
            Self::emit_started(request.run_id, &request.content, 1, sink)?;

            (record.acp_session_id, cancel, rpc, rx)
        };

        run_prompt_loop(
            &self.inner,
            &self.config,
            self.governance.as_ref(),
            &request,
            &acp_session_id,
            &cancel,
            rpc,
            rx,
            sink,
        )
    }

    fn cancel_turn(&self, run_id: RunId) -> Result<(), RuntimeError> {
        self.ensure_alive()?;
        let mut inner = self.inner.lock().expect("inner lock");
        let _ = inner.turns.cancel(run_id)?;
        if let Some(active) = inner.turns.active()
            && active.run_id == run_id
        {
            let sid = active.acp_session_id.clone();
            if let Some(conn) = inner.connection.as_ref() {
                // Cooperative path only — never send `$/cancel_request`.
                let _ = conn
                    .rpc
                    .notify("session/cancel", session_cancel_params(&sid));
            }
        }
        Ok(())
    }

    /// Terminate the agent, and always return.
    ///
    /// Every wait here is bounded, including the one for the reader thread. An
    /// unbounded reader join used to be the exception, and it could not be
    /// satisfied by anything this side does: it ends when the agent's stdout
    /// pipe reaches end-of-input, which needs a *third* party — a leaked
    /// descendant holding an inherited copy of the write end — to let go first.
    /// A desktop exit runs this on the event loop, so blocking here freezes the
    /// window and never reaches the shell's fail-closed branch.
    fn shutdown(&self) -> Result<(), RuntimeError> {
        if self.shut_down.swap(true, Ordering::SeqCst) {
            return Ok(());
        }
        let mut inner = self.inner.lock().expect("inner lock");
        inner.turns.cancel_all();
        if let Some(active) = inner.turns.active() {
            let sid = active.acp_session_id.clone();
            if let Some(conn) = inner.connection.as_ref() {
                let _ = conn
                    .rpc
                    .notify("session/cancel", session_cancel_params(&sid));
            }
        }
        // Shutdown fails if process-tree cleanup cannot be proved.
        let kill_result = match inner.connection.take() {
            Some(conn) => terminate_connection(conn, Duration::from_secs(2)),
            None => Ok(()),
        };
        inner.sessions.clear();
        inner.turns.clear();
        if !matches!(inner.probe_state, ProbeState::AuthenticationFailed) {
            inner.probe_state = ProbeState::Unavailable;
        }
        kill_result
    }
}

fn canonical_project_root(path: String) -> Result<String, RuntimeError> {
    let canonical = fs::canonicalize(&path)
        .map_err(|_| internal("working_directory must be an existing project directory"))?;
    if !canonical.is_dir() {
        return Err(internal(
            "working_directory must be an existing project directory",
        ));
    }
    canonical
        .to_str()
        .map(str::to_owned)
        .ok_or_else(|| internal("working_directory is not valid UTF-8"))
}
