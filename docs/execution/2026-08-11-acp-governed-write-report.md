# ACP Governed Write Execution Report

Status: WORKER COMPLETE / CONTROLLER LIVE ACCEPTANCE PENDING

Completion signal: `ACP-GOVERNED-WRITE-COMPLETE`

## Scope and Result

- `initialize` now advertises both `readTextFile: true` and `writeTextFile: true`.
- The fixed plaintext profile now allowlists native `write` together with `ask_user_question`.
- `search_replace` was not added. It is another native editing surface, and this work order has no live evidence that every one of its operations uses the same ACP write callback. Enabling it would widen the write surface before that route is proved.
- Both `fs/write_text_file` (observed Grok spelling) and `fs/writeTextFile` enter the existing governed write chain.
- `disallowedTools: ["search_tool", "use_tool"]` and `mcpServers: []` remain unchanged.
- `pillow/write_tool` remains for the fake-agent/C3 fixtures, but all three RPC method spellings converge before execution and call the same `GovernedExecutionPort::execute_write` implementation.

## Command Exit Codes

Build/test commands were run without output pipelines. The PowerShell capture shape was:

```powershell
$log = Join-Path $env:TEMP '<log-name>'
<command> *> $log
$exit = $LASTEXITCODE
Write-Output "EXIT=$exit"
```

Cargo commands used `CARGO_TARGET_DIR=<repo>/target-acp-write`. Read-only source-inspection commands are omitted from this table; every build, test, format, lint, and gate command is included.

| Phase | Command | Exit code | Classification / evidence |
| --- | --- | ---: | --- |
| Log preflight | Redirect the first RED log to `<declared-temp-root>` | 1 | Environment-only: that directory's ACL denied the log create, so Cargo did not start. Re-run used the system temp directory. |
| T3 initial RED | `cargo test -p keeper-adapter-grok-acp native_write_allowlist_requires_acp_write_capability_in_the_same_profile` | 101 | Expected RED: shipped profile did not yet expose native `write`. |
| T3 first GREEN | Same focused coupling test | 0 | 1 passed. |
| T4 initial RED | `cargo test -p keeper-adapter-grok-acp --test governed_write_roundtrip acp_snake_write_runs_the_positive_governed_proposal_and_receipt_path -- --exact` | 101 | Expected RED: no proposal arrived; approval waiter timed out. |
| Provider-write units | `cargo test -p keeper-adapter-grok-acp provider_writes::tests::` | 0 | 6 passed, including both aliases, create/modify inference, strict rejection, and response projection. |
| ACP snake positive | Focused `acp_snake_write_runs_the_positive_governed_proposal_and_receipt_path` | 0 | 1 passed; proposal + terminal receipt + exact file content. |
| ACP camel negative | Focused `acp_camel_write_rejection_resolves_without_mutating_the_target` | 0 | 1 passed; proposal + rejected terminal receipt + target absent. |
| T3 mutation RED | Temporarily set only `writeTextFile=false`, then run the focused coupling test | 101 | Expected mutation failure: `native write with writeTextFile=false silently bypasses Pillow through LocalFs`. |
| T3 restored GREEN | Restore `writeTextFile=true`, then run the focused coupling test | 0 | 1 passed; mutation was fully restored. |
| Format check before formatting | `cargo fmt --all --check` | 1 | Expected formatting diff limited to this work order's Rust files. |
| Scoped formatting | `rustfmt --edition 2024 <seven allowed Rust files>` | 0 | Only `crates/adapter-grok-acp/**` files were written. |
| Format check after formatting | `cargo fmt --all --check` | 0 | Clean. |
| Adapter full test | `cargo test -p keeper-adapter-grok-acp` | 0 | 66 unit tests passed; all adapter integration suites passed; 1 opt-in live test remained ignored. |
| Adapter clippy | `cargo clippy -p keeper-adapter-grok-acp --all-targets -- -D warnings` | 0 | Clean. |
| Secret gate before report | `node scripts/check-secrets.mjs` | 0 | 9 patterns self-tested; no findings. |
| Sole worker gate | `node scripts/run-worker-gate.mjs` | 1 | `ENV-BLOCKED`: process supervisor could not obtain the Windows CIM process snapshot (`0x80041003`) before its cargo-fmt child started. No workaround attempted. |
| TypeScript typecheck fallback | `corepack pnpm typecheck` | 0 | Passed. |
| Package tests fallback | `corepack pnpm test` | 0 | 35 test files and 432 tests passed. Expected mocked-window stderr did not change the exit status. |
| Rust workspace fallback | `cargo test --workspace` | 101 | `ENV-BLOCKED`: the predeclared `path_scope.rs` directory-symlink fixture failed with Windows 1314; that desktop lib run reported 152 passed and 1 environment failure. Adapter tests were green. No workaround attempted. |
| Workspace clippy fallback | `cargo clippy --workspace -- -D warnings` | 0 | Clean. |
| ACP lock drift fallback | `node scripts/check-acp-protocol-drift.mjs` | 0 | `SKIPPED (未核对)`: network access was unavailable; not represented as a verified-current upstream lock. |
| Report machine-path scan | Scan the report for drive roots, extended-path prefixes, and profile identifiers | 0 | No matches. |
| Allowed diff whitespace check | `git diff --check -- crates/adapter-grok-acp <this-report>` | 0 | Clean; line-ending conversion warnings only. |
| Final secret gate after report | `node scripts/check-secrets.mjs` | 0 | 9 patterns self-tested; no findings in tracked or untracked-not-ignored files. |

Desktop E2E, desktop smoke, and live ACP probes were not run. They are controller-owned in this work order.

## T1 and T2 Coupling

The shipped state is now one atomic pair:

```text
agentProfile.tools contains "write"
AND
clientCapabilities.fs.readTextFile == true
AND
clientCapabilities.fs.writeTextFile == true
```

The coupling test first proves the safety implication `native write allowed => ACP write capability true`. It then separately proves both sides are actually shipped. Those positive assertions prevent a fake green in which the validator rejects or disables everything.

`search_replace` stays excluded because adding it would create another model-visible write/edit surface without route-specific live proof. `write_file`, terminal/process tools, `workflow`, and `monitor` also remain outside the fixed allowlist.

## T3 Red and Mutation Evidence

Two independent RED observations were recorded:

1. Before T1/T2 implementation, the new test exited 101 because native `write` was absent.
2. After implementation, only `writeTextFile` was mutated back to `false` while the allowlist still contained `write`. The same test exited 101 at the implication assertion with the LocalFs bypass explanation. Restoring the single boolean made the same test exit 0.

The second observation is the important coupling mutation: it directly recreates the silent unsafe combination instead of merely showing that an exact JSON shape changed.

## T4 Governed ACP Write Path

| Provider method | Normalization | Execution and response |
| --- | --- | --- |
| `fs/write_text_file` | ACP absolute path is reduced to a project-relative path; create/modify is inferred from target existence; transport request ID becomes a run-scoped call ID. | Existing proposal -> approval -> `execute_write` -> durable receipt; ACP receives `{}` only after a successful governed receipt. |
| `fs/writeTextFile` | Same normalization and canonical request identity rules. | Same handler, observer, execution port, receipt mapping, and fixed governed error vocabulary. |
| `pillow/write_tool` | Existing strict `fs.write@1` fixture manifest remains intact. | Same handler and same `execute_write`; its fixture-oriented typed result projection remains intact. |

The ACP request allowlist accepts only `sessionId`, `path`, `content`, and reserved `_meta`. `_meta` is ignored and never enters the proposal. Foreign sessions, unknown top-level keys, project escapes, invalid request IDs, and request-ID reuse with changed canonical arguments fail closed.

Positive integration proof is not limited to “not rejected”: it observes both lifecycle events, approves the exact durable proposal, receives ACP success, and asserts exact on-disk content. Negative proof rejects the exact proposal, observes the terminal resolution, receives a governed error, and asserts that the target does not exist.

## T5 `pillow/write_tool` Decision

Recommendation: retain it temporarily as a fake-agent/C3 fixture alias, then retire it in a separate migration after those fixtures and acceptance scripts use ACP methods.

It does not constitute a second governance path in the current implementation:

- method recognition for ACP aliases and `pillow/write_tool` is centralized;
- all recognized write methods enter the same `handle_write_tool` function;
- all produce the same `ProviderWriteProposal` type;
- all invoke the same `GovernedExecutionPort::execute_write` call exactly once;
- proposal lifecycle events and durable receipt handling are shared.

It remains an accepted RPC spelling, so eventual surface removal is still desirable. It was not removed here because that would couple this work order to fake-agent and C3 migration.

## Write Boundary Audit

Files authored by this work order:

- `crates/adapter-grok-acp/src/rpc/methods.rs`
- `crates/adapter-grok-acp/src/provider_writes.rs`
- `crates/adapter-grok-acp/src/provider_tools.rs` (crate-private path helper visibility only)
- `crates/adapter-grok-acp/src/prompt.rs`
- `crates/adapter-grok-acp/src/bin/fake_acp_agent/main.rs`
- `crates/adapter-grok-acp/src/bin/fake_acp_agent/scripts.rs`
- `crates/adapter-grok-acp/tests/governed_write_roundtrip.rs`
- `docs/execution/2026-08-11-acp-governed-write-report.md`

Audit results:

- No renderer, `runtime-host`, historical migration, or `scripts/**` file was edited.
- No dependency or `Cargo.lock` change was made.
- No git write operation, desktop E2E, desktop smoke, or live provider probe was run.
- Unrelated research-document changes appeared concurrently after the initial clean status. They were not authored, formatted, reverted, staged, or included in this work order.
- The dedicated cargo target directory is ignored build output, not a product artifact.

## Residual Risks and Possible Ungoverned Writes

1. Controller live acceptance remains required. The fake-ACP roundtrip proves Pillow's client handler and host chain, but only the controller probe proves the currently installed real Grok binary still selects ACP FS and sends the observed request shape.
2. The agent child is not an OS filesystem sandbox. A compromised or independently direct-writing child process could use ambient OS access outside the native `write` tool backend. This change governs the verified upstream tool route; it does not claim process-level filesystem confinement.
3. `pillow/process_tool` remains a separately governed process fixture surface. An approved process can have filesystem side effects that are not represented as individual write proposals. The shipped profile exposes no native process tool, and real Grok has no standard mapping to this custom RPC, but the custom method remains accepted for fixtures.
4. `pillow/write_tool` remains an accepted custom spelling, although it cannot bypass `execute_write`. Retiring that surface still needs the separate fake-agent/C3 migration described above.
5. `search_replace` is deliberately not exposed. If it is added later, its real Grok route must be live-probed and shown to converge on the same client write RPC before the allowlist changes.
6. ACP has no create/modify field. The adapter infers mode from target existence before the host captures its governed base. Existing host base-digest and atomic-apply checks make races fail closed, but real-provider modify behavior remains part of controller acceptance.
7. The observed real write flow may issue `fs/read_text_file` before `fs/write_text_file`. That read handler was pre-existing and unchanged; the controller probe must confirm the installed provider's complete read-then-write sequence still succeeds through registered governed runs.
8. Existing governance rejects unsupported writes such as empty content or unsafe path classes. Those requests fail closed; they are compatibility limitations, not direct-write fallbacks.

## Controller Acceptance Gate

Run only from the controller-owned environment:

```text
PILLOW_MCP_PROBE=1 PILLOW_MCP_PROBE_ONLY=acp-fs-write-governed,acp-fs-localfs-trap,shipped-profile-mcp
```

Acceptance must separately establish that the governed variant reaches the client and completes the approval path, the LocalFs trap remains the known negative when the capability is disabled, and ambient MCP remains unreachable.
