//! ACP compatibility wrapper over the shared supervised process executor.

use std::collections::{BTreeMap, HashMap};
use std::ffi::OsString;
use std::path::Path;
use std::process::{ChildStdin, ChildStdout};
use std::time::Duration;

use keeper_agent_protocol::RuntimeError;
use keeper_process_executor::{
    ProcessSpec, SecretEnvironment, SupervisedChild, SupervisedProcessRunner,
};

use crate::error::internal;

/// ACP's owned fixed-agent child handle.
///
/// The underlying handle is runtime-neutral and, on Windows, owns a
/// kill-on-close Job Object. ACP keeps this thin type to preserve its existing
/// stdio-facing internal contract.
pub struct ChildHandle {
    child: SupervisedChild,
    #[cfg(debug_assertions)]
    test_fail_after_cleanup: bool,
}

impl ChildHandle {
    pub fn take_stdin(&mut self) -> Option<ChildStdin> {
        self.child.take_stdin()
    }
    pub fn take_stdout(&mut self) -> Option<ChildStdout> {
        self.child.take_stdout()
    }
    pub(crate) fn pid(&self) -> u32 {
        self.child.pid()
    }

    pub fn try_wait(&mut self) -> Result<Option<i32>, RuntimeError> {
        self.child.try_wait().map_err(map_error)
    }

    /// Close stdin and terminate only through the stable owned process/job
    /// handles. The shared executor refuses successful cleanup without proof.
    pub fn kill_tree(&mut self, wait: Duration) -> Result<(), RuntimeError> {
        self.child.kill_tree(wait).map_err(map_error)?;
        #[cfg(debug_assertions)]
        if self.test_fail_after_cleanup {
            return Err(internal("acp_kill_tree_failed: injected test failure"));
        }
        Ok(())
    }
}

impl Drop for ChildHandle {
    fn drop(&mut self) {
        let _ = self.kill_tree(Duration::from_millis(500));
    }
}

/// Spawn ACP with an explicit clean environment and the fixed caller argv.
pub fn spawn_child(
    executable: &Path,
    args: &[String],
    cwd: Option<&Path>,
    env: &HashMap<String, String>,
    secret_env: Option<SecretEnvironment>,
) -> Result<ChildHandle, RuntimeError> {
    let mut clean_env = BTreeMap::new();
    #[cfg(debug_assertions)]
    let test_fail_after_cleanup = env
        .get("KEEPER_ACP_TEST_FAIL_KILL_TREE")
        .is_some_and(|value| value == "1");
    for (name, value) in env {
        if name != "KEEPER_ACP_TEST_FAIL_KILL_TREE" {
            clean_env.insert(OsString::from(name), OsString::from(value));
        }
    }
    let cwd = match cwd {
        Some(path) => path.to_path_buf(),
        None => {
            std::env::current_dir().map_err(|_| internal("acp_working_directory_unavailable"))?
        }
    };
    let spec = ProcessSpec {
        executable: executable.to_path_buf(),
        argv: args.iter().map(OsString::from).collect(),
        cwd,
        env: clean_env,
        // ACP performs request-level cancellation; this only guards the
        // launch-handle API and is not used for RPC lifetime management.
        timeout: Duration::from_secs(180),
        output_byte_cap: 1_048_576,
    };
    let runner = SupervisedProcessRunner::new();
    let child = match secret_env {
        Some(secret_env) => runner.spawn_piped_with_secret_env(spec, secret_env),
        None => runner.spawn_piped(spec),
    }
    .map_err(map_error)?;
    Ok(ChildHandle {
        child,
        #[cfg(debug_assertions)]
        test_fail_after_cleanup,
    })
}

pub(crate) fn process_gone(pid: u32) -> Result<bool, RuntimeError> {
    keeper_process_executor::process_gone(pid).map_err(map_error)
}

fn map_error(error: keeper_process_executor::ProcessError) -> RuntimeError {
    internal(format!("acp_process_executor: {error}"))
}
