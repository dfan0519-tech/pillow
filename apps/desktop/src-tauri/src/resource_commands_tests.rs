//! Focused tests for project-scoped resource bridge isolation.
//! Written before command wiring to establish RED evidence.

use std::fs;

use tempfile::TempDir;

use crate::app_state::AppState;

/// The guard comes first so callers declare it first — locals drop in reverse
/// declaration order, so `state` closes its SQLite handles on `keeper.db` before
/// the directory holding that file is removed. Windows will not delete a file
/// that is still open, and the removal error is easy to lose.
fn test_state() -> (TempDir, AppState) {
    let temp_root = tempfile::Builder::new()
        .prefix("pillow-resource-commands-")
        .tempdir()
        .expect("test root should be created");
    let state = AppState::open(temp_root.path().join("keeper.db")).expect("test state should open");
    (temp_root, state)
}

fn register_project(state: &AppState, temp_root: &std::path::Path, leaf: &str) -> String {
    let project_root = temp_root.join(leaf);
    fs::create_dir_all(&project_root).expect("project root");
    state
        .register_picker_selection("main", &project_root)
        .expect("picker grant");
    let project = state
        .create_project_from_picker_for_window(
            "main",
            leaf.to_owned(),
            project_root.to_string_lossy().into_owned(),
        )
        .expect("create project");
    project.id
}

#[test]
fn unknown_project_id_fails_closed_for_all_resource_ops() {
    let (_temp_root, state) = test_state();
    let unknown = "00000000-0000-0000-0000-000000000099".to_owned();

    let list_err = state
        .list_project_resources(unknown.clone(), None)
        .expect_err("unknown project must not list");
    assert_eq!(list_err.code, "project_not_found");
    assert!(
        !list_err.message.contains('\\') && !list_err.message.contains('/'),
        "error must not contain path separators: {}",
        list_err.message
    );

    let read_err = state
        .read_project_resource(unknown.clone(), "src/main.rs".to_owned())
        .expect_err("unknown project must not read");
    assert_eq!(read_err.code, "project_not_found");

    let search_err = state
        .search_project_resources(unknown.clone(), "needle".to_owned(), None)
        .expect_err("unknown project must not search");
    assert_eq!(search_err.code, "project_not_found");

    let recent_err = state
        .list_recent_project_resources(unknown)
        .expect_err("unknown project must not list recents");
    assert_eq!(recent_err.code, "project_not_found");
}

#[test]
fn empty_or_malformed_project_id_fails_closed_without_fallback() {
    let (temp_root, state) = test_state();
    let _registered = register_project(&state, temp_root.path(), "only-project");

    for bad_id in ["", "not-a-uuid", "00000000-0000-0000-0000-000000000001"] {
        let err = state
            .list_project_resources(bad_id.to_owned(), None)
            .expect_err("invalid project id must fail closed");
        assert_eq!(
            err.code, "project_not_found",
            "expected project_not_found for {bad_id:?}"
        );
        assert!(
            !err.message.to_lowercase().contains("only-project"),
            "must not fall back to a registered project name"
        );
        assert!(
            !err.message
                .contains(&temp_root.path().to_string_lossy().to_string()),
            "must not leak machine temp root"
        );
    }
}

#[test]
fn resource_ops_are_isolated_by_project_id() {
    let (temp_root, state) = test_state();
    let project_a = register_project(&state, temp_root.path(), "project-a");
    let project_b = register_project(&state, temp_root.path(), "project-b");

    let root_a = temp_root.path().join("project-a");
    let root_b = temp_root.path().join("project-b");
    fs::write(root_a.join("only-a.txt"), b"alpha secret").expect("write a");
    fs::write(root_b.join("only-b.txt"), b"beta secret").expect("write b");
    fs::create_dir_all(root_a.join("src")).expect("src a");
    fs::write(root_a.join("src").join("lib.rs"), b"fn a() {}").expect("write a src");

    let list_a = state
        .list_project_resources(project_a.clone(), None)
        .expect("list a");
    let paths_a: Vec<&str> = list_a.iter().map(|e| e.path.as_str()).collect();
    assert!(
        paths_a.contains(&"only-a.txt"),
        "a sees its own file: {paths_a:?}"
    );
    assert!(
        !paths_a.iter().any(|p| p.contains("only-b")),
        "a must not see b: {paths_a:?}"
    );
    assert!(
        list_a
            .iter()
            .all(|e| !e.path.contains(':') && !e.path.contains('\\')),
        "portable paths only: {list_a:?}"
    );

    let list_b = state
        .list_project_resources(project_b.clone(), None)
        .expect("list b");
    let paths_b: Vec<&str> = list_b.iter().map(|e| e.path.as_str()).collect();
    assert!(paths_b.contains(&"only-b.txt"));
    assert!(!paths_b.iter().any(|p| p.contains("only-a")));

    let read_a = state
        .read_project_resource(project_a.clone(), "only-a.txt".to_owned())
        .expect("read a");
    assert_eq!(read_a.path, "only-a.txt");
    assert_eq!(read_a.content.as_deref(), Some("alpha secret"));
    assert_eq!(read_a.content_kind, "utf8");

    let cross = state
        .read_project_resource(project_a.clone(), "only-b.txt".to_owned())
        .expect_err("a cannot read b file by relative path that does not exist under a");
    assert_eq!(cross.code, "resource_not_found");
    assert!(
        !cross.message.contains("beta")
            && !cross
                .message
                .contains(&root_b.to_string_lossy().to_string()),
        "cross-project miss must not leak content or paths: {}",
        cross.message
    );
}

#[test]
fn invalid_relative_paths_fail_closed_without_machine_paths() {
    let (temp_root, state) = test_state();
    let project_id = register_project(&state, temp_root.path(), "path-guard");
    let root = temp_root.path().join("path-guard");
    fs::write(root.join("ok.txt"), b"ok").expect("write");

    for bad in [
        "",
        "..",
        "../secret",
        "C:/Windows",
        "Z:/not-a-real-machine/pillow",
        "/etc/passwd",
        "foo\\bar",
        "a/../../b",
    ] {
        let err = state
            .list_project_resources(project_id.clone(), Some(bad.to_owned()))
            .expect_err("bad path must fail");
        assert!(
            err.code == "resource_path_rejected" || err.code == "resource_not_found",
            "unexpected code {} for {bad:?}",
            err.code
        );
        assert!(
            !err.message
                .contains(&temp_root.path().to_string_lossy().to_string()),
            "error leaked temp root for {bad:?}: {}",
            err.message
        );
        assert!(
            !err.message.contains("passwd") && !err.message.contains("Windows"),
            "error leaked input path detail for {bad:?}: {}",
            err.message
        );
    }
}

#[test]
fn resource_search_rejects_blank_queries_without_fallback_or_path_leakage() {
    let (temp_root, state) = test_state();
    let project_a = register_project(&state, temp_root.path(), "search-a");
    let project_b = register_project(&state, temp_root.path(), "search-b");
    fs::write(
        temp_root.path().join("search-a").join("visible.txt"),
        b"alpha",
    )
    .expect("write a");
    fs::write(
        temp_root.path().join("search-b").join("private.txt"),
        b"beta machine-private marker",
    )
    .expect("write b");

    for query in ["", " ", "\t\n"] {
        let error = state
            .search_project_resources(project_a.clone(), query.to_owned(), None)
            .expect_err("public resource search must reject blank query");
        assert_eq!(error.code, "resource_search_query_rejected");
        assert_eq!(error.message, "Resource search query was rejected.");
        assert!(
            !error
                .message
                .contains(&temp_root.path().to_string_lossy().to_string())
                && !error.message.contains("machine-private")
                && !error.message.contains("search-b"),
            "blank-query error must not fall back or leak a machine path: {}",
            error.message
        );
    }

    let unrelated = state
        .search_project_resources(project_a, "machine-private".to_owned(), None)
        .expect("registered project search remains scoped");
    assert!(
        unrelated.is_empty(),
        "project A must not fall back to project B"
    );

    // Keep the second registration live so the isolation assertion above cannot pass
    // by silently replacing the selected project with the only known project.
    assert!(!project_b.is_empty());
}

#[test]
fn successful_read_updates_only_matching_project_recents() {
    let (temp_root, state) = test_state();
    let project_a = register_project(&state, temp_root.path(), "recent-a");
    let project_b = register_project(&state, temp_root.path(), "recent-b");
    fs::write(temp_root.path().join("recent-a").join("a.txt"), b"a").expect("write a");
    fs::write(temp_root.path().join("recent-b").join("b.txt"), b"b").expect("write b");

    state
        .read_project_resource(project_a.clone(), "a.txt".to_owned())
        .expect("read a");
    state
        .read_project_resource(project_b.clone(), "b.txt".to_owned())
        .expect("read b");

    let recents_a = state
        .list_recent_project_resources(project_a.clone())
        .expect("recents a");
    assert_eq!(recents_a.len(), 1);
    assert_eq!(recents_a[0].path, "a.txt");
    assert!(
        recents_a.iter().all(|r| r.path != "b.txt"),
        "project a must not receive b recents: {recents_a:?}"
    );

    let recents_b = state
        .list_recent_project_resources(project_b)
        .expect("recents b");
    assert_eq!(recents_b.len(), 1);
    assert_eq!(recents_b[0].path, "b.txt");
}

#[test]
fn binary_read_returns_preview_metadata_without_unbounded_bytes() {
    let (temp_root, state) = test_state();
    let project_id = register_project(&state, temp_root.path(), "binary-project");
    let bytes = [0u8, 1, 2, 255, 0x80];
    fs::write(
        temp_root.path().join("binary-project").join("blob.bin"),
        bytes,
    )
    .expect("write binary");

    let file = state
        .read_project_resource(project_id, "blob.bin".to_owned())
        .expect("read binary");

    assert_eq!(file.path, "blob.bin");
    assert_eq!(file.content_kind, "binary");
    assert!(
        file.content.is_none(),
        "binary must not expose UTF-8 content"
    );
    assert_eq!(file.bytes_returned, bytes.len() as u64);
    assert!(!file.truncated);
    // Bridge DTO must not serialize raw bytes to the renderer.
    let json = serde_json::to_value(&file).expect("serialize");
    assert!(json.get("bytes").is_none(), "must omit bytes field: {json}");
}

#[test]
fn resource_preview_unknown_and_empty_project_ids_fail_closed() {
    let (temp_root, state) = test_state();
    let _registered = register_project(&state, temp_root.path(), "preview-only");

    for bad_id in ["", "not-a-uuid", "00000000-0000-0000-0000-000000000099"] {
        let err = state
            .preview_project_resource(bad_id.to_owned(), "notes.md".to_owned())
            .expect_err("invalid project id must fail closed for preview");
        assert_eq!(err.code, "project_not_found");
        assert!(
            !err.message.contains('\\') && !err.message.contains('/'),
            "preview error must not contain path separators: {}",
            err.message
        );
        assert!(
            !err.message
                .contains(&temp_root.path().to_string_lossy().to_string()),
            "preview error must not leak temp root"
        );
        assert!(
            !err.message.to_lowercase().contains("preview-only"),
            "must not fall back to a registered project name"
        );
    }
}

#[test]
fn resource_preview_is_isolated_by_project_id() {
    let (temp_root, state) = test_state();
    let project_a = register_project(&state, temp_root.path(), "preview-a");
    let _project_b = register_project(&state, temp_root.path(), "preview-b");
    let root_a = temp_root.path().join("preview-a");
    let root_b = temp_root.path().join("preview-b");
    fs::write(root_a.join("only-a.txt"), b"alpha secret").expect("write a");
    fs::write(root_b.join("only-b.txt"), b"beta secret").expect("write b");

    let preview_a = state
        .preview_project_resource(project_a.clone(), "only-a.txt".to_owned())
        .expect("preview a");
    assert_eq!(preview_a.path, "only-a.txt");
    assert_eq!(preview_a.kind, "text");
    assert_eq!(preview_a.media_type, "text/plain; charset=utf-8");
    assert_eq!(preview_a.content.as_deref(), Some("alpha secret"));
    assert!(preview_a.data_url.is_none());

    let cross = state
        .preview_project_resource(project_a.clone(), "only-b.txt".to_owned())
        .expect_err("a cannot preview b content");
    assert_eq!(cross.code, "resource_not_found");
    assert!(
        !cross.message.contains("beta")
            && !cross
                .message
                .contains(&root_b.to_string_lossy().to_string()),
        "cross-project miss must not leak content or paths: {}",
        cross.message
    );
}

#[test]
fn resource_preview_classifies_markdown_and_text() {
    let (temp_root, state) = test_state();
    let project_id = register_project(&state, temp_root.path(), "preview-text");
    let root = temp_root.path().join("preview-text");
    fs::create_dir_all(root.join("docs")).expect("docs");
    fs::write(root.join("docs").join("guide.MD"), b"# Title\n\nHello").expect("md");
    fs::write(root.join("plain.txt"), b"hello plain").expect("txt");
    fs::write(root.join("notes.md"), b"## Notes").expect("notes");

    let md = state
        .preview_project_resource(project_id.clone(), "notes.md".to_owned())
        .expect("preview md");
    assert_eq!(md.kind, "markdown");
    assert_eq!(md.media_type, "text/markdown; charset=utf-8");
    assert_eq!(md.content.as_deref(), Some("## Notes"));
    assert!(md.data_url.is_none());

    let upper = state
        .preview_project_resource(project_id.clone(), "docs/guide.MD".to_owned())
        .expect("preview upper md");
    assert_eq!(upper.kind, "markdown");
    assert_eq!(upper.media_type, "text/markdown; charset=utf-8");

    let text = state
        .preview_project_resource(project_id, "plain.txt".to_owned())
        .expect("preview text");
    assert_eq!(text.kind, "text");
    assert_eq!(text.media_type, "text/plain; charset=utf-8");
    assert_eq!(text.content.as_deref(), Some("hello plain"));
    assert!(text.data_url.is_none());
}

fn write_image_fixture(root: &std::path::Path, name: &str, bytes: &[u8]) {
    fs::write(root.join(name), bytes).expect("write image fixture");
}

#[test]
fn resource_preview_allowlisted_images_require_extension_and_signature() {
    let (temp_root, state) = test_state();
    let project_id = register_project(&state, temp_root.path(), "preview-images");
    let root = temp_root.path().join("preview-images");

    let png = b"\x89PNG\r\n\x1a\nIHDR-payload";
    let jpeg = b"\xFF\xD8\xFF\xE0JFIF-payload";
    let gif87 = b"GIF87a\x01\x00payload";
    let gif89 = b"GIF89a\x01\x00payload";
    let mut webp = b"RIFF\x10\x00\x00\x00WEBPpayload".to_vec();
    // Ensure length field is arbitrary but signature bytes are correct.
    webp[4..8].copy_from_slice(&8u32.to_le_bytes());

    write_image_fixture(&root, "icon.png", png);
    write_image_fixture(&root, "photo.jpg", jpeg);
    write_image_fixture(&root, "shot.jpeg", jpeg);
    write_image_fixture(&root, "anim.gif", gif89);
    write_image_fixture(&root, "old.gif", gif87);
    write_image_fixture(&root, "pic.webp", &webp);

    let cases = [
        ("icon.png", "image/png", png.as_slice()),
        ("photo.jpg", "image/jpeg", jpeg.as_slice()),
        ("shot.jpeg", "image/jpeg", jpeg.as_slice()),
        ("anim.gif", "image/gif", gif89.as_slice()),
        ("old.gif", "image/gif", gif87.as_slice()),
        ("pic.webp", "image/webp", webp.as_slice()),
    ];

    for (path, mime, bytes) in cases {
        let preview = state
            .preview_project_resource(project_id.clone(), path.to_owned())
            .expect("preview image");
        assert_eq!(preview.path, path);
        assert_eq!(preview.kind, "image");
        assert_eq!(preview.media_type, mime);
        assert!(preview.content.is_none());
        let data_url = preview
            .data_url
            .as_ref()
            .expect("image must include dataUrl");
        assert!(
            data_url.starts_with(&format!("data:{mime};base64,")),
            "dataUrl prefix mismatch for {path}: {data_url}"
        );
        assert_eq!(preview.bytes_returned, bytes.len() as u64);
        assert!(!preview.truncated);
        let json = serde_json::to_value(&preview).expect("serialize");
        assert!(json.get("bytes").is_none(), "must omit bytes: {json}");
        assert!(json.get("content").is_none() || json["content"].is_null());
    }
}

#[test]
fn resource_preview_mismatched_or_unsupported_binary_is_metadata_only() {
    let (temp_root, state) = test_state();
    let project_id = register_project(&state, temp_root.path(), "preview-binary");
    let root = temp_root.path().join("preview-binary");

    // Extension says PNG but payload is JPEG signature.
    fs::write(root.join("fake.png"), b"\xFF\xD8\xFFnot-a-png").expect("fake png");
    // Extension says JPG but PNG signature.
    fs::write(root.join("fake.jpg"), b"\x89PNG\r\n\x1a\nxxxx").expect("fake jpg");
    // Unsupported binary extension.
    fs::write(root.join("blob.bin"), [0u8, 1, 2, 255, 0x80]).expect("bin");
    // WebP extension without WEBP chunk.
    fs::write(root.join("bad.webp"), b"RIFF\x08\x00\x00\x00NOPE").expect("bad webp");

    for path in ["fake.png", "fake.jpg", "blob.bin", "bad.webp"] {
        let preview = state
            .preview_project_resource(project_id.clone(), path.to_owned())
            .expect("preview binary metadata");
        assert_eq!(preview.path, path);
        assert_eq!(preview.kind, "binary");
        assert_eq!(preview.media_type, "application/octet-stream");
        assert!(preview.content.is_none());
        assert!(preview.data_url.is_none());
        let json = serde_json::to_value(&preview).expect("serialize");
        assert!(json.get("bytes").is_none(), "must omit bytes: {json}");
        assert!(json.get("dataUrl").is_none() || json["dataUrl"].is_null());
        assert!(json.get("content").is_none() || json["content"].is_null());
    }
}

#[test]
fn resource_preview_truncated_binary_never_becomes_image() {
    let (temp_root, state) = test_state();
    let project_id = register_project(&state, temp_root.path(), "preview-trunc");
    let root = temp_root.path().join("preview-trunc");

    // Larger than MAX_READ_BYTES (1 MiB) with a valid PNG signature prefix.
    let mut huge = vec![0u8; 1_048_576 + 64];
    huge[..8].copy_from_slice(b"\x89PNG\r\n\x1a\n");
    fs::write(root.join("huge.png"), &huge).expect("write huge png");

    let preview = state
        .preview_project_resource(project_id, "huge.png".to_owned())
        .expect("preview truncated");
    assert_eq!(preview.kind, "binary");
    assert_eq!(preview.media_type, "application/octet-stream");
    assert!(preview.truncated);
    assert_eq!(preview.bytes_returned, 1_048_576);
    assert!(preview.data_url.is_none());
    assert!(preview.content.is_none());
    let json = serde_json::to_value(&preview).expect("serialize");
    assert!(json.get("bytes").is_none(), "must omit bytes: {json}");
}

#[test]
fn resource_preview_serialized_dto_never_includes_bytes() {
    let (temp_root, state) = test_state();
    let project_id = register_project(&state, temp_root.path(), "preview-serde");
    let root = temp_root.path().join("preview-serde");
    fs::write(root.join("a.txt"), b"text").expect("text");
    fs::write(root.join("b.bin"), [0u8, 255]).expect("bin");
    fs::write(root.join("c.png"), b"\x89PNG\r\n\x1a\nxx").expect("png");

    for path in ["a.txt", "b.bin", "c.png"] {
        let preview = state
            .preview_project_resource(project_id.clone(), path.to_owned())
            .expect("preview");
        let json = serde_json::to_value(&preview).expect("serialize");
        assert!(
            json.get("bytes").is_none(),
            "DTO must never serialize bytes for {path}: {json}"
        );
    }
}

#[test]
fn resource_preview_errors_are_opaque_without_machine_paths() {
    let (temp_root, state) = test_state();
    let project_id = register_project(&state, temp_root.path(), "preview-err");
    let root = temp_root.path().join("preview-err");
    fs::write(root.join("ok.txt"), b"ok").expect("write");

    for bad in [
        "",
        "..",
        "../secret",
        "C:/Windows",
        "Z:/not-a-real-machine/pillow",
        "/etc/passwd",
        "foo\\bar",
        "a/../../b",
    ] {
        let err = state
            .preview_project_resource(project_id.clone(), bad.to_owned())
            .expect_err("bad path must fail");
        assert!(
            err.code == "resource_path_rejected" || err.code == "resource_not_found",
            "unexpected code {} for {bad:?}",
            err.code
        );
        assert!(
            !err.message
                .contains(&temp_root.path().to_string_lossy().to_string()),
            "error leaked temp root for {bad:?}: {}",
            err.message
        );
        assert!(
            !err.message.contains("passwd")
                && !err.message.contains("Windows")
                && !err.message.contains("pillow"),
            "error leaked input path detail for {bad:?}: {}",
            err.message
        );
    }
}

#[test]
fn resource_preview_updates_only_matching_project_recents() {
    let (temp_root, state) = test_state();
    let project_a = register_project(&state, temp_root.path(), "preview-recent-a");
    let project_b = register_project(&state, temp_root.path(), "preview-recent-b");
    fs::write(
        temp_root.path().join("preview-recent-a").join("a.txt"),
        b"a",
    )
    .expect("write a");
    fs::write(
        temp_root.path().join("preview-recent-b").join("b.txt"),
        b"b",
    )
    .expect("write b");

    state
        .preview_project_resource(project_a.clone(), "a.txt".to_owned())
        .expect("preview a");
    state
        .preview_project_resource(project_b.clone(), "b.txt".to_owned())
        .expect("preview b");

    let recents_a = state
        .list_recent_project_resources(project_a.clone())
        .expect("recents a");
    assert_eq!(recents_a.len(), 1);
    assert_eq!(recents_a[0].path, "a.txt");
    assert!(
        recents_a.iter().all(|r| r.path != "b.txt"),
        "project a must not receive b recents: {recents_a:?}"
    );

    let recents_b = state
        .list_recent_project_resources(project_b)
        .expect("recents b");
    assert_eq!(recents_b.len(), 1);
    assert_eq!(recents_b[0].path, "b.txt");
}
