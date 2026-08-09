# Step 1a — 治理契约类型搬家报告

**日期**: 2026-08-08  
**任务**: 纯移动 + re-export，零行为变更  
**工作目录**: `<repo>`  
**Local session note**: 尝试 worker boot 返回 `400 worker claims require local parent session id`（无 parent 会话时预期）；未读取 auth/token。

---

## 1. 搬迁清单

| 类型 | 源位置 | 新位置 | 状态 |
|------|--------|--------|------|
| `ProviderToolProposal` | `runtime-host/src/governed_read.rs` | `agent-protocol/src/governed.rs` | 已搬 |
| `GovernedReadDelivery` | `runtime-host/src/governed_read.rs` | `agent-protocol/src/governed.rs` | 已搬 |
| `GovernedReadReceipt` | `runtime-host/src/governed_read.rs` | `agent-protocol/src/governed.rs` | 已搬 |
| `ProviderWriteProposal` | `runtime-host/src/governed_write.rs` | `agent-protocol/src/governed.rs` | 已搬 |
| `GovernedWriteDelivery` | `runtime-host/src/governed_write.rs` | `agent-protocol/src/governed.rs` | 已搬 |
| `GovernedWriteReceipt` | `runtime-host/src/governed_write.rs` | `agent-protocol/src/governed.rs` | 已搬 |
| `ProviderProcessProposal` | `runtime-host/src/host/process.rs` | `agent-protocol/src/governed.rs` | 已搬 |
| `GovernedProcessDelivery` | `runtime-host/src/host/process.rs` | `agent-protocol/src/governed.rs` | 已搬 |
| `GovernedProcessReceipt` | `runtime-host/src/host/process.rs` | `agent-protocol/src/governed.rs` | 已搬 |

**向后兼容 re-export**（既有 `use keeper_runtime_host::X` 不断）：

- `runtime-host/src/governed_read.rs` → `pub use keeper_agent_protocol::{GovernedReadDelivery, GovernedReadReceipt, ProviderToolProposal}`
- `runtime-host/src/governed_write.rs` → `pub use keeper_agent_protocol::{GovernedWriteDelivery, GovernedWriteReceipt, ProviderWriteProposal}`
- `runtime-host/src/host/process.rs` → `pub use keeper_agent_protocol::{GovernedProcessDelivery, GovernedProcessReceipt, ProviderProcessProposal}`
- 经既有 `lib.rs` / `host/mod.rs` 路径继续对外导出

### 未搬类型与理由

| 类型 | 保留位置 | 理由 |
|------|----------|------|
| `GovernedReadBridgeError` / `GovernedWriteBridgeError` / `GovernedProcessBridgeError` | runtime-host | 宿主 bridge 执行面错误；与 `execute_*` 成对，非纯结果 DTO；adapter 仍因调用执行函数需要它们 |
| `execute_governed_read` / `execute_governed_write` / `execute_governed_process` | runtime-host | 宿主执行逻辑，硬约束禁止搬 |
| `decide_*` / `list_*` / `recover_*` / grant helpers | runtime-host | 宿主执行与审批逻辑 |
| `GovernedProcessLifecycle` | runtime-host | 宿主 callback API，依赖 `ProcessProposalRequestedView`（宿主视图） |
| `ProcessProposalRequestedView` / `ProcessRiskClass`（host 侧） / `ProcessOrigin` | runtime-host | 宿主会话面视图，非 adapter 契约结果 |
| `LiveWriteProposalView` / `WriteDiffView` | runtime-host | `LiveWriteProposalView` 字段含 `BoundedDiff`（`tool-executor`）；上移会迫使 `agent-protocol` 依赖更高层 crate → **禁止** |
| `LiveProcessProposalView` / `ProcessOutputView` / `ProcessOutputChunk` / `ProcessOutputStream` | runtime-host | 宿主 live 投影 / list-open-output 面，非 adapter 归一化契约 |
| `RuntimeHost` / `HostConfig` / bind-release 协调器 | runtime-host | 宿主本体 |

---

## 2. 第 3 节三问回答

### 2.1 哪些搬了、哪些没搬，各自理由

见上表。只搬 **adapter 需要引用的治理结果/提案 DTO**：字段仅依赖 `agent-protocol` 已有类型、`serde_json::Value`、`keeper_policy::ExecProposalState`（`agent-protocol` 已依赖 `keeper-policy`）。凡牵涉执行逻辑、bridge 错误枚举、或更高层 crate 字段的类型一律留在 `runtime-host`。

### 2.2 搬完后 `adapter-grok-acp` 是否仍需 `keeper-runtime-host`？

**仍需要。** `crates/adapter-grok-acp/Cargo.toml` 的 `[dependencies]` 中保留：

```toml
keeper-runtime-host = { path = "../runtime-host" }
```

**原因（成败指标诚实结论）**：生产路径 `prompt.rs` 仍调用宿主执行面：

- `execute_governed_read` / `execute_governed_write` / `execute_governed_process`
- 对应 `Governed*BridgeError`
- `GovernedProcessLifecycle` 生命周期回调

本任务只搬家契约类型，**不**把执行函数迁出宿主。因此 adapter 不再**因为“要说治理结果”**而依赖宿主——`provider_tools` / `provider_writes` / `provider_processes` 与 `prompt` 中的 delivery/receipt 已改为 `keeper_agent_protocol`——但仍因**执行桥**依赖宿主。

后续若要把 adapter 对 host 的依赖降到零，需要另一步：把 `execute_*` 收成 trait/端口（仍属协议层）或由 host 注入 gateway，那是重构而非本步。

### 2.3 循环依赖风险如何规避

- `agent-protocol` 依赖面未增加 crate：仍为 `keeper-policy` + serde/chrono/uuid 等基础库。
- `GovernedProcessReceipt.state: ExecProposalState` 使用已有 `keeper_policy`（与协议 payload 一致）。
- `LiveWriteProposalView` 等含 `tool-executor` 类型**未搬**，避免 `agent-protocol → tool-executor → agent-protocol`。
- `runtime-host` 本就依赖 `agent-protocol`，改为 re-export 不引入反向边。
- 未让 `agent-protocol` 依赖 `runtime-host` / `tool-executor` / `storage-sqlite`。

---

## 3. Adapter 依赖面前后对比

### 前（概念）

| 用途 | crate |
|------|--------|
| 协议事件 / trait | `keeper-agent-protocol` |
| 治理提案/回执/delivery | `keeper-runtime-host` |
| `execute_*` + bridge errors + lifecycle | `keeper-runtime-host` |

### 后

| 用途 | crate |
|------|--------|
| 协议事件 / trait / **治理提案·回执·delivery** | `keeper-agent-protocol` |
| `execute_*` + bridge errors + lifecycle | `keeper-runtime-host`（仍保留） |

**生产 `src/` import 变化**：

- `provider_tools.rs` / `provider_writes.rs` / `provider_processes.rs`：契约类型全部改自 `keeper_agent_protocol`
- `prompt.rs`：delivery/receipt 自 protocol；执行与 bridge error 仍自 host

**`Cargo.toml`**：`keeper-runtime-host` 依赖行未删除（见 2.2）。

**集成测试**（`tests/*`）仍用 `HostConfig` / `RuntimeHost`，合理保留 host 依赖。

---

## 4. 变更文件（本任务白名单内）

| 路径 | 操作 |
|------|------|
| `crates/agent-protocol/src/governed.rs` | **新增** 契约模块 |
| `crates/agent-protocol/src/lib.rs` | 导出 governed 类型 |
| `crates/runtime-host/src/governed_read.rs` | 删除定义 → re-export |
| `crates/runtime-host/src/governed_write.rs` | 删除定义 → re-export |
| `crates/runtime-host/src/host/process.rs` | 删除定义 → re-export |
| `crates/adapter-grok-acp/src/prompt.rs` | import 分流 |
| `crates/adapter-grok-acp/src/provider_tools.rs` | import → protocol |
| `crates/adapter-grok-acp/src/provider_writes.rs` | import → protocol |
| `crates/adapter-grok-acp/src/provider_processes.rs` | import → protocol |
| `docs/execution/2026-08-08-step1a-contract-move-report.md` | 本报告 |

**未改**：`AgentRuntimeAdapter` 签名、字段形状、序列化、错误语义、执行逻辑、`apps/desktop/src/**`、git-executor、测试基建。  
**未动 Cargo.toml**：`agent-protocol` / `adapter-grok-acp` 依赖图无需增删即可完成搬家；`runtime-host` 已依赖 protocol。

---

## 5. 门禁结果

| 命令 | Exit code | 备注 |
|------|-----------|------|
| `node scripts/run-cargo.mjs test --workspace` | **0** | 最终全量通过；中间两次在高并行负载下曾 flake 见下 |
| `node scripts/run-cargo.mjs clippy --workspace -- -D warnings` | **0** | |
| `node scripts/run-cargo.mjs fmt --all -- --check` | **0** | |
| `node scripts/check-secrets.mjs` | **0** | |
| `git diff --check` | **0** | |
| `git diff --cached --name-only` | **0**（空） | 未 stage |

### 过程中的测试观察（未改逻辑）

- `governed_process_bridge::slow_output_is_visible_live_and_reopens_after_waiter_consumes_terminal` 在 **workspace 并行** 下曾两次失败（1s 轮询等 live stdout，负载下超时），**单测 / serial 均通过**。
- 与类型搬家无关：字段/路径未变；执行体未改。记录为宿主过程桥集成测试的既有 timing 脆弱点，**未顺手“优化”**。

---

## 6. 残余风险

1. **Adapter 仍依赖 runtime-host**（执行桥）—— 契约层目标已达成，宿主执行解耦待后续步骤。
2. **Bridge error 仍在 host**—— 若未来要在 protocol 冻结完整 `Result` 形状，可再搬纯 error enum（无更高层依赖）；本步保守未搬以免扩大表面。
3. **`slow_output_*` 并行 flake**—— 建议总控在安静环境重跑 workspace test，或后续单独加长轮询（不在本任务范围）。
4. **工作树并发污染**：会话中曾出现其他 Worker 改动的 `runtime-host/Cargo.toml`（test-support self-dep）与 git-executor/desktop 文件；已**回退**本任务无关的 `runtime-host/Cargo.toml` 改动，未触碰其它 Worker 文件。

---

## 7. 没做的事

- 未 commit / stage / stash / reset / clean
- 未跑桌面 E2E
- 未改 `AgentRuntimeAdapter`、执行函数、decide/list/recover
- 未搬 bridge errors / lifecycle / live views
- 未删除 adapter 对 `keeper-runtime-host` 的 Cargo 依赖
- 未修改 `apps/desktop/src/**`、测试基建、git-executor
- 未“顺手”修 timing flake

---

## 8. 信号

**STEP1A-CONTRACT-MOVE-COMPLETE**
