//! Audit-only findings. Never deletes, auto-archives, or retires documents.
//!
//! Hard policies from the 2026-08-08 inventory audit + ADR-0004 §5 / K1a:
//! - `verification` / `decision` zero-inbound → **expected orphans, not problems**
//! - orphan briefs → **待复核**, never retirement candidates
//! - zero-inbound is **not** a retirement signal
//! - retirement candidates = targets of **explicit** supersedes links whose
//!   superseder is registered **and** the target document is registered
//!   (`to_document_id` resolved). Scanner only produces this list.

use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};

use keeper_storage_sqlite::{
    DocumentLinkRel, DocumentRole, DocumentStatus, KnowledgeDocument, KnowledgeDocumentLink,
};

use crate::classify::{is_brief_like_path, is_report_like_path, pairing_stem};
use crate::supersession::InferredSupersession;

/// One audit finding line.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AuditFinding {
    pub path: String,
    pub detail: String,
}

/// Retirement / review buckets — reporting only (never applied automatically).
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct RetirementBucket {
    /// Targets of explicit supersedes links with registered endpoints.
    /// Same population as [`retirement_candidates`](Self::retirement_candidates)
    /// (alias kept for K0b report sections).
    pub explicit_superseded_candidates: Vec<String>,
    /// Briefs with no paired report — review, not retire.
    pub briefs_pending_review: Vec<String>,
    /// Expected orphans (verification/decision) — labeled non-problem.
    pub expected_orphans_non_problem: Vec<String>,
    /// Archive candidates only: explicit supersession + registered superseder
    /// and registered target. Zero-inbound alone never appears here.
    pub retirement_candidates: Vec<String>,
}

/// Full audit report structure.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct AuditReport {
    pub total_registered: usize,
    pub role_counts: BTreeMap<String, usize>,
    pub status_counts: BTreeMap<String, usize>,
    pub unknown_ratio: f64,
    /// §1 Contradictions (multiple non-superseded “current-like” in same dir).
    pub contradictions: Vec<AuditFinding>,
    /// §2 Orphans (zero text-inbound), grouped by role.
    pub orphans_by_role: BTreeMap<String, Vec<String>>,
    /// §3 Unpaired briefs.
    pub unpaired_briefs: Vec<String>,
    /// §4 Superseded but still referenced by other docs’ text.
    pub superseded_still_referenced: Vec<AuditFinding>,
    /// §5 Unclassifiable / path-content conflict.
    pub unclassifiable: Vec<AuditFinding>,
    /// §6 Digest changes on rescan.
    pub digest_changes: Vec<AuditFinding>,
    /// Inferred supersession (not in DB).
    pub inferred_supersession: Vec<InferredSupersession>,
    pub retirement: RetirementBucket,
}

/// Inputs needed beyond the registry rows.
#[derive(Debug, Clone)]
pub struct AuditInput {
    pub documents: Vec<KnowledgeDocument>,
    pub links: Vec<KnowledgeDocumentLink>,
    /// path → full file content (for text-inbound and conflict notes).
    pub contents: HashMap<String, String>,
    /// path → classification notes / conflict flag from classify pass.
    pub path_content_conflicts: HashMap<String, bool>,
    pub classify_notes: HashMap<String, Vec<String>>,
    pub digest_changes: Vec<(String, String, String)>, // path, old, new
    pub inferred_supersession: Vec<InferredSupersession>,
}

/// Build an audit report from registered documents + raw contents.
pub fn audit_documents(input: AuditInput) -> AuditReport {
    let mut report = AuditReport {
        total_registered: input.documents.len(),
        ..AuditReport::default()
    };

    // Distributions.
    for doc in &input.documents {
        *report
            .role_counts
            .entry(doc.role.as_str().to_owned())
            .or_insert(0) += 1;
        *report
            .status_counts
            .entry(doc.status.as_str().to_owned())
            .or_insert(0) += 1;
    }
    let unknown = report.status_counts.get("unknown").copied().unwrap_or(0);
    report.unknown_ratio = if report.total_registered == 0 {
        0.0
    } else {
        unknown as f64 / report.total_registered as f64
    };

    // Text-inbound: other docs’ raw text contains basename without .md.
    let inbound = compute_text_inbound(&input.documents, &input.contents);

    // §2 Orphans by role.
    for doc in &input.documents {
        let count = inbound.get(&doc.path).copied().unwrap_or(0);
        if count == 0 {
            report
                .orphans_by_role
                .entry(doc.role.as_str().to_owned())
                .or_default()
                .push(doc.path.clone());
        }
    }
    for list in report.orphans_by_role.values_mut() {
        list.sort();
    }

    // §3 Unpaired briefs.
    let report_stems: HashSet<String> = input
        .documents
        .iter()
        .filter(|d| matches!(d.role, DocumentRole::Report) || is_report_like_path(&d.path))
        .filter_map(|d| pairing_stem(&d.path))
        .collect();
    for doc in &input.documents {
        if !(matches!(doc.role, DocumentRole::Brief) || is_brief_like_path(&doc.path)) {
            continue;
        }
        if let Some(stem) = pairing_stem(&doc.path)
            && !report_stems.contains(&stem)
        {
            report.unpaired_briefs.push(doc.path.clone());
        }
    }
    report.unpaired_briefs.sort();

    // §1 Contradictions: same directory, multiple non-superseded handoffs/roadmaps.
    report.contradictions = find_contradictions(&input.documents);

    // §4 Superseded still referenced.
    for doc in &input.documents {
        if doc.status != DocumentStatus::Superseded {
            continue;
        }
        let refs = inbound.get(&doc.path).copied().unwrap_or(0);
        if refs > 0 {
            report.superseded_still_referenced.push(AuditFinding {
                path: doc.path.clone(),
                detail: format!("{refs} other document(s) still mention basename"),
            });
        }
    }

    // §5 Unclassifiable / conflicts.
    for doc in &input.documents {
        if doc.role == DocumentRole::Other {
            let notes = input
                .classify_notes
                .get(&doc.path)
                .map(|n| n.join("; "))
                .unwrap_or_else(|| "role=other".into());
            report.unclassifiable.push(AuditFinding {
                path: doc.path.clone(),
                detail: notes,
            });
        } else if input
            .path_content_conflicts
            .get(&doc.path)
            .copied()
            .unwrap_or(false)
        {
            let notes = input
                .classify_notes
                .get(&doc.path)
                .map(|n| n.join("; "))
                .unwrap_or_else(|| "path/content conflict".into());
            report.unclassifiable.push(AuditFinding {
                path: doc.path.clone(),
                detail: format!("path/content conflict: {notes}"),
            });
        }
    }
    report.unclassifiable.sort_by(|a, b| a.path.cmp(&b.path));

    // §6 Digest changes.
    for (path, old, new) in &input.digest_changes {
        report.digest_changes.push(AuditFinding {
            path: path.clone(),
            detail: format!("digest {old} → {new}"),
        });
    }

    report.inferred_supersession = input.inferred_supersession;

    // Retirement buckets (report-only policy — never auto-applied).
    report.retirement = build_retirement_buckets(&input.documents, &input.links, &report);

    report
}

fn build_retirement_buckets(
    documents: &[KnowledgeDocument],
    links: &[KnowledgeDocumentLink],
    report: &AuditReport,
) -> RetirementBucket {
    let mut bucket = RetirementBucket::default();

    // K1a: retirement candidates require explicit supersedes evidence with both
    // endpoints registered (from_document_id always FK; to_document_id Some).
    // Zero-inbound alone is never enough. Status==superseded alone is not
    // enough without a resolved supersedes link (avoids manual-status false
    // positives and free-path claims without a registered target).
    let id_to_path: HashMap<&str, &str> = documents
        .iter()
        .map(|d| (d.document_id.as_str(), d.path.as_str()))
        .collect();
    let mut candidate_paths: BTreeSet<String> = BTreeSet::new();
    for link in links {
        if link.rel != DocumentLinkRel::Supersedes {
            continue;
        }
        // Superseder must be registered (FK) — skip if from is missing from map
        // (should not happen under FK, but stay defensive).
        if !id_to_path.contains_key(link.from_document_id.as_str()) {
            continue;
        }
        let Some(ref to_id) = link.to_document_id else {
            // Free-path / unresolved target — not a retirement candidate.
            continue;
        };
        if let Some(path) = id_to_path.get(to_id.as_str()) {
            candidate_paths.insert((*path).to_owned());
        }
    }
    bucket.explicit_superseded_candidates = candidate_paths.iter().cloned().collect();
    bucket.retirement_candidates = bucket.explicit_superseded_candidates.clone();

    // Orphan briefs → 待复核, never retirement_candidates.
    for path in &report.unpaired_briefs {
        bucket.briefs_pending_review.push(path.clone());
    }
    // Zero-inbound briefs that are paired still aren't retirement.
    // Keep pending_review for unpaired only per hard policy.
    bucket.briefs_pending_review.sort();
    bucket.briefs_pending_review.dedup();

    // verification / decision orphans → expected, non-problem. Never candidates
    // unless they also appear via explicit supersession (table: orphans 永不;
    // explicit supersession 可以). Zero-inbound verification/decision stay out
    // of candidates unless a registered superseder points at them.
    for role in ["verification", "decision"] {
        if let Some(list) = report.orphans_by_role.get(role) {
            for path in list {
                bucket.expected_orphans_non_problem.push(path.clone());
            }
        }
    }
    bucket.expected_orphans_non_problem.sort();

    // Hard strip: zero-inbound-only verification/decision must not sit in
    // retirement_candidates without an explicit supersedes link. (They only
    // enter candidate_paths via links, so this is a safety net.)
    let expected_set: HashSet<&str> = bucket
        .expected_orphans_non_problem
        .iter()
        .map(String::as_str)
        .collect();
    // If a path is both expected orphan AND has explicit supersession, keep it
    // as candidate (explicit evidence wins). If only in expected orphans, it
    // was never in candidate_paths. No further strip needed for zero-inbound.
    let _ = expected_set;

    bucket
}

fn find_contradictions(documents: &[KnowledgeDocument]) -> Vec<AuditFinding> {
    let mut by_dir: BTreeMap<String, Vec<&KnowledgeDocument>> = BTreeMap::new();
    for doc in documents {
        let dir = parent_dir(&doc.path);
        // Focus on roles where dual “current” is operationally dangerous.
        if matches!(
            doc.role,
            DocumentRole::Handoff | DocumentRole::Roadmap | DocumentRole::Migration
        ) {
            by_dir.entry(dir).or_default().push(doc);
        }
    }

    let mut findings = Vec::new();
    for (dir, docs) in by_dir {
        let live: Vec<_> = docs
            .iter()
            .filter(|d| d.status != DocumentStatus::Superseded)
            .collect();
        if live.len() >= 2 {
            let paths: Vec<_> = live.iter().map(|d| d.path.as_str()).collect();
            findings.push(AuditFinding {
                path: dir,
                detail: format!(
                    "{} non-superseded {} docs: {}",
                    live.len(),
                    live.first().map(|d| d.role.as_str()).unwrap_or("?"),
                    paths.join(", ")
                ),
            });
        }
    }
    findings
}

fn parent_dir(path: &str) -> String {
    match path.rfind('/') {
        Some(i) => path[..i].to_owned(),
        None => ".".into(),
    }
}

/// Basename-without-.md inbound count (inventory audit method).
fn compute_text_inbound(
    documents: &[KnowledgeDocument],
    contents: &HashMap<String, String>,
) -> HashMap<String, usize> {
    let basenames: Vec<(String, String)> = documents
        .iter()
        .map(|d| {
            let base = file_stem(&d.path);
            (d.path.clone(), base)
        })
        .collect();

    // Precompute: for each document content, which basenames it mentions.
    let mut inbound: HashMap<String, usize> = HashMap::new();
    for doc in documents {
        inbound.insert(doc.path.clone(), 0);
    }

    for (src_path, content) in contents {
        for (target_path, base) in &basenames {
            if src_path == target_path {
                continue;
            }
            if base.is_empty() {
                continue;
            }
            if content.contains(base.as_str()) {
                *inbound.entry(target_path.clone()).or_insert(0) += 1;
            }
        }
    }
    inbound
}

fn file_stem(path: &str) -> String {
    let name = path.rsplit('/').next().unwrap_or(path);
    name.strip_suffix(".md").unwrap_or(name).to_owned()
}

/// Render markdown audit report.
pub fn format_audit_report(report: &AuditReport) -> String {
    let mut out = String::new();
    out.push_str("# Knowledge audit run — K0b indexer\n\n");
    out.push_str(&format!("Date: {}\n", chrono_date()));
    out.push_str(
        "Mode: **register + audit only** (no retirement, no delete, no docs/ mutation)\n\n",
    );
    out.push_str("---\n\n");

    out.push_str("## 1. Registration summary\n\n");
    out.push_str(&format!(
        "- **Total registered:** {}\n",
        report.total_registered
    ));
    out.push_str(&format!(
        "- **`unknown` ratio:** {:.1}% ({}/{})\n\n",
        report.unknown_ratio * 100.0,
        report.status_counts.get("unknown").copied().unwrap_or(0),
        report.total_registered
    ));

    out.push_str("### Role distribution\n\n");
    out.push_str("| role | count |\n| --- | ---: |\n");
    for (role, count) in &report.role_counts {
        out.push_str(&format!("| `{role}` | {count} |\n"));
    }
    out.push('\n');

    out.push_str("### Status distribution\n\n");
    out.push_str("| status | count |\n| --- | ---: |\n");
    for (status, count) in &report.status_counts {
        out.push_str(&format!("| `{status}` | {count} |\n"));
    }
    out.push('\n');

    out.push_str("## 2. Contradictions\n\n");
    if report.contradictions.is_empty() {
        out.push_str("_None detected under multi-current handoff/roadmap rule._\n\n");
    } else {
        for f in &report.contradictions {
            out.push_str(&format!("- **{}** — {}\n", f.path, f.detail));
        }
        out.push('\n');
    }

    out.push_str("## 3. Orphans (zero text-inbound), by role\n\n");
    let total_orphans: usize = report.orphans_by_role.values().map(|v| v.len()).sum();
    out.push_str(&format!("**Total zero-inbound:** {total_orphans}\n\n"));
    for (role, paths) in &report.orphans_by_role {
        out.push_str(&format!("### `{role}` ({})\n\n", paths.len()));
        let expected = role == "verification" || role == "decision";
        if expected {
            out.push_str(
                "> **预期孤儿,非问题** — verification/decision are evidence; zero-inbound is not retirement.\n\n",
            );
        }
        for p in paths {
            out.push_str(&format!("- `{p}`\n"));
        }
        out.push('\n');
    }

    out.push_str("## 4. Unpaired briefs (待复核, not retirement)\n\n");
    out.push_str(&format!("**Count:** {}\n\n", report.unpaired_briefs.len()));
    for p in &report.unpaired_briefs {
        out.push_str(&format!("- `{p}`\n"));
    }
    out.push('\n');

    out.push_str("## 5. Superseded but still referenced\n\n");
    if report.superseded_still_referenced.is_empty() {
        out.push_str("_None._\n\n");
    } else {
        for f in &report.superseded_still_referenced {
            out.push_str(&format!("- `{}` — {}\n", f.path, f.detail));
        }
        out.push('\n');
    }

    out.push_str("## 6. Unclassifiable / path-content conflict (`other`)\n\n");
    out.push_str(&format!("**Count:** {}\n\n", report.unclassifiable.len()));
    for f in &report.unclassifiable {
        out.push_str(&format!("- `{}` — {}\n", f.path, f.detail));
    }
    out.push('\n');

    out.push_str("## 7. Digest changes (rescan)\n\n");
    if report.digest_changes.is_empty() {
        out.push_str("_None (first scan or content unchanged)._\n\n");
    } else {
        for f in &report.digest_changes {
            out.push_str(&format!("- `{}` — {}\n", f.path, f.detail));
        }
        out.push('\n');
    }

    out.push_str("## 8. Inferred supersession (not written to DB)\n\n");
    if report.inferred_supersession.is_empty() {
        out.push_str("_None._\n\n");
    } else {
        for inf in &report.inferred_supersession {
            out.push_str(&format!(
                "- `{}` → {:?} — {}\n",
                inf.from_path, inf.to_path, inf.reason
            ));
        }
        out.push('\n');
    }

    out.push_str("## 9. Retirement / review buckets (report only)\n\n");
    out.push_str("### Explicit superseded (safe mark candidates)\n\n");
    for p in &report.retirement.explicit_superseded_candidates {
        out.push_str(&format!("- `{p}`\n"));
    }
    out.push_str("\n### Briefs pending review (not retirement)\n\n");
    out.push_str(&format!(
        "Count: {}\n\n",
        report.retirement.briefs_pending_review.len()
    ));
    out.push_str("_Listed in §4; zero-inbound is never a retirement signal for briefs._\n\n");
    out.push_str("### Expected orphans (verification/decision) — 非问题\n\n");
    for p in &report.retirement.expected_orphans_non_problem {
        out.push_str(&format!("- `{p}` — **预期孤儿,非问题**\n"));
    }
    out.push_str("\n### Retirement candidates\n\n");
    out.push_str(
        "Only documents with **explicit** supersession status. Zero-inbound alone never appears here.\n\n",
    );
    for p in &report.retirement.retirement_candidates {
        out.push_str(&format!("- `{p}`\n"));
    }
    out.push('\n');

    // Sanity: verification/decision orphans must not appear as retirement unless superseded.
    let bad: BTreeSet<_> = report
        .retirement
        .retirement_candidates
        .iter()
        .filter(|p| {
            report
                .retirement
                .expected_orphans_non_problem
                .iter()
                .any(|e| e == *p)
        })
        .cloned()
        .collect();
    if !bad.is_empty() {
        out.push_str("### POLICY VIOLATION (should be empty)\n\n");
        for p in bad {
            out.push_str(&format!("- `{p}` incorrectly in retirement_candidates\n"));
        }
        out.push('\n');
    }

    out.push_str("---\n\n*Generated by `keeper-knowledge-indexer` (K0b).*\n");
    out
}

fn chrono_date() -> String {
    if let Ok(v) = std::env::var("SOURCE_DATE_EPOCH") {
        return format!("epoch:{v}");
    }
    chrono::Utc::now().format("%Y-%m-%d").to_string()
}

#[cfg(test)]
mod unit_tests {
    use super::*;
    use chrono::Utc;
    use keeper_storage_sqlite::{DocumentRole, DocumentStatus, RegisteredBy};

    fn doc(path: &str, role: DocumentRole, status: DocumentStatus) -> KnowledgeDocument {
        KnowledgeDocument {
            document_id: path.to_owned(),
            project_id: "p".into(),
            path: path.to_owned(),
            role,
            status,
            content_digest: format!("sha256:{}", "a".repeat(64)),
            updated_at: Utc::now(),
            registered_by: RegisteredBy::Lint,
            source_run_id: None,
        }
    }

    #[test]
    fn verification_decision_orphans_not_in_retirement() {
        let documents = vec![
            doc(
                "docs/verification/acc.md",
                DocumentRole::Verification,
                DocumentStatus::Unknown,
            ),
            doc(
                "docs/decisions/ADR-0004.md",
                DocumentRole::Decision,
                DocumentStatus::Accepted,
            ),
            doc(
                "docs/handoff/old.md",
                DocumentRole::Handoff,
                DocumentStatus::Superseded,
            ),
            doc(
                "docs/handoff/new.md",
                DocumentRole::Handoff,
                DocumentStatus::Unknown,
            ),
            // Zero-inbound research — must never become a candidate.
            doc(
                "docs/research/lonely.md",
                DocumentRole::Research,
                DocumentStatus::Unknown,
            ),
        ];
        let links = vec![KnowledgeDocumentLink {
            link_id: "l1".into(),
            from_document_id: "docs/handoff/new.md".into(),
            to_document_id: Some("docs/handoff/old.md".into()),
            to_path: Some("docs/handoff/old.md".into()),
            rel: DocumentLinkRel::Supersedes,
        }];
        let mut contents = HashMap::new();
        for d in &documents {
            contents.insert(d.path.clone(), format!("# {}\n", d.path));
        }
        let report = audit_documents(AuditInput {
            documents,
            links,
            contents,
            path_content_conflicts: HashMap::new(),
            classify_notes: HashMap::new(),
            digest_changes: vec![],
            inferred_supersession: vec![],
        });

        assert!(
            report
                .retirement
                .expected_orphans_non_problem
                .iter()
                .any(|p| p.contains("verification"))
        );
        assert!(
            report
                .retirement
                .expected_orphans_non_problem
                .iter()
                .any(|p| p.contains("ADR-0004"))
        );
        // Retirement candidates require explicit supersedes + registered target;
        // never verification/decision orphans or zero-inbound-only research.
        assert!(!report.retirement.retirement_candidates.iter().any(
            |p| p.contains("verification") || p.contains("ADR-0004") || p.contains("lonely.md")
        ));
        assert!(
            report
                .retirement
                .retirement_candidates
                .iter()
                .any(|p| p.contains("handoff/old"))
        );
    }

    #[test]
    fn status_superseded_alone_is_not_retirement_without_link() {
        let documents = vec![doc(
            "docs/handoff/manual-superseded.md",
            DocumentRole::Handoff,
            DocumentStatus::Superseded,
        )];
        let mut contents = HashMap::new();
        contents.insert(documents[0].path.clone(), "# manual status only\n".into());
        let report = audit_documents(AuditInput {
            documents,
            links: vec![],
            contents,
            path_content_conflicts: HashMap::new(),
            classify_notes: HashMap::new(),
            digest_changes: vec![],
            inferred_supersession: vec![],
        });
        assert!(
            report.retirement.retirement_candidates.is_empty(),
            "status=superseded without resolved supersedes link must not be a candidate"
        );
    }
}
