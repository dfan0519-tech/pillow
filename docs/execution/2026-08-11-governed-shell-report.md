# Governed Shell S0/S1 Execution Report

Date: 2026-08-11  
Worker: `codex1`  
Controller: Claude  
Scope: S0 half-open guard plus S1 per-command human approval

## Scope and invariants

- Shell commands reuse the existing governed-process chain: proposal, one-time
  human approval, host execution, durable receipt, and project isolation.
- S1 has no rules and no automatic allow path: every shell command asks a
  human.
- The approval surface and receipt identify the complete, original command
  string. The approved command is the executed command.
- The structured `process.run@1` program catalog is unchanged and does not
  apply to shell commands.
- `scripts/**`, desktop renderer code, historical migrations, and
  `crates/tool-executor/**` are outside this worker's write boundary.
- Desktop E2E is intentionally not run by this worker.

## Exit-code table

| Gate / probe | Command | Exit code | Result | Notes |
|---|---|---:|---|---|
| S0 initial RED | `cargo test -p keeper-adapter-grok-acp native_shell_allowlist_requires_acp_terminal_capability_in_the_same_profile` | 101 | EXPECTED-RED | Profile had no native shell hand yet. |
| S0 half-open mutation | same focused test | 101 | EXPECTED-RED | `run_terminal_cmd` was allowed while `terminal=false`; failure was the LocalTerminal bypass assertion. |
| S0 restored conjunction | same focused test | 0 | PASS | `run_terminal_cmd` and `terminal=true` shipped together. |

All commands used the isolated `<repo>/target-shell-s1` Cargo target. No command
was piped through `tail` or another exit-code-masking consumer.

## S0 coupling test and mutation evidence

The coupling test is
`crates/adapter-grok-acp/src/rpc/methods.rs:169`. It has three directions:

1. if any known shell-class hand is present, ACP terminal capability is true;
2. the shipped profile positively contains a shell hand;
3. the shipped initialize payload positively contains the terminal capability.

The half-open mutation changed only the allowlist to include the real short id
`run_terminal_cmd`, leaving `clientCapabilities.terminal=false`. The focused
test failed with exit code 101 at:

```text
native shell with terminal=false silently bypasses Pillow through LocalTerminal
```

This is the silent-failure shape the test protects. Restoring the conjunction
made the same focused test pass with exit code 0.

## Upstream `terminal/*` contract evidence

The pinned upstream source uses the standard client method family below:

| Method | Request shape | Response shape | Pinned upstream source |
|---|---|---|---|
| `terminal/create` | `sessionId`, `command: String`, `args: String[]`, `env`, optional `cwd`, optional output limit | `terminalId` | `agent-client-protocol-schema/src/v1/client.rs:1127-1176,2290-2298` |
| `terminal/output` | `sessionId`, `terminalId` | `output`, `truncated`, optional exit status | `xai-acp-lib/src/message.rs:152-176` |
| `terminal/wait_for_exit` | `sessionId`, `terminalId` | optional `exitCode`, optional `signal` | `xai-acp-lib/src/message.rs:152-176` |
| `terminal/kill` | `sessionId`, `terminalId` | kill acknowledgement | `xai-acp-lib/src/message.rs:152-176` |
| `terminal/release` | `sessionId`, `terminalId` | empty acknowledgement | `xai-acp-lib/src/message.rs:152-176` |

`ClientCapabilities.terminal` promises support for all `terminal/*` methods at
`agent-client-protocol-schema/src/v1/client.rs:1737-1741`. Grok's production
`AcpTerminalAdapter` sends the original command string with `args=[]` at
`xai-grok-tools/src/terminal/adapter.rs:114-127,161-173,186-187`; its foreground
lifecycle is create, wait, output, release, with timeout leading to kill at
`xai-grok-tools/src/terminal/adapter.rs:185-251`.

These line references are from upstream pin `75e73f3`, as recorded by the
checked-in route research. File references in this report are
repository-relative and include line numbers.

## S1 implementation evidence

S0 and the profile/capability half of S1 are implemented. The client terminal
handler and host shell sealing/execution are pending the write-boundary issue
described below.

## Positive and negative contract coverage

The S0 test pins both acceptance and rejection directions; the full terminal
contract positive/negative matrix remains pending.

## Write-boundary audit

Current writes are limited to:

- `crates/adapter-grok-acp/src/rpc/methods.rs`;
- this report.

This worker has not changed any file under `scripts/**`,
`apps/desktop/src/**`, `crates/tool-executor/**`, or historical migrations.
Concurrent `scripts/**` changes and a separate harness report from the assigned
scripts Worker are visible in the shared worktree and are preserved untouched;
they are not part of this report's write set.

### Boundary issue requiring controller direction

The existing common governed-process chain accepts only a
`keeper_tool_executor::SealedProcessSpec`. `runtime-host` calls the only public
seal entry at `crates/runtime-host/src/host/process.rs:730`, while that entry
unconditionally calls `validate_program_catalog` at
`crates/tool-executor/src/process.rs:560-567`. The same private sealed type is
then required by reserve and execute at
`crates/tool-executor/src/process.rs:593,749`.

Consequently, the allowed `runtime-host` wiring surface cannot construct a
shell proposal that shares the existing reserve, approve-once, claim, Job
Object, cancellation, timeout, and durable-receipt path without one of these
forbidden outcomes:

- applying the structured program catalog to the shell command;
- using a Node/Python trampoline merely to satisfy that catalog;
- copying a second reserve/claim/execute governance implementation into the
  host;
- changing `crates/tool-executor/src/process.rs` to add a parallel shell seal.

The first three contradict the accepted design. The fourth is the narrow
architectural change required for a genuine shared chain, but is outside the
current write allowlist.

## Environment-blocked gates

No environment-blocked gate has been run yet. If a gate cannot obtain the
Windows process snapshot or reports OS
error 1314, it is recorded as `ENV-BLOCKED`; the implementation is not changed
to bypass the gate.

## Residual risks and ungoverned process paths

- With `run_terminal_cmd` allowed and `terminal=true`, Grok cannot silently
  select LocalTerminal; the current missing client handler fails closed as a
  method error rather than spawning locally.
- The terminal client and actual governed shell execution are not implemented
  yet, so the capability is not functional and must not be accepted as S1.
- Agent-side LocalTerminal remains reachable in any future profile that admits
  a shell-class tool while setting terminal capability false; the S0 coupling
  test is the source guard, and the controller's live conjunction probe remains
  required.

---

## S1 continuation after the approved boundary expansion

This section supersedes the earlier statements that the shell client/host path
was pending and that `crates/tool-executor/**` was outside the write boundary.
The controller explicitly expanded that boundary for one parallel shell
seal/revalidate/execute path and its focused tests. The existing S0 profile and
coupling test remain in the worktree.

### Implemented path

- `crates/tool-executor/src/process.rs:295-353` defines
  `SealedShellSpec`, a distinct memory-only authority whose review argv is the
  one complete original command. The shell seal is at
  `crates/tool-executor/src/process.rs:718-745`.
- Shell reserve enters the same private implementation as structured reserve at
  `crates/tool-executor/src/process.rs:775-842`. Shell execute enters the same
  private claim, revalidation, clean-environment, supervised runner,
  cancellation, timeout, Job, and durable terminal-receipt implementation at
  `crates/tool-executor/src/process.rs:927-1139`. Path-specific revalidation is
  selected inside that common implementation at
  `crates/tool-executor/src/process.rs:1142-1171`.
- The trusted shell interpreter is resolved independently of `PATH` at
  `crates/tool-executor/src/process.rs:1499-1532`; the exact approved string is
  passed as the single command operand at
  `crates/tool-executor/src/process.rs:1535-1543`.
- `crates/runtime-host/src/host/process.rs:269-373` keeps the structured and
  shell sealed authorities distinct until a private dispatcher selects their
  shared gateway methods. `execute_governed_shell` is at
  `crates/runtime-host/src/host/process.rs:846-881`; both paths enter the one
  host reserve/approval/wait lifecycle at
  `crates/runtime-host/src/host/process.rs:907-1089`.
- The adapter-facing port has a distinct `ProviderShellProposal` /
  `execute_shell` signature at `crates/governance-port/src/port.rs:79-93`.
  Its default implementation fails closed. Production wiring is at
  `crates/runtime-host/src/governance_port.rs:78-92,191-228`, and the checked
  receipt conversion verifies `program == "shell"` plus
  `requested.argv == [approved_command]` at
  `crates/governance-port/src/outcome.rs:298-328`.

### Type and visibility proof: no structured-catalog bypass

The guarantee is structural, not a caller convention:

1. `SealedShellSpec` and `SealedProcessSpec` are different public types.
   `SealedShellSpec.inner` is private, and there is no `Deref`, `AsRef`, or
   conversion into `SealedProcessSpec`.
2. The public structured `reserve` / `execute` methods still accept only
   `&SealedProcessSpec`; the shell methods accept only `&SealedShellSpec`.
   Only crate-private `SealedSpecRef` can unwrap either type into the common
   private governance implementation.
3. The structured `process.run@1` host entry still parses a
   `ProviderProcessProposal` and calls only the structured seal. The shell port
   accepts only `ProviderShellProposal`; the proposal types cannot be passed to
   each other's trait methods.
4. The compile-fail doctest at
   `crates/tool-executor/src/process.rs:287-293` proves that a shell authority
   cannot be supplied to a structured-only signature. It passed in the current
   run.
5. A source-to-`HEAD` exact-function comparison returned `True` for both
   existing `pub fn seal` and `pub fn validate_program_catalog`. The current
   structured seal is at `crates/tool-executor/src/process.rs:684-710` and
   still unconditionally calls the unchanged catalog validator at line 691.
   The unchanged validator is at
   `crates/tool-executor/src/process.rs:1349-1382`. The focused negative test at
   `crates/tool-executor/src/process.rs:1906-1930` also proves that structured
   `seal(..., "shell", ...)` remains rejected.

### Human-only approval and exact command identity

- Shell proposals select `ApprovalPolicy::HumanOnly`; the host assigns high
  risk, forces `RequireConfirmation`, skips permission-rule evaluation, and
  exposes an empty rule subject at
  `crates/runtime-host/src/host/process.rs:951-987`. The existing renderer uses
  an empty rule subject to suppress both "always" actions. No rule receipt is
  created, even when a global process allow rule exists.
- The ordinary public decision API accepts only approve-once or reject. The
  runtime integration test at
  `crates/runtime-host/tests/governed_process_bridge.rs:268-356` installs a
  global allow rule, observes a still-pending shell proposal, approves it
  manually, checks the exact command in the approval view and receipt, checks
  captured output, checks the absence of rule attribution, and confirms the
  structured `program=shell` direction remains rejected.
- The complete command is never shortened. It is sealed as `argv[0]`, hashed as
  the shell proposal subject, used to build the interpreter argv, copied into
  `GovernedShellReceipt.approved_command`, and checked again by the opaque port
  outcome. Debug implementations redact the raw command.
- The existing approval component receives the full argv and JSON-encodes every
  complete argument at
  `apps/desktop/src/features/execution/executionTypes.ts:98-104`; it renders
  that value at
  `apps/desktop/src/features/execution/CommandApprovalBar.tsx:180-202` and also
  renders the full reversible process summary at lines 221-227. Because the
  closed renderer gateway admits at most 1,024 summary characters
  (`apps/desktop/src/lib/tauriExecGateway.ts:51-55`), shell seal rejects a
  command whose complete reversible summary cannot fit, at
  `crates/tool-executor/src/process.rs:93,1275-1284`. It never truncates and
  then asks for approval. This uses the existing renderer unchanged.

### Pinned upstream `terminal/*` contract and implementation

The pinned primary schema is
`agent-client-protocol-schema-1.5.0/src/v1/client.rs`:

| Method / capability | Upstream request and response evidence | Pillow implementation |
|---|---|---|
| `terminal/create` | request `sessionId`, `command`, `args`, `env`, absolute optional `cwd`, optional `outputByteLimit` at `1127-1176`; response `terminalId` at `1240-1260` | normalization `crates/adapter-grok-acp/src/provider_terminals.rs:205-269`; dispatch `crates/adapter-grok-acp/src/prompt.rs:856-1027` |
| `terminal/output` | request/response at `1284-1358` | method/response registry at `crates/adapter-grok-acp/src/provider_terminals.rs:13-17,95-157`; dispatch `crates/adapter-grok-acp/src/prompt.rs:772-854` |
| `terminal/release` | request/response at `1391-1455` | same registry and dispatch |
| `terminal/kill` | request/response at `1477-1541` | same registry and dispatch |
| `terminal/wait_for_exit` | request/response at `1563-1631`; exit status at `1655-1682` | same registry and dispatch |
| `ClientCapabilities.terminal` | promises all `terminal/*` methods at `1737-1741`; exact method constants at `2290-2298` | capability/profile conjunction `crates/adapter-grok-acp/src/rpc/methods.rs:46-62,169-200` |

S1 deliberately accepts the production Grok shell-string branch: the original
`command`, empty/missing `args`, empty/missing `env`, project-scoped absolute
`cwd`, and an optional bounded output limit. Non-empty `args` or `env` fail
closed because they would add execution material that was not the one raw
command approved in this stage. Output retention follows the upstream rule:
discard from the beginning at a UTF-8 character boundary.

The five-method fixture reaches `terminal/create`, receives a real governed
terminal id only after one human approval and a durable terminal receipt, then
successfully calls wait, output, kill, and release. Its integration test is at
`crates/adapter-grok-acp/tests/governed_process_roundtrip.rs:200-227`.

## S1 continuation exit-code table

Every Cargo command below set
`CARGO_TARGET_DIR=<repo>/target-shell-s1`. No command was piped through `tail`
or any other exit-code-masking consumer.

| Gate / probe | Command | Exit code | Result | Notes |
|---|---|---:|---|---|
| Shell API RED | `cargo test -p keeper-tool-executor shell_ --lib` | 1 | EXPECTED-RED | Parallel shell APIs did not exist. |
| First shell compile | same | 1 | EXPECTED-RED | Lifetime on the private sealed dispatcher was then corrected. |
| Protocol receipt RED | `cargo test -p keeper-agent-protocol governed_shell_receipt --lib` | 1 | EXPECTED-RED | Shell receipt types did not exist. |
| Governance port | `cargo test -p keeper-governance-port --lib` | 0 | PASS | 12/12. |
| Runtime typecheck | `cargo check -p keeper-runtime-host` | 0 | PASS | Host and port wiring typechecked. |
| Mistyped exact filter | `cargo test -p keeper-runtime-host --test governed_process_bridge shell_command_ignores_auto_allow_and_retains_the_exact_approved_subject_and_output -- --exact` | 0 | NON-EVIDENCE | Ran zero tests; not counted as acceptance. |
| Runtime shell focus | same command without `--exact` | 0 | PASS | 1/1. |
| Terminal normalizer focus | `cargo test -p keeper-adapter-grok-acp provider_terminals --lib` | 0 | PASS | 2/2. |
| First terminal roundtrip | `cargo test -p keeper-adapter-grok-acp standard_terminal_methods_roundtrip_through_one_human_approved_shell_receipt --test governed_process_roundtrip` | 1 | EXPECTED-RED | Fake child still enforced the old two-tool S0 profile. |
| Terminal roundtrip after fixture sync | same | 0 | PASS | 1/1. |
| Format check before formatting | `cargo fmt --all -- --check` | 1 | EXPECTED-RED | Reported Rust formatting differences. |
| Format application | `cargo fmt --all` | 0 | PASS | Rust sources formatted. |
| Approval-summary RED | `cargo test -p keeper-tool-executor shell_seal_rejects_a_command_that_cannot_fit_the_untruncated_approval_summary --lib` | 1 | EXPECTED-RED | Exposed the pre-approval display-budget mismatch. |
| Approval-summary GREEN | same | 0 | PASS | Over-budget commands now reject before seal. |
| Agent protocol full lib | `cargo test -p keeper-agent-protocol --lib` | 0 | PASS | 15/15. |
| Governance port full lib, final | `cargo test -p keeper-governance-port --lib` | 0 | PASS | 12/12. |
| Tool executor full lib, final | `cargo test -p keeper-tool-executor --lib` | 0 | PASS | 21/21. |
| Tool executor type proof | `cargo test -p keeper-tool-executor --doc` | 0 | PASS | 1/1 compile-fail doctest. |
| Runtime governed bridge, final | `cargo test -p keeper-runtime-host --test governed_process_bridge` | 0 | PASS | 30/30. |
| ACP adapter full lib, final | `cargo test -p keeper-adapter-grok-acp --lib` | 0 | PASS | 69/69. |
| ACP governed roundtrip, final | `cargo test -p keeper-adapter-grok-acp --test governed_process_roundtrip` | 0 | PASS | 9/9, including all five terminal methods. |
| Contributor gate via PowerShell shim | `pnpm check` | 1 | LAUNCHER-BLOCKED | Local script execution policy rejected the shim before the gate. |
| One-second launcher diagnostic | `pnpm.cmd check` | 124 | NON-EVIDENCE | Worker was intentionally cut off after one second; not a gate result. |
| Contributor gate, compliant launcher | `pnpm.cmd check` | 1 | `ENV-BLOCKED` | Workspace and secret checks passed; the cargo-format wrapper could not query the Windows process snapshot (`Get-CimInstance Win32_Process`, permission denied, `HRESULT 0x80041003`). No script was changed and no alternate gate was used. |

The overall contributor gate is therefore `ENV-BLOCKED`, not green. Desktop
E2E/smoke was not run, as required.

## Final write-boundary audit

The continuation changed only the approved implementation and focused-test
surfaces under:

- `crates/tool-executor/src/{lib.rs,process.rs}`;
- `crates/agent-protocol/src/{governed.rs,lib.rs}`;
- `crates/governance-port/src/{lib.rs,outcome.rs,port.rs}`;
- `crates/runtime-host/src/**` exports/process/port wiring and
  `crates/runtime-host/tests/governed_process_bridge.rs`;
- `crates/adapter-grok-acp/src/**` terminal routing/profile fixtures and
  `crates/adapter-grok-acp/tests/governed_process_roundtrip.rs`;
- this append-only report.

`git diff --name-only` shows no changes under `scripts/**`,
`apps/desktop/src/**`, or migration paths, and no `Cargo.lock` change. The
renderer sources cited above were read-only evidence. No git write operation
was performed.

## Residual risks and process-start audit

- The current governed execution port is intentionally synchronous. Therefore
  `terminal/create` returns after the command has a durable terminal; output
  and wait then return retained completed state, and kill is an idempotent
  acknowledgement for that known completed terminal. Turn cancellation and the
  sealed 60-second timeout still cancel the running supervised Job while create
  is blocked. Live post-create polling/kill would require a separately accepted
  asynchronous handle design; this S1 does not claim interactive terminal
  semantics.
- Non-empty upstream `args` and `env` are rejected in S1. Supporting them would
  require a new approval-subject design rather than silently appending hidden
  execution material.
- Exact command text and output are memory-only. SQLite retains proposal/output
  digests and terminal metadata; after restart a replay can prove the same
  fingerprint but cannot reopen the raw transcript. The returned shell receipt
  still names the exact approved command.
- The inline command row has an existing ellipsis style, but the same approval
  card receives and renders the complete reversible summary. Commands that
  cannot fit that untruncated summary are rejected before approval. No desktop
  screenshot/E2E was permitted in this order, so visual Windows-size acceptance
  remains a controller gate.
- Product code still has intentionally non-process-approval OS child paths:
  the ACP provider launcher at
  `crates/adapter-grok-acp/src/process/child.rs:59-98`, and fixed read-only Git
  templates at `crates/git-executor/src/runner.rs:581-608`. Both use the common
  supervised process runner, whose only ordinary child spawn is
  `crates/process-executor/src/lib.rs:168-223`; its Unix cleanup helper invokes
  `kill` at line 417. These are infrastructure/read-tool paths, not agent shell
  proposals. Operational `scripts/**` can also start processes outside product
  governance and was not changed here.
- A future profile regression that enables a native shell hand while disabling
  ACP terminal support could still select agent-side LocalTerminal. The source
  coupling test prevents the shipped half-open combination. The controller's
  recorded real-Grok observation supplies the other half of acceptance:
  `terminal/create` reached the client and the agent did not create the probe
  file itself. This worker did not rerun that external desktop probe.

GOVERNED-SHELL-S1-COMPLETE
