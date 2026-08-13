//! Native write command service boundary tests.
//!
//! Window grant enforcement (`require_conversation_root_for_window`) is covered
//! by path_scope unit tests: bare conversation UUID, project bootstrap permit,
//! foreign window, and stale window generation all fail until the conversation
//! grant is already bound. Write Tauri commands use require-only (never bind).

use std::sync::Arc;

use crate::conversation_events::NoopEmitter;
use crate::conversation_service::ConversationService;

#[test]
fn decide_rejects_unknown_decision_verb() {
    let temp = tempfile::tempdir().unwrap();
    let db = temp.path().join("pillow.db");
    let service = ConversationService::open(&db, Arc::new(NoopEmitter), false).unwrap();
    let err = service
        .decide_write_proposal(
            "00000000-0000-0000-0000-000000000001".into(),
            "00000000-0000-0000-0000-000000000002".into(),
            "approve_session".into(),
        )
        .unwrap_err();
    // Fail closed: unknown decision or missing conversation — never applies.
    assert!(
        err.code == "write_invalid_decision"
            || err.message.to_ascii_lowercase().contains("conversation")
            || err.message.to_ascii_lowercase().contains("not found")
            || err.code.contains("invalid")
            || err.code.contains("not_found"),
        "unexpected error: {} / {}",
        err.code,
        err.message
    );
}

#[test]
fn open_diff_fails_closed_when_not_live() {
    let temp = tempfile::tempdir().unwrap();
    let db = temp.path().join("pillow.db");
    let service = ConversationService::open(&db, Arc::new(NoopEmitter), false).unwrap();
    let err = service
        .open_write_proposal_diff(
            "00000000-0000-0000-0000-000000000001".into(),
            "00000000-0000-0000-0000-000000000002".into(),
        )
        .unwrap_err();
    assert!(
        err.code == "write_not_live"
            || err.code.contains("not_found")
            || err.code.contains("invalid")
            || err.message.to_ascii_lowercase().contains("conversation"),
        "unexpected error: {} / {}",
        err.code,
        err.message
    );
}

#[test]
fn bare_conversation_uuid_fails_list_before_any_grant() {
    let temp = tempfile::tempdir().unwrap();
    let db = temp.path().join("pillow.db");
    let service = ConversationService::open(&db, Arc::new(NoopEmitter), false).unwrap();
    let err = service
        .list_write_proposals("00000000-0000-0000-0000-000000000099".into())
        .unwrap_err();
    assert!(
        err.code.contains("not_found")
            || err.code.contains("invalid")
            || err.message.to_ascii_lowercase().contains("conversation"),
        "bare UUID must not list proposals: {} / {}",
        err.code,
        err.message
    );
}

/// EA-03 sequence A: Tauri decide command must not accept renderer grant material.
#[test]
fn decide_write_command_source_does_not_accept_renderer_grant() {
    // Normalise line endings: these assertions match exact byte sequences, and
    // git checks this file out CRLF on Windows. Without it the contract holds
    // while the test reports it broken.
    let source = include_str!("write_commands.rs").replace("\r\n", "\n");
    assert!(
        !source.contains("approval_grant"),
        "decide_write_proposal command must not take approval_grant from the renderer"
    );
    assert!(
        source.contains("// Host resolves the live grant; no renderer grant parameter."),
        "host-private grant comment must remain on decide path"
    );
    // Parameter list: conversation_id, proposal_id, decision — no grant.
    assert!(
        source
            .contains("conversation_id: String,\n    proposal_id: String,\n    decision: String,"),
        "decide command signature must be conversation/proposal/decision only"
    );
}
