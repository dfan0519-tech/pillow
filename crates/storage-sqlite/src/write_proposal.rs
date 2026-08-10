//! Durable write-proposal ledger (metadata and digests only).
//!
//! Storage owns proposal TTL, validates all bounded digests/byte counts before
//! SQL, serializes typed apply receipts, and provides owner-independent
//! restart reconciliation for `applying` rows.

use std::path::Path;
#[cfg(any(test, feature = "test-support"))]
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
#[cfg(any(test, feature = "test-support"))]
use std::sync::{Arc, Condvar};
use std::sync::{Mutex, MutexGuard};
#[cfg(any(test, feature = "test-support"))]
use std::time::Duration as StdDuration;

use chrono::{DateTime, Duration, Utc};
use keeper_agent_protocol::{RunId, WRITE_CONTENT_BYTE_CAP, WriteMode};
use keeper_policy::{
    WRITE_PROPOSAL_TTL_SECS, WriteDecision, WriteProposalState, is_expired, is_terminal,
    validate_write_transition,
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

/// Maximum apply-lease duration the repository will accept (milliseconds).
pub const MAX_APPLY_LEASE_MS: u64 = 300_000;

/// Upper bound for stored terminal `result_json` text.
pub const RESULT_JSON_BYTE_CAP: usize = 8_192;

/// Immutable identity + digest record for a governed write proposal.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StoredWriteProposal {
    pub proposal_id: String,
    pub reservation_key: String,
    pub proposal_fingerprint: String,
    pub project_id: String,
    pub conversation_id: String,
    pub run_id: RunId,
    pub call_id: String,
    pub path: String,
    pub mode: WriteMode,
    pub proposed_digest: String,
    pub proposed_bytes: u64,
    pub base_digest: Option<String>,
    pub base_bytes: u64,
    pub state: WriteProposalState,
    pub expires_at: DateTime<Utc>,
    pub result_json: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// Host-supplied reservation payload. Never includes proposed/base content.
/// TTL is owned by storage (`created_at` / `expires_at` are not caller inputs).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WriteProposalReserveInput {
    pub proposal_id: String,
    pub reservation_key: String,
    pub proposal_fingerprint: String,
    pub project_id: String,
    pub conversation_id: String,
    pub run_id: RunId,
    pub call_id: String,
    pub path: String,
    pub mode: WriteMode,
    pub proposed_digest: String,
    pub proposed_bytes: u64,
    pub base_digest: Option<String>,
    pub base_bytes: u64,
}

/// Fixed opaque C3 apply-time failure codes accepted in typed receipts.
///
/// Closed vocabulary (snake_case on the wire). Approval, provider, terminal,
/// Git, and free-form error text are intentionally excluded — only apply-time
/// codes needed by the frozen C3 design.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WriteApplyErrorCode {
    BaseChanged,
    PathRejected,
    WriteFailed,
}

/// Typed terminal apply receipt. Repository serializes this to `result_json`.
///
/// No content, base payload, native path, or debug fields are permitted.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "outcome", rename_all = "snake_case", deny_unknown_fields)]
pub enum WriteApplyReceipt {
    Success {
        path: String,
        mode: WriteMode,
        bytes_written: u64,
        content_digest: String,
        recovered: bool,
    },
    Failure {
        error_code: WriteApplyErrorCode,
        recovered: bool,
    },
}

impl WriteApplyReceipt {
    pub fn terminal_state(&self) -> WriteProposalState {
        match self {
            Self::Success { .. } => WriteProposalState::Succeeded,
            Self::Failure { .. } => WriteProposalState::Failed,
        }
    }

    pub fn recovered(&self) -> bool {
        match self {
            Self::Success { recovered, .. } | Self::Failure { recovered, .. } => *recovered,
        }
    }
}

/// Read-only target observation for restart reconciliation of an `applying` row.
///
/// The host classifies the on-disk target; the repository never reads content.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WriteApplyReconcileObservation {
    /// Target digest equals the proposal's proposed digest.
    MatchesProposed,
    /// Target still matches base (modify) or remains absent (create).
    StillBaseOrAbsent,
    /// Target is neither proposed nor base/absent.
    Neither,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WriteReservationOutcome {
    Reserved(StoredWriteProposal),
    Replay(StoredWriteProposal),
    Conflict(StoredWriteProposal),
    Busy(StoredWriteProposal),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WriteDecisionOutcome {
    Accepted(StoredWriteProposal),
    Terminal(StoredWriteProposal),
    Conflict(StoredWriteProposal),
    Expired(StoredWriteProposal),
}

enum WriteDecisionSource<'a> {
    Human(WriteDecision),
    Rule(&'a PermissionRuleMatchSnapshot),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WriteApplyClaimOutcome {
    Claimed(StoredWriteProposal),
    Busy(StoredWriteProposal),
    Terminal(StoredWriteProposal),
    Expired(StoredWriteProposal),
    Conflict(StoredWriteProposal),
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum WriteProposalStoreError {
    #[error("write proposal context does not exist in the durable project conversation run graph")]
    MissingContext,
    #[error("write proposal conversation ownership mismatch")]
    OwnershipMismatch,
    #[error("write proposal transition is not permitted")]
    InvalidTransition,
    #[error("write proposal result does not match the requested terminal state")]
    InvalidResult,
    #[error("write proposal reservation conflicts with durable state")]
    ReservationConflict,
    #[error("write proposal rule receipt is invalid")]
    InvalidRuleReceipt,
    #[error("SQLite write proposal store error")]
    Sqlite,
    #[error("write proposal store lock was poisoned")]
    LockPoisoned,
}

pub struct SqliteWriteProposalRepository {
    connection: Mutex<Connection>,
    /// When set, all mutating store methods fail closed without writing.
    #[cfg(any(test, feature = "test-support"))]
    fail_writes: AtomicBool,
    /// Counts mutating entry points (including forced failures) for tests.
    #[cfg(any(test, feature = "test-support"))]
    write_attempts: AtomicUsize,
    /// Proposal-scoped rendezvous immediately before a terminal transition.
    #[cfg(any(test, feature = "test-support"))]
    transition_barrier: Mutex<Option<(String, Arc<TransitionBarrierForTests>)>>,
    /// Sources that crossed the pre-transition barrier for deterministic tests.
    #[cfg(any(test, feature = "test-support"))]
    transition_barrier_trace: Mutex<Vec<(String, String)>>,
    /// Sources that actually entered the SQLite transition mutation.
    #[cfg(any(test, feature = "test-support"))]
    transition_attempt_trace: Mutex<Vec<(String, String)>>,
}

#[cfg(any(test, feature = "test-support"))]
struct TransitionBarrierForTests {
    participants: usize,
    preferred_source: Option<String>,
    timeout: StdDuration,
    state: Mutex<TransitionBarrierStateForTests>,
    cv: Condvar,
}

#[cfg(any(test, feature = "test-support"))]
struct TransitionBarrierStateForTests {
    arrivals: usize,
    released: bool,
    preferred_completed: bool,
}

#[cfg(any(test, feature = "test-support"))]
impl TransitionBarrierForTests {
    fn new(participants: usize, preferred_source: Option<&str>, timeout: StdDuration) -> Self {
        assert!(participants > 0, "transition barrier needs participants");
        Self {
            participants,
            preferred_source: preferred_source.map(str::to_owned),
            timeout,
            state: Mutex::new(TransitionBarrierStateForTests {
                arrivals: 0,
                released: false,
                preferred_completed: false,
            }),
            cv: Condvar::new(),
        }
    }

    fn wait(&self, proposal_id: &str, source: &str) {
        let mut state = self.state.lock().expect("transition barrier state");
        if state.released {
            return;
        }
        state.arrivals += 1;
        if state.arrivals >= self.participants {
            state.released = true;
            self.cv.notify_all();
        } else {
            let (waited, _) = self
                .cv
                .wait_timeout_while(state, self.timeout, |state| !state.released)
                .expect("transition barrier arrival wait");
            state = waited;
        }
        assert!(
            state.released,
            "transition barrier timed out for proposal {proposal_id} source {source}: {}/{} arrivals",
            state.arrivals, self.participants
        );

        if self
            .preferred_source
            .as_deref()
            .is_some_and(|preferred| preferred != source)
        {
            let (waited, _) = self
                .cv
                .wait_timeout_while(state, self.timeout, |state| !state.preferred_completed)
                .expect("preferred transition completion wait");
            state = waited;
            assert!(
                state.preferred_completed,
                "preferred transition source did not complete proposal {proposal_id} before {source}"
            );
        }
    }

    fn mark_complete(&self, source: &str) {
        if self.preferred_source.as_deref() != Some(source) {
            return;
        }
        if let Ok(mut state) = self.state.lock() {
            state.preferred_completed = true;
            self.cv.notify_all();
        }
    }
}

impl SqliteWriteProposalRepository {
    pub fn open(path: impl AsRef<Path>) -> Result<Self, WriteProposalStoreError> {
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
            #[cfg(any(test, feature = "test-support"))]
            transition_barrier: Mutex::new(None),
            #[cfg(any(test, feature = "test-support"))]
            transition_barrier_trace: Mutex::new(Vec::new()),
            #[cfg(any(test, feature = "test-support"))]
            transition_attempt_trace: Mutex::new(Vec::new()),
        })
    }

    /// Test-only: force subsequent mutating store calls to fail without writing.
    #[cfg(any(test, feature = "test-support"))]
    pub fn set_fail_writes_for_tests(&self, fail: bool) {
        self.fail_writes.store(fail, Ordering::SeqCst);
    }

    /// Test-only: number of mutating store entry attempts (including forced fails).
    #[cfg(any(test, feature = "test-support"))]
    pub fn write_attempt_count_for_tests(&self) -> usize {
        self.write_attempts.load(Ordering::SeqCst)
    }

    /// Test-only: install a proposal-scoped barrier before terminal transition.
    #[cfg(any(test, feature = "test-support"))]
    pub fn set_transition_barrier_for_tests(
        &self,
        proposal_id: impl Into<String>,
        participants: usize,
        preferred_source: Option<&str>,
        timeout: StdDuration,
    ) {
        if let Ok(mut guard) = self.transition_barrier.lock() {
            *guard = Some((
                proposal_id.into(),
                Arc::new(TransitionBarrierForTests::new(
                    participants,
                    preferred_source,
                    timeout,
                )),
            ));
        }
    }

    /// Test-only: clear the proposal-scoped terminal-transition barrier.
    #[cfg(any(test, feature = "test-support"))]
    pub fn clear_transition_barrier_for_tests(&self) {
        if let Ok(mut guard) = self.transition_barrier.lock() {
            *guard = None;
        }
    }

    /// Test-only: clear barrier arrivals and actual transition-attempt traces.
    #[cfg(any(test, feature = "test-support"))]
    pub fn clear_transition_traces_for_tests(&self) {
        if let Ok(mut trace) = self.transition_barrier_trace.lock() {
            trace.clear();
        }
        if let Ok(mut trace) = self.transition_attempt_trace.lock() {
            trace.clear();
        }
    }

    /// Test-only: snapshot proposal/source pairs that crossed the barrier.
    #[cfg(any(test, feature = "test-support"))]
    pub fn transition_barrier_trace_for_tests(&self) -> Vec<(String, String)> {
        self.transition_barrier_trace
            .lock()
            .map(|trace| trace.clone())
            .unwrap_or_default()
    }

    /// Test-only: snapshot proposal/source pairs that attempted SQLite transition.
    #[cfg(any(test, feature = "test-support"))]
    pub fn transition_attempt_trace_for_tests(&self) -> Vec<(String, String)> {
        self.transition_attempt_trace
            .lock()
            .map(|trace| trace.clone())
            .unwrap_or_default()
    }

    /// Test-only: record a source after its expiry decision and rendezvous with
    /// any configured peer before either source may arbitrate the transition.
    #[cfg(any(test, feature = "test-support"))]
    pub fn wait_at_transition_barrier_for_tests(&self, proposal_id: &str, source: &str) {
        let barrier = self
            .transition_barrier
            .lock()
            .ok()
            .and_then(|guard| match guard.as_ref() {
                Some((target, barrier)) if target == proposal_id => Some(Arc::clone(barrier)),
                _ => None,
            });
        let Some(barrier) = barrier else {
            return;
        };

        if let Ok(mut trace) = self.transition_barrier_trace.lock() {
            trace.push((proposal_id.to_owned(), source.to_owned()));
        }
        eprintln!("STORAGE_BARRIER_REACHED proposal_id={proposal_id} source={source}");
        barrier.wait(proposal_id, source);
    }

    /// Test-only: label the exact source that enters `transition`.
    #[cfg(any(test, feature = "test-support"))]
    pub fn transition_with_source_for_tests(
        &self,
        proposal_id: &str,
        next: WriteProposalState,
        now: DateTime<Utc>,
        source: &str,
    ) -> Result<StoredWriteProposal, WriteProposalStoreError> {
        let barrier = self
            .transition_barrier
            .lock()
            .ok()
            .and_then(|guard| match guard.as_ref() {
                Some((target, barrier)) if target == proposal_id => Some(Arc::clone(barrier)),
                _ => None,
            });
        if barrier.is_some() {
            if let Ok(mut trace) = self.transition_attempt_trace.lock() {
                trace.push((proposal_id.to_owned(), source.to_owned()));
            }
            eprintln!("STORAGE_TRANSITION_ATTEMPT proposal_id={proposal_id} source={source}");
        }
        self.transition(proposal_id, next, now)
    }

    /// Test-only: release a source held behind a preferred transition after the
    /// preferred caller has finished its live-entry publication/removal work.
    #[cfg(any(test, feature = "test-support"))]
    pub fn complete_transition_source_for_tests(&self, proposal_id: &str, source: &str) {
        let barrier = self
            .transition_barrier
            .lock()
            .ok()
            .and_then(|guard| match guard.as_ref() {
                Some((target, barrier)) if target == proposal_id => Some(Arc::clone(barrier)),
                _ => None,
            });
        if let Some(barrier) = barrier {
            barrier.mark_complete(source);
        }
    }

    #[cfg(any(test, feature = "test-support"))]
    fn begin_mutation(&self) -> Result<(), WriteProposalStoreError> {
        self.write_attempts.fetch_add(1, Ordering::SeqCst);
        if self.fail_writes.load(Ordering::SeqCst) {
            return Err(WriteProposalStoreError::Sqlite);
        }
        Ok(())
    }

    #[cfg(not(any(test, feature = "test-support")))]
    fn begin_mutation(&self) -> Result<(), WriteProposalStoreError> {
        Ok(())
    }

    /// Reserve with storage-owned TTL at wall-clock now.
    pub fn reserve(
        &self,
        input: &WriteProposalReserveInput,
    ) -> Result<WriteReservationOutcome, WriteProposalStoreError> {
        self.reserve_at(input, Utc::now())
    }

    /// Deterministic reserve seam: `created_at = now`,
    /// `expires_at = now + WRITE_PROPOSAL_TTL_SECS` exactly.
    pub fn reserve_at(
        &self,
        input: &WriteProposalReserveInput,
        now: DateTime<Utc>,
    ) -> Result<WriteReservationOutcome, WriteProposalStoreError> {
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
                Ok(WriteReservationOutcome::Replay(existing))
            } else {
                Ok(WriteReservationOutcome::Conflict(existing))
            };
        }

        // Alias collisions on proposal_id / reservation_key with foreign identity.
        if let Some(existing) = load_by_id(&transaction, &input.proposal_id)? {
            transaction.commit().map_err(map_sqlite)?;
            return Ok(WriteReservationOutcome::Conflict(existing));
        }
        if let Some(existing) = load_by_reservation_key(&transaction, &input.reservation_key)? {
            transaction.commit().map_err(map_sqlite)?;
            return Ok(WriteReservationOutcome::Conflict(existing));
        }

        if let Some(active) = load_active_on_run(&transaction, &input.run_id.0.to_string())? {
            transaction.commit().map_err(map_sqlite)?;
            return Ok(WriteReservationOutcome::Busy(active));
        }

        let expires_at = now + Duration::seconds(WRITE_PROPOSAL_TTL_SECS);
        let insert = transaction.execute(
            "INSERT INTO write_proposals (
                    proposal_id, reservation_key, proposal_fingerprint, project_id,
                    conversation_id, run_id, call_id, path, mode, proposed_digest,
                    proposed_bytes, base_digest, base_bytes, state, expires_at,
                    result_json, apply_owner, apply_lease_expires_at, created_at, updated_at
                 ) VALUES (
                    ?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10,
                    ?11, ?12, ?13, 'pending_approval', ?14,
                    NULL, NULL, NULL, ?15, ?15
                 )",
            params![
                input.proposal_id,
                input.reservation_key,
                input.proposal_fingerprint,
                input.project_id,
                input.conversation_id,
                input.run_id.0.to_string(),
                input.call_id,
                input.path,
                mode_text(input.mode),
                input.proposed_digest,
                i64::try_from(input.proposed_bytes)
                    .map_err(|_| WriteProposalStoreError::InvalidResult)?,
                input.base_digest,
                i64::try_from(input.base_bytes)
                    .map_err(|_| WriteProposalStoreError::InvalidResult)?,
                expires_at.to_rfc3339(),
                now.to_rfc3339(),
            ],
        );
        match insert {
            Ok(_) => {}
            Err(error) if is_unique_constraint(&error) => {
                // Opaque conflict: never surface a raw SQLite failure for aliases.
                if let Some(existing) = load_by_id(&transaction, &input.proposal_id)? {
                    transaction.commit().map_err(map_sqlite)?;
                    return Ok(WriteReservationOutcome::Conflict(existing));
                }
                if let Some(existing) =
                    load_by_reservation_key(&transaction, &input.reservation_key)?
                {
                    transaction.commit().map_err(map_sqlite)?;
                    return Ok(WriteReservationOutcome::Conflict(existing));
                }
                if let Some(existing) = load_by_identity(
                    &transaction,
                    &input.project_id,
                    &input.conversation_id,
                    &input.run_id.0.to_string(),
                    &input.call_id,
                )? {
                    transaction.commit().map_err(map_sqlite)?;
                    return Ok(WriteReservationOutcome::Conflict(existing));
                }
                return Err(WriteProposalStoreError::ReservationConflict);
            }
            Err(error) => return Err(map_sqlite(error)),
        }
        insert_transition(
            &transaction,
            &input.proposal_id,
            WriteProposalState::PendingApproval,
            None,
            now,
        )?;
        let stored =
            load_by_id(&transaction, &input.proposal_id)?.ok_or(WriteProposalStoreError::Sqlite)?;
        transaction.commit().map_err(map_sqlite)?;
        Ok(WriteReservationOutcome::Reserved(stored))
    }

    pub fn get(
        &self,
        proposal_id: &str,
    ) -> Result<Option<StoredWriteProposal>, WriteProposalStoreError> {
        let connection = self.connection()?;
        load_by_id(&connection, proposal_id)
    }

    pub fn decide(
        &self,
        proposal_id: &str,
        conversation_id: &str,
        decision: WriteDecision,
        approval_grant_digest: &str,
        proposal_fingerprint: &str,
        now: DateTime<Utc>,
    ) -> Result<WriteDecisionOutcome, WriteProposalStoreError> {
        self.decide_internal(
            proposal_id,
            conversation_id,
            WriteDecisionSource::Human(decision),
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
    ) -> Result<WriteDecisionOutcome, WriteProposalStoreError> {
        validate_rule_match_snapshot(matched)
            .map_err(|_| WriteProposalStoreError::InvalidRuleReceipt)?;
        self.decide_internal(
            proposal_id,
            conversation_id,
            WriteDecisionSource::Rule(matched),
            approval_grant_digest,
            proposal_fingerprint,
            now,
        )
    }

    pub fn get_rule_receipt(
        &self,
        proposal_id: &str,
    ) -> Result<Option<StoredPermissionRuleReceipt>, WriteProposalStoreError> {
        let connection = self.connection()?;
        load_rule_receipt(&connection, proposal_id)
    }

    fn decide_internal(
        &self,
        proposal_id: &str,
        conversation_id: &str,
        source: WriteDecisionSource<'_>,
        approval_grant_digest: &str,
        proposal_fingerprint: &str,
        now: DateTime<Utc>,
    ) -> Result<WriteDecisionOutcome, WriteProposalStoreError> {
        let (decision, matched) = match source {
            WriteDecisionSource::Human(decision) => (decision, None),
            WriteDecisionSource::Rule(matched) => {
                let decision = match matched.rule.action {
                    PermissionRuleAction::Allow => WriteDecision::ApproveOnce,
                    PermissionRuleAction::Deny => WriteDecision::Reject,
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
            .ok_or(WriteProposalStoreError::ReservationConflict)?;
        if current.conversation_id != conversation_id {
            return Err(WriteProposalStoreError::OwnershipMismatch);
        }
        if let Some(matched) = matched {
            let expected_prefix = match current.mode {
                WriteMode::Create => PermissionRulePrefix::Write,
                WriteMode::Modify => PermissionRulePrefix::Edit,
            };
            if matched.rule.prefix != expected_prefix || matched.match_subject != current.path {
                return Err(WriteProposalStoreError::InvalidRuleReceipt);
            }
        }
        if current.proposal_fingerprint != proposal_fingerprint {
            transaction.commit().map_err(map_sqlite)?;
            return Ok(WriteDecisionOutcome::Conflict(current));
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
                return Ok(WriteDecisionOutcome::Conflict(current));
            }
            if is_terminal(current.state) {
                if existing_decision.decision == decision
                    && existing_decision.approval_grant_digest == approval_grant_digest
                    && existing_decision.proposal_fingerprint == proposal_fingerprint
                {
                    transaction.commit().map_err(map_sqlite)?;
                    return Ok(WriteDecisionOutcome::Terminal(current));
                }
                transaction.commit().map_err(map_sqlite)?;
                return Ok(WriteDecisionOutcome::Conflict(current));
            }
            if existing_decision.decision == decision
                && existing_decision.approval_grant_digest == approval_grant_digest
                && existing_decision.proposal_fingerprint == proposal_fingerprint
            {
                transaction.commit().map_err(map_sqlite)?;
                return Ok(WriteDecisionOutcome::Accepted(current));
            }
            transaction.commit().map_err(map_sqlite)?;
            return Ok(WriteDecisionOutcome::Conflict(current));
        }

        if let Some(matched) = matched
            && !rule_snapshot_exists(&transaction, matched).map_err(map_sqlite)?
        {
            transaction.commit().map_err(map_sqlite)?;
            return Ok(WriteDecisionOutcome::Conflict(current));
        }

        if is_terminal(current.state) {
            transaction.commit().map_err(map_sqlite)?;
            return Ok(WriteDecisionOutcome::Terminal(current));
        }

        if is_expired(current.expires_at, now) {
            let expired = force_transition(
                &transaction,
                &current,
                WriteProposalState::Expired,
                None,
                now,
            )?;
            transaction.commit().map_err(map_sqlite)?;
            return Ok(WriteDecisionOutcome::Expired(expired));
        }

        if current.state != WriteProposalState::PendingApproval {
            transaction.commit().map_err(map_sqlite)?;
            return Ok(WriteDecisionOutcome::Conflict(current));
        }

        // Grant digest must be globally unique — reject if already consumed.
        if grant_digest_used(&transaction, approval_grant_digest)? {
            transaction.commit().map_err(map_sqlite)?;
            return Ok(WriteDecisionOutcome::Conflict(current));
        }

        let next = match decision {
            WriteDecision::ApproveOnce => WriteProposalState::Approved,
            WriteDecision::Reject => WriteProposalState::Rejected,
        };
        validate_write_transition(current.state, next)
            .map_err(|_| WriteProposalStoreError::InvalidTransition)?;

        let grant_insert = transaction.execute(
            "INSERT INTO write_approval_decisions (
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
                return Ok(WriteDecisionOutcome::Conflict(current));
            }
            Err(error) => return Err(map_sqlite(error)),
        }

        if let Some(matched) = matched {
            insert_rule_receipt(&transaction, proposal_id, matched, now)?;
        }

        let updated = force_transition(&transaction, &current, next, None, now)?;
        transaction.commit().map_err(map_sqlite)?;
        Ok(WriteDecisionOutcome::Accepted(updated))
    }

    pub fn claim_apply(
        &self,
        proposal_id: &str,
        owner: &str,
        lease_ms: u64,
        now: DateTime<Utc>,
    ) -> Result<WriteApplyClaimOutcome, WriteProposalStoreError> {
        self.begin_mutation()?;
        validate_owner(owner)?;
        validate_lease_ms(lease_ms)?;
        let mut connection = self.connection()?;
        let transaction = connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(map_sqlite)?;
        let current = load_by_id(&transaction, proposal_id)?
            .ok_or(WriteProposalStoreError::ReservationConflict)?;

        if is_terminal(current.state) {
            transaction.commit().map_err(map_sqlite)?;
            return Ok(WriteApplyClaimOutcome::Terminal(current));
        }
        if is_expired(current.expires_at, now)
            && matches!(
                current.state,
                WriteProposalState::PendingApproval | WriteProposalState::Approved
            )
        {
            let expired = force_transition(
                &transaction,
                &current,
                WriteProposalState::Expired,
                None,
                now,
            )?;
            transaction.commit().map_err(map_sqlite)?;
            return Ok(WriteApplyClaimOutcome::Expired(expired));
        }

        let (apply_owner, lease_expires_at) = load_apply_lease(&transaction, proposal_id)?;
        let lease_is_live = lease_is_live(lease_expires_at, now);

        if current.state == WriteProposalState::Applying {
            if lease_is_live && apply_owner.as_deref() != Some(owner) {
                transaction.commit().map_err(map_sqlite)?;
                return Ok(WriteApplyClaimOutcome::Busy(current));
            }
            if lease_is_live && apply_owner.as_deref() == Some(owner) {
                transaction.commit().map_err(map_sqlite)?;
                return Ok(WriteApplyClaimOutcome::Claimed(current));
            }
            // Expired lease: re-claim for this owner without re-logging transition.
            let expires_at = now + Duration::milliseconds(i64::try_from(lease_ms).unwrap_or(0));
            transaction
                .execute(
                    "UPDATE write_proposals
                     SET apply_owner = ?1, apply_lease_expires_at = ?2, updated_at = ?3
                     WHERE proposal_id = ?4",
                    params![
                        owner,
                        expires_at.to_rfc3339(),
                        now.to_rfc3339(),
                        proposal_id
                    ],
                )
                .map_err(map_sqlite)?;
            let claimed =
                load_by_id(&transaction, proposal_id)?.ok_or(WriteProposalStoreError::Sqlite)?;
            transaction.commit().map_err(map_sqlite)?;
            return Ok(WriteApplyClaimOutcome::Claimed(claimed));
        }

        if current.state != WriteProposalState::Approved {
            transaction.commit().map_err(map_sqlite)?;
            return Ok(WriteApplyClaimOutcome::Conflict(current));
        }

        let expires_at = now + Duration::milliseconds(i64::try_from(lease_ms).unwrap_or(0));
        transaction
            .execute(
                "UPDATE write_proposals
                 SET state = 'applying', apply_owner = ?1, apply_lease_expires_at = ?2,
                     updated_at = ?3
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
            WriteProposalState::Applying,
            None,
            now,
        )?;
        let claimed =
            load_by_id(&transaction, proposal_id)?.ok_or(WriteProposalStoreError::Sqlite)?;
        transaction.commit().map_err(map_sqlite)?;
        Ok(WriteApplyClaimOutcome::Claimed(claimed))
    }

    /// Complete apply with a typed receipt. State is derived from the receipt.
    ///
    /// Requires the exact owner and a still-live apply lease at `now`.
    pub fn complete_apply(
        &self,
        proposal_id: &str,
        owner: &str,
        receipt: &WriteApplyReceipt,
        now: DateTime<Utc>,
    ) -> Result<StoredWriteProposal, WriteProposalStoreError> {
        self.begin_mutation()?;
        validate_owner(owner)?;
        // Recovery path uses reconcile_applying; live complete must not claim recovered.
        if receipt.recovered() {
            return Err(WriteProposalStoreError::InvalidResult);
        }

        let mut connection = self.connection()?;
        let transaction = connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(map_sqlite)?;
        let current = load_by_id(&transaction, proposal_id)?
            .ok_or(WriteProposalStoreError::ReservationConflict)?;

        let terminal_state = receipt.terminal_state();
        let result_json = serialize_receipt_matching(&current, receipt)?;

        if is_terminal(current.state) {
            if current.state == terminal_state
                && current.result_json.as_deref() == Some(&result_json)
            {
                transaction.commit().map_err(map_sqlite)?;
                return Ok(current);
            }
            return Err(WriteProposalStoreError::InvalidResult);
        }

        let (apply_owner, lease_expires_at) = load_apply_lease(&transaction, proposal_id)?;
        if current.state != WriteProposalState::Applying
            || apply_owner.as_deref() != Some(owner)
            || !lease_is_live(lease_expires_at, now)
        {
            return Err(WriteProposalStoreError::ReservationConflict);
        }

        validate_write_transition(current.state, terminal_state)
            .map_err(|_| WriteProposalStoreError::InvalidTransition)?;

        transaction
            .execute(
                "UPDATE write_proposals
                 SET state = ?1, result_json = ?2, apply_owner = NULL,
                     apply_lease_expires_at = NULL, updated_at = ?3
                 WHERE proposal_id = ?4 AND apply_owner = ?5",
                params![
                    state_text(terminal_state),
                    result_json,
                    now.to_rfc3339(),
                    proposal_id,
                    owner,
                ],
            )
            .map_err(map_sqlite)?;
        insert_transition(
            &transaction,
            proposal_id,
            terminal_state,
            Some(&result_json),
            now,
        )?;
        let terminal =
            load_by_id(&transaction, proposal_id)?.ok_or(WriteProposalStoreError::Sqlite)?;
        transaction.commit().map_err(map_sqlite)?;
        Ok(terminal)
    }

    /// Live cancellation while an apply lease is held.
    ///
    /// Requires current state exactly `applying`, an exact `apply_owner` match,
    /// and a still-live lease at `now`. Atomically transitions to `interrupted`,
    /// clears lease fields, and appends history. Owner mismatch or an expired
    /// lease returns [`WriteProposalStoreError::ReservationConflict`] without
    /// mutating state or history.
    ///
    /// Restart recovery of orphaned `applying` rows must use
    /// [`Self::reconcile_applying`] instead — this path is owner-aware only.
    pub fn interrupt_apply(
        &self,
        proposal_id: &str,
        owner: &str,
        now: DateTime<Utc>,
    ) -> Result<StoredWriteProposal, WriteProposalStoreError> {
        self.begin_mutation()?;
        validate_owner(owner)?;

        let mut connection = self.connection()?;
        let transaction = connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(map_sqlite)?;
        let current = load_by_id(&transaction, proposal_id)?
            .ok_or(WriteProposalStoreError::ReservationConflict)?;

        let (apply_owner, lease_expires_at) = load_apply_lease(&transaction, proposal_id)?;
        if current.state != WriteProposalState::Applying
            || apply_owner.as_deref() != Some(owner)
            || !lease_is_live(lease_expires_at, now)
        {
            return Err(WriteProposalStoreError::ReservationConflict);
        }

        validate_write_transition(current.state, WriteProposalState::Interrupted)
            .map_err(|_| WriteProposalStoreError::InvalidTransition)?;

        let changed = transaction
            .execute(
                "UPDATE write_proposals
                 SET state = 'interrupted', apply_owner = NULL,
                     apply_lease_expires_at = NULL, updated_at = ?1
                 WHERE proposal_id = ?2 AND state = 'applying' AND apply_owner = ?3",
                params![now.to_rfc3339(), proposal_id, owner],
            )
            .map_err(map_sqlite)?;
        if changed != 1 {
            return Err(WriteProposalStoreError::ReservationConflict);
        }

        insert_transition(
            &transaction,
            proposal_id,
            WriteProposalState::Interrupted,
            None,
            now,
        )?;
        let interrupted =
            load_by_id(&transaction, proposal_id)?.ok_or(WriteProposalStoreError::Sqlite)?;
        transaction.commit().map_err(map_sqlite)?;
        Ok(interrupted)
    }

    /// Owner-independent restart reconciliation for an `applying` row.
    ///
    /// Performs no filesystem work and accepts no content. Replay-safe for an
    /// identical terminal outcome; conflicting observations fail closed.
    pub fn reconcile_applying(
        &self,
        proposal_id: &str,
        observation: WriteApplyReconcileObservation,
        now: DateTime<Utc>,
    ) -> Result<StoredWriteProposal, WriteProposalStoreError> {
        self.begin_mutation()?;
        let mut connection = self.connection()?;
        let transaction = connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(map_sqlite)?;
        let current = load_by_id(&transaction, proposal_id)?
            .ok_or(WriteProposalStoreError::ReservationConflict)?;

        let expected = expected_reconcile_terminal(&current, observation)?;

        if is_terminal(current.state) {
            if current.state == expected.state
                && current.result_json.as_deref() == expected.result_json.as_deref()
            {
                transaction.commit().map_err(map_sqlite)?;
                return Ok(current);
            }
            return Err(WriteProposalStoreError::InvalidResult);
        }

        if current.state != WriteProposalState::Applying {
            return Err(WriteProposalStoreError::InvalidTransition);
        }

        validate_write_transition(current.state, expected.state)
            .map_err(|_| WriteProposalStoreError::InvalidTransition)?;

        transaction
            .execute(
                "UPDATE write_proposals
                 SET state = ?1, result_json = ?2, apply_owner = NULL,
                     apply_lease_expires_at = NULL, updated_at = ?3
                 WHERE proposal_id = ?4 AND state = 'applying'",
                params![
                    state_text(expected.state),
                    expected.result_json,
                    now.to_rfc3339(),
                    proposal_id,
                ],
            )
            .map_err(map_sqlite)?;
        insert_transition(
            &transaction,
            proposal_id,
            expected.state,
            expected.result_json.as_deref(),
            now,
        )?;
        let terminal =
            load_by_id(&transaction, proposal_id)?.ok_or(WriteProposalStoreError::Sqlite)?;
        transaction.commit().map_err(map_sqlite)?;
        Ok(terminal)
    }

    pub fn transition(
        &self,
        proposal_id: &str,
        next: WriteProposalState,
        now: DateTime<Utc>,
    ) -> Result<StoredWriteProposal, WriteProposalStoreError> {
        self.begin_mutation()?;
        // Terminal apply outcomes only via complete_apply / reconcile_applying.
        if matches!(
            next,
            WriteProposalState::Applying
                | WriteProposalState::Succeeded
                | WriteProposalState::Failed
                | WriteProposalState::Approved
        ) {
            return Err(WriteProposalStoreError::InvalidTransition);
        }

        let mut connection = self.connection()?;
        let transaction = connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(map_sqlite)?;
        let current = load_by_id(&transaction, proposal_id)?
            .ok_or(WriteProposalStoreError::ReservationConflict)?;
        if current.state == next {
            transaction.commit().map_err(map_sqlite)?;
            return Ok(current);
        }
        if is_terminal(current.state) {
            return Err(WriteProposalStoreError::InvalidTransition);
        }
        validate_write_transition(current.state, next)
            .map_err(|_| WriteProposalStoreError::InvalidTransition)?;

        let updated = force_transition(&transaction, &current, next, None, now)?;
        transaction.commit().map_err(map_sqlite)?;
        Ok(updated)
    }

    pub fn recover_interrupted(
        &self,
        now: DateTime<Utc>,
    ) -> Result<Vec<StoredWriteProposal>, WriteProposalStoreError> {
        self.begin_mutation()?;
        let mut connection = self.connection()?;
        let transaction = connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(map_sqlite)?;
        let ids = {
            let mut statement = transaction
                .prepare(
                    "SELECT proposal_id FROM write_proposals
                     WHERE state IN ('pending_approval', 'approved')
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
                load_by_id(&transaction, &proposal_id)?.ok_or(WriteProposalStoreError::Sqlite)?;
            let updated = force_transition(
                &transaction,
                &current,
                WriteProposalState::Interrupted,
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
    ) -> Result<Vec<WriteProposalState>, WriteProposalStoreError> {
        let connection = self.connection()?;
        let _ = load_by_id(&connection, proposal_id)?
            .ok_or(WriteProposalStoreError::ReservationConflict)?;
        let mut statement = connection
            .prepare(
                "SELECT state FROM write_proposal_transitions
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
    ) -> Result<Vec<StoredWriteProposal>, WriteProposalStoreError> {
        let connection = self.connection()?;
        let mut statement = connection
            .prepare(
                "SELECT proposal_id, reservation_key, proposal_fingerprint, project_id,
                        conversation_id, run_id, call_id, path, mode, proposed_digest,
                        proposed_bytes, base_digest, base_bytes, state, expires_at,
                        result_json, created_at, updated_at
                 FROM write_proposals
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

    /// Read-only enumeration of orphaned `applying` rows for restart recovery.
    ///
    /// Performs no filesystem observation and no state transition. Callers
    /// must observe targets via the write executor and pass results to
    /// [`Self::reconcile_applying`].
    pub fn list_applying(&self) -> Result<Vec<StoredWriteProposal>, WriteProposalStoreError> {
        let connection = self.connection()?;
        let mut statement = connection
            .prepare(
                "SELECT proposal_id, reservation_key, proposal_fingerprint, project_id,
                        conversation_id, run_id, call_id, path, mode, proposed_digest,
                        proposed_bytes, base_digest, base_bytes, state, expires_at,
                        result_json, created_at, updated_at
                 FROM write_proposals
                 WHERE state = 'applying'
                 ORDER BY created_at ASC, proposal_id ASC",
            )
            .map_err(map_sqlite)?;
        statement
            .query_map([], stored_from_row)
            .map_err(map_sqlite)?
            .collect::<Result<Vec<_>, _>>()
            .map_err(map_sqlite)
    }

    fn connection(&self) -> Result<MutexGuard<'_, Connection>, WriteProposalStoreError> {
        self.connection
            .lock()
            .map_err(|_| WriteProposalStoreError::LockPoisoned)
    }
}

#[derive(Debug)]
struct StoredDecision {
    decision: WriteDecision,
    approval_grant_digest: String,
    proposal_fingerprint: String,
}

struct ExpectedTerminal {
    state: WriteProposalState,
    result_json: Option<String>,
}

fn expected_reconcile_terminal(
    current: &StoredWriteProposal,
    observation: WriteApplyReconcileObservation,
) -> Result<ExpectedTerminal, WriteProposalStoreError> {
    match observation {
        WriteApplyReconcileObservation::MatchesProposed => {
            let receipt = WriteApplyReceipt::Success {
                path: current.path.clone(),
                mode: current.mode,
                bytes_written: current.proposed_bytes,
                content_digest: current.proposed_digest.clone(),
                recovered: true,
            };
            let result_json = serialize_receipt_matching(current, &receipt)?;
            Ok(ExpectedTerminal {
                state: WriteProposalState::Succeeded,
                result_json: Some(result_json),
            })
        }
        WriteApplyReconcileObservation::StillBaseOrAbsent => Ok(ExpectedTerminal {
            state: WriteProposalState::Interrupted,
            result_json: None,
        }),
        WriteApplyReconcileObservation::Neither => {
            let receipt = WriteApplyReceipt::Failure {
                error_code: WriteApplyErrorCode::WriteFailed,
                recovered: true,
            };
            let result_json = serialize_receipt_matching(current, &receipt)?;
            Ok(ExpectedTerminal {
                state: WriteProposalState::Failed,
                result_json: Some(result_json),
            })
        }
    }
}

fn serialize_receipt_matching(
    current: &StoredWriteProposal,
    receipt: &WriteApplyReceipt,
) -> Result<String, WriteProposalStoreError> {
    match receipt {
        WriteApplyReceipt::Success {
            path,
            mode,
            bytes_written,
            content_digest,
            recovered: _,
        } => {
            if path != &current.path
                || *mode != current.mode
                || *bytes_written != current.proposed_bytes
                || content_digest != &current.proposed_digest
            {
                return Err(WriteProposalStoreError::InvalidResult);
            }
            validate_canonical_digest(content_digest)?;
        }
        WriteApplyReceipt::Failure {
            error_code,
            recovered: _,
        } => {
            // Exhaustive closed set: adding a variant requires an explicit review.
            match error_code {
                WriteApplyErrorCode::BaseChanged
                | WriteApplyErrorCode::PathRejected
                | WriteApplyErrorCode::WriteFailed => {}
            }
        }
    }
    let json =
        serde_json::to_string(receipt).map_err(|_| WriteProposalStoreError::InvalidResult)?;
    if json.is_empty() || json.len() > RESULT_JSON_BYTE_CAP {
        return Err(WriteProposalStoreError::InvalidResult);
    }
    // Round-trip through deny_unknown_fields deserialize to reject any unexpected shape.
    let _: WriteApplyReceipt =
        serde_json::from_str(&json).map_err(|_| WriteProposalStoreError::InvalidResult)?;
    Ok(json)
}

fn validate_reserve_input(
    input: &WriteProposalReserveInput,
) -> Result<(), WriteProposalStoreError> {
    if Uuid::parse_str(&input.proposal_id).is_err() || input.proposal_id.len() != 36 {
        return Err(WriteProposalStoreError::InvalidResult);
    }
    if input.reservation_key.is_empty() || input.reservation_key.len() > 512 {
        return Err(WriteProposalStoreError::InvalidResult);
    }
    if input.call_id.is_empty() || input.call_id.len() > 128 {
        return Err(WriteProposalStoreError::InvalidResult);
    }
    if input.path.is_empty() || input.path.len() > 4_096 {
        return Err(WriteProposalStoreError::InvalidResult);
    }
    validate_canonical_digest(&input.proposal_fingerprint)?;
    validate_canonical_digest(&input.proposed_digest)?;
    if input.proposed_bytes == 0 || input.proposed_bytes > WRITE_CONTENT_BYTE_CAP {
        return Err(WriteProposalStoreError::InvalidResult);
    }
    match input.mode {
        WriteMode::Create => {
            if input.base_digest.is_some() || input.base_bytes != 0 {
                return Err(WriteProposalStoreError::InvalidResult);
            }
        }
        WriteMode::Modify => {
            let Some(base) = input.base_digest.as_deref() else {
                return Err(WriteProposalStoreError::InvalidResult);
            };
            validate_canonical_digest(base)?;
            if input.base_bytes > WRITE_CONTENT_BYTE_CAP {
                return Err(WriteProposalStoreError::InvalidResult);
            }
        }
    }
    Ok(())
}

fn validate_canonical_digest(digest: &str) -> Result<(), WriteProposalStoreError> {
    let Some(hex) = digest.strip_prefix("sha256:") else {
        return Err(WriteProposalStoreError::InvalidResult);
    };
    if hex.len() != 64
        || !hex
            .as_bytes()
            .iter()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(byte))
    {
        return Err(WriteProposalStoreError::InvalidResult);
    }
    Ok(())
}

fn validate_lease_ms(lease_ms: u64) -> Result<(), WriteProposalStoreError> {
    if lease_ms == 0 || lease_ms > MAX_APPLY_LEASE_MS {
        return Err(WriteProposalStoreError::InvalidResult);
    }
    Ok(())
}

fn validate_owner(owner: &str) -> Result<(), WriteProposalStoreError> {
    if owner.is_empty()
        || owner.len() > 128
        || !owner
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.' | b':'))
    {
        return Err(WriteProposalStoreError::ReservationConflict);
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
) -> Result<(), WriteProposalStoreError> {
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
        Err(WriteProposalStoreError::MissingContext)
    }
}

fn force_transition(
    connection: &Connection,
    current: &StoredWriteProposal,
    next: WriteProposalState,
    result_json: Option<&str>,
    now: DateTime<Utc>,
) -> Result<StoredWriteProposal, WriteProposalStoreError> {
    validate_write_transition(current.state, next)
        .map_err(|_| WriteProposalStoreError::InvalidTransition)?;
    connection
        .execute(
            "UPDATE write_proposals
             SET state = ?1, result_json = COALESCE(?2, result_json),
                 apply_owner = NULL, apply_lease_expires_at = NULL, updated_at = ?3
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
    load_by_id(connection, &current.proposal_id)?.ok_or(WriteProposalStoreError::Sqlite)
}

fn insert_transition(
    connection: &Connection,
    proposal_id: &str,
    state: WriteProposalState,
    result_json: Option<&str>,
    now: DateTime<Utc>,
) -> Result<(), WriteProposalStoreError> {
    connection
        .execute(
            "INSERT INTO write_proposal_transitions (
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
) -> Result<Option<StoredWriteProposal>, WriteProposalStoreError> {
    connection
        .query_row(
            "SELECT proposal_id, reservation_key, proposal_fingerprint, project_id,
                    conversation_id, run_id, call_id, path, mode, proposed_digest,
                    proposed_bytes, base_digest, base_bytes, state, expires_at,
                    result_json, created_at, updated_at
             FROM write_proposals WHERE proposal_id = ?1",
            [proposal_id],
            stored_from_row,
        )
        .optional()
        .map_err(map_sqlite)
}

fn load_by_reservation_key(
    connection: &Connection,
    reservation_key: &str,
) -> Result<Option<StoredWriteProposal>, WriteProposalStoreError> {
    connection
        .query_row(
            "SELECT proposal_id, reservation_key, proposal_fingerprint, project_id,
                    conversation_id, run_id, call_id, path, mode, proposed_digest,
                    proposed_bytes, base_digest, base_bytes, state, expires_at,
                    result_json, created_at, updated_at
             FROM write_proposals WHERE reservation_key = ?1",
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
) -> Result<Option<StoredWriteProposal>, WriteProposalStoreError> {
    connection
        .query_row(
            "SELECT proposal_id, reservation_key, proposal_fingerprint, project_id,
                    conversation_id, run_id, call_id, path, mode, proposed_digest,
                    proposed_bytes, base_digest, base_bytes, state, expires_at,
                    result_json, created_at, updated_at
             FROM write_proposals
             WHERE project_id = ?1 AND conversation_id = ?2
               AND run_id = ?3 AND call_id = ?4",
            params![project_id, conversation_id, run_id, call_id],
            stored_from_row,
        )
        .optional()
        .map_err(map_sqlite)
}

fn load_active_on_run(
    connection: &Connection,
    run_id: &str,
) -> Result<Option<StoredWriteProposal>, WriteProposalStoreError> {
    connection
        .query_row(
            "SELECT proposal_id, reservation_key, proposal_fingerprint, project_id,
                    conversation_id, run_id, call_id, path, mode, proposed_digest,
                    proposed_bytes, base_digest, base_bytes, state, expires_at,
                    result_json, created_at, updated_at
             FROM write_proposals
             WHERE run_id = ?1
               AND state IN ('pending_approval', 'approved', 'applying')
             LIMIT 1",
            [run_id],
            stored_from_row,
        )
        .optional()
        .map_err(map_sqlite)
}

fn load_decision(
    connection: &Connection,
    proposal_id: &str,
) -> Result<Option<StoredDecision>, WriteProposalStoreError> {
    connection
        .query_row(
            "SELECT decision, approval_grant_digest, proposal_fingerprint
             FROM write_approval_decisions WHERE proposal_id = ?1",
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
) -> Result<Option<StoredPermissionRuleReceipt>, WriteProposalStoreError> {
    connection
        .query_row(
            "SELECT proposal_id, decision, actor, rule_id, rule_scope, rule_project_id,
                    rule_prefix, rule_pattern, rule_action, rule_content_digest,
                    match_subject, decided_at
             FROM write_permission_rule_receipts WHERE proposal_id = ?1",
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
) -> Result<(), WriteProposalStoreError> {
    connection
        .execute(
            "INSERT INTO write_permission_rule_receipts (
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
) -> Result<bool, WriteProposalStoreError> {
    connection
        .query_row(
            "SELECT EXISTS(
                SELECT 1 FROM write_approval_decisions WHERE approval_grant_digest = ?1
             )",
            [digest],
            |row| row.get::<_, bool>(0),
        )
        .map_err(map_sqlite)
}

fn load_apply_lease(
    connection: &Connection,
    proposal_id: &str,
) -> Result<(Option<String>, Option<DateTime<Utc>>), WriteProposalStoreError> {
    connection
        .query_row(
            "SELECT apply_owner, apply_lease_expires_at
             FROM write_proposals WHERE proposal_id = ?1",
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

fn stored_from_row(row: &Row<'_>) -> rusqlite::Result<StoredWriteProposal> {
    let run_id: String = row.get(5)?;
    let mode: String = row.get(8)?;
    let proposed_bytes: i64 = row.get(10)?;
    let base_bytes: i64 = row.get(12)?;
    let state: String = row.get(13)?;
    let expires_at: String = row.get(14)?;
    let created_at: String = row.get(16)?;
    let updated_at: String = row.get(17)?;
    Ok(StoredWriteProposal {
        proposal_id: row.get(0)?,
        reservation_key: row.get(1)?,
        proposal_fingerprint: row.get(2)?,
        project_id: row.get(3)?,
        conversation_id: row.get(4)?,
        run_id: RunId(parse_uuid(&run_id, 5)?),
        call_id: row.get(6)?,
        path: row.get(7)?,
        mode: parse_mode(&mode, 8)?,
        proposed_digest: row.get(9)?,
        proposed_bytes: u64::try_from(proposed_bytes).map_err(|error| {
            rusqlite::Error::FromSqlConversionFailure(10, Type::Integer, Box::new(error))
        })?,
        base_digest: row.get(11)?,
        base_bytes: u64::try_from(base_bytes).map_err(|error| {
            rusqlite::Error::FromSqlConversionFailure(12, Type::Integer, Box::new(error))
        })?,
        state: parse_state(&state, 13)?,
        expires_at: parse_timestamp(&expires_at, 14)?,
        result_json: row.get(15)?,
        created_at: parse_timestamp(&created_at, 16)?,
        updated_at: parse_timestamp(&updated_at, 17)?,
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

fn parse_mode(value: &str, index: usize) -> rusqlite::Result<WriteMode> {
    match value {
        "create" => Ok(WriteMode::Create),
        "modify" => Ok(WriteMode::Modify),
        _ => Err(rusqlite::Error::FromSqlConversionFailure(
            index,
            Type::Text,
            Box::new(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "invalid write mode",
            )),
        )),
    }
}

fn parse_state(value: &str, index: usize) -> rusqlite::Result<WriteProposalState> {
    match value {
        "pending_approval" => Ok(WriteProposalState::PendingApproval),
        "approved" => Ok(WriteProposalState::Approved),
        "applying" => Ok(WriteProposalState::Applying),
        "succeeded" => Ok(WriteProposalState::Succeeded),
        "rejected" => Ok(WriteProposalState::Rejected),
        "expired" => Ok(WriteProposalState::Expired),
        "cancelled" => Ok(WriteProposalState::Cancelled),
        "interrupted" => Ok(WriteProposalState::Interrupted),
        "failed" => Ok(WriteProposalState::Failed),
        _ => Err(rusqlite::Error::FromSqlConversionFailure(
            index,
            Type::Text,
            Box::new(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "invalid write proposal state",
            )),
        )),
    }
}

fn parse_decision(value: &str, index: usize) -> rusqlite::Result<WriteDecision> {
    match value {
        "approve_once" => Ok(WriteDecision::ApproveOnce),
        "reject" => Ok(WriteDecision::Reject),
        _ => Err(rusqlite::Error::FromSqlConversionFailure(
            index,
            Type::Text,
            Box::new(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "invalid write decision",
            )),
        )),
    }
}

fn mode_text(mode: WriteMode) -> &'static str {
    match mode {
        WriteMode::Create => "create",
        WriteMode::Modify => "modify",
    }
}

fn state_text(state: WriteProposalState) -> &'static str {
    match state {
        WriteProposalState::PendingApproval => "pending_approval",
        WriteProposalState::Approved => "approved",
        WriteProposalState::Applying => "applying",
        WriteProposalState::Succeeded => "succeeded",
        WriteProposalState::Rejected => "rejected",
        WriteProposalState::Expired => "expired",
        WriteProposalState::Cancelled => "cancelled",
        WriteProposalState::Interrupted => "interrupted",
        WriteProposalState::Failed => "failed",
    }
}

fn decision_text(decision: WriteDecision) -> &'static str {
    match decision {
        WriteDecision::ApproveOnce => "approve_once",
        WriteDecision::Reject => "reject",
    }
}

fn map_sqlite(_: rusqlite::Error) -> WriteProposalStoreError {
    WriteProposalStoreError::Sqlite
}
