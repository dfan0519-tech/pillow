//! Cross-layer portable-path contract: same case table as TypeScript.
//!
//! Source of cases: `tests/fixtures/portable-path-cases.json`.
//! If Rust and TS diverge on accept/reject, this test or the vitest twin fails.

use std::fs;
use std::path::PathBuf;

use keeper_git_executor::{is_portable_entry_path, is_portable_path};
use serde::Deserialize;

#[derive(Debug, Deserialize)]
struct CaseTable {
    #[serde(rename = "fileForm")]
    file_form: FormCases,
    #[serde(rename = "entryForm")]
    entry_form: FormCases,
}

#[derive(Debug, Deserialize)]
struct FormCases {
    accept: Vec<Case>,
    reject: Vec<Case>,
}

#[derive(Debug, Deserialize)]
struct Case {
    path: String,
    #[allow(dead_code)]
    note: Option<String>,
}

fn fixture_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/portable-path-cases.json")
}

fn load_cases() -> CaseTable {
    let raw = fs::read_to_string(fixture_path()).expect("read portable-path-cases.json");
    serde_json::from_str(&raw).expect("parse portable-path-cases.json")
}

#[test]
fn file_form_matches_shared_fixture() {
    let table = load_cases();
    assert!(
        !table.file_form.accept.is_empty() && !table.file_form.reject.is_empty(),
        "fileForm cases must be non-empty"
    );
    for case in &table.file_form.accept {
        assert!(
            is_portable_path(&case.path),
            "fileForm must accept {:?} ({:?})",
            case.path,
            case.note
        );
    }
    for case in &table.file_form.reject {
        assert!(
            !is_portable_path(&case.path),
            "fileForm must reject {:?} ({:?})",
            case.path,
            case.note
        );
    }
}

#[test]
fn entry_form_matches_shared_fixture() {
    let table = load_cases();
    assert!(
        !table.entry_form.accept.is_empty() && !table.entry_form.reject.is_empty(),
        "entryForm cases must be non-empty"
    );
    for case in &table.entry_form.accept {
        assert!(
            is_portable_entry_path(&case.path),
            "entryForm must accept {:?} ({:?})",
            case.path,
            case.note
        );
    }
    for case in &table.entry_form.reject {
        assert!(
            !is_portable_entry_path(&case.path),
            "entryForm must reject {:?} ({:?})",
            case.path,
            case.note
        );
    }
}

#[test]
fn directory_form_is_entry_only() {
    assert!(!is_portable_path("vendor/"));
    assert!(is_portable_entry_path("vendor/"));
}

#[test]
fn shared_fixture_covers_required_minimum() {
    let table = load_cases();
    let file_accept: Vec<&str> = table
        .file_form
        .accept
        .iter()
        .map(|c| c.path.as_str())
        .collect();
    let file_reject: Vec<&str> = table
        .file_form
        .reject
        .iter()
        .map(|c| c.path.as_str())
        .collect();
    let entry_accept: Vec<&str> = table
        .entry_form
        .accept
        .iter()
        .map(|c| c.path.as_str())
        .collect();

    for required in ["a.txt", "src/lib.rs", "."] {
        assert!(
            file_accept.contains(&required),
            "fileForm.accept missing {required}"
        );
    }
    assert!(
        entry_accept.contains(&"vendor/"),
        "entryForm.accept missing vendor/"
    );
    for required in [
        "/",
        "//",
        "vendor//",
        "./",
        "../escape",
        "../escape/",
        "vendor/../escape/",
        "C:/x",
        "a\\b",
        "/absolute",
        "CON",
    ] {
        assert!(
            file_reject.contains(&required),
            "fileForm.reject missing {required:?}"
        );
    }
    assert!(
        file_reject
            .iter()
            .any(|p| p.len() > 255 && !p.contains('/')),
        "fileForm.reject must include an overlong single component"
    );
    assert!(
        file_reject
            .iter()
            .any(|p| p.chars().any(|c| c.is_control())),
        "fileForm.reject must include a control-character path"
    );
}
