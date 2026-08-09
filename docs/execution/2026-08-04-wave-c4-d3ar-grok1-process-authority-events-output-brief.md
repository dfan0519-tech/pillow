# C4 D3AR Grok repair brief — process authority, events and real output

Read this file first. It is the complete task contract.

## Role and status

You are the actual Grok CLI implementation worker identified by the live Keeper
lease. Repair the rejected
D3A Task 8 slice. The controller owns architecture adjudication, source review,
acceptance and aggregate gates.

Start with Keeper boot using project `pillow`, task
`c4-d3ar-process-authority-events-output`, agentRole `worker`. Work only in
`this repository`. Treat `legacy Keeper (out of tree)` as read-only.

## Accepted baseline that must remain true

- HEAD is `7b7f70f7360131cd1cfd984a8b19bdfb5725c21b`; staging must remain empty.
- D2A supervised Windows process ownership, capture-before-resume, Job Object
  cleanup, one-shot run claim and typed terminal receipts are accepted.
- D2B `GovernedProcessToolGateway` is the only process ledger/execution
  authority exposed to runtime-host. No product caller may obtain or recreate
  its mutation-capable SQLite repository.
- No numeric-PID or process-name cleanup is allowed.
- Preserve all unrelated dirty-tree work. No stage, commit, reset, restore,
  clean, package or publish.

## Required repair

### 1. Remove the runtime-host storage side channel

- Remove every direct `SqliteExecProposalRepository` import, open, list,
  transition and recovery call from runtime-host process coordination.
- Delete the gateway-pointer/database-path registry and all related registration
  methods.
- Runtime-host must consume only `GovernedProcessToolGateway`.
- Add only narrow fixed gateway operations required by the host: capability and
  conversation-bound get/list, reject/expire/cancel/interrupt, and fixed startup
  recovery. Do not return the repository, a generic transition function, a
  mutation trait/object, a raw connection or the database path.
- Identical terminal replay must use the existing deterministic reservation key
  and fingerprint result from `reserve`; do not pre-list storage rows.
- Every read/mutation that names a proposal must verify the owning project
  capability and conversation. Foreign project/conversation access fails
  closed.

### 2. Keep approval grants host-private

- The raw approval grant is generated and retained only inside live
  runtime-host state, memory-only, single-use and cleared on decision, expiry,
  cancellation, interruption, release and shutdown.
- `LiveProcessProposalView` exposes `decision_available: bool`, never the grant.
- The ordinary public decision API accepts owner-scoped conversation/proposal
  identity plus approve-once/reject only. It must not accept or return a raw
  grant. The host supplies its retained digest/material internally to the
  gateway.
- Debug, errors, events, SQLite, reports and renderer-facing DTOs must not
  contain the raw grant.

### 3. Deliver actual bounded stdout/stderr

- Fresh execution must surface the real bounded stdout/stderr captured by the
  supervised process runner, with stream tags, monotonic sequence, lossy UTF-8
  and truncation metadata. Do not synthesize chunks and do not leave the vector
  permanently empty.
- Combined retained output remains capped at 256 KiB and 4,000 display lines.
  Escape/control sequences must be neutralized before renderer projection.
- Full output remains memory-only. SQLite keeps metadata/digests only; terminal
  replay after restart has no transcript and explicitly reports that live
  output was not retained.
- Preserve per-stream order. Pillow makes no stronger cross-stream ordering
  claim than the runner can observe.
- If a safe live callback/stream seam is required, implement it in the
  supervised runner and keep it bounded/non-authoritative. A post-completion
  fake stream is not acceptable. Preserve all existing D2A safety gates.

### 4. Add fixed validated process runtime events

Add four fixed `RuntimeEventKind` values and exact deny-unknown-fields payloads:

- process proposal requested;
- process proposal decided;
- process started;
- process terminal outcome.

The names must serialize in the repository's existing snake_case convention.
Payloads may carry only the safe proposal/display/lifecycle fields required by
the C4 design. They must never carry raw grant, native executable/root, full
output, environment, provider payload, session id, database path or debug
metadata. Output chunks use the bounded native output channel, not durable run
events.

Runtime-host coordination must produce these typed validated payloads in the
durable order reserve -> requested -> decision -> start -> terminal. Rejected
or expired proposals have no start. Busy/replay never creates a second start or
spawn authority. Integrate with the existing caller-emits-event pattern; do not
create a second event bus.

### 5. Remove production test authority

`force_live_process_expiry_for_tests` and any equivalent clock/expiry mutator
must be absent from an ordinary build. Gate it with `cfg(test)` or the existing
non-default `test-support` feature, including all re-exports and host methods.

## Exact write allowlist

You may modify only:

- `crates/agent-protocol/src/types.rs`
- `crates/agent-protocol/src/validate.rs`
- `crates/agent-protocol/src/lib.rs`
- `crates/agent-protocol/tests/process_event_contract.rs` (new if useful)
- `crates/process-executor/src/lib.rs`
- `crates/process-executor/src/output.rs`
- `crates/process-executor/tests/supervised_process.rs`
- `crates/tool-executor/src/process.rs`
- `crates/tool-executor/src/lib.rs`
- `crates/tool-executor/tests/governed_process.rs`
- `crates/runtime-host/Cargo.toml`
- `crates/runtime-host/src/host/process.rs`
- `crates/runtime-host/src/host/mod.rs`
- `crates/runtime-host/src/lib.rs`
- `crates/runtime-host/tests/governed_process_bridge.rs`
- `docs/execution/2026-08-04-wave-c4-d3ar-grok-process-authority-events-output-report.md`

Do not modify storage, policy, Tauri, TypeScript, renderer, Git executor, root
manifests/lockfile, scripts, roadmap/progress, acceptance docs or any Codex2
Task 10 file. Stop and report `BLOCKED` if the exact contract genuinely cannot
be met inside this allowlist.

## TDD and required evidence

Write failing tests first, then make them green. At minimum prove:

1. ordinary runtime-host product code contains no direct execution repository
   construction/use and no pointer/path registry;
2. gateway list/read/mutations deny foreign project and conversation identity;
3. raw grant is absent from public views, decision arguments, Debug, events and
   persistence, while approve-once still executes exactly once;
4. actual stdout and stderr text reaches the bounded memory-only output view;
   sequence/tags, lossy UTF-8, control neutralization, 256 KiB and 4,000-line
   truncation are covered;
5. output text is absent after restart while durable terminal metadata remains;
6. exact process event schemas round-trip, reject unknown/secret/native/output
   fields, and preserve requested/decided/started/terminal ordering;
7. reject/expire has no start; replay/busy has no second start/spawn;
8. cancellation/interruption/restart do not replay authority and leave zero
   live process authority;
9. the force-expiry seam cannot be imported from an ordinary integration test;
10. all pre-existing D2A/D2B focused tests stay green.

Run and record:

```text
cargo test -p keeper-agent-protocol --all-targets
cargo test -p keeper-process-executor --all-targets
cargo test -p keeper-tool-executor --all-targets
cargo test -p keeper-runtime-host --all-targets
cargo check --workspace --all-targets
cargo clippy -p keeper-agent-protocol -p keeper-process-executor -p keeper-tool-executor -p keeper-runtime-host --all-targets -- -D warnings
cargo fmt --all --check
git diff --check -- <all allowlisted source/test paths>
git diff --cached --name-only
git rev-parse HEAD
```

The concurrent Codex2 RED desktop anchor may still block a workspace-wide
command. If so, record the exact external failure without editing its files;
package-scoped gates for your allowlist must still be green.

## Completion report

Write the report at the exact allowlisted path. Include:

- `DONE`, `DONE_WITH_CONCERNS` or `BLOCKED`;
- exact changed files;
- public API changes and why no second authority exists;
- output/event ordering and privacy proof;
- each command and result;
- HEAD/staging evidence;
- residual processes and residual risks;
- final line `COMPLETION: complete` only if the whole brief is met.

Return only a short completion signal. Controller acceptance is separate.
