# Pillow — adapter ↔ runtime-host 耦合面盘点与解耦提案

日期：2026-08-08  
性质：只读设计审计；本报告是本单唯一写入  
建议：**D 阶段接入第二个 runtime 之前完成；待总控接受后冻结设计，不在并行 dirty 工作上实施**

## 0. 执行边界与摘要

本轮静态阅读了：

- `crates/adapter-grok-acp/src/**`；
- `crates/runtime-host/src/**`，重点覆盖 governed read/write/process、host、worker、sink、delivery；
- `crates/agent-protocol/src/**`；
- `docs/handoff/2026-08-08-pillow-controller-handoff.md` 第 7 节；
- C3/C3F/C4 checkpoint 以及当前 B2 lifecycle 实施报告；
- 根 `AGENTS.md`。

没有运行 Cargo、测试、gate、E2E、构建或桌面 smoke；没有读取认证或 token
材料；没有修改源码。工作树原本且持续有其他任务的 dirty 内容，本单不触碰。

核心结论有三点：

1. **直接编译耦合很窄**：`adapter-grok-acp` 的生产代码只有一个 normal
   Cargo 依赖、一个 import block、七个 `runtime-host` 符号。
2. **真实行为耦合很宽**：这七个符号背后还依赖 owner-scoped run 注册、同步
   阻塞、out-of-band decision、durable gate 后 callback、取消仲裁、single-flight、
   replay 零事件和 provider 固定错误面。机械地把三个 free function 包进 trait
   只能移动依赖，不能完成解耦。
3. **完成态应是双视图的共享治理服务**：adapter 只拿
   `Arc<dyn GovernedExecutionPort>`；host/UI 保留 bind/list/decide/recover/control
   能力；两者共享同一个 owner-scoped `GovernedToolService`，由不可复制的
   `GovernedRunLease` 原子管理 read/write/process authority。

## 1. 当前直接耦合面

### 1.1 直接编译依赖与符号

`crates/adapter-grok-acp/Cargo.toml` 的 normal dependencies 同时包含
`keeper-agent-protocol` 和 `keeper-runtime-host`。对
`crates/adapter-grok-acp/src/**` 的逐文件检索表明：
`src/prompt.rs:38-42` 是唯一直接导入 `keeper_runtime_host` 的生产文件。

| 编号 | adapter 位置 | runtime-host 依赖 | 当前用途 | 判定 | 注入后的对应面 |
|---|---|---|---|---|---|
| D1 | `Cargo.toml` | `keeper-runtime-host` normal dependency | 让下列七个符号可见 | **偶然** | normal edge 删除；改依赖 `keeper-governance-port` |
| D2 | `prompt.rs:425` | `execute_governed_read` | 根据 `run_id` 查 host authority，验证并执行 read，返回 durable receipt/replay | 治理语义**本质**；free function、全局查找**偶然** | `GovernedExecutionPort::execute_read` |
| D3 | `prompt.rs:427-443` | `GovernedReadBridgeError` | 穷举映射到 provider 固定 JSON-RPC 错误 | 封闭错误分类**本质**；host 内部错误类型泄露**偶然** | `GovernedReadExecutionError`，host 逐项转换 |
| D4 | `prompt.rs:516-546` | `execute_governed_write` | reserve 后 callback，等待 UI decision，返回 terminal/replay receipt | 粗粒度治理事务**本质**；free function/ambient coordinator**偶然** | `execute_write` + 借用 observer |
| D5 | `prompt.rs:548-574` | `GovernedWriteBridgeError` | invalid/busy/unavailable 分类；callback 还人工制造 `RunNotRegistered`/`Gateway` | 封闭分类**本质**；adapter 能制造 host authority error **偶然且危险** | 独立 execution error + 极窄 observer error |
| D6 | `prompt.rs:666-742` | `execute_governed_process` | reserve、等待 decision/claim/process terminal；接收 in-flight lifecycle | 粗粒度治理事务**本质**；free function/global coordinator**偶然** | `execute_process` + 借用 observer |
| D7 | `prompt.rs:711-741` | `GovernedProcessBridgeError` | invalid/conflict/busy/unavailable 分类 | 封闭分类**本质**；control/recovery 错误暴露给 adapter **偶然** | `GovernedProcessExecutionError`，含 `CleanupUnproven` |
| D8 | `prompt.rs:666-709` | `GovernedProcessLifecycle` | Requested/Decided/Started callback；Requested 还要手工把 host view 转成 protocol payload | durable lifecycle **本质**；host-only view/risk enum **偶然** | 端口只携带 protocol payload |

精确计数：三个函数、三个 bridge error、一个 process lifecycle enum，共七个
导入符号。adapter **没有**直接依赖 runtime-host 的 trait 或常量；固定方法名、
tool ID/version 和 provider 错误码均是 adapter 自己的 wire surface，而不是
runtime-host 常量。它们不应搬入治理端口。

`ProviderToolProposal`、`ProviderWriteProposal`、`ProviderProcessProposal`、
三种 receipt/delivery marker 以及 lifecycle payload 已由 adapter 直接从
`keeper-agent-protocol` 使用；`runtime-host` 虽为兼容仍重导出其中一些 DTO，
这不构成 adapter 的残留依赖，也不应在解耦时反向搬回 host。

### 1.2 测试依赖

以下是有意的真实集成耦合，不应与生产依赖混为一谈：

| 测试 | runtime-host 使用面 | 处理建议 |
|---|---|---|
| `tests/governed_read_roundtrip.rs` | `HostConfig`、`RuntimeHost`、register/send/shutdown | 保留真实 adapter → host → SQLite roundtrip |
| `tests/governed_write_roundtrip.rs` | 上述能力加 list/decide/idle | 保留审批、取消、replay 真实链 |
| `tests/governed_process_roundtrip.rs` | 上述能力加 process list/decide | 保留 claim/spawn/cleanup/lifecycle 真实链 |

当前共有 17 个 `GrokAcpAdapter::new(...)` 调用点：16 个位于 adapter 测试，
1 个位于 desktop composition。构造器注入是机械迁移面。生产 normal edge
移除后，可暂时把 `keeper-runtime-host` 留为 dev-dependency；若验收标准要求
连 dev edge 也为零，必须把三组 roundtrip 移到同时依赖两端的 desktop 集成层，
不能删除或用 fake 替代。

## 2. 隐含耦合与完整调用约定

这些约定没有全部出现在 Rust 类型中，却是第二个 adapter 最容易复制错的部分。

| 编号 | 隐含约定 | 当前证据 | 本质/偶然 | 端口/实现必须如何表达 |
|---|---|---|---|---|
| H1 | adapter 只提交 `run_id + normalized proposal`，不提交 project/conversation/root/capability/grant | `governed_read.rs:102-130`；write/process 同构 | **本质：authority 边界** | execute 签名禁止出现 authority 参数 |
| H2 | 外层必须在 provider 可发请求前绑定 run；pump 结束或启动失败后释放 | `conversation_service/service.rs:320-355, 609-635` | 顺序**本质**；手工 split API **偶然** | host-only `GovernedRunLease`；bind 成功后才启动 session |
| H3 | read/write/process 各有进程全局 `OnceLock` coordinator | `governed_read.rs:42-44`、`governed_write.rs:194`、`host/process.rs` coordinator | **偶然** | owner-scoped service 实例；不得用全局 service locator 作为完成态 |
| H4 | `register_governed_read_run` 实际先 bind read 再 bind write；第二步失败局部不回滚 | `host/mod.rs:185-217` | **偶然且有半绑定风险** | 单次原子 bind；失败回滚所有子 binding |
| H5 | `release_governed_read_run` 实际释放 read/write/process，命名与注册不对称 | `host/mod.rs:260-263` | **偶然** | 一个不可复制的 all-tools lease |
| H6 | 三个 execute 都同步阻塞；write/process 由其他线程的 decision/control 解锁 | 三个 execute 签名与 condvar waiter | **本质（当前模型）** | 同步 object-safe trait；不引入 `async_trait` |
| H7 | callback 在 execute 的 waiter/调用线程同步、串行发生；不得保存、异步或并发调用 | `host/process.rs:622-625, 861-883` | **本质** | 借用 observer；端口契约和 conformance test |
| H8 | 调 callback 前必须释放 coordinator lock；adapter 也不跨 execute 持有 `Inner` lock | `host/process.rs:861-883`、`prompt.rs` callback | **本质：避免死锁/再入** | 明文 MUST；锁屏障测试 |
| H9 | adapter prompt loop 由 `operation_gate` 串行；cancel 不取该 gate；UI decision 不经 adapter | `adapter.rs:199-252`、`turn.rs` | **本质的并发模型** | service 自己仍须 single-flight，不能信任 adapter 串行性 |
| H10 | cancellation 是持续轮询的 level-triggered `AtomicBool/SeqCst`，不是入口 snapshot | read/write/process execute 与 waiter loops | level-trigger **本质**；`AtomicBool` 类型**偶然** | `CancellationProbe::is_cancelled`，生产 impl 保持 `SeqCst` |
| H11 | stable call identity + canonical digest；同 identity 同请求 replay，不同请求 conflict | `turn.rs` identity registry；三个 reservation path | **本质** | proposal/receipt/delivery marker 保持一等类型 |
| H12 | write stable reservation 必须在重新捕获 base 前查询，否则 modify 成功后无法 replay | `governed_write.rs:493-538` | **本质** | 仍封装在单个 execute；绝不拆成 adapter 可排序的步骤 |
| H13 | replay 返回 durable receipt，但不得再次 read/apply/spawn，不得 callback，不得 lifecycle emit | 三个 receipt delivery gate；`prompt.rs` | **本质** | 端口 contract + real conformance tests |
| H14 | process callback 与 post-return receipt 都可能含 Decided/Started；adapter 用两个 flag 去重 | `prompt.rs:661-780` | 当前兼容约定**本质**；双通道形态可后续改良 | 迁移期保持；每 kind exactly once |
| H15 | terminal 不进 process callback；只在 durable terminal 后由 execute 返回 | `host/process.rs:615-628` | **本质** | process observer 没有 Terminal variant |
| H16 | provider response 只能在所需 runtime event 被 sink 接受后发送 | `prompt.rs:448-472, 577-618, 744-788` | **本质** | adapter 继续拥有 response/event 顺序 |
| H17 | host cancel 后仅允许一个安全 write resolution，然后唯一 `RunCancelled` | `sink.rs`、`delivery.rs`、`prompt.rs:600-608` | **本质的 C3F 边界** | 不藏进普通 bool/error；保留专门仲裁测试 |
| H18 | process 清理无法证明时保留 `CleanupUnproven` | process bridge/gateway | **本质：fail closed** | process execution error 保留独立 variant |
| H19 | runtime event 还要经过 ProtocolGuard → SQLite append/status transaction → commit → renderer emit | `sink.rs`、`delivery.rs`、desktop pipeline | **本质** | governance port 不接 renderer，也不绕过 EventSink |
| H20 | 只有匹配当前 run/session 且已 `mark_live` 的 `ActivePrompt` 才能进入 governed execute；observer 发事件前再次核对同一 run 仍 live | `prompt.rs:352-371, 483-501, 629-647` 及三个 callback | live/run 身份校验**本质**；`Inner` mutex/flag 形态**偶然** | adapter 保留前置校验；observer 用 `RunNoLongerLive` fail closed |
| H21 | RPC reader thread 可继续收包，但 prompt loop 同步处理 server request；阻塞 write/process 时后续请求只排队，decision/cancel 走带外路径 | `rpc/client.rs` 与 `run_prompt_loop` | 当前顺序/背压模型**本质** | 端口保持同步；第二 adapter 不得假设 callback 在独立 executor |

### 2.1 read 的现状时序

1. adapter 严格解析 ACP/custom RPC、session、tool/version/arguments。
2. native ACP transport ID 经规范化请求 identity 转成稳定 call ID；复用同 ID
   但参数不同，在进入 host 前返回 `-32030 provider_request_id_reused`。
3. host 以 `run_id` 查已绑定的 authority，计算 canonical digest，再按 active
   context/manifest 验证。
4. gateway reserve/claim/execute/complete；只有 durable receipt 完成后返回。
5. `Executed` 时 adapter 依次发 Started → Output → Completed；`Replayed`
   零事件；sink 接受后才回复 provider。

### 2.2 write 的现状时序

1. adapter 只接受 `pillow/write_tool` 与 `fs.write@1` 的精确参数。
2. host 再做 typed/context/digest 验证，并先查 stable reservation identity。
3. capture/prepare/base seal、durable reserve 到 `PendingApproval`、commit、写入
   live entry；此后才调用 Requested observer。
4. waiter 阻塞；UI 从 host control surface 决策。decision 先 durable commit；
   approve 再以 sealed content apply；terminal durable 后才唤醒 waiter。
5. execute 返回 terminal receipt；adapter 发 Resolved，再回复 provider。
6. replay 的 `requested=None`，adapter 不重复 Requested/Resolved。

Requested observer 失败的当前实现会 fail-closed 并中断/清理。这个行为必须写入
新端口实现的 conformance contract。

### 2.3 process 的现状时序

1. adapter 只接受 `pillow/process_tool` 与 `process.run@1` 的精确参数。
2. host 在 seal/reserve 前执行 one-pending-per-run 检查，busy 不产生 spawn
   authority。
3. durable reserve 后 Requested；durable decision commit 后 Decided；
   `claim_run → Claimed/Running` 后 Started；terminal 不 callback，而由 durable
   receipt 返回。
4. adapter 对 Decided/Started 做 callback/post-return 去重；terminal event 被
   sink 接受后才回复 provider。
5. replay 的 requested/decided/started 均为空，零 callback、零生命周期事件，
   且绝不再次 spawn。

当前 `GovernedProcessLifecycle::Requested` 暴露 host-only
`ProcessProposalRequestedView` 和 host risk enum，adapter 被迫逐字段转为 protocol
payload；这是最明确的一处类型泄漏。

### 2.4 错误语义：当前完整映射

#### Read

| bridge error / receipt | provider 表面 |
|---|---|
| `InvalidProposal`、`ReservationConflict` | `-32602 invalid_governed_read_request` |
| `InProgress` | `-32010 governed_read_in_progress` |
| `RunNotRegistered`、`InvalidContext`、`Gateway` | `-32000 governed_read_unavailable` |
| receipt Cancelled | `-32021` |
| receipt deadline exceeded | `-32022` |
| 其他失败/非终态 receipt | `-32020` |

#### Write

| bridge error / receipt | provider 表面 |
|---|---|
| `InvalidProposal`、`ReservationConflict` | `-32602 invalid_governed_write_request` |
| `Busy` | `-32010 governed_write_in_progress` |
| `RunNotRegistered`、`InvalidContext`、`Gateway`、`NotLive`、`OwnershipMismatch`、`InvalidGrant`、`DecisionDenied`、`Expired`、`Cancelled`、`Interrupted`、`ShutDown` | `-32000 governed_write_unavailable` |
| receipt rejected / expired / cancelled / conflict / failed / interrupted | `-32040` / `-32041` / `-32042` / `-32043` / `-32044` / `-32045` |

#### Process

| bridge error / receipt | provider 表面 |
|---|---|
| `InvalidProposal` | `-32602 invalid_governed_process_request` |
| `ReservationConflict` | `-32055 governed_process_conflict` |
| `Busy` | `-32056 governed_process_in_progress` |
| `RunNotRegistered`、`InvalidContext`、`Gateway`、`NotLive`、`OwnershipMismatch`、`DecisionDenied`、`Expired`、`Cancelled`、`Interrupted`、`ShutDown`、`CleanupUnproven` | `-32000 governed_process_unavailable` |
| receipt rejected / expired / cancelled / timed out / failed / interrupted | `-32050` / `-32051` / `-32052` / `-32053` / `-32054` / `-32057` |

当前 adapter 对 bridge enum 的 match 没有 wildcard，这是有价值的编译期信号。
新端口不得改成 `RuntimeError`、字符串、`Box<dyn Error>` 或单一
`Unavailable`。host 内部 control errors 可以不暴露给 adapter，但从内部错误到
端口错误的转换必须逐 variant exhaustive match；新增内部状态时应在 host 层编译
失败，新增端口状态时应在 adapter 层编译失败。

## 3. 推荐的解耦边界

### 3.1 trait 放置：新建 `keeper-governance-port`

推荐新增 `crates/governance-port`，包名 `keeper-governance-port`；除 std 外只
依赖 `keeper-agent-protocol`。端口错误是供 exhaustive match 的无数据枚举，
无需为了字符串化再引入 host error 或动态 error 依赖。

依赖方向：

`adapter-grok-acp → governance-port → agent-protocol`  
`runtime-host → governance-port → agent-protocol`

选择新 crate，而不直接放入 `agent-protocol`，理由如下：

1. `agent-protocol/src/governed.rs:1-4` 已明确将现有内容定义为纯
   proposal/result DTO，并声明 execution、bridge error、coordinator 属于 host。
   注入端口虽是抽象接口，却携带 durable timing、blocking、replay 和 lifecycle
   delivery 义务，属于 application governance API，不是 wire/DTO。
2. 该端口将被多个 runtime adapter 与 host 同时依赖，独立 crate 可以保持
   dependency DAG 清晰，并让它按“新模块独立可替换、聚焦测试”的仓库规则演进。
3. 放在 `runtime-host` 无法切断依赖；放在 Grok adapter 会造成 host 反向依赖
   provider-specific crate。
4. 新 crate 的成本仅是一个极小 manifest、接口文件和 contract tests；在第二个
   adapter 到来前，这个边界值得显式命名。

可接受但不首选的备选是放入 `agent-protocol::governance`。若总控拒绝新增
crate，它仍能切断依赖；但必须保持 implementation、DB/gateway、authority 和
control DTO 全部不进入 `agent-protocol`。不接受把 trait 定义在
`runtime-host` 或 adapter。

### 3.2 具体 trait

以下是推荐的对象安全接口。命名可在实施时按仓库风格微调，但方法集合、参数权限、
所有权和错误封闭性不应改变。

```rust
use std::sync::atomic::{AtomicBool, Ordering};

use keeper_agent_protocol::{
    GovernedProcessReceipt, GovernedReadReceipt, GovernedWriteReceipt,
    ProcessProposalDecidedPayload, ProcessProposalRequestedPayload,
    ProcessStartedPayload, ProviderProcessProposal, ProviderToolProposal,
    ProviderWriteProposal, RunId, WriteProposalRequestedPayload,
};

pub trait CancellationProbe: Send + Sync {
    /// Level-triggered; implementations must read the live state on every call.
    fn is_cancelled(&self) -> bool;
}

impl CancellationProbe for AtomicBool {
    fn is_cancelled(&self) -> bool {
        self.load(Ordering::SeqCst)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LifecycleObserverError {
    RunNoLongerLive,
    DeliveryFailed,
}

pub enum GovernedWriteLifecycle<'a> {
    Requested(&'a WriteProposalRequestedPayload),
}

pub enum GovernedProcessLifecycle<'a> {
    Requested(&'a ProcessProposalRequestedPayload),
    Decided(&'a ProcessProposalDecidedPayload),
    Started(&'a ProcessStartedPayload),
}

pub trait GovernedWriteObserver {
    fn observe(
        &mut self,
        event: GovernedWriteLifecycle<'_>,
    ) -> Result<(), LifecycleObserverError>;
}

pub trait GovernedProcessObserver {
    fn observe(
        &mut self,
        event: GovernedProcessLifecycle<'_>,
    ) -> Result<(), LifecycleObserverError>;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GovernedReadExecutionError {
    RunNotBound,
    InvalidProposal,
    ReservationConflict,
    InProgress,
    InvalidContext,
    Unavailable,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GovernedWriteExecutionError {
    RunNotBound,
    InvalidProposal,
    ReservationConflict,
    Busy,
    InvalidContext,
    NotLive,
    ShuttingDown,
    LifecycleDeliveryFailed,
    Unavailable,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GovernedProcessExecutionError {
    RunNotBound,
    InvalidProposal,
    ReservationConflict,
    Busy,
    InvalidContext,
    NotLive,
    ShuttingDown,
    CleanupUnproven,
    LifecycleDeliveryFailed,
    Unavailable,
}

/// Opaque, validated wrappers. Their inner receipts and constructors are not
/// public; all producers, including test doubles, must use checked conversion.
pub struct GovernedReadOutcome(GovernedReadReceipt);
pub struct GovernedWriteOutcome(GovernedWriteReceipt);
pub struct GovernedProcessOutcome(GovernedProcessReceipt);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GovernedOutcomeValidationError {
    InvalidDeliveryShape,
    InvalidLifecycleOrder,
    InvalidTerminalShape,
    ConflictingProviderOutcome,
}

pub trait CheckedGovernedOutcome<R>: Sized {
    fn try_from_receipt(
        receipt: R,
    ) -> Result<Self, GovernedOutcomeValidationError>;
    fn into_receipt(self) -> R;
}

// The port crate implements this trait for all three opaque outcome types.

pub trait GovernedExecutionPort: Send + Sync + 'static {
    fn execute_read(
        &self,
        run_id: RunId,
        proposal: ProviderToolProposal,
        cancellation: &dyn CancellationProbe,
    ) -> Result<GovernedReadOutcome, GovernedReadExecutionError>;

    fn execute_write(
        &self,
        run_id: RunId,
        proposal: ProviderWriteProposal,
        cancellation: &dyn CancellationProbe,
        observer: &mut dyn GovernedWriteObserver,
    ) -> Result<GovernedWriteOutcome, GovernedWriteExecutionError>;

    fn execute_process(
        &self,
        run_id: RunId,
        proposal: ProviderProcessProposal,
        cancellation: &dyn CancellationProbe,
        observer: &mut dyn GovernedProcessObserver,
    ) -> Result<GovernedProcessOutcome, GovernedProcessExecutionError>;
}
```

设计说明：

- **对象安全**：无泛型方法、关联类型、`impl Trait` 或返回 `Self`。
- **存储所有权**：`GrokAcpAdapter` 持有
  `Arc<dyn GovernedExecutionPort>`；trait object 为 `'static`，没有借用
  `RuntimeHost`。
- **调用期借用**：proposal 按值移动；端口按值返回 opaque validated outcome，
  adapter 再消费为 receipt；cancel probe 和 observer 只在阻塞调用期间借用。
  实现不得保存 observer/cancel reference。
- **同步而非 async**：现有 `AgentRuntimeAdapter::send_turn` 允许阻塞到
  terminal，host 在专用 worker 上调用；改 async 会同时改变取消、decision 和
  terminal 时序，不属于本单。
- **最小权限**：签名没有 project ID、conversation ID、root、capability、
  raw approval grant、gateway、DB、native executable authority、EventSink 或
  renderer。
- **typed lifecycle**：Requested 改为 protocol payload，消除
  `ProcessProposalRequestedView` 泄漏；process terminal 仍只在 receipt。
- **不把步骤拆开**：不提供 `validate`、`reserve`、`persist`、`commit`、
  `emit` 等可由 adapter 自由排序的方法。粗粒度 execute 是保住时序不变量的
  关键。
- **错误不抹平**：adapter-relevant 状态为闭集；host-only
  `OwnershipMismatch/InvalidGrant/DecisionDenied/Expired/Cancelled/Interrupted`
  继续留在 control/内部错误，逐项映射到当前 provider 的 unavailable 类。
- **结果不可伪造**：端口不直接返回 public-field receipt。三个 opaque outcome
  只可由 port crate 的 checked conversion 产生；最低校验包括 Replayed 不带
  lifecycle、Started 必须有 Decided、terminal 必须存在、write provider
  success/error 恰有一个，以及 state/cancelled/timed_out/provider outcome
  相互一致。第二个生产实现与测试 fake 都绕不过同一检查。

### 3.3 trait 无法单独表达的 MUST 契约

以下文字必须成为端口 rustdoc，并由每个生产实现的 conformance suite 证明：

1. 三个方法阻塞到 durable terminal、durable replay 或封闭 execution error。
2. observer 仅在当前调用线程同步、串行调用；不得保存、跨线程、并发或嵌套调用。
3. 调 observer 时不得持有治理 coordinator mutex。
4. 同一 `run_id` 的 observer 不允许再入 `execute_*`；cancel 与 host control
   decision 可以从其他线程并发。
5. Requested 仅在 durable reserve commit 后；Decided 仅在 decision commit 后；
   Started 仅在 durable claim 到 Claimed/Running 后。
6. observer 失败必须 fail-closed，并返回 `LifecycleDeliveryFailed`；不得遗留
   新的 write/spawn authority 或永久 Busy。
7. process terminal 不 callback，只在 terminal commit 后随 receipt 返回。
8. `Replayed` 不执行副作用、不调用 observer、不重复 lifecycle。
9. cancellation 每个 wait cycle 重新读取；不得只在入口 snapshot。
10. host 控制面和 execution port 必须来自同一 owner-scoped service。

普通 Rust trait 对 2-10 没有编译期证明能力。因此“有 trait”不是验收标准；
实现的 deterministic conformance tests 才是。

### 3.4 错误转换

runtime-host 实现应保留现有三个完整 bridge/control error，使用无 wildcard 的
转换函数：

| host 当前错误 | port 错误 |
|---|---|
| read `RunNotRegistered` / `InvalidProposal` / `ReservationConflict` / `InProgress` / `InvalidContext` / `Gateway` | 分别为 `RunNotBound` / `InvalidProposal` / `ReservationConflict` / `InProgress` / `InvalidContext` / `Unavailable` |
| write `RunNotRegistered` / `InvalidProposal` / `ReservationConflict` / `Busy` / `InvalidContext` / `NotLive` / `ShutDown` / `Gateway` | 对应 `RunNotBound` / `InvalidProposal` / `ReservationConflict` / `Busy` / `InvalidContext` / `NotLive` / `ShuttingDown` / `Unavailable` |
| write control-only `OwnershipMismatch` / `InvalidGrant` / `DecisionDenied` / `Expired` / `Cancelled` / `Interrupted` | `Unavailable`，同时保留 host audit；正常 execute terminal 应走 receipt |
| process `RunNotRegistered` / `InvalidProposal` / `ReservationConflict` / `Busy` / `InvalidContext` / `NotLive` / `ShutDown` / `CleanupUnproven` / `Gateway` | 对应独立 port variant；`Gateway → Unavailable` |
| process control-only `OwnershipMismatch` / `DecisionDenied` / `Expired` / `Cancelled` / `Interrupted` | `Unavailable`，同时保留 host audit；正常 terminal 应走 receipt |
| observer `RunNoLongerLive` / `DeliveryFailed` | host 先执行规定的 fail-closed，再返回 `LifecycleDeliveryFailed` |

adapter 对 port enum 也必须 exhaustive match，保持第 2.4 节的 provider code/message
完全不变。

## 4. 谁实现、谁注入、在哪里组装

### 4.1 完成态对象关系

`runtime-host` 新增 owner-scoped `GovernedToolService`：

- 拥有 owner ID、read/write/process gateways、run bindings、live/pending
  coordinator；
- 实现 `GovernedExecutionPort`；
- 对 host 暴露 bind/list/decide/cancel/recover/shutdown 控制面；
- 对 adapter 只擦除为 execution trait object；
- 本身不持有 adapter。

`RuntimeHost` 与 `GrokAcpAdapter` 共享同一个 service allocation：

```text
desktop composition
  ├─ Arc<GovernedToolService>
  │    ├─ erased Arc<dyn GovernedExecutionPort> ──> GrokAcpAdapter
  │    └─ concrete control handle ───────────────> RuntimeHost
  └─ RuntimeHost ──owns──> Arc<dyn AgentRuntimeAdapter>
```

这不是 ownership cycle：service 不拥有 adapter。禁止把
`Arc<RuntimeHost>` 注回 adapter；当前 host 已拥有 adapter，这样会形成构造和
生命周期环。

adapter 的构造边界应明确为
`GrokAcpAdapter::new(config: GrokAcpConfig,
governance: Arc<dyn GovernedExecutionPort>) -> Result<Self, RuntimeError>`，
字段为非可选 `Arc`；后续所有 prompt handler 只借用该 handle。

### 4.2 组合方式

当前 desktop 是“先 `GrokAcpAdapter::new`，再
`ConversationService::open_with_adapter`，后者才打开治理 gateways”。完成态应
增加 prepared builder 或 adapter factory：

```rust
let prepared = PreparedRuntimeHost::open(database_path, host_config)?;
let execution = prepared.governed_execution_port();
let adapter = Arc::new(GrokAcpAdapter::new(grok_config, execution)?);
let host = prepared.finish(adapter);
```

desktop 的 `ConversationService::open_with_adapter_factory` 可封装这段组装。
现有 fake/non-governed constructor 可保留，但生产 Grok adapter 不得拥有
`Option`、silent no-op 或默认 unavailable port；漏注入必须在构造时/编译时失败。

### 4.3 原子 run lease

`GovernedToolService::bind_run(scope)` 应一次验证并绑定 read/write/process，
成功后返回不可 `Clone` 的 `GovernedRunLease`：

- scope 由 host 提供 project/conversation/root；adapter 永远不可见；
- 任一子绑定失败，必须回滚全部，不能留下 read-only 半绑定；
- lease 在 provider session/start worker 前取得；
- session/start/spawn 失败时由 Drop 自动释放；
- pump 结束后释放；shutdown 还要 owner-wide fail-closed；
- lease 不得比 run 的 delivery/pump 生命周期更早 Drop。

原子 lease 是完成态的一部分，不是可选美化。否则第二个 adapter 虽不再编译依赖
runtime-host，仍依赖同一套隐式全局注册顺序。

### 4.4 lease 所有权与 shutdown 状态机

`GovernedRunLease` 必须是 `Send + 'static`、不可 `Clone`，并带
`owner_id + run_id + generation`。绑定后的精确所有权流是：

1. `Bound`：send-message 调用线程取得 lease；尚未启动 provider。
2. `Active` 有两条所有权路径：
   - async spawn 成功时把 lease **move** 进 pump closure/worker record；调用
     线程不再保留副本；
   - 同步 `RuntimeHost::send_turn` admission 把 lease **move** 进
     `GovernedSyncTurnGuard`，由 guard 持有到 `send_turn` 返回且 delivery/terminal
     仲裁完成，不能由外层调用栈临时变量提前 Drop。
3. `Draining`：cancel 或 terminalization 已开始，拒绝新 execute/bind，但现有
   waiter、C3F safe resolution 和唯一 terminal 仍可完成。
4. `Released`：pump 完成且 delivery 仲裁结束后，lease Drop 以
   owner/run/generation 幂等解除全部 binding；旧 lease 不能释放同 run 的新一代
   binding。

若 session/start/spawn 或 sync admission 在 move 前失败，调用线程 Drop lease 并
回滚；若 worker 启动后失败，由 worker/pump 的 finally 路径持有并释放；同步路径
由 sync-turn guard 的唯一退出路径释放。不得把 lease 留在 adapter，也不得用一个
临时栈变量在 worker 或同步 `send_turn` 尚运行时提前 Drop。

service/host shutdown 必须保留当前顺序，不得因 RAII 改写：

1. 线性化 shutdown，拒绝新 turn/bind；
2. 在 delivery 仲裁下取消 workers，并更新 host/adapter cancel state；
3. 等待 sync turns 退出；
4. 调 adapter shutdown；
5. join workers，检查 residual workers/process cleanup；
6. 只有所有 waiter 与安全 write resolution 已完成后，Drop 残余 leases，并执行
   owner-wide idempotent release；
7. 最后关闭 service/gateway。

owner release 早于第 5/6 步会把“正常取消并交付安全 resolution”变成
`RunNotBound/NotLive`，属于 C3F 回归。上述顺序需要 shutdown barrier 和 stale
lease generation tests。

## 5. `validate → persist → commit → emit` 如何保真

该不变量有两层，不能混为一句口号：

1. **治理状态层**：adapter wire normalization → host scoped validation →
   durable reserve/decision/claim/terminal persistence → transaction commit →
   lifecycle observer/receipt。
2. **runtime event 层**：adapter 构造带 sequence 的 `RuntimeEvent` →
   `ProtocolGuardSink` 校验 → SQLite event/status transaction → commit →
   renderer emit。

端口位于两层之间，但不拥有 EventSink。它只保证 callback/receipt 不早于相应
治理 commit；adapter 继续负责 sequence、runtime event 与 provider response
时序；desktop pipeline 继续保证 event commit 先于 renderer。

按事件的不可变 gate：

| 事件 | 最早允许时点 |
|---|---|
| read Started/Output/Completed | read durable execution receipt 返回后；仅 Executed |
| WriteProposalRequested | durable PendingApproval reserve commit + live entry 后 |
| WriteProposalResolved | durable terminal receipt 返回后 |
| ProcessProposalRequested | durable reserve commit 后 |
| ProcessProposalDecided | durable decision commit 后 |
| ProcessStarted | durable claim 为 Claimed/Running 后 |
| ProcessTerminalOutcome | durable terminal receipt 返回后 |
| provider success/error response | 所需 runtime event 已被 sink 接受后 |

特别禁止为了“接口更细”把 validate/reserve/commit/emit 分成 adapter 可调方法；
那会把项目级不变量从 host 的结构性保证降成调用者约定。

## 6. 取消、重放与再入

### 6.1 取消

- `CancellationProbe` 必须是 live、level-triggered；生产 `AtomicBool` 适配继续
  `SeqCst`，不能复制成入口 bool。
- 当前 async host 先在 delivery 仲裁下发布 cancel，再设置 host cancel state，
  随后调用 adapter `cancel_turn` 设置 adapter token；这是两套状态加一个提交
  仲裁。抽 port 时不能无证明地合并为一个较弱 token，若要统一取消源须另做竞态
  设计与 barrier 验收。
- adapter `operation_gate` 仍串行 prompt，但 `cancel_turn` 不取该 gate，因而
  能打断阻塞的 write/process waiter。
- write 必须保留 C3F 特例：host cancel 后允许至多一个已验证的
  cancelled/cancelled 或 interrupted/interrupted Resolved，在唯一
  `RunCancelled` 之前提交；其他事件被拒绝。
- 当前 EventSink 的 `RuntimeError::Cancelled` 同时可能表示“未提交”和
  “安全 resolution 已提交但 cancel 赢得终端仲裁”。adapter 的精确 receipt
  检查仍必须保留。治理 trait 本身不能把这个歧义假装消除。
- process 必须把取消传播给 child；清理无法证明时仍返回
  `CleanupUnproven`，不能降成普通 cancelled。

### 6.2 重放

三类 replay 均须满足：

- 相同 reservation identity + 相同 canonical request 返回同一 durable terminal；
- 不重复 read/apply/spawn；
- observer 调用次数为零；
- adapter lifecycle 新增数为零；
- write 必须先查 terminal reservation 再 recapture base；
- identity 相同而规范化请求不同必须 conflict。

### 6.3 再入

- port 实现不得持 coordinator lock 调 observer，observer 可安全进入 EventSink。
- observer 不得再入同 run 的 governed execute；这是明确禁止，不靠偶然 mutex
  死锁来限制。
- decision/list/cancel/recover 是 host control surface，可与 waiter 并发，但永远
  不注入 adapter。
- service 自己必须执行 single-flight/busy 检查，不能假定某个 adapter 永远串行。

## 7. 测试替身与现有测试改动

### 7.1 三层测试替身

1. **`PanicGovernedExecutionPort`（test-only）**  
   给不应触发治理调用的 adapter tests；任何调用立即失败，避免 silent fallback。
2. **`CheckedScriptedGovernedExecutionPort`（test-only）**  
   以队列声明期望的 `run_id/proposal`、cancel barrier、lifecycle 和结果；调用顺序、
   多/少调用均失败。只能通过校验 builder 构造合法 outcome。
3. **真实 `GovernedToolService`**  
   三组 adapter-host-SQLite roundtrip 和 runtime-host bridge/conformance tests
   继续使用真实 SQLite/gateway/decision，不被 fake 替代。

不导出 production default/no-op；`GrokAcpAdapter` 不接受
`Option<Arc<...>>`。

### 7.2 防止“太容易绿”的 fake

当前 `GovernedWriteReceipt` 与 `GovernedProcessReceipt` 有多个 public
`Option`/bool，fake 可构造生产不会产生的组合，例如 Replayed 却带 Requested、
Started 存在而 Decided 不存在、success 与 provider error 同时存在。

Step 1 就必须做到：

- production port 和 testkit 都只能返回 `Governed*Outcome`；
- checked conversion 校验 delivery 与 lifecycle 组合、terminal/flags/provider
  outcome 一致性；
- adapter 的 scripted fake 只证明调用与映射，不被当作治理正确性证据；
- 真实 roundtrip/conformance 保持验收地位。

应先从当前生产构造点和 roundtrip 枚举所有合法 receipt 组合，再冻结 checked
conversion；不允许把 opaque outcome 推迟到第二个生产实现之后。现有
agent-protocol receipt 可暂时保持兼容 public DTO，但它不再是 execution port
可直接返回的类型。

### 7.3 预计改动量

- 17 个 adapter constructor 调用点增加 port；
- 非治理 fake-child/H5 tests 用 panic fake；
- live native acceptance 用显式 unavailable test fake；
- 三个 governed roundtrip 注入真实 service handle；
- `prompt.rs` 三处 free function 改为 `self.governance.execute_*`，并把 handle
  沿 `run_prompt_loop → handle_incoming → handle_server_request` 传递；
- process Requested 转换移入 runtime-host/service；
- adapter normal dependency 删除；真实 roundtrip 所需 dev dependency 暂留或
  后移到 desktop 集成层。

### 7.4 必须新增的 conformance 场景

- callbacks 同一 waiter thread、串行、无 coordinator lock；
- durable barrier 明确发生在每个 callback 前，使用确定性 barrier，不靠 sleep；
- Requested → Decided → Started → Terminal，拒绝/过期路径无 Started；
- claim failure 永不 Started；
- 每 kind exactly once，post-return 补发不重复；
- replay 零 callback、零事件、零副作用；
- cancellation 在进入后才翻转仍可被观察；
- write safe resolution 先于唯一 RunCancelled；
- observer failure 的 durable fail-closed 与 live/pending 清理；
- process `CleanupUnproven` 保留；
- atomic bind 任一步失败无残留，lease Drop/owner shutdown 精确释放；
- async pump 与同步 `RuntimeHost::send_turn` 两条 admission 都持有 lease 到
  terminal/delivery 仲裁结束；shutdown 等待期间 binding 不提前消失；
- host internal error → port error、port error → provider error 均 exhaustive。

## 8. 对 C3/C4 checkpoint 的影响

### 8.1 不应改变的已接受边界

- C3：provider 没有 filesystem 或 approval authority；只有 host 注入 project
  scope/capability，project switch/cancel/expiry/restart 使 authority 失效。
- C3F：cancel 后至多一个精确安全 write resolution，且位于唯一
  `RunCancelled` 之前。
- C4/process：fixed tool、bounded output、single-flight、durable replay、
  cancellation/cleanup、provider-safe/path-free error。
- adapter 继续拥有 provider wire、runtime sequence/event 构造；host 继续拥有
  approval/grant/apply/spawn/audit/durable state。

若按本报告迁移，外部语义 checkpoint 不重写；但生产代码路径改变，必须做一次
“继承式复验”并记录新的 checkpoint delta。

### 8.2 当前源码基线高于历史 C4 文档

`docs/verification/2026-08-07-wave-c4-checkpoint.md:119-126` 记录的是当时
“process lifecycle 只在 execute 返回后发出”的历史缺口。当前 B2 与源码已经是：

- Requested 在 reserve commit 后 in-flight callback；
- Decided 在 decision commit 后 callback；
- Started 在 Claimed/Running 后 callback；
- terminal 才是 post-return；
- adapter flags 防止 callback 与 receipt 双发；
- replay 零 callback/零 lifecycle。

迁移必须以当前行为为基线，不能拿旧 checkpoint 的缺口当“更简单的兼容行为”。

### 8.3 明确的红线

- 🔴 **若 execute 签名向 adapter 暴露 project/root/capability/grant/list/decide，
  就改变了 C3/C4 authority boundary。**
- 🔴 **若 callback 早于 durable commit、改为异步/可保存、持锁调用、增加 terminal
  callback，或 replay 再 callback/emit，就是治理时序回归。**
- 🔴 **若取消只 snapshot 一次、丢掉 C3F safe resolution 仲裁，或把
  `CleanupUnproven` 降级为 cancelled/unavailable，就是 fail-open。**
- 🔴 **若用 permissive fake 取代真实 SQLite roundtrip，checkpoint 证据变弱。**
- 🔴 **若顺便加入 durable outbox、改变 receipt 状态形状或 renderer delivery，
  已超出纯解耦，必须另开设计与 acceptance。**

### 8.4 两个需要单独接受的现有风险

1. **commit → event sink 不是跨存储原子事务。** durable terminal 已提交但
   EventSink 失败时，下一次 replay 又抑制 lifecycle，可能出现 durable receipt
   存在而 runtime event 缺失。trait 只能保序，不能提供 crash-safe exactly-once；
   真正修复需要 durable outbox/ack，属于另一个设计。
2. **process Decided/Started observer failure 的清理语义不完整。**
   `host/process.rs:861-883` 先把 delivered flag 置位，锁外 callback 后直接
   `?` 返回；它没有 Requested 路径 `fail_closed_after_reserve` 的对称清理。
   新端口不能把这点含糊写成“自动 fail-closed”。应先用 RED/确定性测试锁定，
   再以单独 checkpoint delta 修复；至少不得在抽 trait 时静默改变。

## 9. 增量落地方案

下面均是**未来实施步骤**；本只读任务没有执行表中的任何命令。Cargo 和桌面门
必须串行，避免共享 target/linker 锁。

| 步骤 | 独立提交内容 | 是否可独立过门/提交 | 未来最小验证 |
|---|---|---|---|
| 1. 冻结端口与合法 outcome | 新增 `keeper-governance-port`、闭合 errors、observer/cancel traits、opaque checked outcomes、rustdoc/testkit；无 consumer | 是；纯新增、无行为变化；须先由总控接受设计 | port crate fmt/check/clippy/unit；逐个合法/非法 receipt shape |
| 2. 修复 observer-failure 前置缺口 | 先加确定性 RED，再让 process Decided/Started observer failure durable fail-closed、清除 live/pending authority；单独记录 checkpoint delta | 是；这是明确行为修复，**不能藏进 trait 重构** | process bridge callback-failure、busy cleanup、restart/recovery |
| 3. 迁移 live cancellation probe | runtime-host 三个 executor/waiter 从 `&AtomicBool` 改为 `&dyn CancellationProbe`；adapter 现有 token 通过 `AtomicBool` impl 传入，禁止 snapshot | 是；预期无语义变化 | 调用进入后再翻转 cancel 的 deterministic barrier；`SeqCst` 保真 |
| 4. 兼容实现 | runtime-host 新增 `GovernedExecutionPort` impl、checked outcome conversion 与逐项错误转换；暂时委托现有 free functions/global registry | 是；此前 Step 2/3 已使实现满足 rustdoc；**仅过渡，不得宣称解耦完成** | runtime-host unit/bridge；outcome/error-map exhaustiveness |
| 5. 注入并切 normal edge | adapter 构造器/调用链注入；desktop 用临时 handle 组装；17 个调用点与测试 wiring 同提交；runtime-host 移为 dev-only | 是；该提交必须保持所有调用点同编译 | adapter unit/roundtrip/protocol fixture；normal dependency graph 不含 runtime-host；desktop typecheck/unit |
| 6. 抽 owner-scoped service | 引入 `GovernedToolService` 与 prepared/factory composition；host/adapter 共享同一 Arc，free-function wrapper 暂留兼容 | 是 | ownership/shutdown/composition tests；desktop smoke |
| 7. 迁移 read state | read binding/coordinator/execute 移入 service instance；兼容 wrapper 转发 | 是 | read bridge + read roundtrip + replay/conflict |
| 8. 迁移 write state | write coordinator/control/execute 移入同一 service；保留 C3/C3F | 是 | write bridge/roundtrip；approval/cancel/replay/barrier |
| 9. 迁移 process state | process coordinator/control/execute 移入 service；保留 B2 lifecycle 和 cleanup | 是 | process bridge/roundtrip；in-flight order/no-double/replay/cleanup |
| 10. 原子 lease 与清尾 | 单一 bind + `GovernedRunLease`；实现 Bound→Active→Draining→Released；async pump 与 sync-turn guard 各自拥有 lease；替换 split register/release；删除 globals/free-function compatibility；按需迁移 dev roundtrips | 是；到此才可称 implementation complete | bind rollback/Drop/generation；sync/async admission；shutdown 顺序；全 workspace gate |
| 11. 控制器继承式复验 | 不改语义代码；串行执行 typecheck、lint、unit、integration、adapter acceptance、desktop smoke/E2E，并记录 checkpoint delta | 可独立验收记录；到此才可称 verified | 全 Required Gates；无 UI diff 时不新增截图，若出现 UI 改动则补支持尺寸截图 |

每步提交范围必须只含本步；不得夹带当前 settings/UI dirty 工作。Step 4 的薄 wrapper
有价值，因为它先固定错误和回调契约，并允许 Step 5 切断 production Cargo edge；
但只完成 Step 4/5 会把 coupling 藏到 global service locator 后面，不能关闭本风险。

## 10. 反驳本提案

### 10.1 哪些地方可能让治理语义变弱

1. **trait 名称带来的虚假安全感。** Rust 能检查方法与 error match，不能检查
   durable commit 在 callback 前、observer 不持锁、replay 零 callback。
2. **错误压缩。** 若把所有 host error 压成 `Unavailable`，新增 authority/
   cleanup 状态不会迫使 adapter 和测试更新。本提案因此保留闭合 port 分类与 host
   exhaustive conversion。
3. **测试 fake 过度自由。** public receipt 可制造不可能状态；只跑 fake 会让错误
   实现轻易绿。本提案保留真实 roundtrip，并要求 checked builder。
4. **lease Drop 时点。** RAII 能修复遗漏释放，也可能因所有权放错而过早释放。
   lease 必须由 run/pump lifecycle 持有，而不是 adapter 请求栈临时变量。
5. **observer failure。** 如果新接口只写“delivery failed”却不规定 durable
   fail-closed，write/process authority 可能残留。
6. **取消抽象过度。** 把 `AtomicBool` 换成普通 bool 或一次性 future 会破坏
   level-trigger；抽象必须保留活探针。

### 10.2 哪些“解耦”只是搬家

- runtime-host 提供一个零状态 handle，内部仍调用三个 global free functions；
- trait 定义在 runtime-host，adapter 仍需依赖它；
- adapter 持有 `Arc<RuntimeHost>`，形成 ownership cycle；
- 把 globals 移到新 crate，但 registration/owner/release 仍是手写服务定位器；
- 给 read/write/process 注入三个互不相关 handle，无法证明来自同一 owner/service；
- 把 list/decide/grant 一并塞进 adapter-facing trait，实际上扩大耦合和权限；
- 只移动 DTO/error enum，却复制 lifecycle 去重、取消和 replay 逻辑到第二 adapter。

因此本提案把“production Cargo 边断开”和“治理所有权真正解耦”分成两个里程碑，
只在 Step 10 后称 implementation complete，并在 Step 11 后称 verified。

### 10.3 新 crate 和 service 的代价

- 新增 workspace crate、公共 API 和 conformance 维护面；
- prepared builder/factory 会触及 desktop composition 和多处测试构造；
- `GovernedToolService` 有成为 god object 的风险，必须保持 execution 与 control
  两个能力视图，内部 read/write/process 模块仍可独立替换；
- atomic all-tools lease 会让将来“某 runtime 只支持 read”需要显式 capability
  profile，而不能靠缺 gateway 的偶然行为。若 D 需要部分能力，应先把 supported
  capabilities 做成 host-owned、不可由 adapter 扩权的枚举。

这些代价真实存在，所以不建议在当前并行 dirty wave 中立刻重构。

### 10.4 不做的真实代价

现在不做的短期产品成本很低：生产编译耦合只有一个 import block，第二 runtime
尚未接入，C4 也把 additional runtimes 列为 out of scope。

一旦第二个 adapter 开始实现，成本会迅速上升：它将复制三组 provider error
mapping、Requested/Decided/Started 去重、write safe-cancel 仲裁、stable
reservation/replay 和 run-registration 顺序。届时再抽取需要同时迁移两个
adapter，并判断两套已分叉行为谁才是基线。

## 11. 最终建议

**选择：D 阶段前做。**

具体含义：

- **现在**：建议总控评审本设计；只有总控明确接受后才标记 design accepted/冻结。
  待当前并行 build/dirty worker 收束后，再开独立实施单。
- **D 阶段第二个 runtime 开工前**：至少完成 Step 1-10，以 owner-scoped
  `GovernedToolService`、单一 execution port、原子 `GovernedRunLease` 和真实
  conformance/roundtrip 为验收标准。
- **不接受**：只完成薄 trait/global forwarding 就关闭风险；也不接受把本重构
  拖到第二 adapter 已复制治理逻辑之后。

理由是“现在重构”的并发/验收风险高于即时收益，而“第二 runtime 后再重构”的
重复实现和行为分叉成本明显更高。handoff 给出的时间点是正确的。

---

本报告结论来自静态源码与 checkpoint 对照；未以历史报告代替当前源码。当前任务
没有运行任何 gate，因此本文当前状态仅为 **proposed**，不是 accepted、
implemented 或 verified；文中的未来门仅是实施建议，不是本轮通过声明。
