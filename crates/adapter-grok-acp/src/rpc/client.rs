//! Correlated JSON-RPC client over child stdio.

use std::collections::HashMap;
use std::io::{BufReader, Read, Write};
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
    stdin: Mutex<Option<Box<dyn Write + Send>>>,
    pending: Mutex<HashMap<u64, Pending>>,
    next_id: AtomicU64,
    closed: AtomicBool,
    /// Fan-out for notifications / server requests / fatals during an active turn.
    event_tx: Mutex<Option<Sender<Incoming>>>,
}

/// How long a caller waits for the reader thread to reach the end of the
/// agent's output before giving up on it.
///
/// The reader blocks on the child's stdout pipe, and that pipe reports
/// end-of-input only once *every* handle to its write end is closed. The copy
/// this process held went to the child at spawn time, so after a proven tree
/// kill the only thing that can still hold one is a process the kill did not
/// reach — the pipe is inherited by descendants, and a descendant that escapes
/// supervision takes a copy with it. Waiting longer does not help: the wait is
/// then bounded by the leaked process's lifetime, which nothing on this side
/// controls. Two seconds matches the tree-kill grace next door and is orders of
/// magnitude beyond the scheduler latency a reader that *can* finish needs.
pub const READER_EXIT_GRACE: Duration = Duration::from_secs(2);

/// Whether the reader thread reached the end of the agent's output.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReaderExit {
    /// The reader observed end-of-input (or a transport failure) and returned.
    Finished,
    /// The reader is still parked in a read after the grace period, so some
    /// process outside this client still holds the pipe's write end.
    Blocked,
}

/// Reader thread plus a signal that can be waited on with a timeout.
///
/// `JoinHandle::join` has no bounded form, and the thread it names may be
/// parked in a blocking read that nothing on this side can interrupt.
struct ReaderJoin {
    handle: JoinHandle<()>,
    /// Disconnects when the reader closure ends, on return or on unwind.
    /// Nothing is ever sent on it; disconnection is the whole signal.
    exited: Receiver<()>,
}

enum ReaderState {
    Running(ReaderJoin),
    Resolved(ReaderExit),
}

/// NDJSON JSON-RPC client bound to one child process.
pub struct RpcClient {
    shared: Arc<Shared>,
    reader: Mutex<ReaderState>,
    request_timeout: Duration,
}

impl RpcClient {
    pub fn start<W, R>(
        stdin: W,
        stdout: R,
        max_line_bytes: usize,
        request_timeout: Duration,
    ) -> Self
    where
        W: Write + Send + 'static,
        R: Read + Send + 'static,
    {
        let shared = Arc::new(Shared {
            stdin: Mutex::new(Some(Box::new(stdin))),
            pending: Mutex::new(HashMap::new()),
            next_id: AtomicU64::new(1),
            closed: AtomicBool::new(false),
            event_tx: Mutex::new(None),
        });
        let reader_shared = Arc::clone(&shared);
        let (exited_tx, exited) = mpsc::channel();
        let handle = thread::Builder::new()
            .name("grok-acp-reader".into())
            .spawn(move || {
                // Held for its drop, which is what tells `join_reader` this
                // closure is done — including when it unwinds.
                let _exited = exited_tx;
                reader_loop(reader_shared, stdout, max_line_bytes)
            })
            .expect("spawn reader thread");
        Self {
            shared,
            reader: Mutex::new(ReaderState::Running(ReaderJoin { handle, exited })),
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

    pub fn respond_error(
        &self,
        id: Value,
        code: i64,
        message: &'static str,
    ) -> Result<(), RuntimeError> {
        let response = json!({
            "jsonrpc": "2.0",
            "id": id,
            "error": { "code": code, "message": message }
        });
        self.write_message(&response)
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

    /// Wait up to `grace` for the reader thread to finish, and report whether
    /// it did.
    ///
    /// [`ReaderExit::Blocked`] stops tracking the thread rather than waiting it
    /// out. It is parked in a read on a pipe whose write end someone else still
    /// holds; it will observe end-of-input and exit on its own once that holder
    /// lets go. Until then it touches only its own `Shared`, which
    /// [`Self::close`] has already emptied of pending requests — anything it
    /// still dispatches goes to an event channel whose receiver ended with the
    /// turn. Every later call answers `Blocked` immediately, so a shutdown
    /// followed by a drop cannot pay the grace twice.
    pub fn join_reader(&self, grace: Duration) -> ReaderExit {
        let mut state = self.reader.lock().expect("reader lock");
        let exit = match &*state {
            ReaderState::Resolved(exit) => return *exit,
            ReaderState::Running(join) => match join.exited.recv_timeout(grace) {
                Err(RecvTimeoutError::Timeout) => ReaderExit::Blocked,
                // Disconnected: the reader closure ended and dropped its sender.
                _ => ReaderExit::Finished,
            },
        };
        match std::mem::replace(&mut *state, ReaderState::Resolved(exit)) {
            // The closure has already ended, so this only reaps the thread.
            ReaderState::Running(join) if exit == ReaderExit::Finished => {
                let _ = join.handle.join();
            }
            // Dropping the handle detaches the still-parked thread.
            _ => {}
        }
        exit
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
        // Bounded for the same reason shutdown is: a reader parked on a pipe
        // someone else holds open must not turn dropping this client into a
        // hang. Callers that need the outcome ask for it before dropping.
        let _ = self.join_reader(READER_EXIT_GRACE);
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

fn reader_loop<R: Read>(shared: Arc<Shared>, stdout: R, max_line_bytes: usize) {
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

#[cfg(test)]
mod tests {
    use std::io::{PipeReader, PipeWriter};
    use std::process::{Child, Command, Stdio};
    use std::time::Instant;

    use super::*;

    /// Short enough to keep the suite quick, long enough that a reader with an
    /// open path to end-of-input finishes well inside it.
    const GRACE: Duration = Duration::from_millis(300);

    /// Turns the ignored test below into a parked pipe holder. Without it that
    /// test is inert, so even `--include-ignored` never parks in it.
    const HOLD_PIPE: &str = "KEEPER_ACP_RPC_TEST_HOLD_PIPE";

    /// A client reading a real OS pipe, with the write end handed back so the
    /// caller can play the process that still holds it.
    ///
    /// The reader is parked in a read by the time this returns. Waiting for it
    /// to consume a line is what makes that certain: a reader that has not
    /// reached its first read yet notices a closed client at the top of its
    /// loop and exits, which is a different story than the one under test.
    fn parked_client_on_pipes() -> (RpcClient, PipeWriter, PipeReader) {
        let (agent_stdout, mut agent_stdout_write) = std::io::pipe().expect("stdout pipe");
        let (agent_stdin_read, agent_stdin) = std::io::pipe().expect("stdin pipe");
        let client = RpcClient::start(
            agent_stdin,
            agent_stdout,
            64 * 1024,
            Duration::from_millis(50),
        );

        let (events, incoming) = mpsc::channel();
        client.set_event_sink(Some(events));
        writeln!(agent_stdout_write, r#"{{"jsonrpc":"2.0","method":"ping"}}"#).expect("write line");
        agent_stdout_write.flush().expect("flush line");
        match incoming.recv_timeout(Duration::from_secs(30)) {
            Ok(Incoming::Notification { .. }) => {}
            other => panic!("reader never consumed the line: {other:?}"),
        }
        client.set_event_sink(None);

        (client, agent_stdout_write, agent_stdin_read)
    }

    #[test]
    fn join_reader_gives_up_on_a_held_pipe_and_never_waits_for_it_twice() {
        // One live write end is all it takes: the pipe cannot report
        // end-of-input while any handle to it is open, so the reader stays
        // parked exactly as it does behind a leaked descendant.
        let (client, held_write_end, _agent_stdin) = parked_client_on_pipes();
        client.close();

        let started = Instant::now();
        assert_eq!(client.join_reader(GRACE), ReaderExit::Blocked);
        let waited = started.elapsed();
        assert!(
            waited + Duration::from_millis(20) >= GRACE,
            "answered Blocked without waiting the grace: {waited:?}"
        );

        // The recorded outcome answers every later ask, so a shutdown followed
        // by a drop cannot pay the grace a second time.
        let started = Instant::now();
        assert_eq!(
            client.join_reader(Duration::from_secs(600)),
            ReaderExit::Blocked
        );
        let again = started.elapsed();
        assert!(again < Duration::from_secs(1), "waited again: {again:?}");

        drop(held_write_end);
    }

    #[test]
    fn join_reader_finishes_once_the_last_writer_releases_the_pipe() {
        let (client, held_write_end, _agent_stdin) = parked_client_on_pipes();
        client.close();
        drop(held_write_end);

        assert_eq!(
            client.join_reader(Duration::from_secs(30)),
            ReaderExit::Finished
        );
    }

    #[test]
    fn dropping_the_client_is_bounded_while_a_holder_keeps_the_pipe_open() {
        let (client, held_write_end, _agent_stdin) = parked_client_on_pipes();

        let started = Instant::now();
        drop(client);
        let waited = started.elapsed();

        assert!(
            waited < READER_EXIT_GRACE * 5,
            "drop waited {waited:?} on a pipe nobody was going to close"
        );
        drop(held_write_end);
    }

    /// The whole scenario, with the write end in a process this one does not
    /// own: shutdown returns, and the reader it walked away from still exits by
    /// itself once that process dies.
    #[test]
    fn join_reader_is_bounded_when_a_separate_process_holds_the_pipe() {
        let (agent_stdout, agent_stdout_write) = std::io::pipe().expect("stdout pipe");
        let (_agent_stdin_read, agent_stdin) = std::io::pipe().expect("stdin pipe");
        // The write end moves into the holder and out of this process, so from
        // here on nothing on this side can produce the end-of-input the reader
        // is waiting for. That is precisely where `shutdown` stands after a
        // tree kill that missed a descendant holding an inherited copy.
        let mut holder = KillOnDrop(
            Command::new(std::env::current_exe().expect("test executable"))
                .args([
                    "--ignored",
                    "--exact",
                    "rpc::client::tests::parked_pipe_holder",
                ])
                .env(HOLD_PIPE, "1")
                .stdin(Stdio::from(agent_stdout_write))
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .spawn()
                .expect("spawn pipe holder"),
        );

        let client = RpcClient::start(
            agent_stdin,
            agent_stdout,
            64 * 1024,
            Duration::from_millis(50),
        );
        // Deliberately no `close()`: it would suppress the fatal used below as
        // the reader's completion signal. Blocking does not depend on it — the
        // reader is parked in a read and never looks at the closed flag.
        let (events, incoming) = mpsc::channel();
        client.set_event_sink(Some(events));

        let started = Instant::now();
        assert_eq!(client.join_reader(GRACE), ReaderExit::Blocked);
        let waited = started.elapsed();
        assert!(
            waited < Duration::from_secs(10),
            "join outlasted its grace by far: {waited:?}"
        );

        holder.kill_and_reap();
        // The last write end went with it, so the reader this join walked away
        // from reaches end-of-input and shuts the client down on its own.
        match incoming.recv_timeout(Duration::from_secs(30)) {
            Ok(Incoming::Fatal(_)) => {}
            other => panic!("detached reader never finished: {other:?}"),
        }
        assert!(client.is_closed());
    }

    /// Not a test. This is the surviving process for the test above, started by
    /// exact name with the pipe's write end installed as its stdin so the
    /// handle stays open somewhere this process cannot reach.
    #[test]
    #[ignore = "helper process for join_reader_is_bounded_when_a_separate_process_holds_the_pipe"]
    fn parked_pipe_holder() {
        if std::env::var_os(HOLD_PIPE).is_none() {
            return;
        }
        // The parent kills this. The bound only stops a parked process from
        // outliving the run if that ever fails.
        thread::sleep(Duration::from_secs(30));
    }

    struct KillOnDrop(Child);

    impl KillOnDrop {
        fn kill_and_reap(&mut self) {
            self.0.kill().expect("kill pipe holder");
            self.0.wait().expect("reap pipe holder");
        }
    }

    impl Drop for KillOnDrop {
        fn drop(&mut self) {
            let _ = self.0.kill();
            let _ = self.0.wait();
        }
    }
}
