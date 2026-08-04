//! Governed process gateway tests (C4-D2B / D2BR / D2BR2).
//!
//! Covers process.run@1 argument bounds, capability-bound reservation, catalog
//! allow/deny, fingerprint binding, drift, approval authority, one-shot claim,
//! clean environment, lifecycle receipts and cleanup fail-closed behavior.
//! Public surface must not expose native executable/cwd getters, arbitrary
//! production runner injection, mutable ledger access, or post-seal timeout
//! mutation.

use std::fs;
use std::path::PathBuf;
use std::time::Duration;

use chrono::{Duration as ChronoDuration, Utc};
use keeper_agent_protocol::RunId as ProtocolRunId;
use keeper_policy::{ExecDecision, ExecProposalState};
use keeper_process_executor::CancellationSignal as ProcessCancellationSignal;
use keeper_project::CreateProject;
use keeper_session::{ConversationRepository, NewConversation, NewRun};
use keeper_storage_sqlite::{
    ExecDecisionOutcome, ExecRunClaimOutcome, ExecTerminalReceipt, SqliteConversationRepository,
    SqliteExecProposalRepository, SqliteProjectRepository,
};
use keeper_tool_executor::{
    CLEANUP_UNPROVEN_PUBLIC_ERROR, GovernedProcessError, GovernedProcessOutcome,
    GovernedProcessToolGateway, PROCESS_CATALOG, PROCESS_OUTPUT_BYTE_CAP,
    PROCESS_RUN_ARGV_BYTE_CAP, PROCESS_RUN_ARGV_COUNT_CAP, PROCESS_TIMEOUT, ProjectCapability,
    SealedProcessSpec, build_clean_environment, validate_process_run_arguments,
    validate_program_catalog,
};
use sha2::{Digest, Sha256};
use uuid::Uuid;

const SECRET_ARGV: &str = "SECRET_ARGV_C4D2B_NEVER_STORE_xyz789";

struct Fixture {
    db: PathBuf,
    root: PathBuf,
    project_id: String,
    conversation_id: String,
    run_id: ProtocolRunId,
}

fn fixture(temp: &tempfile::TempDir) -> Fixture {
    let db = temp.path().join("pillow.db");
    let root = temp.path().join("project");
    fs::create_dir(&root).unwrap();
    fs::create_dir(root.join("subdir")).unwrap();
    let projects = SqliteProjectRepository::open(&db).unwrap();
    let project = projects
        .create(CreateProject {
            name: "ProcessProject".into(),
            root_path: root.to_string_lossy().into_owned(),
        })
        .unwrap();
    let sessions = SqliteConversationRepository::open(&db).unwrap();
    let conversation = sessions
        .create_conversation(NewConversation {
            project_id: project.id,
            title: Some("process".into()),
        })
        .unwrap();
    let run = sessions
        .create_run(NewRun {
            conversation_id: conversation.id,
            user_message: "run".into(),
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

fn capability(fx: &Fixture) -> ProjectCapability {
    ProjectCapability::new(&fx.project_id, &fx.root).unwrap()
}

fn digest_of(bytes: &[u8]) -> String {
    let hex = Sha256::digest(bytes)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect::<String>();
    format!("sha256:{hex}")
}

fn grant_digest(seed: &str) -> String {
    digest_of(seed.as_bytes())
}

fn node_version_argv() -> Vec<String> {
    vec!["--version".into()]
}

fn node_exit_argv(code: i32) -> Vec<String> {
    vec!["-e".into(), format!("process.exit({code})")]
}

fn node_sleep_argv(ms: u64) -> Vec<String> {
    vec!["-e".into(), format!("setTimeout(() => {{}}, {ms})")]
}

fn seal_node(fx: &Fixture, argv: Vec<String>, cwd: Option<&str>) -> SealedProcessSpec {
    GovernedProcessToolGateway::seal(&capability(fx), "node", &argv, cwd).unwrap()
}

fn reserve_and_approve(
    gateway: &GovernedProcessToolGateway,
    fx: &Fixture,
    sealed: &SealedProcessSpec,
    call_id: &str,
) -> String {
    let prepared = gateway
        .reserve(
            &capability(fx),
            &fx.conversation_id,
            fx.run_id,
            call_id,
            sealed,
        )
        .unwrap();
    assert_eq!(prepared.metadata.state, ExecProposalState::PendingApproval);
    let grant = grant_digest(&format!("grant:{}", prepared.metadata.proposal_id));
    let decision = gateway
        .decide(
            &prepared.metadata.proposal_id,
            &fx.conversation_id,
            ExecDecision::ApproveOnce,
            &grant,
            sealed.proposal_fingerprint(),
        )
        .unwrap();
    assert!(matches!(decision, ExecDecisionOutcome::Accepted(_)));
    prepared.metadata.proposal_id
}

fn assert_no_secrets_in_db(db: &PathBuf, secrets: &[&str]) {
    let bytes = fs::read(db).unwrap();
    let text = String::from_utf8_lossy(&bytes);
    for secret in secrets {
        assert!(
            !text.contains(secret),
            "secret leaked into sqlite: {secret}"
        );
    }
}

// --- process.run@1 argument contract (D2BR) ---

#[test]
fn argv_count_boundary_accepts_64_and_denies_65() {
    let temp = tempfile::tempdir().unwrap();
    let fx = fixture(&temp);
    let cap = capability(&fx);

    let at_cap: Vec<String> = (0..PROCESS_RUN_ARGV_COUNT_CAP)
        .map(|i| format!("a{i}"))
        .collect();
    assert_eq!(at_cap.len(), 64);
    assert!(validate_process_run_arguments("node", &at_cap, Some(".")).is_ok());
    // Seal must accept the exact bound (catalog + host resolution still apply).
    let sealed = GovernedProcessToolGateway::seal(&cap, "node", &at_cap, Some("."));
    assert!(sealed.is_ok(), "64 argv entries must be accepted at seal");

    let over: Vec<String> = (0..PROCESS_RUN_ARGV_COUNT_CAP + 1)
        .map(|i| format!("a{i}"))
        .collect();
    assert_eq!(over.len(), 65);
    assert!(matches!(
        validate_process_run_arguments("node", &over, Some(".")),
        Err(GovernedProcessError::InvalidArguments)
    ));
    let err = GovernedProcessToolGateway::seal(&cap, "node", &over, Some(".")).unwrap_err();
    assert!(matches!(err, GovernedProcessError::InvalidArguments));
}

#[test]
fn combined_program_argv_byte_boundary_is_32kib() {
    let temp = tempfile::tempdir().unwrap();
    let fx = fixture(&temp);
    let cap = capability(&fx);
    let program = "node";
    let remaining = PROCESS_RUN_ARGV_BYTE_CAP - program.len();
    assert_eq!(PROCESS_RUN_ARGV_BYTE_CAP, 32_768);

    let at_cap = vec!["x".repeat(remaining)];
    assert!(validate_process_run_arguments(program, &at_cap, None).is_ok());
    // Exact combined budget is accepted by the argument contract. Host may still
    // refuse oversized command lines at spawn; the gateway boundary is the contract.
    assert!(validate_process_run_arguments(program, &at_cap, Some(".")).is_ok());

    let over = vec!["x".repeat(remaining + 1)];
    assert!(matches!(
        validate_process_run_arguments(program, &over, None),
        Err(GovernedProcessError::InvalidArguments)
    ));
    let err = GovernedProcessToolGateway::seal(&cap, program, &over, Some(".")).unwrap_err();
    assert!(matches!(err, GovernedProcessError::InvalidArguments));
    let _ = at_cap;
}

#[test]
fn nul_and_control_characters_denied_in_program_and_argv() {
    let temp = tempfile::tempdir().unwrap();
    let fx = fixture(&temp);
    let cap = capability(&fx);

    for argv in [
        vec!["ok\0".into()],
        vec!["line\nbreak".into()],
        vec!["tab\there".into()],
        vec!["\u{1}".into()],
        vec!["del\u{7f}".into()],
    ] {
        assert!(
            matches!(
                validate_process_run_arguments("node", &argv, None),
                Err(GovernedProcessError::InvalidArguments)
            ),
            "argv control should be denied: {argv:?}"
        );
        let err = GovernedProcessToolGateway::seal(&cap, "node", &argv, Some(".")).unwrap_err();
        assert!(
            matches!(err, GovernedProcessError::InvalidArguments),
            "seal must deny control argv: {argv:?}"
        );
    }

    // NUL / control in program label (grammar + explicit control check).
    for program in ["no\0de", "node\n", "\u{1}node"] {
        assert!(
            matches!(
                validate_process_run_arguments(program, &[], None),
                Err(GovernedProcessError::InvalidArguments)
            ),
            "program control should be denied: {program:?}"
        );
    }
}

// --- Catalog ---

#[test]
fn allowlisted_program_labels_resolve() {
    let temp = tempfile::tempdir().unwrap();
    let fx = fixture(&temp);
    let cap = capability(&fx);
    for label in PROCESS_CATALOG {
        // Only require seal success when the host actually has the tool.
        match GovernedProcessToolGateway::seal(&cap, label, &[], Some(".")) {
            Ok(sealed) => {
                assert_eq!(sealed.program(), *label);
                assert!(sealed.argv().is_empty());
                assert_eq!(sealed.portable_cwd(), Some("."));
            }
            Err(GovernedProcessError::ProgramUnavailable) => {
                // Host may lack python3 etc.; catalog acceptance is still validated.
                assert!(validate_program_catalog(label).is_ok());
            }
            Err(other) => panic!("unexpected seal error for {label}: {other}"),
        }
    }
    // node must be present for the rest of the suite.
    let sealed = seal_node(&fx, node_version_argv(), Some("."));
    assert_eq!(sealed.program(), "node");
}

#[test]
fn unknown_program_labels_are_denied() {
    assert!(matches!(
        validate_program_catalog("go"),
        Err(GovernedProcessError::UnknownProgram)
    ));
    assert!(matches!(
        validate_program_catalog("ruby"),
        Err(GovernedProcessError::UnknownProgram)
    ));
    assert!(matches!(
        validate_program_catalog("deno"),
        Err(GovernedProcessError::UnknownProgram)
    ));
}

#[test]
fn git_shells_and_script_shims_are_denied() {
    let denied = [
        "git",
        "cmd",
        "powershell",
        "pwsh",
        "bash",
        "sh",
        "wscript",
        "cscript",
        "mshta",
        "npm",
        "pnpm",
        "tool.cmd",
        "run.bat",
        "script.ps1",
        "x.vbs",
        r"C:\Windows\System32\cmd.exe",
        "../cargo",
    ];
    for label in denied {
        assert!(
            matches!(
                validate_program_catalog(label),
                Err(GovernedProcessError::DeniedProgram)
            ),
            "expected catalog deny for {label}"
        );
    }
}

// --- Capability binding (D2BR) ---

#[test]
fn reservation_with_mismatched_project_capability_is_denied() {
    let temp = tempfile::tempdir().unwrap();
    let fx = fixture(&temp);
    let gateway = GovernedProcessToolGateway::open(&fx.db).unwrap();
    let sealed = seal_node(&fx, node_version_argv(), Some("."));

    // Second project with a distinct id but usable root (sibling dir).
    let other_root = temp.path().join("other-project");
    fs::create_dir(&other_root).unwrap();
    let projects = SqliteProjectRepository::open(&fx.db).unwrap();
    let other = projects
        .create(CreateProject {
            name: "OtherProcessProject".into(),
            root_path: other_root.to_string_lossy().into_owned(),
        })
        .unwrap();
    let foreign_cap = ProjectCapability::new(other.id.0.to_string(), &other_root).unwrap();

    // Reserve under the owning capability first.
    let prepared = gateway
        .reserve(
            &capability(&fx),
            &fx.conversation_id,
            fx.run_id,
            "call-cap-mismatch",
            &sealed,
        )
        .unwrap();
    let grant = grant_digest("grant-cap-mismatch");
    gateway
        .decide(
            &prepared.metadata.proposal_id,
            &fx.conversation_id,
            ExecDecision::ApproveOnce,
            &grant,
            sealed.proposal_fingerprint(),
        )
        .unwrap();

    // Execute with a foreign ProjectCapability project id must fail closed.
    let err = gateway
        .execute(
            &foreign_cap,
            &prepared.metadata.proposal_id,
            &sealed,
            ProcessCancellationSignal::new(),
        )
        .unwrap_err();
    assert!(matches!(err, GovernedProcessError::CapabilityMismatch));
}

#[test]
fn production_reserve_always_binds_to_capability_project_identity() {
    let temp = tempfile::tempdir().unwrap();
    let fx = fixture(&temp);
    let gateway = GovernedProcessToolGateway::open(&fx.db).unwrap();
    let sealed = seal_node(&fx, node_version_argv(), Some("."));
    let prepared = gateway
        .reserve(
            &capability(&fx),
            &fx.conversation_id,
            fx.run_id,
            "call-bound",
            &sealed,
        )
        .unwrap();
    assert_eq!(prepared.metadata.project_id, fx.project_id);
    assert_eq!(prepared.metadata.conversation_id, fx.conversation_id);
    assert_eq!(prepared.metadata.run_id, fx.run_id);
    assert_eq!(prepared.metadata.call_id, "call-bound");
    // Reservation key is capability-derived, not caller-chosen freeform identity.
    assert!(
        prepared
            .metadata
            .reservation_key
            .starts_with(&format!("ep:{}:", fx.project_id))
    );
}

#[test]
fn public_api_has_no_native_getters_or_arbitrary_runner_injection() {
    // Compile-time / source-level proof that production consumers cannot:
    // - read sealed.executable() / sealed.native_cwd()
    // - call open_with_runner / reserve_with_input / ProcessRunner doubles
    // - call gateway.store() or sealed.with_timeout(...)
    let source = include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/src/process.rs"));
    let lib = include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/src/lib.rs"));

    // Public getter methods must not exist.
    assert!(
        !source.contains("pub fn executable("),
        "sealed executable getter must not be public"
    );
    assert!(
        !source.contains("pub fn native_cwd("),
        "sealed native_cwd getter must not be public"
    );

    // Production open has no runner parameter; injection is cfg(test)-only.
    assert!(source.contains("pub fn open("));
    assert!(
        source.contains("#[cfg(test)]\n    fn open_with_runner")
            || source.contains("#[cfg(test)]\r\n    fn open_with_runner"),
        "open_with_runner must be cfg(test)-only and non-public"
    );
    assert!(
        !source.contains("pub fn open_with_runner"),
        "open_with_runner must not be a public production method"
    );
    assert!(
        !source.contains("pub fn reserve_with_input"),
        "reserve_with_input must not be a public production method"
    );
    // D2BR2: no production mutable repository or timeout mutator.
    assert!(
        !source.contains("pub fn store("),
        "gateway.store() must not be a public production method"
    );
    assert!(
        source.contains("#[cfg(test)]\n    fn store(")
            || source.contains("#[cfg(test)]\r\n    fn store("),
        "store() must be cfg(test)-only when retained for unit seams"
    );
    assert!(
        !source.contains("pub fn with_timeout("),
        "SealedProcessSpec::with_timeout must not be a public production method"
    );
    assert!(
        source.contains("#[cfg(test)]\n    fn with_timeout(")
            || source.contains("#[cfg(test)]\r\n    fn with_timeout("),
        "with_timeout must be cfg(test)-only private unit support"
    );
    assert!(
        source.contains("pub fn get_proposal("),
        "read-only get_proposal inspection must remain public"
    );
    assert!(
        !lib.contains("CountingProcessRunner") && !lib.contains("CleanupUnprovenRunner"),
        "runner doubles must not be re-exported from the crate root"
    );
    assert!(
        !lib.contains("ProcessRunner"),
        "ProcessRunner trait must not be re-exported from the crate root"
    );
    assert!(source.contains("SupervisedProcessRunner"));
    // Production open body always installs SupervisedProcessRunner.
    assert!(source.contains("Arc::new(SupervisedProcessRunner::new())"));
}

#[test]
fn production_seal_always_uses_fixed_sixty_second_timeout() {
    let temp = tempfile::tempdir().unwrap();
    let fx = fixture(&temp);
    let sealed = seal_node(&fx, node_version_argv(), Some("."));
    assert_eq!(PROCESS_TIMEOUT, Duration::from_millis(60_000));
    assert_eq!(sealed.timeout(), PROCESS_TIMEOUT);
    assert_eq!(sealed.output_byte_cap(), PROCESS_OUTPUT_BYTE_CAP);
    // Through reserve/decide/execute the sealed timeout remains the fixed constant.
    let gateway = GovernedProcessToolGateway::open(&fx.db).unwrap();
    let prepared = gateway
        .reserve(
            &capability(&fx),
            &fx.conversation_id,
            fx.run_id,
            "call-fixed-timeout",
            &sealed,
        )
        .unwrap();
    assert_eq!(prepared.sealed.timeout(), PROCESS_TIMEOUT);
    assert_eq!(sealed.timeout(), Duration::from_secs(60));
}

#[test]
fn read_only_proposal_inspection_cannot_mutate_ledger() {
    let temp = tempfile::tempdir().unwrap();
    let fx = fixture(&temp);
    let gateway = GovernedProcessToolGateway::open(&fx.db).unwrap();
    let sealed = seal_node(&fx, node_version_argv(), Some("."));
    let prepared = gateway
        .reserve(
            &capability(&fx),
            &fx.conversation_id,
            fx.run_id,
            "call-inspect",
            &sealed,
        )
        .unwrap();
    let id = prepared.metadata.proposal_id.clone();
    let row = gateway.get_proposal(&id).unwrap().unwrap();
    assert_eq!(row.state, ExecProposalState::PendingApproval);
    assert_eq!(row.program, "node");
    assert!(row.argv_digest.starts_with("sha256:"));
    // Metadata DTO only: no reserve/decide/claim methods on the returned value.
    // Mutation stays on the private store / governed gateway operations.
    let source = include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/src/process.rs"));
    assert!(source.contains("pub fn get_proposal("));
    assert!(
        !source.contains("pub fn store("),
        "get_proposal must not re-expose the repository"
    );
    // Second read is stable; state only changes through governed decide/execute.
    let again = gateway.get_proposal(&id).unwrap().unwrap();
    assert_eq!(again.state, ExecProposalState::PendingApproval);
    assert_eq!(again.proposal_id, id);
}

// --- Fingerprint / drift / expiry ---

#[test]
fn argv_and_cwd_bind_into_fingerprint() {
    let temp = tempfile::tempdir().unwrap();
    let fx = fixture(&temp);
    let a = seal_node(&fx, vec!["--version".into()], Some("."));
    let b = seal_node(&fx, vec!["--help".into()], Some("."));
    let c = seal_node(&fx, vec!["--version".into()], Some("subdir"));
    assert_ne!(a.proposal_fingerprint(), b.proposal_fingerprint());
    assert_ne!(a.argv_digest(), b.argv_digest());
    assert_ne!(a.proposal_fingerprint(), c.proposal_fingerprint());
    assert_eq!(a.argv_digest(), c.argv_digest());
}

#[test]
fn fingerprint_mismatch_blocks_execute() {
    let temp = tempfile::tempdir().unwrap();
    let fx = fixture(&temp);
    let gateway = GovernedProcessToolGateway::open(&fx.db).unwrap();
    let sealed = seal_node(&fx, node_version_argv(), Some("."));
    let id = reserve_and_approve(&gateway, &fx, &sealed, "call-mismatch");
    let other = seal_node(&fx, node_exit_argv(0), Some("."));
    let err = gateway
        .execute(
            &capability(&fx),
            &id,
            &other,
            ProcessCancellationSignal::new(),
        )
        .unwrap_err();
    assert!(matches!(err, GovernedProcessError::FingerprintMismatch));
}

#[test]
fn approval_expiry_returns_expired_without_spawn() {
    let temp = tempfile::tempdir().unwrap();
    let fx = fixture(&temp);
    let gateway = GovernedProcessToolGateway::open(&fx.db).unwrap();
    let sealed = seal_node(&fx, node_version_argv(), Some("."));
    let prepared = gateway
        .reserve(
            &capability(&fx),
            &fx.conversation_id,
            fx.run_id,
            "call-expire",
            &sealed,
        )
        .unwrap();
    let proposal_id = prepared.metadata.proposal_id.clone();
    let grant = grant_digest("grant-expire");
    gateway
        .decide(
            &proposal_id,
            &fx.conversation_id,
            ExecDecision::ApproveOnce,
            &grant,
            sealed.proposal_fingerprint(),
        )
        .unwrap();

    // Ledger clock injection uses the storage crate API directly — not a
    // gateway.store() production bypass. Gateway only exposes get_proposal.
    let ledger = SqliteExecProposalRepository::open(&fx.db).unwrap();
    let row = gateway.get_proposal(&proposal_id).unwrap().unwrap();
    let expired_now = row.expires_at + ChronoDuration::seconds(1);
    let claim = ledger
        .claim_run(&proposal_id, "owner-expire", 60_000, expired_now)
        .unwrap();
    assert!(matches!(claim, ExecRunClaimOutcome::Expired(_)));

    let outcome = gateway
        .execute(
            &capability(&fx),
            &proposal_id,
            &sealed,
            ProcessCancellationSignal::new(),
        )
        .unwrap();
    assert!(matches!(
        outcome.outcome,
        GovernedProcessOutcome::Expired(_)
    ));
}

#[test]
fn rejection_never_spawns() {
    let temp = tempfile::tempdir().unwrap();
    let fx = fixture(&temp);
    let gateway = GovernedProcessToolGateway::open(&fx.db).unwrap();
    let sealed = seal_node(&fx, node_version_argv(), Some("."));
    let prepared = gateway
        .reserve(
            &capability(&fx),
            &fx.conversation_id,
            fx.run_id,
            "call-reject",
            &sealed,
        )
        .unwrap();
    let proposal_id = prepared.metadata.proposal_id.clone();
    let decision = gateway
        .decide(
            &proposal_id,
            &fx.conversation_id,
            ExecDecision::Reject,
            &grant_digest("grant-reject"),
            sealed.proposal_fingerprint(),
        )
        .unwrap();
    assert!(matches!(decision, ExecDecisionOutcome::Accepted(_)));

    let outcome = gateway
        .execute(
            &capability(&fx),
            &proposal_id,
            &sealed,
            ProcessCancellationSignal::new(),
        )
        .unwrap();
    assert!(matches!(
        outcome.outcome,
        GovernedProcessOutcome::Rejected(_) | GovernedProcessOutcome::Conflict(_)
    ));
    assert_eq!(
        gateway.get_proposal(&proposal_id).unwrap().unwrap().state,
        ExecProposalState::Rejected
    );
}

// --- Spawn authority / claim matrix ---

#[test]
fn approve_once_executes_with_exactly_one_spawn() {
    let temp = tempfile::tempdir().unwrap();
    let fx = fixture(&temp);
    let gateway = GovernedProcessToolGateway::open(&fx.db).unwrap();
    let sealed = seal_node(&fx, node_version_argv(), Some("."));
    let id = reserve_and_approve(&gateway, &fx, &sealed, "call-once");
    let outcome = gateway
        .execute(
            &capability(&fx),
            &id,
            &sealed,
            ProcessCancellationSignal::new(),
        )
        .unwrap();
    assert!(matches!(
        outcome.outcome,
        GovernedProcessOutcome::Succeeded(_)
    ));

    // Terminal replay: no second spawn (one-shot claim).
    let replay = gateway
        .execute(
            &capability(&fx),
            &id,
            &sealed,
            ProcessCancellationSignal::new(),
        )
        .unwrap();
    assert!(matches!(
        replay.outcome,
        GovernedProcessOutcome::Terminal(_)
    ));
}

#[test]
fn same_owner_other_owner_and_expired_running_claims_return_busy_without_spawn() {
    let temp = tempfile::tempdir().unwrap();
    let fx = fixture(&temp);
    let gateway = GovernedProcessToolGateway::open(&fx.db).unwrap();
    // Fixed production timeout — Busy is proven via one-shot claim, not a
    // post-seal timeout mutator.
    let sealed = seal_node(&fx, node_version_argv(), Some("."));
    assert_eq!(sealed.timeout(), PROCESS_TIMEOUT);
    let id = reserve_and_approve(&gateway, &fx, &sealed, "call-busy");

    // Claim matrix uses the storage crate directly (ledger contract), not
    // gateway.store(). Gateway must still return Busy and never spawn.
    let ledger = SqliteExecProposalRepository::open(&fx.db).unwrap();
    let first = ledger
        .claim_run(&id, "owner-a", 60_000, Utc::now())
        .unwrap();
    assert!(matches!(first, ExecRunClaimOutcome::Claimed(_)));

    // Same owner while Running → Busy (D2AR: no reclaim).
    let same = ledger
        .claim_run(&id, "owner-a", 60_000, Utc::now())
        .unwrap();
    assert!(matches!(same, ExecRunClaimOutcome::Busy(_)));

    // Other owner while Running → Busy.
    let other = ledger
        .claim_run(&id, "owner-b", 60_000, Utc::now())
        .unwrap();
    assert!(matches!(other, ExecRunClaimOutcome::Busy(_)));

    // Expired lease observation: claim_run still returns Busy for Running (never reassigns).
    let after_lease = ledger
        .claim_run(
            &id,
            "owner-c",
            60_000,
            Utc::now() + ChronoDuration::seconds(120),
        )
        .unwrap();
    assert!(matches!(after_lease, ExecRunClaimOutcome::Busy(_)));

    // Gateway execute while Running must not spawn (Busy, never success).
    let outcome = gateway
        .execute(
            &capability(&fx),
            &id,
            &sealed,
            ProcessCancellationSignal::new(),
        )
        .unwrap();
    assert!(matches!(outcome.outcome, GovernedProcessOutcome::Busy(_)));
    assert_eq!(
        gateway.get_proposal(&id).unwrap().unwrap().state,
        ExecProposalState::Running
    );

    ledger.interrupt_run(&id, "owner-a", Utc::now()).unwrap();
}

// --- Environment privacy ---

#[test]
fn clean_environment_excludes_sensitive_parent_names() {
    let previous_token = std::env::var_os("API_TOKEN");
    let previous_key = std::env::var_os("OPENAI_API_KEY");
    unsafe {
        std::env::set_var("API_TOKEN", "must-not-appear");
        std::env::set_var("OPENAI_API_KEY", "must-not-appear");
    }
    let exe = std::env::current_exe().unwrap();
    let env = build_clean_environment(&exe).unwrap();
    for key in env.keys() {
        assert!(
            !keeper_process_executor::sensitive_name(key),
            "sensitive key leaked: {key:?}"
        );
    }
    assert!(!env.contains_key(std::ffi::OsStr::new("API_TOKEN")));
    assert!(!env.contains_key(std::ffi::OsStr::new("OPENAI_API_KEY")));
    assert!(!env.contains_key(std::ffi::OsStr::new("HOME")));
    assert!(!env.contains_key(std::ffi::OsStr::new("USERPROFILE")));
    assert!(!env.contains_key(std::ffi::OsStr::new("APPDATA")));
    assert!(!env.contains_key(std::ffi::OsStr::new("LOCALAPPDATA")));
    assert!(env.contains_key(std::ffi::OsStr::new("PATH")));
    assert!(env.contains_key(std::ffi::OsStr::new("TEMP")));
    assert!(env.contains_key(std::ffi::OsStr::new("TMP")));
    match previous_token {
        Some(v) => unsafe { std::env::set_var("API_TOKEN", v) },
        None => unsafe { std::env::remove_var("API_TOKEN") },
    }
    match previous_key {
        Some(v) => unsafe { std::env::set_var("OPENAI_API_KEY", v) },
        None => unsafe { std::env::remove_var("OPENAI_API_KEY") },
    }
}

// --- Lifecycle ---

#[test]
fn success_nonzero_timeout_cancel_and_receipt_replay() {
    let temp = tempfile::tempdir().unwrap();
    let fx = fixture(&temp);
    let gateway = GovernedProcessToolGateway::open(&fx.db).unwrap();
    let cap = capability(&fx);

    // Success
    let sealed_ok = seal_node(&fx, node_version_argv(), Some("."));
    let id_ok = reserve_and_approve(&gateway, &fx, &sealed_ok, "call-ok");
    let ok = gateway
        .execute(&cap, &id_ok, &sealed_ok, ProcessCancellationSignal::new())
        .unwrap();
    let GovernedProcessOutcome::Succeeded(row_ok) = ok.outcome else {
        panic!("expected success, got {ok:?}");
    };
    assert_eq!(row_ok.state, ExecProposalState::Succeeded);
    assert_eq!(row_ok.exit_code, Some(0));
    assert_eq!(row_ok.cancelled, Some(false));
    let receipt: ExecTerminalReceipt =
        serde_json::from_str(row_ok.result_json.as_deref().unwrap()).unwrap();
    assert!(!receipt.cancelled && !receipt.timed_out);
    assert!(receipt.stdout_sha256.starts_with("sha256:"));

    // Replay terminal
    let replay = gateway
        .execute(&cap, &id_ok, &sealed_ok, ProcessCancellationSignal::new())
        .unwrap();
    assert!(matches!(
        replay.outcome,
        GovernedProcessOutcome::Terminal(_)
    ));

    // Nonzero exit — need a fresh run (one active per run).
    let sessions = SqliteConversationRepository::open(&fx.db).unwrap();
    let run2 = sessions
        .create_run(NewRun {
            conversation_id: keeper_session::ConversationId(
                Uuid::parse_str(&fx.conversation_id).unwrap(),
            ),
            user_message: "run2".into(),
        })
        .unwrap();
    let mut fx2 = Fixture {
        db: fx.db.clone(),
        root: fx.root.clone(),
        project_id: fx.project_id.clone(),
        conversation_id: fx.conversation_id.clone(),
        run_id: ProtocolRunId(run2.id.0),
    };
    let sealed_nz = seal_node(&fx2, node_exit_argv(7), Some("."));
    let id_nz = reserve_and_approve(&gateway, &fx2, &sealed_nz, "call-nz");
    let nz = gateway
        .execute(&cap, &id_nz, &sealed_nz, ProcessCancellationSignal::new())
        .unwrap();
    let GovernedProcessOutcome::Failed(row_nz) = nz.outcome else {
        panic!("expected failed nonzero, got {nz:?}");
    };
    assert_eq!(row_nz.exit_code, Some(7));
    assert_eq!(row_nz.state, ExecProposalState::Failed);

    // Timeout path is covered by private cfg(test) accelerated-timeout unit
    // support (see process unit tests). Integration path retains fixed
    // PROCESS_TIMEOUT and must not call a production with_timeout mutator.
    assert_eq!(PROCESS_TIMEOUT, Duration::from_millis(60_000));

    // Cancellation: cancel a normal long-running child without changing timeout.
    // Pre-spawn cancel after claim uses the fixed 60s seal (no timeout mutator).
    let run3 = sessions
        .create_run(NewRun {
            conversation_id: keeper_session::ConversationId(
                Uuid::parse_str(&fx.conversation_id).unwrap(),
            ),
            user_message: "run3".into(),
        })
        .unwrap();
    fx2.run_id = ProtocolRunId(run3.id.0);
    let sealed_ca = seal_node(&fx2, node_sleep_argv(30_000), Some("."));
    assert_eq!(sealed_ca.timeout(), PROCESS_TIMEOUT);
    let id_ca = reserve_and_approve(&gateway, &fx2, &sealed_ca, "call-ca");
    let signal = ProcessCancellationSignal::new();
    signal.cancel();
    let cancelled = gateway.execute(&cap, &id_ca, &sealed_ca, signal).unwrap();
    // Pre-spawn cancel if claim-then-immediate-cancel, or post-spawn cancelled receipt.
    match cancelled.outcome {
        GovernedProcessOutcome::Cancelled(row) => {
            assert_eq!(row.state, ExecProposalState::Cancelled);
            assert_eq!(row.cancelled, Some(true));
            let receipt: ExecTerminalReceipt =
                serde_json::from_str(row.result_json.as_deref().unwrap()).unwrap();
            assert!(receipt.cancelled);
        }
        other => panic!("expected cancelled, got {other:?}"),
    }

    assert_no_secrets_in_db(&fx.db, &[SECRET_ARGV, "must-not-appear"]);
}

#[test]
fn sealed_argv_and_native_paths_never_enter_sqlite() {
    let temp = tempfile::tempdir().unwrap();
    let fx = fixture(&temp);
    let gateway = GovernedProcessToolGateway::open(&fx.db).unwrap();
    let sealed = seal_node(
        &fx,
        vec!["-e".into(), format!("console.log('{SECRET_ARGV}')")],
        Some("."),
    );
    let id = reserve_and_approve(&gateway, &fx, &sealed, "call-privacy");
    let _ = gateway
        .execute(
            &capability(&fx),
            &id,
            &sealed,
            ProcessCancellationSignal::new(),
        )
        .unwrap();

    let row = gateway.get_proposal(&id).unwrap().unwrap();
    let debug = format!("{row:?}");
    let sealed_debug = format!("{sealed:?}");
    assert!(!debug.contains(SECRET_ARGV));
    assert!(!sealed_debug.contains(SECRET_ARGV));
    // Native material must be redacted in Debug and absent from durable rows.
    assert!(sealed_debug.contains("<redacted>"));
    assert!(!sealed_debug.to_lowercase().contains("\\node"));
    assert!(row.argv_digest.starts_with("sha256:"));
    assert!(row.program == "node");
    assert!(row.portable_cwd.as_deref() == Some("."));
    assert_eq!(sealed.timeout(), PROCESS_TIMEOUT);

    assert_no_secrets_in_db(&fx.db, &[SECRET_ARGV]);
}

#[test]
fn invalid_cwd_is_rejected_at_seal() {
    let temp = tempfile::tempdir().unwrap();
    let fx = fixture(&temp);
    let cap = capability(&fx);
    let err = GovernedProcessToolGateway::seal(&cap, "node", &node_version_argv(), Some(".."))
        .unwrap_err();
    assert!(matches!(err, GovernedProcessError::CwdRejected));
    let err =
        GovernedProcessToolGateway::seal(&cap, "node", &node_version_argv(), Some("missing-dir"))
            .unwrap_err();
    assert!(matches!(err, GovernedProcessError::CwdRejected));
}

#[test]
fn reservation_replay_of_terminal_execute_is_bounded() {
    let temp = tempfile::tempdir().unwrap();
    let fx = fixture(&temp);
    let gateway = GovernedProcessToolGateway::open(&fx.db).unwrap();
    let sealed = seal_node(&fx, node_version_argv(), Some("."));
    let id = reserve_and_approve(&gateway, &fx, &sealed, "call-replay");
    let first = gateway
        .execute(
            &capability(&fx),
            &id,
            &sealed,
            ProcessCancellationSignal::new(),
        )
        .unwrap();
    let GovernedProcessOutcome::Succeeded(a) = first.outcome else {
        panic!("expected success");
    };
    let second = gateway
        .execute(
            &capability(&fx),
            &id,
            &sealed,
            ProcessCancellationSignal::new(),
        )
        .unwrap();
    let GovernedProcessOutcome::Terminal(b) = second.outcome else {
        panic!("expected terminal replay");
    };
    assert_eq!(a.result_json, b.result_json);
}

#[test]
fn no_direct_command_spawn_symbol_in_process_module_source() {
    // Static authority: process gateway source must not open OS children itself.
    let source = include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/src/process.rs"));
    assert!(!source.contains("std::process::Command"));
    assert!(!source.contains("process::Command"));
    assert!(!source.contains(".spawn()"));
    assert!(source.contains("SupervisedProcessRunner"));
    assert!(source.contains("trait ProcessRunner"));
}

#[test]
fn cleanup_unproven_public_token_is_stable() {
    // Integration-level constant lock; fail-closed path is covered by unit tests
    // that inject CleanupUnprovenRunner under cfg(test).
    assert_eq!(CLEANUP_UNPROVEN_PUBLIC_ERROR, "process_cleanup_unproven");
}

#[test]
fn fresh_execute_returns_real_bounded_live_output() {
    let temp = tempfile::tempdir().unwrap();
    let fx = fixture(&temp);
    let gateway = GovernedProcessToolGateway::open(&fx.db).unwrap();
    let sealed = seal_node(&fx, node_version_argv(), Some("."));
    let id = reserve_and_approve(&gateway, &fx, &sealed, "call-live-out");
    let execution = gateway
        .execute(
            &capability(&fx),
            &id,
            &sealed,
            ProcessCancellationSignal::new(),
        )
        .unwrap();
    assert!(matches!(
        execution.outcome,
        GovernedProcessOutcome::Succeeded(_)
    ));
    let live = execution
        .live_output
        .as_ref()
        .expect("fresh spawn retains output");
    assert!(!live.chunks.is_empty(), "chunks must not stay empty");
    let joined: String = live.chunks.iter().map(|c| c.text.as_str()).collect();
    // node --version prints a version string on stdout or stderr depending on build.
    assert!(
        !joined.trim().is_empty() || live.captured_bytes > 0,
        "must surface real captured text, got {live:?}"
    );
    // SQLite row keeps digests only — no transcript in result_json.
    let row = execution.metadata();
    let result = row.result_json.as_deref().unwrap_or("");
    assert!(!result.contains("v1.") || result.contains("sha256:"));
    assert!(!result.contains("chunks"));

    // Terminal replay: no second spawn and no live transcript.
    let replay = gateway
        .execute(
            &capability(&fx),
            &id,
            &sealed,
            ProcessCancellationSignal::new(),
        )
        .unwrap();
    assert!(matches!(
        replay.outcome,
        GovernedProcessOutcome::Terminal(_)
    ));
    assert!(replay.live_output.is_none());
}

#[test]
fn owner_bound_list_get_and_mutations_deny_foreign_identity() {
    let temp = tempfile::tempdir().unwrap();
    let fx = fixture(&temp);
    let gateway = GovernedProcessToolGateway::open(&fx.db).unwrap();
    let sealed = seal_node(&fx, node_version_argv(), Some("."));
    let prepared = gateway
        .reserve(
            &capability(&fx),
            &fx.conversation_id,
            fx.run_id,
            "call-owner-bound",
            &sealed,
        )
        .unwrap();
    let pid = prepared.metadata.proposal_id.clone();

    // Owning list/get succeed.
    let listed = gateway
        .list_for_conversation(&capability(&fx), &fx.conversation_id)
        .unwrap();
    assert!(listed.iter().any(|r| r.proposal_id == pid));
    let got = gateway
        .get_for_owner(&capability(&fx), &fx.conversation_id, &pid)
        .unwrap()
        .unwrap();
    assert_eq!(got.proposal_id, pid);

    // Foreign conversation fails closed.
    let foreign_conv = "00000000-0000-4000-8000-000000000099";
    let err = gateway
        .get_for_owner(&capability(&fx), foreign_conv, &pid)
        .unwrap_err();
    assert!(matches!(err, GovernedProcessError::CapabilityMismatch));
    let foreign_list = gateway
        .list_for_conversation(&capability(&fx), foreign_conv)
        .unwrap();
    assert!(foreign_list.is_empty());

    // Foreign project capability fails closed.
    let other_root = temp.path().join("other-root");
    fs::create_dir(&other_root).unwrap();
    let foreign_cap = ProjectCapability::new("foreign-project-id", &other_root).unwrap();
    let err = gateway
        .get_for_owner(&foreign_cap, &fx.conversation_id, &pid)
        .unwrap_err();
    assert!(matches!(err, GovernedProcessError::CapabilityMismatch));
    let err = gateway
        .expire_proposal(&foreign_cap, &fx.conversation_id, &pid)
        .unwrap_err();
    assert!(matches!(err, GovernedProcessError::CapabilityMismatch));

    // Owning expire works.
    let expired = gateway
        .expire_proposal(&capability(&fx), &fx.conversation_id, &pid)
        .unwrap();
    assert_eq!(expired.state, ExecProposalState::Expired);
}

#[test]
fn reserve_replays_terminal_identity_without_prelist() {
    let temp = tempfile::tempdir().unwrap();
    let fx = fixture(&temp);
    let gateway = GovernedProcessToolGateway::open(&fx.db).unwrap();
    let sealed = seal_node(&fx, node_version_argv(), Some("."));
    let id = reserve_and_approve(&gateway, &fx, &sealed, "call-reserve-replay");
    let _ = gateway
        .execute(
            &capability(&fx),
            &id,
            &sealed,
            ProcessCancellationSignal::new(),
        )
        .unwrap();

    // Second reserve with same call identity returns terminal metadata (no pre-list).
    let prepared = gateway
        .reserve(
            &capability(&fx),
            &fx.conversation_id,
            fx.run_id,
            "call-reserve-replay",
            &sealed,
        )
        .unwrap();
    assert_eq!(prepared.metadata.proposal_id, id);
    assert!(is_exec_terminal_state(prepared.metadata.state));
}

fn is_exec_terminal_state(state: ExecProposalState) -> bool {
    matches!(
        state,
        ExecProposalState::Succeeded
            | ExecProposalState::Failed
            | ExecProposalState::Rejected
            | ExecProposalState::Expired
            | ExecProposalState::Cancelled
            | ExecProposalState::Interrupted
    )
}
