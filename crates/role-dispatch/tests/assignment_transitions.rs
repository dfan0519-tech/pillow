//! Bidirectional state-machine tests: every legal edge accepted, every illegal
//! edge rejected (full cartesian product over AssignmentState::ALL).

use pillow_role_dispatch::{
    AssignmentState, TransitionError, apply_assignment_transition, is_assignment_terminal,
    validate_assignment_transition,
};

fn legal_edges() -> Vec<(AssignmentState, AssignmentState)> {
    vec![
        (AssignmentState::Proposed, AssignmentState::Approved),
        (AssignmentState::Proposed, AssignmentState::Rejected),
        (AssignmentState::Proposed, AssignmentState::Cancelled),
        (AssignmentState::Approved, AssignmentState::Running),
        (AssignmentState::Approved, AssignmentState::Rejected),
        (AssignmentState::Approved, AssignmentState::Cancelled),
        (AssignmentState::Running, AssignmentState::Succeeded),
        (AssignmentState::Running, AssignmentState::Failed),
        (AssignmentState::Running, AssignmentState::Cancelled),
    ]
}

fn is_legal(from: AssignmentState, to: AssignmentState) -> bool {
    legal_edges().into_iter().any(|(f, t)| f == from && t == to)
}

#[test]
fn all_legal_transitions_are_accepted() {
    for (from, to) in legal_edges() {
        assert!(
            validate_assignment_transition(from, to).is_ok(),
            "legal validate {from:?} -> {to:?}"
        );
        assert_eq!(
            apply_assignment_transition(from, to).expect("apply"),
            to,
            "legal apply {from:?} -> {to:?}"
        );
    }
}

#[test]
fn all_illegal_transitions_are_rejected() {
    for &from in AssignmentState::ALL {
        for &to in AssignmentState::ALL {
            if is_legal(from, to) {
                continue;
            }
            let result = validate_assignment_transition(from, to);
            assert!(
                matches!(
                    result,
                    Err(TransitionError::IllegalTransition {
                        from: f,
                        to: t
                    }) if f == from && t == to
                ),
                "expected illegal {from:?} -> {to:?}, got {result:?}"
            );
        }
    }
}

#[test]
fn apply_rejects_terminal_sources_with_already_terminal() {
    for &from in AssignmentState::TERMINAL {
        for &to in AssignmentState::ALL {
            if from == to {
                // same-state: apply short-circuits via already-terminal first
                // when from is terminal, or illegal for non-terminal same-state.
            }
            let result = apply_assignment_transition(from, to);
            assert!(
                matches!(
                    result,
                    Err(TransitionError::AlreadyTerminal { state }) if state == from
                ),
                "terminal apply {from:?} -> {to:?} got {result:?}"
            );
        }
    }
}

#[test]
fn terminal_predicate_is_stable() {
    assert_eq!(
        AssignmentState::TERMINAL.len() + AssignmentState::NON_TERMINAL.len(),
        AssignmentState::ALL.len()
    );
    for &s in AssignmentState::ALL {
        let term = matches!(
            s,
            AssignmentState::Succeeded
                | AssignmentState::Failed
                | AssignmentState::Rejected
                | AssignmentState::Cancelled
        );
        assert_eq!(is_assignment_terminal(s), term, "{s:?}");
    }
}

#[test]
fn happy_path_chain_proposed_to_succeeded() {
    let mut state = AssignmentState::Proposed;
    for next in [
        AssignmentState::Approved,
        AssignmentState::Running,
        AssignmentState::Succeeded,
    ] {
        state = apply_assignment_transition(state, next).expect("chain");
    }
    assert_eq!(state, AssignmentState::Succeeded);
    assert!(is_assignment_terminal(state));
}

#[test]
fn reject_from_proposed_and_cancel_from_running() {
    assert_eq!(
        apply_assignment_transition(AssignmentState::Proposed, AssignmentState::Rejected).unwrap(),
        AssignmentState::Rejected
    );
    assert_eq!(
        apply_assignment_transition(AssignmentState::Running, AssignmentState::Cancelled).unwrap(),
        AssignmentState::Cancelled
    );
}
