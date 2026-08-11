//! Real-repository integration tests for the hardened read-only Git runner.
//!
//! Fixtures are temporary Git repositories created with the host `git` binary.
//! The product path under test uses only fixed templates through
//! `GitReadExecutor` (never caller-controlled argv).

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use keeper_git_executor::{
    DIFF_TEXT_BYTE_CAP, GitDiffSnapshot, GitDiffView, GitHead, GitReadError, GitReadExecutor,
    GitRepositoryId, GitStatusKind, UnsafeConfigKind,
};

static TEMP_SEQ: AtomicU64 = AtomicU64::new(0);

fn host_git() -> PathBuf {
    // Prefer PATH resolution for fixture setup only (not product API).
    which_git().expect("host git required for fixture setup")
}

fn which_git() -> Option<PathBuf> {
    if let Ok(p) = std::env::var("PILLOW_TEST_GIT") {
        let pb = PathBuf::from(p);
        if pb.is_file() {
            return Some(pb);
        }
    }
    let candidates = [
        r"C:\Program Files\Git\cmd\git.exe",
        r"C:\Program Files (x86)\Git\cmd\git.exe",
    ];
    for c in candidates {
        let p = PathBuf::from(c);
        if p.is_file() {
            return Some(p);
        }
    }
    // Last resort: ask `where` via cmd is avoided; try bare name in PATH dirs.
    if let Ok(path) = std::env::var("PATH") {
        for dir in path.split(';') {
            let p = Path::new(dir).join("git.exe");
            if p.is_file() {
                return Some(p);
            }
            let p2 = Path::new(dir).join("git");
            if p2.is_file() {
                return Some(p2);
            }
        }
    }
    None
}

fn temp_root(label: &str) -> PathBuf {
    let n = TEMP_SEQ.fetch_add(1, Ordering::Relaxed);
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let dir = std::env::temp_dir().join(format!("pillow-git-read-{label}-{nanos}-{n}"));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).expect("temp root");
    dir
}

struct Fixture {
    root: PathBuf,
    git: PathBuf,
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.root);
    }
}

impl Fixture {
    fn project(label: &str) -> Self {
        let root = temp_root(label);
        Self {
            root,
            git: host_git(),
        }
    }

    fn git(&self, cwd: &Path, args: &[&str]) -> std::process::Output {
        let out = Command::new(&self.git)
            .args(args)
            .current_dir(cwd)
            .env_remove("GIT_DIR")
            .env_remove("GIT_WORK_TREE")
            .env("GIT_TERMINAL_PROMPT", "0")
            .env("GIT_CONFIG_NOSYSTEM", "1")
            .env("GIT_AUTHOR_NAME", "Pillow Fixture")
            .env("GIT_AUTHOR_EMAIL", "fixture@pillow.local")
            .env("GIT_COMMITTER_NAME", "Pillow Fixture")
            .env("GIT_COMMITTER_EMAIL", "fixture@pillow.local")
            .output()
            .expect("run fixture git");
        if !out.status.success() {
            panic!(
                "fixture git {:?} in {} failed: status={:?}\nstdout={}\nstderr={}",
                args,
                cwd.display(),
                out.status.code(),
                String::from_utf8_lossy(&out.stdout),
                String::from_utf8_lossy(&out.stderr)
            );
        }
        out
    }

    fn init_repo(&self, rel: &str) -> PathBuf {
        let path = if rel == "." {
            self.root.clone()
        } else {
            let p = self.root.join(rel);
            fs::create_dir_all(&p).expect("repo dir");
            p
        };
        self.git(&path, &["init", "-b", "main"]);
        self.git(&path, &["config", "user.name", "Pillow Fixture"]);
        self.git(&path, &["config", "user.email", "fixture@pillow.local"]);
        // Disable local template hooks noise by ensuring no custom hooksPath.
        path
    }

    fn write(&self, repo: &Path, rel: &str, body: &[u8]) {
        let path = repo.join(rel);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).expect("parent");
        }
        fs::write(&path, body).expect("write file");
    }

    fn commit_all(&self, repo: &Path, msg: &str) {
        self.git(repo, &["add", "-A"]);
        self.git(repo, &["commit", "-m", msg]);
    }

    fn executor(&self) -> GitReadExecutor {
        GitReadExecutor::new(self.git.clone(), Duration::from_secs(30)).expect("executor")
    }
}

fn root_id() -> GitRepositoryId {
    GitRepositoryId {
        project_relative: ".".to_string(),
    }
}

fn nested_id(rel: &str) -> GitRepositoryId {
    GitRepositoryId {
        project_relative: rel.to_string(),
    }
}

fn assert_diff_meta(snap: &GitDiffSnapshot, repo: &str, generation: u64) {
    assert_eq!(snap.repository.project_relative, repo);
    assert_eq!(snap.generation, generation);
}

// ---------------------------------------------------------------------------
// Discovery
// ---------------------------------------------------------------------------

#[test]
fn discover_non_repository_returns_empty() {
    let fx = Fixture::project("non-repo");
    fs::write(fx.root.join("readme.txt"), b"not a git repo").unwrap();
    let disc = fx.executor().discover(&fx.root).expect("discover");
    assert!(
        disc.repositories.is_empty(),
        "expected no repositories, got {:?}",
        disc.repositories
    );
}

#[test]
fn discover_ordinary_repository_at_project_root() {
    let fx = Fixture::project("ordinary");
    let repo = fx.init_repo(".");
    fx.write(&repo, "a.txt", b"hello\n");
    fx.commit_all(&repo, "init");

    let disc = fx.executor().discover(&fx.root).expect("discover");
    assert_eq!(disc.repositories.len(), 1);
    assert_eq!(disc.repositories[0].project_relative, ".");
    assert!(disc.repositories[0].available);
}

#[test]
fn discover_nested_repository_isolation() {
    let fx = Fixture::project("nested");
    let root = fx.init_repo(".");
    fx.write(&root, "root.txt", b"root\n");
    fx.commit_all(&root, "root");

    let nested = fx.init_repo("vendor/nested");
    fx.write(&nested, "inner.txt", b"inner\n");
    fx.commit_all(&nested, "nested");

    let disc = fx.executor().discover(&fx.root).expect("discover");
    let paths: Vec<_> = disc
        .repositories
        .iter()
        .map(|r| r.project_relative.as_str())
        .collect();
    assert!(paths.contains(&"."), "paths={paths:?}");
    assert!(paths.contains(&"vendor/nested"), "paths={paths:?}");

    // Nested status is independent of the parent worktree.
    let nested_status = fx
        .executor()
        .status(&fx.root, &nested_id("vendor/nested"), 1)
        .expect("nested status");
    assert_eq!(nested_status.repository.project_relative, "vendor/nested");
    assert!(
        nested_status
            .entries
            .iter()
            .all(|e| !e.path.starts_with("vendor/")),
        "nested status must use repo-relative paths mapped to project-relative; got {:?}",
        nested_status.entries
    );
    // Clean tree: no dirty entries; just ensure head is ok.
    assert!(matches!(nested_status.head, GitHead::Branch { .. }));
}

#[test]
fn root_status_reports_an_untracked_nested_repository_directory() {
    // Regression, real git: an embedded repository that is NOT gitignored makes
    // porcelain v2 emit `? vendor/` for the *root* repository. Every layer —
    // parser, project-path mapping, and the runner — must carry that directory
    // form, otherwise a single untracked folder fails the whole root status.
    //
    // `discover_nested_repository_isolation` builds this exact shape but only
    // asserts discovery and the *nested* status, so it never covered the root.
    let fx = Fixture::project("nested-root-status");
    let root = fx.init_repo(".");
    fx.write(&root, "root.txt", b"root\n");
    fx.commit_all(&root, "root");

    let nested = fx.init_repo("vendor/nested");
    fx.write(&nested, "inner.txt", b"inner\n");
    fx.commit_all(&nested, "nested");

    let status = fx
        .executor()
        .status(&fx.root, &root_id(), 1)
        .expect("root status must succeed with an untracked nested repository");

    let paths: Vec<&str> = status.entries.iter().map(|e| e.path.as_str()).collect();
    assert!(
        paths.contains(&"vendor/"),
        "root status must report the untracked directory; got {paths:?}"
    );
}

#[test]
fn root_status_matches_the_desktop_acceptance_project_shape() {
    // Mirrors the C4 Git acceptance "Project A" fixture exactly (binary blob,
    // rename source, markers, plus an untracked nested repository) so a real
    // desktop failure can be reproduced here in seconds instead of a 4 minute
    // E2E run.
    let fx = Fixture::project("acceptance-shape");
    let root = fx.init_repo(".");
    fx.write(&root, "readme.md", b"project-a root\n");
    fx.write(&root, "note.txt", b"line1\n");
    fx.write(&root, "blob.bin", &[0u8, 1, 2, 3, 0, 9]);
    fx.write(&root, "old_name.txt", b"rename-body\n");
    fx.write(&root, "project-a-only.txt", b"alpha-marker\n");
    fx.commit_all(&root, "init root");

    let nested = fx.init_repo("vendor/nested");
    fx.write(&nested, "nested.txt", b"nested clean\n");
    fx.commit_all(&nested, "init nested");

    let status = fx
        .executor()
        .status(&fx.root, &root_id(), 7)
        .expect("root status must succeed on the acceptance project shape");

    let paths: Vec<&str> = status.entries.iter().map(|e| e.path.as_str()).collect();
    assert_eq!(
        paths,
        vec!["vendor/"],
        "root should report exactly the untracked nested directory"
    );

    // The nested repository stays independently readable and clean.
    let nested_status = fx
        .executor()
        .status(&fx.root, &nested_id("vendor/nested"), 7)
        .expect("nested status");
    assert!(nested_status.entries.is_empty(), "nested must be clean");
}

#[test]
fn linked_worktree_outside_project_is_denied() {
    let fx = Fixture::project("wt-out");
    let main = fx.init_repo(".");
    fx.write(&main, "a.txt", b"a\n");
    fx.commit_all(&main, "init");

    // Create a linked worktree outside the project root.
    let outside = temp_root("wt-sibling");
    let outside_path = outside.join("linked");
    let out = Command::new(&fx.git)
        .args([
            "worktree",
            "add",
            "--detach",
            outside_path.to_str().unwrap(),
        ])
        .current_dir(&main)
        .env("GIT_TERMINAL_PROMPT", "0")
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("GIT_AUTHOR_NAME", "Pillow Fixture")
        .env("GIT_AUTHOR_EMAIL", "fixture@pillow.local")
        .env("GIT_COMMITTER_NAME", "Pillow Fixture")
        .env("GIT_COMMITTER_EMAIL", "fixture@pillow.local")
        .output()
        .expect("worktree add");
    if !out.status.success() {
        let _ = fs::remove_dir_all(&outside);
        panic!(
            "worktree add failed: {}",
            String::from_utf8_lossy(&out.stderr)
        );
    }

    // Discovery under the main project must not surface the outside worktree
    // as a portable repository identity (only the in-tree root).
    let disc = fx.executor().discover(&fx.root).expect("discover main");
    assert_eq!(
        disc.repositories.len(),
        1,
        "expected only project-root repo, got {:?}",
        disc.repositories
    );
    assert_eq!(disc.repositories[0].project_relative, ".");
    assert!(disc.repositories[0].available);
    let outside_s = outside_path.to_string_lossy().replace('\\', "/");
    assert!(
        !disc
            .repositories
            .iter()
            .any(|r| r.project_relative == outside_s),
        "outside absolute path leaked into discovery: {:?}",
        disc.repositories
    );

    // Portable path escape is rejected before any Git argv is built.
    let escape = fx.executor().status(
        &fx.root,
        &GitRepositoryId {
            project_relative: "..".to_string(),
        },
        1,
    );
    assert!(
        matches!(
            escape,
            Err(GitReadError::PathRejected) | Err(GitReadError::RepoOutsideProject)
        ),
        "got {escape:?}"
    );

    let _ = outside_path;
    let _ = fs::remove_dir_all(&outside);
    let _ = Command::new(&fx.git)
        .args(["worktree", "prune"])
        .current_dir(&main)
        .output();
}

// ---------------------------------------------------------------------------
// Status shapes
// ---------------------------------------------------------------------------

#[test]
fn status_ordinary_modified_and_untracked() {
    let fx = Fixture::project("status-basic");
    let repo = fx.init_repo(".");
    fx.write(&repo, "tracked.txt", b"v1\n");
    fx.commit_all(&repo, "init");
    fx.write(&repo, "tracked.txt", b"v2\n");
    fx.write(&repo, "new.txt", b"untracked\n");

    let snap = fx
        .executor()
        .status(&fx.root, &root_id(), 3)
        .expect("status");
    assert_eq!(snap.generation, 3);
    assert_eq!(snap.repository.project_relative, ".");
    assert!(matches!(snap.head, GitHead::Branch { ref name, .. } if name == "main"));

    let kinds: Vec<_> = snap
        .entries
        .iter()
        .map(|e| (e.path.as_str(), e.kind))
        .collect();
    assert!(
        kinds
            .iter()
            .any(|(p, k)| *p == "tracked.txt" && *k == GitStatusKind::Modified),
        "kinds={kinds:?}"
    );
    assert!(
        kinds
            .iter()
            .any(|(p, k)| *p == "new.txt" && *k == GitStatusKind::Untracked),
        "kinds={kinds:?}"
    );
}

#[test]
fn status_rename_binary_conflict_detached() {
    let fx = Fixture::project("status-shapes");
    let repo = fx.init_repo(".");
    fx.write(&repo, "old_name.txt", b"same\n");
    fx.write(&repo, "bin.dat", &[0u8, 1, 2, 3, 0, 9]);
    fx.commit_all(&repo, "base");

    // Rename
    fx.git(&repo, &["mv", "old_name.txt", "new_name.txt"]);
    // Modify binary
    fx.write(&repo, "bin.dat", &[0u8, 1, 2, 3, 0, 9, 10]);
    // Detached HEAD later — first capture rename+binary on branch
    let snap = fx
        .executor()
        .status(&fx.root, &root_id(), 1)
        .expect("status rename");
    assert!(
        snap.entries.iter().any(|e| e.kind == GitStatusKind::Renamed
            && e.path == "new_name.txt"
            && e.old_path.as_deref() == Some("old_name.txt")),
        "entries={:?}",
        snap.entries
    );
    assert!(
        snap.entries
            .iter()
            .any(|e| e.path == "bin.dat" && e.kind == GitStatusKind::Modified),
        "entries={:?}",
        snap.entries
    );

    // Commit rename, then create a conflict.
    fx.commit_all(&repo, "rename");
    fx.write(&repo, "conflict.txt", b"main-side\n");
    fx.commit_all(&repo, "main conflict base");
    fx.git(&repo, &["checkout", "-b", "feature"]);
    fx.write(&repo, "conflict.txt", b"feature-side\n");
    fx.commit_all(&repo, "feature change");
    fx.git(&repo, &["checkout", "main"]);
    fx.write(&repo, "conflict.txt", b"main-other\n");
    fx.commit_all(&repo, "main other");
    let merge = Command::new(&fx.git)
        .args(["merge", "feature", "--no-edit"])
        .current_dir(&repo)
        .env("GIT_TERMINAL_PROMPT", "0")
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("GIT_AUTHOR_NAME", "Pillow Fixture")
        .env("GIT_AUTHOR_EMAIL", "fixture@pillow.local")
        .env("GIT_COMMITTER_NAME", "Pillow Fixture")
        .env("GIT_COMMITTER_EMAIL", "fixture@pillow.local")
        .output()
        .expect("merge");
    assert!(!merge.status.success(), "expected conflict merge failure");

    let conflict_snap = fx
        .executor()
        .status(&fx.root, &root_id(), 2)
        .expect("conflict status");
    assert!(
        conflict_snap
            .entries
            .iter()
            .any(|e| e.path == "conflict.txt" && e.kind == GitStatusKind::Conflict),
        "entries={:?}",
        conflict_snap.entries
    );

    // Detached HEAD — leave the conflicted state via hard reset, then detach.
    let _ = Command::new(&fx.git)
        .args(["merge", "--abort"])
        .current_dir(&repo)
        .env("GIT_TERMINAL_PROMPT", "0")
        .output();
    fx.git(&repo, &["reset", "--hard", "HEAD"]);
    let _ = Command::new(&fx.git)
        .args(["clean", "-fd"])
        .current_dir(&repo)
        .env("GIT_TERMINAL_PROMPT", "0")
        .output();
    let detach_target = String::from_utf8_lossy(&fx.git(&repo, &["rev-parse", "HEAD"]).stdout)
        .trim()
        .to_string();
    fx.git(&repo, &["checkout", "--detach", &detach_target]);
    let det = fx
        .executor()
        .status(&fx.root, &root_id(), 4)
        .expect("detached status");
    match det.head {
        GitHead::Detached { oid: ref o } => assert!(!o.is_empty()),
        other => panic!("expected detached, got {other:?}"),
    }
}

// ---------------------------------------------------------------------------
// Diff bounds + generation
// ---------------------------------------------------------------------------

#[test]
fn diff_text_binary_oversized_and_generation() {
    let fx = Fixture::project("diff-bounds");
    let repo = fx.init_repo(".");
    fx.write(&repo, "note.txt", b"line1\n");
    fx.write(&repo, "blob.bin", &[0u8, 255, 0, 1, 2]);
    fx.commit_all(&repo, "init");

    fx.write(&repo, "note.txt", b"line1\nline2\n");
    let text = fx
        .executor()
        .diff(&fx.root, &root_id(), "note.txt", 10)
        .expect("text diff");
    assert_diff_meta(&text, ".", 10);
    match text.view {
        GitDiffView::Text {
            path,
            content,
            byte_count,
        } => {
            assert_eq!(path, "note.txt");
            assert!(content.contains("line2") || content.contains("+line2"));
            assert!(byte_count > 0);
        }
        other => panic!("expected text, got {other:?}"),
    }

    fx.write(&repo, "blob.bin", &[0u8, 255, 0, 1, 2, 3, 4]);
    let bin = fx
        .executor()
        .diff(&fx.root, &root_id(), "blob.bin", 11)
        .expect("binary diff");
    assert_diff_meta(&bin, ".", 11);
    match bin.view {
        GitDiffView::Binary { path, size_bytes } => {
            assert_eq!(path, "blob.bin");
            assert!(size_bytes > 0);
        }
        other => panic!("expected binary summary, got {other:?}"),
    }

    // Oversized: create a huge tracked text change.
    let huge = "x".repeat(DIFF_TEXT_BYTE_CAP + 8_192);
    fx.write(&repo, "huge.txt", b"seed\n");
    fx.commit_all(&repo, "seed huge");
    fx.write(&repo, "huge.txt", huge.as_bytes());
    let over = fx
        .executor()
        .diff(&fx.root, &root_id(), "huge.txt", 12)
        .expect("oversized diff");
    assert_diff_meta(&over, ".", 12);
    match over.view {
        GitDiffView::Oversized {
            path,
            size_bytes,
            limit_bytes,
        } => {
            assert_eq!(path, "huge.txt");
            assert!(size_bytes > DIFF_TEXT_BYTE_CAP);
            assert_eq!(limit_bytes, DIFF_TEXT_BYTE_CAP);
        }
        other => panic!("expected oversized summary, got {other:?}"),
    }

    // Generation is echoed; successive calls with different generations do not merge.
    let s1 = fx
        .executor()
        .status(&fx.root, &root_id(), 100)
        .expect("gen 100");
    let s2 = fx
        .executor()
        .status(&fx.root, &root_id(), 101)
        .expect("gen 101");
    assert_eq!(s1.generation, 100);
    assert_eq!(s2.generation, 101);
    assert_ne!(s1.generation, s2.generation);

    // Diff snapshot carries exact input generation + repository identity.
    let d1 = fx
        .executor()
        .diff(&fx.root, &root_id(), "note.txt", 777)
        .expect("gen 777");
    let d2 = fx
        .executor()
        .diff(&fx.root, &root_id(), "note.txt", 778)
        .expect("gen 778");
    assert_eq!(d1.generation, 777);
    assert_eq!(d2.generation, 778);
    assert_eq!(d1.repository.project_relative, ".");
    assert_eq!(d2.repository.project_relative, ".");
    assert_ne!(d1.generation, d2.generation);
}

// ---------------------------------------------------------------------------
// Suppression + unsafe config
// ---------------------------------------------------------------------------

#[test]
fn prompt_pager_ext_diff_textconv_are_suppressed() {
    let fx = Fixture::project("suppress");
    let repo = fx.init_repo(".");
    fx.write(&repo, "a.txt", b"a\n");
    fx.commit_all(&repo, "init");
    fx.write(&repo, "a.txt", b"b\n");

    // Plant hostile global-style env that the executor must not inherit.
    // The product builds a clean env; this test only verifies it still works
    // when the parent process has pager/prompt variables set.
    // SAFETY: process-global env mutation is test-only and sequential enough
    // for this integration suite (each test uses unique temps).
    unsafe {
        std::env::set_var("GIT_TERMINAL_PROMPT", "1");
        std::env::set_var("GIT_PAGER", "more");
        std::env::set_var("PAGER", "more");
    }

    let snap = fx
        .executor()
        .status(&fx.root, &root_id(), 1)
        .expect("status under hostile parent env");
    assert!(!snap.entries.is_empty());

    let diff = fx
        .executor()
        .diff(&fx.root, &root_id(), "a.txt", 1)
        .expect("diff");
    assert_eq!(diff.generation, 1);
    assert!(matches!(diff.view, GitDiffView::Text { .. }));

    unsafe {
        std::env::remove_var("GIT_TERMINAL_PROMPT");
        std::env::remove_var("GIT_PAGER");
        std::env::remove_var("PAGER");
    }
}

#[test]
fn unsafe_hook_filter_textconv_and_executable_config_are_denied() {
    // Hook
    {
        let fx = Fixture::project("unsafe-hook");
        let repo = fx.init_repo(".");
        fx.write(&repo, "a.txt", b"a\n");
        fx.commit_all(&repo, "init");
        let hook = repo.join(".git").join("hooks").join("pre-commit");
        fs::write(&hook, b"#!/bin/sh\necho pwned\n").unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mut perms = fs::metadata(&hook).unwrap().permissions();
            perms.set_mode(0o755);
            fs::set_permissions(&hook, perms).unwrap();
        }
        let err = fx
            .executor()
            .status(&fx.root, &root_id(), 1)
            .expect_err("hook must deny");
        match err {
            GitReadError::UnsafeConfiguration { kind } => {
                assert_eq!(kind, UnsafeConfigKind::Hook);
            }
            other => panic!("expected hook denial, got {other:?}"),
        }
    }

    // Filter config
    {
        let fx = Fixture::project("unsafe-filter");
        let repo = fx.init_repo(".");
        fx.write(&repo, "a.txt", b"a\n");
        fx.commit_all(&repo, "init");
        fx.git(&repo, &["config", "filter.evil.clean", "evil-clean-tool"]);
        let err = fx
            .executor()
            .status(&fx.root, &root_id(), 1)
            .expect_err("filter must deny");
        match err {
            GitReadError::UnsafeConfiguration { kind } => {
                assert_eq!(kind, UnsafeConfigKind::Filter);
            }
            other => panic!("expected filter denial, got {other:?}"),
        }
    }

    // Textconv config
    {
        let fx = Fixture::project("unsafe-textconv");
        let repo = fx.init_repo(".");
        fx.write(&repo, "a.txt", b"a\n");
        fx.commit_all(&repo, "init");
        fx.git(
            &repo,
            &["config", "diff.evil.textconv", "evil-textconv-tool"],
        );
        let err = fx
            .executor()
            .diff(&fx.root, &root_id(), "a.txt", 1)
            .expect_err("textconv must deny");
        match err {
            GitReadError::UnsafeConfiguration { kind } => {
                assert_eq!(kind, UnsafeConfigKind::Textconv);
            }
            other => panic!("expected textconv denial, got {other:?}"),
        }
    }

    // Executable-related config (fsmonitor / external diff / sshCommand)
    {
        let fx = Fixture::project("unsafe-exec-cfg");
        let repo = fx.init_repo(".");
        fx.write(&repo, "a.txt", b"a\n");
        fx.commit_all(&repo, "init");
        fx.git(&repo, &["config", "core.fsmonitor", "evil-fsmonitor"]);
        let err = fx
            .executor()
            .status(&fx.root, &root_id(), 1)
            .expect_err("fsmonitor must deny");
        match err {
            GitReadError::UnsafeConfiguration { kind } => {
                assert!(
                    matches!(
                        kind,
                        UnsafeConfigKind::Fsmonitor | UnsafeConfigKind::ExecutableConfig
                    ),
                    "kind={kind:?}"
                );
            }
            other => panic!("expected executable-config denial, got {other:?}"),
        }
    }
}

/// Real fixture: `git config --local --list --null` wire is `key\nvalue\0`.
/// Unsafe include / filter / textconv must be detected from that form.
#[test]
fn real_config_null_wire_detects_include_filter_textconv() {
    let fx = Fixture::project("config-null-wire");
    let repo = fx.init_repo(".");
    fx.write(&repo, "a.txt", b"a\n");
    fx.commit_all(&repo, "init");

    // Capture raw wire bytes from host git to prove the fixture shape.
    let raw = Command::new(&fx.git)
        .args(["config", "--local", "--list", "--null"])
        .current_dir(&repo)
        .env("GIT_TERMINAL_PROMPT", "0")
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .output()
        .expect("config list");
    assert!(raw.status.success());
    // Safe baseline uses key\nvalue\0 (newline separator, not '=').
    assert!(
        raw.stdout.windows(2).any(|w| w == b"\n0") || raw.stdout.contains(&b'\n'),
        "expected key\\nvalue wire, got hex-ish len={}",
        raw.stdout.len()
    );
    assert!(
        !String::from_utf8_lossy(&raw.stdout).contains("filter.evil.clean="),
        "wire must not be key=value for this assertion baseline"
    );

    // Plant unsafe include (external config) — real Git records as include.path\nvalue\0.
    fx.git(&repo, &["config", "include.path", "../outside.inc"]);
    let err = fx
        .executor()
        .status(&fx.root, &root_id(), 1)
        .expect_err("include must deny");
    match err {
        GitReadError::UnsafeConfiguration { kind } => {
            assert_eq!(kind, UnsafeConfigKind::Include, "kind={kind:?}");
        }
        other => panic!("expected include denial, got {other:?}"),
    }

    // Fresh repo: filter via real null list.
    let fx2 = Fixture::project("config-null-filter");
    let repo2 = fx2.init_repo(".");
    fx2.write(&repo2, "a.txt", b"a\n");
    fx2.commit_all(&repo2, "init");
    fx2.git(&repo2, &["config", "filter.evil.clean", "evil-clean-tool"]);
    // Prove wire form contains key\nvalue not key=value.
    let raw2 = Command::new(&fx2.git)
        .args(["config", "--local", "--list", "--null"])
        .current_dir(&repo2)
        .env("GIT_TERMINAL_PROMPT", "0")
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .output()
        .expect("config list");
    let as_str = String::from_utf8_lossy(&raw2.stdout);
    assert!(
        as_str.contains("filter.evil.clean\nevil-clean-tool")
            || raw2
                .stdout
                .windows(b"filter.evil.clean\nevil-clean-tool".len())
                .any(|w| { w == b"filter.evil.clean\nevil-clean-tool" }),
        "expected key\\nvalue in null list; lossy={as_str:?}"
    );
    let err2 = fx2
        .executor()
        .status(&fx2.root, &root_id(), 1)
        .expect_err("filter must deny via null wire");
    assert!(
        matches!(
            err2,
            GitReadError::UnsafeConfiguration {
                kind: UnsafeConfigKind::Filter
            }
        ),
        "got {err2:?}"
    );
}

// ---------------------------------------------------------------------------
// Containment: symlink / junction / outside gitdir / linked attributes
// ---------------------------------------------------------------------------

#[cfg(windows)]
fn win_path(p: &Path) -> String {
    // mklink treats `/Users/...` as a switch when given forward slashes.
    fs::canonicalize(p)
        .map(|c| c.to_string_lossy().replace('/', "\\"))
        .unwrap_or_else(|_| p.to_string_lossy().replace('/', "\\"))
}

#[cfg(windows)]
fn create_junction(link: &Path, target: &Path) -> bool {
    let _ = fs::remove_dir(link);
    if let Some(parent) = link.parent() {
        let _ = fs::create_dir_all(parent);
    }
    let link_s = link.to_string_lossy().replace('/', "\\");
    let target_s = win_path(target);
    let status = Command::new("cmd")
        .args(["/C", "mklink", "/J", &link_s, &target_s])
        .status();
    matches!(status, Ok(s) if s.success())
}

#[cfg(windows)]
fn create_file_symlink(link: &Path, target: &Path) -> bool {
    let _ = fs::remove_file(link);
    if let Some(parent) = link.parent() {
        let _ = fs::create_dir_all(parent);
    }
    // Normalize to backslashes — forward-slash absolute paths break mklink.
    let link_s = link.to_string_lossy().replace('/', "\\");
    let target_s = win_path(target);
    let status = Command::new("cmd")
        .args(["/C", "mklink", &link_s, &target_s])
        .status();
    matches!(status, Ok(s) if s.success())
}

#[cfg(unix)]
fn create_file_symlink(link: &Path, target: &Path) -> bool {
    let _ = fs::remove_file(link);
    std::os::unix::fs::symlink(target, link).is_ok()
}

#[test]
fn untracked_diff_through_symlink_cannot_read_outside() {
    let fx = Fixture::project("symlink-escape");
    let repo = fx.init_repo(".");
    fx.write(&repo, "safe.txt", b"safe\n");
    fx.commit_all(&repo, "init");

    // Outside secret next to project.
    let outside = temp_root("secret-outside");
    let secret = outside.join("secret.txt");
    fs::write(&secret, b"TOP-SECRET-OUTSIDE-CONTENT\n").unwrap();

    let link = repo.join("escape.txt");
    if !create_file_symlink(&link, &secret) {
        let _ = fs::remove_dir_all(&outside);
        // Symlink creation may require privileges; skip soft if unavailable.
        eprintln!("skip: file symlink creation not permitted on this host");
        return;
    }

    let err = fx
        .executor()
        .diff(&fx.root, &root_id(), "escape.txt", 1)
        .expect_err("symlink untracked diff must not read outside");
    assert!(
        matches!(
            err,
            GitReadError::PathRejected
                | GitReadError::NotARepository
                | GitReadError::CommandFailed { .. }
        ),
        "got {err:?}"
    );

    // Ensure secret content never appears via a successful text view.
    // (err path already; double-check no accidental success)
    let _ = fs::remove_file(&link);
    let _ = fs::remove_dir_all(&outside);
}

#[test]
#[cfg(windows)]
fn untracked_diff_through_junction_cannot_read_outside() {
    let fx = Fixture::project("junction-escape");
    let repo = fx.init_repo(".");
    fx.write(&repo, "safe.txt", b"safe\n");
    fx.commit_all(&repo, "init");

    let outside = temp_root("junction-secret");
    fs::write(outside.join("secret.txt"), b"JUNCTION-SECRET-OUTSIDE\n").unwrap();

    // Junction directory inside repo pointing outside; file path via junction.
    let link_dir = repo.join("escapedir");
    if !create_junction(&link_dir, &outside) {
        let _ = fs::remove_dir_all(&outside);
        panic!("mklink /J failed — junction required for this Windows containment test");
    }

    // Path uses junction component: escapedir/secret.txt
    let err = fx
        .executor()
        .diff(&fx.root, &root_id(), "escapedir/secret.txt", 1)
        .expect_err("junction path must be rejected");
    assert!(
        matches!(err, GitReadError::PathRejected | GitReadError::RepoNotFound),
        "got {err:?}"
    );

    let _ = fs::remove_dir(&link_dir); // junction
    let _ = fs::remove_dir_all(&outside);
}

#[test]
#[cfg(windows)]
fn discovery_cannot_traverse_project_junction_to_outside_repo() {
    let fx = Fixture::project("disc-junction");
    let root = fx.init_repo(".");
    fx.write(&root, "root.txt", b"root\n");
    fx.commit_all(&root, "root");

    // Outside real repository.
    let outside = temp_root("outside-repo");
    let out_repo = outside.join("hidden");
    fs::create_dir_all(&out_repo).unwrap();
    let out_fx_git = host_git();
    let init = Command::new(&out_fx_git)
        .args(["init", "-b", "main"])
        .current_dir(&out_repo)
        .env("GIT_TERMINAL_PROMPT", "0")
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .output()
        .expect("init outside");
    assert!(init.status.success());
    fs::write(out_repo.join("secret-repo.txt"), b"outside-repo\n").unwrap();
    let _ = Command::new(&out_fx_git)
        .args(["-C", out_repo.to_str().unwrap(), "config", "user.name", "x"])
        .output();
    let _ = Command::new(&out_fx_git)
        .args([
            "-C",
            out_repo.to_str().unwrap(),
            "config",
            "user.email",
            "x@x",
        ])
        .output();
    let _ = Command::new(&out_fx_git)
        .args(["-C", out_repo.to_str().unwrap(), "add", "-A"])
        .output();
    let _ = Command::new(&out_fx_git)
        .args([
            "-C",
            out_repo.to_str().unwrap(),
            "commit",
            "-m",
            "out",
            "--allow-empty",
        ])
        .env("GIT_AUTHOR_NAME", "x")
        .env("GIT_AUTHOR_EMAIL", "x@x")
        .env("GIT_COMMITTER_NAME", "x")
        .env("GIT_COMMITTER_EMAIL", "x@x")
        .output();

    let junction = fx.root.join("portal");
    if !create_junction(&junction, &out_repo) {
        let _ = fs::remove_dir_all(&outside);
        panic!("mklink /J failed for discovery junction test");
    }

    let disc = fx.executor().discover(&fx.root).expect("discover");
    // Must not surface the outside repo via the junction.
    for r in &disc.repositories {
        assert!(
            !r.project_relative.contains("portal"),
            "junction-traversed repo leaked: {:?}",
            disc.repositories
        );
        assert!(
            !r.project_relative.contains("secret-repo"),
            "outside path leaked: {:?}",
            disc.repositories
        );
    }
    assert!(
        disc.repositories
            .iter()
            .any(|r| r.project_relative == "." && r.available),
        "project root should remain: {:?}",
        disc.repositories
    );

    let _ = fs::remove_dir(&junction);
    let _ = fs::remove_dir_all(&outside);
}

#[test]
fn gitfile_resolving_outside_project_is_unavailable() {
    // Main repo lives *outside* the project; a linked worktree is placed
    // *inside* the project so its .git file points at an outside gitdir.
    let outside_main = temp_root("main-outside");
    let git = host_git();
    let init = Command::new(&git)
        .args(["init", "-b", "main"])
        .current_dir(&outside_main)
        .env("GIT_TERMINAL_PROMPT", "0")
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .output()
        .expect("init");
    assert!(init.status.success());
    fs::write(outside_main.join("a.txt"), b"a\n").unwrap();
    let _ = Command::new(&git)
        .args(["config", "user.name", "Pillow Fixture"])
        .current_dir(&outside_main)
        .output();
    let _ = Command::new(&git)
        .args(["config", "user.email", "fixture@pillow.local"])
        .current_dir(&outside_main)
        .output();
    let _ = Command::new(&git)
        .args(["add", "-A"])
        .current_dir(&outside_main)
        .output();
    let commit = Command::new(&git)
        .args(["commit", "-m", "init"])
        .current_dir(&outside_main)
        .env("GIT_AUTHOR_NAME", "Pillow Fixture")
        .env("GIT_AUTHOR_EMAIL", "fixture@pillow.local")
        .env("GIT_COMMITTER_NAME", "Pillow Fixture")
        .env("GIT_COMMITTER_EMAIL", "fixture@pillow.local")
        .output()
        .expect("commit");
    assert!(commit.status.success(), "commit failed");

    let fx = Fixture::project("gitfile-out");
    let linked = fx.root.join("linked");
    let add = Command::new(&git)
        .args(["worktree", "add", "--detach", linked.to_str().unwrap()])
        .current_dir(&outside_main)
        .env("GIT_TERMINAL_PROMPT", "0")
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .output()
        .expect("worktree add");
    if !add.status.success() {
        let _ = fs::remove_dir_all(&outside_main);
        panic!(
            "worktree add failed: {}",
            String::from_utf8_lossy(&add.stderr)
        );
    }

    // .git file should point outside the project.
    let gitfile = fs::read_to_string(linked.join(".git")).expect("gitfile");
    assert!(
        gitfile.contains("gitdir:"),
        "expected gitfile pointer, got {gitfile}"
    );

    let disc = fx.executor().discover(&fx.root).expect("discover");
    let linked_entry = disc
        .repositories
        .iter()
        .find(|r| r.project_relative == "linked");
    assert!(
        linked_entry.is_some(),
        "expected linked worktree identity, got {:?}",
        disc.repositories
    );
    assert!(
        !linked_entry.unwrap().available,
        "outside gitdir must be unavailable: {:?}",
        disc.repositories
    );

    // Direct status/diff must also fail closed.
    let err = fx
        .executor()
        .status(
            &fx.root,
            &GitRepositoryId {
                project_relative: "linked".to_string(),
            },
            1,
        )
        .expect_err("outside gitdir status");
    assert!(
        matches!(
            err,
            GitReadError::LinkedWorktreeOutsideProject
                | GitReadError::RepoOutsideProject
                | GitReadError::PathRejected
        ),
        "got {err:?}"
    );

    let _ = Command::new(&git)
        .args(["worktree", "remove", "--force", linked.to_str().unwrap()])
        .current_dir(&outside_main)
        .output();
    let _ = fs::remove_dir_all(&outside_main);
}

#[test]
fn internal_linked_worktree_info_attributes_are_inspected() {
    // Main + linked worktree both inside the project boundary.
    let fx = Fixture::project("internal-wt-attr");
    let main = fx.init_repo(".");
    fx.write(&main, "a.txt", b"a\n");
    fx.commit_all(&main, "init");

    let linked = fx.root.join("wt");
    fx.git(
        &main,
        &["worktree", "add", "--detach", linked.to_str().unwrap()],
    );

    // Plant filter driver in common info/attributes (main .git/info/attributes).
    let attrs = main.join(".git").join("info").join("attributes");
    if let Some(parent) = attrs.parent() {
        fs::create_dir_all(parent).unwrap();
    }
    fs::write(&attrs, b"* filter=evil\n").unwrap();

    // Status against the linked worktree must see common info/attributes.
    let err = fx
        .executor()
        .status(
            &fx.root,
            &GitRepositoryId {
                project_relative: "wt".to_string(),
            },
            1,
        )
        .expect_err("info/attributes filter must deny on linked worktree");
    match err {
        GitReadError::UnsafeConfiguration { kind } => {
            assert_eq!(kind, UnsafeConfigKind::Filter);
        }
        other => panic!("expected filter denial from info/attributes, got {other:?}"),
    }

    // Discovery should mark the linked worktree unavailable (unsafe config).
    let disc = fx.executor().discover(&fx.root).expect("discover");
    let wt = disc
        .repositories
        .iter()
        .find(|r| r.project_relative == "wt")
        .expect("wt present");
    assert!(!wt.available, "unsafe linked worktree must be unavailable");

    let _ = Command::new(&fx.git)
        .args(["worktree", "remove", "--force", linked.to_str().unwrap()])
        .current_dir(&main)
        .output();
}

// ---------------------------------------------------------------------------
// D2BR2: metadata no-follow (config / commondir / hooks / attributes)
// ---------------------------------------------------------------------------

/// Replace a regular file with a file symlink (or junction of a directory
/// host) pointing at an outside target. Returns false when the host cannot
/// create the link (caller may soft-skip).
fn replace_with_outside_file_link(link: &Path, outside_target: &Path) -> bool {
    let _ = fs::remove_file(link);
    create_file_symlink(link, outside_target)
}

#[test]
fn config_symlink_outside_project_is_rejected() {
    let fx = Fixture::project("cfg-symlink");
    let repo = fx.init_repo(".");
    fx.write(&repo, "a.txt", b"a\n");
    fx.commit_all(&repo, "init");

    let outside = temp_root("cfg-secret");
    let secret = outside.join("leaked-config");
    // Outside body must never be classified through a followed link.
    fs::write(
        &secret,
        b"[user]\n\tname = OUTSIDE-SECRET-CONFIG-BODY\n\temail = leak@out\n",
    )
    .unwrap();

    let config_path = repo.join(".git").join("config");
    // Keep a real config backup so git remains usable if link fails.
    let backup = fs::read(&config_path).expect("read config");
    if !replace_with_outside_file_link(&config_path, &secret) {
        fs::write(&config_path, backup).unwrap();
        let _ = fs::remove_dir_all(&outside);
        eprintln!("skip: config file symlink not permitted on this host");
        return;
    }

    let err = fx
        .executor()
        .status(&fx.root, &root_id(), 1)
        .expect_err("linked config must not be followed");
    assert!(
        matches!(
            err,
            GitReadError::PathRejected
                | GitReadError::NotARepository
                | GitReadError::MalformedConfiguration
                | GitReadError::UnsafeConfiguration { .. }
                | GitReadError::CommandFailed { .. }
        ),
        "got {err:?}"
    );
    // Fail-closed path: never surface outside secret as a success snapshot.
    // PathRejected is the preferred typed denial for reparse metadata.
    assert!(
        matches!(err, GitReadError::PathRejected)
            || matches!(err, GitReadError::UnsafeConfiguration { .. })
            || matches!(err, GitReadError::CommandFailed { .. })
            || matches!(err, GitReadError::NotARepository)
            || matches!(err, GitReadError::MalformedConfiguration),
        "unexpected success-shaped error {err:?}"
    );

    let _ = fs::remove_file(&config_path);
    let _ = fs::remove_dir_all(&outside);
}

#[test]
#[cfg(windows)]
fn config_worktree_junction_outside_is_rejected() {
    let fx = Fixture::project("cfg-wt-junc");
    let repo = fx.init_repo(".");
    fx.write(&repo, "a.txt", b"a\n");
    fx.commit_all(&repo, "init");

    // config.worktree as a junction is not representable as a file; plant a
    // file symlink to outside instead (Windows). Directory junction on
    // `.git` is covered elsewhere; here we target config.worktree file link.
    let outside = temp_root("cfg-wt-secret");
    let secret = outside.join("wt-config");
    fs::write(
        &secret,
        b"[include]\n\tpath = C:/windows/system32/drivers/etc/hosts\n",
    )
    .unwrap();

    let wt_cfg = repo.join(".git").join("config.worktree");
    if !create_file_symlink(&wt_cfg, &secret) {
        let _ = fs::remove_dir_all(&outside);
        eprintln!("skip: config.worktree symlink not permitted");
        return;
    }

    let err = fx
        .executor()
        .status(&fx.root, &root_id(), 1)
        .expect_err("config.worktree link must be rejected");
    assert!(
        matches!(err, GitReadError::PathRejected),
        "expected PathRejected for reparse config.worktree, got {err:?}"
    );

    let _ = fs::remove_file(&wt_cfg);
    let _ = fs::remove_dir_all(&outside);
}

#[test]
fn commondir_file_symlink_outside_is_rejected() {
    // Simulate a linked-worktree gitdir layout with a reparse `commondir`.
    let fx = Fixture::project("commondir-link");
    let main = fx.init_repo(".");
    fx.write(&main, "a.txt", b"a\n");
    fx.commit_all(&main, "init");

    let linked = fx.root.join("wt");
    fx.git(
        &main,
        &["worktree", "add", "--detach", linked.to_str().unwrap()],
    );

    // Linked gitdir lives under main/.git/worktrees/<name> and has commondir.
    let gitfile = fs::read_to_string(linked.join(".git")).expect("gitfile");
    let gitdir_raw = gitfile
        .trim()
        .strip_prefix("gitdir:")
        .expect("gitdir prefix")
        .trim();
    let gitdir = PathBuf::from(gitdir_raw);
    let commondir_path = gitdir.join("commondir");
    assert!(
        commondir_path.is_file(),
        "expected real commondir at {}",
        commondir_path.display()
    );

    let outside = temp_root("commondir-secret");
    let secret = outside.join("fake-commondir");
    // Body would point at an outside common dir if followed.
    fs::write(&secret, b"C:\\Windows\\Temp\\not-a-git\n").unwrap();

    let backup = fs::read(&commondir_path).unwrap();
    if !replace_with_outside_file_link(&commondir_path, &secret) {
        fs::write(&commondir_path, backup).unwrap();
        let _ = Command::new(&fx.git)
            .args(["worktree", "remove", "--force", linked.to_str().unwrap()])
            .current_dir(&main)
            .output();
        let _ = fs::remove_dir_all(&outside);
        eprintln!("skip: commondir symlink not permitted");
        return;
    }

    let err = fx
        .executor()
        .status(
            &fx.root,
            &GitRepositoryId {
                project_relative: "wt".to_string(),
            },
            1,
        )
        .expect_err("reparse commondir must fail closed");
    assert!(
        matches!(
            err,
            GitReadError::PathRejected
                | GitReadError::LinkedWorktreeOutsideProject
                | GitReadError::RepoOutsideProject
                | GitReadError::NotARepository
        ),
        "got {err:?}"
    );

    let _ = fs::remove_file(&commondir_path);
    fs::write(&commondir_path, backup).ok();
    let _ = Command::new(&fx.git)
        .args(["worktree", "remove", "--force", linked.to_str().unwrap()])
        .current_dir(&main)
        .output();
    let _ = fs::remove_dir_all(&outside);
}

#[test]
#[cfg(windows)]
fn hooks_directory_junction_outside_is_rejected() {
    let fx = Fixture::project("hooks-junc");
    let repo = fx.init_repo(".");
    fx.write(&repo, "a.txt", b"a\n");
    fx.commit_all(&repo, "init");

    let outside = temp_root("hooks-outside");
    fs::create_dir_all(&outside).unwrap();
    fs::write(outside.join("pre-commit"), b"#!/bin/sh\necho pwned\n").unwrap();

    let hooks = repo.join(".git").join("hooks");
    // Replace hooks dir with a junction to outside.
    let _ = fs::remove_dir_all(&hooks);
    if !create_junction(&hooks, &outside) {
        let _ = fs::remove_dir_all(&outside);
        panic!("mklink /J failed for hooks junction test");
    }

    let err = fx
        .executor()
        .status(&fx.root, &root_id(), 1)
        .expect_err("hooks junction must be rejected");
    assert!(
        matches!(
            err,
            GitReadError::PathRejected | GitReadError::UnsafeConfiguration { .. }
        ),
        "got {err:?}"
    );

    let _ = fs::remove_dir(&hooks);
    let _ = fs::remove_dir_all(&outside);
}

#[test]
fn hooks_entry_symlink_outside_is_denied() {
    let fx = Fixture::project("hook-entry-link");
    let repo = fx.init_repo(".");
    fx.write(&repo, "a.txt", b"a\n");
    fx.commit_all(&repo, "init");

    let outside = temp_root("hook-secret");
    let secret_hook = outside.join("evil-hook");
    fs::write(&secret_hook, b"#!/bin/sh\necho OUTSIDE-HOOK\n").unwrap();

    let hook = repo.join(".git").join("hooks").join("pre-commit");
    if !create_file_symlink(&hook, &secret_hook) {
        // Fall back: plant a regular active hook so the suite still proves
        // hook denial when symlink privilege is missing.
        fs::write(&hook, b"#!/bin/sh\necho local\n").unwrap();
        let err = fx
            .executor()
            .status(&fx.root, &root_id(), 1)
            .expect_err("active hook must deny");
        assert!(
            matches!(
                err,
                GitReadError::UnsafeConfiguration {
                    kind: UnsafeConfigKind::Hook
                }
            ),
            "got {err:?}"
        );
        let _ = fs::remove_file(&hook);
        let _ = fs::remove_dir_all(&outside);
        return;
    }

    let err = fx
        .executor()
        .status(&fx.root, &root_id(), 1)
        .expect_err("linked hook entry must deny");
    assert!(
        matches!(
            err,
            GitReadError::UnsafeConfiguration {
                kind: UnsafeConfigKind::Hook
            } | GitReadError::PathRejected
        ),
        "got {err:?}"
    );

    let _ = fs::remove_file(&hook);
    let _ = fs::remove_dir_all(&outside);
}

#[test]
fn worktree_gitattributes_symlink_outside_is_rejected() {
    let fx = Fixture::project("attr-symlink");
    let repo = fx.init_repo(".");
    fx.write(&repo, "a.txt", b"a\n");
    fx.commit_all(&repo, "init");

    let outside = temp_root("attr-secret");
    let secret = outside.join("evil.attributes");
    fs::write(&secret, b"* filter=OUTSIDE-ATTR-FILTER\n").unwrap();

    let attrs = repo.join(".gitattributes");
    if !create_file_symlink(&attrs, &secret) {
        let _ = fs::remove_dir_all(&outside);
        eprintln!("skip: .gitattributes symlink not permitted");
        return;
    }

    let err = fx
        .executor()
        .status(&fx.root, &root_id(), 1)
        .expect_err("linked .gitattributes must not be followed");
    assert!(
        matches!(err, GitReadError::PathRejected),
        "expected PathRejected, got {err:?}"
    );

    let _ = fs::remove_file(&attrs);
    let _ = fs::remove_dir_all(&outside);
}

#[test]
fn gitdir_info_attributes_symlink_outside_is_rejected() {
    let fx = Fixture::project("info-attr-link");
    let repo = fx.init_repo(".");
    fx.write(&repo, "a.txt", b"a\n");
    fx.commit_all(&repo, "init");

    let outside = temp_root("info-attr-secret");
    let secret = outside.join("attributes");
    fs::write(&secret, b"* filter=OUTSIDE-INFO-FILTER\n").unwrap();

    let info = repo.join(".git").join("info");
    fs::create_dir_all(&info).unwrap();
    let attrs = info.join("attributes");
    if !create_file_symlink(&attrs, &secret) {
        let _ = fs::remove_dir_all(&outside);
        eprintln!("skip: info/attributes symlink not permitted");
        return;
    }

    let err = fx
        .executor()
        .status(&fx.root, &root_id(), 1)
        .expect_err("linked info/attributes must not be followed");
    assert!(
        matches!(err, GitReadError::PathRejected),
        "expected PathRejected, got {err:?}"
    );

    let _ = fs::remove_file(&attrs);
    let _ = fs::remove_dir_all(&outside);
}

/// Whitespace-separated include (no `=`) must be denied by raw config scan
/// before Git would follow an outside include file.
#[test]
fn whitespace_include_without_equals_denied_before_outside_read() {
    let fx = Fixture::project("ws-include");
    let repo = fx.init_repo(".");
    fx.write(&repo, "a.txt", b"a\n");
    fx.commit_all(&repo, "init");

    let outside = temp_root("include-target");
    let inc = outside.join("pwn.inc");
    // Canary: if Git or our reader followed the include, this marker would
    // appear in config-derived state. We assert denial without reading it.
    fs::write(
        &inc,
        b"[user]\n\tname = INCLUDE-CANARY-MUST-NOT-LOAD\n\temail = canary@out\n",
    )
    .unwrap();

    // Plant whitespace-separated include without `=` into local config.
    let config_path = repo.join(".git").join("config");
    let mut cfg = fs::read_to_string(&config_path).expect("config");
    // Relative path from .git/config to the outside include.
    let rel = pathdiff_to_outside(&config_path, &inc);
    cfg.push_str(&format!("\n[include]\n\tpath {rel}\n"));
    fs::write(&config_path, cfg.as_bytes()).unwrap();

    let err = fx
        .executor()
        .status(&fx.root, &root_id(), 1)
        .expect_err("whitespace include must deny");
    match err {
        GitReadError::UnsafeConfiguration {
            kind: UnsafeConfigKind::Include,
        } => {}
        // Malformed is also fail-closed if a stricter parser rejects the line,
        // but Include is the preferred classification for this shape.
        GitReadError::MalformedConfiguration => {}
        other => panic!("expected Include or Malformed denial, got {other:?}"),
    }

    let _ = fs::remove_dir_all(&outside);
}

/// Oversized and invalid-UTF-8 metadata files fail closed.
#[test]
fn oversized_and_invalid_utf8_metadata_fail_closed() {
    // Oversized config
    {
        let fx = Fixture::project("meta-oversize");
        let repo = fx.init_repo(".");
        fx.write(&repo, "a.txt", b"a\n");
        fx.commit_all(&repo, "init");
        let config_path = repo.join(".git").join("config");
        let mut body = fs::read(&config_path).unwrap();
        // Exceed METADATA_FILE_BYTE_CAP (256 KiB).
        body.extend(std::iter::repeat_n(b'#', 300 * 1024));
        fs::write(&config_path, &body).unwrap();
        let err = fx
            .executor()
            .status(&fx.root, &root_id(), 1)
            .expect_err("oversized config");
        assert!(
            matches!(
                err,
                GitReadError::MalformedConfiguration
                    | GitReadError::OutputUnreliable
                    | GitReadError::CommandFailed { .. }
                    | GitReadError::PathRejected
            ),
            "got {err:?}"
        );
    }

    // Invalid UTF-8 in .gitattributes
    {
        let fx = Fixture::project("meta-bad-utf8");
        let repo = fx.init_repo(".");
        fx.write(&repo, "a.txt", b"a\n");
        fx.commit_all(&repo, "init");
        let attrs = repo.join(".gitattributes");
        fs::write(&attrs, b"* text\n\x80\xFF invalid\n").unwrap();
        let err = fx
            .executor()
            .status(&fx.root, &root_id(), 1)
            .expect_err("invalid utf8 attributes");
        assert!(
            matches!(
                err,
                GitReadError::Parse(_)
                    | GitReadError::MalformedConfiguration
                    | GitReadError::PathRejected
            ),
            "got {err:?}"
        );
    }
}

/// Absolute portable path for fixture include lines (not product code).
fn pathdiff_to_outside(_from_file: &Path, to_file: &Path) -> String {
    let to_canon = fs::canonicalize(to_file).unwrap_or_else(|_| to_file.to_path_buf());
    to_canon.to_string_lossy().replace('\\', "/")
}

// ---------------------------------------------------------------------------
// Resolution / process failures / no mutation / no argv API
// ---------------------------------------------------------------------------

#[test]
fn trusted_executable_resolution_failure_is_typed() {
    let missing = PathBuf::from(r"C:\pillow-missing-git-not-real\git.exe");
    let err =
        GitReadExecutor::new(missing, Duration::from_secs(5)).expect_err("missing executable");
    assert!(
        matches!(err, GitReadError::ExecutableUnavailable),
        "got {err:?}"
    );
}

#[test]
fn typed_process_failure_for_missing_repo_path() {
    let fx = Fixture::project("missing-repo");
    // Project exists but is not a git repo; status must fail typed.
    fs::write(fx.root.join("x.txt"), b"x").unwrap();
    let err = fx
        .executor()
        .status(&fx.root, &root_id(), 1)
        .expect_err("not a repo");
    assert!(
        matches!(
            err,
            GitReadError::NotARepository | GitReadError::RepoNotFound
        ),
        "got {err:?}"
    );
}

#[test]
fn read_ops_do_not_mutate_worktree() {
    let fx = Fixture::project("no-mutate");
    let repo = fx.init_repo(".");
    fx.write(&repo, "keep.txt", b"stable-content\n");
    fx.commit_all(&repo, "init");
    fx.write(&repo, "keep.txt", b"dirty-but-preserved\n");
    fx.write(&repo, "extra.txt", b"untracked\n");

    let before_keep = fs::read(repo.join("keep.txt")).unwrap();
    let before_extra = fs::read(repo.join("extra.txt")).unwrap();
    let before_head = fx.git(&repo, &["rev-parse", "HEAD"]).stdout;

    let ex = fx.executor();
    let _ = ex.discover(&fx.root).expect("discover");
    let _ = ex.status(&fx.root, &root_id(), 1).expect("status");
    let _ = ex.diff(&fx.root, &root_id(), "keep.txt", 1).expect("diff");

    assert_eq!(fs::read(repo.join("keep.txt")).unwrap(), before_keep);
    assert_eq!(fs::read(repo.join("extra.txt")).unwrap(), before_extra);
    assert_eq!(fx.git(&repo, &["rev-parse", "HEAD"]).stdout, before_head);

    // Index must remain clean of accidental stage from read path.
    let staged = fx.git(&repo, &["diff", "--cached", "--name-only"]);
    assert!(
        staged.stdout.is_empty(),
        "read path staged files: {}",
        String::from_utf8_lossy(&staged.stdout)
    );
}

#[test]
fn no_public_arbitrary_git_argv_surface() {
    // API-surface guard: every callable Git operation has a semantic signature.
    // There is no free-form argv, flag/config collection, cwd, environment, or
    // raw command runner in any method pointer below.
    type DiscoverFn =
        fn(&GitReadExecutor, &Path) -> Result<keeper_git_executor::GitDiscovery, GitReadError>;
    type StatusFn = fn(
        &GitReadExecutor,
        &Path,
        &GitRepositoryId,
        u64,
    ) -> Result<keeper_git_executor::GitStatusSnapshot, GitReadError>;
    type DiffFn = fn(
        &GitReadExecutor,
        &Path,
        &GitRepositoryId,
        &str,
        u64,
    ) -> Result<GitDiffSnapshot, GitReadError>;
    type PathsFn = fn(
        &GitReadExecutor,
        &Path,
        &GitRepositoryId,
        &[String],
    ) -> Result<keeper_git_executor::GitMutationReceipt, GitReadError>;
    type CommitFn = fn(
        &GitReadExecutor,
        &Path,
        &GitRepositoryId,
        &str,
    ) -> Result<keeper_git_executor::GitMutationReceipt, GitReadError>;
    type PushFn = fn(
        &GitReadExecutor,
        &Path,
        &GitRepositoryId,
        &str,
        &str,
    ) -> Result<keeper_git_executor::GitMutationReceipt, GitReadError>;

    let discover: DiscoverFn = GitReadExecutor::discover;
    let status: StatusFn = GitReadExecutor::status;
    let diff: DiffFn = GitReadExecutor::diff;
    let stage: PathsFn = GitReadExecutor::stage;
    let unstage: PathsFn = GitReadExecutor::unstage;
    let commit: CommitFn = GitReadExecutor::commit;
    let push: PushFn = GitReadExecutor::push;
    let _ = (discover, status, diff, stage, unstage, commit, push);

    let git = host_git();
    let ex = GitReadExecutor::new(git, Duration::from_secs(5)).expect("executor");
    // Smoke: constructor + semantic ops exist; no argv parameter appears
    // in any of the function type signatures above.
    let empty = temp_root("api-surface");
    fs::create_dir_all(&empty).unwrap();
    let _ = ex.discover(&empty);
    let _ = fs::remove_dir_all(&empty);
}
