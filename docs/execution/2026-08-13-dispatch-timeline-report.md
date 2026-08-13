# 派单时间线可见 — 执行报告

日期: 2026-08-13 · Worker: grok2 · 总控: Claude

完成信号: `DISPATCH-TIMELINE-COMPLETE`

## 范围

只改时间线渲染、投影、会话 UI 类型、样式与测试。不改宿主、Tauri、
派单提交、审批链。`roleDispatchGateway.ts` 未改（现有 DTO 已够投影
用；缺的字段不能在渲染层补）。

依据 `docs/decisions/2026-08-13-concurrency-semantics.md`:
槽位可配置多个、长期保留；同一时刻一个在对话；其余靠派单。
文案未使用「同时工作」「并行运行」。

## 退出码表

| 命令 | 日志 | EXIT |
| --- | --- | --- |
| `corepack pnpm --filter @pillow/desktop exec vitest run tests/dispatch-timeline.test.tsx`（旧时间线，先红） | `.superpowers/round48/dt-red.txt` | **1** |
| `corepack pnpm --filter @pillow/desktop exec vitest run tests/dispatch-timeline.test.tsx`（实现后） | `.superpowers/round48/dt-green.txt` | **0** |
| `corepack pnpm --filter @pillow/desktop typecheck` | `.superpowers/round48/dt-log1.txt` | **0** |
| `corepack pnpm --filter @pillow/desktop test` | `.superpowers/round48/dt-log2.txt` | **0**（56 files / 733 tests） |

未跑 `scripts/run-worker-gate.mjs`，未跑 E2E。

## 先红后绿

文件: `apps/desktop/tests/dispatch-timeline.test.tsx`

主用例名: `renders a visible dispatch entry with target role and task summary after propose`

旧代码失败信息（时间线只画出用户消息，没有任何 dispatch / assignment 渲染）:

```
Unable to find an element by: [data-testid="dispatch-timeline-row"]
```

当时 DOM 只有:

```
你
请把索引整理任务派给信息管理员
```

先红时该文件 10 个用例: **9 failed / 1 passed**。
唯一通过的是「不绘制内部 id」——旧时间线根本没有派单行，断言空过。
实现后该断言仍成立，且会检查真实派单行。

实现后: **12 passed**（补了 propose DTO 提升与 list-record 不编造两项）。
没有放宽任何既有测试。

## 时间线派单条目实际外观

propose 形状（有 `targetRole` + `taskText` + `proposalState` + 父层给的
`initiatorLabel`）渲染出的可见文本:

```
派单
从 总控
派给 信息管理员
待批准
受 Pillow 治理
任务 整理知识库索引并标出过期条目
```

任务正文在 `<details>` 里，默认折叠；摘要行带任务预览，展开后仍是同一段
`整理知识库索引并标出过期条目`。没有批准 / 拒绝按钮。

只拿 `list_role_dispatches` 记录、没有 propose 叠加时（诚实的稀疏行）:

```
派单
受 Pillow 治理
发起方显示名不可用（列表记录只有内部槽位标识）
目标角色名不可用（列表记录未带角色名）
任务描述不可用（列表记录未带任务文本）
当前步骤不可用（没有对应的进程事件，列表记录也没有退出码）
```

状态格只在有真实依据时出现:

| 依据 | 画出的状态 |
| --- | --- |
| 进程事件 `running` | 执行中 |
| 进程事件 `succeeded` | 完成 |
| 进程事件 `rejected` | 已拒绝 |
| propose 的 `proposal.state = pending_approval` | 待批准 |
| 列表记录 `exitCode === 0`（无进程事件） | 完成 |
| 列表记录非 0 `exitCode`（无进程事件） | 失败 |
| 以上都没有 | **不画状态格**，写「当前步骤不可用…」 |

「已提出」只有父层或宿主明确给出 `proposed` 时才画。当前 propose 返回的是
`pending_approval`，所以刚提交显示「待批准」，不把「已提出」当成装饰。

「已批准」同样只在进程决策事件是 `approved` 时出现。不会把已批准画成执行中。

## 需要总控接线

### 1. 工作台还没把数据传进时间线

本单不能改 `Workbench.tsx` / reducer / App。
`ConversationTimeline` 现在吃两路数据:

- 可选 prop `dispatches`
- 或 `timeline.dispatches`

工作台现在只传 `timeline` + `run`，两条都是空的。
**实现了渲染，活会话里用户仍然看不见派单**，除非总控把数据接进去。

建议接线（不改宿主）:

1. `RoleDispatchPanel.onProposed` 已经把完整 `RoleDispatchProposalView` 交给父层。
2. 父层用 `dispatchViewFromProposal(proposal, { initiatorLabel, targetLabel })`
   叠到当前对话的派单列表。
3. `listRoleDispatches` 刷新时用 `dispatchViewFromRecord`，按 `assignmentId`
   **保留**已缓存的 `targetRole` / `taskText` / `proposalState` / 槽位显示名。
   列表 DTO 补不上这三项。
4. 把合并结果传给 `<ConversationTimeline dispatches={...} />`。
5. 槽位显示名从 `runtimeSlotsGateway.listRuntimeSlots` 解析:
   总控槽 → `initiatorLabel`（角色名「总控」或 `displayName`），
   目标槽 → `targetLabel`（`displayName` 或角色名）。

### 2. 现有宿主 DTO 缺什么（本单未加字段）

`RoleDispatchRecordView`（`list_role_dispatches`）只有:

`assignmentId`, `initiatorSlotId`, `targetSlotId`, `governance`,
`proposalId`, `callId`, `claimedGovernedExecution`, `exitCode`, `durationMs`

缺、且渲染层禁止编造:

| 用户要看见的 | 列表 DTO | propose DTO |
| --- | --- | --- |
| 谁派的（显示名） | 只有内部 `initiatorSlotId` | 同左 |
| 派给谁（角色 / 槽位名） | 无 `targetRole` / `targetRoleCustomName` | 有 |
| 派了什么（任务） | 无 `taskText` | 有 |
| 现在到哪一步 | 无生命周期；`exitCode` 只覆盖终态 | 有 `proposal.state` |
| 发生时间 | 无 | 无 |

内部 id 按既有约定不绘制。没有显示名就不画「谁 / 给谁」。

若总控希望刷新后仍完整，需要二选一:

- 父层缓存 propose 叠加（上面的接线，不改宿主），或
- 扩 `list_role_dispatches` 的宿主 DTO（`targetRole`、`taskText`、时间戳、
  生命周期）。扩 DTO 必须同时改渲染 allowlist，且是破坏性变更。

### 3. 本单没做成的

- 活会话自动出现派单行（差工作台接线）
- 按真实发生时间插入对话流（DTO 没有时间戳；无时间戳的行排在有
  `createdAt` 的消息 / 活动之后）
- 第二个审批条（有意不做；决策仍只在 `CommandApprovalBar`）

## 残留风险

- **能力仍可能等于没有**: 未接线前，用户在对话流里还是看不见派单。
- 只靠 list 刷新、丢掉 propose 缓存时，时间线只剩「不可用」说明，看起来像坏了。
  这是诚实，不是动画。
- 进程事件若已被时间线裁掉，而列表又没有 `exitCode`，状态格会消失。
- `RoleDispatchPanel` 自己的「派单结果」还在。时间线是叙事，不是替代。
  两处文案不一致时，以审批条 + 进程事件为准。
- 派单行没有时间戳，长对话分页时它们会落在最新页末尾，不一定挨着发出派单的那句用户话。
- 本单测试覆盖时间线组件，不覆盖工作台装配。接线后需要总控补一条工作台测试:
  提交派单后，时间线上出现目标角色名和任务摘要。

## 改动文件

| 路径 | 作用 |
| --- | --- |
| `apps/desktop/src/features/conversations/conversationTypes.ts` | `ConversationDispatchView` + 可选 `timeline.dispatches` |
| `apps/desktop/src/features/conversations/agentActivityProjection.ts` | `dispatchViewFromProposal` / `FromRecord` / `projectDispatchTimeline` |
| `apps/desktop/src/features/conversations/ConversationTimeline.tsx` | 把派单画成一等时间线事件 |
| `apps/desktop/src/styles/dispatch-timeline.css` | 叙事行样式 |
| `apps/desktop/tests/dispatch-timeline.test.tsx` | 先红后绿，断言用户看得见的字 |
| `docs/execution/2026-08-13-dispatch-timeline-report.md` | 本报告 |

未改: `crates/**`、`apps/desktop/src-tauri/**`、`RoleDispatchPanel` 提交逻辑、
审批链、`roleDispatchGateway.ts`。
