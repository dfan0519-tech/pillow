//! Closed, structured Git mutation requests.
//!
//! This module is the argv construction boundary. Public callers provide only
//! semantic values. Private validated value types are the only values accepted
//! by `PreparedGitMutation`, and only that opaque prepared type can construct
//! mutation argv. Commit text stays out of argv and is exposed solely as stdin.

use std::ffi::OsString;

use crate::GitReadError;
use crate::portable_path::validate_portable_path_or_directory;

/// Maximum UTF-8 byte length of one commit message.
pub const COMMIT_MESSAGE_MAX_LEN: usize = 4_096;
/// Maximum UTF-8 byte length of one branch name.
pub const BRANCH_NAME_MAX_LEN: usize = 255;
/// Maximum UTF-8 byte length of one remote name.
pub const REMOTE_NAME_MAX_LEN: usize = 64;

const PATHSPEC_COUNT_MAX: usize = 256;
const PATHSPEC_TOTAL_BYTE_MAX: usize = 64 * 1024;

/// Opaque rejected-input class. Values are never retained in the error.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GitInputKind {
    CommitMessage,
    BranchName,
    RemoteName,
    RemoteTarget,
}

/// Closed mutation operation set.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GitMutationKind {
    Stage,
    Unstage,
    Commit,
    Push,
}

/// Successful structured mutation metadata. It never contains argv or paths.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GitMutationReceipt {
    pub operation: GitMutationKind,
    pub remote_name: Option<String>,
    pub branch_name: Option<String>,
}

struct ValidatedPathspec(String);
struct ValidatedCommitMessage(String);
struct ValidatedBranchName(String);
struct ValidatedRemoteName(String);

/// Crate-visible opaque operation: the runner can call semantic accessors but
/// cannot construct a variant or inject unvalidated values.
pub(crate) struct PreparedGitMutation {
    kind: PreparedGitMutationKind,
}

enum PreparedGitMutationKind {
    Stage(Vec<ValidatedPathspec>),
    Unstage(Vec<ValidatedPathspec>),
    Commit(ValidatedCommitMessage),
    Push {
        remote: ValidatedRemoteName,
        branch: ValidatedBranchName,
    },
}

impl PreparedGitMutation {
    pub(crate) fn stage(
        repository_project_relative: &str,
        paths: &[String],
    ) -> Result<Self, GitReadError> {
        Ok(Self {
            kind: PreparedGitMutationKind::Stage(validate_pathspecs(
                repository_project_relative,
                paths,
            )?),
        })
    }

    pub(crate) fn unstage(
        repository_project_relative: &str,
        paths: &[String],
    ) -> Result<Self, GitReadError> {
        Ok(Self {
            kind: PreparedGitMutationKind::Unstage(validate_pathspecs(
                repository_project_relative,
                paths,
            )?),
        })
    }

    pub(crate) fn commit(message: &str) -> Result<Self, GitReadError> {
        Ok(Self {
            kind: PreparedGitMutationKind::Commit(ValidatedCommitMessage::new(message)?),
        })
    }

    pub(crate) fn push(remote: &str, branch: &str) -> Result<Self, GitReadError> {
        // Validate both values before any repository or remote probe is run.
        let remote = ValidatedRemoteName::new(remote)?;
        let branch = ValidatedBranchName::new(branch)?;
        Ok(Self {
            kind: PreparedGitMutationKind::Push { remote, branch },
        })
    }

    /// Fixed argv assembled only from literals and validated value types.
    pub(crate) fn argv(&self) -> Vec<OsString> {
        let mut argv = fixed_global_argv();
        match &self.kind {
            PreparedGitMutationKind::Stage(paths) => {
                argv.extend([OsString::from("add"), OsString::from("--")]);
                argv.extend(paths.iter().map(|path| OsString::from(path.as_str())));
            }
            PreparedGitMutationKind::Unstage(paths) => {
                // Path-scoped mixed reset changes only the index, never the
                // worktree. `--hard` is not represented anywhere in this enum.
                argv.extend([OsString::from("reset"), OsString::from("--")]);
                argv.extend(paths.iter().map(|path| OsString::from(path.as_str())));
            }
            PreparedGitMutationKind::Commit(_) => {
                argv.extend([
                    OsString::from("commit"),
                    OsString::from("--no-verify"),
                    OsString::from("--no-gpg-sign"),
                    OsString::from("-F"),
                    OsString::from("-"),
                ]);
            }
            PreparedGitMutationKind::Push { remote, branch } => {
                argv.extend([
                    OsString::from("push"),
                    OsString::from("--porcelain"),
                    OsString::from("--no-verify"),
                    OsString::from("--no-signed"),
                    OsString::from("--no-follow-tags"),
                    OsString::from("--no-mirror"),
                    OsString::from("--recurse-submodules=no"),
                    OsString::from("--"),
                    OsString::from(remote.as_str()),
                    OsString::from(branch.refspec()),
                ]);
            }
        }
        argv
    }

    /// Fixed read-only probe used before push to classify every effective URL.
    pub(crate) fn push_url_probe_argv(&self) -> Option<Vec<OsString>> {
        let PreparedGitMutationKind::Push { remote, .. } = &self.kind else {
            return None;
        };
        let mut argv = fixed_global_argv();
        argv.extend([
            OsString::from("remote"),
            OsString::from("get-url"),
            OsString::from("--push"),
            OsString::from("--all"),
            OsString::from("--"),
            OsString::from(remote.as_str()),
        ]);
        Some(argv)
    }

    pub(crate) fn stdin_bytes(&self) -> Option<&[u8]> {
        match &self.kind {
            PreparedGitMutationKind::Commit(message) => Some(message.0.as_bytes()),
            _ => None,
        }
    }

    pub(crate) fn pathspecs(&self) -> impl Iterator<Item = &str> {
        let paths = match &self.kind {
            PreparedGitMutationKind::Stage(paths) | PreparedGitMutationKind::Unstage(paths) => {
                paths.as_slice()
            }
            PreparedGitMutationKind::Commit(_) | PreparedGitMutationKind::Push { .. } => &[],
        };
        paths.iter().map(ValidatedPathspec::as_str)
    }

    pub(crate) fn receipt(&self) -> GitMutationReceipt {
        match &self.kind {
            PreparedGitMutationKind::Stage(_) => receipt(GitMutationKind::Stage),
            PreparedGitMutationKind::Unstage(_) => receipt(GitMutationKind::Unstage),
            PreparedGitMutationKind::Commit(_) => receipt(GitMutationKind::Commit),
            PreparedGitMutationKind::Push { remote, branch } => GitMutationReceipt {
                operation: GitMutationKind::Push,
                remote_name: Some(remote.0.clone()),
                branch_name: Some(branch.0.clone()),
            },
        }
    }
}

fn receipt(operation: GitMutationKind) -> GitMutationReceipt {
    GitMutationReceipt {
        operation,
        remote_name: None,
        branch_name: None,
    }
}

fn fixed_global_argv() -> Vec<OsString> {
    vec![
        OsString::from("--no-pager"),
        OsString::from("--no-optional-locks"),
        OsString::from("-c"),
        OsString::from("core.fsmonitor="),
        OsString::from("-c"),
        OsString::from("core.pager=cat"),
        OsString::from("-c"),
        OsString::from("credential.helper="),
        OsString::from("-c"),
        OsString::from("commit.gpgSign=false"),
        OsString::from("-c"),
        OsString::from("push.gpgSign=false"),
        OsString::from("-c"),
        OsString::from("push.followTags=false"),
        OsString::from("-c"),
        OsString::from("protocol.allow=never"),
        OsString::from("-c"),
        OsString::from("protocol.https.allow=always"),
        OsString::from("-c"),
        OsString::from("protocol.file.allow=always"),
        OsString::from("-c"),
        OsString::from("maintenance.auto=false"),
        OsString::from("-c"),
        OsString::from("gc.auto=0"),
    ]
}

fn validate_pathspecs(
    repository_project_relative: &str,
    paths: &[String],
) -> Result<Vec<ValidatedPathspec>, GitReadError> {
    if paths.is_empty() || paths.len() > PATHSPEC_COUNT_MAX {
        return Err(GitReadError::PathRejected);
    }
    let mut total = 0usize;
    let mut validated = Vec::with_capacity(paths.len());
    for path in paths {
        total = total
            .checked_add(path.len())
            .ok_or(GitReadError::PathRejected)?;
        if total > PATHSPEC_TOTAL_BYTE_MAX {
            return Err(GitReadError::PathRejected);
        }
        validated.push(ValidatedPathspec::new(repository_project_relative, path)?);
    }
    Ok(validated)
}

impl ValidatedPathspec {
    fn new(repository_project_relative: &str, path: &str) -> Result<Self, GitReadError> {
        validate_portable_path_or_directory(path).map_err(|_| GitReadError::PathRejected)?;
        let repo_relative = strip_repository_prefix(repository_project_relative, path)?;
        validate_portable_path_or_directory(&repo_relative)
            .map_err(|_| GitReadError::PathRejected)?;
        if repo_relative.starts_with('-') {
            return Err(GitReadError::PathRejected);
        }
        Ok(Self(repo_relative))
    }

    fn as_str(&self) -> &str {
        &self.0
    }
}

fn strip_repository_prefix(repository: &str, project_path: &str) -> Result<String, GitReadError> {
    if repository == "." {
        return Ok(project_path.to_owned());
    }
    let prefix = format!("{repository}/");
    if let Some(relative) = project_path.strip_prefix(&prefix) {
        if relative.is_empty() {
            return Err(GitReadError::PathRejected);
        }
        return Ok(relative.to_owned());
    }
    if project_path == repository {
        return Err(GitReadError::PathRejected);
    }
    // Existing read APIs accept either project-relative or already
    // repository-relative values. Preserve that closed portable convention.
    Ok(project_path.to_owned())
}

impl ValidatedCommitMessage {
    fn new(message: &str) -> Result<Self, GitReadError> {
        if message.is_empty()
            || message.len() > COMMIT_MESSAGE_MAX_LEN
            || message.starts_with('-')
            || message.chars().any(char::is_control)
        {
            return Err(GitReadError::InputRejected {
                kind: GitInputKind::CommitMessage,
            });
        }
        Ok(Self(message.to_owned()))
    }
}

impl ValidatedBranchName {
    fn new(branch: &str) -> Result<Self, GitReadError> {
        let invalid_shape = branch.is_empty()
            || branch.len() > BRANCH_NAME_MAX_LEN
            || branch.starts_with('-')
            || branch.starts_with('/')
            || branch.ends_with('/')
            || branch.ends_with('.')
            || branch.ends_with(".lock")
            || branch.contains("//")
            || branch.contains("..")
            || branch.contains("@{")
            || branch.eq_ignore_ascii_case("HEAD")
            || branch
                .split('/')
                .any(|component| component.starts_with('.') || component.ends_with(".lock"));
        let invalid_character = branch.chars().any(|character| {
            character.is_control()
                || !(character.is_ascii_alphanumeric()
                    || matches!(character, '-' | '_' | '.' | '/'))
        });
        if invalid_shape || invalid_character {
            return Err(GitReadError::InputRejected {
                kind: GitInputKind::BranchName,
            });
        }
        Ok(Self(branch.to_owned()))
    }

    fn refspec(&self) -> String {
        format!("refs/heads/{0}:refs/heads/{0}", self.0)
    }
}

impl ValidatedRemoteName {
    fn new(remote: &str) -> Result<Self, GitReadError> {
        let mut characters = remote.chars();
        let first = characters.next();
        let invalid = remote.is_empty()
            || remote.len() > REMOTE_NAME_MAX_LEN
            || remote.starts_with('-')
            || remote.ends_with('.')
            || remote.contains("..")
            || first.is_none_or(|character| !character.is_ascii_alphanumeric())
            || characters.any(|character| {
                character.is_control()
                    || !(character.is_ascii_alphanumeric() || matches!(character, '-' | '_' | '.'))
            });
        if invalid {
            return Err(GitReadError::InputRejected {
                kind: GitInputKind::RemoteName,
            });
        }
        Ok(Self(remote.to_owned()))
    }

    fn as_str(&self) -> &str {
        &self.0
    }
}

#[cfg(test)]
mod tests {
    use super::{
        BRANCH_NAME_MAX_LEN, COMMIT_MESSAGE_MAX_LEN, PreparedGitMutation, REMOTE_NAME_MAX_LEN,
    };

    fn strings(argv: Vec<std::ffi::OsString>) -> Vec<String> {
        argv.into_iter()
            .map(|value| value.to_string_lossy().into_owned())
            .collect()
    }

    fn fixed_prefix() -> Vec<String> {
        [
            "--no-pager",
            "--no-optional-locks",
            "-c",
            "core.fsmonitor=",
            "-c",
            "core.pager=cat",
            "-c",
            "credential.helper=",
            "-c",
            "commit.gpgSign=false",
            "-c",
            "push.gpgSign=false",
            "-c",
            "push.followTags=false",
            "-c",
            "protocol.allow=never",
            "-c",
            "protocol.https.allow=always",
            "-c",
            "protocol.file.allow=always",
            "-c",
            "maintenance.auto=false",
            "-c",
            "gc.auto=0",
        ]
        .into_iter()
        .map(str::to_owned)
        .collect()
    }

    #[test]
    fn commit_argv_is_fixed_and_message_exists_only_on_stdin() {
        let prepared = PreparedGitMutation::commit("normal commit").expect("prepare");
        let argv = strings(prepared.argv());
        let mut expected = fixed_prefix();
        expected.extend(
            ["commit", "--no-verify", "--no-gpg-sign", "-F", "-"]
                .into_iter()
                .map(str::to_owned),
        );
        assert_eq!(argv, expected);
        assert!(!argv.iter().any(|value| value == "normal commit"));
        assert_eq!(prepared.stdin_bytes(), Some("normal commit".as_bytes()));
    }

    #[test]
    fn mutation_templates_have_no_arbitrary_operation_variant() {
        let stage = PreparedGitMutation::stage(".", &["src/lib.rs".to_owned()]).expect("stage");
        let unstage =
            PreparedGitMutation::unstage(".", &["src/lib.rs".to_owned()]).expect("unstage");
        let push = PreparedGitMutation::push("origin", "feature/safe").expect("push");

        let mut expected_stage = fixed_prefix();
        expected_stage.extend(["add", "--", "src/lib.rs"].into_iter().map(str::to_owned));
        assert_eq!(strings(stage.argv()), expected_stage);

        let mut expected_unstage = fixed_prefix();
        expected_unstage.extend(["reset", "--", "src/lib.rs"].into_iter().map(str::to_owned));
        assert_eq!(strings(unstage.argv()), expected_unstage);

        let mut expected_push = fixed_prefix();
        expected_push.extend(
            [
                "push",
                "--porcelain",
                "--no-verify",
                "--no-signed",
                "--no-follow-tags",
                "--no-mirror",
                "--recurse-submodules=no",
                "--",
                "origin",
                "refs/heads/feature/safe:refs/heads/feature/safe",
            ]
            .into_iter()
            .map(str::to_owned),
        );
        assert_eq!(strings(push.argv()), expected_push);

        let mut expected_probe = fixed_prefix();
        expected_probe.extend(
            ["remote", "get-url", "--push", "--all", "--", "origin"]
                .into_iter()
                .map(str::to_owned),
        );
        assert_eq!(
            strings(push.push_url_probe_argv().expect("push URL probe")),
            expected_probe
        );
    }

    #[test]
    fn nested_repository_paths_are_reduced_only_after_portable_validation() {
        let prepared =
            PreparedGitMutation::stage("vendor/repo", &["vendor/repo/src/lib.rs".to_owned()])
                .expect("nested stage");
        assert_eq!(prepared.pathspecs().collect::<Vec<_>>(), vec!["src/lib.rs"]);
    }

    #[test]
    fn scalar_length_caps_accept_the_limit_and_reject_one_byte_more() {
        assert!(PreparedGitMutation::commit(&"m".repeat(COMMIT_MESSAGE_MAX_LEN)).is_ok());
        assert!(PreparedGitMutation::commit(&"m".repeat(COMMIT_MESSAGE_MAX_LEN + 1)).is_err());

        assert!(PreparedGitMutation::push("origin", &"b".repeat(BRANCH_NAME_MAX_LEN)).is_ok());
        assert!(PreparedGitMutation::push("origin", &"b".repeat(BRANCH_NAME_MAX_LEN + 1)).is_err());

        assert!(PreparedGitMutation::push(&"r".repeat(REMOTE_NAME_MAX_LEN), "main").is_ok());
        assert!(PreparedGitMutation::push(&"r".repeat(REMOTE_NAME_MAX_LEN + 1), "main").is_err());
    }

    #[test]
    fn every_branch_component_rejects_the_reserved_lock_suffix() {
        assert!(PreparedGitMutation::push("origin", "topic.lock/main").is_err());
        assert!(PreparedGitMutation::push("origin", "topic/main.lock").is_err());
        assert!(PreparedGitMutation::push("origin", "topic/lock-main").is_ok());
    }
}
