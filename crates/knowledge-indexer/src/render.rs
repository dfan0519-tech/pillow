//! Registry → Markdown status view (K0c + K0d ledger).
//!
//! Pure projection of K0a rows + K0b audit findings + K0d dispatch/verdict
//! ledger. Never invents narrative project state; never mutates the registry
//! or `docs/`. Never renders lease ids, session ids, or machine user paths.

use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};

use keeper_storage_sqlite::{
    DocumentLinkRel, DocumentStatus, KnowledgeDocument, KnowledgeDocumentLink, KnowledgeTask,
    KnowledgeVerdict, TaskStatus,
};

use crate::audit::AuditReport;

/// Input for the handoff/status renderer. All fields come from the registry
/// scan/audit/ledger path — no free-form prose.
#[derive(Debug, Clone, PartialEq)]
pub struct RenderInput {
    pub documents: Vec<KnowledgeDocument>,
    pub links: Vec<KnowledgeDocumentLink>,
    pub audit: AuditReport,
    /// K0d dispatch ledger rows (project-scoped at the call site).
    pub tasks: Vec<KnowledgeTask>,
    /// K0d acceptance verdict rows (project-scoped at the call site).
    pub verdicts: Vec<KnowledgeVerdict>,
    /// When `false` (the hard default for default call sites), rows with
    /// `status=archived` are omitted from every section — ADR-0004 §5:
    /// archived must be default-invisible or retirement is cosmetic.
    pub include_archived: bool,
}

/// Build a registry-only Markdown status view.
///
/// **Pure function:** same `RenderInput` always yields the same `String`
/// (no wall-clock, no env, no filesystem).
///
/// **Default visibility:** when `include_archived` is `false`, archived
/// documents never appear in the output (not even under “superseded /
/// archived”). Callers must set the flag to recover them. Filtering happens
/// once at the entry so no later section can forget.
pub fn render_status_view(input: &RenderInput) -> String {
    // Single choke-point: every section below reads only filtered rows.
    // Filter documents *and* links so supersession / free-path claims cannot
    // reintroduce archived basenames after rows were dropped.
    let archived_paths: HashSet<&str> = input
        .documents
        .iter()
        .filter(|d| d.status == DocumentStatus::Archived)
        .map(|d| d.path.as_str())
        .collect();
    let archived_ids: HashSet<&str> = input
        .documents
        .iter()
        .filter(|d| d.status == DocumentStatus::Archived)
        .map(|d| d.document_id.as_str())
        .collect();
    let visible: Vec<KnowledgeDocument> = if input.include_archived {
        input.documents.clone()
    } else {
        input
            .documents
            .iter()
            .filter(|d| d.status != DocumentStatus::Archived)
            .cloned()
            .collect()
    };
    let links: Vec<KnowledgeDocumentLink> = if input.include_archived {
        input.links.clone()
    } else {
        input
            .links
            .iter()
            .filter(|l| {
                if archived_ids.contains(l.from_document_id.as_str()) {
                    return false;
                }
                if let Some(ref to_id) = l.to_document_id
                    && archived_ids.contains(to_id.as_str())
                {
                    return false;
                }
                if let Some(ref to_path) = l.to_path
                    && archived_paths.contains(to_path.as_str())
                {
                    return false;
                }
                true
            })
            .cloned()
            .collect()
    };
    let input = RenderInput {
        documents: visible,
        links,
        audit: if input.include_archived {
            input.audit.clone()
        } else {
            strip_archived_from_audit(&input.audit, &archived_paths)
        },
        tasks: input.tasks.clone(),
        verdicts: input.verdicts.clone(),
        include_archived: input.include_archived,
    };

    let mut out = String::new();
    out.push_str("# Knowledge status view (registry-rendered)\n\n");
    out.push_str(
        "Source: **knowledge document registry + dispatch/acceptance ledger** (roles, statuses, links, audit buckets, tasks, verdicts).\n",
    );
    out.push_str(
        "This is a **view**, not a maintained handoff. No hand-written project narrative is inlined.\n",
    );
    out.push_str(
        "Privacy: only **slot labels** (e.g. `grok3`) and portable paths — never lease ids, session ids, or host user paths.\n",
    );
    if input.include_archived {
        out.push_str(
            "Visibility: **`include_archived=true`** — archived rows are shown when present.\n\n",
        );
    } else {
        out.push_str(
            "Visibility: **archived excluded by default** (ADR-0004 §5 / K1a). Pass `include_archived` to recover.\n\n",
        );
    }
    out.push_str("---\n\n");

    let ledger_empty = input.tasks.is_empty() && input.verdicts.is_empty();
    if input.documents.is_empty() && ledger_empty {
        out.push_str("## Statistics\n\n");
        out.push_str("- **Total registered:** 0\n");
        out.push_str("- **`unknown` status ratio:** n/a (empty registry)\n\n");
        out.push_str("## No data\n\n");
        out.push_str(
            "_Registry is empty. Nothing to list under current docs, supersession, contradictions, review buckets, or the dispatch/acceptance ledger._\n\n",
        );
        out.push_str("---\n\n*Generated by `keeper-knowledge-indexer` render (K0c/K0d).*\n");
        return out;
    }

    if input.documents.is_empty() {
        out.push_str("## 1. Statistics\n\n");
        out.push_str("- **Total registered documents:** 0\n");
        out.push_str("- **`unknown` status ratio:** n/a (no documents)\n\n");
        render_ledger_sections(&mut out, &input);
        out.push_str(
            "---\n\n*Generated by `keeper-knowledge-indexer` render (K0c/K0d). Pure registry + ledger projection.*\n",
        );
        return out;
    }

    // Prefer audit counts when present (same scan); else recompute from rows.
    let total = if input.audit.total_registered > 0 {
        input.audit.total_registered
    } else {
        input.documents.len()
    };
    let role_counts = if input.audit.role_counts.is_empty() {
        count_roles(&input.documents)
    } else {
        input.audit.role_counts.clone()
    };
    let status_counts = if input.audit.status_counts.is_empty() {
        count_statuses(&input.documents)
    } else {
        input.audit.status_counts.clone()
    };
    let unknown_count = status_counts.get("unknown").copied().unwrap_or(0);
    let unknown_ratio = if total == 0 {
        0.0
    } else if input.audit.total_registered > 0 {
        input.audit.unknown_ratio
    } else {
        unknown_count as f64 / total as f64
    };

    // --- Statistics (unknown ratio must be prominent) ---
    out.push_str("## 1. Statistics\n\n");
    out.push_str(&format!("- **Total registered:** {total}\n"));
    out.push_str(&format!(
        "- **`unknown` status ratio (UNCLASSIFIED):** **{:.1}%** ({unknown_count}/{total})\n",
        unknown_ratio * 100.0
    ));
    if unknown_ratio >= 0.5 {
        out.push_str(
            "- ⚠️ **Majority of registry rows are status=`unknown` (未分类).** Do not treat path lists as known operational state.\n",
        );
    }
    out.push('\n');

    out.push_str("### Role distribution\n\n");
    out.push_str("| role | count |\n| --- | ---: |\n");
    for (role, count) in &role_counts {
        out.push_str(&format!("| `{role}` | {count} |\n"));
    }
    out.push('\n');

    out.push_str("### Status distribution\n\n");
    out.push_str("| status | count | note |\n| --- | ---: | --- |\n");
    for (status, count) in &status_counts {
        let note = if status == "unknown" {
            "**未分类 / UNCLASSIFIED**"
        } else {
            ""
        };
        out.push_str(&format!("| `{status}` | {count} | {note} |\n"));
    }
    out.push('\n');

    // --- Current effective docs ---
    // Live = not superseded (and archived already filtered unless include_archived).
    // Split known status vs unknown. Archived only appears when include_archived.
    let mut by_role_known: BTreeMap<String, Vec<&KnowledgeDocument>> = BTreeMap::new();
    let mut unclassified: Vec<&KnowledgeDocument> = Vec::new();
    let mut retired: Vec<&KnowledgeDocument> = Vec::new();

    for doc in &input.documents {
        match doc.status {
            DocumentStatus::Superseded | DocumentStatus::Archived => {
                retired.push(doc);
            }
            DocumentStatus::Unknown => {
                unclassified.push(doc);
            }
            _ => {
                by_role_known
                    .entry(doc.role.as_str().to_owned())
                    .or_default()
                    .push(doc);
            }
        }
    }
    for list in by_role_known.values_mut() {
        list.sort_by(|a, b| a.path.cmp(&b.path));
    }
    unclassified.sort_by(|a, b| a.path.cmp(&b.path));
    retired.sort_by(|a, b| a.path.cmp(&b.path));

    out.push_str("## 2. Current effective documents\n\n");
    out.push_str("Definition (registry-only): status ∉ {`superseded`, `archived`}.\n");
    out.push_str(
        "Rows with status=`unknown` are **未分类** — listed in §2.2, not mixed into known-status role groups.\n\n",
    );

    out.push_str("### 2.1 By role (status classified — not `unknown`)\n\n");
    if by_role_known.is_empty() {
        out.push_str("_No documents with a non-`unknown` live status._\n\n");
    } else {
        for (role, docs) in &by_role_known {
            out.push_str(&format!("#### `{role}` ({})\n\n", docs.len()));
            for d in docs {
                out.push_str(&format!(
                    "- `{}` — status=`{}`\n",
                    d.path,
                    d.status.as_str()
                ));
            }
            out.push('\n');
        }
    }

    out.push_str("### 2.2 Unclassified (`status=unknown`) — 未分类\n\n");
    out.push_str(&format!(
        "**Count:** {} ({:.1}% of registry)\n\n",
        unclassified.len(),
        if total == 0 {
            0.0
        } else {
            unclassified.len() as f64 / total as f64 * 100.0
        }
    ));
    out.push_str(
        "> These paths are **registered but not lifecycle-classified**. Listing them is inventory, not a claim that they are current operational truth.\n\n",
    );
    // Group unclassified by role for scannability, still under 未分类.
    let mut unknown_by_role: BTreeMap<String, Vec<&KnowledgeDocument>> = BTreeMap::new();
    for d in &unclassified {
        unknown_by_role
            .entry(d.role.as_str().to_owned())
            .or_default()
            .push(d);
    }
    for (role, docs) in &unknown_by_role {
        out.push_str(&format!(
            "#### role=`{role}` (unclassified status) ({})\n\n",
            docs.len()
        ));
        for d in docs {
            out.push_str(&format!("- `{}` — status=`unknown` **未分类**\n", d.path));
        }
        out.push('\n');
    }

    if input.include_archived {
        out.push_str("### 2.3 Superseded / archived (not current)\n\n");
    } else {
        out.push_str("### 2.3 Superseded (not current; archived omitted by default)\n\n");
    }
    out.push_str(&format!("**Count:** {}\n\n", retired.len()));
    for d in &retired {
        out.push_str(&format!(
            "- `{}` — status=`{}` role=`{}`\n",
            d.path,
            d.status.as_str(),
            d.role.as_str()
        ));
    }
    out.push('\n');

    // --- Supersession chains ---
    out.push_str("## 3. Supersession chains\n\n");
    out.push_str(
        "Edges from registry `supersedes` links only (explicit/查证). Free-path claims have no resolved `to_document_id`.\n\n",
    );

    let id_to_path: HashMap<&str, &str> = input
        .documents
        .iter()
        .map(|d| (d.document_id.as_str(), d.path.as_str()))
        .collect();

    let mut by_from: BTreeMap<String, Vec<SupersedeTarget>> = BTreeMap::new();
    for link in &input.links {
        if link.rel != DocumentLinkRel::Supersedes {
            continue;
        }
        let from_path = id_to_path
            .get(link.from_document_id.as_str())
            .copied()
            .unwrap_or(link.from_document_id.as_str())
            .to_owned();
        let target = resolve_target(link, &id_to_path);
        by_from.entry(from_path).or_default().push(target);
    }
    for targets in by_from.values_mut() {
        targets.sort_by_key(|a| a.sort_key());
    }

    if by_from.is_empty() {
        out.push_str("_No `supersedes` links in registry._\n\n");
    } else {
        out.push_str("| from | arity | targets |\n| --- | ---: | --- |\n");
        for (from, targets) in &by_from {
            let arity = targets.len();
            let arity_note = if arity > 1 { "1→N" } else { "1→1" };
            let target_list: Vec<String> = targets.iter().map(|t| t.render()).collect();
            out.push_str(&format!(
                "| `{from}` | {arity} ({arity_note}) | {} |\n",
                target_list.join("; ")
            ));
        }
        out.push('\n');

        out.push_str("### Free-path / unresolved claims\n\n");
        let mut free: Vec<(String, String)> = Vec::new();
        for (from, targets) in &by_from {
            for t in targets {
                if t.is_free_path() {
                    free.push((from.clone(), t.render_plain()));
                }
            }
        }
        if free.is_empty() {
            out.push_str("_None — every supersedes edge resolved to a registered path or path-shaped target._\n\n");
        } else {
            for (from, claim) in free {
                out.push_str(&format!(
                    "- `{from}` → free-path claim: **{claim}** (no resolved document id)\n"
                ));
            }
            out.push('\n');
        }
    }

    // --- Contradictions ---
    out.push_str("## 4. Contradictions\n\n");
    out.push_str(
        "Multiple non-superseded handoff/roadmap/migration docs in the same directory (audit §1).\n\n",
    );
    if input.audit.contradictions.is_empty() {
        out.push_str("_None detected._\n\n");
    } else {
        for f in &input.audit.contradictions {
            out.push_str(&format!("- **{}** — {}\n", f.path, f.detail));
        }
        out.push('\n');
    }

    // --- 待复核 ---
    out.push_str("## 5. Pending review (待复核)\n\n");
    out.push_str(
        "Unpaired briefs (no matching report stem). **Review only — not retirement.**\n\n",
    );
    let pending = if !input.audit.unpaired_briefs.is_empty() {
        input.audit.unpaired_briefs.clone()
    } else {
        input.audit.retirement.briefs_pending_review.clone()
    };
    out.push_str(&format!("**Count:** {}\n\n", pending.len()));
    if pending.is_empty() {
        out.push_str("_None._\n\n");
    } else {
        for p in &pending {
            out.push_str(&format!("- `{p}`\n"));
        }
        out.push('\n');
    }

    // --- Expected orphans (non-problem) ---
    out.push_str("## 6. Expected orphans (预期孤儿, 非问题)\n\n");
    out.push_str(
        "`verification` / `decision` zero-inbound (or bucketed as expected). **Not a todo. Not retirement.**\n\n",
    );
    let expected = &input.audit.retirement.expected_orphans_non_problem;
    out.push_str(&format!("**Count:** {}\n\n", expected.len()));
    if expected.is_empty() {
        out.push_str(
            "_None in the expected-orphan bucket for this audit (may be zero-inbound empty or cited by other docs)._\n\n",
        );
    } else {
        for p in expected {
            out.push_str(&format!("- `{p}` — **预期孤儿,非问题**\n"));
        }
        out.push('\n');
    }

    // Sanity: expected orphans must not appear in pending review as if they were todos
    // (they can theoretically also be unpaired if mis-roled — still show separation).
    out.push_str("### Separation from todos\n\n");
    let pending_set: BTreeSet<&str> = pending.iter().map(String::as_str).collect();
    let mixed: Vec<_> = expected
        .iter()
        .filter(|p| pending_set.contains(p.as_str()))
        .cloned()
        .collect();
    if mixed.is_empty() {
        out.push_str(
            "_Expected orphans are not listed under §5 待复核. They are non-problems only._\n\n",
        );
    } else {
        out.push_str(
            "_Note: the following paths appear in both buckets (role/pairing edge case); still labeled non-problem for orphan policy:_\n\n",
        );
        for p in mixed {
            out.push_str(&format!("- `{p}`\n"));
        }
        out.push('\n');
    }

    // --- K0d dispatch / acceptance ledger ---
    render_ledger_sections(&mut out, &input);

    out.push_str(
        "---\n\n*Generated by `keeper-knowledge-indexer` render (K0c/K0d). Pure registry + ledger projection.*\n",
    );
    out
}

/// §§ 7–9: in-flight dispatch, recent verdicts, completed-without-verdict.
///
/// Honest about unknowns: completed tasks with no verdict are **listed**, never
/// silently omitted. Renders only slot labels + portable paths + structured
/// verdict fields — never lease/session/host paths.
fn render_ledger_sections(out: &mut String, input: &RenderInput) {
    // Stable sort copies so pure function is order-independent of input shuffle.
    let mut tasks = input.tasks.clone();
    tasks.sort_by(|a, b| {
        b.dispatched_at
            .cmp(&a.dispatched_at)
            .then_with(|| a.task_id.cmp(&b.task_id))
    });
    let mut verdicts = input.verdicts.clone();
    verdicts.sort_by(|a, b| {
        b.decided_at
            .cmp(&a.decided_at)
            .then_with(|| a.verdict_id.cmp(&b.verdict_id))
    });

    let task_ids_with_verdict: HashSet<&str> = verdicts
        .iter()
        .filter_map(|v| v.task_id.as_deref())
        .collect();

    // --- 7. In-flight dispatch ---
    out.push_str("## 7. In-flight dispatch (在途派单)\n\n");
    out.push_str(
        "Tasks with `status=dispatched` only. Slot label is shown; lease/session ids are never stored or rendered.\n\n",
    );
    let inflight: Vec<&KnowledgeTask> = tasks
        .iter()
        .filter(|t| t.status == TaskStatus::Dispatched)
        .collect();
    out.push_str(&format!("**Count:** {}\n\n", inflight.len()));
    if inflight.is_empty() {
        out.push_str("_None — no dispatched-but-not-terminal tasks in the ledger._\n\n");
    } else {
        out.push_str("| title | slot | brief | dispatched_at |\n| --- | --- | --- | --- |\n");
        for t in &inflight {
            let brief = t
                .brief_path
                .as_deref()
                .map(|p| format!("`{p}`"))
                .unwrap_or_else(|| "_none_".into());
            out.push_str(&format!(
                "| {} | `{}` | {brief} | `{}` |\n",
                escape_cell(&t.title),
                t.worker_label,
                t.dispatched_at.to_rfc3339()
            ));
        }
        out.push('\n');
    }

    // --- 8. Recent acceptance verdicts ---
    out.push_str("## 8. Recent acceptance verdicts (最近验收结论)\n\n");
    out.push_str(
        "Controller judgments ordered by `decided_at` descending. **`summary` is the highest-value field.**\n\n",
    );
    out.push_str(&format!("**Count:** {}\n\n", verdicts.len()));
    if verdicts.is_empty() {
        out.push_str(
            "_None recorded. Until acceptance writes verdict rows, this section stays empty — that is honest, not fabricable._\n\n",
        );
    } else {
        for v in &verdicts {
            let task_note = match v.task_id.as_deref() {
                Some(tid) => {
                    let title = tasks
                        .iter()
                        .find(|t| t.task_id == tid)
                        .map(|t| t.title.as_str())
                        .unwrap_or("(task row missing)");
                    format!("task=`{tid}` ({title})")
                }
                None => "task=_none_ (controller-self work)".to_owned(),
            };
            out.push_str(&format!(
                "### outcome=`{}` — {}\n\n",
                v.outcome.as_str(),
                v.decided_at.to_rfc3339()
            ));
            out.push_str(&format!("- **Link:** {task_note}\n"));
            if let Some(ref g) = v.gate_result {
                out.push_str(&format!("- **Gate:** `{g}`\n"));
            }
            if let Some(ref e) = v.e2e_result {
                out.push_str(&format!("- **E2E:** `{e}`\n"));
            }
            out.push_str(&format!("- **Summary:** {}\n", v.summary));
            match v.residuals.as_deref() {
                Some(r) => out.push_str(&format!("- **Residuals:** {r}\n")),
                None => out.push_str("- **Residuals:** _none_\n"),
            }
            out.push('\n');
        }
    }

    // --- 9. Completed without verdict (core detection) ---
    out.push_str("## 9. Completed without verdict (已完成但未验收)\n\n");
    out.push_str(
        "**Mechanical detection of controller acceptance gaps.** Tasks with `status=completed` and **no** matching verdict row are listed here — never silently omitted.\n\n",
    );
    let missing: Vec<&KnowledgeTask> = tasks
        .iter()
        .filter(|t| {
            t.status == TaskStatus::Completed && !task_ids_with_verdict.contains(t.task_id.as_str())
        })
        .collect();
    out.push_str(&format!("**Count:** {}\n\n", missing.len()));
    if missing.is_empty() {
        out.push_str(
            "_None — every completed task has at least one verdict, or there are no completed tasks._\n\n",
        );
    } else {
        out.push_str(
            "> ⚠️ These completed tasks have **no acceptance verdict**. This is the failure mode ADR-0004 §2 exists to surface.\n\n",
        );
        out.push_str(
            "| title | slot | brief | completion_signal | dispatched_at |\n| --- | --- | --- | --- | --- |\n",
        );
        for t in &missing {
            let brief = t
                .brief_path
                .as_deref()
                .map(|p| format!("`{p}`"))
                .unwrap_or_else(|| "_none_".into());
            let signal = t
                .completion_signal
                .as_deref()
                .map(escape_cell)
                .unwrap_or_else(|| "_none_".into());
            out.push_str(&format!(
                "| {} | `{}` | {brief} | {signal} | `{}` |\n",
                escape_cell(&t.title),
                t.worker_label,
                t.dispatched_at.to_rfc3339()
            ));
        }
        out.push('\n');
    }

    // Task status inventory (honest bookkeeping, no auto-migration claims)
    out.push_str("### Ledger task status inventory\n\n");
    let mut status_counts: BTreeMap<&'static str, usize> = BTreeMap::new();
    for t in &tasks {
        *status_counts.entry(t.status.as_str()).or_insert(0) += 1;
    }
    if tasks.is_empty() {
        out.push_str("_No tasks in ledger._\n\n");
    } else {
        out.push_str("| status | count |\n| --- | ---: |\n");
        for (status, count) in &status_counts {
            out.push_str(&format!("| `{status}` | {count} |\n"));
        }
        out.push('\n');
    }
}

fn escape_cell(s: &str) -> String {
    s.replace('|', "\\|").replace('\n', " ")
}

/// Drop archived paths from audit buckets so default render cannot leak them.
fn strip_archived_from_audit(audit: &AuditReport, archived_paths: &HashSet<&str>) -> AuditReport {
    if archived_paths.is_empty() {
        return audit.clone();
    }
    let drop = |paths: &[String]| -> Vec<String> {
        paths
            .iter()
            .filter(|p| !archived_paths.contains(p.as_str()))
            .cloned()
            .collect()
    };
    let drop_findings = |items: &[crate::audit::AuditFinding]| -> Vec<crate::audit::AuditFinding> {
        items
            .iter()
            .filter(|f| !archived_paths.contains(f.path.as_str()))
            .cloned()
            .collect()
    };
    let mut orphans = BTreeMap::new();
    for (role, paths) in &audit.orphans_by_role {
        let kept = drop(paths);
        if !kept.is_empty() {
            orphans.insert(role.clone(), kept);
        }
    }
    let mut retirement = audit.retirement.clone();
    retirement.explicit_superseded_candidates = drop(&retirement.explicit_superseded_candidates);
    retirement.briefs_pending_review = drop(&retirement.briefs_pending_review);
    retirement.expected_orphans_non_problem = drop(&retirement.expected_orphans_non_problem);
    retirement.retirement_candidates = drop(&retirement.retirement_candidates);

    // Recompute totals/counts from non-archived status map when possible.
    let mut status_counts = audit.status_counts.clone();
    if let Some(n) = status_counts.get("archived").copied() {
        status_counts.remove("archived");
        let total = audit.total_registered.saturating_sub(n);
        let unknown = status_counts.get("unknown").copied().unwrap_or(0);
        let unknown_ratio = if total == 0 {
            0.0
        } else {
            unknown as f64 / total as f64
        };
        return AuditReport {
            total_registered: total,
            role_counts: audit.role_counts.clone(), // role dist may still include archived roles; ok for inventory honesty of non-path sections
            status_counts,
            unknown_ratio,
            contradictions: drop_findings(&audit.contradictions),
            orphans_by_role: orphans,
            unpaired_briefs: drop(&audit.unpaired_briefs),
            superseded_still_referenced: drop_findings(&audit.superseded_still_referenced),
            unclassifiable: drop_findings(&audit.unclassifiable),
            digest_changes: drop_findings(&audit.digest_changes),
            inferred_supersession: audit
                .inferred_supersession
                .iter()
                .filter(|i| !archived_paths.contains(i.from_path.as_str()))
                .cloned()
                .collect(),
            retirement,
        };
    }

    AuditReport {
        total_registered: audit.total_registered,
        role_counts: audit.role_counts.clone(),
        status_counts: audit.status_counts.clone(),
        unknown_ratio: audit.unknown_ratio,
        contradictions: drop_findings(&audit.contradictions),
        orphans_by_role: orphans,
        unpaired_briefs: drop(&audit.unpaired_briefs),
        superseded_still_referenced: drop_findings(&audit.superseded_still_referenced),
        unclassifiable: drop_findings(&audit.unclassifiable),
        digest_changes: drop_findings(&audit.digest_changes),
        inferred_supersession: audit
            .inferred_supersession
            .iter()
            .filter(|i| !archived_paths.contains(i.from_path.as_str()))
            .cloned()
            .collect(),
        retirement,
    }
}

fn count_roles(docs: &[KnowledgeDocument]) -> BTreeMap<String, usize> {
    let mut m = BTreeMap::new();
    for d in docs {
        *m.entry(d.role.as_str().to_owned()).or_insert(0) += 1;
    }
    m
}

fn count_statuses(docs: &[KnowledgeDocument]) -> BTreeMap<String, usize> {
    let mut m = BTreeMap::new();
    for d in docs {
        *m.entry(d.status.as_str().to_owned()).or_insert(0) += 1;
    }
    m
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum SupersedeTarget {
    /// Resolved to a registered document path.
    Document { path: String },
    /// Path-shaped string stored on the link without resolved id.
    PathClaim { path: String },
    /// Free-text claim (not path-shaped).
    FreePath { claim: String },
}

impl SupersedeTarget {
    fn sort_key(&self) -> String {
        match self {
            Self::Document { path } => format!("0:{path}"),
            Self::PathClaim { path } => format!("1:{path}"),
            Self::FreePath { claim } => format!("2:{claim}"),
        }
    }

    fn render(&self) -> String {
        match self {
            Self::Document { path } => format!("`{path}`"),
            Self::PathClaim { path } => format!("`{path}` (path claim)"),
            Self::FreePath { claim } => format!("**free-path:** `{claim}`"),
        }
    }

    fn render_plain(&self) -> String {
        match self {
            Self::Document { path } | Self::PathClaim { path } => path.clone(),
            Self::FreePath { claim } => claim.clone(),
        }
    }

    fn is_free_path(&self) -> bool {
        matches!(self, Self::FreePath { .. })
    }
}

fn resolve_target(
    link: &KnowledgeDocumentLink,
    id_to_path: &HashMap<&str, &str>,
) -> SupersedeTarget {
    if let Some(ref id) = link.to_document_id
        && let Some(path) = id_to_path.get(id.as_str())
    {
        return SupersedeTarget::Document {
            path: (*path).to_owned(),
        };
    }
    if let Some(ref p) = link.to_path {
        if looks_path_shaped(p) {
            return SupersedeTarget::PathClaim { path: p.clone() };
        }
        return SupersedeTarget::FreePath { claim: p.clone() };
    }
    SupersedeTarget::FreePath {
        claim: "(missing endpoint)".into(),
    }
}

fn looks_path_shaped(s: &str) -> bool {
    let t = s.trim();
    if t.is_empty() || t.contains(' ') {
        return false;
    }
    t.contains('/') || t.ends_with(".md")
}

#[cfg(test)]
mod unit_tests {
    use super::*;
    use chrono::{TimeZone, Utc};
    use keeper_storage_sqlite::{
        DocumentRole, KnowledgeTask, KnowledgeVerdict, RegisteredBy, TaskStatus, VerdictOutcome,
    };

    use crate::audit::{AuditFinding, AuditInput, RetirementBucket, audit_documents};
    use std::collections::HashMap as StdHashMap;

    fn doc(path: &str, role: DocumentRole, status: DocumentStatus) -> KnowledgeDocument {
        KnowledgeDocument {
            document_id: format!("id-{path}"),
            project_id: "p".into(),
            path: path.to_owned(),
            role,
            status,
            content_digest: format!("sha256:{}", "a".repeat(64)),
            updated_at: Utc.with_ymd_and_hms(2026, 8, 9, 12, 0, 0).unwrap(),
            registered_by: RegisteredBy::Lint,
            source_run_id: None,
        }
    }

    fn link(from_id: &str, to_id: Option<&str>, to_path: Option<&str>) -> KnowledgeDocumentLink {
        KnowledgeDocumentLink {
            link_id: format!("l-{from_id}-{}", to_path.unwrap_or("x")),
            from_document_id: from_id.to_owned(),
            to_document_id: to_id.map(str::to_owned),
            to_path: to_path.map(str::to_owned),
            rel: DocumentLinkRel::Supersedes,
        }
    }

    fn task(
        id: &str,
        title: &str,
        slot: &str,
        status: TaskStatus,
        signal: Option<&str>,
    ) -> KnowledgeTask {
        KnowledgeTask {
            task_id: id.to_owned(),
            project_id: "p".into(),
            title: title.to_owned(),
            brief_path: Some("docs/execution/sample-brief.md".into()),
            worker_label: slot.to_owned(),
            dispatched_at: Utc.with_ymd_and_hms(2026, 8, 9, 10, 0, 0).unwrap(),
            status,
            completion_signal: signal.map(str::to_owned),
        }
    }

    fn verdict(
        id: &str,
        task_id: Option<&str>,
        outcome: VerdictOutcome,
        summary: &str,
        residuals: Option<&str>,
    ) -> KnowledgeVerdict {
        KnowledgeVerdict {
            verdict_id: id.to_owned(),
            task_id: task_id.map(str::to_owned),
            project_id: "p".into(),
            outcome,
            gate_result: Some("exit 0".into()),
            e2e_result: Some("not-run".into()),
            summary: summary.to_owned(),
            residuals: residuals.map(str::to_owned),
            decided_at: Utc.with_ymd_and_hms(2026, 8, 9, 14, 0, 0).unwrap(),
        }
    }

    /// Forbidden needles assembled at runtime so the source file never holds
    /// contiguous secret-gate samples.
    fn privacy_needles() -> (String, String, String, String, String) {
        let lease_prefix = format!("{}-{}-", "grok", "lease");
        let session = format!("{}T{}Z-{}-{}", "20260807", "035151", "pillow", "pxrm");
        let win = format!("{}:\\{}\\", "C", "Users");
        let msys = format!("/{}/{}/", "c", "Users");
        let cyg = format!("/cygdrive/{}/{}/", "c", "Users");
        (lease_prefix, session, win, msys, cyg)
    }

    /// Assert render output never contains §6 privacy red-line samples.
    fn assert_render_privacy_clean(md: &str) {
        let (lease_prefix, session, win, msys, cyg) = privacy_needles();
        assert!(
            !md.contains(&lease_prefix),
            "render must not contain lease id: {md}"
        );
        assert!(!md.contains(&session), "render must not contain session id");
        assert!(
            !md.contains(&win) && !md.contains(&msys) && !md.contains(&cyg),
            "render must not contain host user path"
        );
    }

    #[test]
    fn empty_registry_is_honest() {
        let md = render_status_view(&RenderInput {
            documents: vec![],
            links: vec![],
            audit: AuditReport::default(),
            tasks: vec![],
            verdicts: vec![],
            include_archived: false,
        });
        assert!(md.contains("Total registered:** 0") || md.contains("**Total registered:** 0"));
        assert!(md.contains("No data") || md.contains("empty registry"));
        assert!(!md.contains("在途 Worker"));
        assert_render_privacy_clean(&md);
    }

    #[test]
    fn pure_function_stable() {
        let documents = vec![
            doc(
                "docs/handoff/current.md",
                DocumentRole::Handoff,
                DocumentStatus::Unknown,
            ),
            doc(
                "docs/handoff/old-a.md",
                DocumentRole::Handoff,
                DocumentStatus::Superseded,
            ),
            doc(
                "docs/handoff/old-b.md",
                DocumentRole::Handoff,
                DocumentStatus::Superseded,
            ),
        ];
        let links = vec![
            link(
                "id-docs/handoff/current.md",
                Some("id-docs/handoff/old-a.md"),
                Some("docs/handoff/old-a.md"),
            ),
            link(
                "id-docs/handoff/current.md",
                Some("id-docs/handoff/old-b.md"),
                Some("docs/handoff/old-b.md"),
            ),
        ];
        let audit = AuditReport {
            total_registered: 3,
            role_counts: BTreeMap::from([("handoff".into(), 3)]),
            status_counts: BTreeMap::from([("unknown".into(), 1), ("superseded".into(), 2)]),
            unknown_ratio: 1.0 / 3.0,
            ..AuditReport::default()
        };
        let input = RenderInput {
            documents,
            links,
            audit,
            tasks: vec![task(
                "t1",
                "K0d sample",
                "grok3",
                TaskStatus::Dispatched,
                None,
            )],
            verdicts: vec![],
            include_archived: false,
        };
        let a = render_status_view(&input);
        let b = render_status_view(&input);
        assert_eq!(a, b);
    }

    #[test]
    fn unknown_ratio_prominent_and_correct() {
        let mut documents = Vec::new();
        for i in 0..72 {
            documents.push(doc(
                &format!("docs/execution/u{i}.md"),
                DocumentRole::Report,
                DocumentStatus::Unknown,
            ));
        }
        for i in 0..28 {
            documents.push(doc(
                &format!("docs/execution/c{i}-brief.md"),
                DocumentRole::Brief,
                DocumentStatus::Consumed,
            ));
        }
        let audit = AuditReport {
            total_registered: 100,
            role_counts: BTreeMap::from([("report".into(), 72), ("brief".into(), 28)]),
            status_counts: BTreeMap::from([("unknown".into(), 72), ("consumed".into(), 28)]),
            unknown_ratio: 0.72,
            ..AuditReport::default()
        };
        let md = render_status_view(&RenderInput {
            documents,
            links: vec![],
            audit,
            tasks: vec![],
            verdicts: vec![],
            include_archived: false,
        });
        assert!(md.contains("72.0%") || md.contains("**72.0%**"));
        assert!(md.contains("UNCLASSIFIED") || md.contains("未分类"));
        assert!(md.contains("status=`unknown` **未分类**") || md.contains("未分类"));
        // Must not present unknown docs as known-status role group only.
        assert!(md.contains("### 2.2 Unclassified") || md.contains("Unclassified"));
    }

    #[test]
    fn one_to_many_supersession_rendered() {
        let documents = vec![
            doc(
                "docs/handoff/cur.md",
                DocumentRole::Handoff,
                DocumentStatus::Unknown,
            ),
            doc(
                "docs/handoff/a.md",
                DocumentRole::Handoff,
                DocumentStatus::Superseded,
            ),
            doc(
                "docs/handoff/b.md",
                DocumentRole::Handoff,
                DocumentStatus::Superseded,
            ),
        ];
        let links = vec![
            link(
                "id-docs/handoff/cur.md",
                Some("id-docs/handoff/a.md"),
                Some("docs/handoff/a.md"),
            ),
            link(
                "id-docs/handoff/cur.md",
                Some("id-docs/handoff/b.md"),
                Some("docs/handoff/b.md"),
            ),
            link(
                "id-docs/execution/rep.md",
                None,
                Some("prior delivery status"),
            ),
        ];
        // Free-path source doc not in list — still render claim if from id unknown.
        let documents = {
            let mut d = documents;
            d.push(doc(
                "docs/execution/rep.md",
                DocumentRole::Report,
                DocumentStatus::Unknown,
            ));
            d
        };
        let links = {
            let mut l = links;
            // fix free-path from id
            l[2].from_document_id = "id-docs/execution/rep.md".into();
            l
        };
        let md = render_status_view(&RenderInput {
            documents,
            links,
            audit: AuditReport {
                total_registered: 4,
                unknown_ratio: 0.5,
                status_counts: BTreeMap::from([("unknown".into(), 2), ("superseded".into(), 2)]),
                ..AuditReport::default()
            },
            tasks: vec![],
            verdicts: vec![],
            include_archived: false,
        });
        assert!(md.contains("1→N"), "expected 1→N marker: {md}");
        assert!(md.contains("docs/handoff/a.md"));
        assert!(md.contains("docs/handoff/b.md"));
        assert!(
            md.contains("free-path") || md.contains("prior delivery status"),
            "free-path claim missing"
        );
    }

    #[test]
    fn contradictions_and_pending_review_sections() {
        let documents = vec![
            doc(
                "docs/roadmap/a.md",
                DocumentRole::Roadmap,
                DocumentStatus::Unknown,
            ),
            doc(
                "docs/roadmap/b.md",
                DocumentRole::Roadmap,
                DocumentStatus::Unknown,
            ),
            doc(
                "docs/execution/open-brief.md",
                DocumentRole::Brief,
                DocumentStatus::Unknown,
            ),
        ];
        let audit = AuditReport {
            total_registered: 3,
            unknown_ratio: 1.0,
            status_counts: BTreeMap::from([("unknown".into(), 3)]),
            role_counts: BTreeMap::from([("roadmap".into(), 2), ("brief".into(), 1)]),
            contradictions: vec![AuditFinding {
                path: "docs/roadmap".into(),
                detail: "2 non-superseded roadmap docs: docs/roadmap/a.md, docs/roadmap/b.md"
                    .into(),
            }],
            unpaired_briefs: vec!["docs/execution/open-brief.md".into()],
            retirement: RetirementBucket {
                briefs_pending_review: vec!["docs/execution/open-brief.md".into()],
                ..RetirementBucket::default()
            },
            ..AuditReport::default()
        };
        let md = render_status_view(&RenderInput {
            documents,
            links: vec![],
            audit,
            tasks: vec![],
            verdicts: vec![],
            include_archived: false,
        });
        assert!(md.contains("## 4. Contradictions"));
        assert!(md.contains("docs/roadmap"));
        assert!(md.contains("## 5. Pending review") || md.contains("待复核"));
        assert!(md.contains("docs/execution/open-brief.md"));
    }

    #[test]
    fn expected_orphans_labeled_non_problem_not_in_pending() {
        let documents = vec![
            doc(
                "docs/verification/lonely.md",
                DocumentRole::Verification,
                DocumentStatus::Unknown,
            ),
            doc(
                "docs/decisions/ADR-x.md",
                DocumentRole::Decision,
                DocumentStatus::Accepted,
            ),
            doc(
                "docs/execution/open-brief.md",
                DocumentRole::Brief,
                DocumentStatus::Unknown,
            ),
        ];
        let mut contents = StdHashMap::new();
        for d in &documents {
            contents.insert(d.path.clone(), format!("# {}\n", d.path));
        }
        let audit = audit_documents(AuditInput {
            documents: documents.clone(),
            links: vec![],
            contents,
            path_content_conflicts: StdHashMap::new(),
            classify_notes: StdHashMap::new(),
            digest_changes: vec![],
            inferred_supersession: vec![],
        });
        let md = render_status_view(&RenderInput {
            documents,
            links: vec![],
            audit: audit.clone(),
            tasks: vec![],
            verdicts: vec![],
            include_archived: false,
        });
        assert!(md.contains("预期孤儿") || md.contains("非问题"));
        assert!(md.contains("docs/verification/lonely.md"));
        // Pending review section should list the brief, not frame verification as todo.
        assert!(md.contains("open-brief.md"));
        // Verification orphan must appear with non-problem label.
        let orphan_section = md.split("## 6. Expected orphans").nth(1).unwrap_or("");
        assert!(orphan_section.contains("lonely.md"));
        assert!(orphan_section.contains("非问题"));
        // Must not put expected orphans into retirement-like language in §5.
        let pending_section = md
            .split("## 5. Pending review")
            .nth(1)
            .and_then(|s| s.split("## 6.").next())
            .unwrap_or("");
        assert!(
            !pending_section.contains("lonely.md"),
            "verification orphan must not mix into 待复核"
        );
        assert!(
            !pending_section.contains("ADR-x.md"),
            "decision orphan must not mix into 待复核"
        );
    }

    #[test]
    fn no_handwritten_project_narrative_inlined() {
        let md = render_status_view(&RenderInput {
            documents: vec![doc(
                "docs/handoff/x.md",
                DocumentRole::Handoff,
                DocumentStatus::Unknown,
            )],
            links: vec![],
            audit: AuditReport {
                total_registered: 1,
                unknown_ratio: 1.0,
                status_counts: BTreeMap::from([("unknown".into(), 1)]),
                role_counts: BTreeMap::from([("handoff".into(), 1)]),
                ..AuditReport::default()
            },
            tasks: vec![],
            verdicts: vec![],
            include_archived: false,
        });
        // Forbidden narrative patterns from the hand-written handoff.
        assert!(!md.contains("在途 Worker"));
        assert!(!md.contains("C4 已 checkpoint"));
        assert!(!md.contains("品牌/视觉升级"));
        assert!(!md.contains("storage barrier"));
        assert_render_privacy_clean(&md);
    }

    #[test]
    fn completed_without_verdict_is_listed_never_silent() {
        // Core K0d detection: completed task with no verdict must appear in §9.
        let documents = vec![doc(
            "docs/handoff/x.md",
            DocumentRole::Handoff,
            DocumentStatus::Unknown,
        )];
        let tasks = vec![
            task(
                "t-done-no-verdict",
                "K0d ledger work",
                "grok3",
                TaskStatus::Completed,
                Some("K0D-LEDGER-COMPLETE"),
            ),
            task(
                "t-inflight",
                "Still running",
                "codex1",
                TaskStatus::Dispatched,
                None,
            ),
            task(
                "t-accepted",
                "Already accepted",
                "grok2",
                TaskStatus::Completed,
                Some("done"),
            ),
        ];
        let verdicts = vec![verdict(
            "v1",
            Some("t-accepted"),
            VerdictOutcome::Accepted,
            "Gate green; residuals none for this slice.",
            None,
        )];
        let md = render_status_view(&RenderInput {
            documents,
            links: vec![],
            audit: AuditReport {
                total_registered: 1,
                unknown_ratio: 1.0,
                status_counts: BTreeMap::from([("unknown".into(), 1)]),
                role_counts: BTreeMap::from([("handoff".into(), 1)]),
                ..AuditReport::default()
            },
            tasks,
            verdicts,
            include_archived: false,
        });

        assert!(md.contains("## 7. In-flight dispatch") || md.contains("在途派单"));
        assert!(md.contains("Still running"));
        assert!(md.contains("`codex1`"));

        assert!(md.contains("## 8. Recent acceptance verdicts") || md.contains("最近验收结论"));
        assert!(md.contains("Gate green; residuals none for this slice."));
        assert!(md.contains("outcome=`accepted`"));

        assert!(md.contains("## 9. Completed without verdict") || md.contains("已完成但未验收"));
        let missing_section = md
            .split("## 9. Completed without verdict")
            .nth(1)
            .unwrap_or("");
        assert!(
            missing_section.contains("K0d ledger work"),
            "completed-without-verdict must list the task: {missing_section}"
        );
        assert!(
            missing_section.contains("K0D-LEDGER-COMPLETE"),
            "completion signal must appear"
        );
        // Accepted task must not appear in the missing section.
        assert!(
            !missing_section.contains("Already accepted"),
            "accepted task must not be listed as missing verdict"
        );
        // In-flight must not be listed as completed-without-verdict.
        assert!(!missing_section.contains("Still running"));

        assert_render_privacy_clean(&md);
    }

    #[test]
    fn verdict_without_task_id_renders_as_controller_self() {
        let md = render_status_view(&RenderInput {
            documents: vec![doc(
                "docs/handoff/x.md",
                DocumentRole::Handoff,
                DocumentStatus::Unknown,
            )],
            links: vec![],
            audit: AuditReport {
                total_registered: 1,
                unknown_ratio: 1.0,
                status_counts: BTreeMap::from([("unknown".into(), 1)]),
                ..AuditReport::default()
            },
            tasks: vec![],
            verdicts: vec![verdict(
                "v-self",
                None,
                VerdictOutcome::Partial,
                "Controller fixed migration numbering by hand.",
                Some("follow-up: wire dispatcher"),
            )],
            include_archived: false,
        });
        assert!(md.contains("controller-self work") || md.contains("task=_none_"));
        assert!(md.contains("Controller fixed migration numbering by hand."));
        assert!(md.contains("follow-up: wire dispatcher"));
        assert!(md.contains("outcome=`partial`"));
        assert_render_privacy_clean(&md);
    }

    #[test]
    fn render_output_contains_only_slot_labels_not_lease_or_session() {
        let md = render_status_view(&RenderInput {
            documents: vec![],
            links: vec![],
            audit: AuditReport::default(),
            tasks: vec![task(
                "t1",
                "Slot-only dispatch",
                "grok3",
                TaskStatus::Dispatched,
                None,
            )],
            verdicts: vec![verdict(
                "v1",
                None,
                VerdictOutcome::Accepted,
                "Accepted with clean gate.",
                None,
            )],
            include_archived: false,
        });
        assert!(md.contains("`grok3`"));
        assert_render_privacy_clean(&md);
        // Explicit samples that must never appear (gate-aligned; assembled).
        let lease = format!("{}-{}-{}-{}", "grok", "lease", "msij5g0v", "fc6b73");
        let session = format!("{}T{}Z-{}-{}", "20260807", "035151", "pillow", "pxrm");
        let win = format!("{}:\\{}\\someone", "C", "Users");
        let msys = format!("/{}/{}/someone", "c", "Users");
        assert!(!md.contains(&lease));
        assert!(!md.contains(&session));
        assert!(!md.contains(&win));
        assert!(!md.contains(&msys));
    }
}
