//! K0e: handoff-document vs ledger **stale detection** (report-only).
//!
//! Detects mechanical inconsistency between a written handoff document and the
//! dispatch/acceptance ledger. **Never rewrites documents** — only reports.
//!
//! Three required cases (ADR-0004 context / K0e brief):
//! 1. Doc claims `在途 Worker: N` but ledger `dispatched` count ≠ N
//! 2. Ledger has completed-without-verdict tasks the doc never mentions
//! 3. Doc mentions tasks that do not exist in the ledger (reverse)
//!
//! Privacy: reports only slot labels, titles, task ids, counts — never lease /
//! session / host user paths.

use std::collections::HashSet;

use keeper_storage_sqlite::{KnowledgeTask, TaskStatus};

use crate::ledger::LedgerSnapshot;

/// Kind of stale finding.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StaleKind {
    /// Doc claims an in-flight worker count that does not match ledger.
    InflightCountMismatch {
        claimed: usize,
        ledger_dispatched: usize,
        claim_excerpt: String,
    },
    /// Completed task with no verdict, and the handoff never mentions it.
    CompletedWithoutVerdictUnmentioned {
        task_id: String,
        title: String,
        worker_label: String,
    },
    /// Handoff mentions a task that has no ledger row.
    DocTaskMissingFromLedger {
        mentioned: String,
        mention_kind: String,
        mention_excerpt: String,
    },
}

/// One actionable finding.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StaleFinding {
    pub kind: StaleKind,
}

impl StaleFinding {
    /// Human-readable, actionable explanation (both sides).
    pub fn message(&self) -> String {
        match &self.kind {
            StaleKind::InflightCountMismatch {
                claimed,
                ledger_dispatched,
                claim_excerpt,
            } => {
                format!(
                    "In-flight count mismatch: document claims {claimed} (excerpt: {claim_excerpt:?}), \
                     ledger has {ledger_dispatched} task(s) with status=dispatched. \
                     Update the handoff or record missing/extra dispatches in the ledger."
                )
            }
            StaleKind::CompletedWithoutVerdictUnmentioned {
                task_id,
                title,
                worker_label,
            } => {
                format!(
                    "Completed without verdict not mentioned in document: \
                     title={title:?} task_id=`{task_id}` slot=`{worker_label}`. \
                     Ledger has status=completed with no acceptance verdict; handoff is silent. \
                     Record a verdict or mention the gap in the handoff."
                )
            }
            StaleKind::DocTaskMissingFromLedger {
                mentioned,
                mention_kind,
                mention_excerpt,
            } => {
                format!(
                    "Document mentions a task not present in the ledger: \
                     {mention_kind}={mentioned:?} (excerpt: {mention_excerpt:?}). \
                     Either dispatch/record it in the ledger or remove the stale claim from the handoff."
                )
            }
        }
    }
}

/// Full stale check report.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StaleReport {
    pub findings: Vec<StaleFinding>,
    pub ledger_dispatched: usize,
    pub ledger_completed_without_verdict: usize,
    pub doc_inflight_claims: Vec<(usize, String)>,
    pub doc_task_mentions: usize,
}

impl StaleReport {
    pub fn is_stale(&self) -> bool {
        !self.findings.is_empty()
    }
}

/// Extracted task mention from a handoff document.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TaskMention {
    pub value: String,
    /// `title` or `task_id`
    pub kind: String,
    pub excerpt: String,
}

/// Pure stale check: document text vs ledger snapshot.
///
/// Does not touch the filesystem or mutate anything.
pub fn check_stale(doc: &str, snap: &LedgerSnapshot) -> StaleReport {
    let ledger_dispatched = snap
        .tasks
        .iter()
        .filter(|t| t.status == TaskStatus::Dispatched)
        .count();
    let pending = &snap.completed_without_verdict;

    let mut findings = Vec::new();

    // --- Case 1: in-flight count claims ---
    let claims = extract_inflight_claims(doc);
    for (claimed, excerpt) in &claims {
        if *claimed != ledger_dispatched {
            findings.push(StaleFinding {
                kind: StaleKind::InflightCountMismatch {
                    claimed: *claimed,
                    ledger_dispatched,
                    claim_excerpt: excerpt.clone(),
                },
            });
        }
    }

    // --- Case 2: completed without verdict not mentioned ---
    for task in pending {
        if !document_mentions_task(doc, task) {
            findings.push(StaleFinding {
                kind: StaleKind::CompletedWithoutVerdictUnmentioned {
                    task_id: task.task_id.clone(),
                    title: task.title.clone(),
                    worker_label: task.worker_label.clone(),
                },
            });
        }
    }

    // --- Case 3: doc mentions tasks missing from ledger ---
    let mentions = extract_task_mentions(doc);
    let ledger_titles: HashSet<String> =
        snap.tasks.iter().map(|t| normalize_key(&t.title)).collect();
    let ledger_ids: HashSet<String> = snap.tasks.iter().map(|t| t.task_id.clone()).collect();

    for m in &mentions {
        let missing = match m.kind.as_str() {
            "task_id" => !ledger_ids.contains(&m.value),
            _ => !ledger_titles.contains(&normalize_key(&m.value)),
        };
        if missing {
            findings.push(StaleFinding {
                kind: StaleKind::DocTaskMissingFromLedger {
                    mentioned: m.value.clone(),
                    mention_kind: m.kind.clone(),
                    mention_excerpt: m.excerpt.clone(),
                },
            });
        }
    }

    StaleReport {
        findings,
        ledger_dispatched,
        ledger_completed_without_verdict: pending.len(),
        doc_inflight_claims: claims,
        doc_task_mentions: mentions.len(),
    }
}

/// Format a stale report for stdout (actionable; privacy-clean fields only).
pub fn format_stale_report(report: &StaleReport, doc_label: &str) -> String {
    let mut out = String::new();
    out.push_str("# Handoff vs ledger stale check\n\n");
    out.push_str(&format!("document: {doc_label}\n"));
    out.push_str(&format!(
        "ledger: dispatched={} completed_without_verdict={} doc_inflight_claims={} doc_task_mentions={}\n\n",
        report.ledger_dispatched,
        report.ledger_completed_without_verdict,
        report.doc_inflight_claims.len(),
        report.doc_task_mentions
    ));
    out.push_str(
        "Privacy: report uses slot labels, titles, task ids, counts only — never lease/session/host paths.\n\n",
    );

    if report.findings.is_empty() {
        out.push_str("## Result: **fresh** (no stale findings)\n\n");
        out.push_str(
            "Document claims are consistent with the ledger for the checks performed \
             (in-flight count, completed-without-verdict mentions, reverse task mentions).\n",
        );
        return out;
    }

    out.push_str(&format!(
        "## Result: **STALE** ({} finding(s))\n\n",
        report.findings.len()
    ));
    for (i, f) in report.findings.iter().enumerate() {
        let code = match &f.kind {
            StaleKind::InflightCountMismatch { .. } => "inflight_count_mismatch",
            StaleKind::CompletedWithoutVerdictUnmentioned { .. } => {
                "completed_without_verdict_unmentioned"
            }
            StaleKind::DocTaskMissingFromLedger { .. } => "doc_task_missing_from_ledger",
        };
        out.push_str(&format!("### {}. `{code}`\n\n", i + 1));
        out.push_str(&format!("{}\n\n", f.message()));
    }
    out
}

// ---------------------------------------------------------------------------
// Claim / mention extraction (mechanical, pattern-based — not NLP)
// ---------------------------------------------------------------------------

/// Extract `在途 Worker: N` style claims (and close variants from real handoffs).
pub fn extract_inflight_claims(doc: &str) -> Vec<(usize, String)> {
    let mut out = Vec::new();
    for line in doc.lines() {
        let trimmed = line.trim();
        if let Some((n, excerpt)) = match_inflight_line(trimmed) {
            out.push((n, excerpt));
        }
    }
    out
}

fn match_inflight_line(line: &str) -> Option<(usize, String)> {
    // Normalize fullwidth colon.
    let line_norm = line.replace('：', ":");

    // Patterns (order matters for excerpt clarity):
    // 1. 在途 Worker: N   / **在途 Worker:0。**
    // 2. 在途派单: N
    // 3. Active Grok Worker | N
    // 4. Grok 在途 Worker | N
    // 5. In-flight workers: N / dispatched count: N
    let patterns: &[(&str, &str)] = &[
        ("在途 Worker:", "在途 Worker"),
        ("在途Worker:", "在途Worker"),
        ("在途派单:", "在途派单"),
        ("Active Grok Worker", "Active Grok Worker"),
        ("Grok 在途 Worker", "Grok 在途 Worker"),
        ("In-flight workers:", "In-flight workers"),
        ("in-flight workers:", "in-flight workers"),
        ("dispatched count:", "dispatched count"),
    ];

    for (needle, _label) in patterns {
        if let Some(idx) = find_ci(&line_norm, needle) {
            let after = &line_norm[idx + needle.len()..];
            if let Some(n) = first_usize_token(after) {
                let excerpt = truncate(line, 120);
                return Some((n, excerpt));
            }
        }
    }

    // Table form already covered when "Active Grok Worker" is followed by | N
    // Also: **Count:** N under a 在途 section is NOT used — too ambiguous alone.

    None
}

/// Explicit task mentions the detector understands.
///
/// Supported forms (script/controller-friendly, non-interactive):
/// - `task-title: <title>` / `任务标题: <title>`
/// - `task-id: <id>`
/// - `Task: <title>` (line start / bullet)
/// - `在途任务: <title>` / `派单: <title>`
/// - Markdown table row: `| <title> | `grokN` |` (slot-looking second column)
pub fn extract_task_mentions(doc: &str) -> Vec<TaskMention> {
    let mut out = Vec::new();
    let mut seen: HashSet<String> = HashSet::new();

    for line in doc.lines() {
        let trimmed = line.trim().trim_start_matches(['-', '*', '>']).trim();
        let norm = trimmed.replace('：', ":");

        // task-id: ...
        if let Some(rest) = strip_prefix_ci(&norm, "task-id:") {
            let value = rest.trim().trim_matches('`').trim().to_owned();
            if !value.is_empty() {
                push_mention(&mut out, &mut seen, value, "task_id", trimmed);
            }
            continue;
        }

        // task-title: / 任务标题:
        let mut matched_title_prefix = false;
        for prefix in ["task-title:", "任务标题:"] {
            if let Some(rest) = strip_prefix_ci(&norm, prefix) {
                let value = clean_title(rest);
                if !value.is_empty() {
                    push_mention(&mut out, &mut seen, value, "title", trimmed);
                }
                matched_title_prefix = true;
                break;
            }
        }
        if matched_title_prefix {
            continue;
        }

        // Task: / 在途任务: / 派单:
        let mut matched_task_prefix = false;
        for prefix in ["Task:", "在途任务:", "派单:"] {
            if let Some(rest) = strip_prefix_ci(&norm, prefix) {
                // Avoid matching "派单台账" style without value — need nonempty title.
                let value = clean_title(rest);
                if !value.is_empty() && value.len() <= 512 {
                    push_mention(&mut out, &mut seen, value, "title", trimmed);
                    matched_task_prefix = true;
                    break;
                }
            }
        }
        if matched_task_prefix {
            continue;
        }

        // Markdown table: | title | `grok3` | ...
        if let Some((title, slot)) = parse_title_slot_table_row(trimmed)
            && looks_like_slot_label(&slot)
            && !title.is_empty()
        {
            // Skip header rows
            let t_lower = title.to_ascii_lowercase();
            if t_lower != "title" && t_lower != "任务" {
                push_mention(&mut out, &mut seen, title, "title", trimmed);
            }
        }
    }

    out
}

fn push_mention(
    out: &mut Vec<TaskMention>,
    seen: &mut HashSet<String>,
    value: String,
    kind: &str,
    excerpt: &str,
) {
    let key = format!("{kind}:{}", normalize_key(&value));
    if seen.insert(key) {
        out.push(TaskMention {
            value,
            kind: kind.to_owned(),
            excerpt: truncate(excerpt, 120),
        });
    }
}

fn document_mentions_task(doc: &str, task: &KnowledgeTask) -> bool {
    if doc.contains(&task.task_id) {
        return true;
    }
    // Case-insensitive title substring.
    let doc_lower = doc.to_ascii_lowercase();
    let title_lower = task.title.to_ascii_lowercase();
    if !title_lower.is_empty() && doc_lower.contains(&title_lower) {
        return true;
    }
    // Also accept explicit task-title mentions that normalize equal.
    for m in extract_task_mentions(doc) {
        if m.kind == "task_id" && m.value == task.task_id {
            return true;
        }
        if m.kind == "title" && normalize_key(&m.value) == normalize_key(&task.title) {
            return true;
        }
    }
    false
}

fn parse_title_slot_table_row(line: &str) -> Option<(String, String)> {
    let t = line.trim();
    if !t.starts_with('|') {
        return None;
    }
    let parts: Vec<&str> = t
        .split('|')
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .collect();
    if parts.len() < 2 {
        return None;
    }
    // Skip separator rows | --- | --- |
    if parts
        .iter()
        .all(|p| p.chars().all(|c| c == '-' || c == ':' || c == ' '))
    {
        return None;
    }
    let title = parts[0].trim_matches('`').trim().to_owned();
    let slot = parts[1].trim_matches('`').trim().to_owned();
    Some((title, slot))
}

fn looks_like_slot_label(s: &str) -> bool {
    let s = s.trim();
    if s.is_empty() || s.len() > 64 {
        return false;
    }
    s.bytes()
        .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
        && !s.contains("---")
}

fn clean_title(rest: &str) -> String {
    rest.trim()
        .trim_matches('`')
        .trim_matches('*')
        .trim_matches('"')
        .trim_matches('「')
        .trim_matches('」')
        .trim()
        .trim_end_matches(['。', '.', '；', ';'])
        .trim()
        .to_owned()
}

fn normalize_key(s: &str) -> String {
    s.split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .to_ascii_lowercase()
}

fn first_usize_token(s: &str) -> Option<usize> {
    let mut digits = String::new();
    for ch in s.chars() {
        if ch.is_ascii_digit() {
            digits.push(ch);
        } else if !digits.is_empty() {
            break;
        } else if ch == '|'
            || ch.is_whitespace()
            || ch == '*'
            || ch == '`'
            || ch == '.'
            || ch == '。'
        {
            continue;
        } else if digits.is_empty() {
            // skip bold markers etc. before digits
            continue;
        }
    }
    if digits.is_empty() {
        None
    } else {
        digits.parse().ok()
    }
}

fn find_ci(hay: &str, needle: &str) -> Option<usize> {
    let hay_l = hay.to_ascii_lowercase();
    let needle_l = needle.to_ascii_lowercase();
    // ascii-only needles used; byte index == char index for our patterns
    hay_l.find(&needle_l)
}

fn strip_prefix_ci<'a>(s: &'a str, prefix: &str) -> Option<&'a str> {
    if s.len() < prefix.len() {
        return None;
    }
    if s.get(..prefix.len())
        .map(|p| p.eq_ignore_ascii_case(prefix))
        .unwrap_or(false)
    {
        Some(&s[prefix.len()..])
    } else {
        // For non-ascii prefixes like 任务标题:
        s.strip_prefix(prefix)
    }
}

fn truncate(s: &str, max: usize) -> String {
    let t = s.trim();
    if t.chars().count() <= max {
        t.to_owned()
    } else {
        let clipped: String = t.chars().take(max).collect();
        format!("{clipped}…")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{TimeZone, Utc};
    use keeper_storage_sqlite::{KnowledgeVerdict, VerdictOutcome};

    fn task(id: &str, title: &str, status: TaskStatus, slot: &str) -> KnowledgeTask {
        KnowledgeTask {
            task_id: id.to_owned(),
            project_id: "p".into(),
            title: title.to_owned(),
            brief_path: None,
            worker_label: slot.to_owned(),
            dispatched_at: Utc.with_ymd_and_hms(2026, 8, 9, 10, 0, 0).unwrap(),
            status,
            completion_signal: None,
        }
    }

    fn snap(tasks: Vec<KnowledgeTask>, verdicts: Vec<KnowledgeVerdict>) -> LedgerSnapshot {
        let completed_without_verdict: Vec<KnowledgeTask> = tasks
            .iter()
            .filter(|t| {
                t.status == TaskStatus::Completed
                    && !verdicts
                        .iter()
                        .any(|v| v.task_id.as_deref() == Some(t.task_id.as_str()))
            })
            .cloned()
            .collect();
        LedgerSnapshot {
            project_id: "p".into(),
            project_name: "pillow".into(),
            tasks,
            verdicts,
            completed_without_verdict,
        }
    }

    #[test]
    fn inflight_mismatch_detected() {
        let doc = "Round summary.\n\n**在途 Worker:1。**\n\nMore text.\n";
        let ledger = snap(vec![], vec![]); // 0 dispatched
        let report = check_stale(doc, &ledger);
        assert!(report.is_stale());
        assert!(report.findings.iter().any(|f| matches!(
            f.kind,
            StaleKind::InflightCountMismatch {
                claimed: 1,
                ledger_dispatched: 0,
                ..
            }
        )));
    }

    #[test]
    fn completed_without_verdict_unmentioned() {
        let doc = "在途 Worker: 0\n\nNo pending items listed.\n";
        let t = task("tid-1", "UI premium polish", TaskStatus::Completed, "grok2");
        let ledger = snap(vec![t], vec![]);
        let report = check_stale(doc, &ledger);
        assert!(report.is_stale());
        assert!(report.findings.iter().any(|f| matches!(
            &f.kind,
            StaleKind::CompletedWithoutVerdictUnmentioned { title, .. }
                if title == "UI premium polish"
        )));
    }

    #[test]
    fn doc_task_missing_from_ledger() {
        let doc = "在途 Worker: 0\n\ntask-title: Phantom task never dispatched\n";
        let ledger = snap(vec![], vec![]);
        let report = check_stale(doc, &ledger);
        assert!(report.is_stale());
        assert!(report.findings.iter().any(|f| matches!(
            &f.kind,
            StaleKind::DocTaskMissingFromLedger { mentioned, .. }
                if mentioned.contains("Phantom")
        )));
    }

    #[test]
    fn consistent_document_not_flagged() {
        let t_inflight = task("tid-a", "K0e ledger CLI", TaskStatus::Dispatched, "grok3");
        let t_done = task("tid-b", "K0d ledger tables", TaskStatus::Completed, "grok2");
        let v = KnowledgeVerdict {
            verdict_id: "v1".into(),
            task_id: Some("tid-b".into()),
            project_id: "p".into(),
            outcome: VerdictOutcome::Accepted,
            gate_result: None,
            e2e_result: None,
            summary: "ok".into(),
            residuals: None,
            decided_at: Utc.with_ymd_and_hms(2026, 8, 9, 12, 0, 0).unwrap(),
        };
        // completed with verdict → not in pending
        let ledger = snap(vec![t_inflight, t_done], vec![v]);
        let doc = "\
# Handoff\n\
在途 Worker: 1\n\
\n\
task-title: K0e ledger CLI\n\
task-title: K0d ledger tables\n\
";
        let report = check_stale(doc, &ledger);
        assert!(
            !report.is_stale(),
            "expected fresh, got: {:?}",
            report
                .findings
                .iter()
                .map(|f| f.message())
                .collect::<Vec<_>>()
        );
    }

    #[test]
    fn report_privacy_clean() {
        let t = task("tid-x", "Clean title", TaskStatus::Dispatched, "grok3");
        let ledger = snap(vec![t], vec![]);
        let doc = "在途 Worker: 0\ntask-title: Clean title\n";
        let report = check_stale(doc, &ledger);
        let md = format_stale_report(&report, "docs/handoff/test.md");
        let lease = format!("{}-{}-", "grok", "lease");
        let session = format!("{}T{}Z-{}-{}", "20260807", "035151", "pillow", "pxrm");
        let win = format!("{}:\\{}\\", "C", "Users");
        assert!(!md.contains(&lease));
        assert!(!md.contains(&session));
        assert!(!md.contains(&win));
        assert!(md.contains("grok3") || md.contains("Clean title") || md.contains("STALE"));
    }
}
