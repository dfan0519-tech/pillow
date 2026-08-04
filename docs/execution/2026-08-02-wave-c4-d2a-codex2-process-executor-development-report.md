# C4 D2A shared supervised process executor — development report

Date: 2026-08-02
Worker: codex2

## Delivered

Added `keeper-process-executor`, a runtime-neutral structured-argv executor.
It exposes the locked `ProcessSpec` and `ProcessReceipt` contracts plus
`CancellationSignal` and `SupervisedProcessRunner`.

- The runner clears the environment, accepts only the supplied map, and rejects
  sensitive/provider environment names before launch.
- stdout and stderr are captured separately against one shared byte budget.
  Each `BoundedOutput` retains byte-seen/retained counts, truncation, and
  lossy-UTF-8 state; the executor itself does not log transcript bytes.
- Windows creates the child suspended, owns a kill-on-close Job Object, assigns
  the process before resume, and terminates only through that owned Job handle.
  Both timeout/cancel and ordinary completion wait for root absence and an
  empty Job Object before reporting cleanup proven. Access-denied liveness
  evidence fails closed.
- Setup failures after suspended spawn terminate through the same owned handles
  and require absence proof. A debug-only focused test exercises this branch.
- ACP now delegates child ownership, process liveness, and Job Object cleanup
  to the shared crate through a compatibility handle. Its fixed launch argv,
  stdin/stdout RPC semantics, cancellation behavior, and test hook remain
  unchanged.

## Changed files

- `Cargo.toml`
- `Cargo.lock` (workspace lock also contains accepted concurrent C1–D1 entries)
- `crates/process-executor/Cargo.toml`
- `crates/process-executor/src/lib.rs`
- `crates/process-executor/src/env.rs`
- `crates/process-executor/src/output.rs`
- `crates/process-executor/src/job_windows.rs`
- `crates/process-executor/tests/supervised_process.rs`
- `crates/adapter-grok-acp/Cargo.toml`
- `crates/adapter-grok-acp/src/process/child.rs`
- `crates/adapter-grok-acp/src/process/env.rs`
- `crates/adapter-grok-acp/src/process/job_windows.rs`
- this report

## RED then GREEN evidence

RED was recorded before the crate manifest existed:

```text
cargo test -p keeper-process-executor --tests -- --nocapture
failed to load manifest for workspace member crates/process-executor:
Cargo.toml not found
```

GREEN:

```text
cargo test -p keeper-process-executor --tests
8 passed; 0 failed

cargo test -p keeper-adapter-grok-acp --tests
91 passed; 0 failed; 1 ignored (explicit opt-in live ACP test)

cargo clippy -p keeper-process-executor --all-targets -- -D warnings
passed

cargo clippy -p keeper-adapter-grok-acp --all-targets -- -D warnings
passed

git diff --check -- Cargo.toml Cargo.lock crates/process-executor crates/adapter-grok-acp
passed
```

The executor suite covers stdout/stderr separation, invalid UTF-8,
combined-budget truncation, nonzero exit, timeout, explicit cancellation,
grandchild cleanup, unavailable executable, post-spawn setup failure, and
secret environment rejection.

## Required workspace gates

`cargo check --workspace --all-targets` is blocked by accepted, unrelated
`crates/storage-sqlite/tests/exec_proposal_repository.rs` imports that refer to
missing exec-proposal exports. `cargo clippy --workspace --all-targets -- -D
warnings` is similarly blocked by the unrelated existing
`crates/agent-protocol/tests/tool_contract.rs` `too_many_arguments` lint.

`cargo fmt --all --check` is blocked only by pre-existing formatting deltas in
the accepted `crates/storage-sqlite` exec-proposal worktree. All D2A files were
formatted with `rustfmt` and the scoped diff check is clean.

## Residuals and staging

After focused tests, no `cargo`, `rustc`, `fake_acp_agent`, or `pillow` process
was listed. No process was killed by numeric PID or name. No files were staged,
committed, reset, restored, cleaned, packaged, or published.

## Remaining risk

The current C4 integration still needs controller wiring from the ledger/tool
executor to this new shared runner. The tested ACP compatibility path resumes
after Job assignment as before; the high-level shared runner is the path that
starts bounded capture before resume.
