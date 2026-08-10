# Permission Rules Wiring Execution Report

- Worker: `codex1`
- Controller: Claude
- Workspace: `<repo>`
- Required Cargo target: `<repo>/target-perm-wire`
- Started: `2026-08-10T15:43:13.9896022+09:00`
- Completion signal: `PERMISSION-RULES-WIRING-COMPLETE` (only after all required evidence is recorded)
- Overall handoff status: **BLOCKED-BY-CONCURRENT / ENV-BLOCKED at the mandatory workspace gate**. T1-T4 and all task-focused tests are green; the completion signal is intentionally not asserted.

## Scope and safety constraints

- No desktop E2E.
- No Git writes (`add`, `commit`, `push`, `stash`, `checkout`, or equivalents).
- Do not touch the concurrent `directory_ops.rs`, `path_scope.rs`, or their in-flight tests.
- Allowed writes are limited to the requested runtime-host, storage-sqlite, kernel-policy, Tauri state assembly, their tests, and this report.
- The workspace gate is run only with `CARGO_TARGET_DIR=<repo>/target-perm-wire`.
- Concurrent-worker failures are reported as `BLOCKED-BY-CONCURRENT`; they are not repaired here.

## Live baseline

- Branch: `main`
- Initial HEAD: `cb403dba1dfe080de0118fdf42d24c75ba101fcb`
- Initial `git status --short`: no entries were printed.
- Git emitted a read-only warning that `<user-profile>/.config/git/ignore` was inaccessible; no Git write was attempted. The machine-specific profile path is intentionally redacted.

## T1 original conclusion: command-to-host live visibility

Status: **PASS (original topology; no production connection change)**.

The integration test added a permission rule through the same command-facing path used by Tauri state, then submitted/evaluated the next matching proposal through the host path in the same process without reopening the service. The outcome below was captured before any production connection-strategy change:

- Original result: PASS. The host opened and retained its permission-rule repository first; the desktop command path then opened a second repository against the same file and inserted a global process allow. Without reopening or restarting the host, the next matching `node --version` proposal traversed `requested -> decided -> started` and completed `Succeeded` with no UI decision available.
- Proof boundary: Rust integration test `apps/desktop/src-tauri/tests/permission_rules_wiring.rs::command_rule_added_after_host_open_auto_decides_next_matching_proposal`. It compiles the production `permission_rule_commands.rs` command implementation into the integration-test crate, invokes `add_permission_rule_impl`, then drives the public runtime-host governed-process boundary.
- Failure shape: none observed. The sequential committed command write was immediately visible to the already-open host connection; no `SQLITE_BUSY` occurred in this run.
- Raw evidence/log: `target-perm-wire/permission-rules-wiring-logs/t1-original-rerun.log` (`1 passed`, exit `0`). The preceding cold-build attempt was terminated by the outer command timeout while still compiling `webview2-com`; it never entered the test and is not classified as RED.

## T2 connection strategy decision

Status: **implemented and focused-green**.

- Chosen option: **(a) retain separate connections and add `busy_timeout` hardening**.
- Evidence-based rationale: T1 proved SQLite commit visibility across the command and host connections without restart. Sharing an `Arc` is therefore not required for correctness of live visibility; the remaining evidenced risk is transient lock contention during overlapping access, which `busy_timeout` addresses without widening desktop/runtime composition APIs.
- Implementation: `SqlitePermissionRuleRepository::open` explicitly sets `busy_timeout(Duration::from_secs(5))` before migrations/queries. Both the command and host rule repositories use this constructor, so brief cross-connection contention waits instead of immediately surfacing `SQLITE_BUSY`.
- Dependency finding: `rusqlite 0.32.1` currently installs a 5000ms default handler itself, which made the first short-lock characterization pass before Pillow changed. Rusqlite documents that default as changeable, so Pillow now pins its own value. The explicit-configuration test was observed RED (`101`) before the implementation and green afterward.
- Scope rationale: no shared `Arc` or desktop/runtime API widening was needed. Only the permission-rule connections named by T1/T2 were changed; unrelated repositories retain their existing policies.

## T3 automatic approval ledger evidence

Status: **covered by cross-component integration test**.

Required proof: matching rule -> proposal automatically decided -> emitted event payload contains `auto_approved_by_rule` -> durable receipt identifies both the rule and its content snapshot at decision time.

Evidence: `apps/desktop/src-tauri/tests/permission_rules_wiring.rs::command_rule_added_after_host_open_auto_decides_next_matching_proposal` asserts all of the following in one process:

- the command-created rule causes the next proposal to emit `requested -> decided -> started` and finish `Succeeded` without UI input;
- the decided payload carries `auto_approved_by_rule = { prefix: process, pattern: node --version, action: allow }`;
- the durable exec receipt records `approve_by_rule`, `rule_engine`, the exact rule id, global scope, process prefix, pattern/action snapshot, 71-byte `sha256:` content digest, and match subject;
- deleting the live rule afterward does not remove or alter the historical receipt snapshot.

## T4 invariant audit

Status: **verified**.

1. Project scope only narrows authority: storage `project_scope_explicitly_rejects_allow_before_sql`, desktop command `command_explicitly_rejects_project_allow`, and kernel-policy `project_allow_is_never_an_authority_even_if_a_corrupt_input_reaches_policy` cover normal and corrupt-input rejection. `effective_rules_are_global_allow_or_deny_plus_current_project_deny_only` covers the positive global allow/deny plus project-deny set.
2. Project-directory configuration is never read as an authorization source: the existing source-boundary test remains; new runtime-host integration `project_directory_permission_files_never_authorize_and_no_match_requires_confirmation` places both `.pillow/permissions.toml` and `.grok/config.toml` allow text in the project, proves the proposal still waits for a human decision with no rule attribution, then explicitly rejects it.
3. No match requires confirmation; deny takes precedence over allow: kernel-policy tests `no_matching_rule_still_requires_confirmation` and `deny_beats_allow_regardless_of_input_order` pass, and runtime-host `matching_project_deny_beats_global_allow_before_any_write` proves the deny wins before filesystem mutation.

Each invariant above points to both the authority-granting direction and its fail-closed counterpart; no rejection-only validator is treated as sufficient proof.

## Command exit codes

Commands are run without output pipelines. Logs and exact exit codes are recorded here as evidence accumulates.

| Phase | Command | Log | Exit code | Result |
|---|---|---|---:|---|
| Baseline | `git status --short; git branch --show-current; git rev-parse HEAD` | terminal capture | `0` | Read-only live snapshot; no status entries |
| T1 cold-build attempt | `cargo test -p pillow-desktop --test permission_rules_wiring command_rule_added_after_host_open_auto_decides_next_matching_proposal -- --exact --nocapture` | `target-perm-wire/permission-rules-wiring-logs/t1-original.log` | `124` (outer tool timeout) | Stopped during dependency compilation; test did not run |
| T1 original proof rerun | same exact test command | `target-perm-wire/permission-rules-wiring-logs/t1-original-rerun.log` | `0` | PASS: 1 passed, 0 failed |
| T2 initial short-lock characterization | `cargo test -p keeper-storage-sqlite --test permission_rules_repository permission_rule_connection_waits_for_a_short_cross_connection_write_lock -- --exact --nocapture` | `target-perm-wire/permission-rules-wiring-logs/t2-busy-timeout-red.log` | `0` | Existing rusqlite 5s default already waited; not a valid RED |
| T2 explicit-config RED | `cargo test -p keeper-storage-sqlite --test permission_rules_repository permission_rule_repository_explicitly_pins_busy_timeout -- --exact --nocapture` | `target-perm-wire/permission-rules-wiring-logs/t2-explicit-busy-timeout-red.log` | `101` | Expected RED: Pillow did not explicitly pin timeout |
| T2 storage green | `cargo test -p keeper-storage-sqlite --test permission_rules_repository -- --nocapture` | `target-perm-wire/permission-rules-wiring-logs/t2-storage-green.log` | `0` | PASS: 8 passed, 0 failed |
| T3 event + receipt | T1 exact integration test after full receipt assertions | `target-perm-wire/permission-rules-wiring-logs/t3-event-receipt.log` | `0` | PASS: 1 passed, 0 failed |
| T4 project files / no match | `cargo test -p keeper-runtime-host --test governed_process_bridge project_directory_permission_files_never_authorize_and_no_match_requires_confirmation -- --exact --nocapture` | `target-perm-wire/permission-rules-wiring-logs/t4-project-files-no-match.log` | `0` | PASS: 1 passed, 0 failed |
| T4 pure policy | `cargo test -p keeper-policy --test permission_rules -- --nocapture` | `target-perm-wire/permission-rules-wiring-logs/t4-kernel-policy.log` | `0` | PASS: 6 passed, 0 failed |
| T4 deny over allow | `cargo test -p keeper-runtime-host --test governed_write_bridge matching_project_deny_beats_global_allow_before_any_write -- --exact --nocapture` | `target-perm-wire/permission-rules-wiring-logs/t4-deny-over-allow.log` | `0` | PASS: 1 passed, 0 failed |
| T4 command project allow | `cargo test -p pillow-desktop --lib permission_rule_commands_tests::command_explicitly_rejects_project_allow -- --exact --nocapture` | `target-perm-wire/permission-rules-wiring-logs/t4-command-project-allow.log` | `0` | PASS: 1 passed, 0 failed |
| Format precheck | `cargo fmt --all -- --check` | `target-perm-wire/permission-rules-wiring-logs/fmt-check-before-gate.log` | `1` | Reported only the new integration test; no concurrent file named |
| Scoped formatting | `rustfmt --edition 2024 apps/desktop/src-tauri/tests/permission_rules_wiring.rs` | terminal capture | `0` | Formatted only the task-owned file |
| Format green | `cargo fmt --all -- --check` | `target-perm-wire/permission-rules-wiring-logs/fmt-check-green.log` | `0` | PASS |
| Worker gate attempt 1 | `$env:CARGO_TARGET_DIR='<repo>/target-perm-wire'; node scripts/run-worker-gate.mjs` | `target-perm-wire/permission-rules-wiring-logs/worker-gate.log` | `1` | Secrets gate caught a machine-specific user-profile path in this report; report redacted before rerun |
| Worker gate attempt 2 | same required worker-gate command | `target-perm-wire/permission-rules-wiring-logs/worker-gate-rerun.log` | `1` | **ENV-BLOCKED** before cargo-fmt: process supervisor could not query `Win32_Process` (`PermissionDenied`, `0x80041003`) |
| Direct workspace structure | `node scripts/check-workspace.mjs` | `target-perm-wire/permission-rules-wiring-logs/direct-workspace-structure.log` | `0` | PASS |
| Direct secrets | `node scripts/check-secrets.mjs` | `target-perm-wire/permission-rules-wiring-logs/direct-secrets.log` | `0` | PASS |
| Direct format | `cargo fmt --all -- --check` | `target-perm-wire/permission-rules-wiring-logs/direct-cargo-fmt.log` | `0` | PASS |
| Direct typecheck | `corepack.cmd pnpm typecheck` | `target-perm-wire/permission-rules-wiring-logs/direct-typecheck.log` | `0` | PASS |
| Direct package tests | `corepack.cmd pnpm test` | `target-perm-wire/permission-rules-wiring-logs/direct-package-tests.log` | `0` | PASS: 35 files, 432 tests |
| Direct Rust workspace tests | `cargo test --workspace` | `target-perm-wire/permission-rules-wiring-logs/direct-cargo-test-workspace.log` | `101` | **BLOCKED-BY-CONCURRENT**: 150 desktop lib tests passed, but `path_scope.rs:1149` failed to create its symlink fixture with Windows privilege error 1314 |
| Direct Clippy | `cargo clippy --workspace -- -D warnings` | `target-perm-wire/permission-rules-wiring-logs/direct-cargo-clippy.log` | `0` | PASS |
| Direct ACP drift | `node scripts/check-acp-protocol-drift.mjs` | `target-perm-wire/permission-rules-wiring-logs/direct-acp-protocol-drift.log` | `0` | Gate-defined offline soft skip (`未核对`) |
| Final desktop wiring | `cargo test -p pillow-desktop --test permission_rules_wiring -- --nocapture` | `target-perm-wire/permission-rules-wiring-logs/final-focused-desktop-wiring.log` | `0` | PASS: 1 passed, 0 failed |
| Final storage rules | `cargo test -p keeper-storage-sqlite --test permission_rules_repository -- --nocapture` | `target-perm-wire/permission-rules-wiring-logs/final-focused-storage.log` | `0` | PASS: 8 passed, 0 failed |
| Final host no-file/no-match | exact runtime-host process test | `target-perm-wire/permission-rules-wiring-logs/final-focused-process.log` | `0` | PASS: 1 passed, 0 failed |
| Final host deny-over-allow | exact runtime-host write test | `target-perm-wire/permission-rules-wiring-logs/final-focused-write-deny.log` | `0` | PASS: 1 passed, 0 failed |
| Final pure policy | `cargo test -p keeper-policy --test permission_rules -- --nocapture` | `target-perm-wire/permission-rules-wiring-logs/final-focused-policy.log` | `0` | PASS: 6 passed, 0 failed |

## Changes and rationale

- Added one desktop Rust integration test that crosses the command/store/runtime/event/receipt boundaries without widening production command visibility.
- Explicitly pinned a 5-second busy timeout on every `SqlitePermissionRuleRepository` connection.
- Added storage tests for real short-lock waiting and the explicit timeout contract.
- Added a runtime-host test proving project-directory permission text is inert and no-match remains interactive.

## Write-boundary audit

Final live status before report close listed exactly five task-owned paths:

- `crates/storage-sqlite/src/permission_rules.rs` — allowed production hardening.
- `crates/storage-sqlite/tests/permission_rules_repository.rs` — allowed storage tests.
- `crates/runtime-host/tests/governed_process_bridge.rs` — allowed runtime-host integration test.
- `apps/desktop/src-tauri/tests/permission_rules_wiring.rs` — allowed desktop/Tauri integration test; production command visibility was not widened.
- `docs/execution/2026-08-11-permission-rules-wiring-report.md` — required report.

No renderer, adapter-grok-acp, script, migration, `directory_ops.rs`, `path_scope.rs`, or unrelated crate file was modified. `target-perm-wire/` contains ignored build artifacts/logs only. No Git write operation was run.

## Residual risks and uncovered areas

- Desktop E2E is intentionally not run because it is controller-exclusive.
- The mandatory `node scripts/run-worker-gate.mjs` is not green in this execution environment: its fail-closed process supervisor cannot obtain the Windows process snapshot. No supported bypass exists, and scripts are outside this task's write boundary.
- Direct `cargo test --workspace` is `BLOCKED-BY-CONCURRENT` at the explicitly out-of-scope `apps/desktop/src-tauri/src/path_scope.rs:1149` symlink fixture (Windows privilege error 1314). This worker did not repair or alter that file. Controller should rerun the mandatory gate after the path-scope worker/environment resolves it.
- A 5-second timeout prevents immediate `SQLITE_BUSY` during brief permission-rule connection contention; a lock held longer than the bound can still fail closed with the existing store error.
- The cross-component test invokes the production command implementation rather than a real WebView IPC dispatch. Existing unit coverage pins command registration; real desktop/WebView validation remains controller-owned with desktop E2E.
- ACP drift remained `未核对` because the gate's network fetch was unavailable; the check's defined soft-skip exit was `0`.
