//! Assignment state machine — pure validation, no I/O.

use crate::error::TransitionError;
use crate::types::AssignmentState;

/// Terminal states never accept further transitions.
pub fn is_assignment_terminal(state: AssignmentState) -> bool {
    matches!(
        state,
        AssignmentState::Succeeded
            | AssignmentState::Failed
            | AssignmentState::Rejected
            | AssignmentState::Cancelled
    )
}

/// Validate a pure assignment state transition.
///
/// Allowed graph:
/// - `Proposed` → `Approved` | `Rejected` | `Cancelled`
/// - `Approved` → `Running` | `Rejected` | `Cancelled`
/// - `Running`  → `Succeeded` | `Failed` | `Cancelled`
///
/// Same-state and any other edge are illegal. Terminal states never reopen.
pub fn validate_assignment_transition(
    from: AssignmentState,
    to: AssignmentState,
) -> Result<(), TransitionError> {
    if from == to {
        return Err(TransitionError::IllegalTransition { from, to });
    }
    if is_assignment_terminal(from) {
        return Err(TransitionError::IllegalTransition { from, to });
    }

    let allowed = match from {
        AssignmentState::Proposed => matches!(
            to,
            AssignmentState::Approved | AssignmentState::Rejected | AssignmentState::Cancelled
        ),
        AssignmentState::Approved => matches!(
            to,
            AssignmentState::Running | AssignmentState::Rejected | AssignmentState::Cancelled
        ),
        AssignmentState::Running => matches!(
            to,
            AssignmentState::Succeeded | AssignmentState::Failed | AssignmentState::Cancelled
        ),
        AssignmentState::Succeeded
        | AssignmentState::Failed
        | AssignmentState::Rejected
        | AssignmentState::Cancelled => false,
    };

    if allowed {
        Ok(())
    } else {
        Err(TransitionError::IllegalTransition { from, to })
    }
}

/// Apply a validated transition, returning the next state.
pub fn apply_assignment_transition(
    from: AssignmentState,
    to: AssignmentState,
) -> Result<AssignmentState, TransitionError> {
    if is_assignment_terminal(from) {
        return Err(TransitionError::AlreadyTerminal { state: from });
    }
    validate_assignment_transition(from, to)?;
    Ok(to)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn terminal_predicate_matches_vocabulary() {
        for state in AssignmentState::TERMINAL {
            assert!(is_assignment_terminal(*state), "{state:?}");
        }
        for state in AssignmentState::NON_TERMINAL {
            assert!(!is_assignment_terminal(*state), "{state:?}");
        }
    }

    #[test]
    fn legal_edges_are_accepted() {
        let legal = [
            (AssignmentState::Proposed, AssignmentState::Approved),
            (AssignmentState::Proposed, AssignmentState::Rejected),
            (AssignmentState::Proposed, AssignmentState::Cancelled),
            (AssignmentState::Approved, AssignmentState::Running),
            (AssignmentState::Approved, AssignmentState::Rejected),
            (AssignmentState::Approved, AssignmentState::Cancelled),
            (AssignmentState::Running, AssignmentState::Succeeded),
            (AssignmentState::Running, AssignmentState::Failed),
            (AssignmentState::Running, AssignmentState::Cancelled),
        ];
        for (from, to) in legal {
            assert!(
                validate_assignment_transition(from, to).is_ok(),
                "expected legal {from:?} -> {to:?}"
            );
            assert_eq!(apply_assignment_transition(from, to).ok(), Some(to));
        }
    }

    #[test]
    fn illegal_edges_are_rejected_both_directions_of_check() {
        // Representative illegal jumps (full matrix is in integration tests).
        let illegal = [
            (AssignmentState::Proposed, AssignmentState::Running),
            (AssignmentState::Proposed, AssignmentState::Succeeded),
            (AssignmentState::Approved, AssignmentState::Succeeded),
            (AssignmentState::Approved, AssignmentState::Proposed),
            (AssignmentState::Running, AssignmentState::Approved),
            (AssignmentState::Running, AssignmentState::Rejected),
            (AssignmentState::Succeeded, AssignmentState::Proposed),
            (AssignmentState::Failed, AssignmentState::Running),
            (AssignmentState::Rejected, AssignmentState::Approved),
            (AssignmentState::Cancelled, AssignmentState::Running),
        ];
        for (from, to) in illegal {
            let v = validate_assignment_transition(from, to);
            assert!(
                matches!(
                    v,
                    Err(TransitionError::IllegalTransition {
                        from: f,
                        to: t
                    }) if f == from && t == to
                ),
                "expected illegal {from:?} -> {to:?}, got {v:?}"
            );
        }
    }

    #[test]
    fn same_state_is_illegal() {
        for state in AssignmentState::ALL {
            assert!(matches!(
                validate_assignment_transition(*state, *state),
                Err(TransitionError::IllegalTransition { from, to }) if from == *state && to == *state
            ));
        }
    }
}
