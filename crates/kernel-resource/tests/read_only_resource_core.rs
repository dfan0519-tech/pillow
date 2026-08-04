//! Focused contract tests for the read-only project resource core.
//!
//! Covers path isolation, deterministic list/search, symlink/reparse refusal,
//! and bounded UTF-8/binary reads.

use std::fs;
use std::path::Path;

use keeper_project::ProjectId;
use keeper_resource::{
    ProjectResourceCapability, ResourceContentKind, ResourceEntryKind, ResourceError,
    ResourceListRequest, ResourceReadRequest, ResourceReader, ResourceSearchRequest,
};

fn capability(root: &Path) -> ProjectResourceCapability {
    ProjectResourceCapability::new(ProjectId::new(), root).expect("capability root")
}

fn reader() -> ResourceReader {
    ResourceReader
}

#[test]
fn list_output_is_sorted_by_filename() {
    let temp = tempfile::tempdir().unwrap();
    fs::create_dir(temp.path().join("b-dir")).unwrap();
    fs::write(temp.path().join("z.txt"), b"z").unwrap();
    fs::write(temp.path().join("a.txt"), b"a").unwrap();

    let capability = capability(temp.path());
    let directory = reader()
        .list(
            &capability,
            ResourceListRequest {
                path: ".".into(),
                max_entries: 20,
            },
        )
        .unwrap();

    let paths: Vec<&str> = directory
        .entries
        .iter()
        .map(|entry| entry.path.as_str())
        .collect();
    assert_eq!(paths, vec!["a.txt", "b-dir", "z.txt"]);
    assert!(!directory.truncated);

    let kinds: Vec<ResourceEntryKind> = directory.entries.iter().map(|e| e.kind).collect();
    assert_eq!(
        kinds,
        vec![
            ResourceEntryKind::File,
            ResourceEntryKind::Directory,
            ResourceEntryKind::File
        ]
    );
}

#[cfg(windows)]
#[test]
fn list_surfaces_reparse_as_symlink_and_does_not_traverse() {
    use std::os::windows::fs::symlink_dir;

    let project = tempfile::tempdir().unwrap();
    let outside = tempfile::tempdir().unwrap();
    fs::write(outside.path().join("secret.txt"), b"outside").unwrap();
    fs::write(project.path().join("visible.txt"), b"inside").unwrap();

    if symlink_dir(outside.path(), project.path().join("escape")).is_err() {
        // Privilege may be unavailable; still exercise regular listing.
        let capability = capability(project.path());
        let directory = reader()
            .list(
                &capability,
                ResourceListRequest {
                    path: ".".into(),
                    max_entries: 20,
                },
            )
            .unwrap();
        assert!(directory.entries.iter().any(|e| e.path == "visible.txt"));
        return;
    }

    let capability = capability(project.path());
    let directory = reader()
        .list(
            &capability,
            ResourceListRequest {
                path: ".".into(),
                max_entries: 20,
            },
        )
        .unwrap();

    let escape = directory
        .entries
        .iter()
        .find(|entry| entry.path == "escape")
        .expect("escape entry present");
    assert_eq!(escape.kind, ResourceEntryKind::Symlink);

    // Never follow: listing or reading through the reparse point is rejected.
    let list_err = reader()
        .list(
            &capability,
            ResourceListRequest {
                path: "escape".into(),
                max_entries: 20,
            },
        )
        .unwrap_err();
    assert!(matches!(list_err, ResourceError::PathRejected));

    let read_err = reader()
        .read(
            &capability,
            ResourceReadRequest {
                path: "escape/secret.txt".into(),
                max_bytes: 64,
            },
        )
        .unwrap_err();
    assert!(matches!(read_err, ResourceError::PathRejected));
}

#[cfg(unix)]
#[test]
fn list_surfaces_symlink_and_does_not_traverse() {
    use std::os::unix::fs::symlink;

    let project = tempfile::tempdir().unwrap();
    let outside = tempfile::tempdir().unwrap();
    fs::write(outside.path().join("secret.txt"), b"outside").unwrap();
    fs::write(project.path().join("visible.txt"), b"inside").unwrap();
    symlink(outside.path(), project.path().join("escape")).unwrap();

    let capability = capability(project.path());
    let directory = reader()
        .list(
            &capability,
            ResourceListRequest {
                path: ".".into(),
                max_entries: 20,
            },
        )
        .unwrap();

    let escape = directory
        .entries
        .iter()
        .find(|entry| entry.path == "escape")
        .expect("escape entry present");
    assert_eq!(escape.kind, ResourceEntryKind::Symlink);

    let list_err = reader()
        .list(
            &capability,
            ResourceListRequest {
                path: "escape".into(),
                max_entries: 20,
            },
        )
        .unwrap_err();
    assert!(matches!(list_err, ResourceError::PathRejected));
}

#[test]
fn rejects_absolute_dotdot_backslash_and_outside_root_paths() {
    let temp = tempfile::tempdir().unwrap();
    fs::create_dir(temp.path().join("nested")).unwrap();
    fs::write(temp.path().join("nested").join("file.txt"), b"ok").unwrap();
    let capability = capability(temp.path());

    let cases = [
        "",
        "C:/Windows",
        "/etc/passwd",
        "nested\\file.txt",
        "nested/../file.txt",
        "..",
        "./nested",
        "nested/./file.txt",
    ];

    for path in cases {
        let err = reader()
            .list(
                &capability,
                ResourceListRequest {
                    path: path.into(),
                    max_entries: 10,
                },
            )
            .unwrap_err();
        assert!(
            matches!(err, ResourceError::PathRejected),
            "list should reject path {path:?}, got {err:?}"
        );

        let err = reader()
            .read(
                &capability,
                ResourceReadRequest {
                    path: path.into(),
                    max_bytes: 32,
                },
            )
            .unwrap_err();
        assert!(
            matches!(err, ResourceError::PathRejected),
            "read should reject path {path:?}, got {err:?}"
        );
    }

    // Outside-root via traversal is already covered by `..`; absolute also covered.
    // Empty root marker only `.` is valid for root.
    let ok = reader()
        .list(
            &capability,
            ResourceListRequest {
                path: ".".into(),
                max_entries: 10,
            },
        )
        .unwrap();
    assert!(ok.entries.iter().any(|e| e.path == "nested"));
}

#[test]
fn read_distinguishes_utf8_from_binary_and_obeys_byte_truncation() {
    let temp = tempfile::tempdir().unwrap();
    fs::write(temp.path().join("note.txt"), b"hello pillow").unwrap();
    fs::write(temp.path().join("blob.bin"), [0xff, 0xfe, 0x00, 0x01]).unwrap();
    let capability = capability(temp.path());

    let text = reader()
        .read(
            &capability,
            ResourceReadRequest {
                path: "note.txt".into(),
                max_bytes: 5,
            },
        )
        .unwrap();
    assert_eq!(text.content_kind, ResourceContentKind::Utf8);
    assert_eq!(text.content.as_deref(), Some("hello"));
    assert!(text.bytes.is_none());
    assert_eq!(text.bytes_returned, 5);
    assert!(text.truncated);
    assert_eq!(text.path, "note.txt");

    let full = reader()
        .read(
            &capability,
            ResourceReadRequest {
                path: "note.txt".into(),
                max_bytes: 1_048_576,
            },
        )
        .unwrap();
    assert_eq!(full.content.as_deref(), Some("hello pillow"));
    assert_eq!(full.bytes_returned, 12);
    assert!(!full.truncated);

    let binary = reader()
        .read(
            &capability,
            ResourceReadRequest {
                path: "blob.bin".into(),
                max_bytes: 64,
            },
        )
        .unwrap();
    assert_eq!(binary.content_kind, ResourceContentKind::Binary);
    assert!(binary.content.is_none());
    assert_eq!(binary.bytes.as_deref(), Some(&[0xff, 0xfe, 0x00, 0x01][..]));
    assert_eq!(binary.bytes_returned, 4);
    assert!(!binary.truncated);
}

#[test]
fn read_caps_max_bytes_at_one_mebibyte() {
    let temp = tempfile::tempdir().unwrap();
    // Slightly over 1 MiB of valid UTF-8.
    let payload = vec![b'a'; 1_048_576 + 32];
    fs::write(temp.path().join("big.txt"), &payload).unwrap();
    let capability = capability(temp.path());

    let result = reader()
        .read(
            &capability,
            ResourceReadRequest {
                path: "big.txt".into(),
                max_bytes: 10_000_000,
            },
        )
        .unwrap();

    assert_eq!(result.bytes_returned, 1_048_576);
    assert!(result.truncated);
    assert_eq!(result.content_kind, ResourceContentKind::Utf8);
    assert_eq!(result.content.as_ref().map(|s| s.len()), Some(1_048_576));
}

#[test]
fn search_is_deterministic_bounded_and_skips_symlink_descendants() {
    let temp = tempfile::tempdir().unwrap();
    fs::create_dir(temp.path().join("src")).unwrap();
    fs::write(
        temp.path().join("src").join("lib.rs"),
        "Pillow owns policy.\nprovider does not.\n",
    )
    .unwrap();
    fs::write(temp.path().join("README.md"), "pillow overview\n").unwrap();
    fs::write(temp.path().join("binary.bin"), [0x00, 0xff, b'p', b'i']).unwrap();

    // Long line should be skipped (not matched via unbounded read).
    let long_line = format!("Pillow {}", "x".repeat(70_000));
    fs::write(temp.path().join("long.txt"), long_line).unwrap();

    #[cfg(windows)]
    {
        use std::os::windows::fs::symlink_dir;
        let outside = tempfile::tempdir().unwrap();
        fs::write(outside.path().join("hit.txt"), "pillow outside\n").unwrap();
        let _ = symlink_dir(outside.path(), temp.path().join("link-out"));
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::symlink;
        let outside = tempfile::tempdir().unwrap();
        fs::write(outside.path().join("hit.txt"), "pillow outside\n").unwrap();
        symlink(outside.path(), temp.path().join("link-out")).unwrap();
    }

    let capability = capability(temp.path());
    let results = reader()
        .search(
            &capability,
            ResourceSearchRequest {
                path: ".".into(),
                query: "pillow".into(),
                case_sensitive: false,
                max_results: 20,
            },
        )
        .unwrap();

    assert_eq!(results.matches.len(), 2);
    assert_eq!(results.matches[0].path, "README.md");
    assert_eq!(results.matches[1].path, "src/lib.rs");
    assert!(results.matches.iter().all(|m| m.line == 1));
    assert!(results.matches.iter().all(|m| m.column >= 1));
    assert!(results.matches.iter().all(|m| !m.preview.is_empty()));
    assert!(
        results
            .matches
            .iter()
            .all(|m| !m.path.contains("link-out") && !m.path.contains("hit.txt"))
    );
    // long.txt overlong line is skipped; binary skipped.
    assert!(results.matches.iter().all(|m| m.path != "long.txt"));
    assert!(results.matches.iter().all(|m| m.path != "binary.bin"));
    assert!(!results.truncated);

    // Result cap truncates deterministically.
    let capped = reader()
        .search(
            &capability,
            ResourceSearchRequest {
                path: ".".into(),
                query: "pillow".into(),
                case_sensitive: false,
                max_results: 1,
            },
        )
        .unwrap();
    assert_eq!(capped.matches.len(), 1);
    assert_eq!(capped.matches[0].path, "README.md");
    assert!(capped.truncated);
}

#[test]
fn search_rejects_blank_queries_and_keeps_unicode_search_safe() {
    let temp = tempfile::tempdir().unwrap();
    fs::write(
        temp.path().join("unicode.txt"),
        "naïve café\n猫 is present\n",
    )
    .unwrap();
    let capability = capability(temp.path());

    for query in ["", " ", "\t\n", "\u{2003}"] {
        let error = reader()
            .search(
                &capability,
                ResourceSearchRequest {
                    path: ".".into(),
                    query: query.into(),
                    case_sensitive: false,
                    max_results: 20,
                },
            )
            .expect_err("blank queries must fail closed before scanning");
        assert_eq!(error, ResourceError::InvalidSearchQuery);
    }

    let unicode_match = reader()
        .search(
            &capability,
            ResourceSearchRequest {
                path: ".".into(),
                query: "猫".into(),
                case_sensitive: true,
                max_results: 20,
            },
        )
        .expect("nonblank Unicode query must remain searchable");
    assert_eq!(unicode_match.matches.len(), 1);
    assert_eq!(unicode_match.matches[0].path, "unicode.txt");
    assert_eq!(unicode_match.matches[0].line, 2);
    assert_eq!(unicode_match.matches[0].column, 1);
}

#[test]
fn list_enforces_entry_cap_and_reports_truncation() {
    let temp = tempfile::tempdir().unwrap();
    for index in 0..5 {
        fs::write(temp.path().join(format!("f{index}.txt")), b"x").unwrap();
    }
    let capability = capability(temp.path());
    let directory = reader()
        .list(
            &capability,
            ResourceListRequest {
                path: ".".into(),
                max_entries: 2,
            },
        )
        .unwrap();
    assert_eq!(directory.entries.len(), 2);
    assert!(directory.truncated);
    // Sorted: f0, f1 first.
    assert_eq!(directory.entries[0].path, "f0.txt");
    assert_eq!(directory.entries[1].path, "f1.txt");
}

#[test]
fn public_errors_do_not_embed_absolute_machine_paths() {
    let temp = tempfile::tempdir().unwrap();
    let capability = capability(temp.path());
    let err = reader()
        .read(
            &capability,
            ResourceReadRequest {
                path: "missing.txt".into(),
                max_bytes: 8,
            },
        )
        .unwrap_err();
    let rendered = err.to_string();
    assert!(!rendered.contains(temp.path().to_string_lossy().as_ref()));
    // Also reject drive-letter leakage patterns when present on Windows.
    assert!(!rendered.contains(":\\"));
}
