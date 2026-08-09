//! K0a/K1a knowledge registry: migration, 1→N supersedes, archive, defaults,
//! SSoT path, isolation. No delete/purge API.

use keeper_project::CreateProject;
use keeper_storage_sqlite::{
    AddDocumentLink, DocumentLinkRel, DocumentRole, DocumentStatus, KnowledgeError,
    ListDocumentsQuery, RegisterDocument, RegisteredBy, SqliteKnowledgeRepository,
    SqliteProjectRepository, UpdateDocument,
};
use rusqlite::Connection;

fn digest(seed: u8) -> String {
    // Canonical sha256:<64 lowercase hex> accepted by the repository.
    format!("sha256:{seed:02x}{}", "0".repeat(62))
}

fn open_knowledge(temp: &tempfile::TempDir) -> SqliteKnowledgeRepository {
    SqliteKnowledgeRepository::open(temp.path().join("keeper.db")).expect("open knowledge")
}

fn seed_project(temp: &tempfile::TempDir, name: &str) -> String {
    let projects = SqliteProjectRepository::open(temp.path().join("keeper.db")).unwrap();
    let root = temp.path().join(name);
    std::fs::create_dir_all(&root).unwrap();
    let project = projects
        .create(CreateProject {
            name: name.into(),
            root_path: root.to_string_lossy().into_owned(),
        })
        .unwrap();
    project.id.0.to_string()
}

fn register(
    repo: &SqliteKnowledgeRepository,
    project_id: &str,
    path: &str,
    role: DocumentRole,
    status: Option<DocumentStatus>,
) -> keeper_storage_sqlite::KnowledgeDocument {
    repo.register(RegisterDocument {
        project_id: project_id.to_owned(),
        path: path.to_owned(),
        role,
        status,
        content_digest: digest(1),
        registered_by: RegisteredBy::Controller,
        source_run_id: None,
    })
    .expect("register")
}

// ---------------------------------------------------------------------------
// 1. Migration idempotent
// ---------------------------------------------------------------------------

#[test]
fn migration_0010_is_idempotent_and_creates_knowledge_tables() {
    let temp = tempfile::tempdir().unwrap();
    let db = temp.path().join("keeper.db");

    let first = SqliteKnowledgeRepository::open(&db).expect("first open");
    drop(first);
    let second = SqliteKnowledgeRepository::open(&db).expect("second open");
    drop(second);

    let connection = Connection::open(&db).unwrap();
    for table in ["knowledge_documents", "knowledge_document_links"] {
        let count: i64 = connection
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master
                 WHERE type = 'table' AND name = ?1",
                [table],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(count, 1, "table {table} must exist exactly once");
    }

    let mut statement = connection
        .prepare("PRAGMA table_info(knowledge_documents)")
        .unwrap();
    let columns: Vec<String> = statement
        .query_map([], |row| row.get::<_, String>(1))
        .unwrap()
        .map(|c| c.unwrap())
        .collect();
    for required in [
        "document_id",
        "project_id",
        "path",
        "role",
        "status",
        "content_digest",
        "updated_at",
        "registered_by",
        "source_run_id",
    ] {
        assert!(
            columns.iter().any(|c| c == required),
            "missing column {required}"
        );
    }

    // Default status is unknown at SQL level.
    let status_default: Option<String> = connection
        .query_row(
            "SELECT dflt_value FROM pragma_table_info('knowledge_documents')
             WHERE name = 'status'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    let default_text = status_default.unwrap_or_default();
    assert!(
        default_text.contains("unknown"),
        "status DEFAULT must be unknown, got {default_text}"
    );
    assert!(
        !default_text.contains("active"),
        "status must not default to active"
    );

    // Illegal role rejected by CHECK.
    let err = connection.execute(
        "INSERT INTO knowledge_documents (
            document_id, project_id, path, role, status, content_digest,
            updated_at, registered_by, source_run_id
         ) VALUES ('d1', 'p1', 'a.md', 'not-a-role', 'unknown', 'sha256:x', 't', 'controller', NULL)",
        [],
    );
    assert!(err.is_err(), "illegal role must fail CHECK");

    // Illegal status rejected by CHECK.
    let err = connection.execute(
        "INSERT INTO knowledge_documents (
            document_id, project_id, path, role, status, content_digest,
            updated_at, registered_by, source_run_id
         ) VALUES ('d2', 'p1', 'b.md', 'brief', 'live', 'sha256:x', 't', 'controller', NULL)",
        [],
    );
    assert!(err.is_err(), "illegal status must fail CHECK");
}

// ---------------------------------------------------------------------------
// 2. 1→2 supersession (real handoff case)
// ---------------------------------------------------------------------------

#[test]
fn one_to_two_supersedes_handoff_case() {
    // Real corpus case (docs inventory audit §4.1):
    // docs/handoff/2026-08-08-pillow-controller-handoff.md supersedes BOTH
    //   - docs/handoff/2026-08-07-pillow-controller-handoff.md
    //   - docs/handoff/2026-08-08-round5-tech-debt-status.md
    let temp = tempfile::tempdir().unwrap();
    let project_id = seed_project(&temp, "proj-a");
    let repo = open_knowledge(&temp);

    let current = register(
        &repo,
        &project_id,
        "docs/handoff/2026-08-08-pillow-controller-handoff.md",
        DocumentRole::Handoff,
        Some(DocumentStatus::Active),
    );
    let prior = register(
        &repo,
        &project_id,
        "docs/handoff/2026-08-07-pillow-controller-handoff.md",
        DocumentRole::Handoff,
        Some(DocumentStatus::Superseded),
    );
    let round5 = register(
        &repo,
        &project_id,
        "docs/handoff/2026-08-08-round5-tech-debt-status.md",
        DocumentRole::Handoff,
        Some(DocumentStatus::Superseded),
    );

    let link1 = repo
        .add_link(AddDocumentLink {
            from_document_id: current.document_id.clone(),
            to_document_id: Some(prior.document_id.clone()),
            to_path: Some(prior.path.clone()),
            rel: DocumentLinkRel::Supersedes,
        })
        .expect("supersedes prior handoff");
    let link2 = repo
        .add_link(AddDocumentLink {
            from_document_id: current.document_id.clone(),
            to_document_id: Some(round5.document_id.clone()),
            to_path: Some(round5.path.clone()),
            rel: DocumentLinkRel::Supersedes,
        })
        .expect("supersedes round5");

    assert_ne!(link1.link_id, link2.link_id);

    let links = repo
        .list_links_from_with_rel(&current.document_id, DocumentLinkRel::Supersedes)
        .unwrap();
    assert_eq!(
        links.len(),
        2,
        "1→2 supersession must yield two link rows from the current handoff"
    );

    let targets: Vec<Option<String>> = links.iter().map(|l| l.to_document_id.clone()).collect();
    assert!(targets.contains(&Some(prior.document_id.clone())));
    assert!(targets.contains(&Some(round5.document_id.clone())));

    let paths: Vec<Option<String>> = links.iter().map(|l| l.to_path.clone()).collect();
    assert!(paths.contains(&Some(
        "docs/handoff/2026-08-07-pillow-controller-handoff.md".to_owned()
    )));
    assert!(paths.contains(&Some(
        "docs/handoff/2026-08-08-round5-tech-debt-status.md".to_owned()
    )));
}

// ---------------------------------------------------------------------------
// 3. Both endpoints empty rejected (Rust + SQL CHECK)
// ---------------------------------------------------------------------------

#[test]
fn link_rejects_both_endpoints_empty() {
    let temp = tempfile::tempdir().unwrap();
    let project_id = seed_project(&temp, "proj-a");
    let repo = open_knowledge(&temp);
    let doc = register(
        &repo,
        &project_id,
        "docs/handoff/current.md",
        DocumentRole::Handoff,
        None,
    );

    let err = repo
        .add_link(AddDocumentLink {
            from_document_id: doc.document_id.clone(),
            to_document_id: None,
            to_path: None,
            rel: DocumentLinkRel::Supersedes,
        })
        .unwrap_err();
    assert!(matches!(err, KnowledgeError::LinkEndpointRequired));
    assert_eq!(err.code(), "knowledge_link_endpoint_required");

    // Empty strings normalize to None and are also rejected.
    let err = repo
        .add_link(AddDocumentLink {
            from_document_id: doc.document_id.clone(),
            to_document_id: Some(String::new()),
            to_path: Some("   ".to_owned()),
            rel: DocumentLinkRel::Related,
        })
        .unwrap_err();
    assert!(matches!(err, KnowledgeError::LinkEndpointRequired));

    // SQL CHECK also rejects a direct insert with both NULL.
    let connection = Connection::open(temp.path().join("keeper.db")).unwrap();
    connection
        .execute_batch("PRAGMA foreign_keys = ON;")
        .unwrap();
    let err = connection.execute(
        "INSERT INTO knowledge_document_links (
            link_id, from_document_id, to_document_id, to_path, rel
         ) VALUES ('link-empty', ?1, NULL, NULL, 'supersedes')",
        [&doc.document_id],
    );
    assert!(
        err.is_err(),
        "SQL CHECK must reject both-null link endpoints"
    );
}

// ---------------------------------------------------------------------------
// 4. Pathless supersession claim (to_path free text only)
// ---------------------------------------------------------------------------

#[test]
fn pathless_supersession_claim_registers_with_to_path_only() {
    // Audit §4.1: execution/2026-08-01-wave-c3e-approval-ui-report.md
    // supersedes “prior delivery status” (no target path).
    let temp = tempfile::tempdir().unwrap();
    let project_id = seed_project(&temp, "proj-a");
    let repo = open_knowledge(&temp);
    let report = register(
        &repo,
        &project_id,
        "docs/execution/2026-08-01-wave-c3e-approval-ui-report.md",
        DocumentRole::Report,
        Some(DocumentStatus::Evidence),
    );

    let link = repo
        .add_link(AddDocumentLink {
            from_document_id: report.document_id.clone(),
            to_document_id: None,
            to_path: Some("prior delivery status".to_owned()),
            rel: DocumentLinkRel::Supersedes,
        })
        .expect("pathless claim");

    assert!(link.to_document_id.is_none());
    assert_eq!(link.to_path.as_deref(), Some("prior delivery status"));

    let listed = repo.list_links_from(&report.document_id).unwrap();
    assert_eq!(listed.len(), 1);
    assert_eq!(listed[0].to_path.as_deref(), Some("prior delivery status"));
    assert!(listed[0].to_document_id.is_none());
}

// ---------------------------------------------------------------------------
// 5. Illegal role / status rejected at repository API
// ---------------------------------------------------------------------------

#[test]
fn illegal_role_and_status_rejected_by_parse() {
    // Repository takes typed enums; illegal strings are rejected at the
    // boundary (parse) and by SQL CHECK (covered in migration test).
    assert!(DocumentRole::parse("not-a-role").is_err());
    assert!(DocumentRole::parse("Brief").is_err()); // case-sensitive
    assert!(DocumentStatus::parse("live").is_err());
    assert!(DocumentStatus::parse("Active").is_err());

    // plan is a first-class role (not overloaded onto spec).
    assert_eq!(DocumentRole::parse("plan").unwrap(), DocumentRole::Plan);
    assert_eq!(DocumentRole::parse("spec").unwrap(), DocumentRole::Spec);
    assert_ne!(DocumentRole::Plan.as_str(), DocumentRole::Spec.as_str());
}

// ---------------------------------------------------------------------------
// 6. Unclassified default status is unknown, not active
// ---------------------------------------------------------------------------

#[test]
fn unclassified_document_status_is_unknown_not_active() {
    let temp = tempfile::tempdir().unwrap();
    let project_id = seed_project(&temp, "proj-a");
    let repo = open_knowledge(&temp);

    let doc = repo
        .register(RegisterDocument {
            project_id: project_id.clone(),
            path: "docs/research/unclassified.md".to_owned(),
            role: DocumentRole::Research,
            status: None, // explicit: omit status
            content_digest: digest(2),
            registered_by: RegisteredBy::Worker,
            source_run_id: Some("run-xyz".to_owned()),
        })
        .unwrap();

    assert_eq!(
        doc.status,
        DocumentStatus::Unknown,
        "unclassified must be unknown"
    );
    assert_ne!(doc.status, DocumentStatus::Active);

    let loaded = repo.get(&doc.document_id).unwrap().expect("loaded");
    assert_eq!(loaded.status, DocumentStatus::Unknown);
    assert_eq!(loaded.status.as_str(), "unknown");

    // Explicit active is still allowed when the caller sets it.
    let active = register(
        &repo,
        &project_id,
        "docs/handoff/active.md",
        DocumentRole::Handoff,
        Some(DocumentStatus::Active),
    );
    assert_eq!(active.status, DocumentStatus::Active);
}

// ---------------------------------------------------------------------------
// 7. Path SSoT: superscript device COM¹ rejected
// ---------------------------------------------------------------------------

#[test]
fn path_validation_uses_ssot_rejects_superscript_com() {
    let temp = tempfile::tempdir().unwrap();
    let project_id = seed_project(&temp, "proj-a");
    let repo = open_knowledge(&temp);

    // COM¹ (U+00B9) — accepted only by broken local copies of the device
    // rules; the converged SSoT rejects it.
    let superscript_com = "COM\u{00b9}";
    let err = repo
        .register(RegisterDocument {
            project_id: project_id.clone(),
            path: superscript_com.to_owned(),
            role: DocumentRole::Other,
            status: None,
            content_digest: digest(3),
            registered_by: RegisteredBy::Lint,
            source_run_id: None,
        })
        .unwrap_err();
    assert!(
        matches!(err, KnowledgeError::InvalidPath { .. }),
        "COM¹ must be rejected via SSoT, got {err:?}"
    );
    assert_eq!(err.code(), "knowledge_invalid_path");

    // Also reject nested component form.
    let err = repo
        .register(RegisterDocument {
            project_id,
            path: format!("docs/{superscript_com}/note.md"),
            role: DocumentRole::Other,
            status: None,
            content_digest: digest(3),
            registered_by: RegisteredBy::Lint,
            source_run_id: None,
        })
        .unwrap_err();
    assert!(matches!(err, KnowledgeError::InvalidPath { .. }));

    // Valid portable path still accepted (proves rejection is specific).
    let ok = open_knowledge(&temp);
    // Re-open already migrated DB; seed project still present.
    let project_id = {
        // list projects via a known re-open of knowledge is enough; re-seed
        // would collide roots — use the existing project by re-querying path.
        // We stored project_id in outer scope... re-get from earlier registration
        // attempt that failed; re-seed with a new name.
        seed_project(&temp, "proj-path-ok")
    };
    let accepted = register(
        &ok,
        &project_id,
        "docs/handoff/2026-08-08-pillow-controller-handoff.md",
        DocumentRole::Handoff,
        None,
    );
    assert_eq!(
        accepted.path,
        "docs/handoff/2026-08-08-pillow-controller-handoff.md"
    );
}

// ---------------------------------------------------------------------------
// 8. Project isolation
// ---------------------------------------------------------------------------

#[test]
fn documents_are_isolated_by_project() {
    let temp = tempfile::tempdir().unwrap();
    let project_a = seed_project(&temp, "proj-a");
    let project_b = seed_project(&temp, "proj-b");
    let repo = open_knowledge(&temp);

    let path = "docs/handoff/shared-name.md";
    let a = register(
        &repo,
        &project_a,
        path,
        DocumentRole::Handoff,
        Some(DocumentStatus::Active),
    );
    let b = register(
        &repo,
        &project_b,
        path,
        DocumentRole::Handoff,
        Some(DocumentStatus::Active),
    );
    assert_ne!(a.document_id, b.document_id);

    let list_a = repo.list_by_project(&project_a).unwrap();
    assert_eq!(list_a.len(), 1);
    assert_eq!(list_a[0].document_id, a.document_id);
    assert_eq!(list_a[0].project_id, project_a);

    let list_b = repo.list_by_project(&project_b).unwrap();
    assert_eq!(list_b.len(), 1);
    assert_eq!(list_b[0].document_id, b.document_id);
    assert_eq!(list_b[0].project_id, project_b);

    // get_by_path is project-scoped.
    let found_a = repo.get_by_path(&project_a, path).unwrap().unwrap();
    assert_eq!(found_a.document_id, a.document_id);
    let found_b = repo.get_by_path(&project_b, path).unwrap().unwrap();
    assert_eq!(found_b.document_id, b.document_id);

    // Project A cannot see B's document by id through list (id still gettable
    // if known, but project lists never cross).
    assert!(!list_a.iter().any(|d| d.document_id == b.document_id));
    assert!(!list_b.iter().any(|d| d.document_id == a.document_id));
}

// ---------------------------------------------------------------------------
// 9. Restart recovery
// ---------------------------------------------------------------------------

#[test]
fn restart_recovers_documents_and_links() {
    let temp = tempfile::tempdir().unwrap();
    let db = temp.path().join("keeper.db");
    let project_id = {
        let projects = SqliteProjectRepository::open(&db).unwrap();
        let root = temp.path().join("proj-restart");
        std::fs::create_dir_all(&root).unwrap();
        projects
            .create(CreateProject {
                name: "proj-restart".into(),
                root_path: root.to_string_lossy().into_owned(),
            })
            .unwrap()
            .id
            .0
            .to_string()
    };

    let (current_id, prior_id, link_count) = {
        let repo = SqliteKnowledgeRepository::open(&db).unwrap();
        let current = register(
            &repo,
            &project_id,
            "docs/handoff/2026-08-08-pillow-controller-handoff.md",
            DocumentRole::Handoff,
            Some(DocumentStatus::Active),
        );
        let prior = register(
            &repo,
            &project_id,
            "docs/handoff/2026-08-07-pillow-controller-handoff.md",
            DocumentRole::Handoff,
            Some(DocumentStatus::Superseded),
        );
        let round5 = register(
            &repo,
            &project_id,
            "docs/handoff/2026-08-08-round5-tech-debt-status.md",
            DocumentRole::Handoff,
            Some(DocumentStatus::Superseded),
        );
        repo.add_link(AddDocumentLink {
            from_document_id: current.document_id.clone(),
            to_document_id: Some(prior.document_id.clone()),
            to_path: Some(prior.path.clone()),
            rel: DocumentLinkRel::Supersedes,
        })
        .unwrap();
        repo.add_link(AddDocumentLink {
            from_document_id: current.document_id.clone(),
            to_document_id: Some(round5.document_id.clone()),
            to_path: Some(round5.path.clone()),
            rel: DocumentLinkRel::Supersedes,
        })
        .unwrap();
        // Explicit status update survives restart.
        repo.update(UpdateDocument {
            document_id: prior.document_id.clone(),
            role: None,
            status: Some(DocumentStatus::Superseded),
            content_digest: None,
            registered_by: None,
            source_run_id: None,
        })
        .unwrap();
        let links = repo.list_links_from(&current.document_id).unwrap();
        (current.document_id, prior.document_id, links.len())
    };
    // Handle dropped: simulate process exit.

    let repo = SqliteKnowledgeRepository::open(&db).expect("reopen after restart");
    let list = repo.list_by_project(&project_id).unwrap();
    assert_eq!(list.len(), 3);

    let current = repo
        .get(&current_id)
        .unwrap()
        .expect("current after restart");
    assert_eq!(current.status, DocumentStatus::Active);
    assert_eq!(
        current.path,
        "docs/handoff/2026-08-08-pillow-controller-handoff.md"
    );

    let prior = repo.get(&prior_id).unwrap().expect("prior after restart");
    assert_eq!(prior.status, DocumentStatus::Superseded);

    let links = repo
        .list_links_from_with_rel(&current_id, DocumentLinkRel::Supersedes)
        .unwrap();
    assert_eq!(links.len(), link_count);
    assert_eq!(links.len(), 2);
}

// ---------------------------------------------------------------------------
// Bonus: no automatic status migration on link write
// ---------------------------------------------------------------------------

#[test]
fn adding_supersedes_link_does_not_auto_change_status() {
    let temp = tempfile::tempdir().unwrap();
    let project_id = seed_project(&temp, "proj-a");
    let repo = open_knowledge(&temp);

    let current = register(
        &repo,
        &project_id,
        "docs/handoff/current.md",
        DocumentRole::Handoff,
        Some(DocumentStatus::Active),
    );
    let old = register(
        &repo,
        &project_id,
        "docs/handoff/old.md",
        DocumentRole::Handoff,
        Some(DocumentStatus::Active), // intentionally still active
    );

    repo.add_link(AddDocumentLink {
        from_document_id: current.document_id.clone(),
        to_document_id: Some(old.document_id.clone()),
        to_path: Some(old.path.clone()),
        rel: DocumentLinkRel::Supersedes,
    })
    .unwrap();

    let old_after = repo.get(&old.document_id).unwrap().unwrap();
    assert_eq!(
        old_after.status,
        DocumentStatus::Active,
        "K0a must not auto-migrate status when a supersedes link is written"
    );
}

// ---------------------------------------------------------------------------
// K1a: archive / unarchive round-trip
// ---------------------------------------------------------------------------

#[test]
fn archive_unarchive_round_trip() {
    let temp = tempfile::tempdir().unwrap();
    let project_id = seed_project(&temp, "proj-a");
    let repo = open_knowledge(&temp);

    let doc = register(
        &repo,
        &project_id,
        "docs/handoff/retire-me.md",
        DocumentRole::Handoff,
        Some(DocumentStatus::Active),
    );
    assert_eq!(doc.status, DocumentStatus::Active);

    let archived = repo.archive(&doc.document_id).expect("archive");
    assert_eq!(archived.status, DocumentStatus::Archived);
    let loaded = repo.get(&doc.document_id).unwrap().unwrap();
    assert_eq!(loaded.status, DocumentStatus::Archived);

    // Idempotent archive.
    let again = repo.archive(&doc.document_id).unwrap();
    assert_eq!(again.status, DocumentStatus::Archived);

    let restored = repo.unarchive(&doc.document_id).expect("unarchive");
    assert_eq!(
        restored.status,
        DocumentStatus::Unknown,
        "unarchive restores to unknown (explicit follow-up update for stronger status)"
    );
    let loaded = repo.get(&doc.document_id).unwrap().unwrap();
    assert_eq!(loaded.status, DocumentStatus::Unknown);

    // unarchive on non-archived is a no-op (does not invent status).
    let still = repo.unarchive(&doc.document_id).unwrap();
    assert_eq!(still.status, DocumentStatus::Unknown);
}

// ---------------------------------------------------------------------------
// K1a: default list excludes archived; include_archived makes them visible
// ---------------------------------------------------------------------------

#[test]
fn default_list_excludes_archived_include_flag_shows_them() {
    let temp = tempfile::tempdir().unwrap();
    let project_id = seed_project(&temp, "proj-a");
    let repo = open_knowledge(&temp);

    let live = register(
        &repo,
        &project_id,
        "docs/handoff/live.md",
        DocumentRole::Handoff,
        Some(DocumentStatus::Active),
    );
    let old = register(
        &repo,
        &project_id,
        "docs/handoff/old.md",
        DocumentRole::Handoff,
        Some(DocumentStatus::Superseded),
    );
    repo.archive(&old.document_id).unwrap();

    // Default path — cannot forget to exclude.
    let default_list = repo.list_by_project(&project_id).unwrap();
    assert_eq!(default_list.len(), 1);
    assert_eq!(default_list[0].document_id, live.document_id);
    assert!(
        !default_list
            .iter()
            .any(|d| d.status == DocumentStatus::Archived)
    );

    let explicit_default = repo
        .list_documents(
            &project_id,
            ListDocumentsQuery {
                include_archived: false,
            },
        )
        .unwrap();
    assert_eq!(explicit_default.len(), 1);

    let with_archived = repo
        .list_documents(
            &project_id,
            ListDocumentsQuery {
                include_archived: true,
            },
        )
        .unwrap();
    assert_eq!(with_archived.len(), 2);
    assert!(
        with_archived
            .iter()
            .any(|d| d.document_id == old.document_id && d.status == DocumentStatus::Archived)
    );

    // Point lookup still returns archived (explicit id/path is not a default search).
    let by_id = repo.get(&old.document_id).unwrap().unwrap();
    assert_eq!(by_id.status, DocumentStatus::Archived);
    let by_path = repo
        .get_by_path(&project_id, "docs/handoff/old.md")
        .unwrap()
        .unwrap();
    assert_eq!(by_path.status, DocumentStatus::Archived);
}

// ---------------------------------------------------------------------------
// K1a: archive does not clobber non-unknown on *other* docs; rescan preserve
// is covered in indexer. Here: archive is explicit status write only on target.
// ---------------------------------------------------------------------------

#[test]
fn archive_does_not_overwrite_other_docs_non_unknown_status() {
    let temp = tempfile::tempdir().unwrap();
    let project_id = seed_project(&temp, "proj-a");
    let repo = open_knowledge(&temp);

    let active = register(
        &repo,
        &project_id,
        "docs/handoff/active.md",
        DocumentRole::Handoff,
        Some(DocumentStatus::Active),
    );
    let evidence = register(
        &repo,
        &project_id,
        "docs/execution/report.md",
        DocumentRole::Report,
        Some(DocumentStatus::Evidence),
    );
    let target = register(
        &repo,
        &project_id,
        "docs/handoff/old.md",
        DocumentRole::Handoff,
        Some(DocumentStatus::Superseded),
    );

    repo.archive(&target.document_id).unwrap();

    assert_eq!(
        repo.get(&active.document_id).unwrap().unwrap().status,
        DocumentStatus::Active
    );
    assert_eq!(
        repo.get(&evidence.document_id).unwrap().unwrap().status,
        DocumentStatus::Evidence
    );
    assert_eq!(
        repo.get(&target.document_id).unwrap().unwrap().status,
        DocumentStatus::Archived
    );

    // Scanner-facing constraint: non-unknown statuses (incl. archived) are only
    // changed by explicit update/archive/unarchive — not by register defaults.
    let listed = repo
        .list_documents(
            &project_id,
            ListDocumentsQuery {
                include_archived: true,
            },
        )
        .unwrap();
    assert_eq!(listed.len(), 3);
}

// ---------------------------------------------------------------------------
// K1a: no delete / purge entry on the knowledge API surface
// ---------------------------------------------------------------------------

#[test]
fn knowledge_api_has_no_delete_or_purge_entry() {
    // Source-level assertion: durable knowledge module must not expose delete
    // or purge for documents (ADR-0004 §5 — true delete needs approval + receipt).
    let src = include_str!("../src/knowledge.rs");
    for needle in [
        "fn delete",
        "fn purge",
        "fn remove_document",
        "DELETE FROM knowledge_documents",
        "delete_document",
        "purge_document",
    ] {
        assert!(
            !src.contains(needle),
            "knowledge API must not expose {needle}"
        );
    }
    // Positive: archive/unarchive exist (mark + archive only).
    assert!(src.contains("pub fn archive"));
    assert!(src.contains("pub fn unarchive"));
    assert!(src.contains("include_archived"));
}
