//! Real temporary-repository tests for the closed structured mutation surface.
//!
//! Fixture setup uses the host Git directly, but every operation under test
//! goes through `GitReadExecutor`; Pillow's own repository is never mutated.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use keeper_git_executor::{
    GitInputKind, GitMutationKind, GitReadError, GitReadExecutor, GitRepositoryId,
};

static TEMP_SEQUENCE: AtomicU64 = AtomicU64::new(0);

fn which_git() -> PathBuf {
    if let Ok(path) = std::env::var("PILLOW_TEST_GIT") {
        let path = PathBuf::from(path);
        if path.is_file() {
            return path;
        }
    }
    for candidate in [
        r"C:\Program Files\Git\cmd\git.exe",
        r"C:\Program Files (x86)\Git\cmd\git.exe",
    ] {
        let path = PathBuf::from(candidate);
        if path.is_file() {
            return path;
        }
    }
    if let Ok(path) = std::env::var("PATH") {
        for directory in std::env::split_paths(&path) {
            for executable in ["git.exe", "git"] {
                let candidate = directory.join(executable);
                if candidate.is_file() {
                    return candidate;
                }
            }
        }
    }
    panic!("host Git is required for mutation integration tests");
}

fn temp_root(label: &str) -> PathBuf {
    let sequence = TEMP_SEQUENCE.fetch_add(1, Ordering::Relaxed);
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_nanos())
        .unwrap_or(0);
    let root = std::env::temp_dir().join(format!("pillow-git-mutation-{label}-{nanos}-{sequence}"));
    fs::create_dir_all(&root).expect("create temporary project root");
    root
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
    fn new(label: &str) -> Self {
        let fixture = Self {
            root: temp_root(label),
            git: which_git(),
        };
        fixture.git_ok(&fixture.root, &["init", "-b", "main"]);
        fixture.git_ok(
            &fixture.root,
            &["config", "user.name", "Pillow Mutation Fixture"],
        );
        fixture.git_ok(
            &fixture.root,
            &["config", "user.email", "mutation@pillow.local"],
        );
        fixture
    }

    fn command(&self, cwd: &Path, args: &[&str]) -> Output {
        Command::new(&self.git)
            .args(args)
            .current_dir(cwd)
            .env_remove("GIT_DIR")
            .env_remove("GIT_WORK_TREE")
            .env("GIT_TERMINAL_PROMPT", "0")
            .env("GIT_CONFIG_NOSYSTEM", "1")
            .env("GIT_AUTHOR_NAME", "Pillow Mutation Fixture")
            .env("GIT_AUTHOR_EMAIL", "mutation@pillow.local")
            .env("GIT_COMMITTER_NAME", "Pillow Mutation Fixture")
            .env("GIT_COMMITTER_EMAIL", "mutation@pillow.local")
            .output()
            .expect("run fixture Git")
    }

    fn git_ok(&self, cwd: &Path, args: &[&str]) -> Output {
        let output = self.command(cwd, args);
        assert!(
            output.status.success(),
            "fixture Git {args:?} failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        output
    }

    fn write(&self, relative: &str, contents: &[u8]) {
        let path = self.root.join(relative);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).expect("create file parent");
        }
        fs::write(path, contents).expect("write fixture file");
    }

    fn seed_commit(&self) {
        self.write("seed.txt", b"seed\n");
        self.git_ok(&self.root, &["add", "--", "seed.txt"]);
        self.git_ok(&self.root, &["commit", "-m", "seed"]);
    }

    fn executor(&self) -> GitReadExecutor {
        GitReadExecutor::new(self.git.clone(), Duration::from_secs(30)).expect("Git executor")
    }
}

fn root_repository() -> GitRepositoryId {
    GitRepositoryId {
        project_relative: ".".to_owned(),
    }
}

#[cfg(windows)]
fn create_directory_link(link: &Path, target: &Path) -> bool {
    let link = link.to_string_lossy().replace('/', "\\");
    let target = fs::canonicalize(target)
        .map(|path| path.to_string_lossy().replace('/', "\\"))
        .unwrap_or_else(|_| target.to_string_lossy().replace('/', "\\"));
    Command::new("cmd")
        .args(["/C", "mklink", "/J", &link, &target])
        .status()
        .is_ok_and(|status| status.success())
}

#[cfg(unix)]
fn create_directory_link(link: &Path, target: &Path) -> bool {
    std::os::unix::fs::symlink(target, link).is_ok()
}

fn assert_input_rejected(
    result: Result<keeper_git_executor::GitMutationReceipt, GitReadError>,
    kind: GitInputKind,
) {
    assert!(
        matches!(result, Err(GitReadError::InputRejected { kind: actual }) if actual == kind),
        "expected {kind:?}, got {result:?}"
    );
}

#[test]
fn path_traversal_is_rejected_before_stage() {
    let fixture = Fixture::new("path-traversal");
    let paths = vec!["../outside.txt".to_owned()];
    let result = fixture
        .executor()
        .stage(&fixture.root, &root_repository(), &paths);
    assert!(matches!(result, Err(GitReadError::PathRejected)));
}

#[test]
fn stage_directory_rejects_a_descendant_reparse_entry() {
    let fixture = Fixture::new("descendant-reparse");
    fixture.seed_commit();
    fixture.write("target/outside.txt", b"must not be traversed\n");
    fs::create_dir_all(fixture.root.join("bundle")).expect("bundle directory");
    let link = fixture.root.join("bundle/portal");
    if !create_directory_link(&link, &fixture.root.join("target")) {
        eprintln!("skip: directory link creation is unavailable on this host");
        return;
    }

    let result =
        fixture
            .executor()
            .stage(&fixture.root, &root_repository(), &["bundle/".to_owned()]);
    let _ = fs::remove_dir(&link);
    assert!(matches!(result, Err(GitReadError::PathRejected)));
}

#[test]
fn stage_directory_rejects_a_descendant_repository_marker() {
    let fixture = Fixture::new("descendant-repository");
    fixture.seed_commit();
    let nested = fixture.root.join("bundle/nested");
    fs::create_dir_all(&nested).expect("nested repository directory");
    fixture.git_ok(&nested, &["init", "-b", "main"]);
    fs::write(nested.join("nested.txt"), b"nested repository\n").expect("nested file");
    fixture.git_ok(&nested, &["add", "--", "nested.txt"]);
    fixture.git_ok(&nested, &["commit", "-m", "nested seed"]);

    let result =
        fixture
            .executor()
            .stage(&fixture.root, &root_repository(), &["bundle/".to_owned()]);
    assert!(matches!(result, Err(GitReadError::PathRejected)));
}

#[test]
fn leading_dash_and_control_character_messages_are_rejected() {
    let fixture = Fixture::new("message-validation");
    let executor = fixture.executor();
    for message in ["-pretend-flag", "control\ncharacter", "nul\0character"] {
        assert_input_rejected(
            executor.commit(&fixture.root, &root_repository(), message),
            GitInputKind::CommitMessage,
        );
    }
}

#[test]
fn upload_pack_shaped_and_control_character_branches_are_rejected() {
    let fixture = Fixture::new("branch-validation");
    let executor = fixture.executor();
    for branch in ["--upload-pack=malicious", "feature\nmalicious"] {
        assert_input_rejected(
            executor.push(&fixture.root, &root_repository(), "origin", branch),
            GitInputKind::BranchName,
        );
    }
}

#[test]
fn leading_dash_and_control_character_remote_names_are_rejected() {
    let fixture = Fixture::new("remote-validation");
    let executor = fixture.executor();
    for remote in ["-origin", "origin\u{0007}"] {
        assert_input_rejected(
            executor.push(&fixture.root, &root_repository(), remote, "main"),
            GitInputKind::RemoteName,
        );
    }
}

#[test]
fn stage_unstage_and_commit_succeed_in_a_temporary_repository() {
    let fixture = Fixture::new("local-positive");
    fixture.seed_commit();
    fixture.write("notes/change.txt", b"structured mutation\n");
    let executor = fixture.executor();
    let paths = vec!["notes/change.txt".to_owned()];

    let staged = executor
        .stage(&fixture.root, &root_repository(), &paths)
        .expect("structured stage");
    assert_eq!(staged.operation, GitMutationKind::Stage);
    let cached = fixture.git_ok(
        &fixture.root,
        &["diff", "--cached", "--name-only", "--", "notes/change.txt"],
    );
    assert_eq!(
        String::from_utf8_lossy(&cached.stdout).trim(),
        "notes/change.txt"
    );

    let unstaged = executor
        .unstage(&fixture.root, &root_repository(), &paths)
        .expect("structured unstage");
    assert_eq!(unstaged.operation, GitMutationKind::Unstage);
    let cached = fixture.git_ok(
        &fixture.root,
        &["diff", "--cached", "--name-only", "--", "notes/change.txt"],
    );
    assert!(
        cached.stdout.is_empty(),
        "path must be removed from the index"
    );
    assert!(fixture.root.join("notes/change.txt").is_file());

    executor
        .stage(&fixture.root, &root_repository(), &paths)
        .expect("restage");
    let committed = executor
        .commit(
            &fixture.root,
            &root_repository(),
            "structured mutation commit",
        )
        .expect("structured commit");
    assert_eq!(committed.operation, GitMutationKind::Commit);
    let subject = fixture.git_ok(&fixture.root, &["log", "-1", "--pretty=%s"]);
    assert_eq!(
        String::from_utf8_lossy(&subject.stdout).trim(),
        "structured mutation commit"
    );
}

#[test]
fn push_succeeds_to_the_explicit_contained_remote_and_branch() {
    let fixture = Fixture::new("push-positive");
    fixture.write("push.txt", b"push body\n");
    let remote = fixture.root.join("contained-remote.git");
    fs::create_dir_all(&remote).expect("create bare remote directory");
    fixture.git_ok(&remote, &["init", "--bare"]);
    let remote_text = remote.to_string_lossy().into_owned();
    fixture.git_ok(&fixture.root, &["remote", "add", "origin", &remote_text]);

    let executor = fixture.executor();
    executor
        .stage(&fixture.root, &root_repository(), &["push.txt".to_owned()])
        .expect("stage push commit");
    executor
        .commit(&fixture.root, &root_repository(), "push fixture commit")
        .expect("commit push fixture");

    let pushed = executor
        .push(&fixture.root, &root_repository(), "origin", "main")
        .expect("structured push");
    assert_eq!(pushed.operation, GitMutationKind::Push);
    assert_eq!(pushed.remote_name.as_deref(), Some("origin"));
    assert_eq!(pushed.branch_name.as_deref(), Some("main"));

    let remote_head = fixture.git_ok(
        &fixture.root,
        &["--git-dir", &remote_text, "rev-parse", "refs/heads/main"],
    );
    let local_head = fixture.git_ok(&fixture.root, &["rev-parse", "refs/heads/main"]);
    assert_eq!(remote_head.stdout, local_head.stdout);
}

#[test]
fn non_bare_local_remote_is_rejected_before_push() {
    let fixture = Fixture::new("non-bare-remote");
    fixture.seed_commit();
    let remote = fixture.root.join("non-bare-target");
    fs::create_dir_all(&remote).expect("create non-bare remote directory");
    fixture.git_ok(&remote, &["init", "-b", "other"]);
    let remote_text = remote.to_string_lossy().into_owned();
    fixture.git_ok(&fixture.root, &["remote", "add", "origin", &remote_text]);

    assert_input_rejected(
        fixture
            .executor()
            .push(&fixture.root, &root_repository(), "origin", "main"),
        GitInputKind::RemoteTarget,
    );
}

#[test]
fn non_bare_gitdir_path_is_rejected_before_push() {
    let fixture = Fixture::new("non-bare-gitdir");
    fixture.seed_commit();
    let remote = fixture.root.join("non-bare-worktree");
    fs::create_dir_all(&remote).expect("create non-bare worktree");
    fixture.git_ok(&remote, &["init", "-b", "other"]);
    let gitdir_text = remote.join(".git").to_string_lossy().into_owned();
    fixture.git_ok(&fixture.root, &["remote", "add", "origin", &gitdir_text]);

    assert_input_rejected(
        fixture
            .executor()
            .push(&fixture.root, &root_repository(), "origin", "main"),
        GitInputKind::RemoteTarget,
    );
}

#[test]
fn relative_local_remote_is_rejected_before_push() {
    let fixture = Fixture::new("relative-local-remote");
    fixture.seed_commit();
    let remote = fixture.root.join("relative-remote.git");
    fs::create_dir_all(&remote).expect("create relative bare remote directory");
    fixture.git_ok(&remote, &["init", "--bare"]);
    fixture.git_ok(
        &fixture.root,
        &["remote", "add", "origin", "relative-remote.git"],
    );

    assert_input_rejected(
        fixture
            .executor()
            .push(&fixture.root, &root_repository(), "origin", "main"),
        GitInputKind::RemoteTarget,
    );
}

#[test]
fn executable_remote_helper_transport_is_rejected_before_push() {
    let fixture = Fixture::new("remote-helper");
    fixture.seed_commit();
    fixture.git_ok(
        &fixture.root,
        &["remote", "add", "origin", "ext::malicious-helper"],
    );

    assert_input_rejected(
        fixture
            .executor()
            .push(&fixture.root, &root_repository(), "origin", "main"),
        GitInputKind::RemoteTarget,
    );
}
