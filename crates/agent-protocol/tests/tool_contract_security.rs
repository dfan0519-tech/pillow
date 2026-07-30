use std::fs;
use std::path::PathBuf;

use keeper_agent_protocol::{
    RunId, ToolActiveContext, ToolCallReservationDecision, ToolExecutionState, ToolProposal,
    ToolProtocolError, ToolProtocolErrorCode, ToolResult, ValidatedToolProposal,
    canonical_arguments_digest,
};
use serde_json::{Value, json};
use uuid::Uuid;

fn fixture_path(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../tests/fixtures/protocol")
        .join(name)
}

fn read_fixture(name: &str) -> String {
    fs::read_to_string(fixture_path(name)).expect("protocol fixture")
}

fn proposal_fixture(tool: &str) -> ToolProposal {
    ToolProposal::parse_str(&read_fixture(&format!("tool-proposal-{tool}-v1.json")))
        .expect("safe strict proposal fixture")
}

fn result_fixture(tool: &str) -> ToolResult {
    ToolResult::parse_str(&read_fixture(&format!("tool-result-{tool}-v1.json")))
        .expect("safe strict result fixture")
}

fn active_context() -> ToolActiveContext {
    ToolActiveContext::new("VK-013", "VK-013-C116", RunId(Uuid::nil())).unwrap()
}

fn bind_result(mut result: ToolResult, proposal: &ValidatedToolProposal) -> ToolResult {
    result.project_id = proposal.project_id().into();
    result.conversation_id = proposal.conversation_id().into();
    result.run_id = proposal.run_id();
    result.call_id = proposal.call_id().into();
    result.tool_id = proposal.tool_id().into();
    result.tool_version = proposal.tool_version();
    result.proposal_fingerprint = proposal.fingerprint().clone();
    result.arguments_digest = proposal.normalized_arguments_digest().clone();
    result
}

fn assert_secret_safe(error: ToolProtocolError, code: ToolProtocolErrorCode) {
    assert_eq!(error.code(), code);
    for diagnostic in [error.to_string(), format!("{error:?}")] {
        assert!(!diagnostic.contains("TOP_SECRET"), "{diagnostic}");
    }
}

#[test]
fn operational_wire_parsers_hide_all_proposal_and_result_serde_failures() {
    let mut proposal: Value =
        serde_json::from_str(&read_fixture("tool-proposal-fs-read-v1.json")).unwrap();
    proposal
        .as_object_mut()
        .unwrap()
        .insert("TOP_SECRET_FIELD".into(), json!("TOP_SECRET_VALUE"));
    let proposal_text = serde_json::to_string(&proposal).unwrap();
    assert_secret_safe(
        ToolProposal::parse_str(&proposal_text).unwrap_err(),
        ToolProtocolErrorCode::InvalidEnvelope,
    );
    assert_secret_safe(
        ToolProposal::parse_bytes(proposal_text.as_bytes()).unwrap_err(),
        ToolProtocolErrorCode::InvalidEnvelope,
    );
    assert_secret_safe(
        ToolProposal::parse_value(proposal).unwrap_err(),
        ToolProtocolErrorCode::InvalidEnvelope,
    );

    let mut result: Value =
        serde_json::from_str(&read_fixture("tool-result-fs-read-v1.json")).unwrap();
    result
        .as_object_mut()
        .unwrap()
        .insert("TOP_SECRET_RESULT".into(), json!("TOP_SECRET_VALUE"));
    let result_text = serde_json::to_string(&result).unwrap();
    assert_secret_safe(
        ToolResult::parse_str(&result_text).unwrap_err(),
        ToolProtocolErrorCode::InvalidResult,
    );
    assert_secret_safe(
        ToolResult::parse_bytes(result_text.as_bytes()).unwrap_err(),
        ToolProtocolErrorCode::InvalidResult,
    );
    assert_secret_safe(
        ToolResult::parse_value(result).unwrap_err(),
        ToolProtocolErrorCode::InvalidResult,
    );
}

#[test]
fn reservation_claim_distinguishes_replay_from_changed_arguments_conflict() {
    let context = active_context();
    let original = proposal_fixture("fs-read")
        .validate_for_context(&context)
        .unwrap();
    let replay = proposal_fixture("fs-read")
        .validate_for_context(&context)
        .unwrap();

    let mut changed = proposal_fixture("fs-read");
    changed.arguments = json!({
        "path": "src/types.rs",
        "offset_bytes": 0,
        "max_bytes": 65536
    });
    changed.arguments_digest = canonical_arguments_digest(&changed.arguments).unwrap();
    let changed = changed.validate_for_context(&context).unwrap();

    assert_eq!(original.reservation().key(), changed.reservation().key());
    assert_eq!(original.reservation().key(), replay.reservation().key());
    assert_ne!(original.fingerprint(), changed.fingerprint());
    assert_eq!(original.fingerprint(), replay.fingerprint());
    assert_eq!(
        original.reservation().classify(None),
        ToolCallReservationDecision::Unreserved
    );
    assert_eq!(
        original.reservation().classify(Some(replay.fingerprint())),
        ToolCallReservationDecision::Replay
    );
    assert_eq!(
        original.reservation().classify(Some(changed.fingerprint())),
        ToolCallReservationDecision::Conflict
    );
}

#[test]
fn reservation_key_uses_only_active_call_identity() {
    let context = active_context();
    let original = proposal_fixture("fs-read")
        .validate_for_context(&context)
        .unwrap();
    assert_eq!(
        original.reservation().key().as_str(),
        "sha256:45d45dadb5bb9bf2fe6ee2ae307a96b2d668a43089226d0822670343a84d2878"
    );

    let mut other_call = proposal_fixture("fs-read");
    other_call.call_id = "call-read-2".into();
    let other_call = other_call.validate_for_context(&context).unwrap();
    assert_ne!(original.reservation().key(), other_call.reservation().key());
}

#[test]
fn result_receipt_from_another_proposal_is_rejected() {
    let context = active_context();
    let original = proposal_fixture("fs-read")
        .validate_for_context(&context)
        .unwrap();

    let mut changed = proposal_fixture("fs-read");
    changed.arguments = json!({
        "path": "src/types.rs",
        "offset_bytes": 0,
        "max_bytes": 65536
    });
    changed.arguments_digest = canonical_arguments_digest(&changed.arguments).unwrap();
    let changed = changed.validate_for_context(&context).unwrap();

    let result = bind_result(result_fixture("fs-read"), &original);
    let error = result
        .validate_for_proposal(&context, &changed)
        .unwrap_err();
    assert_eq!(error.code(), ToolProtocolErrorCode::InvalidResult);

    let mut wrong_fingerprint = bind_result(result_fixture("fs-read"), &original);
    wrong_fingerprint.proposal_fingerprint = changed.fingerprint().clone();
    assert_eq!(
        wrong_fingerprint
            .validate_for_proposal(&context, &original)
            .unwrap_err()
            .code(),
        ToolProtocolErrorCode::InvalidResult
    );

    let mut wrong_digest = bind_result(result_fixture("fs-read"), &original);
    wrong_digest.arguments_digest = changed.normalized_arguments_digest().clone();
    assert_eq!(
        wrong_digest
            .validate_for_proposal(&context, &original)
            .unwrap_err()
            .code(),
        ToolProtocolErrorCode::InvalidResult
    );
}

#[test]
fn validated_result_is_an_opaque_proposal_bound_receipt() {
    let context = active_context();
    let proposal = proposal_fixture("fs-read")
        .validate_for_context(&context)
        .unwrap();
    let result = bind_result(result_fixture("fs-read"), &proposal)
        .validate_for_proposal(&context, &proposal)
        .unwrap();

    assert_eq!(result.state(), ToolExecutionState::Succeeded);
    assert_eq!(result.project_id(), proposal.project_id());
    assert_eq!(result.conversation_id(), proposal.conversation_id());
    assert_eq!(result.run_id(), proposal.run_id());
    assert_eq!(result.call_id(), proposal.call_id());
    assert_eq!(result.tool_id(), proposal.tool_id());
    assert_eq!(result.tool_version(), proposal.tool_version());
    assert_eq!(result.proposal_fingerprint(), proposal.fingerprint());
    assert_eq!(
        result.normalized_arguments_digest(),
        proposal.normalized_arguments_digest()
    );
    assert!(result.output().is_some());
    assert!(result.error().is_none());
}

#[test]
fn result_enforces_each_proposals_requested_count_and_byte_limits() {
    let context = active_context();

    let mut list = proposal_fixture("fs-list");
    list.arguments = json!({"path": "src", "max_results": 1});
    list.arguments_digest = canonical_arguments_digest(&list.arguments).unwrap();
    let list = list.validate_for_context(&context).unwrap();
    let list_result = bind_result(result_fixture("fs-list"), &list);
    assert_eq!(
        list_result
            .validate_for_proposal(&context, &list)
            .unwrap_err()
            .code(),
        ToolProtocolErrorCode::InvalidResult
    );

    let mut read = proposal_fixture("fs-read");
    read.arguments = json!({
        "path": "src/lib.rs",
        "offset_bytes": 0,
        "max_bytes": 8
    });
    read.arguments_digest = canonical_arguments_digest(&read.arguments).unwrap();
    let read = read.validate_for_context(&context).unwrap();
    let read_result = bind_result(result_fixture("fs-read"), &read);
    assert_eq!(
        read_result
            .validate_for_proposal(&context, &read)
            .unwrap_err()
            .code(),
        ToolProtocolErrorCode::InvalidResult
    );

    let mut search = proposal_fixture("fs-search");
    search.arguments = json!({
        "path": ".",
        "query": "ToolProposal",
        "case_sensitive": false,
        "max_results": 1
    });
    search.arguments_digest = canonical_arguments_digest(&search.arguments).unwrap();
    let search = search.validate_for_context(&context).unwrap();
    let mut search_result = bind_result(result_fixture("fs-search"), &search);
    let matches = search_result.output.as_mut().unwrap()["matches"]
        .as_array_mut()
        .unwrap();
    matches.push(json!({
        "path": "src/tool/types.rs",
        "line": 2,
        "column": 1,
        "preview": "pub struct ToolProposal"
    }));
    assert_eq!(
        search_result
            .validate_for_proposal(&context, &search)
            .unwrap_err()
            .code(),
        ToolProtocolErrorCode::InvalidResult
    );
}

#[test]
fn read_result_path_must_equal_the_requested_path() {
    let context = active_context();
    let proposal = proposal_fixture("fs-read")
        .validate_for_context(&context)
        .unwrap();
    let mut result = bind_result(result_fixture("fs-read"), &proposal);
    result.output.as_mut().unwrap()["path"] = json!("src/other.rs");

    let error = result
        .validate_for_proposal(&context, &proposal)
        .unwrap_err();
    assert_eq!(error.code(), ToolProtocolErrorCode::InvalidResult);
}

#[test]
fn list_and_search_results_use_component_aware_proposal_scope() {
    let context = active_context();

    let list = proposal_fixture("fs-list")
        .validate_for_context(&context)
        .unwrap();
    assert!(
        bind_result(result_fixture("fs-list"), &list)
            .validate_for_proposal(&context, &list)
            .is_ok()
    );
    for path in ["src2/file.rs", "Cargo.toml", "src/./file.rs"] {
        let mut result = bind_result(result_fixture("fs-list"), &list);
        result.output.as_mut().unwrap()["entries"][1]["path"] = json!(path);
        assert_eq!(
            result
                .validate_for_proposal(&context, &list)
                .unwrap_err()
                .code(),
            ToolProtocolErrorCode::InvalidResult,
            "list path should be rejected: {path}"
        );
    }

    let mut search = proposal_fixture("fs-search");
    search.arguments = json!({
        "path": "src",
        "query": "ToolProposal",
        "case_sensitive": false,
        "max_results": 100
    });
    search.arguments_digest = canonical_arguments_digest(&search.arguments).unwrap();
    let search = search.validate_for_context(&context).unwrap();
    let search_result = bind_result(result_fixture("fs-search"), &search);
    assert!(
        search_result
            .validate_for_proposal(&context, &search)
            .is_ok()
    );
    for path in ["src2/lib.rs", "tests/tool.rs", "src/../tests/tool.rs"] {
        let mut result = bind_result(result_fixture("fs-search"), &search);
        result.output.as_mut().unwrap()["matches"][0]["path"] = json!(path);
        assert_eq!(
            result
                .validate_for_proposal(&context, &search)
                .unwrap_err()
                .code(),
            ToolProtocolErrorCode::InvalidResult,
            "search path should be rejected: {path}"
        );
    }

    let root_search = proposal_fixture("fs-search")
        .validate_for_context(&context)
        .unwrap();
    let mut root_result = bind_result(result_fixture("fs-search"), &root_search);
    root_result.output.as_mut().unwrap()["matches"][0]["path"] = json!("tests/tool.rs");
    assert!(
        root_result
            .validate_for_proposal(&context, &root_search)
            .is_ok()
    );
}

#[test]
fn protocol_errors_never_reflect_provider_controlled_serde_text() {
    let context = active_context();
    let mut proposal = proposal_fixture("fs-read");
    proposal
        .arguments
        .as_object_mut()
        .unwrap()
        .insert("TOP_SECRET_FIELD".into(), json!("TOP_SECRET_VALUE"));
    let error = proposal.validate_for_context(&context).unwrap_err();
    let diagnostic = error.to_string();
    assert_eq!(error.code(), ToolProtocolErrorCode::InvalidArguments);
    assert_eq!(
        error.message(),
        "Tool arguments do not match the fixed protocol contract."
    );
    assert!(!diagnostic.contains("TOP_SECRET"));

    let validated = proposal_fixture("fs-read")
        .validate_for_context(&context)
        .unwrap();
    let mut result = bind_result(result_fixture("fs-read"), &validated);
    result
        .output
        .as_mut()
        .unwrap()
        .as_object_mut()
        .unwrap()
        .insert("TOP_SECRET_RESULT".into(), json!("TOP_SECRET_VALUE"));
    let error = result
        .validate_for_proposal(&context, &validated)
        .unwrap_err();
    assert_eq!(error.code(), ToolProtocolErrorCode::InvalidResult);
    assert!(!error.to_string().contains("TOP_SECRET"));
}

#[test]
fn protocol_paths_reject_windows_and_portability_adversarial_forms() {
    let context = active_context();
    let validated = proposal_fixture("fs-read")
        .validate_for_context(&context)
        .unwrap();
    let base_result = bind_result(result_fixture("fs-read"), &validated);
    let paths = [
        r"\\server\share\file.txt",
        "//server/share/file.txt",
        r"src\file.txt",
        "C:relative.txt",
        "src/file.txt:stream",
        "src/\u{001f}control.txt",
        "COM\u{00b9}",
        "com\u{00b2}.txt",
        "COM\u{00b3}.log",
        "LPT\u{00b9}",
        "lpt\u{00b2}.txt",
        "LPT\u{00b3}.log",
    ];

    for path in paths {
        let mut proposal = proposal_fixture("fs-read");
        proposal.arguments = json!({"path": path});
        let error = proposal.validate_for_context(&context).unwrap_err();
        assert_eq!(
            error.code(),
            ToolProtocolErrorCode::InvalidArguments,
            "proposal path should be rejected: {path:?}"
        );

        let mut result = base_result.clone();
        result.output.as_mut().unwrap()["path"] = json!(path);
        let error = result
            .validate_for_proposal(&context, &validated)
            .unwrap_err();
        assert_eq!(
            error.code(),
            ToolProtocolErrorCode::InvalidResult,
            "result path should be rejected: {path:?}"
        );
    }
}

#[test]
fn result_receipt_fields_are_required_and_strict() {
    let mut value: Value =
        serde_json::from_str(&read_fixture("tool-result-fs-read-v1.json")).unwrap();
    value
        .as_object_mut()
        .unwrap()
        .remove("proposal_fingerprint");
    assert!(ToolResult::parse_value(value).is_err());

    let mut value: Value =
        serde_json::from_str(&read_fixture("tool-result-fs-read-v1.json")).unwrap();
    value.as_object_mut().unwrap().remove("arguments_digest");
    assert!(ToolResult::parse_value(value).is_err());
}
