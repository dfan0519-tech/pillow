//! Active-prompt event loop: updates, permissions, terminal mapping.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, RecvTimeoutError};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

use chrono::Utc;
use keeper_agent_protocol::{EventSink, RuntimeError, RuntimeEventKind, SendTurn};

use crate::config::GrokAcpConfig;
use crate::error::{internal, process_exited, response_too_large};
use crate::map::{
    MappedEvent, decide_permission, map_session_update, map_stop_reason, observe_tool_update,
    terminal_for_error,
};
use crate::rpc::{Incoming, RpcClient, session_cancel_params, session_prompt_params};
use crate::state::{Inner, PromptOutcome};

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

    while outcome.is_none() {
        if cancel.load(Ordering::SeqCst) && !cancel_sent {
            cancel_sent = true;
            let _ = rpc.notify("session/cancel", session_cancel_params(acp_session_id));
        }

        match rx.recv_timeout(Duration::from_millis(10)) {
            Ok(msg) => {
                if !matches!(&msg, Incoming::Response { .. }) {
                    let mut guard = inner.lock().expect("inner lock");
                    if let Some(o) = handle_incoming(msg, &mut guard, config, sink, cancel)? {
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
                    let mut guard = inner.lock().expect("inner lock");
                    let _ = handle_incoming(msg, &mut guard, config, sink, cancel)?;
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
                        let mut guard = inner.lock().expect("inner lock");
                        if let Some(o) = handle_incoming(msg, &mut guard, config, sink, cancel)? {
                            outcome = Some(o);
                        }
                    } else {
                        outcome = Some(PromptOutcome::Err(process_exited()));
                    }
                }
            }
        }
    }

    let _ = req_thread.join();
    while let Ok(msg) = rx.try_recv() {
        if matches!(msg, Incoming::Response { .. }) {
            continue;
        }
        let mut guard = inner.lock().expect("inner lock");
        let _ = handle_incoming(msg, &mut guard, config, sink, cancel)?;
    }
    if let Ok(final_res) = done_rx.try_recv() {
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
    let mut guard = inner.lock().expect("inner lock");
    let sequence = guard.turns.active().map(|a| a.sequence).unwrap_or(1) + 1;
    let terminal = match outcome.unwrap_or(PromptOutcome::Err(process_exited())) {
        PromptOutcome::Stop(reason) => {
            map_stop_reason(request.run_id, sequence, &reason, cancelled, Utc::now())?
        }
        PromptOutcome::Err(err) => {
            if cancelled {
                map_stop_reason(request.run_id, sequence, "cancelled", true, Utc::now())?
            } else {
                terminal_for_error(request.run_id, sequence, &err, false, Utc::now())?
            }
        }
    };
    if let Some(active) = guard.turns.active_mut() {
        active.terminal_emitted = true;
    }
    sink.emit(terminal)?;
    guard.turns.end_prompt();
    if let Some(conn) = guard.connection.as_ref() {
        conn.rpc.set_event_sink(None);
    }
    Ok(())
}

fn handle_incoming(
    incoming: Incoming,
    inner: &mut Inner,
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
            if method == "session/request_permission" {
                let cancelled = cancel.load(Ordering::SeqCst);
                let decision = {
                    let tools = inner.turns.active().map(|a| &a.tools);
                    decide_permission(tools, &params, cancelled)
                };
                if let Some(conn) = inner.connection.as_ref() {
                    conn.rpc.respond(id, decision.result)?;
                }
            }
            Ok(None)
        }
        Incoming::Notification { method, params } => {
            if method.starts_with("_x.ai/") || method != "session/update" {
                return Ok(None);
            }
            on_session_update(inner, config, sink, &params, cancel)
        }
    }
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
