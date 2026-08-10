//! Strict standard ACP `terminal/*` normalization and completed-terminal cache.

use std::collections::HashMap;
use std::path::Path;

use keeper_agent_protocol::{
    GovernedShellReceipt, ProviderShellProposal, canonical_arguments_digest,
};
use serde_json::{Map, Value, json};

use crate::provider_tools::project_relative_path;

pub(crate) const TERMINAL_CREATE_METHOD: &str = "terminal/create";
pub(crate) const TERMINAL_OUTPUT_METHOD: &str = "terminal/output";
pub(crate) const TERMINAL_RELEASE_METHOD: &str = "terminal/release";
pub(crate) const TERMINAL_WAIT_METHOD: &str = "terminal/wait_for_exit";
pub(crate) const TERMINAL_KILL_METHOD: &str = "terminal/kill";

const TERMINAL_OUTPUT_BYTE_CAP: usize = 262_144;
const TERMINALS_PER_PROMPT: usize = 32;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ProviderTerminalRequestError {
    MethodNotFound,
    InvalidParams,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ProviderTerminalResponseError {
    Rejected,
    Expired,
    Cancelled,
    Unavailable,
}

impl ProviderTerminalResponseError {
    pub(crate) fn rpc_error(self) -> (i64, &'static str) {
        match self {
            Self::Rejected => (-32060, "governed_terminal_rejected"),
            Self::Expired => (-32061, "governed_terminal_expired"),
            Self::Cancelled => (-32062, "governed_terminal_cancelled"),
            Self::Unavailable => (-32000, "governed_terminal_unavailable"),
        }
    }
}

#[derive(Clone)]
pub(crate) struct NormalizedTerminalCreate {
    pub proposal: ProviderShellProposal,
    output_byte_limit: usize,
}

impl NormalizedTerminalCreate {
    pub(crate) fn output_byte_limit(&self) -> usize {
        self.output_byte_limit
    }

    pub(crate) fn canonical_request_identity(
        &self,
    ) -> Result<String, ProviderTerminalRequestError> {
        let value = json!({
            "command": self.proposal.command,
            "cwd": self.proposal.portable_cwd,
            "output_byte_limit": self.output_byte_limit,
        });
        let digest = canonical_arguments_digest(&value)
            .map_err(|_| ProviderTerminalRequestError::InvalidParams)?;
        Ok(format!("shell.run@1:{}", digest.as_str()))
    }
}

#[derive(Clone)]
pub(crate) struct CompletedTerminal {
    pub call_id: String,
    pub terminal_id: String,
    output: String,
    truncated: bool,
    exit_code: Option<u32>,
    signal: Option<String>,
}

impl std::fmt::Debug for CompletedTerminal {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CompletedTerminal")
            .field("call_id", &self.call_id)
            .field("terminal_id", &self.terminal_id)
            .field("output", &"<redacted>")
            .field("truncated", &self.truncated)
            .field("exit_code", &self.exit_code)
            .field("signal", &self.signal)
            .finish()
    }
}

impl CompletedTerminal {
    pub(crate) fn from_receipt(
        receipt: GovernedShellReceipt,
        call_id: String,
        output_byte_limit: usize,
    ) -> Result<Self, ProviderTerminalResponseError> {
        if receipt.process.started.is_none() {
            let state = receipt
                .process
                .terminal
                .as_ref()
                .and_then(|terminal| serde_json::to_value(terminal).ok())
                .and_then(|terminal| {
                    terminal
                        .get("state")
                        .and_then(Value::as_str)
                        .map(str::to_owned)
                });
            return Err(match state.as_deref() {
                Some("rejected") => ProviderTerminalResponseError::Rejected,
                Some("expired") => ProviderTerminalResponseError::Expired,
                Some("cancelled") => ProviderTerminalResponseError::Cancelled,
                _ => ProviderTerminalResponseError::Unavailable,
            });
        }
        let host_output = receipt
            .output
            .ok_or(ProviderTerminalResponseError::Unavailable)?;
        let (output, limited) = truncate_output_from_start(host_output.output, output_byte_limit);
        let provider = receipt
            .process
            .provider_ok
            .as_ref()
            .ok_or(ProviderTerminalResponseError::Unavailable)?;
        let signal = if receipt.process.timed_out {
            Some("timeout".into())
        } else if receipt.process.cancelled {
            Some("cancelled".into())
        } else {
            None
        };
        Ok(Self {
            call_id,
            terminal_id: receipt.process.proposal_id,
            output,
            truncated: host_output.truncated || limited,
            exit_code: provider.exit_code.map(|code| code as u32),
            signal,
        })
    }

    pub(crate) fn output_response(&self) -> Value {
        json!({
            "output": self.output,
            "truncated": self.truncated,
            "exitStatus": exit_status(self.exit_code, self.signal.as_deref()),
        })
    }

    pub(crate) fn wait_response(&self) -> Value {
        exit_status(self.exit_code, self.signal.as_deref())
    }
}

#[derive(Debug, Default)]
pub(crate) struct ProviderTerminalRegistry {
    terminals: HashMap<String, CompletedTerminal>,
}

impl ProviderTerminalRegistry {
    pub(crate) fn by_call_id(&self, call_id: &str) -> Option<&CompletedTerminal> {
        self.terminals
            .values()
            .find(|terminal| terminal.call_id == call_id)
    }

    pub(crate) fn get(&self, terminal_id: &str) -> Option<&CompletedTerminal> {
        self.terminals.get(terminal_id)
    }

    pub(crate) fn insert(
        &mut self,
        terminal: CompletedTerminal,
    ) -> Result<(), ProviderTerminalResponseError> {
        if !self.terminals.contains_key(&terminal.terminal_id)
            && self.terminals.len() >= TERMINALS_PER_PROMPT
        {
            return Err(ProviderTerminalResponseError::Unavailable);
        }
        self.terminals
            .insert(terminal.terminal_id.clone(), terminal);
        Ok(())
    }

    pub(crate) fn release(&mut self, terminal_id: &str) -> bool {
        self.terminals.remove(terminal_id).is_some()
    }
}

pub(crate) fn is_provider_terminal_method(method: &str) -> bool {
    matches!(
        method,
        TERMINAL_CREATE_METHOD
            | TERMINAL_OUTPUT_METHOD
            | TERMINAL_RELEASE_METHOD
            | TERMINAL_WAIT_METHOD
            | TERMINAL_KILL_METHOD
    )
}

pub(crate) fn normalize_terminal_create(
    method: &str,
    params: &Value,
    active_session_id: &str,
    project_root: &Path,
) -> Result<NormalizedTerminalCreate, ProviderTerminalRequestError> {
    if method != TERMINAL_CREATE_METHOD {
        return Err(ProviderTerminalRequestError::MethodNotFound);
    }
    let object = strict_object(
        params,
        &[
            "sessionId",
            "command",
            "args",
            "env",
            "cwd",
            "outputByteLimit",
            "_meta",
        ],
    )?;
    require_session(object, active_session_id)?;
    let command = object
        .get("command")
        .and_then(Value::as_str)
        .filter(|command| !command.is_empty() && !command.contains('\0'))
        .ok_or(ProviderTerminalRequestError::InvalidParams)?;
    if command.len() > TERMINAL_OUTPUT_BYTE_CAP {
        return Err(ProviderTerminalRequestError::InvalidParams);
    }
    if !object
        .get("args")
        .is_none_or(|args| args.as_array().is_some_and(Vec::is_empty))
        || !object
            .get("env")
            .is_none_or(|env| env.as_array().is_some_and(Vec::is_empty))
    {
        return Err(ProviderTerminalRequestError::InvalidParams);
    }
    let portable_cwd = match object.get("cwd") {
        None | Some(Value::Null) => Some(".".into()),
        Some(Value::String(cwd)) if Path::new(cwd).is_absolute() => Some(
            project_relative_path(project_root, cwd)
                .map_err(|_| ProviderTerminalRequestError::InvalidParams)?,
        ),
        Some(_) => return Err(ProviderTerminalRequestError::InvalidParams),
    };
    let output_byte_limit = match object.get("outputByteLimit") {
        None | Some(Value::Null) => TERMINAL_OUTPUT_BYTE_CAP,
        Some(value) => value
            .as_u64()
            .and_then(|limit| usize::try_from(limit).ok())
            .map(|limit| limit.min(TERMINAL_OUTPUT_BYTE_CAP))
            .ok_or(ProviderTerminalRequestError::InvalidParams)?,
    };

    Ok(NormalizedTerminalCreate {
        proposal: ProviderShellProposal {
            call_id: String::new(),
            command: command.into(),
            portable_cwd,
        },
        output_byte_limit,
    })
}

pub(crate) fn normalize_terminal_reference(
    method: &str,
    params: &Value,
    active_session_id: &str,
) -> Result<String, ProviderTerminalRequestError> {
    if !is_provider_terminal_method(method) || method == TERMINAL_CREATE_METHOD {
        return Err(ProviderTerminalRequestError::MethodNotFound);
    }
    let object = strict_object(params, &["sessionId", "terminalId", "_meta"])?;
    require_session(object, active_session_id)?;
    object
        .get("terminalId")
        .and_then(Value::as_str)
        .filter(|id| {
            !id.is_empty()
                && id.len() <= 120
                && id
                    .bytes()
                    .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_'))
        })
        .map(str::to_owned)
        .ok_or(ProviderTerminalRequestError::InvalidParams)
}

fn require_session(
    object: &Map<String, Value>,
    active_session_id: &str,
) -> Result<(), ProviderTerminalRequestError> {
    if object.get("sessionId").and_then(Value::as_str) != Some(active_session_id) {
        return Err(ProviderTerminalRequestError::InvalidParams);
    }
    Ok(())
}

fn strict_object<'a>(
    value: &'a Value,
    allowed: &[&str],
) -> Result<&'a Map<String, Value>, ProviderTerminalRequestError> {
    let object = value
        .as_object()
        .ok_or(ProviderTerminalRequestError::InvalidParams)?;
    if object.keys().any(|key| !allowed.contains(&key.as_str())) {
        return Err(ProviderTerminalRequestError::InvalidParams);
    }
    Ok(object)
}

fn truncate_output_from_start(output: String, limit: usize) -> (String, bool) {
    if output.len() <= limit {
        return (output, false);
    }
    let mut start = output.len().saturating_sub(limit);
    while start < output.len() && !output.is_char_boundary(start) {
        start += 1;
    }
    (output[start..].to_owned(), true)
}

fn exit_status(exit_code: Option<u32>, signal: Option<&str>) -> Value {
    let mut status = Map::new();
    if let Some(exit_code) = exit_code {
        status.insert("exitCode".into(), Value::from(exit_code));
    }
    if let Some(signal) = signal {
        status.insert("signal".into(), Value::from(signal));
    }
    Value::Object(status)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn create_accepts_only_the_shell_string_branch_and_project_scoped_absolute_cwd() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().join("project");
        std::fs::create_dir(&root).unwrap();
        let params = json!({
            "sessionId":"active",
            "command":"printf 'full command' && echo next",
            "args":[],
            "env":[],
            "cwd":root,
            "outputByteLimit":128,
            "_meta":{"ignored":true}
        });
        let normalized =
            normalize_terminal_create(TERMINAL_CREATE_METHOD, &params, "active", &root).unwrap();
        assert_eq!(
            normalized.proposal.command,
            "printf 'full command' && echo next"
        );
        assert_eq!(normalized.proposal.portable_cwd.as_deref(), Some("."));
        assert_eq!(normalized.output_byte_limit, 128);

        for invalid in [
            json!({"sessionId":"active","command":"echo x","args":["x"]}),
            json!({"sessionId":"active","command":"echo x","env":[{"name":"X","value":"1"}]}),
            json!({"sessionId":"active","command":"echo x","cwd":"relative"}),
            json!({"sessionId":"other","command":"echo x"}),
        ] {
            assert_eq!(
                normalize_terminal_create(TERMINAL_CREATE_METHOD, &invalid, "active", &root,).err(),
                Some(ProviderTerminalRequestError::InvalidParams)
            );
        }
    }

    #[test]
    fn output_limit_discards_the_beginning_only_at_a_utf8_boundary() {
        let (output, truncated) = truncate_output_from_start("éabcd".into(), 4);
        assert_eq!(output, "abcd");
        assert!(truncated);
        let (empty, truncated) = truncate_output_from_start("é".into(), 0);
        assert_eq!(empty, "");
        assert!(truncated);
    }
}
