//! Byte-fixture tests for porcelain v2 `-z` status parsing.
//!
//! Fixtures mirror real `git status --porcelain=v2 -z --branch` records
//! (Git 2.x machine format). Paths are portable project-relative strings.

use keeper_git_executor::{
    GitHead, GitRepositoryId, GitStatusKind, ParseError, STATUS_ENTRY_CAP, STATUS_TRANSPORT_CAP,
    parse_porcelain_v2_z,
};

fn repo() -> GitRepositoryId {
    GitRepositoryId {
        project_relative: ".".to_string(),
    }
}

fn z(parts: &[&str]) -> Vec<u8> {
    let mut out = Vec::new();
    for part in parts {
        out.extend_from_slice(part.as_bytes());
        out.push(0);
    }
    out
}

#[test]
fn ordinary_modified_entry() {
    let bytes = z(&[
        "# branch.oid abcdef0123456789abcdef0123456789abcdef01",
        "# branch.head main",
        "1 .M N... 100644 100644 100644 ce013625030ba8dba906f756967f9e9ca394464a ce013625030ba8dba906f756967f9e9ca394464a src/lib.rs",
    ]);
    let snap = parse_porcelain_v2_z(&bytes, repo(), 7).expect("parse");
    assert_eq!(snap.generation, 7);
    assert_eq!(snap.repository.project_relative, ".");
    assert!(!snap.truncated);
    match &snap.head {
        GitHead::Branch { name, oid } => {
            assert_eq!(name, "main");
            assert_eq!(oid, "abcdef0123456789abcdef0123456789abcdef01");
        }
        other => panic!("expected branch head, got {other:?}"),
    }
    assert_eq!(snap.entries.len(), 1);
    let e = &snap.entries[0];
    assert_eq!(e.path, "src/lib.rs");
    assert_eq!(e.old_path, None);
    assert_eq!(e.kind, GitStatusKind::Modified);
    assert_eq!(e.xy, [b'.', b'M']);
}

#[test]
fn untracked_entry() {
    let bytes = z(&[
        "# branch.oid deadbeefdeadbeefdeadbeefdeadbeefdeadbeef",
        "# branch.head develop",
        "? notes/todo.md",
    ]);
    let snap = parse_porcelain_v2_z(&bytes, repo(), 1).expect("parse");
    assert_eq!(snap.entries.len(), 1);
    assert_eq!(snap.entries[0].path, "notes/todo.md");
    assert_eq!(snap.entries[0].kind, GitStatusKind::Untracked);
    assert_eq!(snap.entries[0].xy, [b'?', b'?']);
}

#[test]
fn untracked_directory_entry_keeps_trailing_slash() {
    // Real shape: `--untracked-files=normal` collapses an untracked directory
    // into a single record with exactly one trailing separator.
    let bytes = z(&[
        "# branch.oid deadbeefdeadbeefdeadbeefdeadbeefdeadbeef",
        "# branch.head main",
        "? vendor/",
    ]);
    let snap = parse_porcelain_v2_z(&bytes, repo(), 1).expect("parse");
    assert_eq!(snap.entries.len(), 1);
    assert_eq!(snap.entries[0].path, "vendor/");
    assert_eq!(snap.entries[0].kind, GitStatusKind::Untracked);
}

#[test]
fn ignored_directory_entry_keeps_trailing_slash() {
    let bytes = z(&[
        "# branch.oid deadbeefdeadbeefdeadbeefdeadbeefdeadbeef",
        "# branch.head main",
        "! target/",
    ]);
    let snap = parse_porcelain_v2_z(&bytes, repo(), 1).expect("parse");
    assert_eq!(snap.entries.len(), 1);
    assert_eq!(snap.entries[0].path, "target/");
    assert_eq!(snap.entries[0].kind, GitStatusKind::Ignored);
}

#[test]
fn untracked_directory_does_not_abort_the_whole_status() {
    // Regression: a single collapsed-directory record used to fail the entire
    // parse, so one untracked folder blanked the whole repository status.
    let bytes = z(&[
        "# branch.oid deadbeefdeadbeefdeadbeefdeadbeefdeadbeef",
        "# branch.head main",
        "1 .M N... 100644 100644 100644 ce013625030ba8dba906f756967f9e9ca394464a ce013625030ba8dba906f756967f9e9ca394464a src/lib.rs",
        "? vendor/",
        "? notes/todo.md",
        "? nested/deep/dir/",
    ]);
    let snap = parse_porcelain_v2_z(&bytes, repo(), 1).expect("parse");
    let paths: Vec<&str> = snap.entries.iter().map(|e| e.path.as_str()).collect();
    assert_eq!(
        paths,
        vec!["src/lib.rs", "vendor/", "notes/todo.md", "nested/deep/dir/"]
    );
}

#[test]
fn rejects_malformed_trailing_separator_forms() {
    // Only one trailing separator on an untracked/ignored record is legal.
    // Containment for traversal, absolute and repeated separators is unchanged.
    let cases = [
        "? /",
        "? //",
        "? vendor//",
        "? ./",
        "? ../escape/",
        "? vendor/../escape/",
        "! /",
        "! build//",
    ];
    for line in cases {
        let bytes = z(&["# branch.oid aa", "# branch.head main", line]);
        let err = parse_porcelain_v2_z(&bytes, repo(), 1).expect_err(line);
        assert!(
            matches!(err, ParseError::PathRejected { .. }),
            "expected PathRejected for {line}, got {err:?}"
        );
    }
}

#[test]
fn tracked_records_still_reject_a_trailing_separator() {
    // The directory form is permitted only for `?`/`!`. Git never emits a
    // trailing separator for tracked, rename or unmerged records.
    let cases = [
        "1 .M N... 100644 100644 100644 aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb src/dir/",
        "u UU N... 100644 100644 100644 100644 aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb cccccccccccccccccccccccccccccccccccccccc conflict/",
    ];
    for line in cases {
        let bytes = z(&["# branch.oid aa", "# branch.head main", line]);
        let err = parse_porcelain_v2_z(&bytes, repo(), 1).expect_err(line);
        assert!(
            matches!(err, ParseError::PathRejected { .. }),
            "expected PathRejected for {line}, got {err:?}"
        );
    }
}

#[test]
fn rename_is_one_entry_with_old_path() {
    // Real -z shape: score+path NUL-terminated, then origPath NUL-terminated.
    let mut bytes = z(&[
        "# branch.oid 7729c4e5537be1e9f934e9aa447a2610204a51b1",
        "# branch.head master",
    ]);
    bytes.extend_from_slice(
        b"2 R. N... 100644 100644 100644 c1b0730e0133447badcfd47fd144e254807b06e1 c1b0730e0133447badcfd47fd144e254807b06e1 R100 new.txt\0old.txt\0",
    );
    let snap = parse_porcelain_v2_z(&bytes, repo(), 2).expect("parse");
    assert_eq!(snap.entries.len(), 1);
    let e = &snap.entries[0];
    assert_eq!(e.path, "new.txt");
    assert_eq!(e.old_path.as_deref(), Some("old.txt"));
    assert_eq!(e.kind, GitStatusKind::Renamed);
    assert_eq!(e.xy, [b'R', b'.']);
}

#[test]
fn conflict_unmerged_entry() {
    let bytes = z(&[
        "# branch.oid 479473d3325a2fe53aefb47436e8a99f2508682f",
        "# branch.head master",
        "u UU N... 100644 100644 100644 100644 1b9074b7e89cd2ab95843a7dc7f96f0458168820 447924b21920115add0a9766add1d469cc659980 f853c8de5875367bc27a8ac388506d7513050250 e.txt",
    ]);
    let snap = parse_porcelain_v2_z(&bytes, repo(), 3).expect("parse");
    assert_eq!(snap.entries.len(), 1);
    assert_eq!(snap.entries[0].path, "e.txt");
    assert_eq!(snap.entries[0].kind, GitStatusKind::Conflict);
    assert_eq!(snap.entries[0].xy, [b'U', b'U']);
}

#[test]
fn submodule_entry() {
    let bytes = z(&[
        "# branch.oid aabbccddeeff00112233445566778899aabbccdd",
        "# branch.head main",
        "1 .M S.M. 160000 160000 160000 1111111111111111111111111111111111111111 2222222222222222222222222222222222222222 vendor/lib",
    ]);
    let snap = parse_porcelain_v2_z(&bytes, repo(), 4).expect("parse");
    assert_eq!(snap.entries.len(), 1);
    assert_eq!(snap.entries[0].path, "vendor/lib");
    assert_eq!(snap.entries[0].kind, GitStatusKind::Submodule);
}

#[test]
fn detached_head() {
    let bytes = z(&[
        "# branch.oid 7729c4e5537be1e9f934e9aa447a2610204a51b1",
        "# branch.head (detached)",
        "? det.txt",
    ]);
    let snap = parse_porcelain_v2_z(&bytes, repo(), 5).expect("parse");
    match &snap.head {
        GitHead::Detached { oid } => {
            assert_eq!(oid, "7729c4e5537be1e9f934e9aa447a2610204a51b1");
        }
        other => panic!("expected detached head, got {other:?}"),
    }
    assert_eq!(snap.entries[0].kind, GitStatusKind::Untracked);
}

#[test]
fn mixed_fixture_covers_all_states() {
    let mut bytes = z(&[
        "# branch.oid 0123456789abcdef0123456789abcdef01234567",
        "# branch.head feature/x",
        "1 .M N... 100644 100644 100644 aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb src/a.rs",
        "? untracked.rs",
        "u UU N... 100644 100644 100644 100644 cccccccccccccccccccccccccccccccccccccccc dddddddddddddddddddddddddddddddddddddddd eeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeee conflict.txt",
        "1 .M S.M. 160000 160000 160000 ffffffffffffffffffffffffffffffffffffffff 0000000000000000000000000000000000000001 mods/sub",
    ]);
    bytes.extend_from_slice(
        b"2 R. N... 100644 100644 100644 1111111111111111111111111111111111111111 1111111111111111111111111111111111111111 R100 renamed.rs\0old_name.rs\0",
    );
    let snap = parse_porcelain_v2_z(&bytes, repo(), 9).expect("parse");
    assert!(!snap.truncated);
    assert_eq!(snap.entries.len(), 5);
    let kinds: Vec<_> = snap.entries.iter().map(|e| e.kind).collect();
    assert!(kinds.contains(&GitStatusKind::Modified));
    assert!(kinds.contains(&GitStatusKind::Untracked));
    assert!(kinds.contains(&GitStatusKind::Conflict));
    assert!(kinds.contains(&GitStatusKind::Submodule));
    assert!(kinds.contains(&GitStatusKind::Renamed));
    match snap.head {
        GitHead::Branch { name, .. } => assert_eq!(name, "feature/x"),
        other => panic!("unexpected head {other:?}"),
    }
}

#[test]
fn rejects_trailing_partial_record() {
    let mut bytes = z(&["# branch.oid aa", "# branch.head main"]);
    bytes.extend_from_slice(b"1 .M N... 100644 100644 100644 ");
    // no terminating NUL — incomplete ordinary record
    let err = parse_porcelain_v2_z(&bytes, repo(), 1).expect_err("partial");
    assert!(matches!(err, ParseError::Malformed { .. }));
}

#[test]
fn rejects_invalid_repository_identity_at_parser_ingress() {
    let bytes = z(&["# branch.oid aa", "# branch.head main"]);
    for project_relative in ["", "../escape", "C:/outside", "src\\windows", "NUL"] {
        let err = parse_porcelain_v2_z(
            &bytes,
            GitRepositoryId {
                project_relative: project_relative.into(),
            },
            1,
        )
        .expect_err(project_relative);
        assert!(matches!(err, ParseError::PathRejected { .. }));
    }
}

#[test]
fn rejects_empty_nul_delimited_records() {
    let bytes = z(&["# branch.oid aa", "", "# branch.head main"]);
    let err = parse_porcelain_v2_z(&bytes, repo(), 1).expect_err("empty record");
    assert!(matches!(err, ParseError::Malformed { .. }));
}

#[test]
fn rejects_malformed_ordinary_field_count() {
    let bytes = z(&[
        "# branch.oid aa",
        "# branch.head main",
        "1 .M N... only-two-fields",
    ]);
    let err = parse_porcelain_v2_z(&bytes, repo(), 1).expect_err("malformed");
    assert!(matches!(err, ParseError::Malformed { .. }));
}

#[test]
fn rejects_non_portable_paths() {
    let cases = [
        "1 .M N... 100644 100644 100644 aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb C:/evil.txt",
        "1 .M N... 100644 100644 100644 aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb ../escape.txt",
        "? abs\\win.txt",
        "? /absolute",
    ];
    for line in cases {
        let bytes = z(&["# branch.oid aa", "# branch.head main", line]);
        let err = parse_porcelain_v2_z(&bytes, repo(), 1).expect_err(line);
        assert!(
            matches!(err, ParseError::PathRejected { .. }),
            "expected PathRejected for {line}, got {err:?}"
        );
    }
}

#[test]
fn rejects_rename_with_missing_old_path() {
    // Score+path present but no second NUL record for origPath.
    let mut bytes = z(&["# branch.oid aa", "# branch.head main"]);
    bytes.extend_from_slice(
        b"2 R. N... 100644 100644 100644 aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb R100 new.txt\0",
    );
    // stream ends without old path record
    let err = parse_porcelain_v2_z(&bytes, repo(), 1).expect_err("missing old");
    assert!(matches!(err, ParseError::Malformed { .. }));
}

#[test]
fn status_entry_cap_sets_truncated() {
    let mut parts: Vec<String> = vec!["# branch.oid aa".into(), "# branch.head main".into()];
    for i in 0..(STATUS_ENTRY_CAP + 3) {
        parts.push(format!(
            "1 .M N... 100644 100644 100644 aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb f{i:05}.txt"
        ));
    }
    let refs: Vec<&str> = parts.iter().map(|s| s.as_str()).collect();
    let bytes = z(&refs);
    let snap = parse_porcelain_v2_z(&bytes, repo(), 1).expect("parse");
    assert!(snap.truncated);
    assert_eq!(snap.entries.len(), STATUS_ENTRY_CAP);
}

#[test]
fn status_transport_cap_sets_truncated() {
    // Build input larger than transport cap with many small complete records.
    let header = z(&["# branch.oid aa", "# branch.head main"]);
    let entry = b"1 .M N... 100644 100644 100644 aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb pad/xxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxx.txt\0";
    let mut bytes = header;
    while bytes.len() < STATUS_TRANSPORT_CAP + entry.len() * 2 {
        bytes.extend_from_slice(entry);
    }
    assert!(bytes.len() > STATUS_TRANSPORT_CAP);
    let snap = parse_porcelain_v2_z(&bytes, repo(), 1).expect("parse");
    assert!(snap.truncated);
    assert!(!snap.entries.is_empty());
    assert!(snap.entries.len() <= STATUS_ENTRY_CAP);
}
