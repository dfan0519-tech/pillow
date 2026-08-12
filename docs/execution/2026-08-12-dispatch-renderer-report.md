# Dispatch renderer (conversation role-dispatch) — development report

Date: 2026-08-12  
Worker: grok3 (write)  
Controller: Claude  
Task: renderer entry + approval reuse + outcome visibility for role-dispatch  
Status: **DONE**

## Objective

Give the user an in-conversation entry to propose work to another role/slot,
route that proposal through the **existing** process approval bar, surface
exit/output after approval, label ungoverned targets honestly, and map every
`role_dispatch_*` host error to distinct readable copy. No second approval UI,
no optimistic "already dispatched", no renderer network.

## Write boundary (audit)

| Path | Action |
| --- | --- |
| `apps/desktop/src/lib/roleDispatchGateway.ts` | types, public error codes, human copy |
| `apps/desktop/src/lib/tauriRoleDispatchGateway.ts` | closed-key DTO normalize + invoke |
| `apps/desktop/src/features/conversations/RoleDispatchPanel.tsx` | propose entry + outcome list |
| `apps/desktop/src/styles/role-dispatch.css` | panel styles |
| `apps/desktop/src/features/workbench/Workbench.tsx` | mount panel; re-list after propose |
| `apps/desktop/src/App.tsx` | inject gateway + slots |
| `apps/desktop/src/main.tsx` | construct Tauri role-dispatch gateway |
| `apps/desktop/tests/role-dispatch-gateway.test.ts` | whitelist + error copy (accept/reject) |
| `apps/desktop/tests/role-dispatch-panel.test.tsx` | entry, unapproved UX, governance labels |
| `apps/desktop/tests/role-dispatch-workbench.test.tsx` | same CommandApprovalBar path |
| `docs/execution/2026-08-12-dispatch-renderer-report.md` | this report |

**Not touched:** `crates/**`, `apps/desktop/src-tauri/**`, git write ops.

---

## 1. Exit codes (real runs)

| Command | Exit | Result |
| --- | --- | --- |
| `corepack pnpm typecheck` | **0** | desktop `tsc --noEmit` clean |
| `corepack pnpm --filter @pillow/desktop test` | **0** | 51 files, 612 tests (incl. 33 new role-dispatch) |

New suites:

| File | Tests |
| --- | --- |
| `role-dispatch-gateway.test.ts` | 23 |
| `role-dispatch-panel.test.tsx` | 8 |
| `role-dispatch-workbench.test.tsx` | 2 |

---

## 2. T2 evidence — same approval bar (not a second UI)

**Product path**

1. `RoleDispatchPanel` calls only `propose_role_dispatch` (via gateway).
2. On success, Workbench `handleRoleDispatchProposed` → existing `relistApprovals`
   → `execGateway.listExecProposals`.
3. Visible process proposal renders **`CommandApprovalBar`**
   (`data-testid="command-approval-bar"`).
4. Approve/reject still `decide_exec_proposal` only.
5. Panel has no Approve/Reject controls
   (`data-testid="role-dispatch-no-second-approval"`; no `command-approval-*` inside panel).

**Test:** `role-dispatch-workbench.test.tsx`  
`after propose, the process approval bar appears; panel has no approve buttons`

- Before propose: no `command-approval-bar`.
- After propose: bar appears; `command-approval-approve` / `command-approval-reject` exist.
- Inside `role-dispatch-panel`: no 批准/拒绝 buttons.
- Approve click uses existing bar → `decideExecProposal({ decision: "approve_once" })`.

Existing approval-bar suites were **not** weakened (full desktop suite still green).

---

## 3. Unapproved → UI does not claim executed

| Layer | Guard |
| --- | --- |
| Gateway | `normalizeRoleDispatchProposal` **rejects** nested proposal unless `state === "pending_approval"` |
| Panel copy | Success → `ROLE_DISPATCH_PENDING_APPROVAL_NOTICE` only (“已提交为进程提案，等待命令批准。未批准前不会执行。”) |
| Panel assertions | Text must not match `/已派出\|已执行\|执行成功/` |
| Result rows | Unapproved record shows “尚未产生退出码” — no exit code until host has one |

**Tests**

- Gateway: `rejects propose payloads that are not pending_approval`
- Panel: `submits propose and shows pending-approval notice — never 已派出/已执行`
- Workbench: stage text must not match `/已派出|已执行成功|派单已完成/`; result row has `role-dispatch-result-pending`, not exit

---

## 4. Ungoverned target explicitly labeled (accept + reject)

| Case | Evidence |
| --- | --- |
| **Accept (target picker)** | LocalCli `governance: "none"` → badge `目标不受治理` + `ROLE_DISPATCH_UNGOVERNED_HINT` |
| **Accept (outcome row)** | Record `governance: "none"` → same badge + note; exit code shown when present |
| **Accept (governed contrast)** | Librarian `full` → `受 Pillow 治理`; **no** ungoverned hint |
| **Reject (elevation)** | Record with `governance: "none"` + `claimedGovernedExecution: true` **fails normalize** |
| **Helper** | `outcomeClaimsGoverned` returns false when governance is not `full` |

**Tests**

- Panel: `shows ungoverned badge…`, `shows governed badge…`, `lists ungoverned outcome rows…`
- Gateway: `rejects ungoverned record that claims governed execution`
- Workbench: after propose to LocalCli, result governance = `目标不受治理`

---

## 5. Each `role_dispatch_*` error code → distinct copy + tests

Fixed copy in `ROLE_DISPATCH_ERROR_MESSAGES` (host `message` never accepted):

| Code | UI fragment (zh) |
| --- | --- |
| `role_dispatch_self_dispatch` | 不能把任务派给总控自己… |
| `role_dispatch_target_missing` | 找不到目标槽位… |
| `role_dispatch_role_mismatch` | 角色与请求的角色不一致… |
| `role_dispatch_shell_join_forbidden` | 不能使用拼接后的 shell 命令字符串… |
| `role_dispatch_task_empty` | 任务描述不能为空… |
| `role_dispatch_task_in_program_or_cwd` | 不能写入程序名或工作目录… |
| `role_dispatch_program_empty` | 没有可执行的程序… |
| `role_dispatch_local_cli_executable_empty` | 可执行文件路径为空… |
| `role_dispatch_empty_argv_element` | 启动参数含有空项… |
| `role_dispatch_unsupported_auth` | 认证方式尚不支持派单… |
| `role_dispatch_process_unavailable` | 无法进入进程治理链… |
| `role_dispatch_ledger_unavailable` | 派单台账暂时不可用… |
| `role_dispatch_unavailable` | 派单服务暂时不可用… (fallback) |

**Tests**

- Gateway: every public code has unique non-empty message; none equals `"派单失败"`; host secret text stripped.
- Panel: loops the six brief-required codes and asserts full fixed message on `role-dispatch-error`.
- Workbench: self-dispatch error shown; **no** approval bar painted.

Controller excluded from target `<select>` so self-dispatch is hard to reach from UI; host refusal still surfaces if it happens.

---

## 6. Closed-key allowlist widened same change (accept + reject)

| DTO | Keys admitted (camelCase) |
| --- | --- |
| Propose | `assignmentId`, `initiatorSlotId`, `targetSlotId`, `targetRole`, `targetRoleCustomName`, `governance`, `taskText`, `program`, `argv`, `portableCwd`, `callId`, `runId`, `proposal` |
| Record | `assignmentId`, `initiatorSlotId`, `targetSlotId`, `governance`, `proposalId`, `callId`, `claimedGovernedExecution`, `exitCode`, `durationMs` |
| Nested `proposal` | Reuses `normalizeExecProposal` (existing process allowlist) |

**Accept:** exact host propose + list fixtures parse; nested proposal keeps `decisionAvailable` / `pending_approval`.  
**Reject:** unknown keys (`approvalGrant`, `nativePath`, `secretToken`), bad governance enum, missing fields, non-pending nested state, elevation on ungoverned records.

Pinned in `role-dispatch-gateway.test.ts` (accept + reject directions; not reject-only).

---

## 7. Residual risks

1. **Process catalog / path seal** — LocalCli or non-catalog programs may still return
   `role_dispatch_process_unavailable` from host; UI shows that distinct copy, but
   user may need catalog work on host side (out of this renderer ticket).
2. **OAuth / unsupported auth** — Mapped to `role_dispatch_unsupported_auth`; UI
   does not invent a launch path.
3. **Output deep-link** — Result list shows exit code + governance; full stdout still
   opened via existing process review / timeline “完整输出” after approve (same
   ExecGateway). No parallel terminal path added.
4. **In-memory dispatch ledger** — `list_role_dispatches` is host-instance memory;
   restart clears rows (host limitation; renderer only lists what host returns).
5. **Initiator = controller slot** — Panel always uses the project controller as
   `initiatorSlotId`. Multi-controller is host-forbidden already.

---

## Design held

- **One approval authority:** `CommandApprovalBar` + `decide_exec_proposal` only.
- **No optimistic success:** propose success copy is pending-only.
- **No renderer network:** Tauri invoke only.
- **No secrets in DOM:** gateway rejects smuggle keys; error paths drop host messages.

---

**DISPATCH-RENDERER-COMPLETE**
