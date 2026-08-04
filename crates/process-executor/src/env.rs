use std::collections::BTreeMap;
use std::ffi::{OsStr, OsString};

use crate::ProcessError;

const SENSITIVE_PARTS: &[&str] = &[
    "KEY",
    "TOKEN",
    "SECRET",
    "PASSWORD",
    "CREDENTIAL",
    "AUTH",
    "OPENAI",
    "ANTHROPIC",
    "XAI",
    "VOLC",
    "MINIMAX",
    "MIMO",
];

pub fn validate_environment(env: &BTreeMap<OsString, OsString>) -> Result<(), ProcessError> {
    if env.keys().any(|name| sensitive_name(name)) {
        return Err(ProcessError::EnvironmentRejected);
    }
    Ok(())
}

pub fn sensitive_name(name: &OsStr) -> bool {
    let upper = name.to_string_lossy().to_ascii_uppercase();
    SENSITIVE_PARTS.iter().any(|part| upper.contains(part))
}
