//! Active-prompt event loop: updates, permissions, terminal mapping.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, RecvTimeoutError};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

use chrono::Utc;
use keeper_agent_protocol::{
    EventSink, ProcessProposalRequestedPayload, RuntimeError, RuntimeEvent, RuntimeEventKind,
    SendTurn, WriteProposalRequestedPayload, WriteProposalState, WritePublicErrorCode,
};

use crate::config::GrokAcpConfig;
use crate::error::{internal, process_exited, response_too_large};
use crate::health::{ProbeState, is_auth_failure_error};
use crate::map::{
    MappedEvent, decide_permission, map_session_update, map_stop_reason, observe_tool_update,
    terminal_for_error,
};
use crate::provider_processes::{
    ProviderProcessRequestError, ProviderProcessResponseError, normalize_provider_process,
    provider_process_response,
};
use crate::provider_tools::{
    ProviderToolRequestError, normalize_provider_tool, normalized_result_events, provider_response,
    provider_transport_id,
};
use crate::provider_writes::{
    ProviderWriteRequestError, normalize_provider_write, provider_write_response,
};
use crate::rpc::{Incoming, RpcClient, session_cancel_params, session_prompt_params};
use crate::state::{Inner, PromptOutcome};
use crate::turn::ProviderRequestIdentityError;
use keeper_runtime_host::{
    GovernedProcessBridgeError, GovernedProcessDelivery, GovernedReadBridgeError,
    GovernedReadDelivery, GovernedWriteBridgeError, GovernedWriteDelivery, GovernedWriteReceipt,
    execute_governed_process, execute_governed_read, execute_governed_write,
};

#[allow(clippy::too_many_arguments)]
pub(crate) fn run_prompt_loop(
    inner: &Mutex<Inner>,
    config: &GrokAcpConfig,
    request: &SendTurn,
    acp_session_id: &str,
    cancel: &Arc<AtomicBool>,
    rpc: Arc<RpcClient>,
    rx: mpsc::Receiver<Incoming>,
    sink: &mut dyn EventSink,
) -> Result<(), RuntimeError> {
    let params = session_prompt_params(acp_session_id, &request.content);
    let timeout = config.request_timeout;
    let rpc_for_req = Arc::clone(&rpc);
    let (done_tx, done_rx) = mpsc::channel::<Result<serde_json::Value, RuntimeError>>();
    {
        let mut guard = inner.lock().expect("inner lock");
        guard.turns.mark_live();
    }
    let req_thread = thread::Builder::new()
        .name("grok-acp-prompt".into())
        .spawn(move || {
            let result = rpc_for_req.request_timeout("session/prompt", params, timeout);
            let _ = done_tx.send(result);
        })
        .map_err(|e| internal(format!("prompt thread spawn failed: {e}")))?;

    let mut outcome: Option<PromptOutcome> = None;
    let deadline = Instant::now() + timeout + Duration::from_secs(2);
    let mut cancel_sent = false;
    let mut cancel_deadline = None;
    let mut cancel_cleanup_performed = false;
    let mut cancel_cleanup_failed = false;

    while outcome.is_none() {
        if cancel.load(Ordering::SeqCst) && !cancel_sent {
            cancel_sent = true;
            let _ = rpc.notify("session/cancel", session_cancel_params(acp_session_id));
            cancel_deadline = Some(Instant::now() + config.cancel_grace);
        }
        if cancel.load(Ordering::SeqCst)
            && cancel_deadline.is_some_and(|deadline| Instant::now() >= deadline)
        {
            cancel_cleanup_performed = true;
            outcome = Some(match terminate_after_cancel(inner) {
                Ok(()) => PromptOutcome::Err(process_exited()),
                Err(err) => {
                    cancel_cleanup_failed = true;
                    PromptOutcome::Err(err)
                }
            });
            continue;
        }

        match rx.recv_timeout(Duration::from_millis(10)) {
            Ok(msg) => {
                if !matches!(&msg, Incoming::Response { .. })
                    && let Some(o) = handle_incoming(msg, inner, config, sink, cancel)?
                {
                    match o {
                        PromptOutcome::Err(e) => {
                            if !cancel_sent {
                                cancel_sent = true;
                                let _ = rpc.notify(
                                    "session/cancel",
                                    session_cancel_params(acp_session_id),
                                );
                            }
                            outcome = Some(PromptOutcome::Err(e));
                        }
                        PromptOutcome::Stop(s) => outcome = Some(PromptOutcome::Stop(s)),
                    }
                }
            }
            Err(RecvTimeoutError::Timeout) => {}
            Err(RecvTimeoutError::Disconnected) => {
                thread::sleep(Duration::from_millis(5));
            }
        }

        match done_rx.try_recv() {
            Ok(Ok(value)) => {
                while let Ok(msg) = rx.try_recv() {
                    if matches!(msg, Incoming::Response { .. }) {
                        continue;
                    }
                    let _ = handle_incoming(msg, inner, config, sink, cancel)?;
                }
                let stop = value
                    .get("stopReason")
                    .and_then(|v| v.as_str())
                    .unwrap_or("end_turn")
                    .to_string();
                outcome = Some(PromptOutcome::Stop(stop));
            }
            Ok(Err(e)) => outcome = Some(PromptOutcome::Err(e)),
            Err(mpsc::TryRecvError::Empty) => {
                if Instant::now() >= deadline {
                    outcome = Some(PromptOutcome::Err(crate::error::timeout("session/prompt")));
                }
            }
            Err(mpsc::TryRecvError::Disconnected) => {
                if outcome.is_none() {
                    if let Ok(msg) = rx.try_recv() {
                        if let Some(o) = handle_incoming(msg, inner, config, sink, cancel)? {
                            outcome = Some(o);
                        }
                    } else {
                        outcome = Some(PromptOutcome::Err(process_exited()));
                    }
                }
            }
        }
    }

    if cancel.load(Ordering::SeqCst)
        && matches!(outcome, Some(PromptOutcome::Err(_)))
        && !cancel_cleanup_performed
    {
        cancel_cleanup_performed = true;
        if let Err(err) = terminate_after_cancel(inner) {
            cancel_cleanup_failed = true;
            outcome = Some(PromptOutcome::Err(err));
        }
    }

    let _ = req_thread.join();
    while let Ok(msg) = rx.try_recv() {
        if matches!(msg, Incoming::Response { .. }) {
            continue;
        }
        let _ = handle_incoming(msg, inner, config, sink, cancel)?;
    }
    if !cancel_cleanup_failed && let Ok(final_res) = done_rx.try_recv() {
        match final_res {
            Ok(value) => {
                let stop = value
                    .get("stopReason")
                    .and_then(|v| v.as_str())
                    .unwrap_or("end_turn")
                    .to_string();
                outcome = Some(PromptOutcome::Stop(stop));
            }
            Err(e) => {
                if !matches!(outcome, Some(PromptOutcome::Stop(_))) {
                    outcome = Some(PromptOutcome::Err(e));
                }
            }
        }
    }

    let cancelled = cancel.load(Ordering::SeqCst);
    if cancelled
        && matches!(outcome, Some(PromptOutcome::Err(_)))
        && !cancel_cleanup_performed
        && let Err(err) = terminate_after_cancel(inner)
    {
        cancel_cleanup_failed = true;
        outcome = Some(PromptOutcome::Err(err));
    }
    let mut guard = inner.lock().expect("inner lock");
    let sequence = guard.turns.active().map(|a| a.sequence).unwrap_or(1) + 1;
    let terminal = match outcome.unwrap_or(PromptOutcome::Err(process_exited())) {
        PromptOutcome::Stop(reason) => {
            update_health_after_prompt(&mut guard, &reason, cancelled, None);
            map_stop_reason(request.run_id, sequence, &reason, cancelled, Utc::now())?
        }
        PromptOutcome::Err(err) => {
            if cancelled && !cancel_cleanup_failed {
                map_stop_reason(request.run_id, sequence, "cancelled", true, Utc::now())?
            } else {
                update_health_after_prompt(&mut guard, "", false, Some(&err));
                terminal_for_error(request.run_id, sequence, &err, false, Utc::now())?
            }
        }
    };
    if let Some(active) = guard.turns.active_mut() {
        active.terminal_emitted = true;
    }
    // Host post-cancel arbitration may already have forced `run_cancelled` and
    // return `RuntimeError::Cancelled` for a duplicate terminal emit. Still
    // finish adapter cleanup so the next same-process turn is not `acp_busy`.
    match sink.emit(terminal) {
        Ok(()) => {}
        Err(RuntimeError::Cancelled) if cancelled => {}
        Err(e) => {
            guard.turns.end_prompt();
            if let Some(conn) = guard.connection.as_ref() {
                conn.rpc.set_event_sink(None);
            }
            return Err(e);
        }
    }
    guard.turns.end_prompt();
    if let Some(conn) = guard.connection.as_ref() {
        conn.rpc.set_event_sink(None);
    }
    Ok(())
}

/// A cancelled prompt without a cooperative stop must not retain the ACP slot.
fn terminate_after_cancel(inner: &Mutex<Inner>) -> Result<(), RuntimeError> {
    let mut guard = inner.lock().expect("inner lock");
    let kill_result = if let Some(mut conn) = guard.connection.take() {
        conn.rpc.close();
        let result = conn.child.kill_tree(Duration::from_secs(2));
        conn.rpc.join_reader();
        result
    } else {
        Ok(())
    };
    guard.sessions.clear();
    guard.probe_state = ProbeState::Unavailable;
    kill_result
}

/// Successful prompt establishes healthy; recognized auth failures clear it.
fn update_health_after_prompt(
    inner: &mut Inner,
    stop_reason: &str,
    cancelled: bool,
    err: Option<&RuntimeError>,
) {
    if let Some(e) = err {
        if is_auth_failure_error(e) {
            inner.probe_state = ProbeState::AuthenticationFailed;
        }
        return;
    }
    if cancelled || stop_reason.eq_ignore_ascii_case("cancelled") {
        return;
    }
    if stop_reason.eq_ignore_ascii_case("end_turn")
        || stop_reason.eq_ignore_ascii_case("max_tokens")
    {
        // Only a successful real prompt proves authentication/health.
        inner.probe_state = ProbeState::Healthy;
    }
}

fn handle_incoming(
    incoming: Incoming,
    inner: &Mutex<Inner>,
    config: &GrokAcpConfig,
    sink: &mut dyn EventSink,
    cancel: &AtomicBool,
) -> Result<Option<PromptOutcome>, RuntimeError> {
    match incoming {
        Incoming::Fatal(err) => Ok(Some(PromptOutcome::Err(err))),
        Incoming::Response { result, .. } => match result {
            Ok(value) => Ok(Some(PromptOutcome::Stop(
                value
                    .get("stopReason")
                    .and_then(|v| v.as_str())
                    .unwrap_or("end_turn")
                    .to_string(),
            ))),
            Err(e) => Ok(Some(PromptOutcome::Err(e))),
        },
        Incoming::ServerRequest { id, method, params } => {
            handle_server_request(inner, sink, cancel, id, &method, &params)?;
            Ok(None)
        }
        Incoming::Notification { method, params } => {
            if method.starts_with("_x.ai/") || method != "session/update" {
                return Ok(None);
            }
            let mut guard = inner.lock().expect("inner lock");
            on_session_update(&mut guard, config, sink, &params, cancel)
        }
    }
}

fn handle_server_request(
    inner: &Mutex<Inner>,
    sink: &mut dyn EventSink,
    cancel: &AtomicBool,
    id: serde_json::Value,
    method: &str,
    params: &serde_json::Value,
) -> Result<(), RuntimeError> {
    if method == "session/request_permission" {
        let (decision, rpc) = {
            let guard = inner.lock().expect("inner lock");
            let tools = guard.turns.active().map(|active| &active.tools);
            let decision = decide_permission(tools, params, cancel.load(Ordering::SeqCst));
            let rpc = guard
                .connection
                .as_ref()
                .map(|connection| Arc::clone(&connection.rpc));
            (decision, rpc)
        };
        if let Some(rpc) = rpc {
            rpc.respond(id, decision.result)?;
        }
        return Ok(());
    }

    // Strict write entrypoint — never routed through the read normalizer.
    if method == "pillow/write_tool" {
        return handle_write_tool(inner, sink, cancel, id, params);
    }

    // Strict process entrypoint â€” never routed through governed reads.
    if method == "pillow/process_tool" {
        return handle_process_tool(inner, sink, cancel, id, params);
    }

    let (run_id, session_id, project_root, rpc) = {
        let guard = inner.lock().expect("inner lock");
        let Some(active) = guard.turns.active().filter(|active| active.live) else {
            if let Some(connection) = guard.connection.as_ref() {
                connection
                    .rpc
                    .respond_error(id, -32000, "governed_read_unavailable")?;
            }
            return Ok(());
        };
        let Some(connection) = guard.connection.as_ref() else {
            return Ok(());
        };
        (
            active.run_id,
            active.acp_session_id.clone(),
            active.project_root.clone(),
            Arc::clone(&connection.rpc),
        )
    };

    let mut normalized = match normalize_provider_tool(method, params, &session_id, &project_root) {
        Ok(normalized) => normalized,
        Err(ProviderToolRequestError::MethodNotFound) => {
            rpc.respond_error(id, -32601, "method_not_found")?;
            return Ok(());
        }
        Err(ProviderToolRequestError::InvalidParams) => {
            rpc.respond_error(id, -32602, "invalid_governed_read_request")?;
            return Ok(());
        }
    };
    if normalized.is_native_acp_read() {
        let transport_id = match provider_transport_id(&id) {
            Ok(transport_id) => transport_id,
            Err(ProviderToolRequestError::MethodNotFound) => unreachable!(),
            Err(ProviderToolRequestError::InvalidParams) => {
                rpc.respond_error(id, -32602, "invalid_governed_read_request")?;
                return Ok(());
            }
        };
        let canonical_request =
            normalized
                .canonical_request_identity()
                .map_err(|_| RuntimeError::Internal {
                    message: "provider_request_canonicalization_failed".into(),
                })?;
        let call_id = {
            let mut guard = inner.lock().expect("inner lock");
            let Some(active) = guard
                .turns
                .active_mut()
                .filter(|active| active.run_id == run_id && active.live)
            else {
                rpc.respond_error(id, -32000, "governed_read_unavailable")?;
                return Ok(());
            };
            active
                .provider_requests
                .resolve(run_id, transport_id, canonical_request)
        };
        match call_id {
            Ok(call_id) => normalized.proposal.call_id = call_id,
            Err(ProviderRequestIdentityError::Conflict) => {
                rpc.respond_error(id, -32030, "provider_request_id_reused")?;
                return Ok(());
            }
            Err(ProviderRequestIdentityError::Exhausted) => {
                rpc.respond_error(id, -32000, "governed_read_unavailable")?;
                return Ok(());
            }
        }
    }
    let receipt = match execute_governed_read(run_id, normalized.proposal, cancel) {
        Ok(receipt) => receipt,
        Err(
            GovernedReadBridgeError::InvalidProposal | GovernedReadBridgeError::ReservationConflict,
        ) => {
            rpc.respond_error(id, -32602, "invalid_governed_read_request")?;
            return Ok(());
        }
        Err(GovernedReadBridgeError::InProgress) => {
            rpc.respond_error(id, -32010, "governed_read_in_progress")?;
            return Ok(());
        }
        Err(
            GovernedReadBridgeError::RunNotRegistered
            | GovernedReadBridgeError::InvalidContext
            | GovernedReadBridgeError::Gateway,
        ) => {
            rpc.respond_error(id, -32000, "governed_read_unavailable")?;
            return Ok(());
        }
    };
    let response = provider_response(&receipt, normalized.response_kind);

    if receipt.delivery == GovernedReadDelivery::Executed {
        let first_sequence = {
            let mut guard = inner.lock().expect("inner lock");
            let Some(active) = guard
                .turns
                .active_mut()
                .filter(|active| active.run_id == run_id && active.live)
            else {
                rpc.respond_error(id, -32000, "governed_read_unavailable")?;
                return Ok(());
            };
            let first = active.sequence.saturating_add(1);
            active.sequence = active.sequence.saturating_add(3);
            first
        };
        for event in normalized_result_events(run_id, first_sequence, &receipt)? {
            sink.emit(event)?;
        }
    }
    match response {
        Ok(response) => rpc.respond(id, response),
        Err(error) => {
            let (code, message) = error.rpc_error();
            rpc.respond_error(id, code, message)
        }
    }
}

fn handle_write_tool(
    inner: &Mutex<Inner>,
    sink: &mut dyn EventSink,
    cancel: &AtomicBool,
    id: serde_json::Value,
    params: &serde_json::Value,
) -> Result<(), RuntimeError> {
    let (run_id, session_id, rpc) = {
        let guard = inner.lock().expect("inner lock");
        let Some(active) = guard.turns.active().filter(|active| active.live) else {
            if let Some(connection) = guard.connection.as_ref() {
                connection
                    .rpc
                    .respond_error(id, -32000, "governed_write_unavailable")?;
            }
            return Ok(());
        };
        let Some(connection) = guard.connection.as_ref() else {
            return Ok(());
        };
        (
            active.run_id,
            active.acp_session_id.clone(),
            Arc::clone(&connection.rpc),
        )
    };

    let normalized = match normalize_provider_write("pillow/write_tool", params, &session_id) {
        Ok(normalized) => normalized,
        Err(ProviderWriteRequestError::MethodNotFound) => {
            rpc.respond_error(id, -32601, "method_not_found")?;
            return Ok(());
        }
        Err(ProviderWriteRequestError::InvalidParams) => {
            rpc.respond_error(id, -32602, "invalid_governed_write_request")?;
            return Ok(());
        }
    };

    let mut requested_sequence = None;
    let receipt = match execute_governed_write(
        run_id,
        normalized.proposal,
        cancel,
        |requested: &WriteProposalRequestedPayload| {
            let sequence = {
                let mut guard = inner.lock().expect("inner lock");
                let Some(active) = guard
                    .turns
                    .active_mut()
                    .filter(|active| active.run_id == run_id && active.live)
                else {
                    return Err(GovernedWriteBridgeError::RunNotRegistered);
                };
                active.sequence = active.sequence.saturating_add(1);
                active.sequence
            };
            requested_sequence = Some(sequence);
            let event = RuntimeEvent::new(
                run_id,
                sequence,
                RuntimeEventKind::WriteProposalRequested,
                requested.clone(),
                Utc::now(),
            )
            .map_err(|_| GovernedWriteBridgeError::Gateway)?;
            sink.emit(event)
                .map_err(|_| GovernedWriteBridgeError::Gateway)?;
            Ok(())
        },
    ) {
        Ok(receipt) => receipt,
        Err(
            GovernedWriteBridgeError::InvalidProposal
            | GovernedWriteBridgeError::ReservationConflict,
        ) => {
            rpc.respond_error(id, -32602, "invalid_governed_write_request")?;
            return Ok(());
        }
        Err(GovernedWriteBridgeError::Busy) => {
            rpc.respond_error(id, -32010, "governed_write_in_progress")?;
            return Ok(());
        }
        Err(
            GovernedWriteBridgeError::RunNotRegistered
            | GovernedWriteBridgeError::InvalidContext
            | GovernedWriteBridgeError::Gateway
            | GovernedWriteBridgeError::ShutDown
            | GovernedWriteBridgeError::NotLive
            | GovernedWriteBridgeError::OwnershipMismatch
            | GovernedWriteBridgeError::InvalidGrant
            | GovernedWriteBridgeError::DecisionDenied
            | GovernedWriteBridgeError::Expired
            | GovernedWriteBridgeError::Cancelled
            | GovernedWriteBridgeError::Interrupted,
        ) => {
            rpc.respond_error(id, -32000, "governed_write_unavailable")?;
            return Ok(());
        }
    };

    // Emit resolved event after terminal durable transition (when executed).
    if receipt.delivery == GovernedWriteDelivery::Executed {
        let sequence = {
            let mut guard = inner.lock().expect("inner lock");
            let Some(active) = guard
                .turns
                .active_mut()
                .filter(|active| active.run_id == run_id && active.live)
            else {
                rpc.respond_error(id, -32000, "governed_write_unavailable")?;
                return Ok(());
            };
            active.sequence = active.sequence.saturating_add(1);
            active.sequence
        };
        let _ = requested_sequence;
        let event = RuntimeEvent::new(
            run_id,
            sequence,
            RuntimeEventKind::WriteProposalResolved,
            receipt.resolved.clone(),
            Utc::now(),
        )?;
        match sink.emit(event) {
            Ok(()) => {}
            // Host cancel arbitration commits the safe write resolution, forces
            // `run_cancelled`, then returns `Cancelled` to the emitter. That is
            // an already-committed success for the resolution slot — still
            // complete the original `pillow/write_tool` with a fixed error.
            Err(RuntimeError::Cancelled)
                if cancel.load(Ordering::SeqCst) && is_safe_post_cancel_write_receipt(&receipt) => {
            }
            Err(e) => return Err(e),
        }
    }

    match provider_write_response(&receipt) {
        Ok(response) => rpc.respond(id, response),
        Err(error) => {
            let (code, message) = error.rpc_error();
            rpc.respond_error(id, code, message)
        }
    }
}

fn handle_process_tool(
    inner: &Mutex<Inner>,
    sink: &mut dyn EventSink,
    cancel: &AtomicBool,
    id: serde_json::Value,
    params: &serde_json::Value,
) -> Result<(), RuntimeError> {
    let (run_id, session_id, rpc) = {
        let guard = inner.lock().expect("inner lock");
        let Some(active) = guard.turns.active().filter(|active| active.live) else {
            if let Some(connection) = guard.connection.as_ref() {
                connection
                    .rpc
                    .respond_error(id, -32000, "governed_process_unavailable")?;
            }
            return Ok(());
        };
        let Some(connection) = guard.connection.as_ref() else {
            return Ok(());
        };
        (
            active.run_id,
            active.acp_session_id.clone(),
            Arc::clone(&connection.rpc),
        )
    };

    let normalized = match normalize_provider_process("pillow/process_tool", params, &session_id) {
        Ok(normalized) => normalized,
        Err(ProviderProcessRequestError::MethodNotFound) => {
            rpc.respond_error(id, -32601, "method_not_found")?;
            return Ok(());
        }
        Err(ProviderProcessRequestError::InvalidParams) => {
            rpc.respond_error(id, -32602, "invalid_governed_process_request")?;
            return Ok(());
        }
    };

    let receipt = match execute_governed_process(run_id, normalized.proposal, cancel, |requested| {
        let payload = ProcessProposalRequestedPayload {
            proposal_id: requested.proposal_id.clone(),
            program: requested.program.clone(),
            argv: requested.argv.clone(),
            portable_cwd: requested.portable_cwd.clone(),
            summary: requested.summary.clone(),
            risk_class: requested.risk_class.into(),
            expires_at: requested.expires_at,
        };
        emit_process_event(
            inner,
            sink,
            run_id,
            RuntimeEventKind::ProcessProposalRequested,
            payload,
        )
        .map_err(|_| GovernedProcessBridgeError::Gateway)
    }) {
        Ok(receipt) => receipt,
        Err(GovernedProcessBridgeError::InvalidProposal) => {
            rpc.respond_error(id, -32602, "invalid_governed_process_request")?;
            return Ok(());
        }
        Err(GovernedProcessBridgeError::ReservationConflict) => {
            let (code, message) = ProviderProcessResponseError::Conflict.rpc_error();
            rpc.respond_error(id, code, message)?;
            return Ok(());
        }
        Err(GovernedProcessBridgeError::Busy) => {
            let (code, message) = ProviderProcessResponseError::Busy.rpc_error();
            rpc.respond_error(id, code, message)?;
            return Ok(());
        }
        Err(
            GovernedProcessBridgeError::RunNotRegistered
            | GovernedProcessBridgeError::InvalidContext
            | GovernedProcessBridgeError::Gateway
            | GovernedProcessBridgeError::NotLive
            | GovernedProcessBridgeError::OwnershipMismatch
            | GovernedProcessBridgeError::DecisionDenied
            | GovernedProcessBridgeError::Expired
            | GovernedProcessBridgeError::Cancelled
            | GovernedProcessBridgeError::Interrupted
            | GovernedProcessBridgeError::ShutDown
            | GovernedProcessBridgeError::CleanupUnproven,
        ) => {
            rpc.respond_error(id, -32000, "governed_process_unavailable")?;
            return Ok(());
        }
    };

    // A fresh execution's durable receipt contains the post-request lifecycle.
    // Replays return existing terminal evidence and must not duplicate events.
    if receipt.delivery == GovernedProcessDelivery::Executed {
        if let Some(decided) = receipt.decided.as_ref() {
            emit_process_event(
                inner,
                sink,
                run_id,
                RuntimeEventKind::ProcessProposalDecided,
                decided,
            )?;
        }
        if let Some(started) = receipt.started.as_ref() {
            emit_process_event(
                inner,
                sink,
                run_id,
                RuntimeEventKind::ProcessStarted,
                started,
            )?;
        }
        if let Some(terminal) = receipt.terminal.as_ref() {
            // Completion is deliberately sent only after this terminal event has
            // been accepted by the durable sink.
            emit_process_event(
                inner,
                sink,
                run_id,
                RuntimeEventKind::ProcessTerminalOutcome,
                terminal,
            )?;
        }
    }

    match provider_process_response(&receipt) {
        Ok(response) => rpc.respond(id, response),
        Err(error) => {
            let (code, message) = error.rpc_error();
            rpc.respond_error(id, code, message)
        }
    }
}

fn emit_process_event(
    inner: &Mutex<Inner>,
    sink: &mut dyn EventSink,
    run_id: keeper_agent_protocol::RunId,
    kind: RuntimeEventKind,
    payload: impl serde::Serialize,
) -> Result<(), RuntimeError> {
    let sequence = {
        let mut guard = inner.lock().expect("inner lock");
        let Some(active) = guard
            .turns
            .active_mut()
            .filter(|active| active.run_id == run_id && active.live)
        else {
            return Err(RuntimeError::Internal {
                message: "governed_process_unavailable".into(),
            });
        };
        active.sequence = active.sequence.saturating_add(1);
        active.sequence
    };
    let event = RuntimeEvent::new(run_id, sequence, kind, payload, Utc::now())?;
    sink.emit(event)
}

/// Narrow post-cancel write receipt: host may already have committed this
/// `write_proposal_resolved` and forced `run_cancelled`. Only these pairs are
/// treated as safe to continue provider completion after `RuntimeError::Cancelled`.
fn is_safe_post_cancel_write_receipt(receipt: &GovernedWriteReceipt) -> bool {
    matches!(
        (receipt.resolved.state, receipt.resolved.error_code),
        (
            WriteProposalState::Cancelled,
            Some(WritePublicErrorCode::Cancelled)
        ) | (
            WriteProposalState::Interrupted,
            Some(WritePublicErrorCode::Interrupted)
        )
    )
}

fn on_session_update(
    inner: &mut Inner,
    config: &GrokAcpConfig,
    sink: &mut dyn EventSink,
    params: &serde_json::Value,
    cancel: &AtomicBool,
) -> Result<Option<PromptOutcome>, RuntimeError> {
    let Some(active) = inner.turns.active_mut() else {
        return Ok(None);
    };
    if !active.live || cancel.load(Ordering::SeqCst) {
        return Ok(None);
    }
    let session_id = params
        .get("sessionId")
        .and_then(|v| v.as_str())
        .unwrap_or("");
    if session_id != active.acp_session_id {
        return Ok(None);
    }
    let Some(update) = params.get("update") else {
        return Ok(None);
    };

    if matches!(
        update.get("sessionUpdate").and_then(|v| v.as_str()),
        Some("tool_call") | Some("tool_call_update")
    ) {
        observe_tool_update(&mut active.tools, update);
    }

    active.sequence += 1;
    let seq = active.sequence;
    let run_id = active.run_id;
    let mapped = map_session_update(run_id, seq, update, Utc::now())?;
    match mapped {
        MappedEvent::Drop => {
            active.sequence -= 1;
        }
        MappedEvent::Emit(event) => {
            if event.kind == RuntimeEventKind::AssistantDelta {
                let text = event
                    .payload
                    .get("text")
                    .and_then(|v| v.as_str())
                    .unwrap_or("");
                active.assistant_chars = active.assistant_chars.saturating_add(text.len());
                if active.assistant_chars > config.max_response_chars {
                    return Ok(Some(PromptOutcome::Err(response_too_large())));
                }
            }
            sink.emit(event)?;
        }
        MappedEvent::ToolObserve { emit, .. } => {
            if let Some(event) = emit {
                sink.emit(event)?;
            } else {
                active.sequence -= 1;
            }
        }
    }
    Ok(None)
}
