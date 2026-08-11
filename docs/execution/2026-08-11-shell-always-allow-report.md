# Shell Always-Allow Execution Report

Status: IMPLEMENTATION COMPLETE; WORKER GATE ENV-BLOCKED

Completion signal: `SHELL-ALWAYS-ALLOW-COMPLETE`

## Scope and Constraints

- This is one coordinated host/renderer change for shell proposal rule
  segments. The host DTO, renderer closed-key gateway, approval bar, and rule
  dialog landed together.
- The report skeleton was the first task-owned file created.
- No desktop E2E was run; controller ownership of desktop E2E was respected.
- No Git write operation was run.
- Nothing under `scripts/` was changed.
- Every Cargo invocation used the controller-supplied task target directory.
  Its machine-local absolute path is intentionally omitted from this report.
- The pre-existing modification to
  `docs/handoff/2026-08-11-pillow-controller-handoff.md` was not touched.

## Exit Code Table

Commands below were run without an output pipeline. TDD red runs are retained
to show that the new accept path, not only rejection/rendering, was exercised.

| Phase | Command | Exit code | Result |
| --- | --- | ---: | --- |
| Host RED | `cargo test -p keeper-runtime-host --test governed_process_bridge reliable_shell_rule_segments_survive_live_and_stored_decision_views -- --exact --nocapture` | 101 | Expected compile failure before `rule_segments` existed. |
| Host GREEN | Same focused host test | 0 | 1 passed; live and stored decision views preserve the normalized two-segment batch. |
| Rust DTO RED | `cargo test -p pillow-desktop --lib exec_commands_tests::proposal_projection_serializes_portable_project_root_and_relative_cwd -- --exact --nocapture` | 101 | Expected failure before the camelCase serialized field existed. |
| Rust DTO GREEN | Same focused DTO test | 0 | 1 passed; present and omitted serialization directions both asserted. |
| Renderer gateway RED | `pnpm exec vitest run tests/c4-gateway-contract.test.ts` | 1 | The valid `ruleSegments` payload was rejected by the old closed-key allowlist. |
| Renderer gateway GREEN | Same gateway contract file | 0 | 14 passed, including present, malformed, and absent directions. |
| Approval UI RED | `pnpm exec vitest run tests/permission-rules-ui.test.tsx` | 1 | 2 expected failures: no conjunction copy and unsafe shell fallback action still visible. |
| Approval UI GREEN | Same permission UI file | 0 | 12 passed after fail-closed and sequential rule-writing behavior landed. |
| Workbench seam RED | Focused `c4-workbench-process-integration` rule-segment forwarding test | 1 | Temporarily withheld the prop; the integration surface showed the blocked copy. |
| Workbench seam GREEN | Same focused integration test | 0 | 1 passed; the gateway value reaches the approval bar. |
| Exact shell pattern RED/GREEN | Focused permission UI exact-pattern assertion | 1 / 0 | Proved a segment is not generalized to a non-matching trailing-wildcard suggestion. |
| Multi-rule scope lock RED/GREEN | Focused permission UI scope-lock assertion | 1 / 0 | Proved sequential allow remains global and cannot be switched to project scope. |
| Formatting | `cargo fmt --all -- --check` | 0 | Passed. |
| TypeScript typecheck | `pnpm typecheck` | 0 | Passed after one TDD compile correction to retain the validated array narrowing. |
| Focused renderer pack | Five affected Vitest files | 0 | 5 files, 91 tests passed. |
| Runtime-host integration | `cargo test -p keeper-runtime-host --test governed_process_bridge -- --nocapture` | 0 | 34 passed. |
| Desktop Rust DTO/unit slice | `cargo test -p pillow-desktop --lib exec_commands_tests:: -- --nocapture` | 0 | 7 passed. |
| Permission policy regression | `cargo test -p keeper-policy --test permission_rules -- --nocapture` | 0 | 11 passed. |
| Renderer full unit suite | `pnpm test` | 0 | 35 files, 449 tests passed. Expected mock/error-path stderr did not fail tests. |
| Renderer production build | `pnpm --filter @pillow/desktop build` | 0 | Passed; 99 modules transformed. |
| Rust workspace test | `cargo test --workspace` | 101 | `ENV-BLOCKED`: the desktop lib reached 152 passed and 1 failed; `path_scope::tests::picker_selection_rejects_reparse_point_components` could not create its test symlink because Windows returned OS 1314 (missing privilege). The untouched test failed at environment setup, not at a product assertion. |
| Clippy | `cargo clippy -p pillow-desktop -- -D warnings` | 0 | Passed. |
| Clippy | `cargo clippy -p keeper-adapter-grok-acp -- -D warnings` | 0 | Passed. |
| Clippy | `cargo clippy -p keeper-agent-protocol -- -D warnings` | 0 | Passed. |
| Clippy | `cargo clippy -p keeper-git-executor -- -D warnings` | 0 | Passed. |
| Clippy | `cargo clippy -p keeper-governance-port -- -D warnings` | 0 | Passed. |
| Clippy | `cargo clippy -p keeper-policy -- -D warnings` | 0 | Passed. |
| Clippy | `cargo clippy -p keeper-project -- -D warnings` | 0 | Passed. |
| Clippy | `cargo clippy -p keeper-resource -- -D warnings` | 0 | Passed. |
| Clippy | `cargo clippy -p keeper-session -- -D warnings` | 0 | Passed. |
| Clippy | `cargo clippy -p keeper-knowledge-indexer -- -D warnings` | 0 | Passed. |
| Clippy | `cargo clippy -p keeper-process-executor -- -D warnings` | 0 | Passed. |
| Clippy | `cargo clippy -p keeper-runtime-host -- -D warnings` | 0 | Passed. |
| Clippy | `cargo clippy -p keeper-storage-sqlite -- -D warnings` | 0 | Passed. |
| Clippy | `cargo clippy -p keeper-tool-executor -- -D warnings` | 0 | Passed. |
| Worker gate | `node scripts/run-worker-gate.mjs` | 1 | `ENV-BLOCKED` at `cargo-fmt`: the gate's process supervisor could not query `Win32_Process` through CIM (`0x80041003`, permission denied). Workspace and secrets checks passed before the block. No bypass was attempted and no script was changed. |
| Desktop E2E | Prohibited by task | N/A | Not run; controller-owned. |
| Patch hygiene | `git diff --check` | 0 | Passed; line-ending notices only. |

## Host and Renderer Contract Evidence

The single field is carried through one closed seam:

1. `crates/runtime-host/src/host/process.rs` adds
   `LiveProcessProposalView.rule_segments`. For `ShellRules`, the host calls
   `conservative_shell_command_segments` once, uses a clone of that exact
   ordered vector for conjunction evaluation, and publishes the other clone.
   It never falls back to the raw command. Structured proposals publish
   `None`; shell `rule_subject` remains the empty string.
2. `live_view_from_entry` copies the live value. On the governable decision
   projection, `view_from_stored` receives the original one-element shell argv
   and applies the same shared conservative segmenter, so that replay path
   retains the same normalized subjects. Unreliable input remains `None` on
   both paths. A storage-only recovery row intentionally receives no raw argv
   and has `decision_available = false`; it therefore omits the field instead
   of inventing a subject. The existing digest-only storage boundary is
   unchanged.
3. `apps/desktop/src-tauri/src/conversation_service/exec_service.rs` adds
   `ExecProposalDto.rule_segments: Option<Vec<String>>` with
   `skip_serializing_if = "Option::is_none"`. Existing camelCase serde policy
   emits exactly `ruleSegments`.
4. `apps/desktop/src/lib/tauriExecGateway.ts` admits exactly `ruleSegments` in
   `proposalKeys`, then validates it as a non-empty array of at most 64
   subjects. Every subject must be a trimmed-nonempty string of at most 8192
   characters, reusing `isRuleSubjectValue`. The field is accepted only for a
   shell proposal and copied into the normalized DTO.
5. `apps/desktop/src/lib/execGateway.ts` declares the optional renderer field;
   `Workbench` forwards it unchanged to `CommandApprovalBar`. No renderer
   shell splitting or raw-command fallback exists.

This mapping pins the host-emitted key and renderer allowlist/validator in the
same change, including an accept-direction test.

## Renderer Validation Directions

`apps/desktop/tests/c4-gateway-contract.test.ts` covers all three required
directions:

- Present and valid: a shell proposal with
  `ruleSegments: ["cargo test", "cargo build"]` is accepted and both values
  are preserved.
- Malformed or misplaced: a string, an empty array, an empty item, a non-string
  item, 65 items, an item longer than 8192 characters, and use on a structured
  proposal are all rejected.
- Absent: both a structured proposal and an unreliable quoted shell proposal
  remain accepted without the optional field.

The Rust serialization test separately asserts that structured `None` omits
the key while shell `Some` emits `ruleSegments`.

## Approval UI Copy and Conjunctive Meaning

For a reliable two-segment shell command, the bar shows this exact text:

> 该 Shell 命令已拆为 2 段；自动放行必须分别保存 2 条允许规则。

Its entry button says:

> 逐段设置总是允许（2 条规则）

The dialog then says:

> 此 Shell 命令已安全拆为 2 段。自动放行要求每一段都有一条单独的允许规则；缺少任一条，整条命令仍需逐次确认。请逐段保存。

It displays `第 1 / 2 段`, then `第 2 / 2 段`, and writes through the existing
`add_permission_rule` path only when the user explicitly clicks
`保存第 1 条规则并继续` and `保存第 2 条规则并完成`. Tests assert two calls with
the exact host subjects in order, not merely that a dialog rendered.

For an unreliable shell command, both durable-rule buttons are absent and the
bar shows this exact fail-closed explanation:

> 命令结构过于复杂，无法安全拆分；本次需逐次确认，不提供“总是允许”。

No screenshot was taken because desktop E2E is controller-exclusive; the exact
source copy and renderer assertions are provided instead, as allowed by the
task.

## Write-Boundary and Invariant Audit

Task-owned changes are limited to:

- `crates/runtime-host/src/host/process.rs`
- `crates/runtime-host/tests/governed_process_bridge.rs`
- `apps/desktop/src-tauri/src/conversation_service/exec_service.rs`
- `apps/desktop/src-tauri/src/exec_commands_tests.rs`
- `apps/desktop/src/lib/execGateway.ts`
- `apps/desktop/src/lib/tauriExecGateway.ts`
- `apps/desktop/src/features/workbench/Workbench.tsx`
- `apps/desktop/src/features/execution/CommandApprovalBar.tsx`
- `apps/desktop/src/features/permissions/AlwaysRuleDialog.tsx`
- `apps/desktop/src/features/conversations/WriteApprovalBar.tsx`
- `apps/desktop/src/styles/execution.css`
- `apps/desktop/tests/c4-gateway-contract.test.ts`
- `apps/desktop/tests/c4-workbench-process-integration.test.tsx`
- `apps/desktop/tests/permission-rules-ui.test.tsx`
- this report

Boundary findings:

- Automatic approval still runs the existing host evaluation and receipt
  persistence path. No ledger, auto-execution, or decision code was bypassed.
  The runtime-host integration suite includes the conjunction receipt test.
- Shell UI creation is allow-only and global while saving multiple segments;
  action and scope selectors are locked. The existing dialog still permits
  project-scoped deny, never project-scoped allow.
- The change does not read authorization from the project directory and does
  not alter configuration lookup.
- The policy evaluator is unchanged: deny still dominates allow and no match
  still requires confirmation. Focused policy and runtime-host regression
  suites passed.
- Unreliable shell input publishes no segments, receives no always-allow
  entry, and never becomes a full-command rule.
- No dependency, migration, package script, release artifact, Git index, or
  commit was changed.

## Residual Risk and Controller Acceptance

- The repository worker gate remains `ENV-BLOCKED` by the sandbox's CIM
  process-snapshot denial. The direct workspace test is separately
  `ENV-BLOCKED` by symlink privilege. Neither condition was bypassed.
- Segment rules are deliberately saved one at a time. If the user cancels
  after saving an earlier segment, that explicit global allow remains, but the
  full shell command still cannot auto-run until every segment is allowed.
- Restart recovery deliberately cannot reconstruct segments from an argv
  digest. Such storage-only rows are not decision-capable and omit the optional
  field; no raw shell command was added to durable storage for this feature.
- The strict renderer cap is 64 segments. A pathological host payload above
  that cap is rejected rather than partly displayed.
- Controller acceptance still requires the native full gate, desktop E2E, and
  the live conjunction probe. The live probe must establish both that the
  command reached the client and that the proxy did not execute it itself.
