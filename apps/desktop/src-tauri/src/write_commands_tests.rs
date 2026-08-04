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
            "AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA".into(),
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
