# Role dispatch domain — development report

Date: 2026-08-12  
Worker: grok3 (write)  
Controller: Claude  
Task: controller → role assignment as structured process proposal  
Status: **DONE**

## Objective

Provide a pure domain crate that translates “controller dispatches another
role” into a structured process proposal for the **existing** process
governance chain. No second approval path, no executor, no UI, no storage.

## Write boundary (audit)

| Path | Action |
| --- | --- |
| `crates/role-dispatch/**` | created (crate sources + integration tests) |
| root `Cargo.toml` | added workspace member `crates/role-dispatch` |
| `docs/execution/2026-08-12-role-dispatch-report.md` | this report |

No other trees touched (`apps/desktop/**`, `crates/knowledge-indexer/**`,
host/process/storage crates left alone).

## Package

| Item | Value |
| --- | --- |
| Path | `crates/role-dispatch` |
| Package name | `pillow-role-dispatch` |
| Gate | `cargo test -p pillow-role-dispatch` |
| Dependencies | `serde`, `thiserror`, `uuid` only (no host, storage, network, process) |

## Public surface

| Symbol | Role |
| --- | --- |
| `Assignment` / `NewAssignment` / `AssignmentId` | Aggregate: initiator, target, task, command, frozen governance, state |
| `AssignmentState` | `proposed → approved → running → {succeeded\|failed\|rejected\|cancelled}` |
| `validate_assignment_transition` / `apply_assignment_transition` | Pure state machine |
| `SlotAuthShape` / `SlotRole` / `SlotId` | Pure mirrors of storage vocabulary (no DB dep) |
| `GovernanceCapability::{Full,None}` | Auth-derived; never elevated |
| `ProposedCommand` | `{ program, argv, cwd, governance }` — host handoff to process chain |
| `build_proposed_command` | Auth + task text → structured command or `ProposeError` |
| `build_assignment` | Full propose path with self/missing/role checks |
| `reject_shell_joined_command` | Always `ShellJoinForbidden` (no shell-string path) |
| `AssignmentOutcome` / `attribute_outcome` | Audit attribution; elevation fail-closed |

### State graph

- `Proposed` → `Approved` | `Rejected` | `Cancelled`
- `Approved` → `Running` | `Rejected` | `Cancelled`
- `Running` → `Succeeded` | `Failed` | `Cancelled`
- Terminal states never reopen; same-state is illegal

### Argv construction

- **LocalCli**: `program = executable_path`, `argv = launch_args + [--pillow-task, task]`, `governance = None`
- **ApiKey / GrokLogin**: `program = "grok"` (catalog label), ACP-style tokens + model flag + task flag, `governance = Full`
- Task text is always a **separate argv element** after `--pillow-task`
- Task text never becomes `program` or `cwd`
- Shell-join construction is **always** rejected (`ShellJoinForbidden`)

## 1. Exit codes (real runs)

| Command | Exit | Result |
| --- | --- | --- |
| `cargo test -p pillow-role-dispatch` | **0** | 9 lib unit + 6 transition + 7 governance + 13 propose = **35 passed** |

(`cargo check` was not used as the sole gate — it does not compile `#[cfg(test)]`.)

## 2. State machine evidence (bidirectional)

Integration: `crates/role-dispatch/tests/assignment_transitions.rs`

| Test | What it proves |
| --- | --- |
| `all_legal_transitions_are_accepted` | All 9 legal edges ok via validate + apply |
| `all_illegal_transitions_are_rejected` | Full cartesian product over `AssignmentState::ALL`; every non-legal edge → `IllegalTransition` |
| `apply_rejects_terminal_sources_with_already_terminal` | Terminal sources → `AlreadyTerminal` on apply |
| `happy_path_chain_proposed_to_succeeded` | proposed → approved → running → succeeded |
| `reject_from_proposed_and_cancel_from_running` | Reject / cancel terminal paths |

Lib unit tests in `src/transition.rs` cover the same legal/illegal samples.

## 3. T2 rejection evidence

Integration: `crates/role-dispatch/tests/propose_command.rs`

| Case | Test | Error code |
| --- | --- | --- |
| Self-dispatch | `rejects_self_dispatch` | `role_dispatch_self_dispatch` |
| Target missing (slot) | `rejects_missing_target_slot` | `role_dispatch_target_missing` |
| Target missing (role) | `rejects_missing_target_role` | `role_dispatch_target_missing` |
| Role mismatch | `rejects_role_mismatch` | `role_dispatch_role_mismatch` |
| Shell join always refused | `shell_join_construction_is_always_rejected` | `role_dispatch_shell_join_forbidden` |
| Shell metacharacters stay literal | `structured_path_keeps_shell_metacharacters_as_single_argv_element` | (structured succeeds; no shell string) |
| Metacharacter detector | `shell_metacharacters_in_task_are_detected` | — |
| Task in cwd | `rejects_when_cwd_equals_task_text` | `role_dispatch_task_in_program_or_cwd` |
| Empty task | `rejects_empty_task` | `role_dispatch_task_empty` |

## 4. Ungoverned target not elevated

Integration: `crates/role-dispatch/tests/governance_no_elevate.rs`

| Test | What it proves |
| --- | --- |
| `local_cli_command_capability_is_none` | LocalCli → `GovernanceCapability::None` on command |
| `governed_auth_shapes_are_full` | ApiKey/GrokLogin → `Full` |
| `attribute_rejects_claiming_governed_for_local_cli` | `claimed_governed_execution: true` + `None` → `GovernanceElevationForbidden` |
| `attribute_allows_record_only_for_local_cli` | Record-only path ok when claim is false |
| `assignment_freezes_local_cli_governance_and_blocks_elevation` | Frozen on assignment; attribute path fails closed |

## 5. Residual risks — easiest controller wiring mistakes

1. **Calling a process outside the governed chain**  
   This crate only produces `ProposedCommand`. If the controller (or a future
   host glue) spawns from `program`/`argv` directly instead of
   `process.run@1` / host process coordinator, LocalCli *and* governed targets
   bypass approval and durable receipts. The domain types cannot prevent that
   misuse at the call site.

2. **Elevating LocalCli by ignoring `governance`**  
   A host that treats every assignment as “governed process” and sets
   `claimed_governed_execution: true` without checking
   `GovernanceCapability` will trip `attribute_outcome` — but only if it
   uses that helper. Skipping attribution and writing a custom “governed”
   receipt for LocalCli would falsely upgrade capability.

3. **Shell-wrapping the structured argv**  
   Host must pass `program` + `argv` as structured spawn args. If someone
   joins them into `cmd /C` / `sh -c` for convenience, shell metacharacters
   in the task element become real operators and the conservative shell
   segmenter is bypassed. Role-dispatch refuses to produce shell strings;
   the host must not reintroduce them.

4. **Catalog program `"grok"` resolution**  
   Governed shapes emit catalog label `grok`, not a filesystem path. Host
   must map that label the same way as other process catalog entries.
   Pointing it at an arbitrary binary or filling `program` from task text
   would break the “task never in program” invariant.

5. **Task delivery channel**  
   Task text is in argv after `--pillow-task` for domain uniformity. Real
   agent CLIs may ignore that flag and expect stdin/protocol prompts.
   Wiring must either honor the flag or strip the task flag pair and deliver
   task text via the agent’s native channel **without** shell-joining —
   still never invent a second approval path.

6. **Storage not in this crate**  
   Assignment/outcome types are in-memory only. When persisting, keep
   governance frozen, keep digests not secrets, and do not invent a parallel
   approval table that shadows `exec_proposals`.

## Non-goals (held)

- No UI, no SQLite, no process spawn, no network, no config I/O  
- No `unsafe`, no library-path `unwrap`  
- Approval authority remains human + host process chain (not provider)

---

**ROLE-DISPATCH-COMPLETE**
