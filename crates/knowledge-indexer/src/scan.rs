//! Directory walk → classify → upsert into K0a → audit.
//!
//! Idempotent on `(project_id, path)`. Rescan updates digest but **never**
//! overwrites a non-`unknown` status already stored.

use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};

use keeper_project::{CreateProject, validate_portable_path};
use keeper_storage_sqlite::{
    AddDocumentLink, DocumentLinkRel, DocumentRole, DocumentStatus, KnowledgeDocument,
    KnowledgeDocumentLink, KnowledgeError, RegisterDocument, RegisteredBy,
    SqliteKnowledgeRepository, SqliteProjectRepository, UpdateDocument,
};

use crate::audit::{AuditInput, AuditReport, audit_documents, format_audit_report};
use crate::classify::{
    classify_document, is_brief_like_path, is_report_like_path, pairing_stem,
    status_for_paired_brief,
};
use crate::digest::content_digest_sha256;
use crate::render::{RenderInput, render_status_view};
use crate::supersession::{InferredSupersession, extract_supersession};

/// Options for a scan run.
#[derive(Debug, Clone)]
pub struct ScanOptions {
    /// Repository root (contains `docs/`).
    pub repo_root: PathBuf,
    /// SQLite database path (temporary or durable).
    pub db_path: PathBuf,
    /// Project name for the FK row.
    pub project_name: String,
    /// Who is registering (default lint for scanner).
    pub registered_by: RegisteredBy,
    /// Optional run id for provenance.
    pub source_run_id: Option<String>,
    /// Relative docs root under repo (default `docs`).
    pub docs_relative: String,
    /// Portable paths to skip (e.g. this run's own audit output, so a prior
    /// report cannot self-cite every basename and collapse orphan counts).
    pub exclude_paths: Vec<String>,
}

impl Default for ScanOptions {
    fn default() -> Self {
        Self {
            repo_root: PathBuf::from("."),
            db_path: PathBuf::from("knowledge-scan.db"),
            project_name: "pillow".into(),
            registered_by: RegisteredBy::Lint,
            source_run_id: None,
            docs_relative: "docs".into(),
            exclude_paths: Vec::new(),
        }
    }
}

/// Summary counters from a scan.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ScanSummary {
    pub files_seen: usize,
    pub registered_new: usize,
    pub updated_existing: usize,
    pub links_added: usize,
    pub links_skipped_existing: usize,
    pub digest_changed: usize,
    pub status_preserved_non_unknown: usize,
}

/// Full scan result.
#[derive(Debug)]
pub struct ScanResult {
    pub project_id: String,
    pub summary: ScanSummary,
    pub documents: Vec<KnowledgeDocument>,
    pub links: Vec<KnowledgeDocumentLink>,
    pub report: AuditReport,
    pub report_markdown: String,
    /// K0c pure registry status view (no hand-written narrative).
    pub handoff_markdown: String,
    pub inferred_supersession: Vec<InferredSupersession>,
}

#[derive(Debug, thiserror::Error)]
pub enum ScanError {
    #[error("docs directory not found: {0}")]
    DocsMissing(String),
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
    #[error("path not portable: {path}: {detail}")]
    InvalidPath { path: String, detail: String },
    #[error("knowledge store: {0}")]
    Knowledge(#[from] KnowledgeError),
    #[error("project store error")]
    Project,
}

/// Walk `docs/**/*.md`, register, apply explicit supersession, audit.
pub fn scan_and_register(options: &ScanOptions) -> Result<ScanResult, ScanError> {
    let docs_root = options.repo_root.join(&options.docs_relative);
    if !docs_root.is_dir() {
        return Err(ScanError::DocsMissing(docs_root.display().to_string()));
    }

    // Ensure project row exists (FK).
    let project_id = ensure_project(options)?;
    let repo = SqliteKnowledgeRepository::open(&options.db_path)?;

    // Collect markdown files.
    let mut md_files = Vec::new();
    collect_markdown(
        &docs_root,
        &docs_root,
        &options.docs_relative,
        &mut md_files,
    )?;
    if !options.exclude_paths.is_empty() {
        let exclude: HashSet<&str> = options.exclude_paths.iter().map(String::as_str).collect();
        md_files.retain(|f| !exclude.contains(f.portable_path.as_str()));
    }
    md_files.sort_by(|a, b| a.portable_path.cmp(&b.portable_path));

    let mut summary = ScanSummary {
        files_seen: md_files.len(),
        ..ScanSummary::default()
    };

    let mut contents: HashMap<String, String> = HashMap::new();
    let mut path_content_conflicts: HashMap<String, bool> = HashMap::new();
    let mut classify_notes: HashMap<String, Vec<String>> = HashMap::new();
    let mut digest_changes: Vec<(String, String, String)> = Vec::new();
    let mut all_inferred: Vec<InferredSupersession> = Vec::new();

    // Pass 1: classify + upsert documents (provisional status).
    for file in &md_files {
        validate_portable_path(&file.portable_path).map_err(|detail| ScanError::InvalidPath {
            path: file.portable_path.clone(),
            detail: detail.to_owned(),
        })?;

        let bytes = fs::read(&file.abs_path)?;
        let text = String::from_utf8_lossy(&bytes).into_owned();
        let digest = content_digest_sha256(&bytes);
        let classification = classify_document(&file.portable_path, &text);

        contents.insert(file.portable_path.clone(), text.clone());
        path_content_conflicts.insert(
            file.portable_path.clone(),
            classification.path_content_conflict,
        );
        classify_notes.insert(file.portable_path.clone(), classification.notes.clone());

        match repo.get_by_path(&project_id, &file.portable_path)? {
            None => {
                repo.register(RegisterDocument {
                    project_id: project_id.clone(),
                    path: file.portable_path.clone(),
                    role: classification.role,
                    status: Some(classification.status),
                    content_digest: digest,
                    registered_by: options.registered_by,
                    source_run_id: options.source_run_id.clone(),
                })?;
                summary.registered_new += 1;
            }
            Some(existing) => {
                let mut status_update: Option<DocumentStatus> = None;
                if existing.status == DocumentStatus::Unknown {
                    // May refine unknown → stronger explicit status.
                    if classification.status != DocumentStatus::Unknown {
                        status_update = Some(classification.status);
                    }
                } else {
                    // Preserve human/controller non-unknown status.
                    summary.status_preserved_non_unknown += 1;
                }

                if existing.content_digest != digest {
                    digest_changes.push((
                        file.portable_path.clone(),
                        existing.content_digest.clone(),
                        digest.clone(),
                    ));
                    summary.digest_changed += 1;
                }

                repo.update(UpdateDocument {
                    document_id: existing.document_id,
                    role: Some(classification.role),
                    status: status_update,
                    content_digest: Some(digest),
                    registered_by: Some(options.registered_by),
                    source_run_id: Some(options.source_run_id.clone()),
                })?;
                summary.updated_existing += 1;
            }
        }

        let extract = extract_supersession(&file.portable_path, &text);
        all_inferred.extend(extract.inferred);
    }

    // Pass 2: brief pairing → consumed (only if still unknown).
    apply_brief_pairing(&repo, &project_id)?;

    // Pass 3: explicit supersession links + mark targets superseded when unknown/allowed.
    let link_stats =
        apply_explicit_supersession(&repo, &project_id, &contents, options.registered_by)?;
    summary.links_added = link_stats.0;
    summary.links_skipped_existing = link_stats.1;

    // Reload registry.
    let documents = repo.list_by_project(&project_id)?;
    let mut links = Vec::new();
    for doc in &documents {
        links.extend(repo.list_links_from(&doc.document_id)?);
    }

    let report = audit_documents(AuditInput {
        documents: documents.clone(),
        links: links.clone(),
        contents,
        path_content_conflicts,
        classify_notes,
        digest_changes,
        inferred_supersession: all_inferred.clone(),
    });
    let report_markdown = format_audit_report(&report);
    let handoff_markdown = render_status_view(&RenderInput {
        documents: documents.clone(),
        links: links.clone(),
        audit: report.clone(),
    });

    Ok(ScanResult {
        project_id,
        summary,
        documents,
        links,
        report,
        report_markdown,
        handoff_markdown,
        inferred_supersession: all_inferred,
    })
}

fn ensure_project(options: &ScanOptions) -> Result<String, ScanError> {
    let projects =
        SqliteProjectRepository::open(&options.db_path).map_err(|_| ScanError::Project)?;
    // Reuse first project if present (idempotent rescan same DB).
    let existing = projects.list().map_err(|_| ScanError::Project)?;
    if let Some(p) = existing.into_iter().next() {
        return Ok(p.id.0.to_string());
    }
    let root = options
        .repo_root
        .canonicalize()
        .unwrap_or_else(|_| options.repo_root.clone());
    let project = projects
        .create(CreateProject {
            name: options.project_name.clone(),
            root_path: root.to_string_lossy().into_owned(),
        })
        .map_err(|_| ScanError::Project)?;
    Ok(project.id.0.to_string())
}

struct MdFile {
    abs_path: PathBuf,
    portable_path: String,
}

fn collect_markdown(
    abs_dir: &Path,
    docs_root: &Path,
    docs_relative: &str,
    out: &mut Vec<MdFile>,
) -> Result<(), ScanError> {
    let entries = fs::read_dir(abs_dir)?;
    for entry in entries {
        let entry = entry?;
        let path = entry.path();
        let name = entry.file_name();
        let name_str = name.to_string_lossy();
        if name_str.starts_with('.') {
            continue;
        }
        if path.is_dir() {
            collect_markdown(&path, docs_root, docs_relative, out)?;
            continue;
        }
        if path.extension().and_then(|e| e.to_str()) != Some("md") {
            continue;
        }
        let rel_under_docs = path
            .strip_prefix(docs_root)
            .map_err(|_| ScanError::Io(std::io::Error::other("strip_prefix failed")))?;
        let rel = rel_under_docs.to_string_lossy().replace('\\', "/");
        let portable = format!("{docs_relative}/{rel}");
        // Skip non-portable names rather than aborting whole scan? Task says path SSoT —
        // invalid portable paths should surface. Collect and fail on register.
        out.push(MdFile {
            abs_path: path,
            portable_path: portable,
        });
    }
    Ok(())
}

fn apply_brief_pairing(
    repo: &SqliteKnowledgeRepository,
    project_id: &str,
) -> Result<(), ScanError> {
    let documents = repo.list_by_project(project_id)?;
    let report_stems: HashSet<String> = documents
        .iter()
        .filter(|d| matches!(d.role, DocumentRole::Report) || is_report_like_path(&d.path))
        .filter_map(|d| pairing_stem(&d.path))
        .collect();

    for doc in documents {
        if !(matches!(doc.role, DocumentRole::Brief) || is_brief_like_path(&doc.path)) {
            continue;
        }
        let Some(stem) = pairing_stem(&doc.path) else {
            continue;
        };
        let has_report = report_stems.contains(&stem);
        let new_status = status_for_paired_brief(doc.status, has_report);
        if new_status != doc.status {
            // Only transitions unknown → consumed.
            if doc.status == DocumentStatus::Unknown {
                repo.update(UpdateDocument {
                    document_id: doc.document_id,
                    role: None,
                    status: Some(new_status),
                    content_digest: None,
                    registered_by: None,
                    source_run_id: None,
                })?;
            }
        }
    }
    Ok(())
}

fn apply_explicit_supersession(
    repo: &SqliteKnowledgeRepository,
    project_id: &str,
    contents: &HashMap<String, String>,
    _registered_by: RegisteredBy,
) -> Result<(usize, usize), ScanError> {
    let documents = repo.list_by_project(project_id)?;
    let by_path: HashMap<String, KnowledgeDocument> = documents
        .iter()
        .map(|d| (d.path.clone(), d.clone()))
        .collect();

    let mut added = 0usize;
    let mut skipped = 0usize;

    for doc in &documents {
        let Some(text) = contents.get(&doc.path) else {
            continue;
        };
        let extract = extract_supersession(&doc.path, text);
        let existing_links = repo.list_links_from(&doc.document_id)?;

        for edge in extract.explicit {
            // Resolve target document if registered.
            let to_doc = by_path.get(&edge.to_path);
            let to_document_id = to_doc.map(|d| d.document_id.clone());
            let to_path = Some(edge.to_path.clone());

            // Path-shaped targets must pass SSoT before storage (skip bad extracts).
            if edge.target_is_path_shaped && validate_portable_path(&edge.to_path).is_err() {
                // Do not write invalid portable paths; leave for report via inferred.
                continue;
            }

            // Idempotent: skip if same rel + same endpoint already present.
            let already = existing_links.iter().any(|l| {
                l.rel == DocumentLinkRel::Supersedes
                    && (to_document_id
                        .as_ref()
                        .is_some_and(|id| l.to_document_id.as_ref() == Some(id))
                        || l.to_path.as_deref() == Some(edge.to_path.as_str()))
            });
            if already {
                skipped += 1;
                continue;
            }

            match repo.add_link(AddDocumentLink {
                from_document_id: doc.document_id.clone(),
                to_document_id: to_document_id.clone(),
                to_path,
                rel: DocumentLinkRel::Supersedes,
            }) {
                Ok(_) => added += 1,
                Err(KnowledgeError::InvalidPath { .. }) => {
                    // Skip non-portable free-text that storage still rejected.
                    continue;
                }
                Err(err) => return Err(err.into()),
            }

            // Mark target superseded when still unknown. Never overwrite other
            // non-unknown statuses (controller judgment).
            if let Some(target) = to_doc
                && target.status == DocumentStatus::Unknown
            {
                repo.update(UpdateDocument {
                    document_id: target.document_id.clone(),
                    role: None,
                    status: Some(DocumentStatus::Superseded),
                    content_digest: None,
                    registered_by: None,
                    source_run_id: None,
                })?;
            }
        }
    }

    Ok((added, skipped))
}

/// Validate a path via SSoT only — used by tests to prove no local reimplementation.
pub fn validate_path_ssot(path: &str) -> Result<(), &'static str> {
    validate_portable_path(path)
}
