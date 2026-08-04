use chrono::{Duration, TimeZone, Utc};
use keeper_policy::{
    EXEC_PROPOSAL_TTL_SECS, ExecDecision, ExecPolicyError, ExecProposalState, apply_exec_decision,
    is_exec_expired, is_exec_terminal, validate_exec_transition,
};

fn fixed_now() -> chrono::DateTime<Utc> {
    Utc.with_ymd_and_hms(2026, 8, 2, 12, 0, 0).unwrap()
}

#[test]
fn exec_proposal_ttl_is_ten_minutes() {
    assert_eq!(EXEC_PROPOSAL_TTL_SECS, 600);
}

#[test]
fn state_vocabulary_is_fixed_and_snake_case() {
    let names: Vec<String> = ExecProposalState::ALL
        .iter()
        .map(|state| serde_json::to_string(state).unwrap())
        .collect();
    assert_eq!(
        names,
        vec![
            "\"pending_approval\"",
            "\"approved\"",
            "\"running\"",
            "\"succeeded\"",
            "\"failed\"",
            "\"rejected\"",
            "\"expired\"",
            "\"cancelled\"",
            "\"interrupted\"",
        ]
    );
}

#[test]
fn decision_wire_names_are_snake_case() {
    assert_eq!(
        serde_json::to_string(&ExecDecision::ApproveOnce).unwrap(),
        "\"approve_once\""
    );
    assert_eq!(
        serde_json::to_string(&ExecDecision::Reject).unwrap(),
        "\"reject\""
    );
}

#[test]
fn allows_pending_approval_transitions() {
    for next in [
        ExecProposalState::Approved,
        ExecProposalState::Rejected,
        ExecProposalState::Expired,
        ExecProposalState::Cancelled,
        ExecProposalState::Interrupted,
    ] {
        assert!(
            validate_exec_transition(ExecProposalState::PendingApproval, next).is_ok(),
            "PendingApproval -> {next:?}"
        );
    }
}

#[test]
fn allows_approved_transitions() {
    for next in [
        ExecProposalState::Running,
        ExecProposalState::Expired,
        ExecProposalState::Cancelled,
        ExecProposalState::Interrupted,
    ] {
        assert!(
            validate_exec_transition(ExecProposalState::Approved, next).is_ok(),
            "Approved -> {next:?}"
        );
    }
}

#[test]
fn allows_running_transitions() {
    for next in [
        ExecProposalState::Succeeded,
        ExecProposalState::Failed,
        ExecProposalState::Cancelled,
        ExecProposalState::Interrupted,
    ] {
        assert!(
            validate_exec_transition(ExecProposalState::Running, next).is_ok(),
            "Running -> {next:?}"
        );
    }
}

#[test]
fn approved_to_running_is_only_execution_claim() {
    assert!(
        validate_exec_transition(ExecProposalState::Approved, ExecProposalState::Running).is_ok()
    );
    // No other non-terminal state may claim execution by entering Running.
    for from in [
        ExecProposalState::PendingApproval,
        ExecProposalState::Running,
        ExecProposalState::Succeeded,
        ExecProposalState::Failed,
        ExecProposalState::Rejected,
        ExecProposalState::Expired,
        ExecProposalState::Cancelled,
        ExecProposalState::Interrupted,
    ] {
        assert!(
            matches!(
                validate_exec_transition(from, ExecProposalState::Running),
                Err(ExecPolicyError::IllegalTransition { .. })
            ),
            "{from:?} must not claim Running"
        );
    }
}

#[test]
fn rejects_illegal_non_terminal_jumps() {
    let illegal = [
        (
            ExecProposalState::PendingApproval,
            ExecProposalState::Succeeded,
        ),
        (
            ExecProposalState::PendingApproval,
            ExecProposalState::Failed,
        ),
        (
            ExecProposalState::PendingApproval,
            ExecProposalState::Running,
        ),
        (ExecProposalState::Approved, ExecProposalState::Succeeded),
        (ExecProposalState::Approved, ExecProposalState::Failed),
        (ExecProposalState::Approved, ExecProposalState::Rejected),
        (
            ExecProposalState::Approved,
            ExecProposalState::PendingApproval,
        ),
        (ExecProposalState::Running, ExecProposalState::Approved),
        (ExecProposalState::Running, ExecProposalState::Rejected),
        (ExecProposalState::Running, ExecProposalState::Expired),
        (
            ExecProposalState::Running,
            ExecProposalState::PendingApproval,
        ),
    ];
    for (from, to) in illegal {
        assert!(
            matches!(
                validate_exec_transition(from, to),
                Err(ExecPolicyError::IllegalTransition { .. })
            ),
            "expected illegal {from:?} -> {to:?}"
        );
    }
}

#[test]
fn terminal_states_never_reopen() {
    let terminals = [
        ExecProposalState::Succeeded,
        ExecProposalState::Failed,
        ExecProposalState::Rejected,
        ExecProposalState::Expired,
        ExecProposalState::Cancelled,
        ExecProposalState::Interrupted,
    ];
    for from in terminals {
        assert!(is_exec_terminal(from), "{from:?} should be terminal");
        for to in ExecProposalState::ALL {
            assert!(
                matches!(
                    validate_exec_transition(from, *to),
                    Err(ExecPolicyError::IllegalTransition { .. })
                ),
                "terminal {from:?} must not open to {to:?}"
            );
        }
    }
}

/// Full 9×9 transition matrix: every allowed and denied edge is classified.
#[test]
fn complete_transition_matrix_enumerates_all_edges() {
    let mut allowed = 0usize;
    let mut denied = 0usize;
    for from in ExecProposalState::ALL {
        for to in ExecProposalState::ALL {
            match validate_exec_transition(*from, *to) {
                Ok(()) => allowed += 1,
                Err(ExecPolicyError::IllegalTransition { .. }) => denied += 1,
                other => panic!("unexpected result for {from:?} -> {to:?}: {other:?}"),
            }
        }
    }
    // 9 states × 9 targets = 81 edges; exactly 13 legal edges (see design).
    assert_eq!(allowed + denied, 81, "matrix must cover every pair");
    assert_eq!(allowed, 13, "allowed edge count must match process policy");
    assert_eq!(denied, 68, "denied edge count must match process policy");
}

#[test]
fn restart_interrupts_every_non_terminal_state() {
    for from in [
        ExecProposalState::PendingApproval,
        ExecProposalState::Approved,
        ExecProposalState::Running,
    ] {
        assert!(
            !is_exec_terminal(from),
            "{from:?} is non-terminal before restart"
        );
        assert!(
            validate_exec_transition(from, ExecProposalState::Interrupted).is_ok(),
            "restart must interrupt {from:?}"
        );
    }
    // Restart never resumes a terminal state (no reopen into Interrupted from terminal).
    for from in [
        ExecProposalState::Succeeded,
        ExecProposalState::Failed,
        ExecProposalState::Rejected,
        ExecProposalState::Expired,
        ExecProposalState::Cancelled,
        ExecProposalState::Interrupted,
    ] {
        assert!(
            matches!(
                validate_exec_transition(from, ExecProposalState::Interrupted),
                Err(ExecPolicyError::IllegalTransition { .. })
            ),
            "terminal {from:?} must not be reopened by restart"
        );
    }
}

#[test]
fn expiry_is_true_at_exact_boundary() {
    let expires_at = fixed_now();
    assert!(!is_exec_expired(
        expires_at,
        expires_at - Duration::seconds(1)
    ));
    assert!(is_exec_expired(expires_at, expires_at));
    assert!(is_exec_expired(
        expires_at,
        expires_at + Duration::seconds(1)
    ));
}

#[test]
fn approve_once_from_pending_becomes_approved_when_not_expired() {
    let now = fixed_now();
    let expires_at = now + Duration::seconds(EXEC_PROPOSAL_TTL_SECS);
    let next = apply_exec_decision(
        ExecProposalState::PendingApproval,
        ExecDecision::ApproveOnce,
        expires_at,
        now,
    )
    .unwrap();
    assert_eq!(next, ExecProposalState::Approved);
}

#[test]
fn reject_from_pending_becomes_rejected_when_not_expired() {
    let now = fixed_now();
    let expires_at = now + Duration::seconds(EXEC_PROPOSAL_TTL_SECS);
    let next = apply_exec_decision(
        ExecProposalState::PendingApproval,
        ExecDecision::Reject,
        expires_at,
        now,
    )
    .unwrap();
    assert_eq!(next, ExecProposalState::Rejected);
}

#[test]
fn decision_on_expired_proposal_is_rejected() {
    let expires_at = fixed_now();
    let now = expires_at;
    let error = apply_exec_decision(
        ExecProposalState::PendingApproval,
        ExecDecision::ApproveOnce,
        expires_at,
        now,
    )
    .unwrap_err();
    assert_eq!(error, ExecPolicyError::Expired);
}

#[test]
fn decision_is_single_use_and_not_allowed_after_approval() {
    let now = fixed_now();
    let expires_at = now + Duration::seconds(EXEC_PROPOSAL_TTL_SECS);
    let approved = apply_exec_decision(
        ExecProposalState::PendingApproval,
        ExecDecision::ApproveOnce,
        expires_at,
        now,
    )
    .unwrap();
    assert_eq!(approved, ExecProposalState::Approved);

    let second =
        apply_exec_decision(approved, ExecDecision::ApproveOnce, expires_at, now).unwrap_err();
    assert!(matches!(
        second,
        ExecPolicyError::DecisionNotAllowed {
            state: ExecProposalState::Approved
        }
    ));

    let reject_after =
        apply_exec_decision(approved, ExecDecision::Reject, expires_at, now).unwrap_err();
    assert!(matches!(
        reject_after,
        ExecPolicyError::DecisionNotAllowed {
            state: ExecProposalState::Approved
        }
    ));
}

#[test]
fn decision_cannot_reopen_terminal_states() {
    let now = fixed_now();
    let expires_at = now + Duration::seconds(EXEC_PROPOSAL_TTL_SECS);
    for state in [
        ExecProposalState::Succeeded,
        ExecProposalState::Failed,
        ExecProposalState::Rejected,
        ExecProposalState::Expired,
        ExecProposalState::Cancelled,
        ExecProposalState::Interrupted,
    ] {
        let error =
            apply_exec_decision(state, ExecDecision::ApproveOnce, expires_at, now).unwrap_err();
        assert_eq!(error, ExecPolicyError::AlreadyTerminal { state });
    }
}

#[test]
fn decision_not_allowed_while_running() {
    let now = fixed_now();
    let expires_at = now + Duration::seconds(EXEC_PROPOSAL_TTL_SECS);
    let error = apply_exec_decision(
        ExecProposalState::Running,
        ExecDecision::Reject,
        expires_at,
        now,
    )
    .unwrap_err();
    assert!(matches!(
        error,
        ExecPolicyError::DecisionNotAllowed {
            state: ExecProposalState::Running
        }
    ));
}

#[test]
fn happy_path_chain_pending_to_succeeded() {
    validate_exec_transition(
        ExecProposalState::PendingApproval,
        ExecProposalState::Approved,
    )
    .unwrap();
    validate_exec_transition(ExecProposalState::Approved, ExecProposalState::Running).unwrap();
    validate_exec_transition(ExecProposalState::Running, ExecProposalState::Succeeded).unwrap();
}

#[test]
fn happy_path_chain_pending_to_failed_run() {
    validate_exec_transition(
        ExecProposalState::PendingApproval,
        ExecProposalState::Approved,
    )
    .unwrap();
    validate_exec_transition(ExecProposalState::Approved, ExecProposalState::Running).unwrap();
    validate_exec_transition(ExecProposalState::Running, ExecProposalState::Failed).unwrap();
}

#[test]
fn happy_path_chain_running_to_cancelled() {
    validate_exec_transition(ExecProposalState::Running, ExecProposalState::Cancelled).unwrap();
}
