//! Spawn and terminate the ACP child process tree.

use std::collections::HashMap;
use std::io::Write;
use std::path::Path;
use std::process::{Child, ChildStdin, ChildStdout, Command, Stdio};
use std::time::{Duration, Instant};

use crate::error::{internal, transport};
use keeper_agent_protocol::RuntimeError;

#[cfg(windows)]
use super::job_windows::{JobObject, SuspendedPrimaryThread};

/// Owned child plus OS process id and (on Windows) a Job Object for tree kill.
pub struct ChildHandle {
    child: Child,
    pid: u32,
    /// PIDs that must be dead after kill_tree (direct child + known descendants).
    tracked_pids: Vec<u32>,
    #[cfg(windows)]
    job: Option<JobObject>,
}

impl ChildHandle {
    pub fn take_stdin(&mut self) -> Option<ChildStdin> {
        self.child.stdin.take()
    }

    pub fn take_stdout(&mut self) -> Option<ChildStdout> {
        self.child.stdout.take()
    }

    pub fn try_wait(&mut self) -> Result<Option<i32>, RuntimeError> {
        match self.child.try_wait() {
            Ok(Some(status)) => Ok(Some(status.code().unwrap_or(-1))),
            Ok(None) => Ok(None),
            Err(e) => Err(transport(format!("try_wait failed: {e}"))),
        }
    }

    pub(crate) fn pid(&self) -> u32 {
        self.pid
    }

    /// Close stdin and terminate the process tree. Fails if cleanup cannot be proved.
    ///
    /// On Windows, termination is driven by a Job Object with kill-on-close (and
    /// an explicit TerminateJobObject). Success is not claimed from the direct
    /// child alone: every tracked PID must be gone.
    pub fn kill_tree(&mut self, wait: Duration) -> Result<(), RuntimeError> {
        if let Some(mut stdin) = self.child.stdin.take() {
            let _ = stdin.flush();
            drop(stdin);
        }

        #[cfg(windows)]
        {
            if let Some(job) = self.job.as_ref() {
                job.terminate_tree();
            }
            // Drop the job handle: JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE kills residuals.
            self.job.take();
        }
        #[cfg(not(windows))]
        {
            kill_process_tree(self.pid);
        }

        let deadline = Instant::now() + wait;
        loop {
            let child_exited = self.try_wait()?.is_some();
            if child_exited && self.all_tracked_gone() {
                return Ok(());
            }
            if Instant::now() >= deadline {
                // Final escalation.
                let _ = self.child.kill();
                let _ = self.child.wait();
                #[cfg(not(windows))]
                {
                    kill_process_tree(self.pid);
                }
                if self.all_tracked_gone() {
                    return Ok(());
                }
                let alive: Vec<u32> = self
                    .tracked_pids
                    .iter()
                    .copied()
                    .filter(|p| !process_gone(*p))
                    .collect();
                return Err(internal(format!(
                    "acp_kill_tree_failed: root pid {} still has live tree members {alive:?}",
                    self.pid
                )));
            }
            std::thread::sleep(Duration::from_millis(20));
        }
    }

    fn all_tracked_gone(&self) -> bool {
        self.tracked_pids.iter().all(|pid| process_gone(*pid))
    }
}

impl Drop for ChildHandle {
    fn drop(&mut self) {
        let _ = self.kill_tree(Duration::from_millis(500));
    }
}

/// Spawn the ACP child with a fully constructed environment (no inherit).
///
/// On Windows the child is created suspended, assigned to a Job Object, then
/// resumed, so it cannot create descendants before ownership is established.
pub fn spawn_child(
    executable: &Path,
    args: &[String],
    cwd: Option<&Path>,
    env: &HashMap<String, String>,
) -> Result<ChildHandle, RuntimeError> {
    if !executable.is_file() {
        return Err(internal(format!(
            "executable is not a regular file: {}",
            executable.display()
        )));
    }

    #[cfg(windows)]
    let job = JobObject::create()?;

    let mut cmd = Command::new(executable);
    cmd.args(args)
        .env_clear()
        .envs(env.iter())
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null());

    if let Some(dir) = cwd {
        cmd.current_dir(dir);
    }

    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        const CREATE_SUSPENDED: u32 = 0x0000_0004;
        cmd.creation_flags(CREATE_NO_WINDOW | CREATE_SUSPENDED);
    }

    let mut child = cmd.spawn().map_err(|e| {
        internal(format!(
            "failed to spawn ACP child {}: {e}",
            executable.display()
        ))
    })?;
    let pid = child.id();

    #[cfg(windows)]
    {
        let primary_thread = match SuspendedPrimaryThread::find_for_process(pid) {
            Ok(thread) => thread,
            Err(err) => {
                terminate_suspended_child(&mut child, job);
                return Err(err);
            }
        };

        use std::os::windows::io::AsRawHandle;
        // SAFETY: Child's process handle is valid for the lifetime of `child`.
        let process_handle = child.as_raw_handle() as windows_sys::Win32::Foundation::HANDLE;
        if let Err(err) = job.assign_process(process_handle) {
            terminate_suspended_child(&mut child, job);
            return Err(err);
        }
        if let Err(err) = primary_thread.resume() {
            terminate_suspended_child(&mut child, job);
            return Err(err);
        }
        Ok(ChildHandle {
            child,
            pid,
            tracked_pids: vec![pid],
            job: Some(job),
        })
    }

    #[cfg(not(windows))]
    {
        Ok(ChildHandle {
            child,
            pid,
            tracked_pids: vec![pid],
        })
    }
}

#[cfg(windows)]
fn terminate_suspended_child(child: &mut Child, job: JobObject) {
    // Assignment may have succeeded before a later failure; cover both paths.
    job.terminate_tree();
    drop(job);
    let _ = child.kill();
    let _ = child.wait();
}

#[cfg(not(windows))]
fn kill_process_tree(pid: u32) {
    let _ = Command::new("kill")
        .args(["-TERM", &format!("-{pid}")])
        .status();
    let _ = Command::new("kill")
        .args(["-KILL", &pid.to_string()])
        .status();
}

pub(crate) fn process_gone(pid: u32) -> bool {
    #[cfg(windows)]
    {
        use windows_sys::Win32::Foundation::{CloseHandle, WAIT_OBJECT_0};
        use windows_sys::Win32::System::Threading::{
            OpenProcess, PROCESS_SYNCHRONIZE, WaitForSingleObject,
        };

        // SAFETY: opens only a synchronization handle for this explicit PID.
        let process = unsafe { OpenProcess(PROCESS_SYNCHRONIZE, 0, pid) };
        if process.is_null() {
            return true;
        }
        // SAFETY: process is a valid synchronization handle, then closed below.
        let exited = unsafe { WaitForSingleObject(process, 0) } == WAIT_OBJECT_0;
        // SAFETY: process was opened above and is no longer needed.
        unsafe {
            let _ = CloseHandle(process);
        }
        exited
    }
    #[cfg(not(windows))]
    {
        let status = Command::new("kill").args(["-0", &pid.to_string()]).status();
        match status {
            Ok(s) => !s.success(),
            Err(_) => true,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn process_gone_for_invalid_pid() {
        // Extremely unlikely to be live.
        assert!(process_gone(u32::MAX - 17));
    }
}
