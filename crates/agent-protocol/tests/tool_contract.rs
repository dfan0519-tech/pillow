use std::fs;
use std::path::PathBuf;

use keeper_agent_protocol::{
    ArgumentsDigest, FsWriteArguments, PROCESS_ARGV_BYTE_CAP, PROCESS_ARGV_COUNT_CAP,
    ProcessRunArguments, ProcessRunOutput, RunId, ToolActiveContext, ToolArguments,
    ToolExecutionState, ToolId, ToolProposal, ToolProtocolErrorCode, ToolPublicError,
    ToolPublicErrorCode, ToolResult, WRITE_CONTENT_BYTE_CAP, WriteMode, canonical_arguments_digest,
    fixed_tool_manifests,
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
    let actual = serde_json::to_value(fixed_tool_manifests()).expect("serialize manifests");
    let expected: Value =
        serde_json::from_str(&read_fixture("tool-manifests-v1.json")).expect("manifest fixture");
    // Historical read-tool fixtures remain authoritative; fs.write@1 and
    // process.run@1 are additive.
    assert_eq!(
        &actual.as_array().unwrap()[..3],
        expected.as_array().unwrap()
    );
    assert_eq!(actual.as_array().unwrap().len(), 5);
    assert_eq!(actual[3]["tool_id"], json!("fs.write"));
    assert_eq!(actual[3]["tool_version"], json!(1));
    assert_eq!(
        actual[3]["arguments"],
        json!([
            {
                "name": "path",
                "value_type": "project_relative_path",
                "required": true,
                "default": null
            },
            {
                "name": "mode",
                "value_type": "string",
                "required": true,
                "default": null
            },
            {
                "name": "content",
                "value_type": "string",
                "required": true,
                "default": null
            }
        ])
    );
    assert_eq!(actual[3]["limits"]["output_byte_cap"], json!(262_144));
    assert_eq!(actual[4]["tool_id"], json!("process.run"));
    assert_eq!(actual[4]["tool_version"], json!(1));
    assert_eq!(actual[4]["limits"]["timeout_ms"], json!(60_000));
    assert_eq!(actual[4]["limits"]["output_byte_cap"], json!(262_144));
    assert_eq!(
        actual[4]["arguments"],
        json!([
            {
                "name": "program",
                "value_type": "string",
                "required": true,
                "default": null
            },
            {
                "name": "argv",
                "value_type": "string_array",
                "required": true,
                "default": null
            },
            {
                "name": "cwd",
                "value_type": "project_relative_path",
                "required": false,
                "default": null
            }
        ])
    );
}

#[test]
fn process_run_contract_types_and_caps_are_public_at_the_crate_root() {
    let arguments = ProcessRunArguments {
        program: "cargo".into(),
        argv: vec!["test".into()],
        cwd: Some("crates/agent-protocol".into()),
    };
    let output = ProcessRunOutput {
        exit_code: Some(0),
        duration_ms: 1,
        stdout_sha256: "sha256:e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
            .into(),
        stderr_sha256: "sha256:e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
            .into(),
        captured_bytes: 0,
        truncated: false,
        encoding_lossy: false,
        cancelled: false,
        timed_out: false,
    };

    assert_eq!(arguments.argv, ["test"]);
    assert_eq!(output.exit_code, Some(0));
    assert_eq!(PROCESS_ARGV_COUNT_CAP, 64);
    assert_eq!(PROCESS_ARGV_BYTE_CAP, 32_768);
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

fn write_proposal_value(path: &str, mode: &str, content: &str) -> ToolProposal {
    let arguments = json!({
        "path": path,
        "mode": mode,
        "content": content,
    });
    // Prefer a real digest when the payload is within the digest budget; fall
    // back to a well-formed placeholder so oversize argument cases can still
    // exercise schema validation without panicking in the fixture helper.
    let arguments_digest = canonical_arguments_digest(&arguments).unwrap_or_else(|_| {
        ArgumentsDigest::parse(
            "sha256:44136fa355b3678a1146ad16f7e8649e94fb4fc21fe77e8310c060f61caaff8a",
        )
        .unwrap()
    });
    ToolProposal {
        project_id: "VK-013".into(),
        conversation_id: "VK-013-C116".into(),
        run_id: RunId(Uuid::nil()),
        call_id: "call-write-1".into(),
        tool_id: "fs.write".into(),
        tool_version: 1,
        arguments,
        arguments_digest,
    }
}

fn known_write_content_digest() -> &'static str {
    // sha256 of UTF-8 bytes for "export const answer = 42;\n"
    "sha256:a2098bd92b10bf8b816d24b7556b1ce8c49a879d130489065ef1051c17e042f6"
}

#[test]
fn write_tool_is_in_the_fixed_catalog_as_version_one() {
    assert!(ToolId::all().contains(&ToolId::FsWrite));
    assert_eq!(ToolId::FsWrite.as_str(), "fs.write");
    let manifests = fixed_tool_manifests();
    let write = manifests
        .iter()
        .find(|manifest| manifest.tool_id == ToolId::FsWrite)
        .expect("fs.write manifest");
    assert_eq!(write.tool_version, 1);
    assert_eq!(WRITE_CONTENT_BYTE_CAP, 262_144);
}

#[test]
fn valid_write_create_proposal_normalizes_to_typed_arguments() {
    let proposal = write_proposal_value("src/example.ts", "create", "export const n = 1;\n");
    let validated = proposal.validate_for_context(&active_context()).unwrap();
    assert_eq!(validated.tool_id(), "fs.write");
    assert_eq!(validated.tool_version(), 1);
    assert_eq!(
        validated.arguments(),
        &ToolArguments::FsWrite(FsWriteArguments {
            path: "src/example.ts".into(),
            mode: WriteMode::Create,
            content: "export const n = 1;\n".into(),
        })
    );
    assert_eq!(
        validated.canonical_arguments(),
        &json!({
            "path": "src/example.ts",
            "mode": "create",
            "content": "export const n = 1;\n",
        })
    );
}

#[test]
fn valid_write_modify_proposal_accepts_exact_schema() {
    let proposal = write_proposal_value("src/lib.rs", "modify", "fn main() {}\n");
    let validated = proposal.validate_for_context(&active_context()).unwrap();
    match validated.arguments() {
        ToolArguments::FsWrite(arguments) => {
            assert_eq!(arguments.mode, WriteMode::Modify);
            assert_eq!(arguments.path, "src/lib.rs");
            assert_eq!(arguments.content, "fn main() {}\n");
        }
        other => panic!("expected FsWrite, got {other:?}"),
    }
}

#[test]
fn write_arguments_deny_unknown_fields_and_wrong_mode() {
    let mut unknown = write_proposal_value("src/a.ts", "create", "ok");
    unknown
        .arguments
        .as_object_mut()
        .unwrap()
        .insert("base_digest".into(), json!("sha256:deadbeef"));
    unknown.arguments_digest = canonical_arguments_digest(&unknown.arguments).unwrap();
    assert_eq!(
        unknown
            .validate_for_context(&active_context())
            .unwrap_err()
            .code(),
        ToolProtocolErrorCode::InvalidArguments
    );

    let mut bad_mode = write_proposal_value("src/a.ts", "append", "ok");
    // Force digest of the bad payload so rejection is schema-level.
    bad_mode.arguments_digest = canonical_arguments_digest(&bad_mode.arguments).unwrap();
    assert_eq!(
        bad_mode
            .validate_for_context(&active_context())
            .unwrap_err()
            .code(),
        ToolProtocolErrorCode::InvalidArguments
    );
}

#[test]
fn write_content_rejects_empty_nul_and_oversize() {
    let empty = write_proposal_value("src/a.ts", "create", "");
    assert_eq!(
        empty
            .validate_for_context(&active_context())
            .unwrap_err()
            .code(),
        ToolProtocolErrorCode::InvalidArguments
    );

    let with_nul = write_proposal_value("src/a.ts", "create", "a\0b");
    assert_eq!(
        with_nul
            .validate_for_context(&active_context())
            .unwrap_err()
            .code(),
        ToolProtocolErrorCode::InvalidArguments
    );

    let oversize = write_proposal_value(
        "src/a.ts",
        "create",
        &"x".repeat((WRITE_CONTENT_BYTE_CAP as usize) + 1),
    );
    assert_eq!(
        oversize
            .validate_for_context(&active_context())
            .unwrap_err()
            .code(),
        ToolProtocolErrorCode::InvalidArguments
    );

    let at_cap = write_proposal_value(
        "src/a.ts",
        "create",
        &"y".repeat(WRITE_CONTENT_BYTE_CAP as usize),
    );
    assert!(at_cap.validate_for_context(&active_context()).is_ok());

    // Worst-case JSON escaping: each non-NUL control byte becomes `\u00XX`.
    let control_at_cap = write_proposal_value(
        "src/a.ts",
        "create",
        &"\u{1}".repeat(WRITE_CONTENT_BYTE_CAP as usize),
    );
    assert!(
        control_at_cap
            .validate_for_context(&active_context())
            .is_ok()
    );
}

#[test]
fn write_reuses_portable_path_grammar() {
    for path in [
        "../escape.ts",
        "/abs.ts",
        r"src\win.ts",
        "C:drive.ts",
        "src/../lib.ts",
        "CON",
        "src/name.",
        ".",
    ] {
        let proposal = write_proposal_value(path, "create", "body");
        assert_eq!(
            proposal
                .validate_for_context(&active_context())
                .unwrap_err()
                .code(),
            ToolProtocolErrorCode::InvalidArguments,
            "path should be rejected: {path}"
        );
    }
}

#[test]
fn write_version_and_digest_mismatch_are_rejected() {
    let mut version = write_proposal_value("src/a.ts", "create", "body");
    version.tool_version = 2;
    assert_eq!(
        version
            .validate_for_context(&active_context())
            .unwrap_err()
            .code(),
        ToolProtocolErrorCode::UnsupportedToolVersion
    );

    let mut digest = write_proposal_value("src/a.ts", "create", "body");
    digest.arguments_digest = ArgumentsDigest::parse(
        "sha256:44136fa355b3678a1146ad16f7e8649e94fb4fc21fe77e8310c060f61caaff8a",
    )
    .unwrap();
    assert_eq!(
        digest
            .validate_for_context(&active_context())
            .unwrap_err()
            .code(),
        ToolProtocolErrorCode::ArgumentsDigestMismatch
    );
}

#[test]
fn successful_write_result_binds_path_mode_bytes_and_content_digest() {
    let context = active_context();
    let content = "export const answer = 42;\n";
    let proposal = write_proposal_value("src/answer.ts", "create", content)
        .validate_for_context(&context)
        .unwrap();
    let digest = known_write_content_digest();
    let mut result = ToolResult {
        project_id: proposal.project_id().into(),
        conversation_id: proposal.conversation_id().into(),
        run_id: proposal.run_id(),
        call_id: proposal.call_id().into(),
        tool_id: proposal.tool_id().into(),
        tool_version: proposal.tool_version(),
        proposal_fingerprint: proposal.fingerprint().clone(),
        arguments_digest: proposal.normalized_arguments_digest().clone(),
        state: ToolExecutionState::Succeeded,
        output: Some(json!({
            "path": "src/answer.ts",
            "mode": "create",
            "bytes_written": content.len() as u64,
            "content_digest": digest,
        })),
        error: None,
    };
    let validated = result.validate_for_proposal(&context, &proposal).unwrap();
    assert_eq!(validated.state(), ToolExecutionState::Succeeded);
    assert_eq!(validated.output().unwrap()["content_digest"], json!(digest));

    result.output.as_mut().unwrap()["bytes_written"] = json!(1);
    assert_eq!(
        result
            .validate_for_proposal(&context, &proposal)
            .unwrap_err()
            .code(),
        ToolProtocolErrorCode::InvalidResult
    );

    result.output.as_mut().unwrap()["bytes_written"] = json!(content.len() as u64);
    result.output.as_mut().unwrap()["mode"] = json!("modify");
    assert_eq!(
        result
            .validate_for_proposal(&context, &proposal)
            .unwrap_err()
            .code(),
        ToolProtocolErrorCode::InvalidResult
    );

    result.output.as_mut().unwrap()["mode"] = json!("create");
    result.output.as_mut().unwrap()["content_digest"] =
        json!("sha256:0000000000000000000000000000000000000000000000000000000000000000");
    assert_eq!(
        result
            .validate_for_proposal(&context, &proposal)
            .unwrap_err()
            .code(),
        ToolProtocolErrorCode::InvalidResult
    );
}

#[test]
fn write_public_errors_are_canonical_and_reject_arbitrary_messages() {
    for code in [
        ToolPublicErrorCode::ApprovalRequired,
        ToolPublicErrorCode::ApprovalRejected,
        ToolPublicErrorCode::ApprovalExpired,
        ToolPublicErrorCode::BaseChanged,
        ToolPublicErrorCode::WriteConflict,
        ToolPublicErrorCode::PathRejected,
        ToolPublicErrorCode::WriteFailed,
    ] {
        let error = ToolPublicError::canonical(code);
        let wire = serde_json::to_value(error).unwrap();
        let parsed: ToolPublicError = serde_json::from_value(wire.clone()).unwrap();
        assert_eq!(parsed.code(), code);
        assert!(!error.message().is_empty());
        assert!(!error.message().contains('\\'));
        assert!(!error.message().contains('/'));

        let mut custom = wire.clone();
        custom["message"] = json!("leaked path C:\\secret\\file.env");
        assert!(
            serde_json::from_value::<ToolPublicError>(custom).is_err(),
            "custom message must be rejected for {code:?}"
        );
    }
}

// --- process.run@1 -----------------------------------------------------------

fn process_run_proposal(program: &str, argv: &[&str], cwd: Option<&str>) -> ToolProposal {
    let mut arguments = json!({
        "program": program,
        "argv": argv,
    });
    if let Some(cwd) = cwd {
        arguments
            .as_object_mut()
            .unwrap()
            .insert("cwd".into(), json!(cwd));
    }
    let arguments_digest = canonical_arguments_digest(&arguments).unwrap_or_else(|_| {
        ArgumentsDigest::parse(
            "sha256:44136fa355b3678a1146ad16f7e8649e94fb4fc21fe77e8310c060f61caaff8a",
        )
        .unwrap()
    });
    ToolProposal {
        project_id: "VK-013".into(),
        conversation_id: "VK-013-C116".into(),
        run_id: RunId(Uuid::nil()),
        call_id: "call-process-1".into(),
        tool_id: "process.run".into(),
        tool_version: 1,
        arguments,
        arguments_digest,
    }
}

/// Fixture options for `process.run` result output JSON (avoids too-many-args).
struct ProcessRunOutputFixture {
    exit_code: Option<i32>,
    duration_ms: u64,
    stdout_sha256: &'static str,
    stderr_sha256: &'static str,
    captured_bytes: u64,
    truncated: bool,
    encoding_lossy: bool,
    cancelled: bool,
    timed_out: bool,
}

fn process_run_output_json(fixture: ProcessRunOutputFixture) -> Value {
    json!({
        "exit_code": fixture.exit_code,
        "duration_ms": fixture.duration_ms,
        "stdout_sha256": fixture.stdout_sha256,
        "stderr_sha256": fixture.stderr_sha256,
        "captured_bytes": fixture.captured_bytes,
        "truncated": fixture.truncated,
        "encoding_lossy": fixture.encoding_lossy,
        "cancelled": fixture.cancelled,
        "timed_out": fixture.timed_out,
    })
}

const EMPTY_STREAM_DIGEST: &str =
    "sha256:e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855";

#[test]
fn process_run_tool_is_in_the_fixed_catalog_as_version_one() {
    assert!(ToolId::all().contains(&ToolId::ProcessRun));
    assert_eq!(ToolId::ProcessRun.as_str(), "process.run");
    let manifests = fixed_tool_manifests();
    let process = manifests
        .iter()
        .find(|manifest| manifest.tool_id == ToolId::ProcessRun)
        .expect("process.run manifest");
    assert_eq!(process.tool_version, 1);
    assert_eq!(process.limits.timeout_ms, 60_000);
    assert_eq!(process.limits.output_byte_cap, 262_144);
    assert!(process.limits.cancellation_checks_required);
}

#[test]
fn valid_process_run_proposal_accepts_exact_program_argv_cwd_schema() {
    let proposal =
        process_run_proposal("cargo", &["test", "-p", "keeper-tool-executor"], Some("."));
    let validated = proposal.validate_for_context(&active_context()).unwrap();
    assert_eq!(validated.tool_id(), "process.run");
    assert_eq!(validated.tool_version(), 1);
    match validated.arguments() {
        ToolArguments::ProcessRun(arguments) => {
            assert_eq!(arguments.program, "cargo");
            assert_eq!(
                arguments.argv,
                vec![
                    "test".to_string(),
                    "-p".to_string(),
                    "keeper-tool-executor".to_string()
                ]
            );
            assert_eq!(arguments.cwd.as_deref(), Some("."));
        }
        other => panic!("expected ProcessRun, got {other:?}"),
    }
    assert_eq!(
        validated.canonical_arguments(),
        &json!({
            "program": "cargo",
            "argv": ["test", "-p", "keeper-tool-executor"],
            "cwd": ".",
        })
    );
}

#[test]
fn valid_process_run_proposal_allows_omitted_cwd() {
    let proposal = process_run_proposal("rustc", &["--version"], None);
    let validated = proposal.validate_for_context(&active_context()).unwrap();
    match validated.arguments() {
        ToolArguments::ProcessRun(arguments) => {
            assert_eq!(arguments.program, "rustc");
            assert_eq!(arguments.argv, vec!["--version".to_string()]);
            assert_eq!(arguments.cwd, None);
        }
        other => panic!("expected ProcessRun, got {other:?}"),
    }
}

#[test]
fn process_run_arguments_deny_unknown_fields() {
    let mut proposal = process_run_proposal("cargo", &["test"], Some("."));
    proposal
        .arguments
        .as_object_mut()
        .unwrap()
        .insert("environment".into(), json!({"PATH": "/evil"}));
    proposal.arguments_digest = canonical_arguments_digest(&proposal.arguments).unwrap();
    assert_eq!(
        proposal
            .validate_for_context(&active_context())
            .unwrap_err()
            .code(),
        ToolProtocolErrorCode::InvalidArguments
    );

    let mut executable = process_run_proposal("cargo", &["test"], Some("."));
    executable
        .arguments
        .as_object_mut()
        .unwrap()
        .insert("executable".into(), json!("C:/Windows/System32/cmd.exe"));
    executable.arguments_digest = canonical_arguments_digest(&executable.arguments).unwrap();
    assert_eq!(
        executable
            .validate_for_context(&active_context())
            .unwrap_err()
            .code(),
        ToolProtocolErrorCode::InvalidArguments
    );
}

#[test]
fn process_run_program_label_must_match_fixed_grammar() {
    let too_long = "a".repeat(33);
    let rejected = [
        "",
        "Cargo",
        "1cargo",
        "-cargo",
        "cargo.exe",
        "cargo/bin",
        "cargo\\bin",
        "c:\\cargo",
        "cargo ",
        " cargo",
        too_long.as_str(),
        "node\0",
        "py\n",
    ];
    for program in rejected {
        let proposal = process_run_proposal(program, &[], None);
        assert_eq!(
            proposal
                .validate_for_context(&active_context())
                .unwrap_err()
                .code(),
            ToolProtocolErrorCode::InvalidArguments,
            "program should be rejected: {program:?}"
        );
    }

    // Last is exactly 32 chars: a-z (26) + 012345 (6).
    for program in [
        "a",
        "cargo",
        "python3",
        "rustc",
        "node",
        "a_b-c0",
        "abcdefghijklmnopqrstuvwxyz012345",
    ] {
        let proposal = process_run_proposal(program, &["--help"], None);
        assert!(
            proposal.validate_for_context(&active_context()).is_ok(),
            "program should be accepted: {program}"
        );
    }
}

#[test]
fn process_run_rejects_nul_and_controls_in_argv() {
    for argv in [["ok\0"], ["line\nbreak"], ["tab\there"], ["\u{1}"]] {
        let proposal = process_run_proposal("cargo", &argv, None);
        assert_eq!(
            proposal
                .validate_for_context(&active_context())
                .unwrap_err()
                .code(),
            ToolProtocolErrorCode::InvalidArguments,
            "argv should be rejected: {argv:?}"
        );
    }
}

/// EA-06: bidi *formatting* controls (Unicode Format, not Control) must not
/// pass process argv validation. `char::is_control` alone is insufficient.
#[test]
fn process_run_rejects_bidi_formatting_controls_in_argv() {
    // Audit list: U+202A/U+202B/U+202D/U+202E, U+2066..=U+2069.
    const BIDI_FORMAT: [char; 8] = [
        '\u{202A}', // LRE
        '\u{202B}', // RLE
        '\u{202D}', // LRO
        '\u{202E}', // RLO — classic extension-order spoof
        '\u{2066}', // LRI
        '\u{2067}', // RLI
        '\u{2068}', // FSI
        '\u{2069}', // PDI
    ];
    for ch in BIDI_FORMAT {
        // Classic visual spoof: RLO reverses "gnp.exe" toward ".exe.png"-like order.
        let spoofed = format!("photo{ch}gnp.exe");
        let proposal = process_run_proposal("node", &[&spoofed], None);
        assert_eq!(
            proposal
                .validate_for_context(&active_context())
                .unwrap_err()
                .code(),
            ToolProtocolErrorCode::InvalidArguments,
            "bidi formatting U+{:04X} must be rejected in argv",
            ch as u32
        );
        // Mixing into an otherwise-valid flag must still fail.
        let mixed = format!("--file={ch}secret");
        let proposal = process_run_proposal("cargo", &[&mixed], None);
        assert_eq!(
            proposal
                .validate_for_context(&active_context())
                .unwrap_err()
                .code(),
            ToolProtocolErrorCode::InvalidArguments,
            "embedded bidi formatting U+{:04X} must be rejected",
            ch as u32
        );
    }
}

/// EA-06: RTL *letters* (Arabic/Hebrew) are not formatting controls and must
/// still be accepted so legitimate locale filenames are not broken.
#[test]
fn process_run_accepts_rtl_script_letters_without_bidi_controls() {
    for entry in ["ملف.txt", "שלום.js", "docs/ملف/readme.md", "עברית-name"] {
        let proposal = process_run_proposal("node", &[entry], None);
        assert!(
            proposal.validate_for_context(&active_context()).is_ok(),
            "RTL letter filename must validate: {entry}"
        );
    }
}

/// EA-06: project-relative paths also reject bidi formatting (filename surface).
#[test]
fn process_run_cwd_and_fs_paths_reject_bidi_formatting() {
    let rlo_path = "src/photo\u{202E}gnp.exe";
    let proposal = process_run_proposal("cargo", &["test"], Some(rlo_path));
    assert_eq!(
        proposal
            .validate_for_context(&active_context())
            .unwrap_err()
            .code(),
        ToolProtocolErrorCode::InvalidArguments,
        "cwd with RLO must be rejected"
    );
}

#[test]
fn process_run_argv_count_boundary_is_sixty_four() {
    let at_cap: Vec<String> = (0..PROCESS_ARGV_COUNT_CAP)
        .map(|i| format!("a{i}"))
        .collect();
    let at_refs: Vec<&str> = at_cap.iter().map(String::as_str).collect();
    assert!(
        process_run_proposal("cargo", &at_refs, None)
            .validate_for_context(&active_context())
            .is_ok()
    );

    let over: Vec<String> = (0..PROCESS_ARGV_COUNT_CAP + 1)
        .map(|i| format!("a{i}"))
        .collect();
    let over_refs: Vec<&str> = over.iter().map(String::as_str).collect();
    assert_eq!(
        process_run_proposal("cargo", &over_refs, None)
            .validate_for_context(&active_context())
            .unwrap_err()
            .code(),
        ToolProtocolErrorCode::InvalidArguments
    );
}

#[test]
fn process_run_combined_program_argv_byte_boundary_is_32kib() {
    // program "cargo" = 5 bytes; fill argv to exact remaining budget with one entry.
    let program = "cargo";
    let remaining = PROCESS_ARGV_BYTE_CAP - program.len();
    let at_cap = "x".repeat(remaining);
    assert!(
        process_run_proposal(program, &[at_cap.as_str()], None)
            .validate_for_context(&active_context())
            .is_ok()
    );

    let over = "x".repeat(remaining + 1);
    assert_eq!(
        process_run_proposal(program, &[over.as_str()], None)
            .validate_for_context(&active_context())
            .unwrap_err()
            .code(),
        ToolProtocolErrorCode::InvalidArguments
    );
}

#[test]
fn process_run_reuses_portable_cwd_grammar() {
    for cwd in [
        "../escape",
        "/abs",
        r"src\win",
        "C:drive",
        "src/../lib",
        "CON",
        "src/name.",
        "",
        "path\0",
    ] {
        let proposal = process_run_proposal("cargo", &["test"], Some(cwd));
        assert_eq!(
            proposal
                .validate_for_context(&active_context())
                .unwrap_err()
                .code(),
            ToolProtocolErrorCode::InvalidArguments,
            "cwd should be rejected: {cwd:?}"
        );
    }

    for cwd in [".", "crates", "crates/agent-protocol", "src/lib"] {
        let proposal = process_run_proposal("cargo", &["test"], Some(cwd));
        assert!(
            proposal.validate_for_context(&active_context()).is_ok(),
            "cwd should be accepted: {cwd}"
        );
    }
}

#[test]
fn process_run_version_and_digest_mismatch_are_rejected() {
    let mut version = process_run_proposal("cargo", &["test"], Some("."));
    version.tool_version = 2;
    assert_eq!(
        version
            .validate_for_context(&active_context())
            .unwrap_err()
            .code(),
        ToolProtocolErrorCode::UnsupportedToolVersion
    );

    let mut digest = process_run_proposal("cargo", &["test"], Some("."));
    digest.arguments_digest = ArgumentsDigest::parse(
        "sha256:44136fa355b3678a1146ad16f7e8649e94fb4fc21fe77e8310c060f61caaff8a",
    )
    .unwrap();
    assert_eq!(
        digest
            .validate_for_context(&active_context())
            .unwrap_err()
            .code(),
        ToolProtocolErrorCode::ArgumentsDigestMismatch
    );
}

#[test]
fn successful_process_run_result_requires_exact_metadata_fields() {
    let context = active_context();
    let proposal = process_run_proposal("cargo", &["test"], Some("."))
        .validate_for_context(&context)
        .unwrap();
    let mut result = ToolResult {
        project_id: proposal.project_id().into(),
        conversation_id: proposal.conversation_id().into(),
        run_id: proposal.run_id(),
        call_id: proposal.call_id().into(),
        tool_id: proposal.tool_id().into(),
        tool_version: proposal.tool_version(),
        proposal_fingerprint: proposal.fingerprint().clone(),
        arguments_digest: proposal.normalized_arguments_digest().clone(),
        state: ToolExecutionState::Succeeded,
        output: Some(process_run_output_json(ProcessRunOutputFixture {
            exit_code: Some(0),
            duration_ms: 12,
            stdout_sha256: EMPTY_STREAM_DIGEST,
            stderr_sha256: EMPTY_STREAM_DIGEST,
            captured_bytes: 0,
            truncated: false,
            encoding_lossy: false,
            cancelled: false,
            timed_out: false,
        })),
        error: None,
    };
    let validated = result.validate_for_proposal(&context, &proposal).unwrap();
    assert_eq!(validated.state(), ToolExecutionState::Succeeded);
    let output = validated.output().unwrap();
    assert_eq!(output["exit_code"], json!(0));
    assert_eq!(output["duration_ms"], json!(12));
    assert_eq!(output["stdout_sha256"], json!(EMPTY_STREAM_DIGEST));
    assert_eq!(output["stderr_sha256"], json!(EMPTY_STREAM_DIGEST));
    assert_eq!(output["captured_bytes"], json!(0));
    assert_eq!(output["truncated"], json!(false));
    assert_eq!(output["encoding_lossy"], json!(false));
    assert_eq!(output["cancelled"], json!(false));
    assert_eq!(output["timed_out"], json!(false));

    // Unknown output field fails closed.
    result
        .output
        .as_mut()
        .unwrap()
        .as_object_mut()
        .unwrap()
        .insert("stdout".into(), json!("raw transcript must not appear"));
    assert_eq!(
        result
            .validate_for_proposal(&context, &proposal)
            .unwrap_err()
            .code(),
        ToolProtocolErrorCode::InvalidResult
    );

    // Restore and reject non-canonical digest shape.
    result.output = Some(process_run_output_json(ProcessRunOutputFixture {
        exit_code: Some(0),
        duration_ms: 12,
        stdout_sha256: "not-a-digest",
        stderr_sha256: EMPTY_STREAM_DIGEST,
        captured_bytes: 0,
        truncated: false,
        encoding_lossy: false,
        cancelled: false,
        timed_out: false,
    }));
    assert_eq!(
        result
            .validate_for_proposal(&context, &proposal)
            .unwrap_err()
            .code(),
        ToolProtocolErrorCode::InvalidResult
    );

    // Null exit code is allowed (timeout/cancel/signal without code).
    result.output = Some(process_run_output_json(ProcessRunOutputFixture {
        exit_code: None,
        duration_ms: 60_000,
        stdout_sha256: EMPTY_STREAM_DIGEST,
        stderr_sha256: EMPTY_STREAM_DIGEST,
        captured_bytes: 100,
        truncated: true,
        encoding_lossy: true,
        cancelled: false,
        timed_out: true,
    }));
    assert!(result.validate_for_proposal(&context, &proposal).is_ok());
}
