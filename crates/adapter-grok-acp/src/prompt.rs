//! Active-prompt event loop: updates, permissions, terminal mapping.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, RecvTimeoutError};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

use chrono::Utc;
use keeper_agent_protocol::{
    EventSink, GovernedProcessDelivery, GovernedReadDelivery, GovernedWriteDelivery,
    GovernedWriteReceipt, RunId, RuntimeError, RuntimeEvent, RuntimeEventKind, SendTurn,
    WriteProposalState, WritePublicErrorCode,
};
use keeper_governance_port::{
    CancellationProbe, CheckedGovernedOutcome, GovernedExecutionPort,
    GovernedProcessExecutionError, GovernedProcessLifecycle, GovernedProcessObserver,
    GovernedReadExecutionError, GovernedWriteExecutionError, GovernedWriteLifecycle,
    GovernedWriteObserver, LifecycleObserverError,
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
use crate::provider_terminals::{
    CompletedTerminal, ProviderTerminalRequestError, TERMINAL_CREATE_METHOD, TERMINAL_KILL_METHOD,
    TERMINAL_OUTPUT_METHOD, TERMINAL_RELEASE_METHOD, TERMINAL_WAIT_METHOD,
    is_provider_terminal_method, normalize_terminal_create, normalize_terminal_reference,
};
use crate::provider_tools::{
    ProviderToolRequestError, normalize_provider_tool, normalized_result_events, provider_response,
    provider_transport_id,
};
use crate::provider_writes::{
    ProviderWriteRequestError, is_provider_write_method, normalize_provider_write,
    provider_write_response,
};
use crate::rpc::{Incoming, RpcClient, session_cancel_params, session_prompt_params};
use crate::state::{Inner, PromptOutcome, terminate_connection};
use crate::turn::ProviderRequestIdentityError;

#[allow(clippy::too_many_arguments)]
pub(crate) fn run_prompt_loop(
    inner: &Mutex<Inner>,
    config: &GrokAcpConfig,
    governance: &dyn GovernedExecutionPort,
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
                    && let Some(o) = handle_incoming(msg, inner, config, governance, sink, cancel)?
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
                    let _ = handle_incoming(msg, inner, config, governance, sink, cancel)?;
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
                        if let Some(o) =
                            handle_incoming(msg, inner, config, governance, sink, cancel)?
                        {
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
        let _ = handle_incoming(msg, inner, config, governance, sink, cancel)?;
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
    // Bounded throughout: this runs mid-turn while holding `inner`, so a hang
    // here would take the whole adapter with it, not just an exit.
    let kill_result = match guard.connection.take() {
        Some(conn) => terminate_connection(conn, Duration::from_secs(2)),
        None => Ok(()),
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
    governance: &dyn GovernedExecutionPort,
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
            handle_server_request(inner, governance, sink, cancel, id, &method, &params)?;
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
    governance: &dyn GovernedExecutionPort,
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
    if is_provider_write_method(method) {
        return handle_write_tool(inner, governance, sink, cancel, id, method, params);
    }

    // Strict process entrypoint â€” never routed through governed reads.
    if method == "pillow/process_tool" {
        return handle_process_tool(inner, governance, sink, cancel, id, params);
    }

    if is_provider_terminal_method(method) {
        return handle_terminal_request(inner, governance, sink, cancel, id, method, params);
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
    // Live cancel token: AtomicBool implements CancellationProbe (no entry snapshot).
    let cancel_probe: &dyn CancellationProbe = cancel;
    let receipt = match governance.execute_read(run_id, normalized.proposal, cancel_probe) {
        Ok(outcome) => outcome.into_receipt(),
        Err(
            GovernedReadExecutionError::InvalidProposal
            | GovernedReadExecutionError::ReservationConflict,
        ) => {
            rpc.respond_error(id, -32602, "invalid_governed_read_request")?;
            return Ok(());
        }
        Err(GovernedReadExecutionError::InProgress) => {
            rpc.respond_error(id, -32010, "governed_read_in_progress")?;
            return Ok(());
        }
        Err(
            GovernedReadExecutionError::RunNotBound
            | GovernedReadExecutionError::InvalidContext
            | GovernedReadExecutionError::Unavailable,
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

struct WriteLifecycleEmitter<'a> {
    inner: &'a Mutex<Inner>,
    sink: &'a mut dyn EventSink,
    run_id: RunId,
    requested_sequence: &'a mut Option<u64>,
}

impl GovernedWriteObserver for WriteLifecycleEmitter<'_> {
    fn observe(&mut self, event: GovernedWriteLifecycle<'_>) -> Result<(), LifecycleObserverError> {
        let GovernedWriteLifecycle::Requested(requested) = event;
        let sequence = {
            let mut guard = self.inner.lock().expect("inner lock");
            let Some(active) = guard
                .turns
                .active_mut()
                .filter(|active| active.run_id == self.run_id && active.live)
            else {
                return Err(LifecycleObserverError::RunNoLongerLive);
            };
            active.sequence = active.sequence.saturating_add(1);
            active.sequence
        };
        *self.requested_sequence = Some(sequence);
        let event = RuntimeEvent::new(
            self.run_id,
            sequence,
            RuntimeEventKind::WriteProposalRequested,
            requested.clone(),
            Utc::now(),
        )
        .map_err(|_| LifecycleObserverError::DeliveryFailed)?;
        self.sink
            .emit(event)
            .map_err(|_| LifecycleObserverError::DeliveryFailed)
    }
}

struct ProcessLifecycleEmitter<'a> {
    inner: &'a Mutex<Inner>,
    sink: &'a mut dyn EventSink,
    run_id: RunId,
    decided_emitted: &'a AtomicBool,
    started_emitted: &'a AtomicBool,
}

impl GovernedProcessObserver for ProcessLifecycleEmitter<'_> {
    fn observe(
        &mut self,
        event: GovernedProcessLifecycle<'_>,
    ) -> Result<(), LifecycleObserverError> {
        match event {
            GovernedProcessLifecycle::Requested(requested) => emit_process_lifecycle_event(
                self.inner,
                self.sink,
                self.run_id,
                RuntimeEventKind::ProcessProposalRequested,
                requested,
            ),
            GovernedProcessLifecycle::Decided(decided) => {
                emit_process_lifecycle_event(
                    self.inner,
                    self.sink,
                    self.run_id,
                    RuntimeEventKind::ProcessProposalDecided,
                    decided,
                )?;
                self.decided_emitted.store(true, Ordering::SeqCst);
                Ok(())
            }
            GovernedProcessLifecycle::Started(started) => {
                emit_process_lifecycle_event(
                    self.inner,
                    self.sink,
                    self.run_id,
                    RuntimeEventKind::ProcessStarted,
                    started,
                )?;
                self.started_emitted.store(true, Ordering::SeqCst);
                Ok(())
            }
        }
    }
}

fn emit_process_lifecycle_event(
    inner: &Mutex<Inner>,
    sink: &mut dyn EventSink,
    run_id: RunId,
    kind: RuntimeEventKind,
    payload: impl serde::Serialize,
) -> Result<(), LifecycleObserverError> {
    let sequence = {
        let mut guard = inner.lock().expect("inner lock");
        let Some(active) = guard
            .turns
            .active_mut()
            .filter(|active| active.run_id == run_id && active.live)
        else {
            return Err(LifecycleObserverError::RunNoLongerLive);
        };
        active.sequence = active.sequence.saturating_add(1);
        active.sequence
    };
    let event = RuntimeEvent::new(run_id, sequence, kind, payload, Utc::now())
        .map_err(|_| LifecycleObserverError::DeliveryFailed)?;
    sink.emit(event)
        .map_err(|_| LifecycleObserverError::DeliveryFailed)
}

fn handle_write_tool(
    inner: &Mutex<Inner>,
    governance: &dyn GovernedExecutionPort,
    sink: &mut dyn EventSink,
    cancel: &AtomicBool,
    id: serde_json::Value,
    method: &str,
    params: &serde_json::Value,
) -> Result<(), RuntimeError> {
    let (run_id, session_id, project_root, rpc) = {
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
            active.project_root.clone(),
            Arc::clone(&connection.rpc),
        )
    };

    let mut normalized = match normalize_provider_write(method, params, &session_id, &project_root)
    {
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

    if normalized.is_native_acp_write() {
        let transport_id = match provider_transport_id(&id) {
            Ok(transport_id) => transport_id,
            Err(ProviderToolRequestError::MethodNotFound) => unreachable!(),
            Err(ProviderToolRequestError::InvalidParams) => {
                rpc.respond_error(id, -32602, "invalid_governed_write_request")?;
                return Ok(());
            }
        };
        let canonical_request =
            normalized
                .canonical_request_identity()
                .map_err(|_| RuntimeError::Internal {
                    message: "provider_write_request_canonicalization_failed".into(),
                })?;
        let call_id = {
            let mut guard = inner.lock().expect("inner lock");
            let Some(active) = guard
                .turns
                .active_mut()
                .filter(|active| active.run_id == run_id && active.live)
            else {
                rpc.respond_error(id, -32000, "governed_write_unavailable")?;
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
                rpc.respond_error(id, -32000, "governed_write_unavailable")?;
                return Ok(());
            }
        }
    }

    let response_kind = normalized.response_kind;
    let mut requested_sequence = None;
    // Live cancel token: AtomicBool implements CancellationProbe (no entry snapshot).
    let cancel_probe: &dyn CancellationProbe = cancel;
    let execution = {
        let mut observer = WriteLifecycleEmitter {
            inner,
            sink,
            run_id,
            requested_sequence: &mut requested_sequence,
        };
        governance.execute_write(run_id, normalized.proposal, cancel_probe, &mut observer)
    };
    let receipt = match execution {
        Ok(outcome) => outcome.into_receipt(),
        Err(
            GovernedWriteExecutionError::InvalidProposal
            | GovernedWriteExecutionError::ReservationConflict,
        ) => {
            rpc.respond_error(id, -32602, "invalid_governed_write_request")?;
            return Ok(());
        }
        Err(GovernedWriteExecutionError::Busy) => {
            rpc.respond_error(id, -32010, "governed_write_in_progress")?;
            return Ok(());
        }
        Err(
            GovernedWriteExecutionError::RunNotBound
            | GovernedWriteExecutionError::InvalidContext
            | GovernedWriteExecutionError::NotLive
            | GovernedWriteExecutionError::ShuttingDown
            | GovernedWriteExecutionError::LifecycleDeliveryFailed
            | GovernedWriteExecutionError::Unavailable,
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

    match provider_write_response(&receipt, response_kind) {
        Ok(response) => rpc.respond(id, response),
        Err(error) => {
            let (code, message) = error.rpc_error();
            rpc.respond_error(id, code, message)
        }
    }
}

fn handle_terminal_request(
    inner: &Mutex<Inner>,
    governance: &dyn GovernedExecutionPort,
    sink: &mut dyn EventSink,
    cancel: &AtomicBool,
    id: serde_json::Value,
    method: &str,
    params: &serde_json::Value,
) -> Result<(), RuntimeError> {
    if method == TERMINAL_CREATE_METHOD {
        return handle_terminal_create(inner, governance, sink, cancel, id, params);
    }

    let (run_id, session_id, rpc) = {
        let guard = inner.lock().expect("inner lock");
        let Some(active) = guard.turns.active().filter(|active| active.live) else {
            if let Some(connection) = guard.connection.as_ref() {
                connection
                    .rpc
                    .respond_error(id, -32000, "governed_terminal_unavailable")?;
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
    let terminal_id = match normalize_terminal_reference(method, params, &session_id) {
        Ok(terminal_id) => terminal_id,
        Err(ProviderTerminalRequestError::MethodNotFound) => {
            rpc.respond_error(id, -32601, "method_not_found")?;
            return Ok(());
        }
        Err(ProviderTerminalRequestError::InvalidParams) => {
            rpc.respond_error(id, -32602, "invalid_terminal_request")?;
            return Ok(());
        }
    };

    let response = {
        let mut guard = inner.lock().expect("inner lock");
        let Some(active) = guard
            .turns
            .active_mut()
            .filter(|active| active.run_id == run_id && active.live)
        else {
            rpc.respond_error(id, -32000, "governed_terminal_unavailable")?;
            return Ok(());
        };
        match method {
            TERMINAL_OUTPUT_METHOD => active
                .terminals
                .get(&terminal_id)
                .map(CompletedTerminal::output_response),
            TERMINAL_WAIT_METHOD => active
                .terminals
                .get(&terminal_id)
                .map(CompletedTerminal::wait_response),
            // `terminal/create` returns only after this synchronous S1 governed
            // execution reaches durable terminal, so kill is an idempotent
            // acknowledgement for a known completed terminal. Turn cancel and
            // the host timeout still cancel the supervised Job while create runs.
            TERMINAL_KILL_METHOD => active
                .terminals
                .get(&terminal_id)
                .map(|_| serde_json::json!({})),
            TERMINAL_RELEASE_METHOD => active
                .terminals
                .release(&terminal_id)
                .then(|| serde_json::json!({})),
            _ => None,
        }
    };
    match response {
        Some(response) => rpc.respond(id, response),
        None => rpc.respond_error(id, -32065, "terminal_not_found"),
    }
}

fn handle_terminal_create(
    inner: &Mutex<Inner>,
    governance: &dyn GovernedExecutionPort,
    sink: &mut dyn EventSink,
    cancel: &AtomicBool,
    id: serde_json::Value,
    params: &serde_json::Value,
) -> Result<(), RuntimeError> {
    let (run_id, session_id, project_root, rpc) = {
        let guard = inner.lock().expect("inner lock");
        let Some(active) = guard.turns.active().filter(|active| active.live) else {
            if let Some(connection) = guard.connection.as_ref() {
                connection
                    .rpc
                    .respond_error(id, -32000, "governed_terminal_unavailable")?;
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
    let mut normalized =
        match normalize_terminal_create(TERMINAL_CREATE_METHOD, params, &session_id, &project_root)
        {
            Ok(normalized) => normalized,
            Err(ProviderTerminalRequestError::MethodNotFound) => {
                rpc.respond_error(id, -32601, "method_not_found")?;
                return Ok(());
            }
            Err(ProviderTerminalRequestError::InvalidParams) => {
                rpc.respond_error(id, -32602, "invalid_terminal_request")?;
                return Ok(());
            }
        };
    let transport_id = match provider_transport_id(&id) {
        Ok(transport_id) => transport_id,
        Err(_) => {
            rpc.respond_error(id, -32602, "invalid_terminal_request")?;
            return Ok(());
        }
    };
    let canonical_request =
        normalized
            .canonical_request_identity()
            .map_err(|_| RuntimeError::Internal {
                message: "terminal_request_canonicalization_failed".into(),
            })?;
    let call_id = {
        let mut guard = inner.lock().expect("inner lock");
        let Some(active) = guard
            .turns
            .active_mut()
            .filter(|active| active.run_id == run_id && active.live)
        else {
            rpc.respond_error(id, -32000, "governed_terminal_unavailable")?;
            return Ok(());
        };
        active
            .provider_requests
            .resolve(run_id, transport_id, canonical_request)
    };
    let call_id = match call_id {
        Ok(call_id) => call_id,
        Err(ProviderRequestIdentityError::Conflict) => {
            rpc.respond_error(id, -32030, "provider_request_id_reused")?;
            return Ok(());
        }
        Err(ProviderRequestIdentityError::Exhausted) => {
            rpc.respond_error(id, -32000, "governed_terminal_unavailable")?;
            return Ok(());
        }
    };

    let cached = {
        let guard = inner.lock().expect("inner lock");
        guard
            .turns
            .active()
            .filter(|active| active.run_id == run_id && active.live)
            .and_then(|active| active.terminals.by_call_id(&call_id))
            .cloned()
    };
    if let Some(cached) = cached {
        return rpc.respond(id, serde_json::json!({"terminalId": cached.terminal_id}));
    }

    normalized.proposal.call_id = call_id.clone();
    let output_byte_limit = normalized.output_byte_limit();
    let decided_emitted = AtomicBool::new(false);
    let started_emitted = AtomicBool::new(false);
    let cancel_probe: &dyn CancellationProbe = cancel;
    let execution = {
        let mut observer = ProcessLifecycleEmitter {
            inner,
            sink,
            run_id,
            decided_emitted: &decided_emitted,
            started_emitted: &started_emitted,
        };
        governance.execute_shell(run_id, normalized.proposal, cancel_probe, &mut observer)
    };
    let receipt = match execution {
        Ok(outcome) => outcome.into_receipt(),
        Err(GovernedProcessExecutionError::InvalidProposal) => {
            rpc.respond_error(id, -32602, "invalid_terminal_request")?;
            return Ok(());
        }
        Err(GovernedProcessExecutionError::ReservationConflict) => {
            rpc.respond_error(id, -32063, "governed_terminal_conflict")?;
            return Ok(());
        }
        Err(GovernedProcessExecutionError::Busy) => {
            rpc.respond_error(id, -32064, "governed_terminal_in_progress")?;
            return Ok(());
        }
        Err(
            GovernedProcessExecutionError::RunNotBound
            | GovernedProcessExecutionError::InvalidContext
            | GovernedProcessExecutionError::NotLive
            | GovernedProcessExecutionError::ShuttingDown
            | GovernedProcessExecutionError::CleanupUnproven
            | GovernedProcessExecutionError::LifecycleDeliveryFailed
            | GovernedProcessExecutionError::Unavailable,
        ) => {
            rpc.respond_error(id, -32000, "governed_terminal_unavailable")?;
            return Ok(());
        }
    };

    emit_remaining_process_events(
        inner,
        sink,
        run_id,
        &receipt.process,
        &decided_emitted,
        &started_emitted,
    )?;
    let terminal = match CompletedTerminal::from_receipt(receipt, call_id, output_byte_limit) {
        Ok(terminal) => terminal,
        Err(error) => {
            let (code, message) = error.rpc_error();
            rpc.respond_error(id, code, message)?;
            return Ok(());
        }
    };
    let terminal_id = terminal.terminal_id.clone();
    let inserted = {
        let mut guard = inner.lock().expect("inner lock");
        let Some(active) = guard
            .turns
            .active_mut()
            .filter(|active| active.run_id == run_id && active.live)
        else {
            rpc.respond_error(id, -32000, "governed_terminal_unavailable")?;
            return Ok(());
        };
        active.terminals.insert(terminal)
    };
    if let Err(error) = inserted {
        let (code, message) = error.rpc_error();
        rpc.respond_error(id, code, message)?;
        return Ok(());
    }
    rpc.respond(id, serde_json::json!({"terminalId": terminal_id}))
}

fn handle_process_tool(
    inner: &Mutex<Inner>,
    governance: &dyn GovernedExecutionPort,
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

    // Track lifecycle kinds already emitted in-flight so post-return only
    // fills remaining kinds (typically terminal). Double-emit is a failure.
    let decided_emitted = AtomicBool::new(false);
    let started_emitted = AtomicBool::new(false);
    // Live cancel token: AtomicBool implements CancellationProbe (no entry snapshot).
    let cancel_probe: &dyn CancellationProbe = cancel;
    let execution = {
        let mut observer = ProcessLifecycleEmitter {
            inner,
            sink,
            run_id,
            decided_emitted: &decided_emitted,
            started_emitted: &started_emitted,
        };
        governance.execute_process(run_id, normalized.proposal, cancel_probe, &mut observer)
    };
    let receipt = match execution {
        Ok(outcome) => outcome.into_receipt(),
        Err(GovernedProcessExecutionError::InvalidProposal) => {
            rpc.respond_error(id, -32602, "invalid_governed_process_request")?;
            return Ok(());
        }
        Err(GovernedProcessExecutionError::ReservationConflict) => {
            let (code, message) = ProviderProcessResponseError::Conflict.rpc_error();
            rpc.respond_error(id, code, message)?;
            return Ok(());
        }
        Err(GovernedProcessExecutionError::Busy) => {
            let (code, message) = ProviderProcessResponseError::Busy.rpc_error();
            rpc.respond_error(id, code, message)?;
            return Ok(());
        }
        Err(
            GovernedProcessExecutionError::RunNotBound
            | GovernedProcessExecutionError::InvalidContext
            | GovernedProcessExecutionError::NotLive
            | GovernedProcessExecutionError::ShuttingDown
            | GovernedProcessExecutionError::CleanupUnproven
            | GovernedProcessExecutionError::LifecycleDeliveryFailed
            | GovernedProcessExecutionError::Unavailable,
        ) => {
            rpc.respond_error(id, -32000, "governed_process_unavailable")?;
            return Ok(());
        }
    };

    emit_remaining_process_events(
        inner,
        sink,
        run_id,
        &receipt,
        &decided_emitted,
        &started_emitted,
    )?;

    match provider_process_response(&receipt) {
        Ok(response) => rpc.respond(id, response),
        Err(error) => {
            let (code, message) = error.rpc_error();
            rpc.respond_error(id, code, message)
        }
    }
}

fn emit_remaining_process_events(
    inner: &Mutex<Inner>,
    sink: &mut dyn EventSink,
    run_id: RunId,
    receipt: &keeper_agent_protocol::GovernedProcessReceipt,
    decided_emitted: &AtomicBool,
    started_emitted: &AtomicBool,
) -> Result<(), RuntimeError> {
    // Post-return fills only lifecycle kinds not already delivered in-flight.
    // Replays return existing terminal evidence and emit nothing again.
    if receipt.delivery != GovernedProcessDelivery::Executed {
        return Ok(());
    }
    if let Some(decided) = receipt.decided.as_ref()
        && !decided_emitted.load(Ordering::SeqCst)
    {
        emit_process_event(
            inner,
            sink,
            run_id,
            RuntimeEventKind::ProcessProposalDecided,
            decided,
        )?;
    }
    if let Some(started) = receipt.started.as_ref()
        && !started_emitted.load(Ordering::SeqCst)
    {
        emit_process_event(
            inner,
            sink,
            run_id,
            RuntimeEventKind::ProcessStarted,
            started,
        )?;
    }
    if let Some(terminal) = receipt.terminal.as_ref() {
        emit_process_event(
            inner,
            sink,
            run_id,
            RuntimeEventKind::ProcessTerminalOutcome,
            terminal,
        )?;
    }
    Ok(())
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
