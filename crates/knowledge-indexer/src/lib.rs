//! Knowledge document indexer (K0b/K0c/K0d/K0e/K1a/K1b): classify, extract
//! explicit supersession, register into K0a tables, audit-only reports,
//! registry status + composite handoff rendering, ledger CLI helpers, stale
//! detection, and retirement **candidates**.
//!
//! **Report-only for docs lifecycle:** never deletes or moves files. Never
//! auto-archives (K1a: archive is an explicit storage call). Never auto-migrates
//! task status beyond explicit CLI commands. Path grammar is always
//! [`keeper_project::validate_portable_path`] — never reimplemented here.
//!
//! **K0c render** is a pure projection of registry rows + audit buckets — never
//! hand-written project narrative. **Default render excludes `archived`**
//! (ADR-0004 §5); pass `include_archived: true` only when intentionally
//! recovering inventory.
//!
//! **K1b handoff render** (`render_handoff_view`) is ops-first (在途 / 待验收 /
//! 验收结论 / 文档状态) and **states what it still cannot replace**.
//!
//! **K1a retirement candidates** require an explicit supersedes link whose
//! target is registered. Zero-inbound is never a retirement signal.
//!
//! **K0e ledger CLI** records dispatch/acceptance as a byproduct (ADR-0004 §2)
//! and detects handoff-vs-ledger staleness without rewriting documents.

pub mod audit;
pub mod classify;
pub mod digest;
pub mod ledger;
pub mod librarian_audit;
pub mod read_only_ledger;
pub mod render;
pub mod scan;
pub mod stale;
pub mod supersession;

pub use audit::{
    AuditFinding, AuditReport, RetirementBucket, audit_documents, format_audit_report,
};
pub use classify::{
    Classification, ClassifiedDoc, classify_document, classify_role, classify_status_from_content,
};
pub use digest::content_digest_sha256;
pub use ledger::{
    CompleteInput, DispatchInput, LedgerCliError, LedgerDbOptions, LedgerSnapshot, VerdictInput,
    dispatch_task, ensure_project_id, format_ledger_status, format_task_recorded,
    format_verdict_recorded, load_snapshot, open_knowledge, record_verdict, resolve_project_id,
    set_task_status,
};
pub use librarian_audit::{
    GitLogError, LibrarianAuditError, LibrarianAuditInput, LibrarianAuditReport, LibrarianCheck,
    LibrarianCheckKind, LibrarianCheckOutcome, format_librarian_audit_report, load_git_commit_ids,
    run_librarian_audit,
};
pub use read_only_ledger::{ReadOnlyLedgerError, load_snapshot_read_only};
pub use render::{RenderInput, render_handoff_view, render_status_view};
pub use scan::{ScanOptions, ScanResult, ScanSummary, scan_and_register, validate_path_ssot};
pub use stale::{
    StaleFinding, StaleKind, StaleReport, TaskMention, check_stale, extract_inflight_claims,
    extract_task_mentions, format_stale_report,
};
pub use supersession::{
    ExplicitSupersession, InferredSupersession, SupersessionExtract, extract_supersession,
};
