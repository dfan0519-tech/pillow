use std::fs;
use std::path::PathBuf;

use keeper_agent_protocol::{
    ArgumentsDigest, ToolExecutionState, ToolProposal, ToolProtocolErrorCode, ToolPublicError,
    ToolPublicErrorCode, ToolResult, canonical_arguments_digest, fixed_tool_manifests,
};
use serde_json::{Value, json};

fn fixture_path(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../tests/fixtures/protocol")
        .join(name)
}

fn read_fixture(name: &str) -> String {
    fs::read_to_string(fixture_path(name)).expect("protocol fixture")
}

fn proposal_fixture() -> ToolProposal {
    serde_json::from_str(&read_fixture("tool-proposal-fs-read-v1.json"))
        .expect("strict proposal fixture")
}

#[test]
fn fixed_manifests_match_the_deterministic_v1_fixture() {
    let actual = format!(
        "{}\n",
        serde_json::to_string_pretty(&fixed_tool_manifests()).expect("serialize manifests")
    );
    assert_eq!(actual, read_fixture("tool-manifests-v1.json"));
}

#[test]
fn canonical_digest_is_key_order_independent_and_sha256_tagged() {
    let first = json!({
        "path": "src/lib.rs",
        "offset_bytes": 0,
        "max_bytes": 4096
    });
    let second: Value =
        serde_json::from_str(r#"{"max_bytes":4096,"offset_bytes":0,"path":"src/lib.rs"}"#).unwrap();

    let expected = ArgumentsDigest::parse(
        "sha256:39d1d5468d2801975f58ad7bb568647f4dbf73581bf34a3af040cae092805cdb",
    )
    .unwrap();
    assert_eq!(canonical_arguments_digest(&first).unwrap(), expected);
    assert_eq!(canonical_arguments_digest(&second).unwrap(), expected);
}

#[test]
fn canonical_digest_rejects_non_integer_numbers_fail_closed() {
    let error = canonical_arguments_digest(&json!({"path": "src", "ratio": 0.5})).unwrap_err();
    assert_eq!(error.code, ToolProtocolErrorCode::InvalidArguments);
}

#[test]
fn valid_read_proposal_fixture_normalizes_to_typed_arguments() {
    let validated = proposal_fixture().validate_for_project("VK-013").unwrap();
    assert_eq!(validated.manifest.tool_id.as_str(), "fs.read");
    assert_eq!(validated.manifest.tool_version, 1);
    assert_eq!(
        validated.canonical_arguments,
        json!({
            "path": "src/lib.rs",
            "offset_bytes": 0,
            "max_bytes": 4096
        })
    );
}

#[test]
fn proposal_envelope_denies_unknown_and_authority_fields() {
    for forbidden in [
        "executable",
        "environment",
        "cwd",
        "effect_class",
        "approval_level",
    ] {
        let mut value: Value =
            serde_json::from_str(&read_fixture("tool-proposal-fs-read-v1.json")).unwrap();
        value
            .as_object_mut()
            .unwrap()
            .insert(forbidden.into(), json!("provider-owned"));

        let error = serde_json::from_value::<ToolProposal>(value).unwrap_err();
        assert!(
            error.to_string().contains("unknown field"),
            "{forbidden}: {error}"
        );
    }
}

#[test]
fn proposal_arguments_deny_unknown_fields_even_with_a_matching_digest() {
    let mut proposal = proposal_fixture();
    proposal
        .arguments
        .as_object_mut()
        .unwrap()
        .insert("cwd".into(), json!("C:/escape"));
    proposal.arguments_digest = canonical_arguments_digest(&proposal.arguments).unwrap();

    let error = proposal.validate_for_project("VK-013").unwrap_err();
    assert_eq!(error.code, ToolProtocolErrorCode::InvalidArguments);
}

#[test]
fn proposal_rejects_unknown_tool_and_version() {
    let mut unknown_tool = proposal_fixture();
    unknown_tool.tool_id = "fs.execute".into();
    let error = unknown_tool.validate_for_project("VK-013").unwrap_err();
    assert_eq!(error.code, ToolProtocolErrorCode::UnknownTool);

    let mut unknown_version = proposal_fixture();
    unknown_version.tool_version = 2;
    let error = unknown_version.validate_for_project("VK-013").unwrap_err();
    assert_eq!(error.code, ToolProtocolErrorCode::UnsupportedToolVersion);
}

#[test]
fn proposal_rejects_project_mismatch_and_digest_mismatch() {
    let error = proposal_fixture()
        .validate_for_project("VK-999")
        .unwrap_err();
    assert_eq!(error.code, ToolProtocolErrorCode::ProjectMismatch);

    let mut proposal = proposal_fixture();
    proposal.arguments_digest = ArgumentsDigest::parse(
        "sha256:44136fa355b3678a1146ad16f7e8649e94fb4fc21fe77e8310c060f61caaff8a",
    )
    .unwrap();
    let error = proposal.validate_for_project("VK-013").unwrap_err();
    assert_eq!(error.code, ToolProtocolErrorCode::ArgumentsDigestMismatch);
}

#[test]
fn proposal_rejects_path_escape_and_manifest_limit_overrides() {
    let mut escaped = proposal_fixture();
    escaped.arguments = json!({
        "path": "../outside.txt",
        "offset_bytes": 0,
        "max_bytes": 4096
    });
    escaped.arguments_digest = canonical_arguments_digest(&escaped.arguments).unwrap();
    let error = escaped.validate_for_project("VK-013").unwrap_err();
    assert_eq!(error.code, ToolProtocolErrorCode::InvalidArguments);

    let mut oversized = proposal_fixture();
    oversized.arguments = json!({
        "path": "src/lib.rs",
        "offset_bytes": 0,
        "max_bytes": 1_048_577
    });
    oversized.arguments_digest = canonical_arguments_digest(&oversized.arguments).unwrap();
    let error = oversized.validate_for_project("VK-013").unwrap_err();
    assert_eq!(error.code, ToolProtocolErrorCode::InvalidArguments);
}

#[test]
fn search_rejects_whitespace_only_queries() {
    let mut proposal = proposal_fixture();
    proposal.tool_id = "fs.search".into();
    proposal.arguments = json!({
        "path": ".",
        "query": "   ",
        "case_sensitive": false,
        "max_results": 10
    });
    proposal.arguments_digest = canonical_arguments_digest(&proposal.arguments).unwrap();

    let error = proposal.validate_for_project("VK-013").unwrap_err();
    assert_eq!(error.code, ToolProtocolErrorCode::InvalidArguments);
}

#[test]
fn tool_state_vocabulary_is_fixed_and_ordered() {
    let names: Vec<Value> = ToolExecutionState::all()
        .iter()
        .map(|state| serde_json::to_value(state).unwrap())
        .collect();
    assert_eq!(
        names,
        vec![
            json!("proposed"),
            json!("validated"),
            json!("executing"),
            json!("succeeded"),
            json!("failed"),
            json!("cancelled"),
        ]
    );
}

#[test]
fn successful_result_fixture_is_strict_and_valid() {
    let result: ToolResult =
        serde_json::from_str(&read_fixture("tool-result-fs-read-v1.json")).unwrap();
    let validated = result.validate_for_project("VK-013").unwrap();
    assert_eq!(validated.state, ToolExecutionState::Succeeded);
    assert_eq!(
        validated.output.unwrap()["content_kind"],
        Value::String("utf8".into())
    );
}

#[test]
fn every_fixed_tool_has_a_valid_proposal_and_result_fixture() {
    for tool in ["fs-list", "fs-read", "fs-search"] {
        let proposal: ToolProposal =
            serde_json::from_str(&read_fixture(&format!("tool-proposal-{tool}-v1.json"))).unwrap();
        let result: ToolResult =
            serde_json::from_str(&read_fixture(&format!("tool-result-{tool}-v1.json"))).unwrap();

        let proposal = proposal.validate_for_project("VK-013").unwrap();
        let result = result.validate_for_project("VK-013").unwrap();
        assert_eq!(proposal.proposal.tool_id, result.tool_id);
        assert_eq!(proposal.proposal.call_id, result.call_id);
        assert_eq!(result.state, ToolExecutionState::Succeeded);
    }
}

#[test]
fn result_rejects_unknown_fields_output_mismatch_and_invalid_state_shape() {
    let mut unknown: Value =
        serde_json::from_str(&read_fixture("tool-result-fs-read-v1.json")).unwrap();
    unknown
        .as_object_mut()
        .unwrap()
        .insert("provider_payload".into(), json!({}));
    assert!(serde_json::from_value::<ToolResult>(unknown).is_err());

    let mut wrong_output: ToolResult =
        serde_json::from_str(&read_fixture("tool-result-fs-read-v1.json")).unwrap();
    wrong_output.output = Some(json!({
        "entries": [],
        "truncated": false
    }));
    let error = wrong_output.validate_for_project("VK-013").unwrap_err();
    assert_eq!(error.code, ToolProtocolErrorCode::InvalidResult);

    let mut executing: ToolResult =
        serde_json::from_str(&read_fixture("tool-result-fs-read-v1.json")).unwrap();
    executing.state = ToolExecutionState::Executing;
    let error = executing.validate_for_project("VK-013").unwrap_err();
    assert_eq!(error.code, ToolProtocolErrorCode::InvalidResult);
}

#[test]
fn failed_result_rejects_noncanonical_public_error_text() {
    let mut result: ToolResult =
        serde_json::from_str(&read_fixture("tool-result-fs-read-v1.json")).unwrap();
    result.state = ToolExecutionState::Failed;
    result.output = None;
    result.error = Some(ToolPublicError {
        code: ToolPublicErrorCode::InternalError,
        message: "operation failed with token abc123".into(),
        retryable: true,
    });

    let error = result.validate_for_project("VK-013").unwrap_err();
    assert_eq!(error.code, ToolProtocolErrorCode::InvalidResult);

    result.error = Some(ToolPublicError::canonical(
        ToolPublicErrorCode::InternalError,
    ));
    assert!(result.validate_for_project("VK-013").is_ok());
}

#[test]
fn protocol_fixtures_are_provider_neutral_and_secret_free() {
    for name in [
        "tool-manifests-v1.json",
        "tool-proposal-fs-list-v1.json",
        "tool-proposal-fs-read-v1.json",
        "tool-proposal-fs-search-v1.json",
        "tool-result-fs-list-v1.json",
        "tool-result-fs-read-v1.json",
        "tool-result-fs-search-v1.json",
    ] {
        let lower = read_fixture(name).to_ascii_lowercase();
        for forbidden in [
            "openai",
            "anthropic",
            "grok",
            "api_key",
            "authorization",
            "bearer ",
            "mcp",
        ] {
            assert!(!lower.contains(forbidden), "{name} contains {forbidden}");
        }
    }
}
