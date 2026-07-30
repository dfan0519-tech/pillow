use std::fs;
use std::path::PathBuf;

use keeper_agent_protocol::{
    ArgumentsDigest, RunId, ToolActiveContext, ToolExecutionState, ToolProposal,
    ToolProtocolErrorCode, ToolPublicError, ToolPublicErrorCode, ToolResult,
    canonical_arguments_digest, fixed_tool_manifests,
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

fn proposal_fixture() -> ToolProposal {
    ToolProposal::parse_str(&read_fixture("tool-proposal-fs-read-v1.json"))
        .expect("safe strict proposal fixture")
}

fn active_context() -> ToolActiveContext {
    ToolActiveContext::new("VK-013", "VK-013-C116", RunId(Uuid::nil())).unwrap()
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
    assert_eq!(error.code(), ToolProtocolErrorCode::InvalidArguments);
}

#[test]
fn valid_read_proposal_fixture_normalizes_to_typed_arguments() {
    let proposal = proposal_fixture();
    let raw_digest = canonical_arguments_digest(&proposal.arguments).unwrap();
    let validated = proposal.validate_for_context(&active_context()).unwrap();

    assert_ne!(raw_digest, proposal.arguments_digest);
    assert_eq!(validated.manifest().tool_id.as_str(), "fs.read");
    assert_eq!(validated.manifest().tool_version, 1);
    assert_eq!(
        validated.canonical_arguments(),
        &json!({
            "path": "src/lib.rs",
            "offset_bytes": 0,
            "max_bytes": 65536
        })
    );
    assert_eq!(
        validated.normalized_arguments_digest().as_str(),
        "sha256:54928b277e37c7b2ff55a04677fec352eff5db41a3ed25ab38229783076b04e6"
    );
    assert_eq!(
        validated.fingerprint().as_str(),
        "sha256:1c723ae1033107c5f8431372811e9a203b23f69e4401127cc0db5e8ed9dd5975"
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

        let error = ToolProposal::parse_value(value).unwrap_err();
        assert_eq!(error.code(), ToolProtocolErrorCode::InvalidEnvelope);
        assert!(!error.to_string().contains(forbidden));
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

    let error = proposal
        .validate_for_context(&active_context())
        .unwrap_err();
    assert_eq!(error.code(), ToolProtocolErrorCode::InvalidArguments);
}

#[test]
fn proposal_rejects_unknown_tool_and_version() {
    let mut unknown_tool = proposal_fixture();
    unknown_tool.tool_id = "fs.execute".into();
    let error = unknown_tool
        .validate_for_context(&active_context())
        .unwrap_err();
    assert_eq!(error.code(), ToolProtocolErrorCode::UnknownTool);

    let mut unknown_version = proposal_fixture();
    unknown_version.tool_version = 2;
    let error = unknown_version
        .validate_for_context(&active_context())
        .unwrap_err();
    assert_eq!(error.code(), ToolProtocolErrorCode::UnsupportedToolVersion);
}

#[test]
fn proposal_rejects_active_context_mismatches_and_digest_mismatch() {
    let contexts = [
        ToolActiveContext::new("VK-999", "VK-013-C116", RunId(Uuid::nil())).unwrap(),
        ToolActiveContext::new("VK-013", "VK-013-C999", RunId(Uuid::nil())).unwrap(),
        ToolActiveContext::new("VK-013", "VK-013-C116", RunId(Uuid::from_u128(1))).unwrap(),
    ];
    for context in contexts {
        let error = proposal_fixture()
            .validate_for_context(&context)
            .unwrap_err();
        assert_eq!(error.code(), ToolProtocolErrorCode::ActiveContextMismatch);
    }

    let mut proposal = proposal_fixture();
    proposal.arguments_digest = ArgumentsDigest::parse(
        "sha256:44136fa355b3678a1146ad16f7e8649e94fb4fc21fe77e8310c060f61caaff8a",
    )
    .unwrap();
    let error = proposal
        .validate_for_context(&active_context())
        .unwrap_err();
    assert_eq!(error.code(), ToolProtocolErrorCode::ArgumentsDigestMismatch);
}

#[test]
fn call_id_is_validated_and_part_of_the_stable_fingerprint() {
    let original = proposal_fixture()
        .validate_for_context(&active_context())
        .unwrap();

    let mut changed = proposal_fixture();
    changed.call_id = "call-read-2".into();
    let changed = changed.validate_for_context(&active_context()).unwrap();
    assert_ne!(original.fingerprint(), changed.fingerprint());

    let mut invalid = proposal_fixture();
    invalid.call_id = "call/read/1".into();
    let error = invalid.validate_for_context(&active_context()).unwrap_err();
    assert_eq!(error.code(), ToolProtocolErrorCode::InvalidEnvelope);
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
    let error = escaped.validate_for_context(&active_context()).unwrap_err();
    assert_eq!(error.code(), ToolProtocolErrorCode::InvalidArguments);

    let mut oversized = proposal_fixture();
    oversized.arguments = json!({
        "path": "src/lib.rs",
        "offset_bytes": 0,
        "max_bytes": 1_048_577
    });
    oversized.arguments_digest = canonical_arguments_digest(&oversized.arguments).unwrap();
    let error = oversized
        .validate_for_context(&active_context())
        .unwrap_err();
    assert_eq!(error.code(), ToolProtocolErrorCode::InvalidArguments);
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

    let error = proposal
        .validate_for_context(&active_context())
        .unwrap_err();
    assert_eq!(error.code(), ToolProtocolErrorCode::InvalidArguments);
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
    let context = active_context();
    let proposal = proposal_fixture().validate_for_context(&context).unwrap();
    let result = ToolResult::parse_str(&read_fixture("tool-result-fs-read-v1.json")).unwrap();
    let validated = result.validate_for_proposal(&context, &proposal).unwrap();
    assert_eq!(validated.state(), ToolExecutionState::Succeeded);
    assert_eq!(
        validated.output().unwrap()["content_kind"],
        Value::String("utf8".into())
    );
    assert_eq!(validated.proposal_fingerprint(), proposal.fingerprint());
}

#[test]
fn every_fixed_tool_has_a_valid_proposal_and_result_fixture() {
    let context = active_context();
    for tool in ["fs-list", "fs-read", "fs-search"] {
        let proposal =
            ToolProposal::parse_str(&read_fixture(&format!("tool-proposal-{tool}-v1.json")))
                .unwrap();
        let result =
            ToolResult::parse_str(&read_fixture(&format!("tool-result-{tool}-v1.json"))).unwrap();

        let proposal = proposal.validate_for_context(&context).unwrap();
        let result = result.validate_for_proposal(&context, &proposal).unwrap();
        assert_eq!(proposal.tool_id(), result.tool_id());
        assert_eq!(proposal.call_id(), result.call_id());
        assert_eq!(result.state(), ToolExecutionState::Succeeded);
    }
}

#[test]
fn result_rejects_unknown_fields_output_mismatch_and_invalid_state_shape() {
    let context = active_context();
    let proposal = proposal_fixture().validate_for_context(&context).unwrap();
    let mut unknown: Value =
        serde_json::from_str(&read_fixture("tool-result-fs-read-v1.json")).unwrap();
    unknown
        .as_object_mut()
        .unwrap()
        .insert("provider_payload".into(), json!({}));
    assert!(ToolResult::parse_value(unknown).is_err());

    let mut wrong_output =
        ToolResult::parse_str(&read_fixture("tool-result-fs-read-v1.json")).unwrap();
    wrong_output.output = Some(json!({
        "entries": [],
        "truncated": false
    }));
    let error = wrong_output
        .validate_for_proposal(&context, &proposal)
        .unwrap_err();
    assert_eq!(error.code(), ToolProtocolErrorCode::InvalidResult);

    let mut executing =
        ToolResult::parse_str(&read_fixture("tool-result-fs-read-v1.json")).unwrap();
    executing.state = ToolExecutionState::Executing;
    let error = executing
        .validate_for_proposal(&context, &proposal)
        .unwrap_err();
    assert_eq!(error.code(), ToolProtocolErrorCode::InvalidResult);
}

#[test]
fn result_rejects_every_proposal_and_context_identity_mismatch() {
    let context = active_context();
    let proposal = proposal_fixture().validate_for_context(&context).unwrap();
    let base = ToolResult::parse_str(&read_fixture("tool-result-fs-read-v1.json")).unwrap();

    let mut mismatches = Vec::new();
    let mut value = base.clone();
    value.project_id = "VK-999".into();
    mismatches.push(value);
    let mut value = base.clone();
    value.conversation_id = "VK-013-C999".into();
    mismatches.push(value);
    let mut value = base.clone();
    value.run_id = RunId(Uuid::from_u128(1));
    mismatches.push(value);
    let mut value = base.clone();
    value.call_id = "call-read-2".into();
    mismatches.push(value);
    let mut value = base.clone();
    value.tool_id = "fs.list".into();
    mismatches.push(value);
    let mut value = base.clone();
    value.tool_version = 2;
    mismatches.push(value);

    for mismatch in mismatches {
        let error = mismatch
            .validate_for_proposal(&context, &proposal)
            .unwrap_err();
        assert_eq!(error.code(), ToolProtocolErrorCode::InvalidResult);
    }

    let other_context =
        ToolActiveContext::new("VK-013", "VK-013-C999", RunId(Uuid::nil())).unwrap();
    let error = base
        .validate_for_proposal(&other_context, &proposal)
        .unwrap_err();
    assert_eq!(error.code(), ToolProtocolErrorCode::ActiveContextMismatch);
}

#[test]
fn public_errors_are_secret_safe_by_construction_and_serialization() {
    let unsafe_error = json!({
        "code": "internal_error",
        "message": "operation failed with token abc123",
        "retryable": true
    });
    assert!(serde_json::from_value::<ToolPublicError>(unsafe_error).is_err());

    let safe_error = ToolPublicError::canonical(ToolPublicErrorCode::InternalError);
    assert_eq!(
        serde_json::to_value(safe_error).unwrap(),
        json!({
            "code": "internal_error",
            "message": "The tool call failed inside Keeper.",
            "retryable": true
        })
    );

    let context = active_context();
    let proposal = proposal_fixture().validate_for_context(&context).unwrap();
    let mut result = ToolResult::parse_str(&read_fixture("tool-result-fs-read-v1.json")).unwrap();
    result.state = ToolExecutionState::Failed;
    result.output = None;
    result.error = Some(safe_error);
    assert!(result.validate_for_proposal(&context, &proposal).is_ok());
}

#[test]
fn protocol_paths_reject_windows_devices_and_non_portable_aliases() {
    let context = active_context();
    let valid = proposal_fixture().validate_for_context(&context).unwrap();
    let base_result = ToolResult::parse_str(&read_fixture("tool-result-fs-read-v1.json")).unwrap();
    let mut paths = vec![
        "CON".to_string(),
        "con.txt".to_string(),
        "CON .txt".to_string(),
        "src/AUX.rs".to_string(),
        "NUL".to_string(),
        "COM1".to_string(),
        "com9.log".to_string(),
        "COM1 .log".to_string(),
        "LPT1".to_string(),
        "src/lpt9.txt".to_string(),
        "CLOCK$".to_string(),
        "CONIN$".to_string(),
        "CONOUT$".to_string(),
        "src/name.".to_string(),
        "src/name ".to_string(),
        "src/bad<name".to_string(),
        "src/bad>name".to_string(),
        "src/bad\"name".to_string(),
        "src/bad|name".to_string(),
        "src/bad?name".to_string(),
        "src/bad*name".to_string(),
    ];
    paths.push(format!("src/{}", "a".repeat(256)));

    for path in paths {
        let mut proposal = proposal_fixture();
        proposal.arguments = json!({"path": path});
        let error = proposal.validate_for_context(&context).unwrap_err();
        assert_eq!(
            error.code(),
            ToolProtocolErrorCode::InvalidArguments,
            "proposal path should be rejected: {path}"
        );

        let mut result = base_result.clone();
        result.output.as_mut().unwrap()["path"] = json!(path);
        let error = result.validate_for_proposal(&context, &valid).unwrap_err();
        assert_eq!(
            error.code(),
            ToolProtocolErrorCode::InvalidResult,
            "result path should be rejected: {path}"
        );
    }
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
