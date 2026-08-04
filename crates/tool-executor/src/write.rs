//! Host-side governed write preparation, path policy, and atomic apply.
//!
//! Content stays in active-host memory only. SQLite receives digests and typed
//! receipts via the C3-B repository APIs.

use std::fs::{self, File, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};

use chrono::Utc;
use keeper_agent_protocol::{WRITE_CONTENT_BYTE_CAP, WriteMode};
use keeper_policy::WriteProposalState;
use keeper_storage_sqlite::{
    SqliteWriteProposalRepository, StoredWriteProposal, WriteApplyClaimOutcome,
    WriteApplyErrorCode, WriteApplyReceipt, WriteApplyReconcileObservation,
    WriteProposalStoreError,
};
use sha2::{Digest, Sha256};
use thiserror::Error;
use uuid::Uuid;

use crate::write_diff::{BoundedDiff, build_bounded_unified_diff};
use crate::{CancellationSignal, ProjectCapability};

/// Default apply lease duration (milliseconds).
const APPLY_LEASE_MS: u64 = 60_000;

const DENIED_COMPONENTS: &[&str] = &[".git", ".pillow", ".keeper", ".ssh"];
const DENIED_LEAVES: &[&str] = &[".env", ".env.local", "id_rsa", "id_ed25519"];
const DENIED_EXTENSIONS: &[&str] = &[".pem", ".key", ".p12", ".pfx", ".jks", ".keystore"];

/// Sealed base/proposed UTF-8 payloads held only in active-host memory.
///
/// Debug and any future Serialize path must redact both contents.
#[derive(Clone)]
pub struct SealedWriteContents {
    proposed_content: String,
    base_content: Option<String>,
}

impl SealedWriteContents {
    pub fn new(proposed_content: String, base_content: Option<String>) -> Self {
        Self {
            proposed_content,
            base_content,
        }
    }

    pub fn proposed_content(&self) -> &str {
        &self.proposed_content
    }

    pub fn base_content(&self) -> Option<&str> {
        self.base_content.as_deref()
    }
}

impl std::fmt::Debug for SealedWriteContents {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SealedWriteContents")
            .field("proposed_content", &"<redacted>")
            .field(
                "base_content",
                &self.base_content.as_ref().map(|_| "<redacted>"),
            )
            .finish()
    }
}

/// Host-captured base snapshot and proposed digests.
///
/// `base_content` is host-only memory. Debug redacts it; no `Serialize` impl.
#[derive(Clone, PartialEq, Eq)]
pub struct BaseSnapshot {
    pub base_content: Option<String>,
    pub base_digest: Option<String>,
    pub base_bytes: u64,
    pub proposed_digest: String,
    pub proposed_bytes: u64,
}

impl std::fmt::Debug for BaseSnapshot {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("BaseSnapshot")
            .field(
                "base_content",
                &self.base_content.as_ref().map(|_| "<redacted>"),
            )
            .field("base_digest", &self.base_digest)
            .field("base_bytes", &self.base_bytes)
            .field("proposed_digest", &self.proposed_digest)
            .field("proposed_bytes", &self.proposed_bytes)
            .finish()
    }
}

/// Prepared proposal for review: durable metadata + sealed contents + host diff.
///
/// Base/proposed content and the unified diff are redacted in Debug. No
/// `Serialize` impl for this source-bearing type.
#[derive(Clone)]
pub struct PreparedWriteProposal {
    pub metadata: StoredWriteProposal,
    pub base_content: Option<String>,
    pub proposed_content: String,
    pub diff: BoundedDiff,
}

impl std::fmt::Debug for PreparedWriteProposal {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PreparedWriteProposal")
            .field("metadata", &self.metadata)
            .field(
                "base_content",
                &self.base_content.as_ref().map(|_| "<redacted>"),
            )
            .field("proposed_content", &"<redacted>")
            .field("diff", &self.diff)
            .finish()
    }
}

/// Outcome of a governed apply attempt.
///
/// After an apply lease is claimed the proposal is in `applying`. Pre-mutation
/// cancellation must terminalize as durable `interrupted` (C3-A does not allow
/// `applying -> cancelled`). The outcome name matches the durable state.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GovernedWriteApplyOutcome {
    Succeeded(StoredWriteProposal),
    Failed(StoredWriteProposal),
    Busy(StoredWriteProposal),
    Terminal(StoredWriteProposal),
    /// Apply was claimed then cancelled before mutation; durable state is
    /// [`WriteProposalState::Interrupted`].
    Interrupted(StoredWriteProposal),
    Expired(StoredWriteProposal),
    Conflict(StoredWriteProposal),
}

#[derive(Debug, Error)]
pub enum GovernedWriteError {
    #[error("write proposal store error")]
    Store(#[from] WriteProposalStoreError),
    #[error("project capability root could not be established")]
    InvalidProjectRoot,
    #[error("sealed content digests do not match durable proposal metadata")]
    DigestMismatch,
    #[error("write path rejected by portable grammar or deny policy")]
    PathRejected,
    #[error("base capture rejected: binary, oversize, or mode mismatch")]
    BaseCaptureFailed,
    #[error("write proposal is not available for apply")]
    NotApplicable,
}

/// Host gateway: prepare diffs and apply only after a durable accepted decision.
pub struct GovernedWriteToolGateway {
    store: SqliteWriteProposalRepository,
}

impl GovernedWriteToolGateway {
    pub fn open(path: impl AsRef<Path>) -> Result<Self, GovernedWriteError> {
        Ok(Self {
            store: SqliteWriteProposalRepository::open(path)?,
        })
    }

    pub fn store(&self) -> &SqliteWriteProposalRepository {
        &self.store
    }

    /// Read-only observation of an `applying` row's on-disk target for restart
    /// reconciliation.
    ///
    /// Performs no write, returns no content, and reuses path policy,
    /// canonicalization, symlink/reparse, and digest checks. Project mismatch
    /// and any path/policy failure classify as [`WriteApplyReconcileObservation::Neither`].
    pub fn observe_applying_target(
        &self,
        capability: &ProjectCapability,
        metadata: &StoredWriteProposal,
    ) -> WriteApplyReconcileObservation {
        if capability.project_id() != metadata.project_id {
            return WriteApplyReconcileObservation::Neither;
        }
        if validate_write_path_policy(&metadata.path).is_err() {
            return WriteApplyReconcileObservation::Neither;
        }
        let Ok(root) = revalidate_root(capability) else {
            return WriteApplyReconcileObservation::Neither;
        };

        match metadata.mode {
            WriteMode::Create => match path_exists_under(&root, &metadata.path) {
                Ok(false) => WriteApplyReconcileObservation::StillBaseOrAbsent,
                Ok(true) => match read_regular_utf8_file(&root, &metadata.path) {
                    Ok(on_disk)
                        if content_sha256_digest(&on_disk) == metadata.proposed_digest
                            && on_disk.len() as u64 == metadata.proposed_bytes =>
                    {
                        WriteApplyReconcileObservation::MatchesProposed
                    }
                    _ => WriteApplyReconcileObservation::Neither,
                },
                Err(_) => WriteApplyReconcileObservation::Neither,
            },
            WriteMode::Modify => match read_regular_utf8_file(&root, &metadata.path) {
                Ok(on_disk) => {
                    let digest = content_sha256_digest(&on_disk);
                    let bytes = on_disk.len() as u64;
                    if digest == metadata.proposed_digest && bytes == metadata.proposed_bytes {
                        WriteApplyReconcileObservation::MatchesProposed
                    } else if metadata
                        .base_digest
                        .as_deref()
                        .is_some_and(|base| digest == base && bytes == metadata.base_bytes)
                    {
                        WriteApplyReconcileObservation::StillBaseOrAbsent
                    } else {
                        WriteApplyReconcileObservation::Neither
                    }
                }
                Err(_) => {
                    // Missing modify target, symlink/reparse, or unreadable → neither.
                    WriteApplyReconcileObservation::Neither
                }
            },
        }
    }

    /// Capture base content for create/modify under path policy (host-only).
    pub fn capture_base(
        capability: &ProjectCapability,
        path: &str,
        mode: WriteMode,
        proposed_content: &str,
    ) -> Result<BaseSnapshot, GovernedWriteError> {
        validate_proposed_content(proposed_content)?;
        validate_write_path_policy(path)?;

        let root = revalidate_root(capability)?;
        let proposed_digest = content_sha256_digest(proposed_content);
        let proposed_bytes = proposed_content.len() as u64;

        match mode {
            WriteMode::Create => {
                // Target must be absent; parent must exist as a real directory.
                if path_exists_under(&root, path)? {
                    return Err(GovernedWriteError::BaseCaptureFailed);
                }
                ensure_parent_real_dir(&root, path)?;
                Ok(BaseSnapshot {
                    base_content: None,
                    base_digest: None,
                    base_bytes: 0,
                    proposed_digest,
                    proposed_bytes,
                })
            }
            WriteMode::Modify => {
                let base = read_regular_utf8_file(&root, path)?;
                let base_digest = content_sha256_digest(&base);
                let base_bytes = base.len() as u64;
                Ok(BaseSnapshot {
                    base_content: Some(base),
                    base_digest: Some(base_digest),
                    base_bytes,
                    proposed_digest,
                    proposed_bytes,
                })
            }
        }
    }

    /// Verify sealed digests against durable metadata and build the review diff.
    pub fn prepare(
        &self,
        metadata: StoredWriteProposal,
        sealed: SealedWriteContents,
    ) -> Result<PreparedWriteProposal, GovernedWriteError> {
        verify_sealed_against_metadata(&metadata, &sealed)?;
        let diff = build_bounded_unified_diff(sealed.base_content(), sealed.proposed_content());
        Ok(PreparedWriteProposal {
            metadata,
            base_content: sealed.base_content,
            proposed_content: sealed.proposed_content,
            diff,
        })
    }

    /// Apply sealed content after the repository holds an accepted decision.
    ///
    /// Claims an apply lease, revalidates path/base, writes via same-directory
    /// temp + OS atomic create/replace, then persists a typed terminal receipt
    /// before returning.
    pub fn apply(
        &self,
        capability: &ProjectCapability,
        proposal_id: &str,
        sealed: &SealedWriteContents,
        cancellation: &dyn CancellationSignal,
    ) -> Result<GovernedWriteApplyOutcome, GovernedWriteError> {
        if capability.project_id().is_empty() {
            return Err(GovernedWriteError::InvalidProjectRoot);
        }

        let owner = format!("write-gateway-{}", Uuid::new_v4());
        let now = Utc::now();
        let claim = self
            .store
            .claim_apply(proposal_id, &owner, APPLY_LEASE_MS, now)?;

        let metadata = match claim {
            WriteApplyClaimOutcome::Claimed(row) => row,
            WriteApplyClaimOutcome::Busy(row) => {
                return Ok(GovernedWriteApplyOutcome::Busy(row));
            }
            WriteApplyClaimOutcome::Terminal(row) => {
                return Ok(GovernedWriteApplyOutcome::Terminal(row));
            }
            WriteApplyClaimOutcome::Expired(row) => {
                return Ok(GovernedWriteApplyOutcome::Expired(row));
            }
            WriteApplyClaimOutcome::Conflict(row) => {
                return Ok(GovernedWriteApplyOutcome::Conflict(row));
            }
        };

        if let Err(error) = verify_sealed_against_metadata(&metadata, sealed) {
            return self.fail_apply(proposal_id, &owner, WriteApplyErrorCode::WriteFailed, error);
        }

        if cancellation.is_cancelled() {
            return self.interrupt_apply(proposal_id, &owner);
        }

        match self.apply_filesystem(capability, &metadata, sealed, cancellation) {
            Ok(digest) => {
                let receipt = WriteApplyReceipt::Success {
                    path: metadata.path.clone(),
                    mode: metadata.mode,
                    bytes_written: metadata.proposed_bytes,
                    content_digest: digest,
                    recovered: false,
                };
                let row = self
                    .store
                    .complete_apply(proposal_id, &owner, &receipt, Utc::now())?;
                Ok(GovernedWriteApplyOutcome::Succeeded(row))
            }
            Err(ApplyFsError::Cancelled) => self.interrupt_apply(proposal_id, &owner),
            Err(ApplyFsError::BaseChanged) => self.fail_apply(
                proposal_id,
                &owner,
                WriteApplyErrorCode::BaseChanged,
                GovernedWriteError::BaseCaptureFailed,
            ),
            Err(ApplyFsError::PathRejected) => self.fail_apply(
                proposal_id,
                &owner,
                WriteApplyErrorCode::PathRejected,
                GovernedWriteError::PathRejected,
            ),
            Err(ApplyFsError::WriteFailed) => self.fail_apply(
                proposal_id,
                &owner,
                WriteApplyErrorCode::WriteFailed,
                GovernedWriteError::NotApplicable,
            ),
        }
    }

    fn fail_apply(
        &self,
        proposal_id: &str,
        owner: &str,
        code: WriteApplyErrorCode,
        _source: GovernedWriteError,
    ) -> Result<GovernedWriteApplyOutcome, GovernedWriteError> {
        let receipt = WriteApplyReceipt::Failure {
            error_code: code,
            recovered: false,
        };
        let row = self
            .store
            .complete_apply(proposal_id, owner, &receipt, Utc::now())?;
        Ok(GovernedWriteApplyOutcome::Failed(row))
    }

    /// C3-A: once apply is claimed (`applying`), pre-mutation cancel is
    /// `applying -> interrupted`, never `applying -> cancelled`.
    ///
    /// Uses the owner-aware storage interrupt API so an expired or reclaimed
    /// lease cannot cancel a later owner's apply.
    fn interrupt_apply(
        &self,
        proposal_id: &str,
        owner: &str,
    ) -> Result<GovernedWriteApplyOutcome, GovernedWriteError> {
        let row = self.store.interrupt_apply(proposal_id, owner, Utc::now())?;
        debug_assert_eq!(row.state, WriteProposalState::Interrupted);
        Ok(GovernedWriteApplyOutcome::Interrupted(row))
    }

    fn apply_filesystem(
        &self,
        capability: &ProjectCapability,
        metadata: &StoredWriteProposal,
        sealed: &SealedWriteContents,
        cancellation: &dyn CancellationSignal,
    ) -> Result<String, ApplyFsError> {
        if cancellation.is_cancelled() {
            return Err(ApplyFsError::Cancelled);
        }
        if metadata.project_id != capability.project_id() {
            return Err(ApplyFsError::PathRejected);
        }

        let root = revalidate_root(capability).map_err(|_| ApplyFsError::PathRejected)?;
        validate_write_path_policy(&metadata.path).map_err(|_| ApplyFsError::PathRejected)?;

        // First pre-check of mode conditions + base digest.
        precheck_target(&root, metadata, sealed)?;

        if cancellation.is_cancelled() {
            return Err(ApplyFsError::Cancelled);
        }

        let target = target_path(&root, &metadata.path).map_err(|_| ApplyFsError::PathRejected)?;
        let parent = target
            .parent()
            .ok_or(ApplyFsError::PathRejected)?
            .to_path_buf();

        let temp_name = format!(
            ".{}.pillow-tmp-{}",
            target
                .file_name()
                .and_then(|s| s.to_str())
                .unwrap_or("target"),
            Uuid::new_v4()
        );
        let temp_path = parent.join(&temp_name);

        // create_new temp in same directory.
        let write_result = (|| -> Result<String, ApplyFsError> {
            let mut file = OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&temp_path)
                .map_err(|_| ApplyFsError::WriteFailed)?;
            file.write_all(sealed.proposed_content().as_bytes())
                .map_err(|_| ApplyFsError::WriteFailed)?;
            file.flush().map_err(|_| ApplyFsError::WriteFailed)?;
            file.sync_all().map_err(|_| ApplyFsError::WriteFailed)?;
            drop(file);

            if cancellation.is_cancelled() {
                return Err(ApplyFsError::Cancelled);
            }

            // Final revalidation immediately before OS atomic operation.
            revalidate_root(capability).map_err(|_| ApplyFsError::PathRejected)?;
            validate_write_path_policy(&metadata.path).map_err(|_| ApplyFsError::PathRejected)?;
            precheck_target(&root, metadata, sealed)?;

            // Re-resolve every existing parent component + target kind.
            revalidate_parents_and_target(&root, &metadata.path, metadata.mode)?;

            let base_digest_for_inspect = metadata.base_digest.clone();
            match metadata.mode {
                WriteMode::Create => atomic_create(&temp_path, &target)?,
                WriteMode::Modify => atomic_replace_modify(
                    &temp_path,
                    &target,
                    &root,
                    metadata,
                    &base_digest_for_inspect,
                )?,
            }

            // Post-write evidence: reopen through the same root/reparse/regular-file
            // checks and require on-disk UTF-8 bytes/digest to match durable metadata.
            // Never return a digest computed only from sealed memory.
            verify_post_write_target(&root, metadata)
        })();

        match &write_result {
            Ok(_) => {
                // Temp is consumed by replace/create; best-effort cleanup if it remains.
                let _ = fs::remove_file(&temp_path);
            }
            Err(_) => {
                let _ = fs::remove_file(&temp_path);
            }
        }
        write_result
    }
}

#[derive(Debug)]
enum ApplyFsError {
    Cancelled,
    BaseChanged,
    PathRejected,
    WriteFailed,
}

/// SHA-256 digest tagged as `sha256:` + lowercase hex.
pub fn content_sha256_digest(content: &str) -> String {
    let digest = Sha256::digest(content.as_bytes());
    let hex = digest
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect::<String>();
    format!("sha256:{hex}")
}

fn validate_proposed_content(content: &str) -> Result<(), GovernedWriteError> {
    let bytes = content.as_bytes();
    if bytes.is_empty() || bytes.len() as u64 > WRITE_CONTENT_BYTE_CAP {
        return Err(GovernedWriteError::BaseCaptureFailed);
    }
    if bytes.contains(&0) {
        return Err(GovernedWriteError::BaseCaptureFailed);
    }
    Ok(())
}

fn verify_sealed_against_metadata(
    metadata: &StoredWriteProposal,
    sealed: &SealedWriteContents,
) -> Result<(), GovernedWriteError> {
    validate_proposed_content(sealed.proposed_content())?;
    let proposed_digest = content_sha256_digest(sealed.proposed_content());
    if proposed_digest != metadata.proposed_digest
        || sealed.proposed_content().len() as u64 != metadata.proposed_bytes
    {
        return Err(GovernedWriteError::DigestMismatch);
    }
    match metadata.mode {
        WriteMode::Create => {
            if sealed.base_content().is_some()
                || metadata.base_digest.is_some()
                || metadata.base_bytes != 0
            {
                return Err(GovernedWriteError::DigestMismatch);
            }
        }
        WriteMode::Modify => {
            let Some(base) = sealed.base_content() else {
                return Err(GovernedWriteError::DigestMismatch);
            };
            let base_digest = content_sha256_digest(base);
            if Some(base_digest.as_str()) != metadata.base_digest.as_deref()
                || base.len() as u64 != metadata.base_bytes
            {
                return Err(GovernedWriteError::DigestMismatch);
            }
        }
    }
    Ok(())
}

fn validate_write_path_policy(path: &str) -> Result<(), GovernedWriteError> {
    if path == "." || path.is_empty() {
        return Err(GovernedWriteError::PathRejected);
    }
    // Portable grammar (mirror protocol rules used for project-relative files).
    if path.len() > 4_096 {
        return Err(GovernedWriteError::PathRejected);
    }
    if path.starts_with('/') || path.starts_with('\\') || path.contains('\\') || path.contains(':')
    {
        return Err(GovernedWriteError::PathRejected);
    }
    let components: Vec<&str> = path.split('/').collect();
    if components.is_empty() {
        return Err(GovernedWriteError::PathRejected);
    }
    for component in &components {
        if component.is_empty() || *component == "." || *component == ".." {
            return Err(GovernedWriteError::PathRejected);
        }
        if component.len() > 255 {
            return Err(GovernedWriteError::PathRejected);
        }
        if component.ends_with('.') || component.ends_with(' ') {
            return Err(GovernedWriteError::PathRejected);
        }
        if component.chars().any(|character| {
            character.is_control() || matches!(character, '<' | '>' | '"' | '|' | '?' | '*')
        }) {
            return Err(GovernedWriteError::PathRejected);
        }
        if is_windows_device_component(component) {
            return Err(GovernedWriteError::PathRejected);
        }
        if is_denied_component(component) {
            return Err(GovernedWriteError::PathRejected);
        }
    }
    let leaf = *components.last().unwrap();
    if is_denied_leaf(leaf) {
        return Err(GovernedWriteError::PathRejected);
    }
    if is_denied_extension(leaf) {
        return Err(GovernedWriteError::PathRejected);
    }
    Ok(())
}

fn is_denied_component(component: &str) -> bool {
    DENIED_COMPONENTS
        .iter()
        .any(|denied| component.eq_ignore_ascii_case(denied))
}

fn is_denied_leaf(leaf: &str) -> bool {
    // .env.example is explicitly allowed.
    if leaf.eq_ignore_ascii_case(".env.example") {
        return false;
    }
    DENIED_LEAVES
        .iter()
        .any(|denied| leaf.eq_ignore_ascii_case(denied))
}

fn is_denied_extension(leaf: &str) -> bool {
    let lower = leaf.to_ascii_lowercase();
    DENIED_EXTENSIONS.iter().any(|ext| lower.ends_with(ext))
}

fn is_windows_device_component(component: &str) -> bool {
    let stem = component
        .split('.')
        .next()
        .unwrap_or(component)
        .trim_end_matches([' ', '.'])
        .to_ascii_uppercase();
    if matches!(
        stem.as_str(),
        "CON" | "PRN" | "AUX" | "NUL" | "CLOCK$" | "CONIN$" | "CONOUT$"
    ) {
        return true;
    }
    let bytes = stem.as_bytes();
    (bytes.len() == 4 && matches!(&bytes[..3], b"COM" | b"LPT") && matches!(bytes[3], b'1'..=b'9'))
        || matches!(
            stem.as_str(),
            "COM\u{00b9}"
                | "COM\u{00b2}"
                | "COM\u{00b3}"
                | "LPT\u{00b9}"
                | "LPT\u{00b2}"
                | "LPT\u{00b3}"
        )
}

fn revalidate_root(capability: &ProjectCapability) -> Result<PathBuf, GovernedWriteError> {
    let root = capability
        .canonical_root()
        .canonicalize()
        .map_err(|_| GovernedWriteError::InvalidProjectRoot)?;
    if root != *capability.canonical_root() || !root.is_dir() {
        return Err(GovernedWriteError::InvalidProjectRoot);
    }
    Ok(root)
}

fn target_path(root: &Path, relative: &str) -> Result<PathBuf, GovernedWriteError> {
    // Build lexical target without requiring the leaf to exist.
    let mut current = root.to_owned();
    let components: Vec<&str> = relative.split('/').collect();
    for (index, component) in components.iter().enumerate() {
        current.push(component);
        let is_last = index + 1 == components.len();
        if is_last {
            break;
        }
        // Intermediate components must exist as real directories (no reparse).
        let metadata = current
            .symlink_metadata()
            .map_err(|_| GovernedWriteError::PathRejected)?;
        if metadata.file_type().is_symlink() || is_reparse_point(&metadata) {
            return Err(GovernedWriteError::PathRejected);
        }
        if !metadata.is_dir() {
            return Err(GovernedWriteError::PathRejected);
        }
        let canonical = current
            .canonicalize()
            .map_err(|_| GovernedWriteError::PathRejected)?;
        if !canonical.starts_with(root) {
            return Err(GovernedWriteError::PathRejected);
        }
        current = canonical;
    }
    Ok(current)
}

fn path_exists_under(root: &Path, relative: &str) -> Result<bool, GovernedWriteError> {
    let target = target_path(root, relative)?;
    Ok(target.symlink_metadata().is_ok())
}

fn ensure_parent_real_dir(root: &Path, relative: &str) -> Result<(), GovernedWriteError> {
    let target = target_path(root, relative)?;
    let parent = target.parent().ok_or(GovernedWriteError::PathRejected)?;
    if parent == root {
        // Root itself is already validated as a real directory.
        return Ok(());
    }
    let metadata = parent
        .symlink_metadata()
        .map_err(|_| GovernedWriteError::PathRejected)?;
    if metadata.file_type().is_symlink() || is_reparse_point(&metadata) || !metadata.is_dir() {
        return Err(GovernedWriteError::PathRejected);
    }
    let canonical = parent
        .canonicalize()
        .map_err(|_| GovernedWriteError::PathRejected)?;
    if !canonical.starts_with(root) {
        return Err(GovernedWriteError::PathRejected);
    }
    Ok(())
}

fn read_regular_utf8_file(root: &Path, relative: &str) -> Result<String, GovernedWriteError> {
    let target = target_path(root, relative)?;
    let metadata = target
        .symlink_metadata()
        .map_err(|_| GovernedWriteError::BaseCaptureFailed)?;
    if metadata.file_type().is_symlink() || is_reparse_point(&metadata) || !metadata.is_file() {
        return Err(GovernedWriteError::BaseCaptureFailed);
    }
    if metadata.len() > WRITE_CONTENT_BYTE_CAP {
        return Err(GovernedWriteError::BaseCaptureFailed);
    }
    let bytes = fs::read(&target).map_err(|_| GovernedWriteError::BaseCaptureFailed)?;
    if bytes.len() as u64 > WRITE_CONTENT_BYTE_CAP {
        return Err(GovernedWriteError::BaseCaptureFailed);
    }
    if bytes.contains(&0) {
        return Err(GovernedWriteError::BaseCaptureFailed);
    }
    String::from_utf8(bytes).map_err(|_| GovernedWriteError::BaseCaptureFailed)
}

fn precheck_target(
    root: &Path,
    metadata: &StoredWriteProposal,
    sealed: &SealedWriteContents,
) -> Result<(), ApplyFsError> {
    match metadata.mode {
        WriteMode::Create => {
            if path_exists_under(root, &metadata.path).map_err(|_| ApplyFsError::PathRejected)? {
                return Err(ApplyFsError::PathRejected);
            }
            ensure_parent_real_dir(root, &metadata.path).map_err(|_| ApplyFsError::PathRejected)?;
            Ok(())
        }
        WriteMode::Modify => {
            let on_disk = read_regular_utf8_file(root, &metadata.path)
                .map_err(|_| ApplyFsError::PathRejected)?;
            let on_disk_digest = content_sha256_digest(&on_disk);
            let expected = metadata
                .base_digest
                .as_deref()
                .ok_or(ApplyFsError::PathRejected)?;
            if on_disk_digest != expected {
                return Err(ApplyFsError::BaseChanged);
            }
            // Sealed base must still match (already verified, but recheck digest).
            if let Some(base) = sealed.base_content()
                && content_sha256_digest(base) != expected
            {
                return Err(ApplyFsError::BaseChanged);
            }
            Ok(())
        }
    }
}

fn revalidate_parents_and_target(
    root: &Path,
    relative: &str,
    mode: WriteMode,
) -> Result<(), ApplyFsError> {
    let target = target_path(root, relative).map_err(|_| ApplyFsError::PathRejected)?;
    match mode {
        WriteMode::Create => {
            if target.symlink_metadata().is_ok() {
                return Err(ApplyFsError::PathRejected);
            }
            ensure_parent_real_dir(root, relative).map_err(|_| ApplyFsError::PathRejected)?;
        }
        WriteMode::Modify => {
            let metadata = target
                .symlink_metadata()
                .map_err(|_| ApplyFsError::PathRejected)?;
            if metadata.file_type().is_symlink()
                || is_reparse_point(&metadata)
                || !metadata.is_file()
            {
                return Err(ApplyFsError::PathRejected);
            }
        }
    }
    Ok(())
}

/// Reopen the target after a successful OS atomic op and require on-disk bytes
/// and digest to match durable proposed metadata.
fn verify_post_write_target(
    root: &Path,
    metadata: &StoredWriteProposal,
) -> Result<String, ApplyFsError> {
    let on_disk =
        read_regular_utf8_file(root, &metadata.path).map_err(|_| ApplyFsError::WriteFailed)?;
    if on_disk.len() as u64 != metadata.proposed_bytes {
        return Err(ApplyFsError::WriteFailed);
    }
    let actual_digest = content_sha256_digest(&on_disk);
    if actual_digest != metadata.proposed_digest {
        return Err(ApplyFsError::WriteFailed);
    }
    Ok(actual_digest)
}

/// Read-only inspect after an ambiguous modify outcome.
///
/// - exact proposed match → treat as success (caller verifies again)
/// - still base → write_failed
/// - otherwise → write_failed (opaque)
fn inspect_modify_after_ambiguous(
    root: &Path,
    metadata: &StoredWriteProposal,
    base_digest: Option<&str>,
) -> Result<(), ApplyFsError> {
    let on_disk = match read_regular_utf8_file(root, &metadata.path) {
        Ok(text) => text,
        Err(_) => return Err(ApplyFsError::WriteFailed),
    };
    let actual = content_sha256_digest(&on_disk);
    if actual == metadata.proposed_digest && on_disk.len() as u64 == metadata.proposed_bytes {
        // Replacement may have succeeded despite the API error; post-write verify
        // will confirm again before any success receipt.
        return Ok(());
    }
    if let Some(base) = base_digest
        && actual == base
    {
        return Err(ApplyFsError::WriteFailed);
    }
    Err(ApplyFsError::WriteFailed)
}

#[cfg(windows)]
fn atomic_create(temp: &Path, target: &Path) -> Result<(), ApplyFsError> {
    // MoveFileExW without MOVEFILE_REPLACE_EXISTING; with WRITE_THROUGH.
    // Create failures remain failures even if a racer created identical content.
    const MOVEFILE_WRITE_THROUGH: u32 = 0x8;

    let temp_w = wide_path(temp);
    let target_w = wide_path(target);
    let ok = unsafe {
        windows_sys::Win32::Storage::FileSystem::MoveFileExW(
            temp_w.as_ptr(),
            target_w.as_ptr(),
            MOVEFILE_WRITE_THROUGH,
        )
    };
    if ok == 0 {
        return Err(ApplyFsError::WriteFailed);
    }
    Ok(())
}

#[cfg(windows)]
fn atomic_replace_modify(
    temp: &Path,
    target: &Path,
    root: &Path,
    metadata: &StoredWriteProposal,
    base_digest: &Option<String>,
) -> Result<(), ApplyFsError> {
    // ReplaceFileW with WRITE_THROUGH. Never unlink target first. Never retry.
    const REPLACEFILE_WRITE_THROUGH: u32 = 0x1;

    let target_w = wide_path(target);
    let temp_w = wide_path(temp);
    let ok = unsafe {
        windows_sys::Win32::Storage::FileSystem::ReplaceFileW(
            target_w.as_ptr(),
            temp_w.as_ptr(),
            std::ptr::null(),
            REPLACEFILE_WRITE_THROUGH,
            std::ptr::null_mut(),
            std::ptr::null_mut(),
        )
    };
    if ok != 0 {
        return Ok(());
    }
    // Ambiguous failure: read-only inspect. Never run a second replacement.
    inspect_modify_after_ambiguous(root, metadata, base_digest.as_deref())
}

#[cfg(windows)]
fn wide_path(path: &Path) -> Vec<u16> {
    use std::os::windows::ffi::OsStrExt;
    path.as_os_str()
        .encode_wide()
        .chain(std::iter::once(0))
        .collect()
}

#[cfg(not(windows))]
fn atomic_create(temp: &Path, target: &Path) -> Result<(), ApplyFsError> {
    // Atomic no-clobber create: hard_link fails with EEXIST if the target was
    // created between the absence check and this op. Never use replacing rename.
    match fs::hard_link(temp, target) {
        Ok(()) => {
            let _ = fs::remove_file(temp);
            Ok(())
        }
        Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {
            Err(ApplyFsError::PathRejected)
        }
        Err(_) => Err(ApplyFsError::WriteFailed),
    }
}

#[cfg(not(windows))]
fn atomic_replace_modify(
    temp: &Path,
    target: &Path,
    root: &Path,
    metadata: &StoredWriteProposal,
    base_digest: &Option<String>,
) -> Result<(), ApplyFsError> {
    // POSIX rename atomically replaces an existing file on the same filesystem.
    match fs::rename(temp, target) {
        Ok(()) => Ok(()),
        Err(_) => {
            // Conservative inspect if rename reports failure (rare).
            inspect_modify_after_ambiguous(root, metadata, base_digest.as_deref())
        }
    }
}

#[cfg(windows)]
fn is_reparse_point(metadata: &std::fs::Metadata) -> bool {
    use std::os::windows::fs::MetadataExt;
    const FILE_ATTRIBUTE_REPARSE_POINT: u32 = 0x400;
    metadata.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0
}

#[cfg(not(windows))]
fn is_reparse_point(_: &std::fs::Metadata) -> bool {
    false
}

// Silence unused import on non-windows where File may be unused in helpers.
#[allow(dead_code)]
fn _touch_file_type(_: &File) {}

#[cfg(test)]
mod post_write_verify_tests {
    use super::*;
    use keeper_agent_protocol::RunId;
    use std::fs;
    use tempfile::tempdir;

    fn metadata_for(path: &str, proposed: &str) -> StoredWriteProposal {
        let now = Utc::now();
        StoredWriteProposal {
            proposal_id: Uuid::new_v4().to_string(),
            reservation_key: "wp:test".into(),
            proposal_fingerprint: content_sha256_digest("fp"),
            project_id: "proj".into(),
            conversation_id: "conv".into(),
            run_id: RunId(Uuid::new_v4()),
            call_id: "call".into(),
            path: path.into(),
            mode: WriteMode::Create,
            proposed_digest: content_sha256_digest(proposed),
            proposed_bytes: proposed.len() as u64,
            base_digest: None,
            base_bytes: 0,
            state: WriteProposalState::Applying,
            expires_at: now,
            result_json: None,
            created_at: now,
            updated_at: now,
        }
    }

    #[test]
    fn verify_post_write_accepts_exact_on_disk_bytes_and_digest() {
        let temp = tempdir().unwrap();
        let root = temp.path();
        let proposed = "exact-proposed-content\n";
        let metadata = metadata_for("ok.txt", proposed);
        fs::write(root.join("ok.txt"), proposed).unwrap();

        let digest = verify_post_write_target(root, &metadata).unwrap();
        assert_eq!(digest, metadata.proposed_digest);
        assert_eq!(digest, content_sha256_digest(proposed));
    }

    #[test]
    fn verify_post_write_rejects_byte_length_mismatch() {
        let temp = tempdir().unwrap();
        let root = temp.path();
        let proposed = "expected-len-content\n";
        let metadata = metadata_for("len.txt", proposed);
        fs::write(root.join("len.txt"), "short\n").unwrap();

        let err = verify_post_write_target(root, &metadata).unwrap_err();
        assert!(matches!(err, ApplyFsError::WriteFailed));
    }

    #[test]
    fn verify_post_write_rejects_digest_mismatch_same_length() {
        let temp = tempdir().unwrap();
        let root = temp.path();
        let proposed = "ABCDEFGHIJ\n";
        let metadata = metadata_for("dig.txt", proposed);
        // Same length, different bytes → digest mismatch after length check passes.
        assert_eq!(proposed.len(), "0123456789\n".len());
        fs::write(root.join("dig.txt"), "0123456789\n").unwrap();

        let err = verify_post_write_target(root, &metadata).unwrap_err();
        assert!(matches!(err, ApplyFsError::WriteFailed));
    }

    #[test]
    fn verify_post_write_rejects_missing_or_unreadable_target() {
        let temp = tempdir().unwrap();
        let root = temp.path();
        let proposed = "missing-target\n";
        let metadata = metadata_for("gone.txt", proposed);

        let err = verify_post_write_target(root, &metadata).unwrap_err();
        assert!(matches!(err, ApplyFsError::WriteFailed));
    }
}
