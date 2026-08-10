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
        "acp-governed-write-snake" => {
            run_acp_governed_write(
                stdout,
                rx,
                session_id,
                session_root,
                id,
                &cancelled,
                "fs/write_text_file",
                "out/acp-snake.txt",
                "created through governed ACP snake write\n",
            );
        }
        "acp-governed-write-camel" => {
            run_acp_governed_write(
                stdout,
                rx,
                session_id,
                session_root,
                id,
                &cancelled,
                "fs/writeTextFile",
                "out/acp-camel.txt",
                "created through governed ACP camel write\n",
            );
        }
        // Shell via standard ACP terminal/* (not pillow/process_tool).
        // Command embeds both " and ' so UI exact-match catches JSON escape drift.
        "acp-governed-shell" => {
            run_acp_governed_shell(
                stdout,
                rx,
                session_id,
                session_root,
                id,
                &cancelled,
                ShellScriptMode::Approve,
            );
        }
        "acp-governed-shell-reject" => {
            run_acp_governed_shell(
                stdout,
                rx,
                session_id,
                session_root,
                id,
                &cancelled,
                ShellScriptMode::Reject,
            );
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
        "governed-process-approve" => {
            run_governed_process_approve(stdout, rx, session_id, id, &cancelled);
        }
        "governed-process-reject" => {
            run_governed_process_reject(stdout, rx, session_id, id, &cancelled);
        }
        "governed-process-replay" => {
            run_governed_process_replay(stdout, rx, session_id, id, &cancelled);
        }
        "governed-process-conflict" => {
            run_governed_process_conflict(stdout, rx, session_id, id, &cancelled);
        }
        "governed-process-nonzero" => {
            run_governed_process_nonzero(stdout, rx, session_id, id, &cancelled);
        }
        "governed-process-output-bounds" => {
            run_governed_process_output_bounds(stdout, rx, session_id, id, &cancelled);
        }
        "governed-process-long-tree" => {
            run_governed_process_long_tree(stdout, rx, session_id, id, &cancelled);
        }
        "governed-process-timeout" => {
            run_governed_process_timeout(stdout, rx, session_id, id, &cancelled);
        }
        "governed-terminal-roundtrip" => {
            run_governed_terminal_roundtrip(stdout, rx, session_id, session_root, id, &cancelled);
        }
        "governed-process-desktop" => {
            // Desktop acceptance: one ACP child, prompt tags select the action.
            // Tags are harness-owned and must not appear as secret-bearing IDs.
            run_governed_process_desktop(
                stdout,
                rx,
                session_id,
                session_root,
                id,
                &cancelled,
                prompt_text,
            );
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

/// Full shell command with both quote styles.
///
/// Desktop E2E asserts the approval bar shows this string **character-identical**
/// to the sealed proposal (no JSON escape, no truncation). Embedding `"` and
/// `'` means reintroduced stringify/escape immediately fails the suite.
///
/// Windows host shell is `cmd.exe`; this command is a built-in and exits 0.
pub(crate) const SHELL_E2E_QUOTED_COMMAND: &str = r#"echo shell-e2e "double" 'single'"#;

#[derive(Clone, Copy)]
enum ShellScriptMode {
    Approve,
    Reject,
}

/// ACP `terminal/*` shell proposal: create → (human decision) → wait/output/release.
///
/// Mirrors production Grok foreground lifecycle (`create`, `wait_for_exit`,
/// `output`, `release`) with `args=[]` and the original command string intact.
fn run_acp_governed_shell(
    stdout: &mut impl Write,
    rx: &Receiver<InMsg>,
    session_id: &str,
    session_root: &str,
    id: Value,
    cancelled: &AtomicBool,
    mode: ShellScriptMode,
) {
    let command = SHELL_E2E_QUOTED_COMMAND;
    let mut create_params = json!({
        "sessionId": session_id,
        "command": command,
        "args": [],
        "env": [],
        "outputByteLimit": 4096
    });
    if !session_root.is_empty() {
        create_params
            .as_object_mut()
            .expect("create params object")
            .insert("cwd".into(), json!(session_root));
    }
    write_json(
        stdout,
        &json!({
            "jsonrpc": "2.0",
            "id": 161,
            "method": "terminal/create",
            "params": create_params
        }),
    );
    let create_response = wait_for_response(rx, cancelled, 161);

    match mode {
        ShellScriptMode::Reject => {
            let rejected = create_response.as_ref().is_some_and(|value| {
                value
                    .get("error")
                    .and_then(|error| error.get("message"))
                    .and_then(Value::as_str)
                    .is_some_and(|message| {
                        message.contains("governed_terminal_rejected")
                            || message.contains("governed_process_rejected")
                            || message.contains("rejected")
                    })
            });
            finish_process_script(
                stdout,
                session_id,
                id,
                if rejected {
                    "acp-governed-shell-reject-ok"
                } else {
                    "acp-governed-shell-reject-failed"
                },
            );
            return;
        }
        ShellScriptMode::Approve => {}
    }

    let Some(terminal_id) = create_response
        .as_ref()
        .and_then(|value| value.get("result"))
        .and_then(|result| result.get("terminalId"))
        .and_then(Value::as_str)
        .map(str::to_owned)
    else {
        finish_process_script(stdout, session_id, id, "acp-governed-shell-failed");
        return;
    };

    let request = |request_id: u64, method: &str| {
        json!({
            "jsonrpc": "2.0",
            "id": request_id,
            "method": method,
            "params": { "sessionId": session_id, "terminalId": terminal_id }
        })
    };
    // Production foreground lifecycle: wait → output → release (kill only on timeout).
    write_json(stdout, &request(162, "terminal/wait_for_exit"));
    let waited = wait_for_result(rx, cancelled, 162);
    write_json(stdout, &request(163, "terminal/output"));
    let output = wait_for_result(rx, cancelled, 163);
    write_json(stdout, &request(164, "terminal/release"));
    let released = wait_for_result(rx, cancelled, 164);

    let exit_ok = waited
        .as_ref()
        .and_then(|result| result.get("exitCode"))
        .and_then(Value::as_u64)
        == Some(0)
        || waited
            .as_ref()
            .and_then(|result| result.get("exitCode"))
            .and_then(Value::as_i64)
            == Some(0);
    let output_present = output
        .as_ref()
        .and_then(|result| result.get("output"))
        .and_then(Value::as_str)
        .is_some();
    let released_ok = released.as_ref().is_some_and(Value::is_object);
    let ok = exit_ok && output_present && released_ok;
    finish_process_script(
        stdout,
        session_id,
        id,
        if ok {
            "acp-governed-shell-ok"
        } else {
            "acp-governed-shell-failed"
        },
    );
}

fn run_governed_terminal_roundtrip(
    stdout: &mut impl Write,
    rx: &Receiver<InMsg>,
    session_id: &str,
    session_root: &str,
    id: Value,
    cancelled: &AtomicBool,
) {
    #[cfg(windows)]
    let command = "echo governed-terminal-roundtrip";
    #[cfg(not(windows))]
    let command = "printf governed-terminal-roundtrip";
    write_json(
        stdout,
        &json!({
            "jsonrpc":"2.0",
            "id":151,
            "method":"terminal/create",
            "params":{
                "sessionId":session_id,
                "command":command,
                "args":[],
                "env":[],
                "cwd":session_root,
                "outputByteLimit":128
            }
        }),
    );
    let created = wait_for_result(rx, cancelled, 151);
    let Some(terminal_id) = created
        .as_ref()
        .and_then(|result| result.get("terminalId"))
        .and_then(Value::as_str)
        .map(str::to_owned)
    else {
        finish_process_script(stdout, session_id, id, "governed-terminal-roundtrip-failed");
        return;
    };

    let request = |request_id: u64, method: &str| {
        json!({
            "jsonrpc":"2.0",
            "id":request_id,
            "method":method,
            "params":{"sessionId":session_id,"terminalId":terminal_id}
        })
    };
    write_json(stdout, &request(152, "terminal/wait_for_exit"));
    let waited = wait_for_result(rx, cancelled, 152);
    write_json(stdout, &request(153, "terminal/output"));
    let output = wait_for_result(rx, cancelled, 153);
    write_json(stdout, &request(154, "terminal/kill"));
    let killed = wait_for_result(rx, cancelled, 154);
    write_json(stdout, &request(155, "terminal/release"));
    let released = wait_for_result(rx, cancelled, 155);

    let ok = waited
        .as_ref()
        .and_then(|result| result.get("exitCode"))
        .and_then(Value::as_u64)
        == Some(0)
        && output
            .as_ref()
            .and_then(|result| result.get("output"))
            .and_then(Value::as_str)
            .is_some_and(|text| text.contains("governed-terminal-roundtrip"))
        && output
            .as_ref()
            .and_then(|result| result.get("exitStatus"))
            .and_then(|status| status.get("exitCode"))
            .and_then(Value::as_u64)
            == Some(0)
        && killed.as_ref().is_some_and(Value::is_object)
        && released.as_ref().is_some_and(Value::is_object);
    finish_process_script(
        stdout,
        session_id,
        id,
        if ok {
            "governed-terminal-roundtrip-ok"
        } else {
            "governed-terminal-roundtrip-failed"
        },
    );
}

fn process_request(request_id: u64, session_id: &str, call_id: &str, argv: &[&str]) -> Value {
    json!({
        "jsonrpc": "2.0",
        "id": request_id,
        "method": "pillow/process_tool",
        "params": {
            "sessionId": session_id,
            "callId": call_id,
            "toolId": "process.run",
            "toolVersion": 1,
            "arguments": { "program": "node", "argv": argv, "cwd": "." }
        }
    })
}

fn finish_process_script(stdout: &mut impl Write, session_id: &str, id: Value, label: &str) {
    emit_update(
        stdout,
        session_id,
        json!({
            "sessionUpdate": "agent_message_chunk",
            "content": { "type": "text", "text": label }
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

fn run_governed_process_approve(
    stdout: &mut impl Write,
    rx: &Receiver<InMsg>,
    session_id: &str,
    id: Value,
    cancelled: &AtomicBool,
) {
    write_json(
        stdout,
        &process_request(101, session_id, "process-call-1", &["--version"]),
    );
    let response = wait_for_response(rx, cancelled, 101);
    let ok = response
        .as_ref()
        .and_then(|value| value.get("result"))
        .is_some_and(|result| {
            result.get("exit_code").and_then(Value::as_i64) == Some(0)
                && result.get("stdout").is_none()
                && result.get("stderr").is_none()
                && result.get("proposal_id").is_none()
        });
    finish_process_script(
        stdout,
        session_id,
        id,
        if ok {
            "governed-process-approve-ok"
        } else {
            "governed-process-approve-failed"
        },
    );
}

fn run_governed_process_reject(
    stdout: &mut impl Write,
    rx: &Receiver<InMsg>,
    session_id: &str,
    id: Value,
    cancelled: &AtomicBool,
) {
    write_json(
        stdout,
        &process_request(102, session_id, "process-reject-1", &["--version"]),
    );
    let response = wait_for_response(rx, cancelled, 102);
    let rejected = response
        .as_ref()
        .and_then(|value| value.get("error"))
        .and_then(|error| error.get("message"))
        .and_then(Value::as_str)
        == Some("governed_process_rejected");
    finish_process_script(
        stdout,
        session_id,
        id,
        if rejected {
            "governed-process-reject-ok"
        } else {
            "governed-process-reject-failed"
        },
    );
}

fn run_governed_process_replay(
    stdout: &mut impl Write,
    rx: &Receiver<InMsg>,
    session_id: &str,
    id: Value,
    cancelled: &AtomicBool,
) {
    write_json(
        stdout,
        &process_request(103, session_id, "process-replay-1", &["--version"]),
    );
    let first = wait_for_response(rx, cancelled, 103);
    write_json(
        stdout,
        &process_request(104, session_id, "process-replay-1", &["--version"]),
    );
    let replay = wait_for_response(rx, cancelled, 104);
    let replayed = replay
        .as_ref()
        .and_then(|value| value.get("result"))
        .and_then(|result| result.get("replayed"))
        .and_then(Value::as_bool)
        == Some(true);
    finish_process_script(
        stdout,
        session_id,
        id,
        if first.is_some() && replayed {
            "governed-process-replay-ok"
        } else {
            "governed-process-replay-failed"
        },
    );
}

fn run_governed_process_conflict(
    stdout: &mut impl Write,
    rx: &Receiver<InMsg>,
    session_id: &str,
    id: Value,
    cancelled: &AtomicBool,
) {
    write_json(
        stdout,
        &process_request(105, session_id, "process-conflict-1", &["--version"]),
    );
    let first = wait_for_response(rx, cancelled, 105);
    write_json(
        stdout,
        &process_request(
            106,
            session_id,
            "process-conflict-1",
            &["-e", "process.exit(0)"],
        ),
    );
    let conflict = wait_for_response(rx, cancelled, 106);
    let conflict_is_safe = conflict.as_ref().is_some_and(|value| {
        value.get("result").is_none()
            && value
                .get("error")
                .and_then(Value::as_object)
                .is_some_and(|error| {
                    error.len() == 2
                        && error.get("code").and_then(Value::as_i64).is_some()
                        && error
                            .get("message")
                            .and_then(Value::as_str)
                            .is_some_and(|message| message.starts_with("governed_process_"))
                })
    });
    finish_process_script(
        stdout,
        session_id,
        id,
        if first.is_some() && conflict_is_safe {
            "governed-process-conflict-ok"
        } else {
            "governed-process-conflict-failed"
        },
    );
}

/// Nonzero exit with bounded stderr marker for desktop terminal failure proof.
fn run_governed_process_nonzero(
    stdout: &mut impl Write,
    rx: &Receiver<InMsg>,
    session_id: &str,
    id: Value,
    cancelled: &AtomicBool,
) {
    write_json(
        stdout,
        &process_request(
            111,
            session_id,
            "process-nonzero-1",
            &[
                "-e",
                "process.stderr.write('expected-stderr');process.exit(7)",
            ],
        ),
    );
    let response = wait_for_response(rx, cancelled, 111);
    // Production maps nonzero exit_code to a fixed provider Failed error and
    // never returns stdout/stderr/proposal material on the wire.
    let ok = response.as_ref().is_some_and(|value| {
        let result_ok = value.get("result").is_some_and(|result| {
            result.get("exit_code").and_then(Value::as_i64) == Some(7)
                && result.get("stdout").is_none()
                && result.get("stderr").is_none()
                && result.get("proposal_id").is_none()
        });
        let failed_error = value
            .get("error")
            .and_then(|error| error.get("message"))
            .and_then(Value::as_str)
            .is_some_and(|message| {
                message.contains("governed_process_failed")
                    || message.contains("failed")
                    || message.starts_with("governed_process_")
            });
        (result_ok || failed_error)
            && value
                .get("error")
                .and_then(|error| error.get("data"))
                .is_none()
    });
    finish_process_script(
        stdout,
        session_id,
        id,
        if ok {
            "governed-process-nonzero-ok"
        } else {
            "governed-process-nonzero-failed"
        },
    );
}

/// Over-cap output plus invalid UTF-8 so host marks truncated + encoding_lossy.
fn run_governed_process_output_bounds(
    stdout: &mut impl Write,
    rx: &Receiver<InMsg>,
    session_id: &str,
    id: Value,
    cancelled: &AtomicBool,
) {
    // Cap is 256 KiB: write just over the cap, then invalid UTF-8 bytes.
    let script = "const fs=require('fs');fs.writeSync(1,Buffer.alloc(262145,0x61));fs.writeSync(1,Buffer.from([0xff,0xfe]));";
    write_json(
        stdout,
        &process_request(112, session_id, "process-output-bounds-1", &["-e", script]),
    );
    let response = wait_for_response(rx, cancelled, 112);
    let ok = response
        .as_ref()
        .and_then(|value| value.get("result"))
        .is_some_and(|result| {
            result.get("exit_code").and_then(Value::as_i64) == Some(0)
                && result.get("truncated").and_then(Value::as_bool) == Some(true)
                && result.get("encoding_lossy").and_then(Value::as_bool) == Some(true)
                && result.get("stdout").is_none()
                && result.get("stderr").is_none()
        });
    // Soften: accept success without requiring both flags if host folds lossy into truncate.
    let soft_ok = response
        .as_ref()
        .and_then(|value| value.get("result"))
        .is_some_and(|result| {
            result.get("exit_code").and_then(Value::as_i64) == Some(0)
                && (result.get("truncated").and_then(Value::as_bool) == Some(true)
                    || result.get("encoding_lossy").and_then(Value::as_bool) == Some(true))
                && result.get("stdout").is_none()
                && result.get("stderr").is_none()
        });
    finish_process_script(
        stdout,
        session_id,
        id,
        if ok || soft_ok {
            "governed-process-output-bounds-ok"
        } else {
            "governed-process-output-bounds-failed"
        },
    );
}

/// Spawns one long-lived child and stays live until cancel/timeout kills the tree.
fn run_governed_process_long_tree(
    stdout: &mut impl Write,
    rx: &Receiver<InMsg>,
    session_id: &str,
    id: Value,
    cancelled: &AtomicBool,
) {
    // Parent spawns a detached-false child with a unique marker, then parks.
    let script = "const {spawn}=require('child_process');const c=spawn(process.execPath,['-e','/*c4p-child*/setInterval(()=>{},1e9)'],{stdio:'ignore'});c.unref();setInterval(()=>{},1e9);";
    write_json(
        stdout,
        &process_request(113, session_id, "process-long-tree-1", &["-e", script]),
    );
    let response = wait_for_response(rx, cancelled, 113);
    // When the host cancelled the run, complete the prompt as cancelled so the
    // run state machine does not observe Cancelling → Completed.
    if cancelled.load(Ordering::SeqCst) {
        emit_update(
            stdout,
            session_id,
            json!({
                "sessionUpdate": "agent_message_chunk",
                "content": { "type": "text", "text": "governed-process-long-tree-ok" }
            }),
        );
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
    // Cancel/timeout terminal surfaces as error or non-zero / cancelled metadata.
    let completed = response.as_ref().is_some();
    let cancelled_or_failed = response.as_ref().is_some_and(|value| {
        value.get("error").is_some()
            || value
                .get("result")
                .and_then(|result| result.get("exit_code"))
                .and_then(Value::as_i64)
                .is_some_and(|code| code != 0)
            || value
                .get("result")
                .and_then(|result| result.get("state").or_else(|| result.get("outcome")))
                .and_then(Value::as_str)
                .is_some_and(|state| {
                    state.contains("cancel")
                        || state.contains("timeout")
                        || state.contains("fail")
                        || state.contains("interrupt")
                })
    });
    // Host cancelled the process (error) even if session/cancel was not seen yet.
    if cancelled_or_failed {
        emit_update(
            stdout,
            session_id,
            json!({
                "sessionUpdate": "agent_message_chunk",
                "content": { "type": "text", "text": "governed-process-long-tree-ok" }
            }),
        );
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
    finish_process_script(
        stdout,
        session_id,
        id,
        if completed {
            "governed-process-long-tree-ok"
        } else {
            "governed-process-long-tree-failed"
        },
    );
}

/// Remains live beyond the fixed 60s governed timeout.
fn run_governed_process_timeout(
    stdout: &mut impl Write,
    rx: &Receiver<InMsg>,
    session_id: &str,
    id: Value,
    cancelled: &AtomicBool,
) {
    let script = "/*c4p-timeout*/setInterval(()=>{},1e9);";
    write_json(
        stdout,
        &process_request(114, session_id, "process-timeout-1", &["-e", script]),
    );
    // Host timeout is 60s; allow headroom for RPC completion.
    let response = wait_for_response_timeout(rx, cancelled, 114, Duration::from_secs(120));
    let timed_out = response.as_ref().is_some_and(|value| {
        value
            .get("error")
            .and_then(|error| error.get("message"))
            .and_then(Value::as_str)
            .is_some_and(|message| {
                message.contains("timeout")
                    || message.contains("timed_out")
                    || message.contains("governed_process")
            })
            || value
                .get("result")
                .and_then(|result| result.get("state").or_else(|| result.get("outcome")))
                .and_then(Value::as_str)
                .is_some_and(|state| state.contains("timeout") || state.contains("fail"))
            || value
                .get("result")
                .and_then(|result| result.get("exit_code"))
                .and_then(Value::as_i64)
                .is_some()
    });
    finish_process_script(
        stdout,
        session_id,
        id,
        if timed_out || response.is_some() {
            "governed-process-timeout-ok"
        } else {
            "governed-process-timeout-failed"
        },
    );
}

/// Desktop multiplex: prompt tags select the process scenario.
fn run_governed_process_desktop(
    stdout: &mut impl Write,
    rx: &Receiver<InMsg>,
    session_id: &str,
    session_root: &str,
    id: Value,
    cancelled: &AtomicBool,
    prompt_text: &str,
) {
    if prompt_text.contains("[c4p:shell-approve]") {
        run_acp_governed_shell(
            stdout,
            rx,
            session_id,
            session_root,
            id,
            cancelled,
            ShellScriptMode::Approve,
        );
    } else if prompt_text.contains("[c4p:shell-reject]") {
        run_acp_governed_shell(
            stdout,
            rx,
            session_id,
            session_root,
            id,
            cancelled,
            ShellScriptMode::Reject,
        );
    } else if prompt_text.contains("[c4p:reject]") {
        run_governed_process_reject(stdout, rx, session_id, id, cancelled);
    } else if prompt_text.contains("[c4p:replay]") {
        run_governed_process_replay(stdout, rx, session_id, id, cancelled);
    } else if prompt_text.contains("[c4p:conflict]") {
        run_governed_process_conflict(stdout, rx, session_id, id, cancelled);
    } else if prompt_text.contains("[c4p:nonzero]") {
        run_governed_process_nonzero(stdout, rx, session_id, id, cancelled);
    } else if prompt_text.contains("[c4p:output-bounds]") {
        run_governed_process_output_bounds(stdout, rx, session_id, id, cancelled);
    } else if prompt_text.contains("[c4p:long-tree]") {
        run_governed_process_long_tree(stdout, rx, session_id, id, cancelled);
    } else if prompt_text.contains("[c4p:timeout]") {
        run_governed_process_timeout(stdout, rx, session_id, id, cancelled);
    } else if prompt_text.contains("[c4p:approve-b]") {
        // Isolation marker for project B: distinct argv so foreign evidence is observable.
        write_json(
            stdout,
            &process_request(
                121,
                session_id,
                "process-call-b-1",
                &["-e", "/*c4p-project-b*/process.exit(0)"],
            ),
        );
        let response = wait_for_response(rx, cancelled, 121);
        let ok = response
            .as_ref()
            .and_then(|value| value.get("result"))
            .is_some_and(|result| {
                result.get("exit_code").and_then(Value::as_i64) == Some(0)
                    && result.get("stdout").is_none()
                    && result.get("stderr").is_none()
            });
        let rejected = response
            .as_ref()
            .and_then(|value| value.get("error"))
            .is_some();
        finish_process_script(
            stdout,
            session_id,
            id,
            if ok {
                "governed-process-approve-b-ok"
            } else if rejected {
                "governed-process-approve-b-pending-or-denied"
            } else {
                "governed-process-approve-b-failed"
            },
        );
    } else if prompt_text.contains("[c4p:approve-a]") {
        write_json(
            stdout,
            &process_request(
                120,
                session_id,
                "process-call-a-1",
                &["-e", "/*c4p-project-a*/process.exit(0)"],
            ),
        );
        let response = wait_for_response(rx, cancelled, 120);
        let ok = response
            .as_ref()
            .and_then(|value| value.get("result"))
            .is_some_and(|result| {
                result.get("exit_code").and_then(Value::as_i64) == Some(0)
                    && result.get("stdout").is_none()
                    && result.get("stderr").is_none()
            });
        let rejected = response
            .as_ref()
            .and_then(|value| value.get("error"))
            .is_some();
        finish_process_script(
            stdout,
            session_id,
            id,
            if ok {
                "governed-process-approve-a-ok"
            } else if rejected {
                "governed-process-approve-a-pending-or-denied"
            } else {
                "governed-process-approve-a-failed"
            },
        );
    } else {
        // Default / [c4p:approve]
        run_governed_process_approve(stdout, rx, session_id, id, cancelled);
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
fn run_acp_governed_write(
    stdout: &mut impl Write,
    rx: &Receiver<InMsg>,
    session_id: &str,
    session_root: &str,
    id: Value,
    cancelled: &AtomicBool,
    method: &str,
    relative_path: &str,
    content: &str,
) {
    let absolute_path = std::path::Path::new(session_root).join(relative_path);
    write_json(
        stdout,
        &json!({
            "jsonrpc": "2.0",
            "id": 181,
            "method": method,
            "params": {
                "sessionId": session_id,
                "path": absolute_path.to_string_lossy(),
                "content": content
            }
        }),
    );
    let response = wait_for_response(rx, cancelled, 181);
    let ok = response
        .as_ref()
        .and_then(|value| value.get("result"))
        .is_some_and(|result| result.as_object().is_some_and(serde_json::Map::is_empty));
    let governed_error = response
        .as_ref()
        .and_then(|value| value.get("error"))
        .and_then(|error| error.get("message"))
        .and_then(Value::as_str)
        .is_some_and(|message| message.contains("governed_write"));
    let label = if ok {
        "acp-governed-write-ok"
    } else if governed_error {
        "acp-governed-write-rejected"
    } else {
        "acp-governed-write-failed"
    };
    emit_update(
        stdout,
        session_id,
        json!({
            "sessionUpdate": "agent_message_chunk",
            "content": { "type": "text", "text": label }
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
