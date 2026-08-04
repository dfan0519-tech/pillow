# C4 D2AR process ordering and ACP pipe repair — development report

Date: 2026-08-02
Worker: codex2

## Delivered

The process executor no longer lets a caller choose whether a Windows child is
resumed inside its reusable spawn helper.

- `SupervisedChild::spawn` now returns after Windows suspended creation and Job
  assignment, with the owned primary-thread handle still retained.
- `SupervisedProcessRunner::run` takes stdout and stderr, starts both bounded
  capture threads, then explicitly resumes the child. This preserves the
  required order: resolve executable -> create suspended -> create/configure
  kill-on-close Job -> assign -> establish both capture readers -> resume.
- `spawn_piped` creates stdin/stdout pipes but uses `Stdio::null()` for stderr,
  then explicitly resumes the assigned child before returning it to ACP. ACP
  therefore owns and drains only protocol stdout; an agent cannot block its
  protocol stdout by filling an unconsumed stderr pipe.
- Non-Windows still uses normal process creation and the explicit `resume()`
  call remains a no-op there. It makes no claim that the child was suspended.

The reviewed ACP environment change remains limited to its previous scope:
delegation to the shared `sensitive_name` predicate and the opaque
`acp_extra_env_rejected` rejection. This repair did not expand its behavior.

## Exact repair files

- `crates/process-executor/src/lib.rs`
- `crates/process-executor/tests/supervised_process.rs`
- `docs/execution/2026-08-02-wave-c4-d2ar-codex2-process-order-repair-report.md`

`crates/adapter-grok-acp/src/process/env.rs` was reviewed under the explicit
authorization but was not modified by this repair.

## RED then GREEN

RED was added first in `supervised_process.rs` and run before production code
was changed:

```text
cargo test -p keeper-process-executor --test supervised_process -- --nocapture
10 tests: 9 passed, 1 failed
spawn_piped_discards_stderr_and_cleans_its_owned_child
assertion failed: child.take_stderr().is_none()
```

The same test holds stable-handle cleanup accountable by calling `kill_tree`
and requiring `process_gone(pid)` afterward. The companion regression starts
immediate concurrent stdout/stderr saturation through `run`, requires normal
completion within two seconds, and verifies the shared 32 KiB output cap and
cleanup proof.

GREEN:

```text
cargo test -p keeper-process-executor --tests
10 passed; 0 failed

cargo test -p keeper-adapter-grok-acp --tests
95 passed; 0 failed; 1 ignored (explicit opt-in live ACP test)

cargo clippy -p keeper-process-executor --all-targets -- -D warnings
passed

cargo clippy -p keeper-adapter-grok-acp --all-targets -- -D warnings
passed

git diff --check -- crates/process-executor crates/adapter-grok-acp/src/process
passed
```

The ACP test run includes all existing environment tests, including sensitive
parent-env exclusion, isolated-home construction, sensitive extra-env
rejection, and profile extra-env rejection.

`cargo fmt --all --check` was run but is blocked by an existing, unrelated
formatting delta in the out-of-allowlist file
`crates/storage-sqlite/tests/exec_proposal_repository.rs` (two long digest
assertions). It was not changed. The repair files themselves were checked with
the same workspace formatting configuration; the shared pre-existing Windows
Job implementation remains outside this repair's allowlist.

## Residuals and remaining risk

After the focused tests, no owned `cargo`, `rustc`, `fake_acp_agent`,
`supervised_process`, or `pillow` process remained. No numeric PID or
process-name cleanup was performed. Staging remains empty.

The remaining acceptance risk is the unrelated workspace formatting blocker;
only the controller may authorize that file's repair. The Windows ordering is
covered by the explicit API shape and saturation regression, but it remains
appropriate for the controller to independently inspect the Windows Job path
before acceptance.
