# Pillow C4 D2BR2 - sealed process authority repair brief

## Worker and boundary

- Worker: actual Grok CLI `.grok2`, Grok 4.5 high.
- Keeper: boot as worker for `c4-d2br2-sealed-process-authority`, parent
  `<keeper-session>`.
- Work only in `this repository`; Keeper/references are read-only.
- Preserve HEAD `7b7f70f` and all dirty accepted/in-flight slices. No stage,
  commit, reset, restore, clean, package or publish.

## Controller rejection

D2BR1 fixed the originally listed surface, but controller review found two
remaining production bypasses:

1. `GovernedProcessToolGateway::store()` publicly returns
   `&SqliteExecProposalRepository`. A production caller can reserve, decide or
   claim directly and bypass the gateway's capability/identity contract.
2. `SealedProcessSpec::with_timeout()` is public. It changes the accepted fixed
   60-second process timeout after sealing, is not fingerprint-bound, and is
   described as a test seam while remaining available in production builds.

## Required repair

- Remove all production public access to the mutable execution repository.
  If normal callers need inspection, expose only a narrowly read-only gateway
  query returning the accepted metadata-only DTO; do not return the repository
  or a mutation-capable trait/object.
- Remove the production public timeout mutator. Every production seal must
  retain the protocol/manifest fixed timeout. Accelerated timeout tests belong
  in private `cfg(test)` unit support; cancellation tests may cancel a normal
  long-running child without changing the timeout.
- Ensure no other public method can mutate timeout, output cap, executable,
  native cwd, runner, reservation identity or claim state outside the governed
  operations.
- Preserve all D2BR1 argument/capability/native-path/runner fixes.

## RED -> GREEN evidence

Add failing-then-passing tests proving:

- normal external consumers cannot call `gateway.store()`;
- normal external consumers cannot call `sealed.with_timeout(...)`;
- fixed `PROCESS_TIMEOUT` remains 60 seconds from seal through execution spec;
- read-only proposal inspection, if retained, cannot reserve/decide/claim;
- Busy, expiry/drift, timeout, cancellation, cleanup uncertainty and metadata
  privacy regressions remain covered without a production test seam.

Run:

- `cargo test -p keeper-tool-executor --test governed_process -- --nocapture`
- `cargo test -p keeper-tool-executor --tests`
- `cargo check --workspace --all-targets`
- `cargo clippy -p keeper-tool-executor --all-targets -- -D warnings`
- `cargo fmt --all --check`
- `git diff --check -- crates/tool-executor`

## Exclusive write allowlist

- `crates/tool-executor/src/process.rs`
- `crates/tool-executor/src/lib.rs`
- `crates/tool-executor/tests/governed_process.rs`
- `docs/execution/2026-08-03-wave-c4-d2br2-grok2-sealed-authority-repair-report.md`

No manifest/lock or other crate/app/script edits. Report blocker rather than
crossing the allowlist.

## Completion

Return changed files, RED/GREEN evidence, final public surface, staging/HEAD
state, residual processes and risks. Post one Keeper completion message and
emit `COMPLETION: complete` or a precise blocker. Controller alone accepts.
