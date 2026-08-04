use std::fs;

use keeper_agent_protocol::{
    FsListOutput, FsReadOutput, FsSearchOutput, RunId, ToolActiveContext, ToolExecutionState,
    ToolProposal, ToolPublicErrorCode, canonical_arguments_digest,
};
use keeper_tool_executor::{
    CancellationSignal, NeverCancelled, ProjectCapability, ReadToolExecutor,
};
use serde_json::{Value, json};

fn validated(tool_id: &str, arguments: Value) -> keeper_agent_protocol::ValidatedToolProposal {
    let run_id = RunId(uuid::Uuid::new_v4());
    let context = ToolActiveContext::new("project-1", "conversation-1", run_id).unwrap();
    let digest = canonical_arguments_digest(&arguments).unwrap();
    ToolProposal {
        project_id: "project-1".into(),
        conversation_id: "conversation-1".into(),
        run_id,
        call_id: format!("call-{}", uuid::Uuid::new_v4()),
        tool_id: tool_id.into(),
        tool_version: 1,
        arguments,
        arguments_digest: digest,
    }
    .validate_for_context(&context)
    .unwrap()
}

#[test]
fn reads_utf8_with_a_hard_byte_bound_and_reports_truncation() {
    let temp = tempfile::tempdir().unwrap();
    fs::write(temp.path().join("note.txt"), b"hello pillow").unwrap();
    let capability = ProjectCapability::new("project-1", temp.path()).unwrap();
    let proposal = validated(
        "fs.read",
        json!({"path": "note.txt", "offset_bytes": 0, "max_bytes": 5}),
    );

    let result = ReadToolExecutor
        .execute(&proposal, &capability, &NeverCancelled)
        .unwrap();
    let output: FsReadOutput = serde_json::from_value(result.output().unwrap().clone()).unwrap();

    assert_eq!(result.state(), ToolExecutionState::Succeeded);
    assert_eq!(output.content.as_deref(), Some("hello"));
    assert_eq!(output.bytes_returned, 5);
    assert!(output.truncated);
}

#[test]
fn json_expansion_over_the_manifest_cap_returns_a_public_limit_error() {
    let temp = tempfile::tempdir().unwrap();
    fs::write(temp.path().join("controls.txt"), vec![0_u8; 200_000]).unwrap();
    let capability = ProjectCapability::new("project-1", temp.path()).unwrap();
    let proposal = validated(
        "fs.read",
        json!({
            "path": "controls.txt",
            "offset_bytes": 0,
            "max_bytes": 200_000
        }),
    );

    let result = ReadToolExecutor
        .execute(&proposal, &capability, &NeverCancelled)
        .unwrap();

    assert_eq!(result.state(), ToolExecutionState::Failed);
    assert_eq!(
        result.error().unwrap().code(),
        ToolPublicErrorCode::OutputLimitExceeded
    );
}

#[test]
fn lists_direct_children_in_portable_deterministic_order() {
    let temp = tempfile::tempdir().unwrap();
    fs::create_dir(temp.path().join("b-dir")).unwrap();
    fs::write(temp.path().join("z.txt"), b"z").unwrap();
    fs::write(temp.path().join("a.txt"), b"a").unwrap();
    let capability = ProjectCapability::new("project-1", temp.path()).unwrap();
    let proposal = validated("fs.list", json!({"path": ".", "max_results": 20}));

    let result = ReadToolExecutor
        .execute(&proposal, &capability, &NeverCancelled)
        .unwrap();
    let output: FsListOutput = serde_json::from_value(result.output().unwrap().clone()).unwrap();
    let paths = output
        .entries
        .iter()
        .map(|entry| entry.path.as_str())
        .collect::<Vec<_>>();

    assert_eq!(paths, vec!["a.txt", "b-dir", "z.txt"]);
    assert!(!output.truncated);
}

#[test]
fn searches_recursively_without_provider_specific_behavior() {
    let temp = tempfile::tempdir().unwrap();
    fs::create_dir(temp.path().join("src")).unwrap();
    fs::write(
        temp.path().join("src").join("lib.rs"),
        "Pillow owns policy.\nprovider does not.\n",
    )
    .unwrap();
    fs::write(temp.path().join("README.md"), "pillow overview\n").unwrap();
    let capability = ProjectCapability::new("project-1", temp.path()).unwrap();
    let proposal = validated(
        "fs.search",
        json!({
            "path": ".",
            "query": "pillow",
            "case_sensitive": false,
            "max_results": 20
        }),
    );

    let result = ReadToolExecutor
        .execute(&proposal, &capability, &NeverCancelled)
        .unwrap();
    let output: FsSearchOutput = serde_json::from_value(result.output().unwrap().clone()).unwrap();

    assert_eq!(result.state(), ToolExecutionState::Succeeded);
    assert_eq!(output.matches.len(), 2);
    assert_eq!(output.matches[0].path, "README.md");
    assert_eq!(output.matches[1].path, "src/lib.rs");
    assert!(output.matches.iter().all(|item| item.line == 1));
}

#[test]
fn search_preview_is_bounded_even_for_a_very_long_source_line() {
    let temp = tempfile::tempdir().unwrap();
    let line = format!("Pillow {}", "x".repeat(20_000));
    fs::write(temp.path().join("long.txt"), line).unwrap();
    let capability = ProjectCapability::new("project-1", temp.path()).unwrap();
    let proposal = validated(
        "fs.search",
        json!({
            "path": ".",
            "query": "Pillow",
            "case_sensitive": true,
            "max_results": 20
        }),
    );

    let result = ReadToolExecutor
        .execute(&proposal, &capability, &NeverCancelled)
        .unwrap();
    let output: FsSearchOutput = serde_json::from_value(result.output().unwrap().clone()).unwrap();

    assert_eq!(result.state(), ToolExecutionState::Succeeded);
    assert_eq!(output.matches.len(), 1);
    assert!(output.matches[0].preview.len() <= 2_048);
}

struct Cancelled;

impl CancellationSignal for Cancelled {
    fn is_cancelled(&self) -> bool {
        true
    }
}

#[test]
fn cancellation_is_a_validated_terminal_result() {
    let temp = tempfile::tempdir().unwrap();
    fs::write(temp.path().join("note.txt"), "content").unwrap();
    let capability = ProjectCapability::new("project-1", temp.path()).unwrap();
    let proposal = validated(
        "fs.read",
        json!({"path": "note.txt", "offset_bytes": 0, "max_bytes": 10}),
    );

    let result = ReadToolExecutor
        .execute(&proposal, &capability, &Cancelled)
        .unwrap();

    assert_eq!(result.state(), ToolExecutionState::Cancelled);
    assert!(result.output().is_none());
    assert!(result.error().is_none());
}

#[test]
fn a_capability_for_another_project_never_executes() {
    let temp = tempfile::tempdir().unwrap();
    fs::write(temp.path().join("secret.txt"), "must not read").unwrap();
    let capability = ProjectCapability::new("project-2", temp.path()).unwrap();
    let proposal = validated(
        "fs.read",
        json!({"path": "secret.txt", "offset_bytes": 0, "max_bytes": 20}),
    );

    let result = ReadToolExecutor
        .execute(&proposal, &capability, &NeverCancelled)
        .unwrap();

    assert_eq!(result.state(), ToolExecutionState::Failed);
    assert_eq!(
        result.error().unwrap().code(),
        ToolPublicErrorCode::ProjectMismatch
    );
}

#[test]
fn fs_write_fails_closed_with_approval_required_and_does_not_mutate() {
    let temp = tempfile::tempdir().unwrap();
    let existing = temp.path().join("existing.txt");
    fs::write(&existing, b"original-bytes").unwrap();
    let missing = temp.path().join("missing.txt");
    let capability = ProjectCapability::new("project-1", temp.path()).unwrap();

    let modify = validated(
        "fs.write",
        json!({
            "path": "existing.txt",
            "mode": "modify",
            "content": "mutated-should-not-apply"
        }),
    );
    let create = validated(
        "fs.write",
        json!({
            "path": "missing.txt",
            "mode": "create",
            "content": "created-should-not-apply"
        }),
    );

    let modify_result = ReadToolExecutor
        .execute(&modify, &capability, &NeverCancelled)
        .unwrap();
    let create_result = ReadToolExecutor
        .execute(&create, &capability, &NeverCancelled)
        .unwrap();

    assert_eq!(modify_result.state(), ToolExecutionState::Failed);
    assert_eq!(
        modify_result.error().unwrap().code(),
        ToolPublicErrorCode::ApprovalRequired
    );
    assert!(modify_result.output().is_none());
    assert_eq!(create_result.state(), ToolExecutionState::Failed);
    assert_eq!(
        create_result.error().unwrap().code(),
        ToolPublicErrorCode::ApprovalRequired
    );
    assert!(create_result.output().is_none());
    assert_eq!(fs::read(&existing).unwrap(), b"original-bytes");
    assert!(!missing.exists());
}

#[test]
fn process_run_fails_closed_with_approval_required_without_a_child_process_path() {
    let temp = tempfile::tempdir().unwrap();
    let capability = ProjectCapability::new("project-1", temp.path()).unwrap();
    let proposal = validated(
        "process.run",
        json!({"program": "cargo", "argv": ["--version"]}),
    );

    let result = ReadToolExecutor
        .execute(&proposal, &capability, &NeverCancelled)
        .unwrap();

    assert_eq!(result.state(), ToolExecutionState::Failed);
    assert_eq!(
        result.error().unwrap().code(),
        ToolPublicErrorCode::ApprovalRequired
    );
    assert!(result.output().is_none());
    assert!(temp.path().read_dir().unwrap().next().is_none());
}

#[cfg(unix)]
#[test]
fn symlink_targets_are_rejected_instead_of_followed() {
    use std::os::unix::fs::symlink;

    let project = tempfile::tempdir().unwrap();
    let outside = tempfile::tempdir().unwrap();
    fs::write(outside.path().join("secret.txt"), "outside").unwrap();
    symlink(outside.path(), project.path().join("escape")).unwrap();
    let capability = ProjectCapability::new("project-1", project.path()).unwrap();
    let proposal = validated(
        "fs.read",
        json!({"path": "escape/secret.txt", "offset_bytes": 0, "max_bytes": 20}),
    );

    let result = ReadToolExecutor
        .execute(&proposal, &capability, &NeverCancelled)
        .unwrap();

    assert_eq!(result.state(), ToolExecutionState::Failed);
    assert_eq!(
        result.error().unwrap().code(),
        ToolPublicErrorCode::PathRejected
    );
}

#[cfg(windows)]
#[test]
fn symlink_targets_are_rejected_instead_of_followed() {
    use std::os::windows::fs::symlink_dir;

    let project = tempfile::tempdir().unwrap();
    let outside = tempfile::tempdir().unwrap();
    fs::write(outside.path().join("secret.txt"), "outside").unwrap();
    if symlink_dir(outside.path(), project.path().join("escape")).is_err() {
        return;
    }
    let capability = ProjectCapability::new("project-1", project.path()).unwrap();
    let proposal = validated(
        "fs.read",
        json!({"path": "escape/secret.txt", "offset_bytes": 0, "max_bytes": 20}),
    );

    let result = ReadToolExecutor
        .execute(&proposal, &capability, &NeverCancelled)
        .unwrap();

    assert_eq!(result.state(), ToolExecutionState::Failed);
    assert_eq!(
        result.error().unwrap().code(),
        ToolPublicErrorCode::PathRejected
    );
}
