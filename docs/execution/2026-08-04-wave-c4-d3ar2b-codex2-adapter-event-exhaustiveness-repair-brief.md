# C4 D3AR2B Codex2 brief — adapter event exhaustiveness repair

Read this file first. It is the complete task contract.

You are the actual account-2 Codex CLI worker (`CODEX_HOME=%USERPROFILE%\.codex2`,
`gpt-5.6-terra`, high reasoning, `fastMode=false`). Boot Keeper as a `pillow`
worker for task `c4-d3ar2b-adapter-event-exhaustiveness-repair`. Work only in
`this repository`; Keeper is read-only. Preserve HEAD
`7b7f70f7360131cd1cfd984a8b19bdfb5725c21b`, empty staging and all unrelated
dirty work. No stage, commit, reset, restore, clean, package, publish or
process cleanup.

## Controller rejection evidence

After D3AR2 task B focused gates passed, controller ran `pnpm check`. UI tests
passed 272/272, then Rust workspace compilation failed with E0004 in
`crates/adapter-grok-acp/tests/live_native_acceptance.rs` at the exhaustive
`match kind`: the four fixed process event kinds are not covered.

Repair only this integration exhaustiveness gap. Preserve the fixed vocabulary:
add explicit mappings for `ProcessProposalRequested`, `ProcessProposalDecided`,
`ProcessStarted`, and `ProcessTerminalOutcome`. Do not add a wildcard arm and
do not weaken the acceptance assertions.

## Exact write allowlist

- `crates/adapter-grok-acp/tests/live_native_acceptance.rs`
- `docs/execution/2026-08-04-wave-c4-d3ar2b-codex2-adapter-event-exhaustiveness-repair-report.md`

No other source, test, manifest, migration or document may be changed.

## Required gates

```text
cargo test -p keeper-adapter-grok-acp --all-targets
cargo test --workspace
cargo clippy --workspace -- -D warnings
cargo fmt --all --check
git diff --check -- crates/adapter-grok-acp/tests/live_native_acceptance.rs
git diff --cached --name-only
git rev-parse HEAD
```

Write the report path above with exact changes, commands, risks, HEAD/staging
and Keeper completion message ID. End with `COMPLETION: complete` only if every
gate passes; otherwise use `BLOCKED` or `DONE_WITH_CONCERNS`. Controller will
independently rerun `pnpm check` and the remaining production/E2E gates.
