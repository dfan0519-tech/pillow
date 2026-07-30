//! Correlated JSON-RPC client over child stdio.

use std::collections::HashMap;
use std::io::{BufReader, Read, Write};
use std::process::{ChildStdin, ChildStdout};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender};
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};
use std::time::Duration;

use serde_json::{Value, json};

use super::framer::LineFramer;
use crate::error::{internal, process_exited, rpc_failed, timeout, transport};
use keeper_agent_protocol::RuntimeError;

/// Messages delivered to the adapter control plane from the reader thread.
#[derive(Debug)]
pub enum Incoming {
    /// Agent → client request (`session/request_permission`).
    ServerRequest {
        id: Value,
        method: String,
        params: Value,
    },
    /// Agent → client notification.
    Notification { method: String, params: Value },
    /// Response to a client-originated request.
    Response {
        #[allow(dead_code)]
        id: u64,
        result: Result<Value, RuntimeError>,
    },
    /// Fatal transport failure (malformed line, oversized, reader exit).
    Fatal(RuntimeError),
}

struct Pending {
    tx: Sender<Result<Value, RuntimeError>>,
}

struct Shared {
    stdin: Mutex<Option<ChildStdin>>,
    pending: Mutex<HashMap<u64, Pending>>,
    next_id: AtomicU64,
    closed: AtomicBool,
    /// Fan-out for notifications / server requests / fatals during an active turn.
    event_tx: Mutex<Option<Sender<Incoming>>>,
}

/// NDJSON JSON-RPC client bound to one child process.
pub struct RpcClient {
    shared: Arc<Shared>,
    reader: Mutex<Option<JoinHandle<()>>>,
    request_timeout: Duration,
}

impl RpcClient {
    pub fn start(
        stdin: ChildStdin,
        stdout: ChildStdout,
        max_line_bytes: usize,
        request_timeout: Duration,
    ) -> Self {
        let shared = Arc::new(Shared {
            stdin: Mutex::new(Some(stdin)),
            pending: Mutex::new(HashMap::new()),
            next_id: AtomicU64::new(1),
            closed: AtomicBool::new(false),
            event_tx: Mutex::new(None),
        });
        let reader_shared = Arc::clone(&shared);
        let handle = thread::Builder::new()
            .name("grok-acp-reader".into())
            .spawn(move || reader_loop(reader_shared, stdout, max_line_bytes))
            .expect("spawn reader thread");
        Self {
            shared,
            reader: Mutex::new(Some(handle)),
            request_timeout,
        }
    }

    /// Install a channel that receives notifications/server-requests/fatals.
    pub fn set_event_sink(&self, tx: Option<Sender<Incoming>>) {
        *self.shared.event_tx.lock().expect("event_tx lock") = tx;
    }

    pub fn is_closed(&self) -> bool {
        self.shared.closed.load(Ordering::SeqCst)
    }

    /// Send a request and wait for the correlated response.
    pub fn request(&self, method: &str, params: Value) -> Result<Value, RuntimeError> {
        self.request_timeout(method, params, self.request_timeout)
    }

    pub fn request_timeout(
        &self,
        method: &str,
        params: Value,
        wait: Duration,
    ) -> Result<Value, RuntimeError> {
        if self.shared.closed.load(Ordering::SeqCst) {
            return Err(process_exited());
        }
        let id = self.shared.next_id.fetch_add(1, Ordering::SeqCst);
        let (tx, rx) = mpsc::channel();
        {
            let mut pending = self.shared.pending.lock().expect("pending lock");
            pending.insert(id, Pending { tx });
        }
        let message = json!({
            "jsonrpc": "2.0",
            "id": id,
            "method": method,
            "params": params
        });
        if let Err(e) = self.write_message(&message) {
            self.shared
                .pending
                .lock()
                .expect("pending lock")
                .remove(&id);
            return Err(e);
        }
        wait_response(&rx, wait, method).inspect_err(|_| {
            self.shared
                .pending
                .lock()
                .expect("pending lock")
                .remove(&id);
        })
    }

    /// Fire-and-forget notification (no response expected).
    ///
    /// Never used for `$/cancel_request` or `authenticated` (unsupported).
    pub fn notify(&self, method: &str, params: Value) -> Result<(), RuntimeError> {
        if self.shared.closed.load(Ordering::SeqCst) {
            return Err(process_exited());
        }
        // Hard ban unsupported methods.
        if method == "$/cancel_request" || method == "authenticated" {
            return Err(internal(format!(
                "refusing to send unsupported method `{method}`"
            )));
        }
        let message = json!({
            "jsonrpc": "2.0",
            "method": method,
            "params": params
        });
        self.write_message(&message)
    }

    pub fn respond(&self, id: Value, result: Value) -> Result<(), RuntimeError> {
        let message = json!({
            "jsonrpc": "2.0",
            "id": id,
            "result": result
        });
        self.write_message(&message)
    }

    pub fn close(&self) {
        if self.shared.closed.swap(true, Ordering::SeqCst) {
            return;
        }
        if let Some(mut stdin) = self.shared.stdin.lock().expect("stdin lock").take() {
            let _ = stdin.flush();
            drop(stdin);
        }
        let pending = std::mem::take(&mut *self.shared.pending.lock().expect("pending lock"));
        for (_, p) in pending {
            let _ = p.tx.send(Err(process_exited()));
        }
        if let Some(tx) = self.shared.event_tx.lock().expect("event_tx lock").as_ref() {
            let _ = tx.send(Incoming::Fatal(process_exited()));
        }
    }

    pub fn join_reader(&self) {
        if let Some(handle) = self.reader.lock().expect("reader lock").take() {
            let _ = handle.join();
        }
    }

    fn write_message(&self, message: &Value) -> Result<(), RuntimeError> {
        let line = LineFramer::encode_message(message)?;
        let mut guard = self.shared.stdin.lock().expect("stdin lock");
        let Some(stdin) = guard.as_mut() else {
            return Err(process_exited());
        };
        writeln!(stdin, "{line}").map_err(|e| {
            if e.kind() == std::io::ErrorKind::BrokenPipe {
                process_exited()
            } else {
                transport(format!("stdin write failed: {e}"))
            }
        })?;
        stdin
            .flush()
            .map_err(|e| transport(format!("stdin flush failed: {e}")))
    }
}

impl Drop for RpcClient {
    fn drop(&mut self) {
        self.close();
        self.join_reader();
    }
}

fn wait_response(
    rx: &Receiver<Result<Value, RuntimeError>>,
    wait: Duration,
    method: &str,
) -> Result<Value, RuntimeError> {
    match rx.recv_timeout(wait) {
        Ok(result) => result,
        Err(RecvTimeoutError::Timeout) => Err(timeout(method)),
        Err(RecvTimeoutError::Disconnected) => Err(process_exited()),
    }
}

fn reader_loop(shared: Arc<Shared>, stdout: ChildStdout, max_line_bytes: usize) {
    let mut reader = BufReader::new(stdout);
    let mut framer = LineFramer::new(max_line_bytes);
    let mut buf = [0u8; 8192];
    loop {
        if shared.closed.load(Ordering::SeqCst) {
            break;
        }
        match reader.read(&mut buf) {
            Ok(0) => {
                fail_closed(&shared, process_exited());
                break;
            }
            Ok(n) => match framer.push(&buf[..n]) {
                Ok(lines) => {
                    for line in lines {
                        if let Err(err) = dispatch_line(&shared, &line) {
                            fail_closed(&shared, err);
                            return;
                        }
                    }
                }
                Err(err) => {
                    fail_closed(&shared, err);
                    break;
                }
            },
            Err(_) => {
                fail_closed(&shared, process_exited());
                break;
            }
        }
    }
}

fn dispatch_line(shared: &Shared, line: &[u8]) -> Result<(), RuntimeError> {
    let value = LineFramer::parse_line(line)?;
    let obj = value.as_object().ok_or_else(crate::error::invalid_json)?;

    // Response: has id and result/error, no method.
    if obj.contains_key("id") && !obj.contains_key("method") {
        let id = match &obj["id"] {
            Value::Number(n) => n.as_u64().ok_or_else(crate::error::invalid_json)?,
            _ => {
                // Server may use other id shapes for its own requests only.
                return Ok(());
            }
        };
        let result = if let Some(err) = obj.get("error") {
            let msg = err
                .get("message")
                .and_then(|m| m.as_str())
                .unwrap_or("ACP request failed");
            Err(rpc_failed(msg))
        } else {
            Ok(obj.get("result").cloned().unwrap_or(Value::Null))
        };
        let mut pending = shared.pending.lock().expect("pending lock");
        if let Some(p) = pending.remove(&id) {
            let _ = p.tx.send(result.clone());
        }
        // Also forward response to event channel so turn loop can correlate.
        if let Some(tx) = shared.event_tx.lock().expect("event_tx lock").as_ref() {
            let _ = tx.send(Incoming::Response { id, result });
        }
        return Ok(());
    }

    let method = obj
        .get("method")
        .and_then(|m| m.as_str())
        .unwrap_or("")
        .to_string();
    let params = obj.get("params").cloned().unwrap_or(Value::Null);

    if obj.contains_key("id") && !method.is_empty() {
        // Server → client request.
        let id = obj.get("id").cloned().unwrap_or(Value::Null);
        if let Some(tx) = shared.event_tx.lock().expect("event_tx lock").as_ref() {
            let _ = tx.send(Incoming::ServerRequest { id, method, params });
        }
        return Ok(());
    }

    if !method.is_empty() {
        // Notification — ignore unknown and _x.ai/* at higher layer.
        if let Some(tx) = shared.event_tx.lock().expect("event_tx lock").as_ref() {
            let _ = tx.send(Incoming::Notification { method, params });
        }
    }
    Ok(())
}

fn fail_closed(shared: &Shared, err: RuntimeError) {
    if shared.closed.swap(true, Ordering::SeqCst) {
        return;
    }
    if let Some(mut stdin) = shared.stdin.lock().expect("stdin lock").take() {
        let _ = stdin.flush();
        drop(stdin);
    }
    let pending = std::mem::take(&mut *shared.pending.lock().expect("pending lock"));
    for (_, p) in pending {
        let _ = p.tx.send(Err(err.clone()));
    }
    if let Some(tx) = shared.event_tx.lock().expect("event_tx lock").as_ref() {
        let _ = tx.send(Incoming::Fatal(err));
    }
}
