use std::collections::BTreeMap;
use std::ffi::OsString;
use std::path::Path;
use std::time::Duration;

use keeper_process_executor::{
    CancellationSignal, PROCESS_OUTPUT_BYTE_CAP, ProcessSpec, SupervisedProcessRunner,
    sensitive_name,
};

use crate::{ExternalCliStatus, RuntimeSlotError};

pub(crate) fn run_status(
    executable: &str,
    args: &[String],
    cwd: &Path,
) -> Result<ExternalCliStatus, RuntimeSlotError> {
    let mut env = BTreeMap::new();
    for (name, value) in std::env::vars_os() {
        if !sensitive_name(&name) {
            env.insert(name, value);
        }
    }
    let receipt = SupervisedProcessRunner::new()
        .run(
            ProcessSpec {
                executable: executable.into(),
                argv: args.iter().map(OsString::from).collect(),
                cwd: cwd.to_path_buf(),
                env,
                timeout: Duration::from_secs(10),
                output_byte_cap: PROCESS_OUTPUT_BYTE_CAP,
            },
            CancellationSignal::new(),
        )
        .map_err(|_| RuntimeSlotError::ExternalStatusUnavailable)?;
    if !receipt.cleanup_proven {
        return Err(RuntimeSlotError::ExternalStatusUnavailable);
    }
    Ok(ExternalCliStatus {
        exit_code: receipt.exit_code,
        stdout: receipt.stdout.text,
        stderr: receipt.stderr.text,
        timed_out: receipt.timed_out,
        truncated: receipt.stdout.truncated || receipt.stderr.truncated,
    })
}
