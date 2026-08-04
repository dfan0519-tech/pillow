use chrono::{Duration, TimeZone, Utc};
use keeper_policy::{
    PolicyError, WRITE_PROPOSAL_TTL_SECS, WriteDecision, WriteProposalState, apply_write_decision,
    is_expired, is_terminal, validate_write_transition,
};

fn fixed_now() -> chrono::DateTime<Utc> {
    Utc.with_ymd_and_hms(2026, 8, 1, 12, 0, 0).unwrap()
}

#[test]
fn write_proposal_ttl_is_ten_minutes() {
    assert_eq!(WRITE_PROPOSAL_TTL_SECS, 600);
}

#[test]
fn state_vocabulary_is_fixed_and_snake_case() {
    let names: Vec<String> = WriteProposalState::ALL
        .iter()
        .map(|state| serde_json::to_string(state).unwrap())
        .collect();
    assert_eq!(
        names,
        vec![
            "\"pending_approval\"",
            "\"approved\"",
            "\"applying\"",
            "\"succeeded\"",
            "\"rejected\"",
            "\"expired\"",
            "\"cancelled\"",
            "\"interrupted\"",
            "\"failed\"",
        ]
    );
}

#[test]
fn allows_pending_approval_transitions() {
    for next in [
        WriteProposalState::Approved,
        WriteProposalState::Rejected,
        WriteProposalState::Expired,
        WriteProposalState::Cancelled,
        WriteProposalState::Interrupted,
    ] {
        assert!(
            validate_write_transition(WriteProposalState::PendingApproval, next).is_ok(),
            "PendingApproval -> {next:?}"
        );
    }
}

#[test]
fn allows_approved_transitions() {
    for next in [
        WriteProposalState::Applying,
        WriteProposalState::Expired,
        WriteProposalState::Cancelled,
        WriteProposalState::Interrupted,
    ] {
        assert!(
            validate_write_transition(WriteProposalState::Approved, next).is_ok(),
            "Approved -> {next:?}"
        );
    }
}

#[test]
fn allows_applying_transitions() {
    for next in [
        WriteProposalState::Succeeded,
        WriteProposalState::Failed,
        WriteProposalState::Interrupted,
    ] {
        assert!(
            validate_write_transition(WriteProposalState::Applying, next).is_ok(),
            "Applying -> {next:?}"
        );
    }
}

#[test]
fn rejects_illegal_non_terminal_jumps() {
    let illegal = [
        (
            WriteProposalState::PendingApproval,
            WriteProposalState::Succeeded,
        ),
        (
            WriteProposalState::PendingApproval,
            WriteProposalState::Failed,
        ),
        (
            WriteProposalState::PendingApproval,
            WriteProposalState::Applying,
        ),
        (WriteProposalState::Approved, WriteProposalState::Succeeded),
        (WriteProposalState::Approved, WriteProposalState::Failed),
        (WriteProposalState::Approved, WriteProposalState::Rejected),
        (
            WriteProposalState::Approved,
            WriteProposalState::PendingApproval,
        ),
        (WriteProposalState::Applying, WriteProposalState::Approved),
        (WriteProposalState::Applying, WriteProposalState::Rejected),
        (WriteProposalState::Applying, WriteProposalState::Expired),
        (WriteProposalState::Applying, WriteProposalState::Cancelled),
        (
            WriteProposalState::Applying,
            WriteProposalState::PendingApproval,
        ),
    ];
    for (from, to) in illegal {
        assert!(
            matches!(
                validate_write_transition(from, to),
                Err(PolicyError::IllegalTransition { .. })
            ),
            "expected illegal {from:?} -> {to:?}"
        );
    }
}

#[test]
fn terminal_states_never_reopen() {
    let terminals = [
        WriteProposalState::Succeeded,
        WriteProposalState::Rejected,
        WriteProposalState::Expired,
        WriteProposalState::Cancelled,
        WriteProposalState::Interrupted,
        WriteProposalState::Failed,
    ];
    for from in terminals {
        assert!(is_terminal(from), "{from:?} should be terminal");
        for to in WriteProposalState::ALL {
            assert!(
                matches!(
                    validate_write_transition(from, *to),
                    Err(PolicyError::IllegalTransition { .. })
                ),
                "terminal {from:?} must not open to {to:?}"
            );
        }
    }
}

#[test]
fn expiry_is_true_at_exact_boundary() {
    let expires_at = fixed_now();
    assert!(!is_expired(expires_at, expires_at - Duration::seconds(1)));
    assert!(is_expired(expires_at, expires_at));
    assert!(is_expired(expires_at, expires_at + Duration::seconds(1)));
}

#[test]
fn approve_once_from_pending_becomes_approved_when_not_expired() {
    let now = fixed_now();
    let expires_at = now + Duration::seconds(WRITE_PROPOSAL_TTL_SECS);
    let next = apply_write_decision(
        WriteProposalState::PendingApproval,
        WriteDecision::ApproveOnce,
        expires_at,
        now,
    )
    .unwrap();
    assert_eq!(next, WriteProposalState::Approved);
}

#[test]
fn reject_from_pending_becomes_rejected_when_not_expired() {
    let now = fixed_now();
    let expires_at = now + Duration::seconds(WRITE_PROPOSAL_TTL_SECS);
    let next = apply_write_decision(
        WriteProposalState::PendingApproval,
        WriteDecision::Reject,
        expires_at,
        now,
    )
    .unwrap();
    assert_eq!(next, WriteProposalState::Rejected);
}

#[test]
fn decision_on_expired_proposal_is_rejected() {
    let expires_at = fixed_now();
    let now = expires_at;
    let error = apply_write_decision(
        WriteProposalState::PendingApproval,
        WriteDecision::ApproveOnce,
        expires_at,
        now,
    )
    .unwrap_err();
    assert_eq!(error, PolicyError::Expired);
}

#[test]
fn decision_is_single_use_and_not_allowed_after_approval() {
    let now = fixed_now();
    let expires_at = now + Duration::seconds(WRITE_PROPOSAL_TTL_SECS);
    let approved = apply_write_decision(
        WriteProposalState::PendingApproval,
        WriteDecision::ApproveOnce,
        expires_at,
        now,
    )
    .unwrap();
    assert_eq!(approved, WriteProposalState::Approved);

    let second =
        apply_write_decision(approved, WriteDecision::ApproveOnce, expires_at, now).unwrap_err();
    assert!(matches!(
        second,
        PolicyError::DecisionNotAllowed {
            state: WriteProposalState::Approved
        }
    ));

    let reject_after =
        apply_write_decision(approved, WriteDecision::Reject, expires_at, now).unwrap_err();
    assert!(matches!(
        reject_after,
        PolicyError::DecisionNotAllowed {
            state: WriteProposalState::Approved
        }
    ));
}

#[test]
fn decision_cannot_reopen_terminal_states() {
    let now = fixed_now();
    let expires_at = now + Duration::seconds(WRITE_PROPOSAL_TTL_SECS);
    for state in [
        WriteProposalState::Succeeded,
        WriteProposalState::Rejected,
        WriteProposalState::Expired,
        WriteProposalState::Cancelled,
        WriteProposalState::Interrupted,
        WriteProposalState::Failed,
    ] {
        let error =
            apply_write_decision(state, WriteDecision::ApproveOnce, expires_at, now).unwrap_err();
        assert_eq!(error, PolicyError::AlreadyTerminal { state });
    }
}

#[test]
fn decision_wire_names_are_snake_case() {
    assert_eq!(
        serde_json::to_string(&WriteDecision::ApproveOnce).unwrap(),
        "\"approve_once\""
    );
    assert_eq!(
        serde_json::to_string(&WriteDecision::Reject).unwrap(),
        "\"reject\""
    );
}

#[test]
fn happy_path_chain_pending_to_succeeded() {
    validate_write_transition(
        WriteProposalState::PendingApproval,
        WriteProposalState::Approved,
    )
    .unwrap();
    validate_write_transition(WriteProposalState::Approved, WriteProposalState::Applying).unwrap();
    validate_write_transition(WriteProposalState::Applying, WriteProposalState::Succeeded).unwrap();
}

#[test]
fn happy_path_chain_pending_to_failed_apply() {
    validate_write_transition(
        WriteProposalState::PendingApproval,
        WriteProposalState::Approved,
    )
    .unwrap();
    validate_write_transition(WriteProposalState::Approved, WriteProposalState::Applying).unwrap();
    validate_write_transition(WriteProposalState::Applying, WriteProposalState::Failed).unwrap();
}
