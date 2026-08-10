//! Durable process-proposal ledger (metadata and digests only).
//!
//! Storage owns proposal TTL, validates all bounded digests/program labels
//! before SQL, serializes typed terminal receipts, and provides
//! owner-independent restart interruption for non-terminal rows.
//!
//! Sealed executable/argv/native cwd, raw grants, environment and full
//! transcript never enter SQLite.

use std::path::Path;
#[cfg(any(test, feature = "test-support"))]
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Mutex, MutexGuard};

use chrono::{DateTime, Duration, Utc};
use keeper_agent_protocol::RunId;
use keeper_policy::{
    EXEC_PROPOSAL_TTL_SECS, ExecDecision, ExecProposalState, is_exec_expired, is_exec_terminal,
    validate_exec_transition,
};
use rusqlite::types::Type;
use rusqlite::{Connection, OptionalExtension, Row, TransactionBehavior, ffi, params};
use serde::{Deserialize, Serialize};
use thiserror::Error;
use uuid::Uuid;

use crate::permission_rules::{
    PermissionRuleAction, PermissionRuleMatchSnapshot, PermissionRulePrefix,
    PermissionRuleReceiptActor, PermissionRuleReceiptDecision, StoredPermissionRuleReceipt,
    permission_rule_receipt_from_row, receipt_matches_snapshot, rule_snapshot_exists,
    validate_rule_match_snapshot,
};

/// Maximum run-lease duration the repository will accept (milliseconds).
pub const MAX_RUN_LEASE_MS: u64 = 300_000;

/// Upper bound for stored terminal `result_json` text.
pub const EXEC_RESULT_JSON_BYTE_CAP: usize = 8_192;

/// Immutable identity + digest record for a governed process proposal.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StoredExecProposal {
    pub proposal_id: String,
    pub reservation_key: String,
    pub proposal_fingerprint: String,
    pub project_id: String,
    pub conversation_id: String,
    pub run_id: RunId,
    pub call_id: String,
    pub program: String,
    pub argv_digest: String,
    pub portable_cwd: Option<String>,
    pub state: ExecProposalState,
    pub expires_at: DateTime<Utc>,
    pub result_json: Option<String>,
    pub started_at: Option<DateTime<Utc>>,
    pub completed_at: Option<DateTime<Utc>>,
    pub exit_code: Option<i32>,
    pub duration_ms: Option<u64>,
    pub stdout_digest: Option<String>,
    pub stderr_digest: Option<String>,
    pub captured_bytes: Option<u64>,
    pub truncated: Option<bool>,
    pub encoding_lossy: Option<bool>,
    pub cancelled: Option<bool>,
    pub timed_out: Option<bool>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// Host-supplied reservation payload. Never includes raw argv, native paths,
/// environment, grants or transcript. TTL is owned by storage.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExecProposalReserveInput {
    pub proposal_id: String,
    pub reservation_key: String,
    pub proposal_fingerprint: String,
    pub project_id: String,
    pub conversation_id: String,
    pub run_id: RunId,
    pub call_id: String,
    pub program: String,
    pub argv_digest: String,
    pub portable_cwd: Option<String>,
}

/// Typed terminal process receipt. Repository serializes this to `result_json`.
///
/// No argv, native path, grant, environment or transcript payload is permitted.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ExecTerminalReceipt {
    pub exit_code: Option<i32>,
    pub duration_ms: u64,
    pub stdout_sha256: String,
    pub stderr_sha256: String,
    pub captured_bytes: u64,
    pub truncated: bool,
    pub encoding_lossy: bool,
    pub cancelled: bool,
    pub timed_out: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ExecReservationOutcome {
    Reserved(StoredExecProposal),
    Replay(StoredExecProposal),
    Conflict(StoredExecProposal),
    Busy(StoredExecProposal),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ExecDecisionOutcome {
    Accepted(StoredExecProposal),
    Terminal(StoredExecProposal),
    Conflict(StoredExecProposal),
    Expired(StoredExecProposal),
}

enum ExecDecisionSource<'a> {
    Human(ExecDecision),
    Rule(&'a PermissionRuleMatchSnapshot),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ExecRunClaimOutcome {
    Claimed(StoredExecProposal),
    Busy(StoredExecProposal),
    Terminal(StoredExecProposal),
    Expired(StoredExecProposal),
    Conflict(StoredExecProposal),
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum ExecProposalStoreError {
    #[error("exec proposal context does not exist in the durable project conversation run graph")]
    MissingContext,
    #[error("exec proposal conversation ownership mismatch")]
    OwnershipMismatch,
    #[error("exec proposal transition is not permitted")]
    InvalidTransition,
    #[error("exec proposal result does not match the requested terminal state")]
    InvalidResult,
    #[error("exec proposal reservation conflicts with durable state")]
    ReservationConflict,
    #[error("exec proposal rule receipt is invalid")]
    InvalidRuleReceipt,
    #[error("SQLite exec proposal store error")]
    Sqlite,
    #[error("exec proposal store lock was poisoned")]
    LockPoisoned,
}

pub struct SqliteExecProposalRepository {
    connection: Mutex<Connection>,
    #[cfg(any(test, feature = "test-support"))]
    fail_writes: AtomicBool,
    #[cfg(any(test, feature = "test-support"))]
    write_attempts: AtomicUsize,
}

impl SqliteExecProposalRepository {
    pub fn open(path: impl AsRef<Path>) -> Result<Self, ExecProposalStoreError> {
        let connection = Connection::open(path).map_err(map_sqlite)?;
        connection
            .execute_batch("PRAGMA foreign_keys = ON;")
            .map_err(map_sqlite)?;
        super::migrations::apply(&connection).map_err(map_sqlite)?;
        Ok(Self {
            connection: Mutex::new(connection),
            #[cfg(any(test, feature = "test-support"))]
            fail_writes: AtomicBool::new(false),
            #[cfg(any(test, feature = "test-support"))]
            write_attempts: AtomicUsize::new(0),
        })
    }

    #[cfg(any(test, feature = "test-support"))]
    pub fn set_fail_writes_for_tests(&self, fail: bool) {
        self.fail_writes.store(fail, Ordering::SeqCst);
    }

    #[cfg(any(test, feature = "test-support"))]
    pub fn write_attempt_count_for_tests(&self) -> usize {
        self.write_attempts.load(Ordering::SeqCst)
    }

    #[cfg(any(test, feature = "test-support"))]
    fn begin_mutation(&self) -> Result<(), ExecProposalStoreError> {
        self.write_attempts.fetch_add(1, Ordering::SeqCst);
        if self.fail_writes.load(Ordering::SeqCst) {
            return Err(ExecProposalStoreError::Sqlite);
        }
        Ok(())
    }

    #[cfg(not(any(test, feature = "test-support")))]
    fn begin_mutation(&self) -> Result<(), ExecProposalStoreError> {
        Ok(())
    }

    /// Reserve with storage-owned TTL at wall-clock now.
    pub fn reserve(
        &self,
        input: &ExecProposalReserveInput,
    ) -> Result<ExecReservationOutcome, ExecProposalStoreError> {
        self.reserve_at(input, Utc::now())
    }

    /// Deterministic reserve seam: `created_at = now`,
    /// `expires_at = now + EXEC_PROPOSAL_TTL_SECS` exactly.
    pub fn reserve_at(
        &self,
        input: &ExecProposalReserveInput,
        now: DateTime<Utc>,
    ) -> Result<ExecReservationOutcome, ExecProposalStoreError> {
        self.begin_mutation()?;
        validate_reserve_input(input)?;
        let mut connection = self.connection()?;
        let transaction = connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(map_sqlite)?;
        ensure_context_exists(
            &transaction,
            &input.project_id,
            &input.conversation_id,
            &input.run_id.0.to_string(),
        )?;

        if let Some(existing) = load_by_identity(
            &transaction,
            &input.project_id,
            &input.conversation_id,
            &input.run_id.0.to_string(),
            &input.call_id,
        )? {
            transaction.commit().map_err(map_sqlite)?;
            return if existing.proposal_fingerprint == input.proposal_fingerprint
                && existing.proposal_id == input.proposal_id
                && existing.reservation_key == input.reservation_key
            {
                Ok(ExecReservationOutcome::Replay(existing))
            } else {
                Ok(ExecReservationOutcome::Conflict(existing))
            };
        }

        if let Some(existing) = load_by_id(&transaction, &input.proposal_id)? {
            transaction.commit().map_err(map_sqlite)?;
            return Ok(ExecReservationOutcome::Conflict(existing));
        }
        if let Some(existing) = load_by_reservation_key(&transaction, &input.reservation_key)? {
            transaction.commit().map_err(map_sqlite)?;
            return Ok(ExecReservationOutcome::Conflict(existing));
        }

        if let Some(active) = load_active_on_run(&transaction, &input.run_id.0.to_string())? {
            transaction.commit().map_err(map_sqlite)?;
            return Ok(ExecReservationOutcome::Busy(active));
        }

        let expires_at = now + Duration::seconds(EXEC_PROPOSAL_TTL_SECS);
        let insert = transaction.execute(
            "INSERT INTO exec_proposals (
                    proposal_id, reservation_key, proposal_fingerprint, project_id,
                    conversation_id, run_id, call_id, program, argv_digest, portable_cwd,
                    state, expires_at, result_json, run_owner, run_lease_expires_at,
                    started_at, completed_at, exit_code, duration_ms, stdout_digest,
                    stderr_digest, captured_bytes, truncated, encoding_lossy, cancelled,
                    timed_out, created_at, updated_at
                 ) VALUES (
                    ?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10,
                    'pending_approval', ?11, NULL, NULL, NULL,
                    NULL, NULL, NULL, NULL, NULL,
                    NULL, NULL, NULL, NULL, NULL,
                    NULL, ?12, ?12
                 )",
            params![
                input.proposal_id,
                input.reservation_key,
                input.proposal_fingerprint,
                input.project_id,
                input.conversation_id,
                input.run_id.0.to_string(),
                input.call_id,
                input.program,
                input.argv_digest,
                input.portable_cwd,
                expires_at.to_rfc3339(),
                now.to_rfc3339(),
            ],
        );
        match insert {
            Ok(_) => {}
            Err(error) if is_unique_constraint(&error) => {
                if let Some(existing) = load_by_id(&transaction, &input.proposal_id)? {
                    transaction.commit().map_err(map_sqlite)?;
                    return Ok(ExecReservationOutcome::Conflict(existing));
                }
                if let Some(existing) =
                    load_by_reservation_key(&transaction, &input.reservation_key)?
                {
                    transaction.commit().map_err(map_sqlite)?;
                    return Ok(ExecReservationOutcome::Conflict(existing));
                }
                if let Some(existing) = load_by_identity(
                    &transaction,
                    &input.project_id,
                    &input.conversation_id,
                    &input.run_id.0.to_string(),
                    &input.call_id,
                )? {
                    transaction.commit().map_err(map_sqlite)?;
                    return Ok(ExecReservationOutcome::Conflict(existing));
                }
                return Err(ExecProposalStoreError::ReservationConflict);
            }
            Err(error) => return Err(map_sqlite(error)),
        }
        insert_transition(
            &transaction,
            &input.proposal_id,
            ExecProposalState::PendingApproval,
            None,
            now,
        )?;
        let stored =
            load_by_id(&transaction, &input.proposal_id)?.ok_or(ExecProposalStoreError::Sqlite)?;
        transaction.commit().map_err(map_sqlite)?;
        Ok(ExecReservationOutcome::Reserved(stored))
    }

    pub fn get(
        &self,
        proposal_id: &str,
    ) -> Result<Option<StoredExecProposal>, ExecProposalStoreError> {
        let connection = self.connection()?;
        load_by_id(&connection, proposal_id)
    }

    pub fn decide(
        &self,
        proposal_id: &str,
        conversation_id: &str,
        decision: ExecDecision,
        approval_grant_digest: &str,
        proposal_fingerprint: &str,
        now: DateTime<Utc>,
    ) -> Result<ExecDecisionOutcome, ExecProposalStoreError> {
        self.decide_internal(
            proposal_id,
            conversation_id,
            ExecDecisionSource::Human(decision),
            approval_grant_digest,
            proposal_fingerprint,
            now,
        )
    }

    pub fn decide_by_rule(
        &self,
        proposal_id: &str,
        conversation_id: &str,
        matched: &PermissionRuleMatchSnapshot,
        approval_grant_digest: &str,
        proposal_fingerprint: &str,
        now: DateTime<Utc>,
    ) -> Result<ExecDecisionOutcome, ExecProposalStoreError> {
        validate_rule_match_snapshot(matched)
            .map_err(|_| ExecProposalStoreError::InvalidRuleReceipt)?;
        if matched.rule.prefix != PermissionRulePrefix::Process {
            return Err(ExecProposalStoreError::InvalidRuleReceipt);
        }
        self.decide_internal(
            proposal_id,
            conversation_id,
            ExecDecisionSource::Rule(matched),
            approval_grant_digest,
            proposal_fingerprint,
            now,
        )
    }

    pub fn get_rule_receipt(
        &self,
        proposal_id: &str,
    ) -> Result<Option<StoredPermissionRuleReceipt>, ExecProposalStoreError> {
        let connection = self.connection()?;
        load_rule_receipt(&connection, proposal_id)
    }

    fn decide_internal(
        &self,
        proposal_id: &str,
        conversation_id: &str,
        source: ExecDecisionSource<'_>,
        approval_grant_digest: &str,
        proposal_fingerprint: &str,
        now: DateTime<Utc>,
    ) -> Result<ExecDecisionOutcome, ExecProposalStoreError> {
        let (decision, matched) = match source {
            ExecDecisionSource::Human(decision) => (decision, None),
            ExecDecisionSource::Rule(matched) => {
                let decision = match matched.rule.action {
                    PermissionRuleAction::Allow => ExecDecision::ApproveOnce,
                    PermissionRuleAction::Deny => ExecDecision::Reject,
                };
                (decision, Some(matched))
            }
        };
        self.begin_mutation()?;
        validate_canonical_digest(approval_grant_digest)?;
        validate_canonical_digest(proposal_fingerprint)?;
        let mut connection = self.connection()?;
        let transaction = connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(map_sqlite)?;
        let current = load_by_id(&transaction, proposal_id)?
            .ok_or(ExecProposalStoreError::ReservationConflict)?;
        if current.conversation_id != conversation_id {
            return Err(ExecProposalStoreError::OwnershipMismatch);
        }
        if let Some(matched) = matched {
            let program_prefix = format!("{} ", current.program);
            if matched.match_subject != current.program
                && !matched.match_subject.starts_with(&program_prefix)
            {
                return Err(ExecProposalStoreError::InvalidRuleReceipt);
            }
        }
        if current.proposal_fingerprint != proposal_fingerprint {
            transaction.commit().map_err(map_sqlite)?;
            return Ok(ExecDecisionOutcome::Conflict(current));
        }

        if let Some(existing_decision) = load_decision(&transaction, proposal_id)? {
            let existing_rule_receipt = load_rule_receipt(&transaction, proposal_id)?;
            let same_source = match (matched, existing_rule_receipt.as_ref()) {
                (None, None) => true,
                (Some(matched), Some(receipt)) => {
                    receipt_matches_snapshot(receipt, matched)
                        && receipt.decision == rule_receipt_decision(matched.rule.action)
                }
                (None, Some(_)) | (Some(_), None) => false,
            };
            if !same_source {
                transaction.commit().map_err(map_sqlite)?;
                return Ok(ExecDecisionOutcome::Conflict(current));
            }
            if is_exec_terminal(current.state) {
                if existing_decision.decision == decision
                    && existing_decision.approval_grant_digest == approval_grant_digest
                    && existing_decision.proposal_fingerprint == proposal_fingerprint
                {
                    transaction.commit().map_err(map_sqlite)?;
                    return Ok(ExecDecisionOutcome::Terminal(current));
                }
                transaction.commit().map_err(map_sqlite)?;
                return Ok(ExecDecisionOutcome::Conflict(current));
            }
            if existing_decision.decision == decision
                && existing_decision.approval_grant_digest == approval_grant_digest
                && existing_decision.proposal_fingerprint == proposal_fingerprint
            {
                transaction.commit().map_err(map_sqlite)?;
                return Ok(ExecDecisionOutcome::Accepted(current));
            }
            transaction.commit().map_err(map_sqlite)?;
            return Ok(ExecDecisionOutcome::Conflict(current));
        }

        if let Some(matched) = matched
            && !rule_snapshot_exists(&transaction, matched).map_err(map_sqlite)?
        {
            transaction.commit().map_err(map_sqlite)?;
            return Ok(ExecDecisionOutcome::Conflict(current));
        }

        if is_exec_terminal(current.state) {
            transaction.commit().map_err(map_sqlite)?;
            return Ok(ExecDecisionOutcome::Terminal(current));
        }

        if is_exec_expired(current.expires_at, now) {
            let expired = force_transition(
                &transaction,
                &current,
                ExecProposalState::Expired,
                None,
                now,
            )?;
            transaction.commit().map_err(map_sqlite)?;
            return Ok(ExecDecisionOutcome::Expired(expired));
        }

        if current.state != ExecProposalState::PendingApproval {
            transaction.commit().map_err(map_sqlite)?;
            return Ok(ExecDecisionOutcome::Conflict(current));
        }

        if grant_digest_used(&transaction, approval_grant_digest)? {
            transaction.commit().map_err(map_sqlite)?;
            return Ok(ExecDecisionOutcome::Conflict(current));
        }

        let next = match decision {
            ExecDecision::ApproveOnce => ExecProposalState::Approved,
            ExecDecision::Reject => ExecProposalState::Rejected,
        };
        validate_exec_transition(current.state, next)
            .map_err(|_| ExecProposalStoreError::InvalidTransition)?;

        let grant_insert = transaction.execute(
            "INSERT INTO exec_approval_decisions (
                    proposal_id, decision, approval_grant_digest, proposal_fingerprint, decided_at
                 ) VALUES (?1, ?2, ?3, ?4, ?5)",
            params![
                proposal_id,
                decision_text(decision),
                approval_grant_digest,
                proposal_fingerprint,
                now.to_rfc3339(),
            ],
        );
        match grant_insert {
            Ok(_) => {}
            Err(error) if is_unique_constraint(&error) => {
                transaction.commit().map_err(map_sqlite)?;
                return Ok(ExecDecisionOutcome::Conflict(current));
            }
            Err(error) => return Err(map_sqlite(error)),
        }

        if let Some(matched) = matched {
            insert_rule_receipt(&transaction, proposal_id, matched, now)?;
        }

        let updated = force_transition(&transaction, &current, next, None, now)?;
        transaction.commit().map_err(map_sqlite)?;
        Ok(ExecDecisionOutcome::Accepted(updated))
    }

    pub fn claim_run(
        &self,
        proposal_id: &str,
        owner: &str,
        lease_ms: u64,
        now: DateTime<Utc>,
    ) -> Result<ExecRunClaimOutcome, ExecProposalStoreError> {
        self.begin_mutation()?;
        validate_owner(owner)?;
        validate_lease_ms(lease_ms)?;
        let mut connection = self.connection()?;
        let transaction = connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(map_sqlite)?;
        let current = load_by_id(&transaction, proposal_id)?
            .ok_or(ExecProposalStoreError::ReservationConflict)?;

        if is_exec_terminal(current.state) {
            transaction.commit().map_err(map_sqlite)?;
            return Ok(ExecRunClaimOutcome::Terminal(current));
        }
        if is_exec_expired(current.expires_at, now)
            && matches!(
                current.state,
                ExecProposalState::PendingApproval | ExecProposalState::Approved
            )
        {
            let expired = force_transition(
                &transaction,
                &current,
                ExecProposalState::Expired,
                None,
                now,
            )?;
            transaction.commit().map_err(map_sqlite)?;
            return Ok(ExecRunClaimOutcome::Expired(expired));
        }

        // One-shot C4 process authority: Running never renews or reassigns.
        // Same owner, other owner and expired lease all return Busy so a caller
        // cannot interpret Claimed as spawn authority while a process may live.
        if current.state == ExecProposalState::Running {
            transaction.commit().map_err(map_sqlite)?;
            return Ok(ExecRunClaimOutcome::Busy(current));
        }

        if current.state != ExecProposalState::Approved {
            transaction.commit().map_err(map_sqlite)?;
            return Ok(ExecRunClaimOutcome::Conflict(current));
        }

        let expires_at = now + Duration::milliseconds(i64::try_from(lease_ms).unwrap_or(0));
        transaction
            .execute(
                "UPDATE exec_proposals
                 SET state = 'running', run_owner = ?1, run_lease_expires_at = ?2,
                     started_at = COALESCE(started_at, ?3), updated_at = ?3
                 WHERE proposal_id = ?4 AND state = 'approved'",
                params![
                    owner,
                    expires_at.to_rfc3339(),
                    now.to_rfc3339(),
                    proposal_id
                ],
            )
            .map_err(map_sqlite)?;
        insert_transition(
            &transaction,
            proposal_id,
            ExecProposalState::Running,
            None,
            now,
        )?;
        let claimed =
            load_by_id(&transaction, proposal_id)?.ok_or(ExecProposalStoreError::Sqlite)?;
        transaction.commit().map_err(map_sqlite)?;
        Ok(ExecRunClaimOutcome::Claimed(claimed))
    }

    /// Complete a claimed run with a typed terminal receipt.
    ///
    /// Requires exact owner and a still-live run lease at `now`.
    /// `next` must be `Succeeded`, `Failed`, or post-spawn `Cancelled`.
    /// Cancellation flags must match the terminal state exactly:
    /// `Cancelled` requires `receipt.cancelled == true`; succeed/fail require
    /// `receipt.cancelled == false`.
    pub fn complete_run(
        &self,
        proposal_id: &str,
        owner: &str,
        next: ExecProposalState,
        receipt: &ExecTerminalReceipt,
        now: DateTime<Utc>,
    ) -> Result<StoredExecProposal, ExecProposalStoreError> {
        self.begin_mutation()?;
        validate_owner(owner)?;
        if !matches!(
            next,
            ExecProposalState::Succeeded | ExecProposalState::Failed | ExecProposalState::Cancelled
        ) {
            return Err(ExecProposalStoreError::InvalidResult);
        }
        // Exact cancellation-flag consistency with the requested terminal state.
        let expect_cancelled = next == ExecProposalState::Cancelled;
        if receipt.cancelled != expect_cancelled {
            return Err(ExecProposalStoreError::InvalidResult);
        }

        let mut connection = self.connection()?;
        let transaction = connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(map_sqlite)?;
        let current = load_by_id(&transaction, proposal_id)?
            .ok_or(ExecProposalStoreError::ReservationConflict)?;

        let result_json = serialize_receipt(receipt)?;

        if is_exec_terminal(current.state) {
            if current.state == next && current.result_json.as_deref() == Some(&result_json) {
                transaction.commit().map_err(map_sqlite)?;
                return Ok(current);
            }
            return Err(ExecProposalStoreError::InvalidResult);
        }

        let (run_owner, lease_expires_at) = load_run_lease(&transaction, proposal_id)?;
        if current.state != ExecProposalState::Running
            || run_owner.as_deref() != Some(owner)
            || !lease_is_live(lease_expires_at, now)
        {
            return Err(ExecProposalStoreError::ReservationConflict);
        }

        validate_exec_transition(current.state, next)
            .map_err(|_| ExecProposalStoreError::InvalidTransition)?;

        let duration_ms = i64::try_from(receipt.duration_ms)
            .map_err(|_| ExecProposalStoreError::InvalidResult)?;
        let captured_bytes = i64::try_from(receipt.captured_bytes)
            .map_err(|_| ExecProposalStoreError::InvalidResult)?;

        transaction
            .execute(
                "UPDATE exec_proposals
                 SET state = ?1, result_json = ?2, run_owner = NULL,
                     run_lease_expires_at = NULL, completed_at = ?3, updated_at = ?3,
                     exit_code = ?4, duration_ms = ?5, stdout_digest = ?6, stderr_digest = ?7,
                     captured_bytes = ?8, truncated = ?9, encoding_lossy = ?10,
                     cancelled = ?11, timed_out = ?12
                 WHERE proposal_id = ?13 AND run_owner = ?14",
                params![
                    state_text(next),
                    result_json,
                    now.to_rfc3339(),
                    receipt.exit_code,
                    duration_ms,
                    receipt.stdout_sha256,
                    receipt.stderr_sha256,
                    captured_bytes,
                    bool_int(receipt.truncated),
                    bool_int(receipt.encoding_lossy),
                    bool_int(receipt.cancelled),
                    bool_int(receipt.timed_out),
                    proposal_id,
                    owner,
                ],
            )
            .map_err(map_sqlite)?;
        insert_transition(&transaction, proposal_id, next, Some(&result_json), now)?;
        let terminal =
            load_by_id(&transaction, proposal_id)?.ok_or(ExecProposalStoreError::Sqlite)?;
        transaction.commit().map_err(map_sqlite)?;
        Ok(terminal)
    }

    /// Live cancellation while a run lease is held.
    ///
    /// Requires current state exactly `running`, exact `run_owner` match, and
    /// a still-live lease at `now`. Restart recovery of orphaned rows must use
    /// [`Self::recover_interrupted`] instead.
    pub fn interrupt_run(
        &self,
        proposal_id: &str,
        owner: &str,
        now: DateTime<Utc>,
    ) -> Result<StoredExecProposal, ExecProposalStoreError> {
        self.begin_mutation()?;
        validate_owner(owner)?;

        let mut connection = self.connection()?;
        let transaction = connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(map_sqlite)?;
        let current = load_by_id(&transaction, proposal_id)?
            .ok_or(ExecProposalStoreError::ReservationConflict)?;

        let (run_owner, lease_expires_at) = load_run_lease(&transaction, proposal_id)?;
        if current.state != ExecProposalState::Running
            || run_owner.as_deref() != Some(owner)
            || !lease_is_live(lease_expires_at, now)
        {
            return Err(ExecProposalStoreError::ReservationConflict);
        }

        validate_exec_transition(current.state, ExecProposalState::Interrupted)
            .map_err(|_| ExecProposalStoreError::InvalidTransition)?;

        let changed = transaction
            .execute(
                "UPDATE exec_proposals
                 SET state = 'interrupted', run_owner = NULL,
                     run_lease_expires_at = NULL, updated_at = ?1
                 WHERE proposal_id = ?2 AND state = 'running' AND run_owner = ?3",
                params![now.to_rfc3339(), proposal_id, owner],
            )
            .map_err(map_sqlite)?;
        if changed != 1 {
            return Err(ExecProposalStoreError::ReservationConflict);
        }

        insert_transition(
            &transaction,
            proposal_id,
            ExecProposalState::Interrupted,
            None,
            now,
        )?;
        let interrupted =
            load_by_id(&transaction, proposal_id)?.ok_or(ExecProposalStoreError::Sqlite)?;
        transaction.commit().map_err(map_sqlite)?;
        Ok(interrupted)
    }

    pub fn transition(
        &self,
        proposal_id: &str,
        next: ExecProposalState,
        now: DateTime<Utc>,
    ) -> Result<StoredExecProposal, ExecProposalStoreError> {
        self.begin_mutation()?;
        // Execution claim and apply terminals only via claim_run / complete_run.
        // Live interrupt of a running row only via interrupt_run.
        if matches!(
            next,
            ExecProposalState::Running
                | ExecProposalState::Succeeded
                | ExecProposalState::Failed
                | ExecProposalState::Approved
        ) {
            return Err(ExecProposalStoreError::InvalidTransition);
        }

        let mut connection = self.connection()?;
        let transaction = connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(map_sqlite)?;
        let current = load_by_id(&transaction, proposal_id)?
            .ok_or(ExecProposalStoreError::ReservationConflict)?;
        if current.state == next {
            transaction.commit().map_err(map_sqlite)?;
            return Ok(current);
        }
        if is_exec_terminal(current.state) {
            return Err(ExecProposalStoreError::InvalidTransition);
        }
        // Generic transition never mutates a Running row — no ownerless
        // Running -> Cancelled / Interrupted bypass of the run owner.
        if current.state == ExecProposalState::Running {
            return Err(ExecProposalStoreError::InvalidTransition);
        }
        validate_exec_transition(current.state, next)
            .map_err(|_| ExecProposalStoreError::InvalidTransition)?;

        let updated = force_transition(&transaction, &current, next, None, now)?;
        transaction.commit().map_err(map_sqlite)?;
        Ok(updated)
    }

    /// Startup recovery: mark every non-terminal proposal interrupted.
    ///
    /// Never replays authority. Terminal rows are left unchanged.
    pub fn recover_interrupted(
        &self,
        now: DateTime<Utc>,
    ) -> Result<Vec<StoredExecProposal>, ExecProposalStoreError> {
        self.begin_mutation()?;
        let mut connection = self.connection()?;
        let transaction = connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(map_sqlite)?;
        let ids = {
            let mut statement = transaction
                .prepare(
                    "SELECT proposal_id FROM exec_proposals
                     WHERE state IN ('pending_approval', 'approved', 'running')
                     ORDER BY created_at ASC, proposal_id ASC",
                )
                .map_err(map_sqlite)?;
            statement
                .query_map([], |row| row.get::<_, String>(0))
                .map_err(map_sqlite)?
                .collect::<Result<Vec<_>, _>>()
                .map_err(map_sqlite)?
        };

        let mut recovered = Vec::with_capacity(ids.len());
        for proposal_id in ids {
            let current =
                load_by_id(&transaction, &proposal_id)?.ok_or(ExecProposalStoreError::Sqlite)?;
            let updated = force_transition(
                &transaction,
                &current,
                ExecProposalState::Interrupted,
                None,
                now,
            )?;
            recovered.push(updated);
        }
        transaction.commit().map_err(map_sqlite)?;
        Ok(recovered)
    }

    pub fn history(
        &self,
        proposal_id: &str,
    ) -> Result<Vec<ExecProposalState>, ExecProposalStoreError> {
        let connection = self.connection()?;
        let _ = load_by_id(&connection, proposal_id)?
            .ok_or(ExecProposalStoreError::ReservationConflict)?;
        let mut statement = connection
            .prepare(
                "SELECT state FROM exec_proposal_transitions
                 WHERE proposal_id = ?1 ORDER BY id ASC",
            )
            .map_err(map_sqlite)?;
        let states = statement
            .query_map([proposal_id], |row| {
                let state: String = row.get(0)?;
                parse_state(&state, 0)
            })
            .map_err(map_sqlite)?
            .collect::<Result<Vec<_>, _>>()
            .map_err(map_sqlite)?;
        Ok(states)
    }

    pub fn list_for_conversation(
        &self,
        conversation_id: &str,
    ) -> Result<Vec<StoredExecProposal>, ExecProposalStoreError> {
        let connection = self.connection()?;
        let mut statement = connection
            .prepare(
                "SELECT proposal_id, reservation_key, proposal_fingerprint, project_id,
                        conversation_id, run_id, call_id, program, argv_digest, portable_cwd,
                        state, expires_at, result_json, started_at, completed_at, exit_code,
                        duration_ms, stdout_digest, stderr_digest, captured_bytes, truncated,
                        encoding_lossy, cancelled, timed_out, created_at, updated_at
                 FROM exec_proposals
                 WHERE conversation_id = ?1
                 ORDER BY created_at ASC, proposal_id ASC",
            )
            .map_err(map_sqlite)?;
        statement
            .query_map([conversation_id], stored_from_row)
            .map_err(map_sqlite)?
            .collect::<Result<Vec<_>, _>>()
            .map_err(map_sqlite)
    }

    /// Read-only enumeration of orphaned `running` rows for host observation.
    pub fn list_running(&self) -> Result<Vec<StoredExecProposal>, ExecProposalStoreError> {
        let connection = self.connection()?;
        let mut statement = connection
            .prepare(
                "SELECT proposal_id, reservation_key, proposal_fingerprint, project_id,
                        conversation_id, run_id, call_id, program, argv_digest, portable_cwd,
                        state, expires_at, result_json, started_at, completed_at, exit_code,
                        duration_ms, stdout_digest, stderr_digest, captured_bytes, truncated,
                        encoding_lossy, cancelled, timed_out, created_at, updated_at
                 FROM exec_proposals
                 WHERE state = 'running'
                 ORDER BY created_at ASC, proposal_id ASC",
            )
            .map_err(map_sqlite)?;
        statement
            .query_map([], stored_from_row)
            .map_err(map_sqlite)?
            .collect::<Result<Vec<_>, _>>()
            .map_err(map_sqlite)
    }

    fn connection(&self) -> Result<MutexGuard<'_, Connection>, ExecProposalStoreError> {
        self.connection
            .lock()
            .map_err(|_| ExecProposalStoreError::LockPoisoned)
    }
}

#[derive(Debug)]
struct StoredDecision {
    decision: ExecDecision,
    approval_grant_digest: String,
    proposal_fingerprint: String,
}

const SELECT_COLUMNS: &str = "proposal_id, reservation_key, proposal_fingerprint, project_id,
                        conversation_id, run_id, call_id, program, argv_digest, portable_cwd,
                        state, expires_at, result_json, started_at, completed_at, exit_code,
                        duration_ms, stdout_digest, stderr_digest, captured_bytes, truncated,
                        encoding_lossy, cancelled, timed_out, created_at, updated_at";

fn serialize_receipt(receipt: &ExecTerminalReceipt) -> Result<String, ExecProposalStoreError> {
    validate_canonical_digest(&receipt.stdout_sha256)?;
    validate_canonical_digest(&receipt.stderr_sha256)?;
    let json = serde_json::to_string(receipt).map_err(|_| ExecProposalStoreError::InvalidResult)?;
    if json.is_empty() || json.len() > EXEC_RESULT_JSON_BYTE_CAP {
        return Err(ExecProposalStoreError::InvalidResult);
    }
    let _: ExecTerminalReceipt =
        serde_json::from_str(&json).map_err(|_| ExecProposalStoreError::InvalidResult)?;
    Ok(json)
}

fn validate_reserve_input(input: &ExecProposalReserveInput) -> Result<(), ExecProposalStoreError> {
    if Uuid::parse_str(&input.proposal_id).is_err() || input.proposal_id.len() != 36 {
        return Err(ExecProposalStoreError::InvalidResult);
    }
    if input.reservation_key.is_empty() || input.reservation_key.len() > 512 {
        return Err(ExecProposalStoreError::InvalidResult);
    }
    if input.call_id.is_empty() || input.call_id.len() > 128 {
        return Err(ExecProposalStoreError::InvalidResult);
    }
    validate_program_label(&input.program)?;
    validate_canonical_digest(&input.proposal_fingerprint)?;
    validate_canonical_digest(&input.argv_digest)?;
    if let Some(cwd) = input.portable_cwd.as_deref() {
        validate_portable_cwd(cwd)?;
    }
    Ok(())
}

fn validate_program_label(program: &str) -> Result<(), ExecProposalStoreError> {
    if program.is_empty() || program.len() > 32 {
        return Err(ExecProposalStoreError::InvalidResult);
    }
    let mut chars = program.chars();
    let Some(first) = chars.next() else {
        return Err(ExecProposalStoreError::InvalidResult);
    };
    if !first.is_ascii_lowercase() {
        return Err(ExecProposalStoreError::InvalidResult);
    }
    if !chars.all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_' || c == '-') {
        return Err(ExecProposalStoreError::InvalidResult);
    }
    Ok(())
}

fn validate_portable_cwd(cwd: &str) -> Result<(), ExecProposalStoreError> {
    if cwd.is_empty() || cwd.len() > 4_096 {
        return Err(ExecProposalStoreError::InvalidResult);
    }
    // Portable project-relative only: no drive, UNC, backslash, or absolute.
    if cwd.contains('\\')
        || cwd.contains(':')
        || cwd.starts_with('/')
        || cwd.contains('\0')
        || cwd.contains("..")
    {
        return Err(ExecProposalStoreError::InvalidResult);
    }
    Ok(())
}

fn validate_canonical_digest(digest: &str) -> Result<(), ExecProposalStoreError> {
    let Some(hex) = digest.strip_prefix("sha256:") else {
        return Err(ExecProposalStoreError::InvalidResult);
    };
    if hex.len() != 64
        || !hex
            .as_bytes()
            .iter()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(byte))
    {
        return Err(ExecProposalStoreError::InvalidResult);
    }
    Ok(())
}

fn validate_lease_ms(lease_ms: u64) -> Result<(), ExecProposalStoreError> {
    if lease_ms == 0 || lease_ms > MAX_RUN_LEASE_MS {
        return Err(ExecProposalStoreError::InvalidResult);
    }
    Ok(())
}

fn validate_owner(owner: &str) -> Result<(), ExecProposalStoreError> {
    if owner.is_empty()
        || owner.len() > 128
        || !owner
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.' | b':'))
    {
        return Err(ExecProposalStoreError::ReservationConflict);
    }
    Ok(())
}

fn lease_is_live(lease_expires_at: Option<DateTime<Utc>>, now: DateTime<Utc>) -> bool {
    lease_expires_at.is_some_and(|expires_at| expires_at > now)
}

fn is_unique_constraint(error: &rusqlite::Error) -> bool {
    matches!(
        error,
        rusqlite::Error::SqliteFailure(error, _)
            if error.extended_code == ffi::SQLITE_CONSTRAINT_UNIQUE
                || error.extended_code == ffi::SQLITE_CONSTRAINT_PRIMARYKEY
    )
}

fn ensure_context_exists(
    connection: &Connection,
    project_id: &str,
    conversation_id: &str,
    run_id: &str,
) -> Result<(), ExecProposalStoreError> {
    let exists = connection
        .query_row(
            "SELECT EXISTS(
                SELECT 1
                FROM runs
                JOIN conversations ON conversations.id = runs.conversation_id
                JOIN projects ON projects.id = conversations.project_id
                WHERE runs.id = ?1
                  AND conversations.id = ?2
                  AND projects.id = ?3
             )",
            params![run_id, conversation_id, project_id],
            |row| row.get::<_, bool>(0),
        )
        .map_err(map_sqlite)?;
    if exists {
        Ok(())
    } else {
        Err(ExecProposalStoreError::MissingContext)
    }
}

fn force_transition(
    connection: &Connection,
    current: &StoredExecProposal,
    next: ExecProposalState,
    result_json: Option<&str>,
    now: DateTime<Utc>,
) -> Result<StoredExecProposal, ExecProposalStoreError> {
    validate_exec_transition(current.state, next)
        .map_err(|_| ExecProposalStoreError::InvalidTransition)?;
    connection
        .execute(
            "UPDATE exec_proposals
             SET state = ?1, result_json = COALESCE(?2, result_json),
                 run_owner = NULL, run_lease_expires_at = NULL, updated_at = ?3
             WHERE proposal_id = ?4",
            params![
                state_text(next),
                result_json,
                now.to_rfc3339(),
                current.proposal_id,
            ],
        )
        .map_err(map_sqlite)?;
    insert_transition(connection, &current.proposal_id, next, result_json, now)?;
    load_by_id(connection, &current.proposal_id)?.ok_or(ExecProposalStoreError::Sqlite)
}

fn insert_transition(
    connection: &Connection,
    proposal_id: &str,
    state: ExecProposalState,
    result_json: Option<&str>,
    now: DateTime<Utc>,
) -> Result<(), ExecProposalStoreError> {
    connection
        .execute(
            "INSERT INTO exec_proposal_transitions (
                proposal_id, state, result_json, created_at
             ) VALUES (?1, ?2, ?3, ?4)",
            params![
                proposal_id,
                state_text(state),
                result_json,
                now.to_rfc3339(),
            ],
        )
        .map_err(map_sqlite)?;
    Ok(())
}

fn load_by_id(
    connection: &Connection,
    proposal_id: &str,
) -> Result<Option<StoredExecProposal>, ExecProposalStoreError> {
    connection
        .query_row(
            &format!("SELECT {SELECT_COLUMNS} FROM exec_proposals WHERE proposal_id = ?1"),
            [proposal_id],
            stored_from_row,
        )
        .optional()
        .map_err(map_sqlite)
}

fn load_by_reservation_key(
    connection: &Connection,
    reservation_key: &str,
) -> Result<Option<StoredExecProposal>, ExecProposalStoreError> {
    connection
        .query_row(
            &format!("SELECT {SELECT_COLUMNS} FROM exec_proposals WHERE reservation_key = ?1"),
            [reservation_key],
            stored_from_row,
        )
        .optional()
        .map_err(map_sqlite)
}

fn load_by_identity(
    connection: &Connection,
    project_id: &str,
    conversation_id: &str,
    run_id: &str,
    call_id: &str,
) -> Result<Option<StoredExecProposal>, ExecProposalStoreError> {
    connection
        .query_row(
            &format!(
                "SELECT {SELECT_COLUMNS} FROM exec_proposals
                 WHERE project_id = ?1 AND conversation_id = ?2
                   AND run_id = ?3 AND call_id = ?4"
            ),
            params![project_id, conversation_id, run_id, call_id],
            stored_from_row,
        )
        .optional()
        .map_err(map_sqlite)
}

fn load_active_on_run(
    connection: &Connection,
    run_id: &str,
) -> Result<Option<StoredExecProposal>, ExecProposalStoreError> {
    connection
        .query_row(
            &format!(
                "SELECT {SELECT_COLUMNS} FROM exec_proposals
                 WHERE run_id = ?1
                   AND state IN ('pending_approval', 'approved', 'running')
                 LIMIT 1"
            ),
            [run_id],
            stored_from_row,
        )
        .optional()
        .map_err(map_sqlite)
}

fn load_decision(
    connection: &Connection,
    proposal_id: &str,
) -> Result<Option<StoredDecision>, ExecProposalStoreError> {
    connection
        .query_row(
            "SELECT decision, approval_grant_digest, proposal_fingerprint
             FROM exec_approval_decisions WHERE proposal_id = ?1",
            [proposal_id],
            |row| {
                let decision: String = row.get(0)?;
                Ok(StoredDecision {
                    decision: parse_decision(&decision, 0)?,
                    approval_grant_digest: row.get(1)?,
                    proposal_fingerprint: row.get(2)?,
                })
            },
        )
        .optional()
        .map_err(map_sqlite)
}

fn load_rule_receipt(
    connection: &Connection,
    proposal_id: &str,
) -> Result<Option<StoredPermissionRuleReceipt>, ExecProposalStoreError> {
    connection
        .query_row(
            "SELECT proposal_id, decision, actor, rule_id, rule_scope, rule_project_id,
                    rule_prefix, rule_pattern, rule_action, rule_content_digest,
                    match_subject, decided_at
             FROM exec_permission_rule_receipts WHERE proposal_id = ?1",
            [proposal_id],
            permission_rule_receipt_from_row,
        )
        .optional()
        .map_err(map_sqlite)
}

fn insert_rule_receipt(
    connection: &Connection,
    proposal_id: &str,
    matched: &PermissionRuleMatchSnapshot,
    now: DateTime<Utc>,
) -> Result<(), ExecProposalStoreError> {
    connection
        .execute(
            "INSERT INTO exec_permission_rule_receipts (
                proposal_id, decision, actor, rule_id, rule_scope, rule_project_id,
                rule_prefix, rule_pattern, rule_action, rule_content_digest,
                match_subject, decided_at
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)",
            params![
                proposal_id,
                rule_receipt_decision(matched.rule.action).as_str(),
                PermissionRuleReceiptActor::RuleEngine.as_str(),
                matched.rule.rule_id,
                matched.rule.scope.as_str(),
                matched.rule.project_id,
                matched.rule.prefix.as_str(),
                matched.rule.pattern,
                matched.rule.action.as_str(),
                matched.rule.content_digest,
                matched.match_subject,
                now.to_rfc3339(),
            ],
        )
        .map_err(map_sqlite)?;
    Ok(())
}

fn rule_receipt_decision(action: PermissionRuleAction) -> PermissionRuleReceiptDecision {
    match action {
        PermissionRuleAction::Allow => PermissionRuleReceiptDecision::ApproveByRule,
        PermissionRuleAction::Deny => PermissionRuleReceiptDecision::RejectByRule,
    }
}

fn grant_digest_used(
    connection: &Connection,
    digest: &str,
) -> Result<bool, ExecProposalStoreError> {
    connection
        .query_row(
            "SELECT EXISTS(
                SELECT 1 FROM exec_approval_decisions WHERE approval_grant_digest = ?1
             )",
            [digest],
            |row| row.get::<_, bool>(0),
        )
        .map_err(map_sqlite)
}

fn load_run_lease(
    connection: &Connection,
    proposal_id: &str,
) -> Result<(Option<String>, Option<DateTime<Utc>>), ExecProposalStoreError> {
    connection
        .query_row(
            "SELECT run_owner, run_lease_expires_at
             FROM exec_proposals WHERE proposal_id = ?1",
            [proposal_id],
            |row| {
                let owner: Option<String> = row.get(0)?;
                let lease: Option<String> = row.get(1)?;
                let lease = lease.map(|value| parse_timestamp(&value, 1)).transpose()?;
                Ok((owner, lease))
            },
        )
        .map_err(map_sqlite)
}

fn stored_from_row(row: &Row<'_>) -> rusqlite::Result<StoredExecProposal> {
    let run_id: String = row.get(5)?;
    let state: String = row.get(10)?;
    let expires_at: String = row.get(11)?;
    let started_at: Option<String> = row.get(13)?;
    let completed_at: Option<String> = row.get(14)?;
    let duration_ms: Option<i64> = row.get(16)?;
    let captured_bytes: Option<i64> = row.get(19)?;
    let truncated: Option<i64> = row.get(20)?;
    let encoding_lossy: Option<i64> = row.get(21)?;
    let cancelled: Option<i64> = row.get(22)?;
    let timed_out: Option<i64> = row.get(23)?;
    let created_at: String = row.get(24)?;
    let updated_at: String = row.get(25)?;
    Ok(StoredExecProposal {
        proposal_id: row.get(0)?,
        reservation_key: row.get(1)?,
        proposal_fingerprint: row.get(2)?,
        project_id: row.get(3)?,
        conversation_id: row.get(4)?,
        run_id: RunId(parse_uuid(&run_id, 5)?),
        call_id: row.get(6)?,
        program: row.get(7)?,
        argv_digest: row.get(8)?,
        portable_cwd: row.get(9)?,
        state: parse_state(&state, 10)?,
        expires_at: parse_timestamp(&expires_at, 11)?,
        result_json: row.get(12)?,
        started_at: started_at
            .map(|value| parse_timestamp(&value, 13))
            .transpose()?,
        completed_at: completed_at
            .map(|value| parse_timestamp(&value, 14))
            .transpose()?,
        exit_code: row.get(15)?,
        duration_ms: duration_ms
            .map(|v| {
                u64::try_from(v).map_err(|error| {
                    rusqlite::Error::FromSqlConversionFailure(16, Type::Integer, Box::new(error))
                })
            })
            .transpose()?,
        stdout_digest: row.get(17)?,
        stderr_digest: row.get(18)?,
        captured_bytes: captured_bytes
            .map(|v| {
                u64::try_from(v).map_err(|error| {
                    rusqlite::Error::FromSqlConversionFailure(19, Type::Integer, Box::new(error))
                })
            })
            .transpose()?,
        truncated: truncated.map(|v| v != 0),
        encoding_lossy: encoding_lossy.map(|v| v != 0),
        cancelled: cancelled.map(|v| v != 0),
        timed_out: timed_out.map(|v| v != 0),
        created_at: parse_timestamp(&created_at, 24)?,
        updated_at: parse_timestamp(&updated_at, 25)?,
    })
}

fn parse_uuid(value: &str, index: usize) -> rusqlite::Result<Uuid> {
    Uuid::parse_str(value).map_err(|error| {
        rusqlite::Error::FromSqlConversionFailure(index, Type::Text, Box::new(error))
    })
}

fn parse_timestamp(value: &str, index: usize) -> rusqlite::Result<DateTime<Utc>> {
    DateTime::parse_from_rfc3339(value)
        .map(|timestamp| timestamp.with_timezone(&Utc))
        .map_err(|error| {
            rusqlite::Error::FromSqlConversionFailure(index, Type::Text, Box::new(error))
        })
}

fn parse_state(value: &str, index: usize) -> rusqlite::Result<ExecProposalState> {
    match value {
        "pending_approval" => Ok(ExecProposalState::PendingApproval),
        "approved" => Ok(ExecProposalState::Approved),
        "running" => Ok(ExecProposalState::Running),
        "succeeded" => Ok(ExecProposalState::Succeeded),
        "failed" => Ok(ExecProposalState::Failed),
        "rejected" => Ok(ExecProposalState::Rejected),
        "expired" => Ok(ExecProposalState::Expired),
        "cancelled" => Ok(ExecProposalState::Cancelled),
        "interrupted" => Ok(ExecProposalState::Interrupted),
        _ => Err(rusqlite::Error::FromSqlConversionFailure(
            index,
            Type::Text,
            Box::new(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "invalid exec proposal state",
            )),
        )),
    }
}

fn parse_decision(value: &str, index: usize) -> rusqlite::Result<ExecDecision> {
    match value {
        "approve_once" => Ok(ExecDecision::ApproveOnce),
        "reject" => Ok(ExecDecision::Reject),
        _ => Err(rusqlite::Error::FromSqlConversionFailure(
            index,
            Type::Text,
            Box::new(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "invalid exec decision",
            )),
        )),
    }
}

fn state_text(state: ExecProposalState) -> &'static str {
    match state {
        ExecProposalState::PendingApproval => "pending_approval",
        ExecProposalState::Approved => "approved",
        ExecProposalState::Running => "running",
        ExecProposalState::Succeeded => "succeeded",
        ExecProposalState::Failed => "failed",
        ExecProposalState::Rejected => "rejected",
        ExecProposalState::Expired => "expired",
        ExecProposalState::Cancelled => "cancelled",
        ExecProposalState::Interrupted => "interrupted",
    }
}

fn decision_text(decision: ExecDecision) -> &'static str {
    match decision {
        ExecDecision::ApproveOnce => "approve_once",
        ExecDecision::Reject => "reject",
    }
}

fn bool_int(value: bool) -> i64 {
    if value { 1 } else { 0 }
}

fn map_sqlite(_error: rusqlite::Error) -> ExecProposalStoreError {
    ExecProposalStoreError::Sqlite
}
