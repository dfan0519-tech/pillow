//! Private adapter process / session / turn state.

use std::sync::Arc;
use std::time::Duration;

use keeper_agent_protocol::RuntimeError;

use crate::config::GrokAcpConfig;
use crate::error::internal;
use crate::health::{ProbeState, extract_agent_version};
use crate::map::SessionMap;
use crate::process::{ChildHandle, build_child_env, spawn_child};
use crate::rpc::{RpcClient, initialize_params};
use crate::turn::TurnRegistry;

pub(crate) struct Connection {
    pub child: ChildHandle,
    pub rpc: Arc<RpcClient>,
    pub initialized: bool,
    pub load_session: bool,
}

pub(crate) struct Inner {
    pub connection: Option<Connection>,
    pub sessions: SessionMap,
    pub turns: TurnRegistry,
    /// Auth/transport health for `probe()` (never healthy from file presence alone).
    pub probe_state: ProbeState,
    /// Observed agent version from initialize `_meta.agentVersion`.
    pub agent_version: Option<String>,
}

pub(crate) enum PromptOutcome {
    Stop(String),
    Err(RuntimeError),
}

pub(crate) fn ensure_connection(
    inner: &mut Inner,
    config: &GrokAcpConfig,
) -> Result<(), RuntimeError> {
    if let Some(conn) = inner.connection.as_mut() {
        if conn.rpc.is_closed() || conn.child.try_wait()?.is_some() {
            let mut dead = inner.connection.take().expect("connection");
            dead.rpc.close();
            let _ = dead.child.kill_tree(Duration::from_millis(500));
            dead.rpc.join_reader();
            // Lost transport proof until re-initialize.
            if !matches!(inner.probe_state, ProbeState::AuthenticationFailed) {
                inner.probe_state = ProbeState::Unavailable;
            }
        } else {
            return Ok(());
        }
    }

    let env = build_child_env(config)?;
    let argv = config.launch_argv();
    let mut child = spawn_child(&config.executable, &argv, None, &env)?;
    let stdin = child
        .take_stdin()
        .ok_or_else(|| internal("child stdin missing"))?;
    let stdout = child
        .take_stdout()
        .ok_or_else(|| internal("child stdout missing"))?;
    let rpc = Arc::new(RpcClient::start(
        stdin,
        stdout,
        config.max_line_bytes,
        config.request_timeout,
    ));
    inner.connection = Some(Connection {
        child,
        rpc,
        initialized: false,
        load_session: false,
    });
    Ok(())
}

pub(crate) fn initialize_if_needed(inner: &mut Inner) -> Result<(), RuntimeError> {
    let conn = inner
        .connection
        .as_mut()
        .ok_or_else(|| internal("no ACP connection"))?;
    if conn.initialized {
        return Ok(());
    }
    let rpc = Arc::clone(&conn.rpc);
    let result = rpc.request(
        "initialize",
        initialize_params("Pillow", env!("CARGO_PKG_VERSION")),
    )?;
    let conn = inner.connection.as_mut().expect("connection");
    conn.load_session = result
        .pointer("/agentCapabilities/loadSession")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
    conn.initialized = true;

    if let Some(version) = extract_agent_version(&result) {
        inner.agent_version = Some(version);
    }

    // Initialize proves transport/protocol only — never authentication.
    if !matches!(
        inner.probe_state,
        ProbeState::Healthy | ProbeState::AuthenticationFailed
    ) {
        inner.probe_state = ProbeState::TransportReadyAuthUnverified;
    }
    Ok(())
}
