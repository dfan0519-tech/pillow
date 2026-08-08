//! Knowledge document indexer (K0b/K0c): classify, extract explicit supersession,
//! register into K0a tables, audit-only reports, and registry status rendering.
//!
//! **Report-only:** never deletes, archives, or moves documents. Never auto-
//! migrates status beyond explicit scan rules. Path grammar is always
//! [`keeper_project::validate_portable_path`] — never reimplemented here.
//!
//! **K0c render** is a pure projection of registry rows + audit buckets — never
//! hand-written project narrative.

pub mod audit;
pub mod classify;
pub mod digest;
pub mod render;
pub mod scan;
pub mod supersession;

pub use audit::{
    AuditFinding, AuditReport, RetirementBucket, audit_documents, format_audit_report,
};
pub use classify::{
    Classification, ClassifiedDoc, classify_document, classify_role, classify_status_from_content,
};
pub use digest::content_digest_sha256;
pub use render::{RenderInput, render_status_view};
pub use scan::{ScanOptions, ScanResult, ScanSummary, scan_and_register, validate_path_ssot};
pub use supersession::{
    ExplicitSupersession, InferredSupersession, SupersessionExtract, extract_supersession,
};
