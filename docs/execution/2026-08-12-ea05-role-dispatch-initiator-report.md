# EA-05 — role-dispatch 发起者绑定当前项目 Controller

状态：实现与本单 focused 验证完成；桌面 lib 全量门已启动，但被在途的
EA-03 测试失败阻断，详见退出码表。

## 修复

`propose_role_dispatch` 仍保留兼容性所需的 `initiator_slot_id` 参数，但该值
现在只是声明值。原生命令在已经确认 conversation、window 与 active project
scope 后，从该项目的 `RuntimeSlotService::list(project_id)` 结果中解析它：

- UUID 格式无效：`role_dispatch_initiator_id_invalid`
- 不在当前项目：`role_dispatch_initiator_not_in_project`
- 在当前项目但不是 Controller：`role_dispatch_initiator_not_controller`

只有命中当前项目的 Controller 后，才把 host-loaded slot metadata 中的 id
写入 `BuildAssignmentInput`。因此 domain assignment、proposal DTO 和后续账本
均使用宿主派生值，而非原始 renderer 字符串。

`crates/runtime-host/src/dispatch.rs` 的总控接线也已收紧：

1. `PreparedDispatch` 私有冻结 `host_derived_initiator_slot_id`。
2. `register_dispatch_on_requested` 以该冻结值写入 `DispatchRecord`。
3. 若公开的 assignment projection 在 register 前被改写为其他 initiator，
   返回 `role_dispatch_initiator_attribution_mismatch`，且不写入账本。

现有 conversation service 无须改动：它已将同一个 `PreparedDispatch` 传给
`register_dispatch_on_requested`。这保持派单为普通 `pending_approval` process
proposal，未新增任何审批旁路或 side executor。

## 先红后绿的复现

先新增 native-boundary 测试，再在旧实现上执行定向桌面测试。旧实现只解析
initiator UUID，因而随机 UUID、其他项目 Controller 与当前项目 Librarian
均被错误接受：7 passed、3 failed，`EXIT=101`。

应用修复后，对同一测试选择器重跑：10 passed、0 failed，`EXIT=0`。

## T3 证据

| 情形 | native-boundary 测试 | 结果 |
| --- | --- | --- |
| 合法当前项目 Controller | `project_controller_is_accepted_as_dispatch_initiator` | 成功，input 使用 host-loaded id |
| 随机 UUID | `random_initiator_id_is_rejected_at_native_dispatch_boundary` | 拒绝：`role_dispatch_initiator_not_in_project` |
| 其他项目 Controller | `other_project_initiator_is_rejected_at_native_dispatch_boundary` | 拒绝：`role_dispatch_initiator_not_in_project` |
| 当前项目 Librarian | `non_controller_initiator_is_rejected_at_native_dispatch_boundary` | 拒绝：`role_dispatch_initiator_not_controller` |
| 自派 | `controller_self_dispatch_remains_rejected_after_initiator_resolution` | 保持拒绝：`role_dispatch_self_dispatch` |

账本还增加两条 host unit test：`ledger_records_the_host_derived_initiator` 与
`ledger_refuses_reattribution_after_preparation`。后者确认篡改 assignment
projection 后不会生成 ledger record。

## 实际退出码

| 命令 | 退出码 | 结果 |
| --- | ---: | --- |
| `node scripts/run-cargo.mjs test -p pillow-desktop --lib role_dispatch_commands::tests`（修复前） | 101 | 7 passed、3 failed；预期红测 |
| `node scripts/run-cargo.mjs test -p pillow-desktop --lib role_dispatch_commands::tests`（修复后） | 0 | 10 passed |
| `node scripts/run-cargo.mjs test -p keeper-runtime-host --lib dispatch::tests::ledger_` | 0 | 2 passed |
| `node scripts/run-cargo.mjs test -p keeper-runtime-host --lib` | 0 | 37 passed |
| `cargo test -p pillow-role-dispatch` | 0 | 35 passed；doctest 0 |
| `node scripts/run-cargo.mjs test -p pillow-desktop --lib` | 101 | 210 passed、1 failed、2 ignored；失败的是在途 EA-03 的 `conversation_service::write_service::ea03_write_grant_boundary_tests::decide_write_proposal_service_source_does_not_accept_renderer_grant` |
| `node scripts/run-cargo.mjs test -p pillow-desktop --lib`（复跑） | 124 | 工具在 120 秒后超时；超时前已再次观察到同一 EA-03 测试失败，未把它当作通过 |
| `rustfmt --check --edition 2024`（本单两处 Rust 文件） | 0 | 格式通过 |
| `git diff --check -- crates/role-dispatch apps/desktop/src-tauri/src/role_dispatch_commands.rs apps/desktop/src-tauri/src/role_dispatch_commands_tests.rs crates/runtime-host/src/dispatch.rs` | 0 | 本单源码范围无空白错误 |

最后一项桌面全量门不是 ENV-BLOCKED：它正常启动并执行到测试完成，只是被本单
禁止修改的、并发中的 EA-03 source test 失败阻断。待该 worker 收口后应重跑
该命令；本单的 10 条派单测试均已通过。

## 写入边界审计与残留风险

- 本单没有新增 SQLite schema、repository 或 renderer 写能力；也没有修改
  approval/decision 命令。
- 派单仍经既有 process reserve → `pending_approval` → 人工决定 → host
  execute → receipt 链路。账本仅在既有 `Requested` 生命周期点登记。
- 新增的账本字段只冻结内存中的 host-derived attribution；不引入 secret、
  native path 或 renderer authority 到 DTO。
- 兼容性 API 仍接收 renderer 的 initiator 声明。因此任何未来新增的 native
  派单入口都必须复用当前项目 Controller 解析，不能直接构造未验证的
  `BuildAssignmentInput`。
- dispatch ledger 仍是既有的进程内账本；若未来接入 durable receipt/storage，
  该持久化路径也必须只采用 `host_derived_initiator_slot_id`。

EA-05-COMPLETE（本单实现与 owned tests 已完成；桌面全量门的 EA-03 并发失败
仍待其 owner 收口后复跑。）
