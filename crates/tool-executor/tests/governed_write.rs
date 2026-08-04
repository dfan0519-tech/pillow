//! Governed host-side write executor tests (C3-C).
//!
//! Covers create/modify apply, path denial, base drift, atomic failure cleanup,
//! cancellation, and concurrent claim behavior. Proposed/base content must never
//! appear in SQLite.

use std::fs;
use std::sync::{
    Arc, Barrier,
    atomic::{AtomicBool, Ordering},
};
use std::thread;
use std::time::Duration;

use chrono::Utc;
use keeper_agent_protocol::{RunId as ProtocolRunId, WRITE_CONTENT_BYTE_CAP, WriteMode};
use keeper_policy::{WriteDecision, WriteProposalState};
use keeper_project::CreateProject;
use keeper_session::{ConversationRepository, NewConversation, NewRun};
use keeper_storage_sqlite::{
    SqliteConversationRepository, SqliteProjectRepository, SqliteWriteProposalRepository,
    WriteApplyErrorCode, WriteApplyReceipt, WriteApplyReconcileObservation,
    WriteProposalReserveInput, WriteReservationOutcome,
};
use keeper_tool_executor::{
    CancellationSignal, GovernedWriteApplyOutcome, GovernedWriteToolGateway, NeverCancelled,
    ProjectCapability, SealedWriteContents, content_sha256_digest,
};
use uuid::Uuid;

const SECRET_PROPOSED: &str = "SECRET_PROPOSED_C3C_NEVER_LOG_xyz789";
const SECRET_BASE: &str = "SECRET_BASE_C3C_NEVER_LOG_abc123";
const SECRET_DIFF: &str = "SECRET_DIFF_IN_PREPARED_C3C_never_debug";

struct Fixture {
    db: std::path::PathBuf,
    root: std::path::PathBuf,
    project_id: String,
    conversation_id: String,
    run_id: ProtocolRunId,
}

fn fixture(temp: &tempfile::TempDir) -> Fixture {
    let db = temp.path().join("pillow.db");
    let root = temp.path().join("project");
    fs::create_dir(&root).unwrap();
    let projects = SqliteProjectRepository::open(&db).unwrap();
    let project = projects
        .create(CreateProject {
            name: "WriteProject".into(),
            root_path: root.to_string_lossy().into_owned(),
        })
        .unwrap();
    let sessions = SqliteConversationRepository::open(&db).unwrap();
    let conversation = sessions
        .create_conversation(NewConversation {
            project_id: project.id,
            title: Some("writes".into()),
        })
        .unwrap();
    let run = sessions
        .create_run(NewRun {
            conversation_id: conversation.id,
            user_message: "write".into(),
        })
        .unwrap();
    Fixture {
        db,
        root,
        project_id: project.id.0.to_string(),
        conversation_id: conversation.id.0.to_string(),
        run_id: ProtocolRunId(run.id.0),
    }
}

fn digest_of(content: &str) -> String {
    content_sha256_digest(content)
}

fn reserve_create(fx: &Fixture, path: &str, proposed: &str) -> WriteProposalReserveInput {
    WriteProposalReserveInput {
        proposal_id: Uuid::new_v4().to_string(),
        reservation_key: format!(
            "wp:{}:{}:{}:{}",
            fx.project_id,
            fx.conversation_id,
            fx.run_id,
            Uuid::new_v4()
        ),
        proposal_fingerprint: digest_of(&format!("fp:{path}:{proposed}")),
        project_id: fx.project_id.clone(),
        conversation_id: fx.conversation_id.clone(),
        run_id: fx.run_id,
        call_id: format!("call-{}", Uuid::new_v4()),
        path: path.into(),
        mode: WriteMode::Create,
        proposed_digest: digest_of(proposed),
        proposed_bytes: proposed.len() as u64,
        base_digest: None,
        base_bytes: 0,
    }
}

fn reserve_modify(
    fx: &Fixture,
    path: &str,
    proposed: &str,
    base: &str,
) -> WriteProposalReserveInput {
    WriteProposalReserveInput {
        proposal_id: Uuid::new_v4().to_string(),
        reservation_key: format!(
            "wp:{}:{}:{}:{}",
            fx.project_id,
            fx.conversation_id,
            fx.run_id,
            Uuid::new_v4()
        ),
        proposal_fingerprint: digest_of(&format!("fp:{path}:{proposed}:{base}")),
        project_id: fx.project_id.clone(),
        conversation_id: fx.conversation_id.clone(),
        run_id: fx.run_id,
        call_id: format!("call-{}", Uuid::new_v4()),
        path: path.into(),
        mode: WriteMode::Modify,
        proposed_digest: digest_of(proposed),
        proposed_bytes: proposed.len() as u64,
        base_digest: Some(digest_of(base)),
        base_bytes: base.len() as u64,
    }
}

fn approve(store: &SqliteWriteProposalRepository, fx: &Fixture, input: &WriteProposalReserveInput) {
    let grant = digest_of(&format!("grant:{}", input.proposal_id));
    store
        .decide(
            &input.proposal_id,
            &fx.conversation_id,
            WriteDecision::ApproveOnce,
            &grant,
            &input.proposal_fingerprint,
            Utc::now(),
        )
        .unwrap();
}

fn reserve_and_approve_create(
    store: &SqliteWriteProposalRepository,
    fx: &Fixture,
    path: &str,
    proposed: &str,
) -> WriteProposalReserveInput {
    let input = reserve_create(fx, path, proposed);
    let outcome = store.reserve(&input).unwrap();
    assert!(matches!(outcome, WriteReservationOutcome::Reserved(_)));
    approve(store, fx, &input);
    input
}

fn reserve_and_approve_modify(
    store: &SqliteWriteProposalRepository,
    fx: &Fixture,
    path: &str,
    proposed: &str,
    base: &str,
) -> WriteProposalReserveInput {
    let input = reserve_modify(fx, path, proposed, base);
    let outcome = store.reserve(&input).unwrap();
    assert!(matches!(outcome, WriteReservationOutcome::Reserved(_)));
    approve(store, fx, &input);
    input
}

fn sealed(proposed: &str, base: Option<&str>) -> SealedWriteContents {
    SealedWriteContents::new(proposed.to_owned(), base.map(str::to_owned))
}

fn assert_failure_code(outcome: GovernedWriteApplyOutcome, code: WriteApplyErrorCode) {
    match outcome {
        GovernedWriteApplyOutcome::Failed(row) => {
            assert_eq!(row.state, WriteProposalState::Failed);
            let receipt: WriteApplyReceipt =
                serde_json::from_str(row.result_json.as_deref().unwrap()).unwrap();
            match receipt {
                WriteApplyReceipt::Failure {
                    error_code,
                    recovered,
                } => {
                    assert_eq!(error_code, code);
                    assert!(!recovered);
                }
                other => panic!("expected failure receipt, got {other:?}"),
            }
        }
        other => panic!("expected Failed outcome, got {other:?}"),
    }
}

#[test]
fn create_applies_atomically_and_persists_success_receipt() {
    let temp = tempfile::tempdir().unwrap();
    let fx = fixture(&temp);
    let gateway = GovernedWriteToolGateway::open(&fx.db).unwrap();
    let capability = ProjectCapability::new(&fx.project_id, &fx.root).unwrap();
    let store = SqliteWriteProposalRepository::open(&fx.db).unwrap();

    let proposed = "export const answer = 42;\n";
    let input = reserve_and_approve_create(&store, &fx, "src/answer.ts", proposed);
    fs::create_dir(fx.root.join("src")).unwrap();

    let prepared = gateway
        .prepare(
            store.get(&input.proposal_id).unwrap().unwrap(),
            sealed(proposed, None),
        )
        .unwrap();
    assert!(!prepared.diff.unified.is_empty());
    assert_eq!(prepared.proposed_content, proposed);
    assert!(prepared.base_content.is_none());

    let outcome = gateway
        .apply(
            &capability,
            &input.proposal_id,
            &sealed(proposed, None),
            &NeverCancelled,
        )
        .unwrap();
    let GovernedWriteApplyOutcome::Succeeded(row) = outcome else {
        panic!("create must succeed");
    };
    assert_eq!(row.state, WriteProposalState::Succeeded);
    let on_disk = fs::read_to_string(fx.root.join("src/answer.ts")).unwrap();
    assert_eq!(on_disk, proposed);
    let receipt: WriteApplyReceipt =
        serde_json::from_str(row.result_json.as_deref().unwrap()).unwrap();
    match receipt {
        WriteApplyReceipt::Success {
            path,
            mode,
            bytes_written,
            content_digest,
            recovered,
        } => {
            assert_eq!(path, "src/answer.ts");
            assert_eq!(mode, WriteMode::Create);
            assert_eq!(bytes_written, proposed.len() as u64);
            assert_eq!(content_digest, digest_of(proposed));
            assert!(!recovered);
        }
        other => panic!("unexpected receipt {other:?}"),
    }
    // No temp residue.
    let leftovers: Vec<_> = fs::read_dir(fx.root.join("src"))
        .unwrap()
        .filter_map(|e| e.ok())
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .filter(|n| n.contains("pillow-tmp") || n.starts_with('.'))
        .collect();
    assert!(leftovers.is_empty(), "temp leftovers: {leftovers:?}");
}

#[test]
fn modify_applies_when_base_digest_matches() {
    let temp = tempfile::tempdir().unwrap();
    let fx = fixture(&temp);
    let gateway = GovernedWriteToolGateway::open(&fx.db).unwrap();
    let capability = ProjectCapability::new(&fx.project_id, &fx.root).unwrap();
    let store = SqliteWriteProposalRepository::open(&fx.db).unwrap();

    let base = format!("{SECRET_BASE}\nline2\n");
    let proposed = format!("{SECRET_PROPOSED}\nline2\n");
    fs::write(fx.root.join("note.txt"), &base).unwrap();
    let input = reserve_and_approve_modify(&store, &fx, "note.txt", &proposed, &base);

    let outcome = gateway
        .apply(
            &capability,
            &input.proposal_id,
            &sealed(&proposed, Some(&base)),
            &NeverCancelled,
        )
        .unwrap();
    assert!(matches!(outcome, GovernedWriteApplyOutcome::Succeeded(_)));
    assert_eq!(
        fs::read_to_string(fx.root.join("note.txt")).unwrap(),
        proposed
    );

    // Distinctive secrets must not appear in SQLite bytes.
    let db_bytes = fs::read(&fx.db).unwrap();
    let hay = String::from_utf8_lossy(&db_bytes);
    assert!(!hay.contains(SECRET_PROPOSED));
    assert!(!hay.contains(SECRET_BASE));
}

#[test]
fn create_fails_when_target_already_exists() {
    let temp = tempfile::tempdir().unwrap();
    let fx = fixture(&temp);
    let gateway = GovernedWriteToolGateway::open(&fx.db).unwrap();
    let capability = ProjectCapability::new(&fx.project_id, &fx.root).unwrap();
    let store = SqliteWriteProposalRepository::open(&fx.db).unwrap();

    fs::write(fx.root.join("exists.txt"), "already").unwrap();
    let proposed = "new content\n";
    let input = reserve_and_approve_create(&store, &fx, "exists.txt", proposed);

    let outcome = gateway
        .apply(
            &capability,
            &input.proposal_id,
            &sealed(proposed, None),
            &NeverCancelled,
        )
        .unwrap();
    assert_failure_code(outcome, WriteApplyErrorCode::PathRejected);
    assert_eq!(
        fs::read_to_string(fx.root.join("exists.txt")).unwrap(),
        "already"
    );
}

#[test]
fn modify_fails_when_target_missing() {
    let temp = tempfile::tempdir().unwrap();
    let fx = fixture(&temp);
    let gateway = GovernedWriteToolGateway::open(&fx.db).unwrap();
    let capability = ProjectCapability::new(&fx.project_id, &fx.root).unwrap();
    let store = SqliteWriteProposalRepository::open(&fx.db).unwrap();

    let base = "base\n";
    let proposed = "proposed\n";
    let input = reserve_and_approve_modify(&store, &fx, "missing.txt", proposed, base);

    let outcome = gateway
        .apply(
            &capability,
            &input.proposal_id,
            &sealed(proposed, Some(base)),
            &NeverCancelled,
        )
        .unwrap();
    assert_failure_code(outcome, WriteApplyErrorCode::PathRejected);
    assert!(!fx.root.join("missing.txt").exists());
}

#[test]
fn modify_fails_on_oversize_or_binary_base() {
    let temp = tempfile::tempdir().unwrap();
    let fx = fixture(&temp);
    let gateway = GovernedWriteToolGateway::open(&fx.db).unwrap();
    let capability = ProjectCapability::new(&fx.project_id, &fx.root).unwrap();
    let store = SqliteWriteProposalRepository::open(&fx.db).unwrap();

    // Binary base (NUL).
    fs::write(fx.root.join("bin.txt"), b"a\0b").unwrap();
    // Reserve with a fake UTF-8 base digest so storage accepts; apply re-reads disk.
    let fake_base = "aXb";
    let proposed = "clean\n";
    let input = reserve_and_approve_modify(&store, &fx, "bin.txt", proposed, fake_base);
    let outcome = gateway
        .apply(
            &capability,
            &input.proposal_id,
            &sealed(proposed, Some(fake_base)),
            &NeverCancelled,
        )
        .unwrap();
    assert_failure_code(outcome, WriteApplyErrorCode::PathRejected);

    // Oversize base.
    let temp2 = tempfile::tempdir().unwrap();
    let fx2 = fixture(&temp2);
    let gateway2 = GovernedWriteToolGateway::open(&fx2.db).unwrap();
    let capability2 = ProjectCapability::new(&fx2.project_id, &fx2.root).unwrap();
    let store2 = SqliteWriteProposalRepository::open(&fx2.db).unwrap();
    let oversize = "o".repeat((WRITE_CONTENT_BYTE_CAP as usize) + 1);
    fs::write(fx2.root.join("big.txt"), &oversize).unwrap();
    let short_base = "o";
    let proposed2 = "small\n";
    let input2 = reserve_and_approve_modify(&store2, &fx2, "big.txt", proposed2, short_base);
    let outcome2 = gateway2
        .apply(
            &capability2,
            &input2.proposal_id,
            &sealed(proposed2, Some(short_base)),
            &NeverCancelled,
        )
        .unwrap();
    assert_failure_code(outcome2, WriteApplyErrorCode::PathRejected);
}

#[test]
fn base_drift_returns_base_changed_without_clobber() {
    let temp = tempfile::tempdir().unwrap();
    let fx = fixture(&temp);
    let gateway = GovernedWriteToolGateway::open(&fx.db).unwrap();
    let capability = ProjectCapability::new(&fx.project_id, &fx.root).unwrap();
    let store = SqliteWriteProposalRepository::open(&fx.db).unwrap();

    let base = "original base\n";
    let proposed = "new content\n";
    fs::write(fx.root.join("drift.txt"), base).unwrap();
    let input = reserve_and_approve_modify(&store, &fx, "drift.txt", proposed, base);
    // External edit after approval.
    fs::write(fx.root.join("drift.txt"), "drifted externally\n").unwrap();

    let outcome = gateway
        .apply(
            &capability,
            &input.proposal_id,
            &sealed(proposed, Some(base)),
            &NeverCancelled,
        )
        .unwrap();
    assert_failure_code(outcome, WriteApplyErrorCode::BaseChanged);
    assert_eq!(
        fs::read_to_string(fx.root.join("drift.txt")).unwrap(),
        "drifted externally\n"
    );
}

#[test]
fn denied_path_classes_are_rejected() {
    let temp = tempfile::tempdir().unwrap();
    let fx = fixture(&temp);
    let gateway = GovernedWriteToolGateway::open(&fx.db).unwrap();
    let capability = ProjectCapability::new(&fx.project_id, &fx.root).unwrap();
    let store = SqliteWriteProposalRepository::open(&fx.db).unwrap();

    let denied = [
        ".git/config",
        "nested/.pillow/secret",
        "a/.keeper/x",
        ".ssh/id_rsa",
        ".env",
        ".env.local",
        "keys/id_rsa",
        "keys/id_ed25519",
        "cert.pem",
        "private.key",
        "store.p12",
        "store.pfx",
        "java.jks",
        "android.keystore",
    ];
    let proposed = "blocked\n";
    for path in denied {
        // Parent dirs where needed so failure is policy, not missing parent.
        if let Some(parent) = std::path::Path::new(path).parent()
            && !parent.as_os_str().is_empty()
        {
            let _ = fs::create_dir_all(fx.root.join(parent));
        }
        let input = reserve_and_approve_create(&store, &fx, path, proposed);
        let outcome = gateway
            .apply(
                &capability,
                &input.proposal_id,
                &sealed(proposed, None),
                &NeverCancelled,
            )
            .unwrap();
        assert_failure_code(outcome, WriteApplyErrorCode::PathRejected);
        assert!(
            !fx.root.join(path).exists(),
            "denied path must not be created: {path}"
        );
    }

    // .env.example remains allowed.
    let allowed = reserve_and_approve_create(&store, &fx, ".env.example", "KEY=\n");
    let outcome = gateway
        .apply(
            &capability,
            &allowed.proposal_id,
            &sealed("KEY=\n", None),
            &NeverCancelled,
        )
        .unwrap();
    assert!(matches!(outcome, GovernedWriteApplyOutcome::Succeeded(_)));
    assert_eq!(
        fs::read_to_string(fx.root.join(".env.example")).unwrap(),
        "KEY=\n"
    );
}

#[test]
fn traversal_device_and_ads_paths_are_rejected() {
    let temp = tempfile::tempdir().unwrap();
    let fx = fixture(&temp);
    let gateway = GovernedWriteToolGateway::open(&fx.db).unwrap();
    let capability = ProjectCapability::new(&fx.project_id, &fx.root).unwrap();
    let store = SqliteWriteProposalRepository::open(&fx.db).unwrap();
    let proposed = "x\n";

    let bad_paths = [
        "../escape.txt",
        "a/../../b.txt",
        "C:/windows/x.txt",
        "abs\\path",
        "foo:bar",
        "file:stream",
        "CON",
        "COM1.txt",
        "trailing.",
        "space ",
    ];
    for path in bad_paths {
        // Storage accepts any non-empty path string; executor enforces grammar.
        let mut input = reserve_create(&fx, "placeholder.txt", proposed);
        input.path = path.into();
        input.proposal_id = Uuid::new_v4().to_string();
        input.reservation_key = format!("wp-bad-{}", Uuid::new_v4());
        input.call_id = format!("call-{}", Uuid::new_v4());
        // Some paths may fail storage validation (empty etc.) — only test those that reserve.
        if store.reserve(&input).is_err() {
            continue;
        }
        approve(&store, &fx, &input);
        let outcome = gateway
            .apply(
                &capability,
                &input.proposal_id,
                &sealed(proposed, None),
                &NeverCancelled,
            )
            .unwrap();
        assert_failure_code(outcome, WriteApplyErrorCode::PathRejected);
    }
}

#[test]
fn directory_symlink_and_reparse_targets_are_rejected() {
    let temp = tempfile::tempdir().unwrap();
    let fx = fixture(&temp);
    let gateway = GovernedWriteToolGateway::open(&fx.db).unwrap();
    let capability = ProjectCapability::new(&fx.project_id, &fx.root).unwrap();
    let store = SqliteWriteProposalRepository::open(&fx.db).unwrap();

    // Directory as modify target.
    fs::create_dir(fx.root.join("adir")).unwrap();
    let base = "unused\n";
    let proposed = "data\n";
    let input = reserve_and_approve_modify(&store, &fx, "adir", proposed, base);
    let outcome = gateway
        .apply(
            &capability,
            &input.proposal_id,
            &sealed(proposed, Some(base)),
            &NeverCancelled,
        )
        .unwrap();
    assert_failure_code(outcome, WriteApplyErrorCode::PathRejected);

    // Symlink component (Windows).
    #[cfg(windows)]
    {
        use std::os::windows::fs::symlink_file;
        fs::write(fx.root.join("real.txt"), "real\n").unwrap();
        let link = fx.root.join("link.txt");
        if symlink_file(fx.root.join("real.txt"), &link).is_ok() {
            let base = "real\n";
            let proposed = "via-link\n";
            let input = reserve_and_approve_modify(&store, &fx, "link.txt", proposed, base);
            let outcome = gateway
                .apply(
                    &capability,
                    &input.proposal_id,
                    &sealed(proposed, Some(base)),
                    &NeverCancelled,
                )
                .unwrap();
            assert_failure_code(outcome, WriteApplyErrorCode::PathRejected);
            assert_eq!(
                fs::read_to_string(fx.root.join("real.txt")).unwrap(),
                "real\n"
            );
        }
    }
}

struct FlagCancel {
    flag: AtomicBool,
}

impl CancellationSignal for FlagCancel {
    fn is_cancelled(&self) -> bool {
        self.flag.load(Ordering::SeqCst)
    }
}

#[test]
fn pre_mutation_cancel_after_claim_is_interrupted_not_cancelled() {
    let temp = tempfile::tempdir().unwrap();
    let fx = fixture(&temp);
    let gateway = GovernedWriteToolGateway::open(&fx.db).unwrap();
    let capability = ProjectCapability::new(&fx.project_id, &fx.root).unwrap();
    let store = SqliteWriteProposalRepository::open(&fx.db).unwrap();

    let proposed = "should-not-land\n";
    let input = reserve_and_approve_create(&store, &fx, "cancel.txt", proposed);
    let cancel = FlagCancel {
        flag: AtomicBool::new(true),
    };

    let outcome = gateway
        .apply(
            &capability,
            &input.proposal_id,
            &sealed(proposed, None),
            &cancel,
        )
        .unwrap();
    // C3-A: applying -> interrupted only (never applying -> cancelled).
    let GovernedWriteApplyOutcome::Interrupted(row) = outcome else {
        panic!("expected Interrupted after claimed apply cancel, got {outcome:?}");
    };
    assert_eq!(
        row.state,
        WriteProposalState::Interrupted,
        "durable state must be interrupted, not cancelled/failed"
    );
    assert_ne!(row.state, WriteProposalState::Cancelled);
    assert_ne!(row.state, WriteProposalState::Failed);

    let history = store.history(&input.proposal_id).unwrap();
    assert!(
        history.contains(&WriteProposalState::Applying),
        "history must record applying: {history:?}"
    );
    assert!(
        history.contains(&WriteProposalState::Interrupted),
        "history must record interrupted: {history:?}"
    );
    assert!(
        !history.contains(&WriteProposalState::Cancelled),
        "history must not claim cancelled after apply claim: {history:?}"
    );
    assert_eq!(
        history.last().copied(),
        Some(WriteProposalState::Interrupted)
    );

    assert!(!fx.root.join("cancel.txt").exists());
    let leftovers: Vec<_> = fs::read_dir(&fx.root)
        .unwrap()
        .filter_map(|e| e.ok())
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .filter(|n| n.contains("pillow-tmp"))
        .collect();
    assert!(leftovers.is_empty(), "temp not cleaned: {leftovers:?}");
}

#[test]
fn concurrent_apply_second_caller_is_busy_or_terminal() {
    let temp = tempfile::tempdir().unwrap();
    let fx = fixture(&temp);
    let store = SqliteWriteProposalRepository::open(&fx.db).unwrap();
    let proposed = "race-content\n";
    let input = reserve_and_approve_create(&store, &fx, "race.txt", proposed);

    let barrier = Arc::new(Barrier::new(2));
    let db = fx.db.clone();
    let root = fx.root.clone();
    let project_id = fx.project_id.clone();
    let proposal_id = input.proposal_id.clone();

    let b1 = Arc::clone(&barrier);
    let db1 = db.clone();
    let root1 = root.clone();
    let pid1 = project_id.clone();
    let prop1 = proposal_id.clone();
    let t1 = thread::spawn(move || {
        let gateway = GovernedWriteToolGateway::open(&db1).unwrap();
        let capability = ProjectCapability::new(&pid1, &root1).unwrap();
        b1.wait();
        gateway
            .apply(
                &capability,
                &prop1,
                &sealed(proposed, None),
                &NeverCancelled,
            )
            .unwrap()
    });

    let b2 = Arc::clone(&barrier);
    let t2 = thread::spawn(move || {
        let gateway = GovernedWriteToolGateway::open(&db).unwrap();
        let capability = ProjectCapability::new(&project_id, &root).unwrap();
        b2.wait();
        // Small delay so one claim usually wins first.
        thread::sleep(Duration::from_millis(5));
        gateway
            .apply(
                &capability,
                &proposal_id,
                &sealed(proposed, None),
                &NeverCancelled,
            )
            .unwrap()
    });

    let o1 = t1.join().unwrap();
    let o2 = t2.join().unwrap();
    let outcomes = [o1, o2];
    let successes = outcomes
        .iter()
        .filter(|o| matches!(o, GovernedWriteApplyOutcome::Succeeded(_)))
        .count();
    let busy_or_terminal = outcomes
        .iter()
        .filter(|o| {
            matches!(
                o,
                GovernedWriteApplyOutcome::Busy(_)
                    | GovernedWriteApplyOutcome::Terminal(_)
                    | GovernedWriteApplyOutcome::Succeeded(_)
            )
        })
        .count();
    assert_eq!(successes, 1, "exactly one apply must succeed");
    assert_eq!(busy_or_terminal, 2);
    assert_eq!(
        fs::read_to_string(fx.root.join("race.txt")).unwrap(),
        proposed
    );
}

#[test]
fn sealed_debug_redacts_base_and_proposed_content() {
    let sealed = SealedWriteContents::new(SECRET_PROPOSED.to_owned(), Some(SECRET_BASE.to_owned()));
    let debug = format!("{sealed:?}");
    assert!(!debug.contains(SECRET_PROPOSED));
    assert!(!debug.contains(SECRET_BASE));
    assert!(debug.contains("redacted") || debug.contains("REDACTED"));
}

#[test]
fn public_source_bearing_debug_surfaces_are_redacted() {
    let temp = tempfile::tempdir().unwrap();
    let fx = fixture(&temp);
    let gateway = GovernedWriteToolGateway::open(&fx.db).unwrap();
    let capability = ProjectCapability::new(&fx.project_id, &fx.root).unwrap();
    let store = SqliteWriteProposalRepository::open(&fx.db).unwrap();

    fs::write(fx.root.join("priv.txt"), format!("{SECRET_BASE}\n")).unwrap();
    let proposed = format!("{SECRET_PROPOSED}\n{SECRET_DIFF}\n");
    let base = format!("{SECRET_BASE}\n");
    let snap = GovernedWriteToolGateway::capture_base(
        &capability,
        "priv.txt",
        WriteMode::Modify,
        &proposed,
    )
    .unwrap();
    // Distinctive secrets live in the public structure.
    assert!(snap.base_content.as_deref().unwrap().contains(SECRET_BASE));
    let snap_debug = format!("{snap:?}");
    assert!(
        !snap_debug.contains(SECRET_BASE),
        "BaseSnapshot Debug leaked base: {snap_debug}"
    );
    assert!(
        !snap_debug.contains(SECRET_PROPOSED),
        "BaseSnapshot Debug leaked proposed secret: {snap_debug}"
    );

    let input = reserve_and_approve_modify(&store, &fx, "priv.txt", &proposed, &base);
    let prepared = gateway
        .prepare(
            store.get(&input.proposal_id).unwrap().unwrap(),
            sealed(&proposed, Some(&base)),
        )
        .unwrap();
    assert!(
        prepared
            .base_content
            .as_deref()
            .unwrap()
            .contains(SECRET_BASE)
    );
    assert!(prepared.proposed_content.contains(SECRET_PROPOSED));
    assert!(
        prepared.diff.unified.contains(SECRET_DIFF)
            || prepared.diff.unified.contains(SECRET_PROPOSED)
    );

    let prepared_debug = format!("{prepared:?}");
    assert!(
        !prepared_debug.contains(SECRET_BASE),
        "PreparedWriteProposal Debug leaked base: {prepared_debug}"
    );
    assert!(
        !prepared_debug.contains(SECRET_PROPOSED),
        "PreparedWriteProposal Debug leaked proposed: {prepared_debug}"
    );
    assert!(
        !prepared_debug.contains(SECRET_DIFF),
        "PreparedWriteProposal Debug leaked diff secret: {prepared_debug}"
    );
    let diff_debug = format!("{:?}", prepared.diff);
    assert!(!diff_debug.contains(SECRET_DIFF));
    assert!(!diff_debug.contains(SECRET_PROPOSED));
    assert!(!diff_debug.contains(SECRET_BASE));
}

#[test]
fn prepare_rejects_digest_mismatch_against_metadata() {
    let temp = tempfile::tempdir().unwrap();
    let fx = fixture(&temp);
    let gateway = GovernedWriteToolGateway::open(&fx.db).unwrap();
    let store = SqliteWriteProposalRepository::open(&fx.db).unwrap();
    let proposed = "correct\n";
    let input = reserve_and_approve_create(&store, &fx, "prep.txt", proposed);
    let metadata = store.get(&input.proposal_id).unwrap().unwrap();
    let err = gateway
        .prepare(metadata, sealed("wrong content\n", None))
        .unwrap_err();
    let msg = format!("{err}");
    assert!(
        msg.to_lowercase().contains("digest") || msg.to_lowercase().contains("mismatch"),
        "unexpected error: {msg}"
    );
}

#[test]
fn create_requires_existing_parent_directory() {
    let temp = tempfile::tempdir().unwrap();
    let fx = fixture(&temp);
    let gateway = GovernedWriteToolGateway::open(&fx.db).unwrap();
    let capability = ProjectCapability::new(&fx.project_id, &fx.root).unwrap();
    let store = SqliteWriteProposalRepository::open(&fx.db).unwrap();
    let proposed = "orphan\n";
    let input = reserve_and_approve_create(&store, &fx, "nope/child.txt", proposed);
    let outcome = gateway
        .apply(
            &capability,
            &input.proposal_id,
            &sealed(proposed, None),
            &NeverCancelled,
        )
        .unwrap();
    assert_failure_code(outcome, WriteApplyErrorCode::PathRejected);
    assert!(!fx.root.join("nope/child.txt").exists());
}

#[test]
fn capture_base_for_create_and_modify() {
    let temp = tempfile::tempdir().unwrap();
    let fx = fixture(&temp);
    let capability = ProjectCapability::new(&fx.project_id, &fx.root).unwrap();

    let create = GovernedWriteToolGateway::capture_base(
        &capability,
        "new.txt",
        WriteMode::Create,
        "hello\n",
    )
    .unwrap();
    assert!(create.base_content.is_none());
    assert!(create.base_digest.is_none());
    assert_eq!(create.base_bytes, 0);
    assert_eq!(create.proposed_digest, digest_of("hello\n"));
    assert_eq!(create.proposed_bytes, 6);

    fs::write(fx.root.join("old.txt"), "old\n").unwrap();
    let modify =
        GovernedWriteToolGateway::capture_base(&capability, "old.txt", WriteMode::Modify, "new\n")
            .unwrap();
    assert_eq!(modify.base_content.as_deref(), Some("old\n"));
    assert_eq!(
        modify.base_digest.as_deref(),
        Some(digest_of("old\n").as_str())
    );
    assert_eq!(modify.base_bytes, 4);
}

fn new_run_on_conversation(fx: &Fixture, label: &str) -> ProtocolRunId {
    let conversations = SqliteConversationRepository::open(&fx.db).unwrap();
    let run = conversations
        .create_run(NewRun {
            conversation_id: keeper_session::ConversationId(
                Uuid::parse_str(&fx.conversation_id).unwrap(),
            ),
            user_message: label.into(),
        })
        .unwrap();
    ProtocolRunId(run.id.0)
}

fn rebind_run(
    mut input: WriteProposalReserveInput,
    run_id: ProtocolRunId,
) -> WriteProposalReserveInput {
    input.run_id = run_id;
    input.reservation_key = format!(
        "wp:{}:{}:{}:{}",
        input.project_id, input.conversation_id, run_id, input.call_id
    );
    input
}

#[test]
fn observe_applying_target_create_and_modify_cases() {
    let temp = tempfile::tempdir().unwrap();
    let fx = fixture(&temp);
    let gateway = GovernedWriteToolGateway::open(&fx.db).unwrap();
    let capability = ProjectCapability::new(&fx.project_id, &fx.root).unwrap();
    let store = SqliteWriteProposalRepository::open(&fx.db).unwrap();

    // Create absent → StillBaseOrAbsent
    let create_input = reserve_and_approve_create(&store, &fx, "obs-create.txt", "proposed\n");
    store
        .claim_apply(&create_input.proposal_id, "obs-owner", 30_000, Utc::now())
        .unwrap();
    let create_row = store.get(&create_input.proposal_id).unwrap().unwrap();
    assert_eq!(
        gateway.observe_applying_target(&capability, &create_row),
        WriteApplyReconcileObservation::StillBaseOrAbsent
    );

    // Create with proposed content → MatchesProposed
    fs::write(fx.root.join("obs-create.txt"), "proposed\n").unwrap();
    assert_eq!(
        gateway.observe_applying_target(&capability, &create_row),
        WriteApplyReconcileObservation::MatchesProposed
    );

    // Create with neither content → Neither
    fs::write(fx.root.join("obs-create.txt"), "other\n").unwrap();
    assert_eq!(
        gateway.observe_applying_target(&capability, &create_row),
        WriteApplyReconcileObservation::Neither
    );

    // Modify base → StillBaseOrAbsent; proposed → MatchesProposed; other → Neither
    let base = "base-body\n";
    let proposed = "new-body\n";
    fs::write(fx.root.join("obs-mod.txt"), base).unwrap();
    let mod_run = new_run_on_conversation(&fx, "mod");
    let mod_input = rebind_run(reserve_modify(&fx, "obs-mod.txt", proposed, base), mod_run);
    store.reserve(&mod_input).unwrap();
    approve(&store, &fx, &mod_input);
    store
        .claim_apply(&mod_input.proposal_id, "obs-mod-owner", 30_000, Utc::now())
        .unwrap();
    let mod_row = store.get(&mod_input.proposal_id).unwrap().unwrap();
    assert_eq!(
        gateway.observe_applying_target(&capability, &mod_row),
        WriteApplyReconcileObservation::StillBaseOrAbsent
    );
    fs::write(fx.root.join("obs-mod.txt"), proposed).unwrap();
    assert_eq!(
        gateway.observe_applying_target(&capability, &mod_row),
        WriteApplyReconcileObservation::MatchesProposed
    );
    fs::write(fx.root.join("obs-mod.txt"), "neither\n").unwrap();
    assert_eq!(
        gateway.observe_applying_target(&capability, &mod_row),
        WriteApplyReconcileObservation::Neither
    );

    // Missing modify target → Neither
    let miss_run = new_run_on_conversation(&fx, "miss");
    let missing = rebind_run(
        reserve_modify(&fx, "missing-mod.txt", "new\n", "gone\n"),
        miss_run,
    );
    store.reserve(&missing).unwrap();
    approve(&store, &fx, &missing);
    store
        .claim_apply(&missing.proposal_id, "obs-miss-owner", 30_000, Utc::now())
        .unwrap();
    let miss_row = store.get(&missing.proposal_id).unwrap().unwrap();
    assert_eq!(
        gateway.observe_applying_target(&capability, &miss_row),
        WriteApplyReconcileObservation::Neither
    );

    // Project mismatch → Neither
    let foreign = ProjectCapability::new("other-project-id", &fx.root).unwrap();
    assert_eq!(
        gateway.observe_applying_target(&foreign, &create_row),
        WriteApplyReconcileObservation::Neither
    );

    // Observation never mutates disk content of unrelated files.
    assert_eq!(
        fs::read_to_string(fx.root.join("obs-create.txt")).unwrap(),
        "other\n"
    );
}
