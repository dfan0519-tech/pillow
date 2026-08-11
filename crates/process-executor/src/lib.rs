//! Runtime-neutral, structured-argv supervised process execution.
//!
//! This crate never invokes a shell. Callers select an already-approved
//! executable and arguments. Ordinary calls start with only supplied,
//! non-sensitive environment entries; one explicit host-only spawn API adds a
//! typed secret environment value without putting it in a receipt or spec.

mod env;
#[cfg(windows)]
mod job_windows;
mod output;

use std::collections::BTreeMap;
use std::ffi::OsString;
use std::io::{self, Read};
use std::path::PathBuf;
use std::process::{Child, ChildStdin, ChildStdout, Command, Stdio};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use thiserror::Error;

pub use env::sensitive_name;
pub use output::{
    BoundedOutput, OutputChunk, OutputObserver, OutputStreamTag, PROCESS_DISPLAY_LINE_CAP,
    PROCESS_OUTPUT_BYTE_CAP, build_output_chunks, neutralize_control_sequences,
};
use output::{Capture, SharedCapture};

#[derive(Debug, Clone)]
pub struct ProcessSpec {
    pub executable: PathBuf,
    pub argv: Vec<OsString>,
    pub cwd: PathBuf,
    pub env: BTreeMap<OsString, OsString>,
    pub timeout: Duration,
    pub output_byte_cap: usize,
}

#[derive(Debug, Clone)]
pub struct ProcessReceipt {
    pub exit_code: Option<i32>,
    pub duration: Duration,
    pub stdout: BoundedOutput,
    pub stderr: BoundedOutput,
    /// Stream-tagged observation chunks (memory-only; not authoritative alone).
    pub chunks: Vec<OutputChunk>,
    pub cancelled: bool,
    pub timed_out: bool,
    pub cleanup_proven: bool,
}

/// Host-only secret environment for one owned child process.
///
/// Ordinary [`ProcessSpec::env`] remains non-secret and rejects sensitive
/// names. This type is intentionally non-`Debug`, non-`Clone`, and
/// non-serializable; callers must opt into the separate spawn API.
pub struct SecretEnvironment {
    name: OsString,
    value: Vec<u8>,
}

impl SecretEnvironment {
    pub fn single(name: &str, value: Vec<u8>) -> Result<Self, ProcessError> {
        if name.is_empty()
            || name.contains(['\0', '='])
            || !sensitive_name(std::ffi::OsStr::new(name))
            || value.is_empty()
            || value.len() > 65_536
            || value.contains(&0)
            || std::str::from_utf8(&value).is_err()
        {
            return Err(ProcessError::EnvironmentRejected);
        }
        Ok(Self {
            name: OsString::from(name),
            value,
        })
    }

    fn apply(&self, command: &mut Command) -> Result<(), ProcessError> {
        let value =
            std::str::from_utf8(&self.value).map_err(|_| ProcessError::EnvironmentRejected)?;
        command.env(&self.name, value);
        Ok(())
    }
}

impl Drop for SecretEnvironment {
    fn drop(&mut self) {
        use std::sync::atomic::{Ordering, compiler_fence};
        for byte in &mut self.value {
            // SAFETY: `byte` is a valid uniquely borrowed byte in this vector.
            unsafe { std::ptr::write_volatile(byte, 0) };
        }
        compiler_fence(Ordering::SeqCst);
    }
}

#[derive(Debug, Error, Clone, Copy, PartialEq, Eq)]
pub enum ProcessError {
    #[error("process executable unavailable")]
    ExecutableUnavailable,
    #[error("process environment rejected")]
    EnvironmentRejected,
    #[error("process launch failed")]
    Spawn,
    #[error("process setup failed")]
    Setup,
    #[error("process wait failed")]
    Wait,
    #[error("process output capture failed")]
    Output,
    #[error("process cleanup could not be proven")]
    CleanupUnproven,
}

#[derive(Debug, Clone, Default)]
pub struct CancellationSignal(Arc<AtomicBool>);

impl CancellationSignal {
    pub fn new() -> Self {
        Self::default()
    }
    pub fn cancel(&self) {
        self.0.store(true, Ordering::Release);
    }
    pub fn is_cancelled(&self) -> bool {
        self.0.load(Ordering::Acquire)
    }
}

#[derive(Debug, Default, Clone, Copy)]
pub struct SupervisedProcessRunner;

impl SupervisedProcessRunner {
    pub fn new() -> Self {
        Self
    }

    pub fn run(
        &self,
        spec: ProcessSpec,
        cancellation: CancellationSignal,
    ) -> Result<ProcessReceipt, ProcessError> {
        self.run_with_observer(spec, cancellation, None)
    }

    /// Run with an optional non-authoritative live output observer.
    ///
    /// The observer may see partial bytes before terminal completion. It must
    /// remain bounded and non-blocking. Authority stays with the final receipt.
    pub fn run_with_observer(
        &self,
        spec: ProcessSpec,
        cancellation: CancellationSignal,
        observer: Option<OutputObserver>,
    ) -> Result<ProcessReceipt, ProcessError> {
        validate_spec(&spec)?;
        let started = Instant::now();
        let mut child = SupervisedChild::spawn(spec, true, true, None)?;
        let stdout = child.take_stdout().ok_or(ProcessError::Setup)?;
        let stderr = child.take_stderr().ok_or(ProcessError::Setup)?;
        let capture = Capture::new_with_observer(child.output_cap, observer);
        let stdout_reader = capture_stream(stdout, Arc::clone(&capture), true);
        let stderr_reader = capture_stream(stderr, Arc::clone(&capture), false);
        child.resume()?;

        let mut cancelled = false;
        let mut timed_out = false;
        let exit_code = loop {
            if cancellation.is_cancelled() {
                cancelled = true;
                child.kill_tree(Duration::from_secs(2))?;
                break child.try_wait()?;
            }
            if started.elapsed() >= child.timeout {
                timed_out = true;
                child.kill_tree(Duration::from_secs(2))?;
                break child.try_wait()?;
            }
            if let Some(status) = child.try_wait()? {
                break Some(status);
            }
            thread::sleep(Duration::from_millis(10));
        };
        join_capture(stdout_reader)?;
        join_capture(stderr_reader)?;
        let capture = Arc::try_unwrap(capture)
            .map_err(|_| ProcessError::Output)?
            .into_inner()
            .map_err(|_| ProcessError::Output)?;
        let (stdout, stderr, chunks) = capture.finish();
        let cleanup_proven = if cancelled || timed_out {
            true
        } else {
            child.kill_tree(Duration::from_secs(2))?;
            true
        };
        Ok(ProcessReceipt {
            exit_code,
            duration: started.elapsed(),
            stdout,
            stderr,
            chunks,
            cancelled,
            timed_out,
            cleanup_proven,
        })
    }

    /// Spawn an owned, piped ACP-style child. It shares the stable-handle
    /// Job Object lifecycle but leaves stdout consumption to the protocol.
    pub fn spawn_piped(&self, spec: ProcessSpec) -> Result<SupervisedChild, ProcessError> {
        validate_spec(&spec)?;
        let mut child = SupervisedChild::spawn(spec, true, false, None)?;
        child.resume()?;
        Ok(child)
    }

    /// Spawn a piped owned child with one host-supplied secret environment
    /// value. The general process API cannot use this path implicitly.
    pub fn spawn_piped_with_secret_env(
        &self,
        spec: ProcessSpec,
        secret_env: SecretEnvironment,
    ) -> Result<SupervisedChild, ProcessError> {
        validate_spec(&spec)?;
        let mut child = SupervisedChild::spawn(spec, true, false, Some(secret_env))?;
        child.resume()?;
        Ok(child)
    }
}

pub struct SupervisedChild {
    child: Child,
    pid: u32,
    timeout: Duration,
    output_cap: usize,
    #[cfg(windows)]
    job: Option<job_windows::JobObject>,
    #[cfg(windows)]
    suspended_primary: Option<job_windows::SuspendedPrimaryThread>,
}

impl SupervisedChild {
    fn spawn(
        spec: ProcessSpec,
        stdin: bool,
        capture_stderr: bool,
        secret_env: Option<SecretEnvironment>,
    ) -> Result<Self, ProcessError> {
        #[cfg(windows)]
        let job = job_windows::JobObject::create()?;
        let mut command = Command::new(&spec.executable);
        command
            .args(&spec.argv)
            .current_dir(&spec.cwd)
            .env_clear()
            .envs(&spec.env);
        if let Some(secret_env) = &secret_env {
            secret_env.apply(&mut command)?;
        }
        command.stdin(if stdin { Stdio::piped() } else { Stdio::null() });
        command.stdout(Stdio::piped());
        command.stderr(if capture_stderr {
            Stdio::piped()
        } else {
            Stdio::null()
        });
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            const CREATE_NO_WINDOW: u32 = 0x0800_0000;
            const CREATE_SUSPENDED: u32 = 0x0000_0004;
            command.creation_flags(CREATE_NO_WINDOW | CREATE_SUSPENDED);
        }
        let mut spawned = command.spawn().map_err(|_| ProcessError::Spawn)?;
        let pid = spawned.id();
        #[cfg(windows)]
        {
            use std::os::windows::io::AsRawHandle;
            let primary = match job_windows::SuspendedPrimaryThread::find_for_process(pid) {
                Ok(thread) => thread,
                Err(error) => return Err(fail_suspended_setup(&mut spawned, job, error)),
            };
            let process = spawned.as_raw_handle() as windows_sys::Win32::Foundation::HANDLE;
            if let Err(error) = job.assign_process(process) {
                return Err(fail_suspended_setup(&mut spawned, job, error));
            }
            #[cfg(debug_assertions)]
            if spec
                .env
                .get(&OsString::from(
                    "KEEPER_PROCESS_EXECUTOR_TEST_SETUP_FAILURE",
                ))
                .is_some_and(|value| value == "1")
            {
                return Err(fail_suspended_setup(&mut spawned, job, ProcessError::Setup));
            }
            Ok(Self {
                child: spawned,
                pid,
                timeout: spec.timeout,
                output_cap: spec.output_byte_cap,
                job: Some(job),
                suspended_primary: Some(primary),
            })
        }
        #[cfg(not(windows))]
        {
            Ok(Self {
                child: spawned,
                pid,
                timeout: spec.timeout,
                output_cap: spec.output_byte_cap,
            })
        }
    }

    fn resume(&mut self) -> Result<(), ProcessError> {
        #[cfg(windows)]
        if let Some(primary) = self.suspended_primary.take() {
            primary.resume()?;
        }
        Ok(())
    }

    pub fn take_stdin(&mut self) -> Option<ChildStdin> {
        self.child.stdin.take()
    }
    pub fn take_stdout(&mut self) -> Option<ChildStdout> {
        self.child.stdout.take()
    }
    pub fn take_stderr(&mut self) -> Option<std::process::ChildStderr> {
        self.child.stderr.take()
    }
    pub fn pid(&self) -> u32 {
        self.pid
    }

    pub fn try_wait(&mut self) -> Result<Option<i32>, ProcessError> {
        self.child
            .try_wait()
            .map(|status| status.map(|value| value.code().unwrap_or(-1)))
            .map_err(|_| ProcessError::Wait)
    }

    pub fn kill_tree(&mut self, wait: Duration) -> Result<(), ProcessError> {
        drop(self.child.stdin.take());
        #[cfg(windows)]
        let terminate_result = if let Some(job) = self.job.as_ref() {
            job.terminate_tree()
        } else {
            Ok(())
        };
        #[cfg(not(windows))]
        {
            let _ = self.child.kill();
        }
        let deadline = Instant::now() + wait;
        loop {
            if self.try_wait()?.is_some() && self.tree_gone()? {
                #[cfg(windows)]
                terminate_result?;
                #[cfg(windows)]
                self.job.take();
                return Ok(());
            }
            if Instant::now() >= deadline {
                break;
            }
            thread::sleep(Duration::from_millis(10));
        }
        let _ = self.child.kill();
        let _ = self.child.wait();
        if self.tree_gone()? {
            #[cfg(windows)]
            terminate_result?;
            #[cfg(windows)]
            self.job.take();
            Ok(())
        } else {
            Err(ProcessError::CleanupUnproven)
        }
    }

    fn tree_gone(&mut self) -> Result<bool, ProcessError> {
        if !process_gone(self.pid)? {
            return Ok(false);
        }
        #[cfg(windows)]
        if let Some(job) = self.job.as_ref() {
            return job.is_empty();
        }
        Ok(true)
    }
}

impl Drop for SupervisedChild {
    fn drop(&mut self) {
        let _ = self.kill_tree(Duration::from_millis(500));
    }
}

fn validate_spec(spec: &ProcessSpec) -> Result<(), ProcessError> {
    if !spec.executable.is_file() || !spec.cwd.is_dir() || spec.timeout.is_zero() {
        return Err(ProcessError::ExecutableUnavailable);
    }
    env::validate_environment(&spec.env)
}

fn capture_stream<R: Read + Send + 'static>(
    mut reader: R,
    capture: SharedCapture,
    stdout: bool,
) -> JoinHandle<io::Result<()>> {
    thread::spawn(move || {
        let mut buffer = [0_u8; 8192];
        loop {
            let count = reader.read(&mut buffer)?;
            if count == 0 {
                return Ok(());
            }
            let mut capture = capture
                .lock()
                .map_err(|_| io::Error::other("capture lock"))?;
            capture.append(stdout, &buffer[..count]);
        }
    })
}

fn join_capture(reader: JoinHandle<io::Result<()>>) -> Result<(), ProcessError> {
    reader
        .join()
        .map_err(|_| ProcessError::Output)?
        .map_err(|_| ProcessError::Output)
}

#[cfg(windows)]
fn fail_suspended_setup(
    child: &mut Child,
    job: job_windows::JobObject,
    error: ProcessError,
) -> ProcessError {
    let termination = job.terminate_tree();
    drop(job);
    let _ = child.kill();
    let deadline = Instant::now() + Duration::from_millis(500);
    while Instant::now() < deadline {
        if child.try_wait().ok().flatten().is_some() && process_gone(child.id()).unwrap_or(false) {
            return termination.err().unwrap_or(error);
        }
        thread::sleep(Duration::from_millis(10));
    }
    ProcessError::CleanupUnproven
}

pub fn process_gone(pid: u32) -> Result<bool, ProcessError> {
    #[cfg(windows)]
    {
        use windows_sys::Win32::Foundation::{
            CloseHandle, ERROR_INVALID_PARAMETER, GetLastError, WAIT_OBJECT_0,
        };
        use windows_sys::Win32::System::Threading::{
            OpenProcess, PROCESS_SYNCHRONIZE, WaitForSingleObject,
        };
        let process = unsafe { OpenProcess(PROCESS_SYNCHRONIZE, 0, pid) };
        if process.is_null() {
            return if unsafe { GetLastError() } == ERROR_INVALID_PARAMETER {
                Ok(true)
            } else {
                Err(ProcessError::CleanupUnproven)
            };
        }
        let exited = unsafe { WaitForSingleObject(process, 0) } == WAIT_OBJECT_0;
        unsafe {
            let _ = CloseHandle(process);
        }
        Ok(exited)
    }
    #[cfg(not(windows))]
    {
        let status = Command::new("kill")
            .args(["-0", &pid.to_string()])
            .status()
            .map_err(|_| ProcessError::CleanupUnproven)?;
        Ok(!status.success())
    }
}
