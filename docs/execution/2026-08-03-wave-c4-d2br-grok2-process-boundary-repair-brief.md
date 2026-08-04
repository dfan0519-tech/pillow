# Pillow C4 D2BR - process gateway boundary repair brief

## Worker identity and session

- Worker: real Grok CLI slot `grok2`, model Grok 4.5, high reasoning.
- Keeper boot: `project=pillow`, `task=c4-d2br-process-boundary-repair`,
  `agentName=grok2`, `agentRole=worker`,
  `parentSessionId=<keeper-session>`.
- Repository: `<workspace>\pillow` only. Keeper and references are read-only.
- Preserve HEAD `7b7f70f` and every existing dirty C1-C4 slice. No stage,
  commit, reset, restore, clean, package or publish.

## Why D2B was rejected

The controller's fresh focused tests were green, but source review found that
the public gateway still admits authority-bearing inputs outside the accepted
`process.run@1` boundary. Tests that merely exercise the current API are not
acceptance evidence until the following bypasses are removed.

## Required repairs

1. Make `seal` consume and validate the exact bounded process-run argument
   contract. Enforce at the real production boundary:
   - at most 64 argv entries;
   - at most 32 KiB combined program/argv text;
   - no NUL or control characters;
   - the accepted fixed program label and portable cwd rules.
   Do not leave a second raw `&[String]` path that skips these checks.
2. Remove the identity/capability bypass in public `reserve_with_input`.
   Production reservation must be bound to `ProjectCapability`, and the input
   project/conversation/run/call identities must be validated against that
   authority. If tests require construction helpers, isolate them so ordinary
   production consumers cannot reserve caller-selected identity.
3. Stop exposing resolved native executable and native cwd material from the
   public sealed-spec surface. Keep native authority memory-only inside the
   gateway/runner boundary. Safe argv display may remain as sanitized data.
4. Production gateway construction must always select
   `SupervisedProcessRunner`. Arbitrary public `ProcessRunner` injection and
   counting/cleanup doubles must not be usable by production consumers. A
   narrowly gated test-support surface is acceptable when it is not present in
   normal builds.
5. Preserve the already-correct behavior: drift/expiry after claim transitions
   to a durable terminal failure, Busy never spawns, the clean environment is
   retained, cancellation persists the typed receipt, and cleanup uncertainty
   fails closed.

## Required RED -> GREEN evidence

Add tests that fail before the repair and pass after it for:

- 64 argv entries accepted and 65 denied;
- exactly bounded combined text and over-32-KiB denial;
- NUL and representative C0/DEL control denial in program/argv;
- reservation with a mismatched `ProjectCapability` project denied;
- caller-selected project/conversation/run/call identities cannot bypass the
  production reservation boundary;
- no public native executable/cwd getter and no production arbitrary-runner
  construction path;
- existing drift-after-claim, Busy/no-spawn, environment, cancellation and
  cleanup-failure tests remain green.

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
- `crates/tool-executor/Cargo.toml` only if a non-default test-support feature
  is strictly required
- `docs/execution/2026-08-03-wave-c4-d2br-grok2-process-boundary-repair-report.md`

Do not modify root manifests/lock, protocol, policy, process-executor, storage,
runtime-host, Tauri, renderer, Git executor or scripts. Stop and report if the
repair genuinely requires an out-of-allowlist change.

## Completion contract

Report exact changed files, before/after test evidence, the final public API
surface, capability-binding proof, staging state, residual processes and risks.
Post one Keeper completion message to the parent session and emit
`COMPLETION: complete` or a precise blocker. The controller alone accepts.
