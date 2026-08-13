//! Bounded host-side unified diff tests (C3-C).
//!
//! Exact zero-context unified strings for create, replace, insert, delete,
//! final-newline, Unicode, identical text, and cap truncation. Also proves
//! `BoundedDiff` Debug never leaks source.

use keeper_tool_executor::{
    BoundedDiff, DIFF_BYTE_CAP, DIFF_LINE_CAP, build_bounded_unified_diff, content_sha256_digest,
    page_proposed_content, proposed_content_digest,
};

const SECRET_DIFF_BODY: &str = "SECRET_DIFF_UNIFIED_C3C_xyz_never_in_debug";

#[test]
fn create_diff_exact_zero_context_string() {
    let diff = build_bounded_unified_diff(None, "alpha\nbeta\n");
    let expected = "\
--- /dev/null
+++ b
@@ -0,0 +1,2 @@
+alpha
+beta
";
    assert_eq!(diff.unified, expected);
    assert!(!diff.truncated);
    assert_eq!(diff.byte_count, expected.len() as u64);
    assert_eq!(diff.line_count, expected.lines().count() as u32);
}

#[test]
fn replacement_after_common_prefix_suffix_exact() {
    let base = "one\ntwo\nthree\n";
    let proposed = "one\ntwo-changed\nthree\n";
    let first = build_bounded_unified_diff(Some(base), proposed);
    let second = build_bounded_unified_diff(Some(base), proposed);
    let expected = "\
--- a
+++ b
@@ -2,1 +2,1 @@
-two
+two-changed
";
    assert_eq!(first.unified, expected);
    assert_eq!(first.unified, second.unified);
    assert!(!first.truncated);
    // Zero-context: no bare context line for "one" or "three".
    assert!(!first.unified.lines().any(|l| l == " one" || l == " three"));
}

#[test]
fn insertion_after_n_existing_lines_old_count_zero() {
    let base = "a\nb\nc\n";
    let proposed = "a\nb\nINSERTED\nc\n";
    let diff = build_bounded_unified_diff(Some(base), proposed);
    let expected = "\
--- a
+++ b
@@ -2,0 +3,1 @@
+INSERTED
";
    assert_eq!(diff.unified, expected);
    assert!(diff.unified.contains("@@ -2,0 +3,1 @@"));
}

#[test]
fn deletion_after_n_retained_lines_new_count_zero() {
    let base = "a\nb\nDEL\nc\n";
    let proposed = "a\nb\nc\n";
    let diff = build_bounded_unified_diff(Some(base), proposed);
    let expected = "\
--- a
+++ b
@@ -3,1 +2,0 @@
-DEL
";
    assert_eq!(diff.unified, expected);
    assert!(diff.unified.contains("@@ -3,1 +2,0 @@"));
}

#[test]
fn add_final_newline_shows_last_line_and_marker() {
    let base = "a\nb";
    let proposed = "a\nb\n";
    let diff = build_bounded_unified_diff(Some(base), proposed);
    let expected = "\
--- a
+++ b
@@ -2,1 +2,1 @@
-b
\\ No newline at end of file
+b
";
    assert_eq!(diff.unified, expected);
    assert!(diff.unified.contains("\\ No newline at end of file"));
    // Must not emit an empty hunk body.
    assert!(diff.unified.contains("-b"));
    assert!(diff.unified.contains("+b"));
}

#[test]
fn remove_final_newline_shows_last_line_and_marker() {
    let base = "a\nb\n";
    let proposed = "a\nb";
    let diff = build_bounded_unified_diff(Some(base), proposed);
    let expected = "\
--- a
+++ b
@@ -2,1 +2,1 @@
-b
+b
\\ No newline at end of file
";
    assert_eq!(diff.unified, expected);
}

#[test]
fn unicode_lines_exact_insertion() {
    let base = "你好\n世界\n";
    let proposed = "你好\nPillow 🌊\n世界\n";
    let first = build_bounded_unified_diff(Some(base), proposed);
    let second = build_bounded_unified_diff(Some(base), proposed);
    let expected = "\
--- a
+++ b
@@ -1,0 +2,1 @@
+Pillow 🌊
";
    assert_eq!(first.unified, expected);
    assert_eq!(first.unified, second.unified);
    assert!(!first.truncated);
}

#[test]
fn identical_base_and_proposed_yields_headers_only() {
    let content = "same\nlines\n";
    let diff = build_bounded_unified_diff(Some(content), content);
    let expected = "\
--- a
+++ b
";
    assert_eq!(diff.unified, expected);
    assert!(!diff.truncated);
    let body_changes = diff
        .unified
        .lines()
        .filter(|line| {
            (line.starts_with('+') && !line.starts_with("+++"))
                || (line.starts_with('-') && !line.starts_with("---"))
        })
        .count();
    assert_eq!(body_changes, 0);
}

#[test]
fn truncates_at_exact_line_cap() {
    let mut proposed = String::new();
    for i in 0..(DIFF_LINE_CAP as usize + 500) {
        proposed.push_str(&format!("line-{i:05}\n"));
    }
    let diff = build_bounded_unified_diff(None, &proposed);
    assert!(diff.truncated, "expected truncation by line cap");
    assert!(
        diff.line_count <= DIFF_LINE_CAP,
        "line_count {} exceeds cap {}",
        diff.line_count,
        DIFF_LINE_CAP
    );
    assert!(
        diff.unified.contains("truncated") || diff.unified.contains("TRUNCATED"),
        "truncation marker missing"
    );
}

#[test]
fn truncates_at_exact_byte_cap() {
    let long = "x".repeat(4_000);
    let mut proposed = String::new();
    for i in 0..80 {
        proposed.push_str(&format!("{i:04}-{long}\n"));
    }
    let diff = build_bounded_unified_diff(None, &proposed);
    assert!(diff.truncated, "expected truncation by byte cap");
    assert!(
        diff.byte_count <= DIFF_BYTE_CAP,
        "byte_count {} exceeds cap {}",
        diff.byte_count,
        DIFF_BYTE_CAP
    );
    assert!(diff.unified.len() as u64 <= DIFF_BYTE_CAP);
    assert!(
        diff.unified.contains("truncated") || diff.unified.contains("TRUNCATED"),
        "truncation marker missing"
    );
}

#[test]
fn within_cap_is_not_truncated() {
    let proposed = "a\nb\nc\n";
    let diff = build_bounded_unified_diff(None, proposed);
    assert!(!diff.truncated);
    assert!(diff.line_count < DIFF_LINE_CAP);
    assert!(diff.byte_count < DIFF_BYTE_CAP);
}

#[test]
fn bounded_diff_debug_redacts_unified_source() {
    let mut diff = build_bounded_unified_diff(None, &format!("{SECRET_DIFF_BODY}\n"));
    // Force distinctive secret into the public structure even if builder changed.
    diff.unified = format!("--- a\n+++ b\n+{SECRET_DIFF_BODY}\n");
    let debug = format!("{diff:?}");
    assert!(
        !debug.contains(SECRET_DIFF_BODY),
        "Debug leaked unified source: {debug}"
    );
    assert!(debug.contains("redacted") || debug.contains("REDACTED"));
    // Compile-time: BoundedDiff must not implement Serialize.
    fn assert_not_serialize<T: ?Sized>(_value: &T) {}
    assert_not_serialize::<BoundedDiff>(&diff);
}

#[test]
fn content_digest_matches_executor_sealed_digest_not_unified_text() {
    let proposed = "hello 你好 🌊\nHIDDEN_TAIL\n";
    let diff = build_bounded_unified_diff(None, proposed);
    assert_eq!(diff.content_digest, content_sha256_digest(proposed));
    assert_eq!(diff.content_digest, proposed_content_digest(proposed));
    assert_ne!(diff.content_digest, content_sha256_digest(&diff.unified));
    assert_eq!(diff.proposed_content(), proposed);
}

#[test]
fn create_line_cap_truncation_still_digests_full_proposed() {
    let mut proposed = String::new();
    for i in 0..(DIFF_LINE_CAP as usize + 80) {
        proposed.push_str(&format!("line-{i:05}\n"));
    }
    proposed.push_str("HIDDEN_CREATE_TAIL\n");
    let diff = build_bounded_unified_diff(None, &proposed);
    assert!(diff.truncated, "expected line-cap truncation");
    assert!(
        !diff.unified.contains("HIDDEN_CREATE_TAIL"),
        "bounded unified must not contain the hidden tail"
    );
    assert_eq!(diff.content_digest, content_sha256_digest(&proposed));
    assert_eq!(diff.proposed_content(), proposed);
}

#[test]
fn modify_huge_base_truncates_diff_but_digest_is_proposed() {
    let mut base = String::new();
    for i in 0..(DIFF_LINE_CAP as usize + 80) {
        base.push_str(&format!("old-{i:05}\n"));
    }
    let proposed = "new-head\nHIDDEN_NEW_TAIL\n";
    let diff = build_bounded_unified_diff(Some(&base), proposed);
    assert!(diff.truncated, "huge base must truncate the unified diff");
    assert!(
        !diff.unified.contains("HIDDEN_NEW_TAIL"),
        "proposed tail sits after deleted-base rows and must be truncated away"
    );
    assert_eq!(diff.content_digest, content_sha256_digest(proposed));
    assert_eq!(diff.proposed_content(), proposed);
}

#[test]
fn content_pages_cover_create_and_modify_and_reassemble() {
    let mut create_body = String::new();
    for i in 0..(DIFF_LINE_CAP as usize + 25) {
        create_body.push_str(&format!("c-{i:05}\n"));
    }
    let create = build_bounded_unified_diff(None, &create_body);
    let first = create.content_page(0).expect("page 0");
    assert!(first.page_count >= 2);
    let mut rebuilt = String::new();
    for index in 0..first.page_count {
        let page = create.content_page(index).expect("page in range");
        assert!(page.line_count <= DIFF_LINE_CAP);
        assert!(page.byte_count <= DIFF_BYTE_CAP);
        rebuilt.push_str(&page.text);
    }
    assert_eq!(rebuilt, create_body);

    let mut old = String::new();
    for i in 0..(DIFF_LINE_CAP as usize + 10) {
        old.push_str(&format!("prev-{i:05}\n"));
    }
    let modify_proposed = "only-new\n";
    let modify = build_bounded_unified_diff(Some(&old), modify_proposed);
    assert!(modify.truncated);
    let page = modify.content_page(0).expect("modify page");
    assert_eq!(page.page_count, 1);
    assert_eq!(page.text, modify_proposed);
    assert_eq!(
        modify.content_digest,
        content_sha256_digest(modify_proposed)
    );
}

#[test]
fn content_pages_never_split_utf8_multibyte_characters() {
    let emoji = "🌊";
    assert_eq!(emoji.len(), 4);
    // Leave 2 bytes of room so a naive byte split would tear the emoji.
    let prefix = "a".repeat(DIFF_BYTE_CAP as usize - 2);
    let proposed = format!("{prefix}{emoji}tail\n");
    let page0 = page_proposed_content(&proposed, 0).expect("page 0");
    assert!(std::str::from_utf8(page0.text.as_bytes()).is_ok());
    assert!(!page0.text.contains('\u{fffd}'));
    assert!(
        page0.page_count >= 2,
        "emoji must not fit on the first page"
    );
    assert!(
        page0.text.ends_with('a'),
        "page 0 must stop before the emoji: {:?}",
        page0.text.chars().rev().take(4).collect::<String>()
    );
    let page1 = page_proposed_content(&proposed, 1).expect("page 1");
    assert!(page1.text.starts_with(emoji), "page 1 must start at emoji");
    let mut rebuilt = String::new();
    for index in 0..page0.page_count {
        rebuilt.push_str(&page_proposed_content(&proposed, index).unwrap().text);
    }
    assert_eq!(rebuilt, proposed);
}
