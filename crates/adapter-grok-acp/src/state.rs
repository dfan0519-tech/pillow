//! Private adapter process / session / turn state.

use std::sync::Arc;
use std::time::Duration;

use keeper_agent_protocol::RuntimeError;

use crate::config::GrokAcpConfig;
use crate::error::internal;
use crate::health::{ProbeState, extract_agent_version};
use crate::map::SessionMap;
use crate::process::{ChildHandle, build_child_env, spawn_child};
use crate::rpc::{READER_EXIT_GRACE, ReaderExit, RpcClient, initialize_params};
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

/// Close the transport and terminate the child, reporting both proofs of
/// cleanup. Every wait is bounded, so this always returns.
///
/// The reader thread is the wait that used to be unbounded, and it is the one
/// this side cannot satisfy on its own. It blocks on the agent's stdout, and
/// that pipe reports end-of-input only once every handle to the write end is
/// closed. The child held one and is now gone, so a reader still parked after
/// the grace means some process the kill did not reach inherited a copy and
/// outlived the tree — descendants inherit the pipe whatever they do with their
/// own stdio. Waiting it out is not an option: the wait would last as long as
/// the leaked process does, which is what froze a desktop exit for 1045 seconds
/// until the leak was killed by hand.
///
/// So a blocked reader is reported as a cleanup failure. It is genuine evidence
/// of one, and on platforms without a job object it is the *only* evidence
/// available: `kill_tree` there kills the direct child and proves nothing about
/// its descendants. The tree-kill error wins when both fail — it names the
/// process that was supposed to die.
pub(crate) fn terminate_connection(
    mut connection: Connection,
    kill_wait: Duration,
) -> Result<(), RuntimeError> {
    connection.rpc.close();
    let killed = connection.child.kill_tree(kill_wait);
    let reader = connection.rpc.join_reader(READER_EXIT_GRACE);
    cleanup_outcome(killed, reader)
}

fn cleanup_outcome(kill: Result<(), RuntimeError>, reader: ReaderExit) -> Result<(), RuntimeError> {
    kill?;
    match reader {
        ReaderExit::Finished => Ok(()),
        ReaderExit::Blocked => Err(internal(
            "acp_reader_blocked: agent output pipe still held after process tree cleanup",
        )),
    }
}

pub(crate) fn ensure_connection(
    inner: &mut Inner,
    config: &GrokAcpConfig,
) -> Result<(), RuntimeError> {
    if let Some(conn) = inner.connection.as_mut() {
        if conn.rpc.is_closed() || conn.child.try_wait()?.is_some() {
            let dead = inner.connection.take().expect("connection");
            // Discarded either way, so the outcome has no caller here. It still
            // must not block: this runs on the way into a user's turn, so a
            // pipe holder that outlived the dead child would otherwise freeze
            // the next prompt rather than a shutdown.
            let _ = terminate_connection(dead, Duration::from_millis(500));
            // Lost transport proof until re-initialize.
            if !matches!(inner.probe_state, ProbeState::AuthenticationFailed) {
                inner.probe_state = ProbeState::Unavailable;
            }
        } else {
            return Ok(());
        }
    }

    let env = build_child_env(config)?;
    let secret_env = config.launch_secret_environment()?;
    let argv = config.launch_argv();
    let mut child = spawn_child(&config.executable, &argv, None, &env, secret_env)?;
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_finished_reader_over_a_killed_tree_is_clean_cleanup() {
        assert!(cleanup_outcome(Ok(()), ReaderExit::Finished).is_ok());
    }

    #[test]
    fn a_blocked_reader_fails_cleanup_even_when_the_tree_kill_was_proven() {
        let error = cleanup_outcome(Ok(()), ReaderExit::Blocked).expect_err("blocked reader");
        assert!(
            error.to_string().contains("acp_reader_blocked"),
            "unexpected error: {error}"
        );
    }

    #[test]
    fn the_tree_kill_failure_is_reported_when_both_proofs_fail() {
        let error = cleanup_outcome(
            Err(internal("acp_process_executor: process cleanup failed")),
            ReaderExit::Blocked,
        )
        .expect_err("failed kill");
        let message = error.to_string();
        assert!(message.contains("acp_process_executor"), "got: {message}");
        assert!(!message.contains("acp_reader_blocked"), "got: {message}");
    }
}
