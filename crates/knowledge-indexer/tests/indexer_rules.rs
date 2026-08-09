//! K0b/K0c acceptance tests — classification, supersession, idempotency, policy, render.

use std::fs;
use std::path::Path;

use keeper_knowledge_indexer::{
    RenderInput, ScanOptions, classify_document, extract_supersession, render_handoff_view,
    render_status_view, scan_and_register, validate_path_ssot,
};
use keeper_project::CreateProject;
use keeper_project::validate_portable_path;
use keeper_storage_sqlite::{
    DocumentRole, DocumentStatus, RegisterDocument, RegisteredBy, SqliteKnowledgeRepository,
    SqliteProjectRepository, UpdateDocument,
};

fn write(path: &Path, body: &str) {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).unwrap();
    }
    fs::write(path, body).unwrap();
}

// ---------------------------------------------------------------------------
// 1. Classification rules (incl. path/content conflict â†’ other)
// ---------------------------------------------------------------------------

#[test]
fn classify_directory_and_filename_rules() {
    let brief = classify_document(
        "docs/execution/2026-08-08-wave-x-brief.md",
        "# Task brief\n\nDo the thing.\n",
    );
    assert_eq!(brief.role, DocumentRole::Brief);
    assert_eq!(brief.status, DocumentStatus::Unknown);

    let report = classify_document(
        "docs/execution/2026-08-08-wave-x-report.md",
        "# Report\n\nStatus: DONE\n",
    );
    assert_eq!(report.role, DocumentRole::Report);
    assert_eq!(report.status, DocumentStatus::Unknown);

    let verification = classify_document(
        "docs/verification/2026-08-01-wave-c3-acceptance.md",
        "# Acceptance\n\nPassed.\n",
    );
    assert_eq!(verification.role, DocumentRole::Verification);

    let plan = classify_document(
        "docs/superpowers/plans/2026-08-01-c3-implementation.md",
        "# Plan\n",
    );
    assert_eq!(plan.role, DocumentRole::Plan);

    let research_misfit = classify_document(
        "docs/execution/2026-08-08-ui-premium-research.md",
        "# UI research\n",
    );
    assert_eq!(research_misfit.role, DocumentRole::Research);
}

#[test]
fn path_content_conflict_yields_other() {
    // handoff dir + worker-brief content markers â†’ other (do not invent handoff).
    let c = classify_document(
        "docs/handoff/2026-08-08-misplaced-worker-brief.md",
        "# Worker brief\n\nWorker æ§½ä½: grok2\n\n## ç²¾ç¡®å†™å…¥ç™½åå•\n\n- crates/foo\n",
    );
    assert_eq!(c.role, DocumentRole::Other);
    assert!(c.path_content_conflict);
}

// ---------------------------------------------------------------------------
// 2. Unsure â†’ unknown (never default active)
// ---------------------------------------------------------------------------

#[test]
fn unsure_status_is_unknown_never_active() {
    let samples = [
        (
            "docs/execution/2026-08-08-flake-analysis-report.md",
            "# Analysis\n\nDONE\n",
        ),
        (
            "docs/research/2026-08-08-knowledge-layer-research.md",
            "# Research\n",
        ),
        (
            "docs/handoff/2026-08-08-pillow-controller-handoff.md",
            "# Handoff\n\nCurrent state.\n",
        ),
        ("docs/verification/worker-gate-entry.md", "# Gate entry\n"),
    ];
    for (path, body) in samples {
        let c = classify_document(path, body);
        assert_eq!(
            c.status,
            DocumentStatus::Unknown,
            "{path} must be unknown when unsure"
        );
        assert_ne!(c.status, DocumentStatus::Active);
    }
}

#[test]
fn adr_accepted_and_proposed_status() {
    let adr = classify_document(
        "docs/decisions/ADR-0001-clean-successor.md",
        "Date: 2026-07-29\n\nStatus: Accepted\n\n## Context\n",
    );
    assert_eq!(adr.role, DocumentRole::Decision);
    assert_eq!(adr.status, DocumentStatus::Accepted);

    let proposed = classify_document(
        "docs/superpowers/specs/2026-08-08-settings-credentials-design.md",
        "# Design\n\nStatus: Proposed â€” not authorized for implementation\n",
    );
    assert_eq!(proposed.status, DocumentStatus::Proposed);
}

// ---------------------------------------------------------------------------
// 3â€“4. Explicit supersession extraction; inferred not stored; 1â†’N
// ---------------------------------------------------------------------------

#[test]
fn explicit_one_to_many_and_inferred_not_explicit() {
    let content = r#"> 本文件取代 `docs/handoff/2026-08-07-pillow-controller-handoff.md` 与
> `docs/handoff/2026-08-08-round5-tech-debt-status.md`。旧文件描述的是 C4,
"#;
    let ext = extract_supersession(
        "docs/handoff/2026-08-08-pillow-controller-handoff.md",
        content,
    );
    assert_eq!(ext.explicit.len(), 2, "1â†’N explicit supersession");
    assert!(ext.inferred.is_empty());

    let soft = extract_supersession(
        "docs/roadmap/a.md",
        "current state moved to `docs/roadmap/b.md`\n",
    );
    assert!(
        soft.explicit.is_empty(),
        "inferred must not be treated as explicit"
    );
    assert_eq!(soft.inferred.len(), 1);
}

#[test]
fn scan_registers_one_to_many_links_not_inferred() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();
    write(
        &root.join("docs/handoff/current.md"),
        "> 本文件取代 `docs/handoff/old-a.md` 与\n> `docs/handoff/old-b.md`。\n",
    );
    write(&root.join("docs/handoff/old-a.md"), "# Old A\n");
    write(&root.join("docs/handoff/old-b.md"), "# Old B\n");
    write(
        &root.join("docs/roadmap/soft.md"),
        "current state moved to `docs/roadmap/target.md`\n",
    );
    write(&root.join("docs/roadmap/target.md"), "# Target\n");

    let db = root.join("k.db");
    let result = scan_and_register(&ScanOptions {
        repo_root: root.to_path_buf(),
        db_path: db.clone(),
        project_name: "t".into(),
        registered_by: RegisteredBy::Lint,
        source_run_id: None,
        docs_relative: "docs".into(),
        exclude_paths: vec![],
    })
    .expect("scan");

    let supersede_links: Vec<_> = result
        .links
        .iter()
        .filter(|l| l.rel.as_str() == "supersedes")
        .collect();
    assert_eq!(
        supersede_links.len(),
        2,
        "only explicit 1â†’2 edges in DB, not soft pointer"
    );

    let old_a = result
        .documents
        .iter()
        .find(|d| d.path.ends_with("old-a.md"))
        .unwrap();
    assert_eq!(old_a.status, DocumentStatus::Superseded);
    let old_b = result
        .documents
        .iter()
        .find(|d| d.path.ends_with("old-b.md"))
        .unwrap();
    assert_eq!(old_b.status, DocumentStatus::Superseded);

    // Soft pointer only in report inferred section.
    assert!(
        result
            .inferred_supersession
            .iter()
            .any(|i| i.from_path.contains("soft.md"))
    );
}

// ---------------------------------------------------------------------------
// 5. Idempotent double scan
// ---------------------------------------------------------------------------

#[test]
fn double_scan_is_idempotent() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();
    write(
        &root.join("docs/execution/2026-08-08-demo-brief.md"),
        "# Brief\n",
    );
    write(
        &root.join("docs/execution/2026-08-08-demo-report.md"),
        "# Report\n",
    );
    write(
        &root.join("docs/decisions/ADR-0009-test.md"),
        "Status: Accepted\n\n# ADR\n",
    );

    let db = root.join("k.db");
    let opts = ScanOptions {
        repo_root: root.to_path_buf(),
        db_path: db.clone(),
        project_name: "t".into(),
        registered_by: RegisteredBy::Lint,
        source_run_id: Some("run1".into()),
        docs_relative: "docs".into(),
        exclude_paths: vec![],
    };
    let first = scan_and_register(&opts).unwrap();
    let second = scan_and_register(&opts).unwrap();

    assert_eq!(first.documents.len(), second.documents.len());
    assert_eq!(first.links.len(), second.links.len());
    assert_eq!(second.summary.registered_new, 0);
    // Second pass updates existing rows but does not grow count.
    let repo = SqliteKnowledgeRepository::open(&db).unwrap();
    let listed = repo.list_by_project(&first.project_id).unwrap();
    assert_eq!(listed.len(), first.documents.len());
}

// ---------------------------------------------------------------------------
// 6. Rescan does not overwrite non-unknown status
// ---------------------------------------------------------------------------

#[test]
fn rescan_preserves_non_unknown_status() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();
    let path = "docs/handoff/live.md";
    write(
        &root.join(path),
        "# Handoff\n\nLooks current but scanner would say unknown.\n",
    );

    let db = root.join("k.db");
    let opts = ScanOptions {
        repo_root: root.to_path_buf(),
        db_path: db.clone(),
        project_name: "t".into(),
        registered_by: RegisteredBy::Lint,
        source_run_id: None,
        docs_relative: "docs".into(),
        exclude_paths: vec![],
    };
    let first = scan_and_register(&opts).unwrap();
    let doc = first
        .documents
        .iter()
        .find(|d| d.path == path)
        .expect("registered");
    assert_eq!(doc.status, DocumentStatus::Unknown);

    // Controller marks active explicitly.
    let repo = SqliteKnowledgeRepository::open(&db).unwrap();
    repo.update(UpdateDocument {
        document_id: doc.document_id.clone(),
        role: None,
        status: Some(DocumentStatus::Active),
        content_digest: None,
        registered_by: Some(RegisteredBy::Controller),
        source_run_id: None,
    })
    .unwrap();

    // Change content so digest updates; status must stay active.
    write(
        &root.join(path),
        "# Handoff\n\nUpdated body for digest change.\n",
    );
    let second = scan_and_register(&opts).unwrap();
    let again = second.documents.iter().find(|d| d.path == path).unwrap();
    assert_eq!(
        again.status,
        DocumentStatus::Active,
        "scanner must not clobber non-unknown status"
    );
    assert!(second.summary.digest_changed >= 1);
    assert!(second.summary.status_preserved_non_unknown >= 1);
}

// ---------------------------------------------------------------------------
// 7. verification/decision orphans never in retirement candidates
// ---------------------------------------------------------------------------

#[test]
fn verification_decision_orphans_not_retirement_candidates() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();
    write(
        &root.join("docs/verification/lonely-acceptance.md"),
        "# Acceptance record\n\nNobody links this.\n",
    );
    write(
        &root.join("docs/decisions/ADR-0099-orphan.md"),
        "Status: Accepted\n\n# Orphan ADR\n",
    );
    write(
        &root.join("docs/handoff/new.md"),
        "> 本文件取代 `docs/handoff/old.md`。\n",
    );
    write(&root.join("docs/handoff/old.md"), "# Old\n");

    let result = scan_and_register(&ScanOptions {
        repo_root: root.to_path_buf(),
        db_path: root.join("k.db"),
        project_name: "t".into(),
        registered_by: RegisteredBy::Lint,
        source_run_id: None,
        docs_relative: "docs".into(),
        exclude_paths: vec![],
    })
    .unwrap();

    assert!(
        result
            .report
            .retirement
            .expected_orphans_non_problem
            .iter()
            .any(|p| p.contains("lonely-acceptance"))
    );
    assert!(
        result
            .report
            .retirement
            .expected_orphans_non_problem
            .iter()
            .any(|p| p.contains("ADR-0099"))
    );

    for p in &result.report.retirement.retirement_candidates {
        assert!(
            !p.contains("lonely-acceptance") && !p.contains("ADR-0099"),
            "verification/decision orphan leaked into retirement: {p}"
        );
    }
    assert!(
        result
            .report
            .retirement
            .retirement_candidates
            .iter()
            .any(|p| p.contains("old.md"))
    );
}

// ---------------------------------------------------------------------------
// 8. Path validation SSoT — COM¹ (U+00B9) rejected
// ---------------------------------------------------------------------------

#[test]
fn path_validation_uses_ssot_rejects_com_superscript() {
    // U+00B9 SUPERSCRIPT ONE — same device-name form as kernel-project contract.
    let bad = "docs/COM\u{00b9}/note.md".to_string();
    let ssot = validate_portable_path(&bad);
    let via_indexer = validate_path_ssot(&bad);
    assert!(ssot.is_err(), "SSoT must reject COM\\u{{00b9}}");
    assert_eq!(
        ssot.err(),
        via_indexer.err(),
        "indexer must not reimplement path rules"
    );
    assert_eq!(ssot.unwrap_err(), "windows device name");

    // Register path through knowledge repo also rejects (storage uses same SSoT).
    let temp = tempfile::tempdir().unwrap();
    let db = temp.path().join("k.db");
    let projects = SqliteProjectRepository::open(&db).unwrap();
    let root = temp.path().join("proj");
    fs::create_dir_all(&root).unwrap();
    let project = projects
        .create(CreateProject {
            name: "p".into(),
            root_path: root.to_string_lossy().into_owned(),
        })
        .unwrap();
    let repo = SqliteKnowledgeRepository::open(&db).unwrap();
    let err = repo
        .register(RegisterDocument {
            project_id: project.id.0.to_string(),
            path: bad.to_owned(),
            role: DocumentRole::Other,
            status: None,
            content_digest: format!("sha256:{}", "0".repeat(64)),
            registered_by: RegisteredBy::Lint,
            source_run_id: None,
        })
        .unwrap_err();
    assert!(
        format!("{err}").contains("portable") || err.code() == "knowledge_invalid_path",
        "got {err:?}"
    );
}

// ---------------------------------------------------------------------------
// Paired brief â†’ consumed
// ---------------------------------------------------------------------------

#[test]
fn paired_brief_becomes_consumed() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();
    write(
        &root.join("docs/execution/2026-08-08-feature-brief.md"),
        "# Brief\n",
    );
    write(
        &root.join("docs/execution/2026-08-08-feature-report.md"),
        "# Report\n",
    );
    write(
        &root.join("docs/execution/2026-08-08-open-brief.md"),
        "# Open brief without report\n",
    );

    let result = scan_and_register(&ScanOptions {
        repo_root: root.to_path_buf(),
        db_path: root.join("k.db"),
        project_name: "t".into(),
        registered_by: RegisteredBy::Lint,
        source_run_id: None,
        docs_relative: "docs".into(),
        exclude_paths: vec![],
    })
    .unwrap();

    let paired = result
        .documents
        .iter()
        .find(|d| d.path.contains("feature-brief"))
        .unwrap();
    assert_eq!(paired.status, DocumentStatus::Consumed);

    let open = result
        .documents
        .iter()
        .find(|d| d.path.contains("open-brief"))
        .unwrap();
    assert_eq!(open.status, DocumentStatus::Unknown);
    assert!(
        result
            .report
            .unpaired_briefs
            .iter()
            .any(|p| p.contains("open-brief"))
    );
}

// ---------------------------------------------------------------------------
// K0c — registry status render (pure view)
// ---------------------------------------------------------------------------

#[test]
fn render_from_scan_is_pure_and_covers_buckets() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();
    write(
        &root.join("docs/handoff/current.md"),
        "> 本文件取代 `docs/handoff/old-a.md` 与\n> `docs/handoff/old-b.md`。\n",
    );
    write(&root.join("docs/handoff/old-a.md"), "# Old A\n");
    write(&root.join("docs/handoff/old-b.md"), "# Old B\n");
    write(&root.join("docs/roadmap/r1.md"), "# Roadmap one\n");
    write(&root.join("docs/roadmap/r2.md"), "# Roadmap two\n");
    write(&root.join("docs/execution/open-brief.md"), "# Open brief\n");
    write(
        &root.join("docs/verification/lonely-acceptance.md"),
        "# Acceptance\n",
    );
    write(
        &root.join("docs/decisions/ADR-orphan.md"),
        "Status: Accepted\n\n# ADR\n",
    );
    write(&root.join("docs/execution/done-brief.md"), "# Brief\n");
    write(
        &root.join("docs/execution/done-report.md"),
        "This report supersedes prior delivery status.\n",
    );

    let result = scan_and_register(&ScanOptions {
        repo_root: root.to_path_buf(),
        db_path: root.join("k.db"),
        project_name: "t".into(),
        registered_by: RegisteredBy::Lint,
        source_run_id: Some("k0c".into()),
        docs_relative: "docs".into(),
        exclude_paths: vec![],
    })
    .unwrap();

    let md = result.handoff_markdown.clone();
    let again = render_handoff_view(&RenderInput {
        documents: result.documents.clone(),
        links: result.links.clone(),
        audit: result.report.clone(),
        tasks: vec![],
        verdicts: vec![],
        include_archived: false,
    });
    assert_eq!(md, again, "handoff render must be pure");

    // K1b five body paragraphs present
    assert!(md.contains("当前在途") || md.contains("Current in-flight"));
    assert!(md.contains("待验收") || md.contains("Pending acceptance"));
    assert!(md.contains("最近验收结论") || md.contains("Recent acceptance"));
    assert!(md.contains("文档状态") || md.contains("Document status"));
    assert!(md.contains("替不掉") || md.contains("cannot replace"));

    // empty ledger → 无在途 (negative control)
    assert!(md.contains("无在途"));

    // unknown ratio present
    assert!(
        md.contains("unknown") && (md.contains("UNCLASSIFIED") || md.contains("未分类")),
        "unknown ratio must be prominent"
    );

    // 1→N supersession
    assert!(md.contains("1→N") || md.contains("old-a.md"), "1→N chain");
    assert!(md.contains("old-b.md"));

    // free-path claim from report
    assert!(
        md.contains("free-path") || md.contains("prior delivery status"),
        "free-path claim must render"
    );

    // contradictions (two roadmaps)
    assert!(md.contains("Contradiction") || md.contains("contradiction") || md.contains("roadmap"));

    // pending review
    assert!(md.contains("open-brief.md"));
    assert!(md.contains("待复核") || md.contains("Pending review"));

    // expected orphans non-problem, not mixed into pending-only framing
    assert!(md.contains("预期孤儿") || md.contains("非问题"));
    assert!(md.contains("lonely-acceptance") || md.contains("verification"));

    // no hand-written operational narrative
    assert!(!md.contains("在途 Worker"));
    assert!(!md.contains("C4 已 checkpoint"));
}

#[test]
fn render_empty_registry_honest() {
    let md = render_status_view(&RenderInput {
        documents: vec![],
        links: vec![],
        audit: keeper_knowledge_indexer::AuditReport::default(),
        tasks: vec![],
        verdicts: vec![],
        include_archived: false,
    });
    assert!(md.contains("0"));
    assert!(
        md.contains("empty")
            || md.contains("No data")
            || md.contains("无数据")
            || md.contains("Nothing")
    );
}

// ---------------------------------------------------------------------------
// K1a — retirement: archive visibility, candidates, negative controls
// ---------------------------------------------------------------------------

#[test]
fn render_default_excludes_archived_include_flag_shows() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();
    write(
        &root.join("docs/handoff/current.md"),
        "> 本文件取代 `docs/handoff/old.md`。\n",
    );
    write(&root.join("docs/handoff/old.md"), "# Old handoff\n");
    write(&root.join("docs/handoff/live.md"), "# Live\n");

    let result = scan_and_register(&ScanOptions {
        repo_root: root.to_path_buf(),
        db_path: root.join("k.db"),
        project_name: "t".into(),
        registered_by: RegisteredBy::Lint,
        source_run_id: None,
        docs_relative: "docs".into(),
        exclude_paths: vec![],
    })
    .unwrap();

    let old = result
        .documents
        .iter()
        .find(|d| d.path.contains("old.md"))
        .expect("old registered");
    let repo = SqliteKnowledgeRepository::open(root.join("k.db")).unwrap();
    repo.archive(&old.document_id).unwrap();

    // Reload after archive.
    let docs_all = repo
        .list_documents(
            &result.project_id,
            keeper_storage_sqlite::ListDocumentsQuery {
                include_archived: true,
            },
        )
        .unwrap();
    assert!(
        docs_all
            .iter()
            .any(|d| d.path.contains("old.md") && d.status == DocumentStatus::Archived)
    );

    let default_md = render_status_view(&RenderInput {
        documents: docs_all.clone(),
        links: result.links.clone(),
        audit: result.report.clone(),
        tasks: vec![],
        verdicts: vec![],
        include_archived: false,
    });
    assert!(
        !default_md.contains("docs/handoff/old.md"),
        "default render must omit archived path: {default_md}"
    );
    assert!(
        default_md.contains("archived excluded by default")
            || default_md.contains("include_archived"),
        "default render must state archived exclusion"
    );

    let with_md = render_status_view(&RenderInput {
        documents: docs_all,
        links: result.links.clone(),
        audit: result.report.clone(),
        tasks: vec![],
        verdicts: vec![],
        include_archived: true,
    });
    assert!(
        with_md.contains("docs/handoff/old.md"),
        "include_archived=true must show archived path"
    );
    assert!(
        with_md.contains("status=`archived`") || with_md.contains("`archived`"),
        "archived status should appear when included"
    );
}

#[test]
fn retirement_candidates_only_explicit_supersession_registered_target() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();
    // Explicit supersession with registered target → candidate.
    write(
        &root.join("docs/handoff/new.md"),
        "> 本文件取代 `docs/handoff/old.md`。\n",
    );
    write(&root.join("docs/handoff/old.md"), "# Old\n");
    // Free-path claim only → no registered target → not a candidate source for others.
    write(
        &root.join("docs/execution/status-report.md"),
        "This report supersedes prior delivery status.\n",
    );
    // Zero-inbound research → never a candidate.
    write(
        &root.join("docs/research/zero-inbound-note.md"),
        "# Research nobody links\n",
    );
    // Manual-looking old date alone → never a candidate.
    write(
        &root.join("docs/research/2020-01-01-ancient.md"),
        "# Looks old\n",
    );

    let result = scan_and_register(&ScanOptions {
        repo_root: root.to_path_buf(),
        db_path: root.join("k.db"),
        project_name: "t".into(),
        registered_by: RegisteredBy::Lint,
        source_run_id: None,
        docs_relative: "docs".into(),
        exclude_paths: vec![],
    })
    .unwrap();

    let candidates = &result.report.retirement.retirement_candidates;
    assert!(
        candidates.iter().any(|p| p.contains("handoff/old.md")),
        "explicit supersession target must be candidate: {candidates:?}"
    );
    // Negative: zero-inbound must not appear.
    assert!(
        !candidates
            .iter()
            .any(|p| p.contains("zero-inbound-note") || p.contains("ancient")),
        "zero-inbound / date-old must not be candidates: {candidates:?}"
    );
    // Free-path claim has no registered target path.
    assert!(
        !candidates
            .iter()
            .any(|p| p.contains("prior delivery") || p.contains("status-report")),
        "free-path claim must not invent candidates: {candidates:?}"
    );
}

#[test]
fn zero_inbound_never_retirement_candidate_negative() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();
    write(
        &root.join("docs/research/lonely-research.md"),
        "# Orphan research\n\nNo inbound references.\n",
    );
    write(
        &root.join("docs/execution/lonely-report.md"),
        "# Orphan report\n",
    );

    let result = scan_and_register(&ScanOptions {
        repo_root: root.to_path_buf(),
        db_path: root.join("k.db"),
        project_name: "t".into(),
        registered_by: RegisteredBy::Lint,
        source_run_id: None,
        docs_relative: "docs".into(),
        exclude_paths: vec![],
    })
    .unwrap();

    // Confirm they are orphans.
    let total_orphans: usize = result
        .report
        .orphans_by_role
        .values()
        .map(|v| v.len())
        .sum();
    assert!(total_orphans >= 2, "fixture must produce orphans");

    assert!(
        result.report.retirement.retirement_candidates.is_empty(),
        "zero-inbound alone must never produce retirement candidates: {:?}",
        result.report.retirement.retirement_candidates
    );
}

#[test]
fn expected_orphans_verification_decision_labeled_not_candidates() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();
    write(
        &root.join("docs/verification/lonely-acceptance.md"),
        "# Acceptance\n\nEvidence only.\n",
    );
    write(
        &root.join("docs/decisions/ADR-0099-orphan.md"),
        "Status: Accepted\n\n# Orphan ADR\n",
    );

    let result = scan_and_register(&ScanOptions {
        repo_root: root.to_path_buf(),
        db_path: root.join("k.db"),
        project_name: "t".into(),
        registered_by: RegisteredBy::Lint,
        source_run_id: None,
        docs_relative: "docs".into(),
        exclude_paths: vec![],
    })
    .unwrap();

    let expected = &result.report.retirement.expected_orphans_non_problem;
    assert!(
        expected.iter().any(|p| p.contains("lonely-acceptance")),
        "verification orphan must be labeled expected: {expected:?}"
    );
    assert!(
        expected.iter().any(|p| p.contains("ADR-0099")),
        "decision orphan must be labeled expected: {expected:?}"
    );

    for p in &result.report.retirement.retirement_candidates {
        assert!(
            !p.contains("lonely-acceptance") && !p.contains("ADR-0099"),
            "expected orphan leaked into candidates: {p}"
        );
    }

    // Report markdown carries the Chinese non-problem label.
    assert!(
        result.report_markdown.contains("预期孤儿") || result.report_markdown.contains("非问题"),
        "audit markdown must label expected orphans"
    );
}

#[test]
fn orphan_brief_goes_to_pending_review_not_retirement() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();
    write(
        &root.join("docs/execution/2026-08-09-unfinished-brief.md"),
        "# Brief without report\n",
    );

    let result = scan_and_register(&ScanOptions {
        repo_root: root.to_path_buf(),
        db_path: root.join("k.db"),
        project_name: "t".into(),
        registered_by: RegisteredBy::Lint,
        source_run_id: None,
        docs_relative: "docs".into(),
        exclude_paths: vec![],
    })
    .unwrap();

    assert!(
        result
            .report
            .retirement
            .briefs_pending_review
            .iter()
            .any(|p| p.contains("unfinished-brief")),
        "orphan brief must be 待复核: {:?}",
        result.report.retirement.briefs_pending_review
    );
    assert!(
        result
            .report
            .unpaired_briefs
            .iter()
            .any(|p| p.contains("unfinished-brief"))
    );
    assert!(
        !result
            .report
            .retirement
            .retirement_candidates
            .iter()
            .any(|p| p.contains("unfinished-brief")),
        "orphan brief must not be retirement candidate"
    );
}

#[test]
fn archive_preserves_non_unknown_on_rescan_and_is_explicit_only() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();
    let path = "docs/handoff/controller-active.md";
    write(&root.join(path), "# Handoff\n\nController-owned active.\n");

    let db = root.join("k.db");
    let opts = ScanOptions {
        repo_root: root.to_path_buf(),
        db_path: db.clone(),
        project_name: "t".into(),
        registered_by: RegisteredBy::Lint,
        source_run_id: None,
        docs_relative: "docs".into(),
        exclude_paths: vec![],
    };
    let first = scan_and_register(&opts).unwrap();
    let doc = first.documents.iter().find(|d| d.path == path).unwrap();

    // Controller judgment: active (non-unknown).
    let repo = SqliteKnowledgeRepository::open(&db).unwrap();
    repo.update(UpdateDocument {
        document_id: doc.document_id.clone(),
        role: None,
        status: Some(DocumentStatus::Active),
        content_digest: None,
        registered_by: Some(RegisteredBy::Controller),
        source_run_id: None,
    })
    .unwrap();

    // Rescan must not clobber Active (K0b + K1a).
    write(
        &root.join(path),
        "# Handoff\n\nUpdated body; still controller-active.\n",
    );
    let second = scan_and_register(&opts).unwrap();
    let again = second.documents.iter().find(|d| d.path == path).unwrap();
    assert_eq!(again.status, DocumentStatus::Active);

    // Explicit archive is the only path to archived; scanner did not auto-archive.
    assert!(
        !second
            .documents
            .iter()
            .any(|d| d.status == DocumentStatus::Archived)
    );
    let archived = repo.archive(&again.document_id).unwrap();
    assert_eq!(archived.status, DocumentStatus::Archived);

    // Rescan preserves archived (non-unknown).
    write(
        &root.join(path),
        "# Handoff\n\nDigest change after archive.\n",
    );
    let third = scan_and_register(&opts).unwrap();
    let after = third
        .documents
        .iter()
        .find(|d| d.path == path)
        .expect("archived still in full inventory");
    assert_eq!(
        after.status,
        DocumentStatus::Archived,
        "rescan must not overwrite archived"
    );
}

#[test]
fn indexer_has_no_delete_or_purge_api() {
    let lib = include_str!("../src/lib.rs");
    let audit = include_str!("../src/audit.rs");
    let scan = include_str!("../src/scan.rs");
    for (name, src) in [("lib", lib), ("audit", audit), ("scan", scan)] {
        for needle in ["fn delete", "fn purge", "delete_document", "purge_document"] {
            assert!(
                !src.contains(needle),
                "knowledge-indexer {name} must not expose {needle}"
            );
        }
    }
    // Scanner must not call archive either (candidates only).
    assert!(!scan.contains(".archive("), "scanner must not auto-archive");
}
