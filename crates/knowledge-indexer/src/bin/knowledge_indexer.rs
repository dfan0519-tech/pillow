//! CLI: scan docs/, register into a SQLite knowledge DB, print audit / handoff
//! views, and (K0e) record dispatch/acceptance ledger rows + stale checks.
//!
//! Does **not** modify anything under `docs/` except optional output paths the
//! caller explicitly passes via `--report` / `--handoff`. Writes the database
//! path and those flag paths only. **Never rewrites handoff documents.**
//!
//! All subcommands are non-interactive (no stdin prompts).

use std::env;
use std::fs;
use std::path::PathBuf;
use std::process::ExitCode;

use keeper_knowledge_indexer::{
    CompleteInput, DispatchInput, LedgerDbOptions, ScanOptions, VerdictInput, check_stale,
    dispatch_task, format_ledger_status, format_stale_report, format_task_recorded,
    format_verdict_recorded, load_snapshot, record_verdict, scan_and_register, set_task_status,
};
use keeper_storage_sqlite::{RegisteredBy, TaskStatus, VerdictOutcome};

fn main() -> ExitCode {
    let args: Vec<String> = env::args().collect();
    if args.iter().any(|a| a == "-h" || a == "--help") {
        print_help();
        return ExitCode::SUCCESS;
    }

    // Subcommand is the first non-flag token after argv[0].
    let sub = first_subcommand(&args);
    match sub.as_deref() {
        None | Some("scan") => run_scan(&args),
        Some("task") => run_task(&args),
        Some("verdict") => run_verdict(&args),
        Some("ledger") => run_ledger(&args),
        Some(other) => {
            eprintln!("unknown subcommand: {other}");
            eprintln!("try: knowledge-indexer --help");
            ExitCode::from(1)
        }
    }
}

fn first_subcommand(args: &[String]) -> Option<String> {
    // Skip argv[0]; first token that does not start with `-` and is not a flag value.
    let mut i = 1;
    while i < args.len() {
        let a = &args[i];
        if a == "-h" || a == "--help" {
            i += 1;
            continue;
        }
        if a.starts_with("--") {
            // skip flag and its value unless --flag=value
            if a.contains('=') {
                i += 1;
            } else {
                i += 2;
            }
            continue;
        }
        if a.starts_with('-') {
            i += 1;
            continue;
        }
        return Some(a.clone());
    }
    None
}

// ---------------------------------------------------------------------------
// scan (default / K0b–K0d)
// ---------------------------------------------------------------------------

fn run_scan(args: &[String]) -> ExitCode {
    let repo_root = flag_value(args, "--repo")
        .map(PathBuf::from)
        .unwrap_or_else(|| env::current_dir().unwrap_or_else(|_| PathBuf::from(".")));
    let db_path = flag_value(args, "--db")
        .map(PathBuf::from)
        .unwrap_or_else(|| std::env::temp_dir().join("pillow-knowledge-k0c.db"));
    let report_path = flag_value(args, "--report").map(PathBuf::from);
    let handoff_path = flag_value(args, "--handoff").map(PathBuf::from);
    let project_name = flag_value(args, "--project").unwrap_or_else(|| "pillow".into());

    let mut exclude_paths = vec![
        "docs/research/2026-08-08-knowledge-audit-run.md".to_owned(),
        "docs/execution/2026-08-08-knowledge-indexer-k0b-report.md".to_owned(),
        "docs/research/2026-08-08-handoff-rendered-sample.md".to_owned(),
        "docs/execution/2026-08-09-k0c-render-report.md".to_owned(),
        "docs/execution/2026-08-09-k0d-ledger-report.md".to_owned(),
        "docs/execution/2026-08-09-k0e-ledger-cli-report.md".to_owned(),
        "docs/execution/2026-08-09-k1a-retirement-report.md".to_owned(),
        "docs/execution/2026-08-09-k1b-handoff-render-report.md".to_owned(),
        "docs/research/2026-08-09-handoff-renderability-matrix.md".to_owned(),
    ];
    for rp in [&report_path, &handoff_path].into_iter().flatten() {
        let rp_str = rp.to_string_lossy().replace('\\', "/");
        if let Some(idx) = rp_str.find("docs/") {
            exclude_paths.push(rp_str[idx..].to_owned());
        }
    }
    exclude_paths.sort();
    exclude_paths.dedup();

    let options = ScanOptions {
        repo_root,
        db_path: db_path.clone(),
        project_name,
        registered_by: RegisteredBy::Lint,
        source_run_id: Some("k0e-scan".into()),
        docs_relative: "docs".into(),
        exclude_paths,
    };

    match scan_and_register(&options) {
        Ok(result) => {
            eprintln!(
                "scan ok: files={} new={} updated={} links_added={} digest_changed={}",
                result.summary.files_seen,
                result.summary.registered_new,
                result.summary.updated_existing,
                result.summary.links_added,
                result.summary.digest_changed
            );
            eprintln!(
                "registered={} unknown_ratio={:.1}% roles={:?} status={:?}",
                result.report.total_registered,
                result.report.unknown_ratio * 100.0,
                result.report.role_counts,
                result.report.status_counts
            );
            eprintln!("db={}", db_path.display());

            let wrote_report = if let Some(ref path) = report_path {
                if let Some(parent) = path.parent() {
                    let _ = fs::create_dir_all(parent);
                }
                if let Err(err) = fs::write(path, &result.report_markdown) {
                    eprintln!("failed to write report {}: {err}", path.display());
                    return ExitCode::from(2);
                }
                eprintln!("report={}", path.display());
                true
            } else {
                false
            };

            let wrote_handoff = if let Some(ref path) = handoff_path {
                if let Some(parent) = path.parent() {
                    let _ = fs::create_dir_all(parent);
                }
                if let Err(err) = fs::write(path, &result.handoff_markdown) {
                    eprintln!("failed to write handoff view {}: {err}", path.display());
                    return ExitCode::from(2);
                }
                eprintln!("handoff={}", path.display());
                true
            } else {
                false
            };

            if !wrote_report && !wrote_handoff {
                println!("{}", result.handoff_markdown);
            }

            ExitCode::SUCCESS
        }
        Err(err) => {
            eprintln!("scan failed: {err}");
            ExitCode::from(1)
        }
    }
}

// ---------------------------------------------------------------------------
// task dispatch | task complete
// ---------------------------------------------------------------------------

fn run_task(args: &[String]) -> ExitCode {
    let action = nth_subcommand(args, 1);
    match action.as_deref() {
        Some("dispatch") => task_dispatch(args),
        Some("complete") => task_complete(args),
        Some(other) => {
            eprintln!("unknown task subcommand: {other}");
            eprintln!("use: task dispatch | task complete");
            ExitCode::from(1)
        }
        None => {
            eprintln!("missing task subcommand (dispatch|complete)");
            ExitCode::from(1)
        }
    }
}

fn task_dispatch(args: &[String]) -> ExitCode {
    let opts = match ledger_opts(args, true) {
        Ok(o) => o,
        Err(code) => return code,
    };
    let title = match require_flag(args, "--title") {
        Ok(v) => v,
        Err(code) => return code,
    };
    let worker_label = match flag_value(args, "--slot")
        .or_else(|| flag_value(args, "--worker-label"))
        .filter(|s| !s.is_empty())
    {
        Some(v) => v,
        None => {
            eprintln!("missing required --slot <label> (slot label only, e.g. grok3)");
            return ExitCode::from(1);
        }
    };
    let brief_path = flag_value(args, "--brief");

    match dispatch_task(
        &opts,
        DispatchInput {
            title,
            brief_path,
            worker_label,
        },
    ) {
        Ok(task) => {
            println!("{}", format_task_recorded(&task, "dispatched"));
            ExitCode::SUCCESS
        }
        Err(err) => {
            eprintln!("task dispatch failed: {err}");
            ExitCode::from(1)
        }
    }
}

fn task_complete(args: &[String]) -> ExitCode {
    let opts = match ledger_opts(args, false) {
        Ok(o) => o,
        Err(code) => return code,
    };
    let task_id = match require_flag(args, "--task-id") {
        Ok(v) => v,
        Err(code) => return code,
    };
    let status_raw = flag_value(args, "--status").unwrap_or_else(|| "completed".into());
    let status = match TaskStatus::parse(&status_raw) {
        Ok(s) => s,
        Err(err) => {
            eprintln!("invalid --status: {err}");
            return ExitCode::from(1);
        }
    };
    // Prefer --signal; also accept --completion-signal.
    let completion_signal =
        flag_value(args, "--signal").or_else(|| flag_value(args, "--completion-signal"));

    match set_task_status(
        &opts,
        CompleteInput {
            task_id,
            status,
            completion_signal,
        },
    ) {
        Ok(task) => {
            println!("{}", format_task_recorded(&task, "status_set"));
            ExitCode::SUCCESS
        }
        Err(err) => {
            eprintln!("task complete failed: {err}");
            ExitCode::from(1)
        }
    }
}

// ---------------------------------------------------------------------------
// verdict record
// ---------------------------------------------------------------------------

fn run_verdict(args: &[String]) -> ExitCode {
    let action = nth_subcommand(args, 1);
    match action.as_deref() {
        Some("record") => verdict_record(args),
        Some(other) => {
            eprintln!("unknown verdict subcommand: {other}");
            eprintln!("use: verdict record");
            ExitCode::from(1)
        }
        None => {
            eprintln!("missing verdict subcommand (record)");
            ExitCode::from(1)
        }
    }
}

fn verdict_record(args: &[String]) -> ExitCode {
    let opts = match ledger_opts(args, true) {
        Ok(o) => o,
        Err(code) => return code,
    };
    let outcome_raw = match require_flag(args, "--outcome") {
        Ok(v) => v,
        Err(code) => return code,
    };
    let outcome = match VerdictOutcome::parse(&outcome_raw) {
        Ok(o) => o,
        Err(err) => {
            eprintln!("invalid --outcome: {err}");
            return ExitCode::from(1);
        }
    };
    let summary = match require_flag(args, "--summary") {
        Ok(v) => v,
        Err(code) => return code,
    };
    let task_id = flag_value(args, "--task-id");
    let gate_result = flag_value(args, "--gate");
    let e2e_result = flag_value(args, "--e2e");
    let residuals = flag_value(args, "--residuals");

    match record_verdict(
        &opts,
        VerdictInput {
            task_id,
            outcome,
            gate_result,
            e2e_result,
            summary,
            residuals,
        },
    ) {
        Ok(v) => {
            println!("{}", format_verdict_recorded(&v));
            ExitCode::SUCCESS
        }
        Err(err) => {
            eprintln!("verdict record failed: {err}");
            ExitCode::from(1)
        }
    }
}

// ---------------------------------------------------------------------------
// ledger status | ledger stale
// ---------------------------------------------------------------------------

fn run_ledger(args: &[String]) -> ExitCode {
    let action = nth_subcommand(args, 1);
    match action.as_deref() {
        Some("status") => ledger_status(args),
        Some("stale") => ledger_stale(args),
        Some(other) => {
            eprintln!("unknown ledger subcommand: {other}");
            eprintln!("use: ledger status | ledger stale");
            ExitCode::from(1)
        }
        None => {
            eprintln!("missing ledger subcommand (status|stale)");
            ExitCode::from(1)
        }
    }
}

fn ledger_status(args: &[String]) -> ExitCode {
    let opts = match ledger_opts(args, false) {
        Ok(o) => o,
        Err(code) => return code,
    };
    match load_snapshot(&opts) {
        Ok(snap) => {
            print!("{}", format_ledger_status(&snap));
            ExitCode::SUCCESS
        }
        Err(err) => {
            eprintln!("ledger status failed: {err}");
            ExitCode::from(1)
        }
    }
}

fn ledger_stale(args: &[String]) -> ExitCode {
    let opts = match ledger_opts(args, false) {
        Ok(o) => o,
        Err(code) => return code,
    };
    let doc_path = match flag_value(args, "--doc")
        .or_else(|| flag_value(args, "--handoff-doc"))
        .filter(|s| !s.is_empty())
    {
        Some(v) => PathBuf::from(v),
        None => {
            eprintln!("missing required --doc <path> (handoff markdown to check)");
            return ExitCode::from(1);
        }
    };

    let doc_text = match fs::read_to_string(&doc_path) {
        Ok(t) => t,
        Err(err) => {
            eprintln!("failed to read {}: {err}", doc_path.display());
            return ExitCode::from(1);
        }
    };

    let snap = match load_snapshot(&opts) {
        Ok(s) => s,
        Err(err) => {
            eprintln!("ledger stale failed (load snapshot): {err}");
            return ExitCode::from(1);
        }
    };

    let report = check_stale(&doc_text, &snap);
    let label = doc_path.to_string_lossy().replace('\\', "/");
    print!("{}", format_stale_report(&report, &label));

    if report.is_stale() {
        // Exit 3 = stale findings (scriptable; distinct from I/O errors).
        ExitCode::from(3)
    } else {
        ExitCode::SUCCESS
    }
}

// ---------------------------------------------------------------------------
// Shared flag / opts helpers
// ---------------------------------------------------------------------------

fn ledger_opts(args: &[String], _create_ok: bool) -> Result<LedgerDbOptions, ExitCode> {
    let db_path = PathBuf::from(require_flag(args, "--db")?);
    let project_name = flag_value(args, "--project").unwrap_or_else(|| "pillow".into());
    let project_root = flag_value(args, "--repo")
        .or_else(|| flag_value(args, "--project-root"))
        .map(PathBuf::from)
        .or_else(|| env::current_dir().ok());
    Ok(LedgerDbOptions {
        db_path,
        project_name,
        project_root,
    })
}

fn nth_subcommand(args: &[String], n: usize) -> Option<String> {
    // Collect non-flag positional tokens after argv[0].
    let mut pos = Vec::new();
    let mut i = 1;
    while i < args.len() {
        let a = &args[i];
        if a == "-h" || a == "--help" {
            i += 1;
            continue;
        }
        if a.starts_with("--") {
            if a.contains('=') {
                i += 1;
            } else {
                i += 2;
            }
            continue;
        }
        if a.starts_with('-') {
            i += 1;
            continue;
        }
        pos.push(a.clone());
        i += 1;
    }
    pos.get(n).cloned()
}

fn flag_value(args: &[String], name: &str) -> Option<String> {
    let mut i = 0;
    while i < args.len() {
        if args[i] == name {
            return args.get(i + 1).cloned();
        }
        if let Some(rest) = args[i].strip_prefix(&format!("{name}=")) {
            return Some(rest.to_owned());
        }
        i += 1;
    }
    None
}

fn require_flag(args: &[String], name: &str) -> Result<String, ExitCode> {
    match flag_value(args, name) {
        Some(v) if !v.is_empty() => Ok(v),
        _ => {
            eprintln!("missing required {name}");
            Err(ExitCode::from(1))
        }
    }
}

fn print_help() {
    eprintln!(
        "usage:\n\
         knowledge-indexer [scan] [--repo DIR] [--db PATH] [--report PATH] [--handoff PATH] [--project NAME]\n\
         knowledge-indexer task dispatch --db PATH --project NAME --title TITLE --slot LABEL [--brief PATH] [--repo DIR]\n\
         knowledge-indexer task complete --db PATH --task-id ID [--status completed|blocked|abandoned] [--signal TEXT]\n\
         knowledge-indexer verdict record --db PATH --project NAME --outcome accepted|rejected|partial --summary TEXT [--task-id ID] [--gate TEXT] [--e2e TEXT] [--residuals TEXT]\n\
         knowledge-indexer ledger status --db PATH --project NAME\n\
         knowledge-indexer ledger stale --db PATH --project NAME --doc PATH\n\
         \n\
         All subcommands are non-interactive (no stdin). Slot labels only — never lease/session/host paths.\n\
         ledger stale exit: 0=fresh, 3=stale findings, 1=error. Never rewrites documents.\n\
         scan: never modifies docs/ except explicit --report/--handoff output paths."
    );
}
