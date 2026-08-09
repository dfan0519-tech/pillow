# C4 D3AR2A Codex2 brief — formatting-only acceptance repair

Read this file first. It is the complete task contract.

You are the actual account-2 Codex CLI worker (`CODEX_HOME=%USERPROFILE%\.codex2`,
`gpt-5.6-terra`, high reasoning, `fastMode=false`). Boot Keeper as a `pillow`
worker for task `c4-d3ar2a-format-repair` before editing. Work only in
`this repository`; `legacy Keeper (out of tree)` is read-only.

Controller review found D3AR2 task A functionally green, but
`cargo fmt --all --check` fails. Perform formatting-only repair. Do not change
behavior, tests, assertions, public contracts, manifests or documentation
other than the completion report. Preserve HEAD
`7b7f70f7360131cd1cfd984a8b19bdfb5725c21b`, empty staging and all unrelated
dirty work. No stage, commit, reset, restore, clean, package, publish or
process cleanup.

## Exact write allowlist

- `crates/process-executor/src/job_windows.rs`
- `crates/process-executor/src/lib.rs`
- `crates/process-executor/tests/supervised_process.rs`
- `crates/runtime-host/src/host/process.rs`
- `crates/runtime-host/tests/governed_process_bridge.rs`
- `crates/tool-executor/src/process.rs`
- `crates/tool-executor/tests/governed_process.rs`
- `docs/execution/2026-08-04-wave-c4-d3ar2a-codex2-format-repair-report.md`

The controller independently observed these seven Rust files as the complete
`rustfmt` failure set. Use the workspace formatter, then verify that no file
outside this allowlist changed because of your action. If the formatter wants
to touch any additional file, stop and report `BLOCKED`.

## Required gates

Run and report exact results:

```text
cargo fmt --all --check
cargo test -p keeper-process-executor --all-targets
cargo test -p keeper-tool-executor --all-targets
cargo test -p keeper-runtime-host --all-targets
cargo test -p keeper-runtime-host --features test-support --test governed_process_bridge
cargo check -p keeper-process-executor -p keeper-tool-executor -p keeper-runtime-host --all-targets
cargo clippy -p keeper-process-executor -p keeper-tool-executor -p keeper-runtime-host --all-targets -- -D warnings
git diff --check -- crates/process-executor/src/job_windows.rs crates/process-executor/src/lib.rs crates/process-executor/tests/supervised_process.rs crates/runtime-host/src/host/process.rs crates/runtime-host/tests/governed_process_bridge.rs crates/tool-executor/src/process.rs crates/tool-executor/tests/governed_process.rs
git diff --cached --name-only
git rev-parse HEAD
```

Write the exact report path above with files touched, proof that changes are
formatting-only, command results, HEAD/staging, residual risks and Keeper
completion message ID. End with `COMPLETION: complete` only if every gate is
green; otherwise use `BLOCKED` or `DONE_WITH_CONCERNS`. Controller acceptance
remains separate.
