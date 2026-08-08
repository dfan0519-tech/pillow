//! Role and status classification rules (conservative, inventory-aligned).
//!
//! Status rule of thumb: **when unsure → `unknown`**. Never invent `active`.

use keeper_storage_sqlite::{DocumentRole, DocumentStatus};

/// Full classification for one document path + content.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Classification {
    pub role: DocumentRole,
    /// Provisional status from this document alone (before peer supersession).
    pub status: DocumentStatus,
    /// True when directory convention and content signals clearly conflict.
    pub path_content_conflict: bool,
    /// Human-readable reason for role/status (for tests and reports).
    pub notes: Vec<String>,
}

/// Classified document with path retained for pairing/audit.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClassifiedDoc {
    /// Portable project-relative path (`docs/...`).
    pub path: String,
    pub classification: Classification,
}

/// Classify role + provisional status from path and full file content.
pub fn classify_document(path: &str, content: &str) -> Classification {
    let mut notes = Vec::new();
    let (role, path_content_conflict) = classify_role(path, content, &mut notes);
    let status = classify_status_from_content(path, content, role, &mut notes);
    Classification {
        role,
        status,
        path_content_conflict,
        notes,
    }
}

/// Directory-first role assignment, then filename, then light content signals.
///
/// When directory and content are in clear conflict, returns [`DocumentRole::Other`]
/// and sets `path_content_conflict` so the auditor can list the file without
/// inventing a “correct” role.
pub fn classify_role(path: &str, content: &str, notes: &mut Vec<String>) -> (DocumentRole, bool) {
    let normalized = path.replace('\\', "/");
    let lower_path = normalized.to_ascii_lowercase();
    let file_name = file_name_of(&normalized);
    let lower_name = file_name.to_ascii_lowercase();

    // Directory convention (primary).
    if let Some(dir_role) = role_from_directory(&lower_path) {
        // Filename overrides inside execution/ (multi-role zone).
        if lower_path.contains("/execution/") || lower_path.starts_with("docs/execution/") {
            if let Some(fn_role) = role_from_execution_filename(&lower_name) {
                notes.push(format!(
                    "execution filename override → {}",
                    fn_role.as_str()
                ));
                // Mis-filed research under execution is filename-driven research,
                // not a path/content conflict forcing `other`.
                return (fn_role, false);
            }
            // No filename cue — try light content head, else other.
            if content_looks_like_research(content) {
                notes.push(
                    "execution path without brief/report/plan cue; content looks research → other (conflict)"
                        .into(),
                );
                return (DocumentRole::Other, true);
            }
            notes.push("execution path without classifiable filename → other".into());
            return (DocumentRole::Other, false);
        }

        // superpowers/plans → plan (K0a enum has plan; inventory mapped these to spec).
        if matches!(dir_role, DocumentRole::Plan) {
            notes.push("directory superpowers/plans → plan".into());
            return (DocumentRole::Plan, false);
        }
        if matches!(dir_role, DocumentRole::Spec) {
            notes.push("directory superpowers/specs → spec".into());
            return (DocumentRole::Spec, false);
        }

        // Directory/content contradiction checks (conservative).
        if let Some(conflict) = directory_content_conflict(dir_role, &lower_name, content) {
            notes.push(conflict);
            return (DocumentRole::Other, true);
        }

        notes.push(format!("directory convention → {}", dir_role.as_str()));
        return (dir_role, false);
    }

    // No directory convention — filename only.
    if let Some(fn_role) = role_from_any_filename(&lower_name) {
        notes.push(format!("filename cue → {}", fn_role.as_str()));
        return (fn_role, false);
    }

    notes.push("no directory/filename role → other".into());
    (DocumentRole::Other, false)
}

/// Provisional status from this document's own content only.
///
/// Does **not** apply peer supersession (that is a second pass). Never returns
/// [`DocumentStatus::Active`].
pub fn classify_status_from_content(
    path: &str,
    content: &str,
    role: DocumentRole,
    notes: &mut Vec<String>,
) -> DocumentStatus {
    // Explicit self-claim of being superseded (rare; usually the *other* doc claims).
    if self_claims_superseded(content) {
        notes.push("self-claim superseded".into());
        return DocumentStatus::Superseded;
    }

    // ADR + Status: Accepted
    if matches!(role, DocumentRole::Decision) && adr_status_accepted(content) {
        notes.push("ADR Status: Accepted → accepted".into());
        return DocumentStatus::Accepted;
    }

    // Explicit proposed / not accepted design.
    if explicit_proposed(content) {
        notes.push("explicit proposed → proposed".into());
        return DocumentStatus::Proposed;
    }

    // Brief pairing is applied later when peer inventory is known.
    // Alone, a brief stays unknown (honest default).
    let _ = path;
    let _ = role;
    notes.push("no explicit lifecycle → unknown".into());
    DocumentStatus::Unknown
}

/// Mark brief as consumed when a same-stem report exists (caller supplies flag).
pub fn status_for_paired_brief(
    current: DocumentStatus,
    has_matching_report: bool,
) -> DocumentStatus {
    // Never overwrite non-unknown; never invent active.
    if current != DocumentStatus::Unknown {
        return current;
    }
    if has_matching_report {
        DocumentStatus::Consumed
    } else {
        DocumentStatus::Unknown
    }
}

// ---------------------------------------------------------------------------
// Internals
// ---------------------------------------------------------------------------

fn file_name_of(path: &str) -> &str {
    path.rsplit('/').next().unwrap_or(path)
}

fn role_from_directory(lower_path: &str) -> Option<DocumentRole> {
    // Paths are project-relative like docs/decisions/...
    let after_docs = lower_path.strip_prefix("docs/").unwrap_or(lower_path);

    if after_docs.starts_with("decisions/") {
        return Some(DocumentRole::Decision);
    }
    if after_docs.starts_with("handoff/") {
        return Some(DocumentRole::Handoff);
    }
    if after_docs.starts_with("research/") {
        return Some(DocumentRole::Research);
    }
    if after_docs.starts_with("verification/") {
        return Some(DocumentRole::Verification);
    }
    if after_docs.starts_with("roadmap/") {
        return Some(DocumentRole::Roadmap);
    }
    if after_docs.starts_with("migration/") {
        return Some(DocumentRole::Migration);
    }
    if after_docs.starts_with("references/") {
        return Some(DocumentRole::Reference);
    }
    if after_docs.starts_with("superpowers/specs/") {
        return Some(DocumentRole::Spec);
    }
    if after_docs.starts_with("superpowers/plans/") {
        return Some(DocumentRole::Plan);
    }
    if after_docs.starts_with("execution/") {
        // Multi-role zone; caller uses filename.
        return Some(DocumentRole::Other);
    }
    None
}

fn role_from_execution_filename(lower_name: &str) -> Option<DocumentRole> {
    if lower_name.contains("brief") {
        return Some(DocumentRole::Brief);
    }
    if lower_name.contains("report") || lower_name.contains("verdict") {
        return Some(DocumentRole::Report);
    }
    if lower_name.contains("plan") {
        return Some(DocumentRole::Plan);
    }
    if lower_name.contains("research") {
        return Some(DocumentRole::Research);
    }
    // Audit hybrid under execution (inventory §3.3).
    if lower_name.contains("audit") {
        return Some(DocumentRole::Research);
    }
    None
}

fn role_from_any_filename(lower_name: &str) -> Option<DocumentRole> {
    if lower_name.contains("brief") {
        return Some(DocumentRole::Brief);
    }
    if lower_name.contains("report") || lower_name.contains("verdict") {
        return Some(DocumentRole::Report);
    }
    if lower_name.starts_with("adr-") || lower_name.contains("adr-") {
        return Some(DocumentRole::Decision);
    }
    if lower_name.contains("handoff") {
        return Some(DocumentRole::Handoff);
    }
    if lower_name.contains("research") {
        return Some(DocumentRole::Research);
    }
    if lower_name.contains("plan") {
        return Some(DocumentRole::Plan);
    }
    if lower_name.contains("spec") || lower_name.contains("design") {
        return Some(DocumentRole::Spec);
    }
    None
}

fn content_looks_like_research(content: &str) -> bool {
    let head: String = content.chars().take(800).collect();
    let lower = head.to_ascii_lowercase();
    lower.contains("research")
        || lower.contains("inventory audit")
        || lower.contains("## method")
        || lower.contains("corpus")
}

/// Clear directory vs content conflicts only — do not re-label dual-role docs.
fn directory_content_conflict(
    dir_role: DocumentRole,
    lower_name: &str,
    content: &str,
) -> Option<String> {
    let head: String = content.chars().take(1200).collect();
    let lower_head = head.to_ascii_lowercase();

    // handoff/ directory but file is clearly a task brief.
    if matches!(dir_role, DocumentRole::Handoff)
        && (lower_name.contains("brief")
            || lower_head.contains("worker 槽位")
            || lower_head.contains("## 精确写入白名单")
            || lower_head.contains("write whitelist"))
    {
        return Some(
            "handoff directory but content/filename looks like worker brief → other".into(),
        );
    }

    // decisions/ but not ADR and not decision-shaped.
    if matches!(dir_role, DocumentRole::Decision)
        && !lower_name.contains("adr")
        && !lower_head.contains("status: accepted")
        && !lower_head.contains("# adr")
        && (lower_name.contains("brief") || lower_name.contains("report"))
    {
        return Some("decisions directory but filename/content is brief/report → other".into());
    }

    // verification/ named as brief (mis-filed contract).
    if matches!(dir_role, DocumentRole::Verification) && lower_name.contains("brief") {
        return Some("verification directory but filename is brief → other".into());
    }

    None
}

fn self_claims_superseded(content: &str) -> bool {
    let lower = content.to_ascii_lowercase();
    // Conservative: explicit “this document has been superseded” style only.
    lower.contains("this document has been superseded")
        || lower.contains("this file has been superseded")
        || content.contains("本文档已被取代")
        || content.contains("本文件已被取代")
}

fn adr_status_accepted(content: &str) -> bool {
    // Match common ADR headers: "Status: Accepted" (case-insensitive line).
    for line in content.lines().take(40) {
        let trimmed = line.trim();
        let lower = trimmed.to_ascii_lowercase();
        if lower.starts_with("status:") {
            let value = lower.trim_start_matches("status:").trim();
            if value.starts_with("accepted") {
                return true;
            }
        }
        // Chinese variant if present.
        if trimmed.contains("状态") && trimmed.contains("已接受") {
            return true;
        }
    }
    false
}

fn explicit_proposed(content: &str) -> bool {
    for line in content.lines().take(60) {
        let trimmed = line.trim();
        let lower = trimmed.to_ascii_lowercase();
        if lower.starts_with("status:") {
            let value = lower.trim_start_matches("status:").trim();
            if value.starts_with("proposed")
                || value.contains("not accepted")
                || value.contains("not authorized")
            {
                return true;
            }
        }
        // Common design-header form in this corpus.
        if lower.contains("status")
            && (lower.contains("proposed") && lower.contains("not authorized")
                || lower.contains("**proposed**"))
        {
            return true;
        }
    }
    false
}

/// Stem used for brief↔report pairing (strip date prefix, brief/report/verdict suffixes).
pub fn pairing_stem(path: &str) -> Option<String> {
    let normalized = path.replace('\\', "/");
    let name = file_name_of(&normalized);
    let stem = name.strip_suffix(".md").unwrap_or(name);
    let lower = stem.to_ascii_lowercase();

    // Prefer hyphenated kind suffixes used by this corpus.
    if let Some(s) = lower.strip_suffix("-brief") {
        return Some(s.to_owned());
    }
    if let Some(s) = lower.strip_suffix("-report") {
        return Some(s.to_owned());
    }
    if let Some(s) = lower.strip_suffix("-verdict") {
        return Some(s.to_owned());
    }
    if let Some(s) = lower.strip_suffix("_verdict") {
        return Some(s.to_owned());
    }
    // Bare `*brief` / `*report` without hyphen are not paired (too ambiguous).
    None
}

/// Whether `path` looks like a report/verdict artifact for pairing.
pub fn is_report_like_path(path: &str) -> bool {
    let normalized = path.replace('\\', "/");
    let name = file_name_of(&normalized).to_ascii_lowercase();
    name.contains("report") || name.contains("verdict")
}

/// Whether `path` looks like a brief artifact for pairing.
pub fn is_brief_like_path(path: &str) -> bool {
    let normalized = path.replace('\\', "/");
    let name = file_name_of(&normalized).to_ascii_lowercase();
    name.contains("brief")
}

#[cfg(test)]
mod unit_tests {
    use super::*;

    #[test]
    fn unsure_is_unknown_never_active() {
        let c = classify_document(
            "docs/execution/2026-08-08-something-report.md",
            "# Report\n\nWorker finished. Status: DONE\n",
        );
        assert_eq!(c.status, DocumentStatus::Unknown);
        assert_ne!(c.status, DocumentStatus::Active);
    }

    #[test]
    fn adr_accepted() {
        let c = classify_document(
            "docs/decisions/ADR-0004-knowledge-layer.md",
            "# ADR-0004\n\nStatus: Accepted\n\n## Context\n",
        );
        assert_eq!(c.role, DocumentRole::Decision);
        assert_eq!(c.status, DocumentStatus::Accepted);
    }
}
