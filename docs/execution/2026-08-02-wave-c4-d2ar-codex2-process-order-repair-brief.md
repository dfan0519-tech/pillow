# Pillow C4 D2AR — process ordering and ACP pipe repair brief

## Worker identity

- Actual account-2 Codex CLI, `gpt-5.6-terra`, high reasoning.
- Local session worker boot (optional) with `<local-parent-session>`,
  task `c4-d2ar-process-order-repair`, agent `codex2`.
- Preserve HEAD `7b7f70f`, dirty accepted work and empty staging. No commit,
  reset, restore, clean, package or publish.

## Controller rejection and root cause

The first D2A implementation is not accepted despite focused green tests.

1. `SupervisedProcessRunner::run` calls
   `SupervisedChild::spawn(spec, true, true, true)`. On Windows the final
   boolean resumes the suspended child inside `spawn`, before stdout/stderr
   reader threads are created. The later `resume()` is a no-op. This violates
   the locked create-suspended -> assign Job -> establish capture -> resume
   order.
2. `spawn_piped` passes `capture_stderr=true`, but the ACP compatibility handle
   never takes or drains stderr. A sufficiently noisy agent can fill the pipe
   and deadlock before producing protocol stdout. The prior ACP implementation
   used `Stdio::null()` for stderr.
3. The initial worker changed `adapter-grok-acp/src/process/env.rs` outside its
   allowlist. The controller now explicitly authorizes that exact file only for
   the already-reviewed shared sensitive-name predicate and opaque rejection;
   no additional behavior expansion is allowed.

Root cause: a reusable `resume_now` boolean made the security-sensitive launch
order caller-selectable, while `spawn_piped` reused the bounded-execution pipe
shape even though ACP owns stdout only.

## Required RED -> GREEN repair

1. Read the original brief/report, full current shared executor and ACP wrapper,
   and the accepted C4 process order.
2. RED first in `supervised_process.rs`:
   - add a focused test that `spawn_piped` exposes no stderr handle and still
     owns/cleans the child;
   - add a regression test for immediate dual-stream output saturation through
     `run` so capture completes within the fixed test timeout and remains
     bounded.
3. Remove the `resume_now` footgun. `SupervisedChild::spawn` must return the
   assigned suspended Windows child; callers resume explicitly:
   - `run`: take both pipes -> create both capture threads -> resume;
   - `spawn_piped`: configure stderr null -> resume -> return.
   Non-Windows behavior must continue compiling without claiming Windows
   suspended semantics.
4. Do not change Job cleanup, public receipt types, output caps or environment
   policy except as required for the two rejected paths.
5. Keep the reviewed `env.rs` change limited to using shared
   `sensitive_name` and returning the opaque ACP rejection. Run existing env
   tests to prove parity.
6. Run:
   - `cargo test -p keeper-process-executor --tests`
   - `cargo test -p keeper-adapter-grok-acp --tests`
   - `cargo clippy -p keeper-process-executor --all-targets -- -D warnings`
   - `cargo clippy -p keeper-adapter-grok-acp --all-targets -- -D warnings`
   - `cargo fmt --all --check`
   - `git diff --check -- crates/process-executor crates/adapter-grok-acp/src/process`

## Exclusive repair allowlist

- `crates/process-executor/src/lib.rs`
- `crates/process-executor/tests/supervised_process.rs`
- `crates/adapter-grok-acp/src/process/env.rs`
- `docs/execution/2026-08-02-wave-c4-d2ar-codex2-process-order-repair-report.md`

No manifest, lockfile, Job implementation, storage, protocol, runtime, Tauri,
renderer or other ACP file may change.

## Completion

Report RED/GREEN evidence, exact files, launch-order reasoning, ACP stderr
behavior, residual processes and risks. Post one completion message and emit
`COMPLETION: complete` or a precise blocker. Controller owns acceptance.
