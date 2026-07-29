//! Pure run lifecycle transition rules.

use crate::domain::{RunStatus, SessionError};

/// Validate a run status transition. Terminal states never accept further
/// changes. Same-status transitions are rejected.
pub fn validate_run_transition(from: RunStatus, to: RunStatus) -> Result<(), SessionError> {
    if from == to {
        return Err(SessionError::IllegalTransition { from, to });
    }

    let allowed = match from {
        RunStatus::Pending => matches!(
            to,
            RunStatus::Running
                | RunStatus::Failed
                | RunStatus::Cancelled
                | RunStatus::Interrupted
        ),
        RunStatus::Running => matches!(
            to,
            RunStatus::Cancelling
                | RunStatus::Completed
                | RunStatus::Failed
                | RunStatus::Interrupted
        ),
        RunStatus::Cancelling => matches!(
            to,
            RunStatus::Cancelled | RunStatus::Failed | RunStatus::Interrupted
        ),
        RunStatus::Completed
        | RunStatus::Failed
        | RunStatus::Cancelled
        | RunStatus::Interrupted => false,
    };

    if allowed {
        Ok(())
    } else {
        Err(SessionError::IllegalTransition { from, to })
    }
}

#[cfg(test)]
mod tests {
    use super::validate_run_transition;
    use crate::domain::{RunStatus, SessionError};

    #[test]
    fn allows_legal_pending_transitions() {
        for next in [
            RunStatus::Running,
            RunStatus::Failed,
            RunStatus::Cancelled,
            RunStatus::Interrupted,
        ] {
            assert!(
                validate_run_transition(RunStatus::Pending, next).is_ok(),
                "Pending -> {next:?} should be legal"
            );
        }
    }

    #[test]
    fn allows_legal_running_transitions() {
        for next in [
            RunStatus::Cancelling,
            RunStatus::Completed,
            RunStatus::Failed,
            RunStatus::Interrupted,
        ] {
            assert!(
                validate_run_transition(RunStatus::Running, next).is_ok(),
                "Running -> {next:?} should be legal"
            );
        }
    }

    #[test]
    fn allows_legal_cancelling_transitions() {
        for next in [
            RunStatus::Cancelled,
            RunStatus::Failed,
            RunStatus::Interrupted,
        ] {
            assert!(
                validate_run_transition(RunStatus::Cancelling, next).is_ok(),
                "Cancelling -> {next:?} should be legal"
            );
        }
    }

    #[test]
    fn rejects_terminal_state_transitions() {
        for from in [
            RunStatus::Completed,
            RunStatus::Failed,
            RunStatus::Cancelled,
            RunStatus::Interrupted,
        ] {
            for to in RunStatus::ALL {
                let result = validate_run_transition(from, to);
                assert!(
                    matches!(
                        result,
                        Err(SessionError::IllegalTransition {
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
    fn rejects_illegal_non_terminal_jumps() {
        let illegal = [
            (RunStatus::Pending, RunStatus::Completed),
            (RunStatus::Pending, RunStatus::Cancelling),
            (RunStatus::Running, RunStatus::Pending),
            (RunStatus::Running, RunStatus::Cancelled),
            (RunStatus::Cancelling, RunStatus::Running),
            (RunStatus::Cancelling, RunStatus::Pending),
            (RunStatus::Cancelling, RunStatus::Completed),
        ];

        for (from, to) in illegal {
            assert!(
                matches!(
                    validate_run_transition(from, to),
                    Err(SessionError::IllegalTransition { .. })
                ),
                "expected illegal {from:?} -> {to:?}"
            );
        }
    }

    #[test]
    fn rejects_same_status_transitions() {
        for status in RunStatus::ALL {
            assert!(matches!(
                validate_run_transition(status, status),
                Err(SessionError::IllegalTransition { from, to })
                    if from == status && to == status
            ));
        }
    }

    #[test]
    fn classifies_inflight_and_terminal_statuses() {
        assert!(RunStatus::Pending.is_inflight());
        assert!(RunStatus::Running.is_inflight());
        assert!(RunStatus::Cancelling.is_inflight());
        assert!(!RunStatus::Completed.is_inflight());
        assert!(RunStatus::Failed.is_terminal());
        assert!(RunStatus::Cancelled.is_terminal());
        assert!(RunStatus::Interrupted.is_terminal());
    }

    #[test]
    fn run_status_round_trips_through_snake_case_strings() {
        for status in RunStatus::ALL {
            assert_eq!(RunStatus::parse(status.as_str()), Some(status));
        }
        assert_eq!(RunStatus::parse("unknown"), None);
    }
}
