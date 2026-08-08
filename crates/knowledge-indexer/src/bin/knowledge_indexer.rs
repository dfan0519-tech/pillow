//! CLI: scan docs/, register into a SQLite knowledge DB, print audit report.
//!
//! Does **not** modify anything under `docs/`. Writes only the database path
//! and optional report file paths provided by flags.

use std::env;
use std::fs;
use std::path::PathBuf;
use std::process::ExitCode;

use keeper_knowledge_indexer::{ScanOptions, scan_and_register};
use keeper_storage_sqlite::RegisteredBy;

fn main() -> ExitCode {
    let args: Vec<String> = env::args().collect();
    if args.iter().any(|a| a == "-h" || a == "--help") {
        print_help();
        return ExitCode::SUCCESS;
    }

    let repo_root = flag_value(&args, "--repo")
        .map(PathBuf::from)
        .unwrap_or_else(|| env::current_dir().unwrap_or_else(|_| PathBuf::from(".")));
    let db_path = flag_value(&args, "--db")
        .map(PathBuf::from)
        .unwrap_or_else(|| std::env::temp_dir().join("pillow-knowledge-k0b.db"));
    let report_path = flag_value(&args, "--report").map(PathBuf::from);
    let project_name = flag_value(&args, "--project").unwrap_or_else(|| "pillow".into());

    // Exclude this run's deliverable paths (and known K0b outputs) so a prior
    // audit report cannot self-cite every basename and zero the orphan list.
    let mut exclude_paths = vec![
        "docs/research/2026-08-08-knowledge-audit-run.md".to_owned(),
        "docs/execution/2026-08-08-knowledge-indexer-k0b-report.md".to_owned(),
    ];
    if let Some(ref rp) = report_path {
        // Also exclude whatever path we're about to overwrite, if under docs/.
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
        source_run_id: Some("k0b-scan".into()),
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

            if let Some(path) = report_path {
                if let Some(parent) = path.parent() {
                    let _ = fs::create_dir_all(parent);
                }
                if let Err(err) = fs::write(&path, &result.report_markdown) {
                    eprintln!("failed to write report {}: {err}", path.display());
                    return ExitCode::from(2);
                }
                eprintln!("report={}", path.display());
            } else {
                println!("{}", result.report_markdown);
            }
            ExitCode::SUCCESS
        }
        Err(err) => {
            eprintln!("scan failed: {err}");
            ExitCode::from(1)
        }
    }
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

fn print_help() {
    eprintln!(
        "usage: knowledge-indexer [--repo DIR] [--db PATH] [--report PATH] [--project NAME]\n\
         \n\
         Scan DIR/docs/**/*.md, upsert into knowledge tables at --db (temp default),\n\
         write audit markdown to --report or stdout. Never modifies docs/."
    );
}
