//! Bounds tests for one-file Git text/binary/oversized diff views.

use keeper_git_executor::{DIFF_TEXT_BYTE_CAP, GitDiffView, bound_git_diff};

#[test]
fn accepts_small_text_diff() {
    let raw = b"diff --git a/a.rs b/a.rs\n--- a/a.rs\n+++ b/a.rs\n@@ -1 +1 @@\n-old\n+new\n";
    let view = bound_git_diff("a.rs", raw).expect("bound");
    match view {
        GitDiffView::Text {
            path,
            content,
            byte_count,
        } => {
            assert_eq!(path, "a.rs");
            assert_eq!(content.as_bytes(), raw.as_slice());
            assert_eq!(byte_count, raw.len());
        }
        other => panic!("expected Text, got {other:?}"),
    }
}

#[test]
fn binary_returns_typed_summary_not_partial_text() {
    let mut raw = b"diff --git a/pic.bin b/pic.bin\n".to_vec();
    raw.extend_from_slice(b"Binary files a/pic.bin and b/pic.bin differ\n");
    raw.push(0); // NUL proves binary payload
    raw.extend_from_slice(&[0xFF, 0xD8, 0xFF]);
    let view = bound_git_diff("pic.bin", &raw).expect("bound");
    match view {
        GitDiffView::Binary { path, size_bytes } => {
            assert_eq!(path, "pic.bin");
            assert_eq!(size_bytes, raw.len());
        }
        other => panic!("expected Binary summary, got {other:?}"),
    }
}

#[test]
fn oversized_returns_typed_summary_not_partial_success() {
    let mut raw = vec![b'x'; DIFF_TEXT_BYTE_CAP + 1];
    raw[0] = b'd'; // still text-ish, just oversized
    let view = bound_git_diff("big.rs", &raw).expect("bound");
    match view {
        GitDiffView::Oversized {
            path,
            size_bytes,
            limit_bytes,
        } => {
            assert_eq!(path, "big.rs");
            assert_eq!(size_bytes, DIFF_TEXT_BYTE_CAP + 1);
            assert_eq!(limit_bytes, DIFF_TEXT_BYTE_CAP);
        }
        other => panic!("expected Oversized summary, got {other:?}"),
    }
}

#[test]
fn text_at_exact_cap_is_accepted() {
    let raw = vec![b'a'; DIFF_TEXT_BYTE_CAP];
    let view = bound_git_diff("exact.rs", &raw).expect("bound");
    match view {
        GitDiffView::Text {
            content,
            byte_count,
            ..
        } => {
            assert_eq!(byte_count, DIFF_TEXT_BYTE_CAP);
            assert_eq!(content.len(), DIFF_TEXT_BYTE_CAP);
        }
        other => panic!("expected Text at cap, got {other:?}"),
    }
}

#[test]
fn rejects_non_portable_diff_path() {
    let err = bound_git_diff("../escape.rs", b"+ok\n").expect_err("path");
    assert!(format!("{err:?}").contains("PathRejected") || format!("{err}").contains("path"));
}

#[test]
fn empty_diff_is_text() {
    let view = bound_git_diff("empty.rs", b"").expect("bound");
    match view {
        GitDiffView::Text {
            content,
            byte_count,
            ..
        } => {
            assert!(content.is_empty());
            assert_eq!(byte_count, 0);
        }
        other => panic!("expected empty Text, got {other:?}"),
    }
}
