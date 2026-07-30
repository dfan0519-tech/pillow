//! Prompt script scenarios for the fake ACP agent.

use std::io::Write;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{Receiver, RecvTimeoutError};
use std::thread;
use std::time::Duration;

use serde_json::{Value, json};

use super::InMsg;

pub(crate) fn run_prompt_script(
    stdout: &mut impl Write,
    rx: &Receiver<InMsg>,
    script: &str,
    session_id: &str,
    id: Value,
    delay_ms: u64,
    cancelled: Arc<AtomicBool>,
) {
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
