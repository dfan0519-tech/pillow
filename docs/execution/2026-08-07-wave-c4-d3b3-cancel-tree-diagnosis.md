# C4 D3B3 — cancel-tree `composer-cancel` 超时：只读根因诊断

**Worker:** grok1（只读诊断；未改产品源码/测试/脚本）  
**Controller session:** `<session-id>`  
**Date:** 2026-08-07  
**Worktree:** `<repo>` @ dirty tree after HEAD `62149be`  
**Scope:** 解释 RED `cancel-tree: getByTestId('composer-cancel') waitFor visible 60s timeout`  
**Hard limits observed:** 未跑桌面 E2E；未改 `App.tsx` / `Workbench.tsx` / `ResourcePane.tsx`；未动 git 工作树状态。

---

## 0. RED 复述

```
KEEPER_WAVE_C4_GOVERNED_PROCESS_ACCEPTANCE
{"status":"failed",
 "error":"cancel-tree: locator.waitFor: Timeout 60000ms exceeded.
  Call log:
  - waiting for getByTestId('composer-cancel') to be visible"}
```

同次 Round 4 串行桌面 E2E：10 项 7 过 3 挂；C2-E / C3-F 已另有 Worker 定位为变更面板挤掉资源浏览器；C4 Git spec 12/12 通过。  
本失败来自 process 套件 `batch-core` 内第 7 场景 `cancel-tree`（见 `scripts/wave-c4-governed-process-acceptance.mjs` 顺序）。

Playwright 语义要点：`locator.waitFor({ state: "visible" })` 在 **任一时刻** 变为可见即成功。因此在 `cancelOwningRun` 的整段 60s 内，`composer-cancel` **从未**进入可见 DOM。

---

## 1. `composer-cancel` 出现的确切条件

### 1.1 渲染闸门（唯一 DOM 条件）

`apps/desktop/src/features/workbench/Composer.tsx`：

| 行 | 条件 |
| --- | --- |
| 161–172 | **仅当** `canCancel === true` 时挂载 `<button data-testid="composer-cancel">` |
| 38 | prop 默认 `canCancel = false` |

没有 “disabled 但仍存在” 的取消钮：`canCancel` 为 false 时 **节点不在 DOM**。  
按钮本身不读 `busy` / `sending`；这两项只影响发送与输入禁用（48、90）。

### 1.2 Workbench → Composer

`apps/desktop/src/features/workbench/Workbench.tsx`：

| 行 | 逻辑 |
| --- | --- |
| 227 | `const canCancel = isRunActive(run);` |
| 787 | `<Composer canCancel={canCancel} onCancelRun={onCancelRun} … />` |

`run` 是 **props**，Workbench 不自建 run 状态。

### 1.3 `isRunActive` / `selectActiveRun`

`apps/desktop/src/features/conversations/conversationReducer.ts`：

```ts
// 140–153
export function selectActiveRun(state): RunView | null {
  const timeline = selectActiveTimeline(state);
  if (!timeline?.activeRunId) return null;
  return state.runs[timeline.activeRunId] ?? null;
}

export function isRunActive(run): boolean {
  if (!run) return false;
  return (
    run.status === "pending"
    || run.status === "running"
    || run.status === "cancelling"
  );
}
```

**不在** active 集合：`completed` | `failed` | `cancelled` | `interrupted`。

### 1.4 App 所有权闸门（可把 run 置为 null）

`apps/desktop/src/App.tsx`：

| 行 | 逻辑 |
| --- | --- |
| 886–903 | `selectionOwnedByActiveProject = conversationBelongsToProject(state, selectedConversationId, activeProject.id)` |
| 901–903 | `activeRun = selectionOwnedByActiveProject ? selectActiveRun(state) : null` |
| 933 | `run={activeRun}` 传入 Workbench |

`conversationBelongsToProject`（reducer 156–164）：`selectedConversationId` 必须出现在 `conversationsByProject[projectId]` **列表**中。  
列表来自 `conversations_loaded`（reducer 48–55）——**整表替换**，不是 merge。

### 1.5 小结：`composer-cancel` 可见 ⟺

1. 选中对话通过项目所有权证明；且  
2. `timelines[selected].activeRunId` 非空；且  
3. `runs[activeRunId].status ∈ { pending, running, cancelling }`。

任一失败 → 无 `composer-cancel`。

---

## 2. cancel-tree 走到哪一步开始等它？应用应处什么状态？

### 2.1 编排位置

`scripts/wave-c4-governed-process-acceptance.mjs` `scenarioCancelTree`（约 1143–1193）：

```text
selectProject(A)
  → collapseResourcePaneIfOpen
  → sendProcessPrompt("[c4p:long-tree] …")   // 新建对话 + 发送
  → waitForCommandApproval(node / c4p-child)
  → approveCommandOnce({ waitForClear: false })  // 长跑：不要求审批条消失
  → waitForCommandActivity(运行中, 20s).catch(ignore)  // 见 §3.2
  → findMarkedNodeProcesses(["c4p-child"])
  → cancelOwningRun(page)   ← 在此 60s 等 composer-cancel 可见  ← RED
  → … cancelled activity / child cleanup / idle
```

`scripts/lib/c4-governed-process-acceptance.mjs` `cancelOwningRun`（714–719）：

```js
const cancel = page.getByTestId("composer-cancel");
await cancel.waitFor({ state: "visible", timeout: 60_000 });
```

`withStep("cancel-tree", …)` 把错误前缀成 `cancel-tree: …`，与 RED 一致。

### 2.2 此时应用**应有**状态（产品契约）

| 层 | 期望 |
| --- | --- |
| ACP / host | 本轮 run 仍 mid-turn；`pillow/process_tool` 阻塞在 `execute_governed_process` → `wait_for_resolution` |
| Process | 用户已 Approve；`decide_process_proposal` **同步阻塞到 execute 结束**（`crates/runtime-host/src/host/process.rs` ~1376–1446，`execute_with_observer`） |
| Fake ACP | `run_governed_process_long_tree`（`scripts.rs` ~899–989）已发出 process request 113，并 `wait_for_response`；子进程带 `/*c4p-child*/` |
| UI run | `isRunActive(run) === true` → **`composer-cancel` 已可见**（自 `sendTurn` 被接受起即可，不必等 process “运行中”活动行） |
| UI 审批条 | 可因 decide 阻塞仍保持 submitting（`waitForClear: false` 正是为此）；**不**应挡住 cancel 渲染 |

D3B3 原报告已记录该同步 decide 设计与 harness 对策（`docs/execution/2026-08-05-wave-c4-d3b3-grok4-real-desktop-process-e2e-report.md` Residual risks #1）。

### 2.3 场景顺序上下文

同 session `batch-core` 中 cancel-tree 前已串行：  
`reject → approve-success → terminal-output → replay-conflict → nonzero-exit → output-bounds`  
再进入 cancel-tree。失败时总墙钟 ~293s，与 “前 6 场景通过 + cancel-tree 内 60s 等待” 量级一致。

---

## 3. 为什么没出现？具体代码路径

### 3.1 直接机制（高置信）

`cancelOwningRun` 等待的是 **DOM 条件**，不是 process 状态。  
60s 内始终不可见 ⟹ 整段等待期间 `canCancel` 一直为 false ⟹  
`isRunActive(activeRun)` 为 false，即：

- **A.** `activeRun === null`（所有权失败 **或** `activeRunId`/runs 缺失），或  
- **B.** `activeRun.status` 已是终态（非 pending/running/cancelling）。

以下是可落到源码的两条主路径（无需 E2E 即可从代码证明“能发生”；**哪一条在 RED 现场命中需总控复跑确认**）。

### 3.2 路径 A — 所有权闸门 + `conversations_loaded` 整表替换竞态（中高置信）

**代码链：**

1. 每个场景开头 `selectProject` → App `selectProject` **总是** `clearConversationScope()` 并 `setActiveProject`（`App.tsx` ~636–645）。  
2. `useEffect([activeProject, conversationGateway])`（257–278）**无 generation 丢弃**：异步 `listConversations` 完成后无条件 `conversations_loaded`。  
3. reducer `conversations_loaded`（48–55）**整表替换**该项目对话列表。  
4. `sendProcessPrompt` → 新建对话 + `sendTurn`；`sendTurn` 内另一次 `listConversations`（`App.tsx` ~822–838）。  
5. 若 **较早发起** 的 list 在 **较晚的 create/send list 之后** 返回旧快照：  
   - 列表 **丢失** 当前 `selectedConversationId`  
   - `conversationBelongsToProject` → false  
   - `ownedConversationId` / `activeTimeline` / **`activeRun` 全 null**  
   - `composer-cancel` 消失  
6. Workbench 的 `conversationId` 变 null → 审批 scope `clearApprovalScope`；但 cancel-tree 已过 `waitForCommandApproval` / `approve`，且  
   `waitForCommandActivity(...).catch(() => undefined)` **吞掉** “活动行/时间线消失” 的失败，继续进入 `cancelOwningRun`。

**为何更像长串行放大：**

- 每场景 `selectProject` + 每场景 `新建对话` → list 请求叠压；对话条数随场景增加 → list 更慢 → 竞态窗口变大。  
- cancel-tree 在 approve 后还有最多 **20s** 空等 “运行中”（见 3.3），把 “approve 后到点 cancel” 的窗口拉长，给迟到 list 更多时间。  
- 前序场景若未读 `composer-cancel`，**不会**用同一断言暴露该竞态。

**与 D3B3 曾 GREEN 的关系：** 竞态是既有结构（ownership + replace），D3B3 独立 process 套件 ~453s 曾 11/11；不排除 flaky，或当时 list 更快/对话更少而未触发。

### 3.3 路径 B — approve 后 run 已终态，cancel 合法地不应出现（中置信）

**代码链：**

1. Approve → `Workbench.handleProcessDecide`（~536–565）→ `execGateway.decideExecProposal` → host `decide_process_proposal` **阻塞到** `execute_with_observer` 返回。  
2. 若进程 **很快** 以 Failed/Succeeded/Cancelled 结束（spawn 失败、策略拒绝、快速退出、或其它 bridge error），`handle_process_tool`（`prompt.rs` ~659–751）返回 receipt/error，fake long-tree `wait_for_response` 得到 error 后走 `stopReason: cancelled` 或 finish 路径（`scripts.rs` ~937–989）。  
3. UI 收到 `turn_completed` / `turn_failed` / `run_cancelled` → `statusFromEvent` 终态（`applyRuntimeEvent.ts` 324–347）→ `isRunActive` false → **cancel 按钮拆除**。  
4. 此后 `cancelOwningRun` 空等 60s → 与 RED 一致。

固定产品超时 `PROCESS_TIMEOUT = 60_000ms`（`crates/tool-executor/src/process.rs:71`）。  
若 cancel 在 approve 后 **立即** 开始等待，正常 long-tree 在超时前应仍有 active run；但 cancel-tree 先 **空等最多 20s** 找 “运行中” 活动行，再 `Get-CimInstance Win32_Process` 扫标记进程，再进入 60s cancel 等待——若中间 run 已终态，仍会卡满 60s。

### 3.4 路径 C — “运行中”活动行永远等不到（高置信为旁路事实，**不是** cancel 缺失的根因）

`crates/adapter-grok-acp/src/prompt.rs` 713–742：  
`process_proposal_decided` / `process_started` / `process_terminal_outcome` 仅在 **`execute_governed_process` 返回之后** 才 emit。  

长跑 approve 期间 decide 阻塞在 execute 内 → UI 时间线活动行长期停在 **待批准**（`process_proposal_requested`），**不会**出现 “运行中”。  

因此：

```js
waitForCommandActivity({ status: PROCESS_STATUS.running, timeoutMs: 20_000 }).catch(() => undefined)
```

在 long-tree 上 **几乎必然** 20s 后被吞掉——这是 D3B3 已知同步 decide 的 UI 投影延迟，**不能**解释 cancel 按钮本身缺失（cancel 只依赖 run status，不依赖 process 活动行）。

### 3.5 已排除 / 低置信

| 假设 | 理由 |
| --- | --- |
| CSS 盖住但仍 “visible” | Playwright `visible` 不要求未被遮挡；且 `canCancel` false 时节点不存在 |
| Composer 未挂载 | Workbench 在有 project 时始终挂 Composer（~785） |
| 与 mock unit 回归 | `conversation-gateway-stream` cancel 用例本诊断已跑通（见 §7） |
| process 事件把 run 标终态 | `statusFromEvent` 对 process_* 走 default：`previous ?? "pending"`（`applyRuntimeEvent.ts` 324–347）；host `status_for_kind` 对 process 返回 `None`（`conversation_events.rs` 149–157） |

---

## 4. D3B4 回归 vs 长串行放大？（git 证据）

### 4.1 Git 现状（只读）

- HEAD：`62149be feat(stage1): checkpoint governed IDE through C4 D3AR2`  
- 工作树：大量 **已修改 + 未跟踪** D3B1–D3B4 累积（process + git + harness）；**无** 指向 D3B3 process E2E 的独立 commit（仍 untracked / dirty）。  
- `Composer.tsx` / `isRunActive`：**未**出现在当前 dirty diff 中（取消钮定义与 active 判定相对稳定）。  
- 与 cancel 相关的 **脏改动** 主要在：

| 区域 | 与 cancel 关系 |
| --- | --- |
| `App.tsx`（+git / +exec token） | 改变 re-render、项目切换副作用密度；**ownership + listConversations 逻辑仍在** |
| `Workbench.tsx` / `ResourcePane.tsx` | process 审批 + git bodyMode；**canCancel 仍 = isRunActive(run)** |
| `tauriConversationGateway.ts` + process event kinds | 解析 process 事件；失败则 drop 事件，**不**应单独拆掉 cancel |
| `ConversationTimeline.tsx` + `processActivityModel` | 活动行投影；不控制 Composer |
| `fake_acp_agent/scripts.rs` long-tree | cancel-tree 脚本；D3B3 已 GREEN 过同一路径 |
| `service.rs` `register_governed_process_run` | 发送路径；失败会 send 失败，与 “用户消息+审批条已出现” 不兼容 |

### 4.2 判定

| 判定 | 结论 |
| --- | --- |
| **D3B4 直接改坏 `composer-cancel` 渲染条件？** | **否**（无 diff 触及 Composer / `isRunActive` 定义） |
| **D3B4 后首次长串行放大既有脆弱点？** | **是，偏高可能**（ownership list 竞态 + 同步 decide + cancel-tree 空等 20s + 多对话 list 变慢） |
| **纯 D3B4 功能逻辑（Git pane）导致 cancel 消失？** | **低**（`gitChangesActive` 默认 false；Changes 发现不在默认路径跑） |
| **需总控复跑确认** | 单场景 `cancel-tree` vs 完整 process 串行；失败瞬间 dump：是否有 `composer-cancel`、`active-project`、对话行是否含当前选中、body 是否已终态中文标签 |

### 4.3 旁证

- `.superpowers/c4-process-acceptance/c4-process-acceptance-record.json` 仍为 **2026-08-05 passed**（含 cancel-tree）；**不是**本次 RED 的失败记录。  
- 同目录下 `reject`/`approve`/`terminal` 截图时间戳有 2026-08-07，与 “batch-core 前半通过、中后失败” 一致。  
- D3B3 报告：`cancel-tree | passed | markersBefore=2`（独立 process 套件 GREEN）。

---

## 5. 与「变更面板挤掉资源浏览器」是否同源？

**否（不同源）。**

| | C2-E / C3-F（另一 Worker） | 本 cancel-tree |
| --- | --- | --- |
| 表面 | ResourcePane body 模式：`resources` vs `changes`；Terminal/Diff 优先级 | Composer `canCancel` / run 活跃态 |
| 主文件簇 | `ResourcePane.tsx`、Workbench bodyMode、App `gitChangesActive` | `Composer.tsx`、`isRunActive`、`App` ownership、`conversations_loaded`、process decide 时序 |
| cancel-tree harness | 开场 `collapseResourcePaneIfOpen` | 失败点在 cancel 钮，非资源树断言 |

Terminal 目标可优先于 Changes（设计如此）——**仍不驱动** `composer-cancel` 挂载条件。  
两处可同批暴露于 Round 4 串行 E2E，但是 **独立缺陷**。

---

## 6. 最小修复建议（草案；本 Worker 未应用）

### 6.1 推荐主修：防止 `conversations_loaded` 冲掉已选对话（authority 不削弱）

**文件：** `apps/desktop/src/features/conversations/conversationReducer.ts`  
**为何最小：** 只修列表替换的 fail-open 竞态；不放宽取消权限、不绕过 project 边界——**仍要求 selected id 来自本项目已知对话**（若迟到 list 丢条目，保留本地已 create 的条目直到权威 list 再次包含或用户显式离开）。

**草案 diff：**

```diff
--- a/apps/desktop/src/features/conversations/conversationReducer.ts
+++ b/apps/desktop/src/features/conversations/conversationReducer.ts
@@ case "conversations_loaded":
-    case "conversations_loaded":
-      return {
-        ...state,
-        conversationsByProject: {
-          ...state.conversationsByProject,
-          [action.projectId]: action.conversations,
-        },
-      };
+    case "conversations_loaded": {
+      const previous = state.conversationsByProject[action.projectId] ?? [];
+      let next = action.conversations;
+      const selected = state.selectedConversationId;
+      // Stale list replies must not orphan the selected conversation: ownership
+      // proof would null activeRun and hide composer-cancel mid-turn.
+      if (
+        selected
+        && !next.some((item) => item.id === selected)
+      ) {
+        const retained = previous.find((item) => item.id === selected);
+        if (retained) {
+          next = [...next, retained];
+        }
+      }
+      return {
+        ...state,
+        conversationsByProject: {
+          ...state.conversationsByProject,
+          [action.projectId]: next,
+        },
+      };
+    }
```

**配套（推荐，仍小）：** `App.tsx` listConversations effect 加 **scope generation**（与 gitScope / projectScope 同型），过期回复不 dispatch——避免依赖 “保留 orphan” 作为唯一防线。  
（注意：`App.tsx` 与另一 Worker 文件簇相交时，由总控串行合入。）

### 6.2 次修：harness 可观测性 + 少浪费 20s（不削弱 authority）

**文件：** `scripts/lib/c4-governed-process-acceptance.mjs` / `scripts/wave-c4-governed-process-acceptance.mjs`  
（总控独占 E2E 时再改。）

1. 在 `sendProcessPrompt` 成功后 **立即** `expect composer-cancel visible`（失败则 dump body + 是否有 conversation-row）。  
2. cancel-tree：**不要**为空的 “运行中” 活动行烧掉 20s；改为等 `composer-cancel` 或 host 侧 `c4p-child` 标记。  
3. `cancelOwningRun` 超时错误附带：cancel count、approval count、body 片段（是否已 “已取消/已失败/已成功”）。

### 6.3 可选产品 UX（非最小、不解决 cancel 缺失本身）

在 **claim execute 之前或开始后** 发布 `process_started`（今日要等 execute 整段结束才 emit，见 `prompt.rs` 713+），使活动行能显示 “运行中”。  
这改善可观测性，**不**替代 6.1。

### 6.4 明确不要做的

- 不要为了 GREEN 去掉 `isRunActive` 或让无 run 时也显示 cancel（削弱 authority / 误导用户）。  
- 不要在 harness 里 `force` 点不存在的取消或注入 renderer 状态。  
- 不要把 cancel 绑到 process 审批条（C3 write cancel 曾要求 bar 存在；C4 process cancel 故意可在 bar submitting 时点 Composer Stop）。

---

## 7. 本 Worker 已做的只读验证

| 检查 | 结果 |
| --- | --- |
| `composer-cancel` / `canCancel` / `isRunActive` 源码追踪 | 完成（上文行号） |
| cancel-tree / cancelOwningRun / long-tree / decide 同步 execute | 完成 |
| git status / diff 范围（相对 HEAD） | 完成；Composer/`isRunActive` 非脏 |
| unit：`vitest run tests/conversation-gateway-stream.test.tsx` | **8/8 pass**（含 workbench cancel） |
| 桌面 E2E / wave acceptance | **未跑**（总控独占） |
| 修改产品/测试 | **无** |

---

## 8. 六个问题对照表

| # | 回答 | 置信度 |
| --- | --- | --- |
| 1 | 见 §1：`canCancel` ← `isRunActive(run)` ← App ownership-gated `activeRun` | **高** |
| 2 | 见 §2：approve 之后、`cancelOwningRun`；run 应仍 active，cancel 应已可见 | **高** |
| 3 | 见 §3：60s 内 canCancel 恒 false；主路径 A 所有权/list 竞态，B 提前终态；C 为旁路 | **中**（现场分支需 E2E dump） |
| 4 | 非 D3B4 直接改坏 cancel 条件；长串行放大既有竞态/时序 + D3B4 后未重跑 process 全套 | **中高** |
| 5 | 与变更挤资源浏览器 **不同源** | **高** |
| 6 | 见 §6：优先 reducer 保留 selected + list generation；harness 早断言 cancel | **中高**（修复有效性待复跑） |

### 无法确定（需总控复跑，勿编）

1. RED 当次是路径 A 还是 B（缺失败瞬间 DOM/timeline dump）。  
2. `markersBefore` 在失败次是否 >0（未写入失败 record）。  
3. 单跑 `cancel-tree` 是否稳定 GREEN（区分 flaky 竞态 vs 必现终态）。  
4. 另一 Worker 对 App/Workbench 的在途修改是否已/将改变本结论（本诊断基于任务开始时的只读快照）。

### 建议总控复跑命令（本 Worker 不执行）

```text
# 1) 仅 process 套件（串行、总控独占）
pnpm --filter @pillow/desktop exec node --test --test-concurrency=1 tests/e2e/c4-governed-process.spec.ts

# 2) 若仍红：在 cancelOwningRun 超时分支临时加 evidence（或 CDP 断点）
#    - page.getByTestId('composer-cancel').count()
#    - body 是否含 已成功/已失败/已取消/已中断
#    - conversation-row 数量与选中态
#    - 是否仍存在 command-approval-bar
```

---

## 9. 完成信号

诊断结论可交付：cancel 缺失是 **UI `canCancel` 链**问题，不是 harness 选错 testid；与 Git 变更面板回归不同源；最小修复应保护 selected conversation 的 ownership 证明并收紧 list 竞态，辅以 harness 早断言。

C4-D3B3-CANCEL-TREE-DIAGNOSIS-COMPLETE
