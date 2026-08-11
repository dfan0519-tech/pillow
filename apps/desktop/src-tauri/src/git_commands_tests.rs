//! Focused tests for native Git command authority.
//!
//! Product commands never touch the Pillow repository: fixtures live under
//! temporary directories and use a host-resolved Git only for setup.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use tempfile::TempDir;

use crate::app_state::AppState;
use crate::git_commands::{GitCommandState, GitPushConfirmationInput, is_git_public_error};

// ---------------------------------------------------------------------------
// Fixture helpers
// ---------------------------------------------------------------------------

fn which_git() -> PathBuf {
    if let Ok(p) = std::env::var("PILLOW_TEST_GIT") {
        let pb = PathBuf::from(p);
        if pb.is_file() {
            return pb;
        }
    }
    for c in [
        r"C:\Program Files\Git\cmd\git.exe",
        r"C:\Program Files (x86)\Git\cmd\git.exe",
    ] {
        let p = PathBuf::from(c);
        if p.is_file() {
            return p;
        }
    }
    if let Ok(path) = std::env::var("PATH") {
        for dir in std::env::split_paths(&path) {
            let p = dir.join("git.exe");
            if p.is_file() {
                return p;
            }
        }
    }
    panic!("host git required for git_commands_tests fixtures");
}

/// Owns the fixture tree; removing it is the guard's own `Drop`, never ours.
fn temp_root(label: &str) -> TempDir {
    tempfile::Builder::new()
        .prefix(&format!("pillow-git-cmd-{label}-"))
        .tempdir()
        .expect("temp root")
}

struct Harness {
    app: AppState,
    git: GitCommandState,
    project_id: String,
    project_root: PathBuf,
    host_git: PathBuf,
    /// Last field on purpose: fields drop in declaration order, so `app` closes
    /// its SQLite handles on `keeper.db` before the directory holding that file
    /// is removed. Windows refuses to delete a file that is still open, and a
    /// hand-written `impl Drop for Harness` runs *before* any field — which is
    /// how this fixture used to leak its whole tree on every run.
    temp: TempDir,
}

impl Harness {
    fn new(label: &str) -> Self {
        let host_git = which_git();
        let temp = temp_root(label);
        let project_root = temp.path().join("project");
        fs::create_dir_all(&project_root).expect("project root");
        let app = AppState::open(temp.path().join("keeper.db")).expect("app state");
        app.register_picker_selection("main", &project_root)
            .expect("picker");
        let project = app
            .create_project_from_picker_for_window(
                "main",
                label.to_owned(),
                project_root.to_string_lossy().into_owned(),
            )
            .expect("register project");
        let git = GitCommandState::with_git_executable(Some(host_git.clone()));
        Self {
            app,
            git,
            project_id: project.id,
            project_root,
            host_git,
            temp,
        }
    }

    fn unavailable_git(label: &str) -> Self {
        let temp = temp_root(label);
        let project_root = temp.path().join("project");
        fs::create_dir_all(&project_root).expect("project root");
        let app = AppState::open(temp.path().join("keeper.db")).expect("app state");
        app.register_picker_selection("main", &project_root)
            .expect("picker");
        let project = app
            .create_project_from_picker_for_window(
                "main",
                label.to_owned(),
                project_root.to_string_lossy().into_owned(),
            )
            .expect("register project");
        let git = GitCommandState::with_git_executable(Some(PathBuf::from(
            r"C:\pillow-missing-git-not-real\git.exe",
        )));
        Self {
            app,
            git,
            project_id: project.id,
            project_root,
            host_git: which_git(),
            temp,
        }
    }

    fn git_run(&self, cwd: &Path, args: &[&str]) {
        let out = Command::new(&self.host_git)
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
            .expect("fixture git");
        assert!(
            out.status.success(),
            "fixture git {:?} failed: {}",
            args,
            String::from_utf8_lossy(&out.stderr)
        );
    }

    fn init_repo(&self, rel: &str) -> PathBuf {
        let path = if rel == "." {
            self.project_root.clone()
        } else {
            let p = self.project_root.join(rel);
            fs::create_dir_all(&p).expect("repo dir");
            p
        };
        self.git_run(&path, &["init", "-b", "main"]);
        self.git_run(&path, &["config", "user.name", "Pillow Fixture"]);
        self.git_run(&path, &["config", "user.email", "fixture@pillow.local"]);
        path
    }

    fn write(&self, repo: &Path, rel: &str, body: &[u8]) {
        let path = repo.join(rel);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).expect("parent");
        }
        fs::write(&path, body).expect("write");
    }

    fn commit_all(&self, repo: &Path, msg: &str) {
        self.git_run(repo, &["add", "-A"]);
        self.git_run(repo, &["commit", "-m", msg]);
    }

    fn root(&self) -> &Path {
        self.temp.path()
    }
}

fn assert_path_free(err: &crate::app_state::CommandError) {
    assert!(
        is_git_public_error(err.code),
        "code must be public git_* : {}",
        err.code
    );
    assert!(
        !err.message.contains('\\'),
        "message must not contain backslash paths: {}",
        err.message
    );
    // Drive letters / absolute unix roots.
    assert!(
        !err.message.contains(":\\") && !err.message.contains(":/"),
        "message must not contain native path roots: {}",
        err.message
    );
    // No absolute POSIX or Windows-looking fragments from temp dirs.
    assert!(
        !err.message.to_ascii_lowercase().contains("temp")
            || !err.message.contains('/') && !err.message.contains('\\'),
        "message must stay path-free: {}",
        err.message
    );
}

// ---------------------------------------------------------------------------
// Fixture hygiene
// ---------------------------------------------------------------------------

/// The harness used to remove its tree from an `impl Drop for Harness`, which
/// runs before any field: `app` still held `keeper.db` open, Windows refused the
/// delete, and the discarded error left one directory behind per test, per gate
/// run. Removal now belongs to the `TempDir` field that drops last.
#[test]
fn dropping_the_harness_removes_its_temp_root() {
    let root = {
        let h = Harness::new("temp-hygiene");
        let root = h.root().to_path_buf();
        assert!(root.join("keeper.db").is_file(), "fixture db should exist");
        root
    };
    assert!(!root.exists(), "harness left {} behind", root.display());
}

// ---------------------------------------------------------------------------
// Registered / missing project
// ---------------------------------------------------------------------------

#[test]
fn missing_project_maps_to_git_project_not_found() {
    let h = Harness::new("missing-project");
    let err = h
        .git
        .discover(&h.app, "00000000-0000-0000-0000-000000000099")
        .expect_err("unknown project");
    assert_eq!(err.code, "git_project_not_found");
    assert_path_free(&err);
}

#[test]
fn registered_project_discovers_root_repository() {
    let h = Harness::new("discover-root");
    let repo = h.init_repo(".");
    h.write(&repo, "a.txt", b"hello\n");
    h.commit_all(&repo, "init");

    let disc = h.git.discover(&h.app, &h.project_id).expect("discover");
    assert_eq!(disc.generation, 1);
    assert_eq!(disc.repositories.len(), 1);
    assert_eq!(disc.repositories[0].repository_id, ".");
    assert!(disc.repositories[0].available);

    let json = serde_json::to_value(&disc).expect("serialize");
    assert!(json.get("generation").is_some());
    assert!(json.get("repositories").is_some());
    assert!(json.get("rootPath").is_none());
    assert!(json.get("executable").is_none());
}

// ---------------------------------------------------------------------------
// Host Git unavailable
// ---------------------------------------------------------------------------

#[test]
fn host_git_unavailable_maps_to_git_unavailable() {
    let h = Harness::unavailable_git("no-git");
    assert!(!h.git.git_available());
    let err = h
        .git
        .discover(&h.app, &h.project_id)
        .expect_err("missing git");
    assert_eq!(err.code, "git_unavailable");
    assert_path_free(&err);
}

// ---------------------------------------------------------------------------
// Discovery generation + stale / mismatch
// ---------------------------------------------------------------------------

#[test]
fn discovery_advances_generation_atomically() {
    let h = Harness::new("gen-advance");
    let repo = h.init_repo(".");
    h.write(&repo, "a.txt", b"v1\n");
    h.commit_all(&repo, "init");

    let d1 = h.git.discover(&h.app, &h.project_id).expect("d1");
    let d2 = h.git.discover(&h.app, &h.project_id).expect("d2");
    assert_eq!(d1.generation, 1);
    assert_eq!(d2.generation, 2);
    assert_eq!(h.git.latest_generation(&h.project_id), Some(2));
}

#[test]
fn status_rejects_stale_generation() {
    let h = Harness::new("stale-gen");
    let repo = h.init_repo(".");
    h.write(&repo, "a.txt", b"v1\n");
    h.commit_all(&repo, "init");

    let d1 = h.git.discover(&h.app, &h.project_id).expect("d1");
    let _d2 = h.git.discover(&h.app, &h.project_id).expect("d2");

    let err = h
        .git
        .status(&h.app, &h.project_id, ".", d1.generation)
        .expect_err("stale");
    assert_eq!(err.code, "git_generation_stale");
    assert_path_free(&err);
}

#[test]
fn status_rejects_future_generation_as_mismatch() {
    let h = Harness::new("future-gen");
    let repo = h.init_repo(".");
    h.write(&repo, "a.txt", b"v1\n");
    h.commit_all(&repo, "init");

    let d1 = h.git.discover(&h.app, &h.project_id).expect("d1");
    let err = h
        .git
        .status(&h.app, &h.project_id, ".", d1.generation + 10)
        .expect_err("future");
    assert_eq!(err.code, "git_generation_mismatch");
    assert_path_free(&err);
}

#[test]
fn status_before_discovery_is_stale() {
    let h = Harness::new("no-disc");
    let err = h
        .git
        .status(&h.app, &h.project_id, ".", 1)
        .expect_err("no discovery");
    assert_eq!(err.code, "git_generation_stale");
    assert_path_free(&err);
}

// ---------------------------------------------------------------------------
// Missing / unavailable repository
// ---------------------------------------------------------------------------

#[test]
fn missing_repository_id_is_not_found() {
    let h = Harness::new("missing-repo");
    let repo = h.init_repo(".");
    h.write(&repo, "a.txt", b"v1\n");
    h.commit_all(&repo, "init");

    let d = h.git.discover(&h.app, &h.project_id).expect("discover");
    let err = h
        .git
        .status(&h.app, &h.project_id, "not-a-repo", d.generation)
        .expect_err("missing repo");
    assert_eq!(err.code, "git_repository_not_found");
    assert_path_free(&err);
}

// ---------------------------------------------------------------------------
// Portable path rejection
// ---------------------------------------------------------------------------

#[test]
fn open_diff_rejects_native_and_traversal_paths() {
    let h = Harness::new("path-reject");
    let repo = h.init_repo(".");
    h.write(&repo, "a.txt", b"v1\n");
    h.commit_all(&repo, "init");
    h.write(&repo, "a.txt", b"v2\n");

    let d = h.git.discover(&h.app, &h.project_id).expect("discover");

    for bad in [
        r"C:\Windows\system.ini",
        r"..\secret",
        "/etc/passwd",
        "foo\\bar",
        "",
    ] {
        let err = h
            .git
            .diff(&h.app, &h.project_id, ".", d.generation, bad)
            .expect_err("bad path");
        assert_eq!(
            err.code, "git_path_rejected",
            "path {bad:?} should be rejected"
        );
        assert_path_free(&err);
    }
}

// ---------------------------------------------------------------------------
// Nested repositories
// ---------------------------------------------------------------------------

#[test]
fn nested_repositories_publish_portable_ids() {
    let h = Harness::new("nested");
    let root = h.init_repo(".");
    h.write(&root, "root.txt", b"root\n");
    h.commit_all(&root, "root init");

    let nested = h.init_repo("vendor/lib");
    h.write(&nested, "lib.txt", b"lib\n");
    h.commit_all(&nested, "nested init");

    let disc = h.git.discover(&h.app, &h.project_id).expect("discover");
    let ids: Vec<&str> = disc
        .repositories
        .iter()
        .map(|r| r.repository_id.as_str())
        .collect();
    assert!(ids.contains(&"."), "ids={ids:?}");
    assert!(ids.contains(&"vendor/lib"), "ids={ids:?}");
    for repo in &disc.repositories {
        assert!(
            !repo.repository_id.contains('\\') && !repo.repository_id.contains(':'),
            "portable only: {}",
            repo.repository_id
        );
        assert!(repo.available);
    }

    // Status against nested repo with current generation.
    let nested_status = h
        .git
        .status(&h.app, &h.project_id, "vendor/lib", disc.generation)
        .expect("nested status");
    assert_eq!(nested_status.repository_id, "vendor/lib");
    assert_eq!(nested_status.generation, disc.generation);
}

// ---------------------------------------------------------------------------
// Status / diff happy path + DTO shape
// ---------------------------------------------------------------------------

#[test]
fn status_and_diff_serialize_exact_gateway_fields() {
    let h = Harness::new("status-diff");
    let repo = h.init_repo(".");
    h.write(&repo, "tracked.txt", b"v1\n");
    h.commit_all(&repo, "init");
    h.write(&repo, "tracked.txt", b"v2\n");
    h.write(&repo, "new.txt", b"untracked\n");

    let disc = h.git.discover(&h.app, &h.project_id).expect("discover");
    let status = h
        .git
        .status(&h.app, &h.project_id, ".", disc.generation)
        .expect("status");

    assert_eq!(status.repository_id, ".");
    assert_eq!(status.generation, disc.generation);
    assert_eq!(status.branch.as_deref(), Some("main"));
    assert!(!status.detached);
    assert!(!status.head_short.is_empty());
    assert!(status.dirty_count >= 2);
    assert!(
        status
            .files
            .iter()
            .any(|f| f.path == "tracked.txt" && f.kind == "modified")
    );
    assert!(
        status
            .files
            .iter()
            .any(|f| f.path == "new.txt" && f.kind == "untracked")
    );

    let status_json = serde_json::to_value(&status).expect("status json");
    for key in [
        "repositoryId",
        "generation",
        "branch",
        "detached",
        "headShort",
        "dirtyCount",
        "files",
        "truncated",
    ] {
        assert!(status_json.get(key).is_some(), "missing {key}");
    }
    assert!(status_json.get("rootPath").is_none());
    assert!(status_json.get("argv").is_none());
    assert!(status_json.get("executable").is_none());

    let diff = h
        .git
        .diff(&h.app, &h.project_id, ".", disc.generation, "tracked.txt")
        .expect("diff");
    assert_eq!(diff.repository_id, ".");
    assert_eq!(diff.generation, disc.generation);
    assert_eq!(diff.path, "tracked.txt");
    assert_eq!(diff.kind, "text");
    assert!(diff.unified.as_ref().is_some_and(|u| !u.is_empty()));
    assert_eq!(
        diff.byte_count,
        Some(diff.unified.as_ref().unwrap().len() as u32)
    );

    let diff_json = serde_json::to_value(&diff).expect("diff json");
    for key in ["repositoryId", "generation", "path", "kind", "truncated"] {
        assert!(diff_json.get(key).is_some(), "missing {key}");
    }
    assert!(diff_json.get("unified").is_some());
    assert!(diff_json.get("lineCount").is_some());
    assert!(diff_json.get("byteCount").is_some());
    assert!(diff_json.get("nativePath").is_none());
}

// ---------------------------------------------------------------------------
// Path-free error serialization
// ---------------------------------------------------------------------------

#[test]
fn public_errors_serialize_without_paths_or_private_codes() {
    let codes = [
        "git_project_not_found",
        "git_path_rejected",
        "git_repository_not_found",
        "git_unavailable",
        "git_generation_stale",
        "git_generation_mismatch",
        "git_message_rejected",
        "git_branch_rejected",
        "git_remote_rejected",
        "git_push_confirmation_required",
        "git_mutation_failed",
    ];
    for code in codes {
        assert!(is_git_public_error(code));
    }
    assert!(!is_git_public_error("project_not_found"));
    assert!(!is_git_public_error("native_error"));

    let h = Harness::new("err-ser");
    let err = h.git.discover(&h.app, "not-a-uuid").expect_err("bad id");
    let json = serde_json::to_string(&err).expect("ser");
    assert!(json.contains("git_project_not_found"));
    assert!(!json.contains(&h.project_root.to_string_lossy().to_string()));
    assert!(!json.contains("\\\\"));
}

// ---------------------------------------------------------------------------
// Structured mutation commands
// ---------------------------------------------------------------------------

#[test]
fn stage_unstage_and_commit_mutate_only_the_registered_temporary_repository() {
    let h = Harness::new("mutation-positive");
    let repo = h.init_repo(".");
    h.write(&repo, "seed.txt", b"seed\n");
    h.commit_all(&repo, "seed");
    h.write(&repo, "change.txt", b"change\n");
    let discovery = h.git.discover(&h.app, &h.project_id).expect("discover");
    let paths = vec!["change.txt".to_owned()];

    let staged = h
        .git
        .stage(&h.app, &h.project_id, ".", discovery.generation, &paths)
        .expect("stage");
    assert_eq!(staged.operation, "stage");
    assert_eq!(staged.repository_id, ".");
    assert_eq!(staged.generation, discovery.generation);
    let staged_json = serde_json::to_value(&staged).expect("stage dto");
    let mut staged_keys: Vec<&str> = staged_json
        .as_object()
        .expect("stage object")
        .keys()
        .map(String::as_str)
        .collect();
    staged_keys.sort_unstable();
    assert_eq!(staged_keys, ["generation", "operation", "repositoryId"]);
    assert!(staged_json.get("argv").is_none());
    assert!(staged_json.get("nativePath").is_none());
    let cached = Command::new(&h.host_git)
        .args(["diff", "--cached", "--name-only", "--", "change.txt"])
        .current_dir(&repo)
        .output()
        .expect("cached diff");
    assert_eq!(String::from_utf8_lossy(&cached.stdout).trim(), "change.txt");

    let unstaged = h
        .git
        .unstage(&h.app, &h.project_id, ".", discovery.generation, &paths)
        .expect("unstage");
    assert_eq!(unstaged.operation, "unstage");
    assert!(
        repo.join("change.txt").is_file(),
        "worktree must be preserved"
    );

    h.git
        .stage(&h.app, &h.project_id, ".", discovery.generation, &paths)
        .expect("restage");
    let committed = h
        .git
        .commit(
            &h.app,
            &h.project_id,
            ".",
            discovery.generation,
            "command layer commit",
        )
        .expect("commit");
    assert_eq!(committed.operation, "commit");
    let subject = Command::new(&h.host_git)
        .args(["log", "-1", "--pretty=%s"])
        .current_dir(&repo)
        .output()
        .expect("log subject");
    assert_eq!(
        String::from_utf8_lossy(&subject.stdout).trim(),
        "command layer commit"
    );
}

#[test]
fn command_layer_maps_adversarial_mutation_inputs_to_opaque_errors() {
    let h = Harness::new("mutation-rejections");
    let repo = h.init_repo(".");
    h.write(&repo, "seed.txt", b"seed\n");
    h.commit_all(&repo, "seed");
    let discovery = h.git.discover(&h.app, &h.project_id).expect("discover");

    let path = h
        .git
        .stage(
            &h.app,
            &h.project_id,
            ".",
            discovery.generation,
            &["../escape".to_owned()],
        )
        .expect_err("traversal");
    assert_eq!(path.code, "git_path_rejected");
    assert_path_free(&path);

    let message = h
        .git
        .commit(
            &h.app,
            &h.project_id,
            ".",
            discovery.generation,
            "-pretend-flag",
        )
        .expect_err("message flag");
    assert_eq!(message.code, "git_message_rejected");
    assert_path_free(&message);

    let branch = h
        .git
        .push(
            &h.app,
            &h.project_id,
            ".",
            discovery.generation,
            &GitPushConfirmationInput {
                confirmed: true,
                remote_name: "origin".to_owned(),
                branch_name: "--upload-pack=malicious".to_owned(),
            },
        )
        .expect_err("branch flag");
    assert_eq!(branch.code, "git_branch_rejected");
    assert_path_free(&branch);
}

#[test]
fn push_requires_a_backend_checked_target_bound_confirmation() {
    let h = Harness::new("push-confirmation");
    let repo = h.init_repo(".");
    h.write(&repo, "push.txt", b"push\n");
    h.commit_all(&repo, "push seed");
    let remote = h.project_root.join("contained-remote.git");
    fs::create_dir_all(&remote).expect("remote directory");
    h.git_run(&remote, &["init", "--bare"]);
    let remote_text = remote.to_string_lossy().into_owned();
    h.git_run(&repo, &["remote", "add", "origin", &remote_text]);
    let discovery = h.git.discover(&h.app, &h.project_id).expect("discover");

    let denied = h
        .git
        .push(
            &h.app,
            &h.project_id,
            ".",
            discovery.generation,
            &GitPushConfirmationInput {
                confirmed: false,
                remote_name: "origin".to_owned(),
                branch_name: "main".to_owned(),
            },
        )
        .expect_err("confirmation is mandatory");
    assert_eq!(denied.code, "git_push_confirmation_required");
    assert_path_free(&denied);

    let pushed = h
        .git
        .push(
            &h.app,
            &h.project_id,
            ".",
            discovery.generation,
            &GitPushConfirmationInput {
                confirmed: true,
                remote_name: "origin".to_owned(),
                branch_name: "main".to_owned(),
            },
        )
        .expect("confirmed push");
    assert_eq!(pushed.operation, "push");
    assert_eq!(pushed.remote_name.as_deref(), Some("origin"));
    assert_eq!(pushed.branch_name.as_deref(), Some("main"));
    let pushed_json = serde_json::to_value(&pushed).expect("push dto");
    let mut pushed_keys: Vec<&str> = pushed_json
        .as_object()
        .expect("push object")
        .keys()
        .map(String::as_str)
        .collect();
    pushed_keys.sort_unstable();
    assert_eq!(
        pushed_keys,
        [
            "branchName",
            "generation",
            "operation",
            "remoteName",
            "repositoryId",
        ]
    );

    let unknown = serde_json::from_value::<GitPushConfirmationInput>(serde_json::json!({
        "confirmed": true,
        "remoteName": "origin",
        "branchName": "main",
        "argv": ["push"]
    }));
    assert!(unknown.is_err(), "push confirmation input must be closed");
}

// ---------------------------------------------------------------------------
// Command registration
// ---------------------------------------------------------------------------

#[test]
fn desktop_invoke_handler_registers_exact_git_contract_commands() {
    let source = include_str!("lib.rs");
    let handler = source
        .split("tauri::generate_handler![")
        .nth(1)
        .expect("desktop invoke handler must exist");
    for command in [
        "discover_git_repositories",
        "get_git_status",
        "open_git_diff",
        "stage_git_paths",
        "unstage_git_paths",
        "commit_git_changes",
        "push_git_branch",
    ] {
        assert!(
            handler.contains(command),
            "missing invoke command {command}"
        );
    }
    // Destructive and arbitrary mutation surfaces remain absent.
    for forbidden in [
        "discard_git_changes",
        "reset_git_hard",
        "git_checkout",
        "run_git",
    ] {
        assert!(
            !handler.contains(forbidden),
            "mutation command must not be registered: {forbidden}"
        );
    }
}

#[test]
fn managed_state_type_is_wired_in_lib() {
    let source = include_str!("lib.rs");
    assert!(
        source.contains("GitCommandState"),
        "lib.rs must manage GitCommandState"
    );
    assert!(
        source.contains("mod git_commands"),
        "lib.rs must declare git_commands module"
    );
}
