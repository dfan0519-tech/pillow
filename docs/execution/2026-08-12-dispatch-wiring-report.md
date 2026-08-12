# Dispatch wiring into governed process chain — development report

Date: 2026-08-12  
Worker: grok3 (write)  
Controller: Claude  
Task: host-side wiring of role-dispatch → existing process governance  
Status: **DONE**

## Objective

Feed `pillow-role-dispatch` `ProposedCommand` values into the **same**
governed process chain (proposal → one-shot human approval → host execute →
durable receipt). No second approval path, no side executor, no provider
approval authority.

## Write boundary (audit)

| Path | Action |
| --- | --- |
| `crates/runtime-host/src/dispatch.rs` | prepare / provider proposal / ledger / attribution |
| `crates/runtime-host/src/lib.rs` | re-exports |
| `crates/runtime-host/Cargo.toml` | path dep on `pillow-role-dispatch` |
| `crates/runtime-host/tests/dispatch_wiring.rs` | unapproved-no-exec + LocalCli attribution |
| `apps/desktop/src-tauri/src/conversation_service/dispatch_service.rs` | closed DTOs |
| `apps/desktop/src-tauri/src/conversation_service/service.rs` | propose + list methods |
| `apps/desktop/src-tauri/src/conversation_service/mod.rs` | module + re-exports |
| `apps/desktop/src-tauri/src/role_dispatch_commands.rs` | Tauri commands + SlotAuth map |
| `apps/desktop/src-tauri/src/role_dispatch_commands_tests.rs` | handler registration evidence |
| `apps/desktop/src-tauri/src/runtime_slot_commands.rs` | SlotAuth fail-closed for Oauth/future |
| `apps/desktop/src-tauri/src/lib.rs` | `generate_handler!` registration |
| `apps/desktop/src-tauri/Cargo.toml` | path dep on `pillow-role-dispatch` |
| `docs/execution/2026-08-12-dispatch-wiring-report.md` | this report |

**Not touched:** `crates/storage-sqlite/**`, `crates/runtime-slots/**`,
`crates/knowledge-indexer/**`, `apps/desktop/src/**`, root `Cargo.toml`.

## Design

1. **T1 Propose** — desktop resolves slots → maps `SlotAuth` → `SlotAuthShape`
   (unknown/`Oauth` → `role_dispatch_unsupported_auth`) →
   `prepare_dispatch` / `build_assignment` → structured
   `ProviderProcessProposal` → `execute_prepared_dispatch` (=
   `execute_governed_process`) → return live proposal view after reserve.
   `ProposeError` codes surface unchanged via `DispatchError::Propose`.
2. **T2 Attribute** — on terminal receipt,
   `attribute_outcome_for_assignment` with
   `claimed_governed_execution = governance.is_governed()` only. LocalCli
   remains record-only. In-memory ledger keyed by proposal id.
3. **T3 Register** — `propose_role_dispatch` and `list_role_dispatches` in
   `generate_handler!`.

### Authority red lines (held)

- No second approval table; approve/reject is still `decide_exec_proposal`.
- No side spawn of `program`/`argv` outside the process coordinator.
- Approval never delegated to the provider.
- Argv stays a structured array at every handoff (JSON `argv` field).
- Task text never written into `program` or `cwd` on the provider proposal.

---

## 1. Exit codes (real runs)

| Command | Exit | Result |
| --- | --- | --- |
| `cargo test -p keeper-runtime-host --lib dispatch` | **0** | 9 dispatch unit tests |
| `cargo test -p keeper-runtime-host --test dispatch_wiring` | **0** | 3 integration tests |
| `cargo test -p keeper-runtime-host` | **0** | full crate (incl. 37 process-bridge) |
| `node scripts/run-cargo.mjs test -p pillow-desktop --lib` | **0** | 191 passed, 2 ignored |

(`cargo check` was not used as the sole gate — it does not compile `#[cfg(test)]`.)

---

## 2. 「未批准 → 不执行」— critical evidence

Integration: `crates/runtime-host/tests/dispatch_wiring.rs`  
Test: **`unapproved_dispatch_never_starts_a_process`**

What it does:

1. `prepare_dispatch` for a LocalCli target (`node --version` + task argv).
2. Spawns `execute_prepared_dispatch` (same path as agent process.run@1).
3. On `Requested`, registers the dispatch ledger entry.
4. A second thread rejects via `decide_process_proposal(…, Reject)` — never Approve.
5. Asserts:
   - lifecycle `Started` count == **0**
   - receipt `started.is_none()`
   - receipt / durable row state == **Rejected**
   - durable `exit_code` is **None**
6. Attribution still records initiator/target with
   `claimed_governed_execution == false`.

Companion: `prepare_failures_do_not_touch_process_coordinator` — self-dispatch
fails at prepare with **zero** live process proposals.

---

## 3. ProposeError passthrough evidence

Host unit: `dispatch::tests::propose_errors_surface_original_codes`  
(+ `empty_argv_element_surfaces_domain_code`)

| Case | Error code |
| --- | --- |
| Self-dispatch | `role_dispatch_self_dispatch` |
| Target missing | `role_dispatch_target_missing` |
| Role mismatch | `role_dispatch_role_mismatch` |
| Empty task | `role_dispatch_task_empty` |
| Task in cwd | `role_dispatch_task_in_program_or_cwd` |
| Empty LocalCli executable | `role_dispatch_local_cli_executable_empty` |
| Empty argv element | `role_dispatch_empty_argv_element` |
| Shell-join (domain helper) | `role_dispatch_shell_join_forbidden` |

Errors are `DispatchError::Propose(ProposeError)` — `code()` delegates to the
domain code. Desktop `map_dispatch_error` / `ConversationCommandError` use the
same code string (not collapsed to `process_unavailable`).

Unsupported / future auth (incl. storage `SlotAuth::Oauth` until role-dispatch
gains a shape): `role_dispatch_unsupported_auth` (fail-closed `_ =>`).

---

## 4. LocalCli 产出不被标成受治理

| Test | Proof |
| --- | --- |
| `dispatch::tests::local_cli_attribution_never_claims_governed_execution` | outcome.governance=None, claimed=false; manual elevation → `GovernanceElevationForbidden` |
| `dispatch_wiring::approved_dispatch_uses_same_process_chain_and_attributes_local_cli_as_record_only` | real approve+spawn of `node`; still `claimed_governed_execution == false` |
| `unapproved_dispatch_never_starts_a_process` | ledger outcome after reject keeps claimed=false |

`claimed_governed_execution_for(governance)` is exactly `governance.is_governed()`.

---

## 5. Command registration evidence

Source: `apps/desktop/src-tauri/src/lib.rs` `tauri::generate_handler![…]`  
includes:

- `propose_role_dispatch`
- `list_role_dispatches`

Pinned by unit test  
`role_dispatch_commands_tests::desktop_invoke_handler_registers_role_dispatch_commands`
(`include_str!("lib.rs")` substring check — same pattern as exec commands).

---

## 6. Residual risks — renderer wiring hot spots

1. **Closed key allowlist on new DTOs**  
   `RoleDispatchProposalDto` / nested `ExecProposalDto` fields must be admitted
   in any future `tauri*Gateway` validators **in the same change**. A widened
   host DTO without renderer allowlist update makes the feature silently
   disappear (same class as historical `rule_subject` break).

2. **Do not call native spawn from the renderer or a second host path**  
   Renderer must only `propose_role_dispatch` → show approval →
   `decide_exec_proposal`. Invoking a local CLI / shell from UI would bypass
   receipts and the unapproved-no-exec red line.

3. **`governance` / `claimedGovernedExecution` presentation**  
   For LocalCli (`governance: "none"`), UI must never label the row as
   “governed execution” even after a successful process terminal. Prefer the
   ledger fields over inventing a second “governed” flag.

4. **Catalog program `grok` and path-shaped LocalCli**  
   Domain governed shapes emit catalog label `grok`; process catalog today is
   `cargo|rustc|node|python|python3`. LocalCli full filesystem paths also fail
   seal. Propose still builds the assignment; process reserve returns
   `role_dispatch_process_unavailable` until catalog/resolution work lands.
   Tests use catalog-valid `node` LocalCli to pin the approval red line.

5. **OAuth auth (parallel worker already on `SlotAuth`)**  
   Dispatch maps Oauth/unknown to `role_dispatch_unsupported_auth` until
   `SlotAuthShape` gains a governed OAuth shape. Do not invent elevation or
   best-effort argv for Oauth in the renderer.

6. **Task delivery channel**  
   Task text is argv after `--pillow-task`. Real agent CLIs may ignore that
   flag; a later host may strip the pair and deliver via ACP — still without
   shell-joining and without a second approval path.

7. **Detached process waiter**  
   Propose returns after Requested and detaches the waiter thread. Renderer
   must not assume the invoke blocks until terminal; use list/decide/output
   commands and timeline events for lifecycle.

---

## Public surface (host)

| Symbol | Role |
| --- | --- |
| `prepare_dispatch` | assignment + structured provider proposal |
| `provider_proposal_from_command` | ProposedCommand → process.run@1 JSON |
| `execute_prepared_dispatch` | thin handoff to `execute_governed_process` |
| `register_dispatch_on_requested` | ledger bind after durable reserve |
| `attribute_dispatch_receipt` / `complete_dispatch_on_receipt` | T2 |
| `list_dispatch_records` / `get_dispatch_record` | in-memory audit |

## Desktop commands

| Command | Args (conceptual) | Returns |
| --- | --- | --- |
| `propose_role_dispatch` | conversationId, initiatorSlotId, targetSlotId?, targetRole?, taskText | `RoleDispatchProposalDto` |
| `list_role_dispatches` | conversationId | `RoleDispatchRecordDto[]` |

Approval: existing `decide_exec_proposal` / `list_exec_proposals` /
`open_exec_output`.

---

**DISPATCH-WIRING-COMPLETE**
