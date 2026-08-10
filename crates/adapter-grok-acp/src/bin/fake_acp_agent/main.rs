//! Deterministic fake ACP child for adapter tests (no live account).
//!
//! Script selection via `KEEPER_FAKE_ACP_SCRIPT`:
//! `text-only` (default), `with-tools`, `permission-read`, `permission-write`,
//! `hang-until-cancel`, `exit-early`, `malformed`, `oversized-line`,
//! `load-replay`, `timeout-prompt`, `vendor-noise`, `auth-failed`,
//! `spawn-descendant`, `hold-descendant`, `race-session-operation`, `ignore-cancel`,
//! `governed-write-create`, `governed-write-modify`, `governed-write-conflict`,
//! `acp-governed-write-snake`, `acp-governed-write-camel`,
//! `governed-write-desktop`, `governed-process-approve`,
//! `governed-process-reject`, `governed-process-replay`,
//! `governed-process-conflict`, `governed-process-nonzero`,
//! `governed-process-output-bounds`, `governed-process-long-tree`,
//! `governed-process-timeout`, and `governed-process-desktop`.
//!
//! When the parent process does not forward `KEEPER_FAKE_ACP_SCRIPT` (production
//! Grok ACP env allow-list), a sibling file `keeper_fake_acp_script.txt` next to
//! this executable is accepted so desktop acceptance can select a scenario
//! without product env-policy changes.

mod scripts;

use std::io::{self, BufRead, Write};
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc;
use std::thread;

use serde_json::{Value, json};

use scripts::{PromptScriptContext, run_prompt_script, write_error, write_json};

pub(crate) enum InMsg {
    Line(String),
    Eof,
}

/// Resolve script name: env first, then sibling file beside this binary.
fn resolve_script_name() -> String {
    if let Ok(value) = std::env::var("KEEPER_FAKE_ACP_SCRIPT") {
        let trimmed = value.trim();
        if !trimmed.is_empty() {
            return trimmed.to_string();
        }
    }
    if let Ok(exe) = std::env::current_exe() {
        let sibling = exe
            .parent()
            .map(|dir| dir.join("keeper_fake_acp_script.txt"))
            .unwrap_or_else(|| PathBuf::from("keeper_fake_acp_script.txt"));
        if let Ok(raw) = std::fs::read_to_string(sibling) {
            let trimmed = raw.trim();
            if !trimmed.is_empty() {
                return trimmed.to_string();
            }
        }
    }
    "text-only".into()
}

fn main() {
    let script = resolve_script_name();
    let delay_ms: u64 = std::env::var("KEEPER_FAKE_ACP_DELAY_MS")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(0);

    if script == "exit-early" {
        let _ = writeln!(io::stderr(), "fake exit-early");
        std::process::exit(2);
    }

    if script == "hold-descendant" {
        loop {
            thread::park();
        }
    }

    // Record our own PID for process-tree cleanup tests.
    if let Ok(path) = std::env::var("KEEPER_FAKE_ACP_PARENT_PID_FILE") {
        let _ = std::fs::write(path, std::process::id().to_string());
    }

    let mut descendant = None;
    if script == "spawn-descendant" {
        // This is the first operation after the suspended primary thread resumes.
        descendant = spawn_hanging_descendant();
        if let (Some(child), Ok(path)) = (
            descendant.as_ref(),
            std::env::var("KEEPER_FAKE_ACP_DESCENDANT_PID_FILE"),
        ) {
            let _ = std::fs::write(path, child.id().to_string());
        }
    }

    let cancelled = Arc::new(AtomicBool::new(false));
    let (tx, rx) = mpsc::channel::<InMsg>();
    thread::spawn(move || {
        let stdin = io::stdin();
        for line in stdin.lock().lines() {
            match line {
                Ok(l) => {
                    if tx.send(InMsg::Line(l)).is_err() {
                        break;
                    }
                }
                Err(_) => break,
            }
        }
        let _ = tx.send(InMsg::Eof);
    });

    let mut session_id = String::from("fake-session-1");
    let mut session_root = String::new();
    let mut initialized = false;
    let mut stdout = io::stdout();

    while let Ok(InMsg::Line(line)) = rx.recv() {
        if line.trim().is_empty() {
            continue;
        }
        let msg: Value = match serde_json::from_str(&line) {
            Ok(v) => v,
            Err(_) => continue,
        };

        if msg.get("method").is_some() && msg.get("id").is_none() {
            let method = msg.get("method").and_then(|m| m.as_str()).unwrap_or("");
            if method == "session/cancel" {
                cancelled.store(true, Ordering::SeqCst);
            }
            continue;
        }

        // Client responses to our server requests (permission) — ignore body.
        if msg.get("id").is_some() && msg.get("method").is_none() {
            continue;
        }

        let id = msg.get("id").cloned().unwrap_or(Value::Null);
        let method = msg.get("method").and_then(|m| m.as_str()).unwrap_or("");
        let params = msg.get("params").cloned().unwrap_or(Value::Null);

        match method {
            "initialize" => {
                initialized = true;
                write_json(
                    &mut stdout,
                    &json!({
                        "jsonrpc": "2.0",
                        "id": id,
                        "result": {
                            "protocolVersion": 1,
                            "agentCapabilities": {
                                "loadSession": true,
                                "promptCapabilities": {
                                    "image": false,
                                    "audio": false,
                                    "embeddedContext": true
                                }
                            },
                            "_meta": {
                                "agentVersion": "0.0.0-fake"
                            }
                        }
                    }),
                );
                if script == "vendor-noise" {
                    write_json(
                        &mut stdout,
                        &json!({
                            "jsonrpc": "2.0",
                            "method": "_x.ai/models/update",
                            "params": { "models": ["x"] }
                        }),
                    );
                }
            }
            "session/new" => {
                if !initialized {
                    write_error(&mut stdout, id, -32000, "not initialized");
                    continue;
                }
                if !has_safe_plaintext_session_params(&params) {
                    write_error(&mut stdout, id, -32602, "unsafe session profile");
                    continue;
                }
                session_id = format!("fake-session-{}", simple_id());
                session_root = params
                    .get("cwd")
                    .and_then(Value::as_str)
                    .unwrap_or("")
                    .to_string();
                write_json(
                    &mut stdout,
                    &json!({
                        "jsonrpc": "2.0",
                        "id": id,
                        "result": {
                            "sessionId": session_id,
                            "models": { "currentModelId": "fake" }
                        }
                    }),
                );
                write_json(
                    &mut stdout,
                    &json!({
                        "jsonrpc": "2.0",
                        "method": "session/update",
                        "params": {
                            "sessionId": session_id,
                            "update": {
                                "sessionUpdate": "available_commands_update",
                                "availableCommands": []
                            }
                        }
                    }),
                );
            }
            "session/load" => {
                if !has_safe_plaintext_session_params(&params) {
                    write_error(&mut stdout, id, -32602, "unsafe session profile");
                    continue;
                }
                let sid = params
                    .get("sessionId")
                    .and_then(|v| v.as_str())
                    .unwrap_or("fake-loaded");
                session_id = sid.to_string();
                write_json(
                    &mut stdout,
                    &json!({
                        "jsonrpc": "2.0",
                        "id": id,
                        "result": {
                            "models": { "currentModelId": "fake" },
                            "_meta": { "title": "loaded" }
                        }
                    }),
                );
                write_json(
                    &mut stdout,
                    &json!({
                        "jsonrpc": "2.0",
                        "method": "session/update",
                        "params": {
                            "sessionId": session_id,
                            "update": {
                                "sessionUpdate": "agent_message_chunk",
                                "content": {
                                    "type": "text",
                                    "text": "old assistant replay"
                                }
                            },
                            "_meta": { "isReplay": true }
                        }
                    }),
                );
                write_json(
                    &mut stdout,
                    &json!({
                        "jsonrpc": "2.0",
                        "method": "_x.ai/session/update",
                        "params": {
                            "sessionId": session_id,
                            "_meta": { "isReplay": true }
                        }
                    }),
                );
            }
            "session/prompt" => {
                let sid = params
                    .get("sessionId")
                    .and_then(|v| v.as_str())
                    .unwrap_or(&session_id)
                    .to_string();
                if script == "auth-failed" {
                    write_error(&mut stdout, id, -32000, "unauthenticated: login required");
                    continue;
                }
                let prompt_text = extract_prompt_text(&params);
                run_prompt_script(
                    &mut stdout,
                    &rx,
                    PromptScriptContext {
                        script: &script,
                        session_id: &sid,
                        session_root: &session_root,
                        id,
                        delay_ms,
                        cancelled: Arc::clone(&cancelled),
                        prompt_text: prompt_text.as_str(),
                    },
                );
            }
            _ => write_error(&mut stdout, id, -32601, "Method not found"),
        }
    }

    // Keep descendant handle alive until we exit so the OS job tree can kill it.
    drop(descendant);
}

/// Concatenate text blocks from ACP `session/prompt` params.
fn extract_prompt_text(params: &Value) -> String {
    let Some(blocks) = params.get("prompt").and_then(Value::as_array) else {
        return String::new();
    };
    let mut out = String::new();
    for block in blocks {
        if block.get("type").and_then(Value::as_str) != Some("text") {
            continue;
        }
        if let Some(text) = block.get("text").and_then(Value::as_str) {
            if !out.is_empty() {
                out.push(' ');
            }
            out.push_str(text);
        }
    }
    out
}

fn has_safe_plaintext_session_params(params: &Value) -> bool {
    let encoded = params.to_string().to_ascii_lowercase();
    params.get("mcpServers") == Some(&json!([]))
        && !encoded.contains("yolomode")
        && !encoded.contains("automode")
        && params
            .get("_meta")
            .and_then(|meta| meta.get("agentProfile"))
            == Some(&json!({
                "name": "pillow-plaintext",
                "description": "Pillow-hosted session. Native authority is restricted; \
                                file writes run through governed client filesystem RPC.",
                "tools": ["ask_user_question", "write"],
                "disallowedTools": ["search_tool", "use_tool"]
            }))
}

/// Spawn an immediate long-lived child without a shell launcher or timer process.
fn spawn_hanging_descendant() -> Option<std::process::Child> {
    Command::new(std::env::current_exe().ok()?)
        .env("KEEPER_FAKE_ACP_SCRIPT", "hold-descendant")
        .env_remove("KEEPER_FAKE_ACP_PARENT_PID_FILE")
        .env_remove("KEEPER_FAKE_ACP_DESCENDANT_PID_FILE")
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .ok()
}

fn simple_id() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| format!("{:x}", d.as_nanos()))
        .unwrap_or_else(|_| "1".into())
}
