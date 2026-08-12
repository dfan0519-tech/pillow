//! Structured process proposal construction from slot auth + task text.
//!
//! **Never** joins a shell command string. Argv is always a `Vec<String>`.
//! Task text never becomes `program` or `cwd`.

use crate::error::ProposeError;
use crate::types::{
    GOVERNED_CATALOG_PROGRAM, GovernanceCapability, SlotAuthShape, SlotId, SlotRole, TASK_ARGV_FLAG,
};

/// Structured process proposal ready for the existing process-governance chain.
///
/// This is deliberately isomorphic to `ProcessRunArguments` / host process
/// proposals: `program` + structured `argv` + optional `cwd`. Capability is
/// frozen from the target auth and cannot be elevated later.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProposedCommand {
    pub program: String,
    pub argv: Vec<String>,
    pub cwd: Option<String>,
    /// Derived from target auth at construction time; never upgraded.
    pub governance: GovernanceCapability,
}

/// Snapshot of a target slot used for pure construction (caller-resolved).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TargetSlotRecord {
    pub slot_id: SlotId,
    pub role: SlotRole,
    pub auth: SlotAuthShape,
}

/// Build a structured [`ProposedCommand`] from target auth shape + task text.
///
/// - **LocalCli**: `program` = `executable_path`, `argv` = `launch_args` +
///   `[TASK_ARGV_FLAG, task_text]`, governance = `None`.
/// - **ApiKey / GrokLogin**: `program` = catalog label [`GOVERNED_CATALOG_PROGRAM`],
///   `argv` = optional model flag + fixed ACP-style launch tokens + task flag,
///   governance = `Full`.
///
/// Task text is always a **separate argv element** after [`TASK_ARGV_FLAG`].
/// It is never shell-joined and never written into `program` or `cwd`.
pub fn build_proposed_command(
    auth: &SlotAuthShape,
    task_text: &str,
    cwd: Option<&str>,
) -> Result<ProposedCommand, ProposeError> {
    let task = normalize_task_text(task_text)?;
    let cwd = normalize_cwd(cwd, task)?;

    let (program, mut argv, governance) = match auth {
        SlotAuthShape::LocalCli {
            executable_path,
            launch_args,
            status_args: _,
        } => {
            let program = executable_path.trim();
            if program.is_empty() {
                return Err(ProposeError::LocalCliExecutableEmpty);
            }
            for arg in launch_args {
                if arg.is_empty() {
                    return Err(ProposeError::EmptyArgvElement);
                }
            }
            (
                program.to_owned(),
                launch_args.clone(),
                GovernanceCapability::None,
            )
        }
        SlotAuthShape::ApiKey { model_id } => {
            let mut argv = Vec::new();
            let model = model_id.trim();
            if !model.is_empty() {
                argv.push("-m".to_owned());
                argv.push(model.to_owned());
            }
            argv.extend(governed_launch_tokens());
            (
                GOVERNED_CATALOG_PROGRAM.to_owned(),
                argv,
                GovernanceCapability::Full,
            )
        }
        SlotAuthShape::GrokLogin { model_id } => {
            let mut argv = Vec::new();
            if let Some(model) = model_id {
                let model = model.trim();
                if !model.is_empty() {
                    argv.push("-m".to_owned());
                    argv.push(model.to_owned());
                }
            }
            argv.extend(governed_launch_tokens());
            (
                GOVERNED_CATALOG_PROGRAM.to_owned(),
                argv,
                GovernanceCapability::Full,
            )
        }
    };

    if program.is_empty() {
        return Err(ProposeError::ProgramEmpty);
    }
    // Task text must never become program or cwd (belt-and-suspenders).
    if program == task || cwd.as_deref() == Some(task) {
        return Err(ProposeError::TaskTextInProgramOrCwd);
    }

    // Structured: flag and task are independent argv elements — never joined.
    argv.push(TASK_ARGV_FLAG.to_owned());
    argv.push(task.to_owned());

    Ok(ProposedCommand {
        program,
        argv,
        cwd,
        governance,
    })
}

/// Explicitly refuse shell-string construction.
///
/// Role dispatch must not become a bypass of the host's conservative shell
/// segmenter. Any attempt to join program + task into a single shell command
/// is rejected with a clear reason — including when the task contains shell
/// metacharacters that would be dangerous if shell-joined.
pub fn reject_shell_joined_command(program: &str, task_text: &str) -> ProposeError {
    let _ = (program, task_text);
    ProposeError::ShellJoinForbidden
}

/// Characters that make a task dangerous if shell-joined (aligned with the
/// host's conservative shell language fail-closed set). Used only by tests and
/// documentation of the shell-join refusal path — structured argv construction
/// does not consult this list because metacharacters are literal data.
pub fn task_contains_shell_metacharacters(task: &str) -> bool {
    task.chars().any(|ch| {
        matches!(
            ch,
            ';' | '|'
                | '&'
                | '\''
                | '"'
                | '`'
                | '$'
                | '\\'
                | '<'
                | '>'
                | '*'
                | '?'
                | '['
                | ']'
                | '{'
                | '}'
                | '('
                | ')'
                | '~'
                | '#'
                | '%'
                | '!'
                | '^'
                | '\n'
                | '\r'
        )
    })
}

fn governed_launch_tokens() -> Vec<String> {
    // Matches production ACP launch shape (agent --no-leader stdio).
    vec![
        "agent".to_owned(),
        "--no-leader".to_owned(),
        "stdio".to_owned(),
    ]
}

fn normalize_task_text(task_text: &str) -> Result<&str, ProposeError> {
    let task = task_text.trim();
    if task.is_empty() {
        return Err(ProposeError::TaskTextEmpty);
    }
    Ok(task)
}

fn normalize_cwd(cwd: Option<&str>, task: &str) -> Result<Option<String>, ProposeError> {
    match cwd {
        None => Ok(None),
        Some(raw) => {
            let c = raw.trim();
            if c.is_empty() {
                return Ok(None);
            }
            if c == task {
                return Err(ProposeError::TaskTextInProgramOrCwd);
            }
            Ok(Some(c.to_owned()))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn local_cli_builds_structured_argv_with_task_flag() {
        let auth = SlotAuthShape::LocalCli {
            executable_path: "codex".into(),
            launch_args: vec!["exec".into()],
            status_args: vec![],
        };
        let cmd = build_proposed_command(&auth, "implement feature X", Some(".")).unwrap();
        assert_eq!(cmd.program, "codex");
        assert_eq!(
            cmd.argv,
            vec!["exec", TASK_ARGV_FLAG, "implement feature X"]
        );
        assert_eq!(cmd.cwd.as_deref(), Some("."));
        assert_eq!(cmd.governance, GovernanceCapability::None);
    }

    #[test]
    fn shell_metacharacters_stay_literal_in_structured_argv() {
        let task = "fix; rm -rf / && echo done";
        assert!(task_contains_shell_metacharacters(task));
        let auth = SlotAuthShape::LocalCli {
            executable_path: "worker".into(),
            launch_args: vec!["run".into()],
            status_args: vec![],
        };
        let cmd = build_proposed_command(&auth, task, None).unwrap();
        // Single argv element — not shell-split.
        assert_eq!(cmd.argv.last().map(String::as_str), Some(task));
        assert!(!cmd.argv.iter().any(|a| a.contains("&&") && a != task));
        // program/cwd never hold the task
        assert_ne!(cmd.program, task);
        assert!(cmd.cwd.is_none());
    }

    #[test]
    fn shell_join_path_always_rejected() {
        let err = reject_shell_joined_command("bash", "echo hi; rm -rf x");
        assert_eq!(err, ProposeError::ShellJoinForbidden);
        assert_eq!(err.code(), "role_dispatch_shell_join_forbidden");
    }

    #[test]
    fn governed_auth_never_downgrades_capability_tag() {
        let auth = SlotAuthShape::GrokLogin {
            model_id: Some("grok-4".into()),
        };
        let cmd = build_proposed_command(&auth, "review PR", None).unwrap();
        assert_eq!(cmd.program, GOVERNED_CATALOG_PROGRAM);
        assert_eq!(cmd.governance, GovernanceCapability::Full);
        assert!(
            cmd.argv
                .windows(2)
                .any(|w| { w[0] == "-m" && w[1] == "grok-4" })
        );
    }

    #[test]
    fn local_cli_capability_is_none() {
        let auth = SlotAuthShape::LocalCli {
            executable_path: "cli".into(),
            launch_args: vec![],
            status_args: vec![],
        };
        let cmd = build_proposed_command(&auth, "task", None).unwrap();
        assert_eq!(cmd.governance, GovernanceCapability::None);
        assert!(!cmd.governance.is_governed());
    }
}
