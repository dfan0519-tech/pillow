# Pillow C4 D2A — shared supervised process executor development brief

## Worker identity and session

- Worker: actual account-2 Codex CLI (`CODEX_HOME=%USERPROFILE%\.codex2`).
- Model: `gpt-5.6-terra`, reasoning `high`, `fastMode=false`.
- Keeper boot: `project=pillow`, `task=c4-d2a-process-executor`,
  `agentName=codex2`, `agentRole=worker`,
  `parentSessionId=<keeper-session>`.
- Repository: `<workspace>\pillow` only. `<workspace>\VaultSystem\keeper` and reference
  clones are read-only.
- Preserve HEAD `7b7f70f` and the accepted dirty C1–C3 plus D1 tree. No stage,
  commit, reset, restore, clean, package or publish.

## Inputs to read completely

1. `<workspace>\AGENTS.md`
2. `<workspace>\VaultSystem\vault\AGENTS.md`
3. `<workspace>\pillow\AGENTS.md`
4. `docs/superpowers/specs/2026-08-02-c4-governed-process-git-design.md`
5. Task 5 in `docs/superpowers/plans/2026-08-02-c4-governed-process-git-implementation.md`
6. `docs/execution/2026-08-01-desktop-process-cleanup-safety-fix-report.md`
7. Current complete files:
   - `crates/adapter-grok-acp/src/process/child.rs`
   - `crates/adapter-grok-acp/src/process/job_windows.rs`
   - `crates/adapter-grok-acp/src/process/env.rs`

## Goal

Create a runtime-neutral `keeper-process-executor` crate that owns structured
argv process execution, bounded stdout/stderr capture, timeout/cancellation and
stable-handle Windows Job Object cleanup. Migrate the Grok ACP adapter to this
shared implementation without weakening its behavior or changing its public
contract.

## Locked interfaces

```rust
pub struct ProcessSpec {
    pub executable: PathBuf,
    pub argv: Vec<OsString>,
    pub cwd: PathBuf,
    pub env: BTreeMap<OsString, OsString>,
    pub timeout: Duration,
    pub output_byte_cap: usize,
}

pub struct ProcessReceipt {
    pub exit_code: Option<i32>,
    pub duration: Duration,
    pub stdout: BoundedOutput,
    pub stderr: BoundedOutput,
    pub cancelled: bool,
    pub timed_out: bool,
    pub cleanup_proven: bool,
}
```

Also produce `CancellationSignal` and `SupervisedProcessRunner`. Public errors
must be typed/path-free; no raw environment or native executable is formatted
into user-facing error text.

## Security invariants

- Structured executable plus argv only; no shell string or arbitrary shell
  host support.
- `env_clear` before host-selected entries. Reject names containing `KEY`,
  `TOKEN`, `SECRET`, `PASSWORD`, `CREDENTIAL`, `AUTH` or provider names.
- Windows order: resolve executable -> create suspended -> create/configure
  kill-on-close Job Object -> assign child -> start bounded capture -> resume ->
  wait/cancel/timeout -> close/terminate via owned handles -> prove cleanup.
- No `taskkill`, process-name-wide cleanup, accumulated numeric-PID killing or
  PID-reuse assumptions. PID may be observation metadata only; missing or
  access-denied identity evidence fails closed.
- Combined output is bounded and records truncation, byte counts and lossy
  UTF-8. Do not write transcript bytes to ordinary logs.
- A setup failure after suspended spawn must terminate through owned handles and
  prove the child gone before returning.

## TDD workflow

1. Add RED tests before the new production crate exists. Cover:
   stdout/stderr separation, lossy UTF-8, combined cap/truncation, nonzero exit,
   timeout, explicit cancellation, grandchild cleanup, spawn/setup failure and
   secret-environment exclusion.
2. Run `cargo test -p keeper-process-executor --tests -- --nocapture` and record
   the expected package/module failure.
3. Implement the smallest shared executor satisfying those tests.
4. Adapt Grok ACP to the shared implementation. Preserve its existing fixed
   agent launch, stdin/stdout semantics, cancellation and cleanup tests.
5. Run:
   - `cargo test -p keeper-process-executor --tests`
   - `cargo test -p keeper-adapter-grok-acp --tests`
   - `cargo check --workspace --all-targets`
   - `cargo clippy --workspace --all-targets -- -D warnings`
   - `cargo fmt --all --check`
   - `git diff --check -- Cargo.toml Cargo.lock crates/process-executor crates/adapter-grok-acp`
6. Inspect relevant descendants/listeners after tests and report only
   ownership-proven residuals. Never clean them by numeric PID.

## Exclusive write allowlist

- `Cargo.toml`
- `Cargo.lock`
- `crates/process-executor/Cargo.toml`
- `crates/process-executor/src/lib.rs`
- `crates/process-executor/src/env.rs`
- `crates/process-executor/src/output.rs`
- `crates/process-executor/src/job_windows.rs`
- `crates/process-executor/tests/supervised_process.rs`
- `crates/adapter-grok-acp/Cargo.toml`
- `crates/adapter-grok-acp/src/process/child.rs`
- `crates/adapter-grok-acp/src/process/job_windows.rs`
- `docs/execution/2026-08-02-wave-c4-d2a-codex2-process-executor-development-report.md`

Do not modify protocol, policy, storage, runtime-host, Tauri, renderer, scripts
or any other report/plan. If the locked interface cannot be met inside this
allowlist, stop and report the precise blocker.

## Completion contract

The report must include exact changed files, RED and GREEN evidence, stable
handle ownership reasoning, environment denial evidence, output bounds,
residual processes, staging state and remaining risks. Post one Keeper
completion message to the parent session and print `COMPLETION: complete` or a
precise blocker. The controller owns integration and acceptance.
