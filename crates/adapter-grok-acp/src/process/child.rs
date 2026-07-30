//! Spawn and terminate the ACP child process tree.

use std::collections::HashMap;
use std::io::Write;
use std::path::Path;
use std::process::{Child, ChildStdin, ChildStdout, Command, Stdio};
use std::time::{Duration, Instant};

use crate::error::{internal, transport};
use keeper_agent_protocol::RuntimeError;

/// Owned child plus its OS process id for kill-tree cleanup.
pub struct ChildHandle {
    child: Child,
    pid: u32,
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

    /// Close stdin and terminate the process tree. Best-effort bounded wait.
    pub fn kill_tree(&mut self, wait: Duration) -> Result<(), RuntimeError> {
        if let Some(mut stdin) = self.child.stdin.take() {
            let _ = stdin.flush();
            drop(stdin);
        }

        if self.try_wait()?.is_some() {
            return Ok(());
        }

        kill_process_tree(self.pid);

        let deadline = Instant::now() + wait;
        loop {
            if self.try_wait()?.is_some() {
                return Ok(());
            }
            if Instant::now() >= deadline {
                // Final attempt.
                let _ = self.child.kill();
                let _ = self.child.wait();
                if self.try_wait()?.is_some() || process_gone(self.pid) {
                    return Ok(());
                }
                return Err(internal(format!(
                    "acp_kill_tree_failed: pid {} still alive",
                    self.pid
                )));
            }
            std::thread::sleep(Duration::from_millis(20));
        }
    }
}

impl Drop for ChildHandle {
    fn drop(&mut self) {
        let _ = self.kill_tree(Duration::from_millis(500));
    }
}

/// Spawn the ACP child with a fully constructed environment (no inherit).
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
        cmd.creation_flags(CREATE_NO_WINDOW);
    }

    let child = cmd.spawn().map_err(|e| {
        internal(format!(
            "failed to spawn ACP child {}: {e}",
            executable.display()
        ))
    })?;
    let pid = child.id();
    Ok(ChildHandle { child, pid })
}

fn kill_process_tree(pid: u32) {
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        let _ = Command::new("taskkill.exe")
            .args(["/PID", &pid.to_string(), "/T", "/F"])
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .creation_flags(CREATE_NO_WINDOW)
            .status();
    }
    #[cfg(not(windows))]
    {
        let _ = Command::new("kill")
            .args(["-TERM", &format!("-{pid}")])
            .status();
        let _ = Command::new("kill")
            .args(["-KILL", &pid.to_string()])
            .status();
    }
}

fn process_gone(pid: u32) -> bool {
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        // tasklist exit 0 always; parse for image presence is heavy — use OpenProcess via tasklist filter.
        let output = Command::new("tasklist.exe")
            .args(["/FI", &format!("PID eq {pid}"), "/NH"])
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .creation_flags(CREATE_NO_WINDOW)
            .output();
        match output {
            Ok(out) => {
                let text = String::from_utf8_lossy(&out.stdout);
                !text.contains(&pid.to_string())
            }
            Err(_) => false,
        }
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
