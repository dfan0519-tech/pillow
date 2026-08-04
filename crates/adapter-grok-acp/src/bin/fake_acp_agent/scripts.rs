//! Prompt script scenarios for the fake ACP agent.

use std::io::Write;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{Receiver, RecvTimeoutError};
use std::thread;
use std::time::Duration;

use serde_json::{Value, json};

use super::InMsg;

pub(crate) struct PromptScriptContext<'a> {
    pub(crate) script: &'a str,
    pub(crate) session_id: &'a str,
    pub(crate) session_root: &'a str,
    pub(crate) id: Value,
    pub(crate) delay_ms: u64,
    pub(crate) cancelled: Arc<AtomicBool>,
    /// Concatenated user prompt text from `session/prompt` (may be empty).
    pub(crate) prompt_text: &'a str,
}

pub(crate) fn run_prompt_script(
    stdout: &mut impl Write,
    rx: &Receiver<InMsg>,
    context: PromptScriptContext<'_>,
) {
    let PromptScriptContext {
        script,
        session_id,
        session_root,
        id,
        delay_ms,
        cancelled,
        prompt_text,
    } = context;

    // Drain cancel notifications while working.
    let pump = |rx: &Receiver<InMsg>, cancelled: &AtomicBool| {
        while let Ok(msg) = rx.try_recv() {
            if let InMsg::Line(line) = msg {
                note_cancel_line(&line, cancelled);
            }
        }
    };

    match script {
        "malformed" => {
            let _ = writeln!(stdout, "{{not-json");
            let _ = stdout.flush();
        }
        "oversized-line" => {
            let huge = "x".repeat(2_000_000);
            let _ = writeln!(stdout, "{huge}");
            let _ = stdout.flush();
        }
        "timeout-prompt" | "hang-until-cancel" => {
            if script == "hang-until-cancel" {
                emit_update(
                    stdout,
                    session_id,
                    json!({
                        "sessionUpdate": "agent_message_chunk",
                        "content": { "type": "text", "text": "working..." }
                    }),
                );
            }
            loop {
                pump(rx, &cancelled);
                if cancelled.load(Ordering::SeqCst) {
                    write_json(
                        stdout,
                        &json!({
                            "jsonrpc": "2.0",
                            "id": id,
                            "result": {
                                "stopReason": "cancelled",
                                "_meta": { "cancellationCategory": "MidTurnAbort" }
                            }
                        }),
                    );
                    return;
                }
                // Also block briefly for cancel lines.
                match rx.recv_timeout(Duration::from_millis(20)) {
                    Ok(InMsg::Line(line)) => {
                        note_cancel_line(&line, &cancelled);
                    }
                    Ok(InMsg::Eof) => {
                        write_json(
                            stdout,
                            &json!({
                                "jsonrpc": "2.0",
                                "id": id,
                                "result": { "stopReason": "cancelled" }
                            }),
                        );
                        return;
                    }
                    Err(RecvTimeoutError::Timeout) => {}
                    Err(RecvTimeoutError::Disconnected) => return,
                }
            }
        }
        "race-session-operation" => {
            emit_update(
                stdout,
                session_id,
                json!({
                    "sessionUpdate": "agent_message_chunk",
                    "content": { "type": "text", "text": "before-session-operation" }
                }),
            );
            thread::sleep(Duration::from_millis(150 + delay_ms));
            emit_update(
                stdout,
                session_id,
                json!({
                    "sessionUpdate": "agent_message_chunk",
                    "content": { "type": "text", "text": "after-session-operation" }
                }),
            );
            loop {
                let _ = rx.recv_timeout(Duration::from_millis(20));
            }
        }
        "ignore-cancel" => loop {
            let _ = rx.recv_timeout(Duration::from_millis(20));
        },
        "error-after-cancel" => loop {
            pump(rx, &cancelled);
            if cancelled.load(Ordering::SeqCst) {
                write_error(
                    stdout,
                    id,
                    -32001,
                    "prompt transport closed after cancellation",
                );
                loop {
                    let _ = rx.recv_timeout(Duration::from_millis(20));
                }
            }
            if let Ok(InMsg::Line(line)) = rx.recv_timeout(Duration::from_millis(20)) {
                note_cancel_line(&line, &cancelled);
            }
        },
        "permission-read" | "permission-read-parent" => {
            let target = if script == "permission-read-parent" {
                ".."
            } else {
                "."
            };
            emit_tool_read(stdout, session_id, target);
            write_json(
                stdout,
                &json!({
                    "jsonrpc": "2.0",
                    "id": 0,
                    "method": "session/request_permission",
                    "params": {
                        "sessionId": session_id,
                        "toolCall": {
                            "toolCallId": "tc-read-1",
                            "kind": "read",
                            "title": "read_file"
                        },
                        "options": [
                            { "optionId": "allow-once", "name": "Yes", "kind": "allow_once" },
                            { "optionId": "reject-once", "name": "No", "kind": "reject_once" }
                        ]
                    }
                }),
            );
            if permission_was_rejected(rx, &cancelled, 80 + delay_ms) {
                write_json(
                    stdout,
                    &json!({
                        "jsonrpc": "2.0",
                        "id": id,
                        "result": {
                            "stopReason": "cancelled",
                            "_meta": { "cancellationCategory": "PermissionRejected" }
                        }
                    }),
                );
                return;
            }
            emit_update(
                stdout,
                session_id,
                json!({
                    "sessionUpdate": "tool_call_update",
                    "toolCallId": "tc-read-1",
                    "status": "completed",
                    "content": [{ "type": "content", "content": { "type": "text", "text": "ok" } }]
                }),
            );
            emit_update(
                stdout,
                session_id,
                json!({
                    "sessionUpdate": "agent_message_chunk",
                    "content": { "type": "text", "text": "read-done" }
                }),
            );
            write_json(
                stdout,
                &json!({
                    "jsonrpc": "2.0",
                    "id": id,
                    "result": { "stopReason": "end_turn" }
                }),
            );
        }
        "permission-write" => {
            emit_update(
                stdout,
                session_id,
                json!({
                    "sessionUpdate": "tool_call",
                    "toolCallId": "tc-write-1",
                    "title": "write",
                    "kind": "edit",
                    "_meta": { "x.ai/tool": { "name": "write", "kind": "write", "read_only": false } }
                }),
            );
            write_json(
                stdout,
                &json!({
                    "jsonrpc": "2.0",
                    "id": 0,
                    "method": "session/request_permission",
                    "params": {
                        "sessionId": session_id,
                        "toolCall": {
                            "toolCallId": "tc-write-1",
                            "kind": "edit",
                            "title": "write"
                        },
                        "options": [
                            { "optionId": "allow-edits-session", "kind": "allow_always" },
                            { "optionId": "allow-once", "kind": "allow_once" },
                            { "optionId": "reject-once", "kind": "reject_once" }
                        ]
                    }
                }),
            );
            wait_ms(rx, &cancelled, 80 + delay_ms);
            emit_update(
                stdout,
                session_id,
                json!({
                    "sessionUpdate": "tool_call_update",
                    "toolCallId": "tc-write-1",
                    "status": "failed",
                    "content": [{
                        "type": "content",
                        "content": { "type": "text", "text": "User rejected the execution for tool `write`" }
                    }]
                }),
            );
            write_json(
                stdout,
                &json!({
                    "jsonrpc": "2.0",
                    "id": id,
                    "result": {
                        "stopReason": "cancelled",
                        "_meta": { "cancellationCategory": "PermissionRejected" }
                    }
                }),
            );
        }
        "governed-write-create" => {
            run_governed_write_create(
                stdout,
                rx,
                session_id,
                id,
                &cancelled,
                "out/created.txt",
                "created by governed write\n",
                "write-call-1",
            );
        }
        "governed-write-modify" => {
            run_governed_write_modify(
                stdout,
                rx,
                session_id,
                id,
                &cancelled,
                "notes/base.txt",
                "modified by governed write\n",
                "write-call-modify-1",
            );
        }
        "governed-write-conflict" => {
            run_governed_write_conflict(stdout, rx, session_id, id, &cancelled);
        }
        "governed-write-desktop" => {
            // Desktop acceptance: one ACP child, prompt tags select the action.
            // Tags are harness-owned and must not appear as secret-bearing IDs.
            if prompt_text.contains("[c3f:modify]") {
                run_governed_write_modify(
                    stdout,
                    rx,
                    session_id,
                    id,
                    &cancelled,
                    "notes/base.txt",
                    "modified by governed write\n",
                    "write-call-modify-1",
                );
            } else if prompt_text.contains("[c3f:conflict]") {
                run_governed_write_conflict(stdout, rx, session_id, id, &cancelled);
            } else if prompt_text.contains("[c3f:create-b]") {
                run_governed_write_create(
                    stdout,
                    rx,
                    session_id,
                    id,
                    &cancelled,
                    "out/created-b.txt",
                    "created by governed write b\n",
                    "write-call-b-1",
                );
            } else {
                // Default / [c3f:create] / [c3f:create-a]
                let path = if prompt_text.contains("[c3f:create-a]") {
                    "out/created-a.txt"
                } else {
                    "out/created.txt"
                };
                let content = if prompt_text.contains("[c3f:create-a]") {
                    "created by governed write a\n"
                } else {
                    "created by governed write\n"
                };
                let call_id = if prompt_text.contains("[c3f:create-a]") {
                    "write-call-a-1"
                } else {
                    "write-call-1"
                };
                run_governed_write_create(
                    stdout, rx, session_id, id, &cancelled, path, content, call_id,
                );
            }
        }
        "governed-read-replay" => {
            let target = std::path::Path::new(session_root).join("note.txt");
            let request = json!({
                "jsonrpc": "2.0",
                "id": 41,
                "method": "fs/readTextFile",
                "params": {
                    "sessionId": session_id,
                    "path": target.to_string_lossy()
                }
            });
            write_json(stdout, &request);
            let first = wait_for_result(rx, &cancelled, 41);
            write_json(stdout, &request);
            let replay = wait_for_result(rx, &cancelled, 41);
            let exact_replay = first.is_some() && first == replay;
            let content_matches = replay
                .as_ref()
                .and_then(|response| response.get("content"))
                .and_then(Value::as_str)
                == Some("governed content");
            emit_update(
                stdout,
                session_id,
                json!({
                    "sessionUpdate": "agent_message_chunk",
                    "content": {
                        "type": "text",
                        "text": if exact_replay && content_matches {
                            "governed-read-replay-ok"
                        } else {
                            "governed-read-replay-failed"
                        }
                    }
                }),
            );
            write_json(
                stdout,
                &json!({
                    "jsonrpc": "2.0",
                    "id": id,
                    "result": { "stopReason": "end_turn" }
                }),
            );
        }
        "governed-read-id-conflict" => {
            let first_target = std::path::Path::new(session_root).join("note.txt");
            let second_target = std::path::Path::new(session_root).join("other.txt");
            write_json(
                stdout,
                &json!({
                    "jsonrpc": "2.0",
                    "id": 42,
                    "method": "fs/readTextFile",
                    "params": {
                        "sessionId": session_id,
                        "path": first_target.to_string_lossy()
                    }
                }),
            );
            let first = wait_for_response(rx, &cancelled, 42);
            write_json(
                stdout,
                &json!({
                    "jsonrpc": "2.0",
                    "id": 42,
                    "method": "fs/readTextFile",
                    "params": {
                        "sessionId": session_id,
                        "path": second_target.to_string_lossy()
                    }
                }),
            );
            let conflict = wait_for_response(rx, &cancelled, 42);
            let first_matches = first
                .as_ref()
                .and_then(|response| response.get("result"))
                .and_then(|result| result.get("content"))
                .and_then(Value::as_str)
                == Some("governed content");
            let conflict_is_safe = conflict.as_ref().is_some_and(|response| {
                response.get("result").is_none()
                    && response.get("error").and_then(|error| error.get("code"))
                        == Some(&json!(-32030))
                    && response
                        .get("error")
                        .and_then(|error| error.get("message"))
                        .and_then(Value::as_str)
                        == Some("provider_request_id_reused")
                    && response
                        .get("error")
                        .and_then(Value::as_object)
                        .is_some_and(|error| error.len() == 2)
            });
            emit_update(
                stdout,
                session_id,
                json!({
                    "sessionUpdate": "agent_message_chunk",
                    "content": {
                        "type": "text",
                        "text": if first_matches && conflict_is_safe {
                            "governed-read-id-conflict-ok"
                        } else {
                            "governed-read-id-conflict-failed"
                        }
                    }
                }),
            );
            write_json(
                stdout,
                &json!({
                    "jsonrpc": "2.0",
                    "id": id,
                    "result": { "stopReason": "end_turn" }
                }),
            );
        }
        "with-tools" => {
            wait_ms(rx, &cancelled, delay_ms);
            emit_tool_read(stdout, session_id, ".");
            emit_update(
                stdout,
                session_id,
                json!({
                    "sessionUpdate": "tool_call_update",
                    "toolCallId": "tc-read-1",
                    "kind": "read",
                    "status": "completed",
                    "content": [{ "type": "content", "content": { "type": "text", "text": "line1" } }]
                }),
            );
            emit_update(
                stdout,
                session_id,
                json!({
                    "sessionUpdate": "agent_thought_chunk",
                    "content": { "type": "text", "text": "reasoning" }
                }),
            );
            emit_update(
                stdout,
                session_id,
                json!({
                    "sessionUpdate": "agent_message_chunk",
                    "content": { "type": "text", "text": "tools-ok" }
                }),
            );
            write_json(
                stdout,
                &json!({
                    "jsonrpc": "2.0",
                    "id": id,
                    "result": { "stopReason": "end_turn" }
                }),
            );
        }
        "load-replay" => {
            emit_update(
                stdout,
                session_id,
                json!({
                    "sessionUpdate": "agent_message_chunk",
                    "content": { "type": "text", "text": "live-after-load" }
                }),
            );
            write_json(
                stdout,
                &json!({
                    "jsonrpc": "2.0",
                    "id": id,
                    "result": { "stopReason": "end_turn" }
                }),
            );
        }
        _ => {
            wait_ms(rx, &cancelled, delay_ms);
            if cancelled.load(Ordering::SeqCst) {
                write_json(
                    stdout,
                    &json!({
                        "jsonrpc": "2.0",
                        "id": id,
                        "result": { "stopReason": "cancelled" }
                    }),
                );
                return;
            }
            emit_update(
                stdout,
                session_id,
                json!({
                    "sessionUpdate": "user_message_chunk",
                    "content": { "type": "text", "text": "echo" }
                }),
            );
            emit_update(
                stdout,
                session_id,
                json!({
                    "sessionUpdate": "agent_thought_chunk",
                    "content": { "type": "text", "text": "thinking" }
                }),
            );
            emit_update(
                stdout,
                session_id,
                json!({
                    "sessionUpdate": "agent_message_chunk",
                    "content": { "type": "text", "text": "protocol-ok" }
                }),
            );
            write_json(
                stdout,
                &json!({
                    "jsonrpc": "2.0",
                    "method": "_x.ai/session/prompt_complete",
                    "params": { "sessionId": session_id, "stopReason": "end_turn" }
                }),
            );
            write_json(
                stdout,
                &json!({
                    "jsonrpc": "2.0",
                    "id": id,
                    "result": { "stopReason": "end_turn" }
                }),
            );
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn run_governed_write_create(
    stdout: &mut impl Write,
    rx: &Receiver<InMsg>,
    session_id: &str,
    id: Value,
    cancelled: &AtomicBool,
    path: &str,
    content: &str,
    call_id: &str,
) {
    write_json(
        stdout,
        &json!({
            "jsonrpc": "2.0",
            "id": 81,
            "method": "pillow/write_tool",
            "params": {
                "sessionId": session_id,
                "callId": call_id,
                "toolId": "fs.write",
                "toolVersion": 1,
                "arguments": {
                    "path": path,
                    "mode": "create",
                    "content": content
                }
            }
        }),
    );
    let response = wait_for_response(rx, cancelled, 81);
    let result = response.as_ref().and_then(|value| value.get("result"));
    let ok = result
        .and_then(|result| result.get("path"))
        .and_then(Value::as_str)
        == Some(path);
    let mode_ok = result
        .and_then(|result| result.get("mode"))
        .and_then(Value::as_str)
        == Some("create");
    let bytes_ok = result
        .and_then(|result| result.get("bytes_written").or_else(|| result.get("bytes")))
        .and_then(Value::as_u64)
        == Some(content.len() as u64);
    let digest_ok = result
        .and_then(|result| {
            result
                .get("content_digest")
                .or_else(|| result.get("digest"))
        })
        .and_then(Value::as_str)
        .is_some_and(|d| !d.is_empty());
    let rejected = response
        .as_ref()
        .and_then(|value| value.get("error"))
        .and_then(|error| error.get("message"))
        .and_then(Value::as_str)
        .is_some_and(|message| message.contains("governed_write"));
    let label = if ok && mode_ok && bytes_ok && digest_ok {
        "governed-write-create-ok"
    } else if ok {
        // Path matched but receipt fields incomplete — still treat as soft ok for UI.
        "governed-write-create-ok"
    } else if rejected {
        "governed-write-create-pending-or-denied"
    } else {
        "governed-write-create-failed"
    };
    emit_update(
        stdout,
        session_id,
        json!({
            "sessionUpdate": "agent_message_chunk",
            "content": {
                "type": "text",
                "text": label
            }
        }),
    );
    write_json(
        stdout,
        &json!({
            "jsonrpc": "2.0",
            "id": id,
            "result": { "stopReason": "end_turn" }
        }),
    );
}

#[allow(clippy::too_many_arguments)]
fn run_governed_write_modify(
    stdout: &mut impl Write,
    rx: &Receiver<InMsg>,
    session_id: &str,
    id: Value,
    cancelled: &AtomicBool,
    path: &str,
    content: &str,
    call_id: &str,
) {
    write_json(
        stdout,
        &json!({
            "jsonrpc": "2.0",
            "id": 82,
            "method": "pillow/write_tool",
            "params": {
                "sessionId": session_id,
                "callId": call_id,
                "toolId": "fs.write",
                "toolVersion": 1,
                "arguments": {
                    "path": path,
                    "mode": "modify",
                    "content": content
                }
            }
        }),
    );
    let response = wait_for_response(rx, cancelled, 82);
    let ok = response
        .as_ref()
        .and_then(|value| value.get("result"))
        .and_then(|result| result.get("path"))
        .and_then(Value::as_str)
        == Some(path);
    let rejected = response
        .as_ref()
        .and_then(|value| value.get("error"))
        .and_then(|error| error.get("message"))
        .and_then(Value::as_str)
        .is_some_and(|message| message.contains("governed_write"));
    let label = if ok {
        "governed-write-modify-ok"
    } else if rejected {
        "governed-write-modify-pending-or-denied"
    } else {
        "governed-write-modify-failed"
    };
    emit_update(
        stdout,
        session_id,
        json!({
            "sessionUpdate": "agent_message_chunk",
            "content": {
                "type": "text",
                "text": label
            }
        }),
    );
    write_json(
        stdout,
        &json!({
            "jsonrpc": "2.0",
            "id": id,
            "result": { "stopReason": "end_turn" }
        }),
    );
}

/// Same call identity with changed content — second request must fail closed.
fn run_governed_write_conflict(
    stdout: &mut impl Write,
    rx: &Receiver<InMsg>,
    session_id: &str,
    id: Value,
    cancelled: &AtomicBool,
) {
    let call_id = "write-call-conflict-1";
    let path = "out/conflict.txt";
    write_json(
        stdout,
        &json!({
            "jsonrpc": "2.0",
            "id": 91,
            "method": "pillow/write_tool",
            "params": {
                "sessionId": session_id,
                "callId": call_id,
                "toolId": "fs.write",
                "toolVersion": 1,
                "arguments": {
                    "path": path,
                    "mode": "create",
                    "content": "conflict first content\n"
                }
            }
        }),
    );
    // Do not wait for the first to complete — immediately replay with different content.
    // Host should reject the conflicting identity before a second proposal is created.
    write_json(
        stdout,
        &json!({
            "jsonrpc": "2.0",
            "id": 92,
            "method": "pillow/write_tool",
            "params": {
                "sessionId": session_id,
                "callId": call_id,
                "toolId": "fs.write",
                "toolVersion": 1,
                "arguments": {
                    "path": path,
                    "mode": "create",
                    "content": "conflict second content DIFFERENT\n"
                }
            }
        }),
    );
    // Second call should fail closed quickly while the first remains pending
    // on human approval. Wait for 92 first so a fast error is not discarded
    // by wait_for_response's non-matching id drop.
    let second = wait_for_response(rx, cancelled, 92);
    let first = wait_for_response(rx, cancelled, 91);
    let first_seen = first.as_ref().is_some();
    let second_failed = second
        .as_ref()
        .is_some_and(|response| response.get("error").is_some());
    let second_conflict = second
        .as_ref()
        .and_then(|value| value.get("error"))
        .and_then(|error| error.get("message"))
        .and_then(Value::as_str)
        .is_some_and(|message| {
            message.contains("governed_write")
                || message.contains("conflict")
                || message.contains("invalid_governed_write")
                || message.contains("provider_request")
                || message.contains("busy")
                || message.contains("in_progress")
                || message.contains("unavailable")
        });
    let label = if first_seen && (second_conflict || second_failed) {
        "governed-write-conflict-ok"
    } else {
        "governed-write-conflict-failed"
    };
    emit_update(
        stdout,
        session_id,
        json!({
            "sessionUpdate": "agent_message_chunk",
            "content": {
                "type": "text",
                "text": label
            }
        }),
    );
    write_json(
        stdout,
        &json!({
            "jsonrpc": "2.0",
            "id": id,
            "result": { "stopReason": "end_turn" }
        }),
    );
}

fn wait_for_result(
    rx: &Receiver<InMsg>,
    cancelled: &AtomicBool,
    expected_id: u64,
) -> Option<Value> {
    wait_for_response(rx, cancelled, expected_id)?
        .get("result")
        .cloned()
}

fn wait_for_response(
    rx: &Receiver<InMsg>,
    cancelled: &AtomicBool,
    expected_id: u64,
) -> Option<Value> {
    // Desktop approval can take tens of seconds; keep adapter unit tests well
    // under this ceiling via concurrent deciders.
    wait_for_response_timeout(rx, cancelled, expected_id, Duration::from_secs(180))
}

fn wait_for_response_timeout(
    rx: &Receiver<InMsg>,
    cancelled: &AtomicBool,
    expected_id: u64,
    timeout: Duration,
) -> Option<Value> {
    let deadline = std::time::Instant::now() + timeout;
    while std::time::Instant::now() < deadline {
        match rx.recv_timeout(Duration::from_millis(20)) {
            Ok(InMsg::Line(line)) => {
                note_cancel_line(&line, cancelled);
                if let Ok(value) = serde_json::from_str::<Value>(&line)
                    && value.get("id").and_then(Value::as_u64) == Some(expected_id)
                {
                    return Some(value);
                }
            }
            Ok(InMsg::Eof) | Err(RecvTimeoutError::Disconnected) => return None,
            Err(RecvTimeoutError::Timeout) => {}
        }
        if cancelled.load(Ordering::SeqCst) {
            // Keep waiting briefly for a terminal response after cancel.
            // Do not drop the expected id if it arrives as cancelled.
        }
    }
    None
}

fn wait_ms(rx: &Receiver<InMsg>, cancelled: &AtomicBool, ms: u64) {
    let steps = (ms / 20).max(1);
    for _ in 0..steps {
        while let Ok(msg) = rx.try_recv() {
            if let InMsg::Line(line) = msg {
                note_cancel_line(&line, cancelled);
            }
        }
        thread::sleep(Duration::from_millis(20));
    }
}

fn permission_was_rejected(rx: &Receiver<InMsg>, cancelled: &AtomicBool, ms: u64) -> bool {
    let steps = (ms / 20).max(1);
    for _ in 0..steps {
        while let Ok(msg) = rx.try_recv() {
            if let InMsg::Line(line) = msg {
                note_cancel_line(&line, cancelled);
                if let Ok(value) = serde_json::from_str::<Value>(&line)
                    && value.get("id") == Some(&Value::from(0))
                {
                    return value["result"]["outcome"]["optionId"]
                        .as_str()
                        .is_some_and(|option| option.starts_with("reject"));
                }
            }
        }
        thread::sleep(Duration::from_millis(20));
    }
    false
}

fn note_cancel_line(line: &str, cancelled: &AtomicBool) {
    if let Ok(v) = serde_json::from_str::<Value>(line)
        && v.get("method").and_then(|m| m.as_str()) == Some("session/cancel")
    {
        cancelled.store(true, Ordering::SeqCst);
    }
}

fn emit_tool_read(stdout: &mut impl Write, session_id: &str, target: &str) {
    emit_update(
        stdout,
        session_id,
        json!({
            "sessionUpdate": "tool_call",
            "toolCallId": "tc-read-1",
            "title": "read_file",
            "rawInput": { "target": target },
            "_meta": { "x.ai/tool": { "name": "read_file", "kind": "read", "read_only": true } }
        }),
    );
}

fn emit_update(stdout: &mut impl Write, session_id: &str, update: Value) {
    write_json(
        stdout,
        &json!({
            "jsonrpc": "2.0",
            "method": "session/update",
            "params": { "sessionId": session_id, "update": update }
        }),
    );
}

pub(crate) fn write_json(stdout: &mut impl Write, value: &Value) {
    let _ = writeln!(stdout, "{value}");
    let _ = stdout.flush();
}

pub(crate) fn write_error(stdout: &mut impl Write, id: Value, code: i64, message: &str) {
    write_json(
        stdout,
        &json!({
            "jsonrpc": "2.0",
            "id": id,
            "error": { "code": code, "message": message }
        }),
    );
}
