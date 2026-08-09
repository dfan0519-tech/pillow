# Pillow 解耦 Step 4 执行报告

状态：**DECOUPLE-STEP4-COMPLETE**

workspace worker gate：**BLOCKED-BY-CONCURRENT**

本单只完成提案增量表第 4 行的过渡兼容实现。它**没有完成解耦，也不能关闭
adapter 与 runtime-host 的耦合风险**。新实现仍委托既有 free functions / global
registry；依赖与所有权的结构性改变仍属于 Step 5–10。

## 1. 实施范围

`runtime-host` 新增并导出 `GlobalGovernedExecutionPort`：

- 零状态、`Send + Sync` 的 `GovernedExecutionPort` 实现；
- `execute_read/write/process` 原样借用 live `CancellationProbe`，转发现有
  `execute_governed_*` free functions；
- 使用 `CheckedGovernedOutcome::try_from_receipt` 将三类生产 receipt 受检转换为
  opaque outcome；
- 将三类 bridge 闭合错误集无 wildcard 地逐项转换为 port 闭合错误集；
- 将 write/process observer 错误与普通 bridge `Gateway` 区分，既有 bridge 完成
  fail-closed 后返回 `LifecycleDeliveryFailed`；
- 将 process Requested 的 host view 仅在同步回调栈内转换为 protocol payload，
  Decided/Started 继续借用既有 protocol payload。

未修改任何既有 free function，也未给 adapter 注入 port；未抽 service、未改变
ownership/lease、未删除 global registry。

## 2. 实现结构与设计取舍

### 2.1 为什么是 `GlobalGovernedExecutionPort`

该类型故意用 `Global` 命名，避免制造“已 owner-scoped”的假象。它自身不持有 authority，
每次调用仍按 `run_id` 进入既有 ambient registry。实现放在独立模块并从 crate root 导出，
供 Step 5 临时组装；后续真正的 `GovernedToolService` 可以独立替换它。

### 2.2 放弃的方案

1. **直接给 `RuntimeHost` 实现 trait**：放弃。当前执行 authority 实际位于 global
   registries；把 trait 绑到某个 host instance 会错误暗示实例隔离，并给后续 adapter
   持有 host/ownership cycle 留下误导。
2. **observer error 先压成 bridge `Gateway`，再普通映射为 `Unavailable`**：放弃。
   这会丢失 port 明确要求的 `LifecycleDeliveryFailed`。实现用调用期 `Cell` 记录原始闭合
   observer error，但 fail-closed 仍由原 free function 在返回前完成。
3. **直接包装 public receipt 或校验失败时 panic**：放弃。opaque outcome 的私有表示不被
   绕过；所有生产 receipt 必经 checked conversion，校验失败穷举映射为内部
   `Unavailable`，不会伪造成功。
4. **给 port 增加 `Other`/字符串逃生口**：放弃。当前生产错误全部可表达，没有 Step 1
   缺口，因此 `governance-port` 未修改。
5. **顺手抽 service、改 free functions 或 adapter wiring**：放弃，均属于 Step 5–10，
   会超出本单且掩盖真实耦合状态。

## 3. Opaque outcome 的 checked conversion

三个返回路径分别调用：

- `GovernedReadOutcome::try_from_receipt`；
- `GovernedWriteOutcome::try_from_receipt`；
- `GovernedProcessOutcome::try_from_receipt`。

runtime-host 无法直接构造 opaque tuple field。四个闭合校验错误
`InvalidDeliveryShape / InvalidLifecycleOrder / InvalidTerminalShape /
ConflictingProviderOutcome` 也使用无 wildcard 的穷举转换；任何不可能的生产 receipt 都
fail closed 为对应 execute error 的 `Unavailable`，不会变成 outcome。

真实 port/bridge 对齐测试为 read、write、process 各覆盖 `Executed` 与 `Replayed` 两个合法
delivery 分支。write/process replay 均断言 observer 计数不增加。

## 4. 错误映射如何穷举

所有转换 `match` 均没有 `_ =>`；新增 bridge/observer/validation error 变体会令编译失败。
单元测试还逐个构造当前每个变体并核对目标值。

### Read

| bridge error | port error |
| --- | --- |
| `RunNotRegistered` | `RunNotBound` |
| `InvalidProposal` | `InvalidProposal` |
| `ReservationConflict` | `ReservationConflict` |
| `InProgress` | `InProgress` |
| `InvalidContext` | `InvalidContext` |
| `Gateway` | `Unavailable` |

### Write

| bridge error | port error |
| --- | --- |
| `RunNotRegistered` | `RunNotBound` |
| `InvalidProposal` | `InvalidProposal` |
| `ReservationConflict` | `ReservationConflict` |
| `Busy` | `Busy` |
| `InvalidContext` | `InvalidContext` |
| `NotLive` | `NotLive` |
| `ShutDown` | `ShuttingDown` |
| `Gateway / OwnershipMismatch / InvalidGrant / DecisionDenied / Expired / Cancelled / Interrupted` | `Unavailable` |
| observer `RunNoLongerLive / DeliveryFailed` | fail-closed 后 `LifecycleDeliveryFailed` |

### Process

| bridge error | port error |
| --- | --- |
| `RunNotRegistered` | `RunNotBound` |
| `InvalidProposal` | `InvalidProposal` |
| `ReservationConflict` | `ReservationConflict` |
| `Busy` | `Busy` |
| `InvalidContext` | `InvalidContext` |
| `NotLive` | `NotLive` |
| `ShutDown` | `ShuttingDown` |
| `CleanupUnproven` | `CleanupUnproven` |
| `Gateway / OwnershipMismatch / DecisionDenied / Expired / Cancelled / Interrupted` | `Unavailable` |
| observer `RunNoLongerLive / DeliveryFailed` | fail-closed 后 `LifecycleDeliveryFailed` |

没有发现 port 错误集表达不了的生产错误。`CleanupUnproven` 在 observer 上下文转换之前
优先保留，绝不降级为 `LifecycleDeliveryFailed` 或 `Unavailable`。

## 5. 五条时序契约

1. **callback 晚于 durable commit**：兼容层不 reserve/commit，也不提前调用 observer；
   只在既有 bridge 的 post-commit callback 内同步转交。
2. **不持治理锁调用 callback**：兼容层没有锁。既有 bridge 在 callback 前 drop
   coordinator guard；完整 bridge 回归继续覆盖 durable barrier 与 observer failure。
3. **replay 不回调**：free functions 在 terminal replay 分支直接返回；新增 write/process
   测试明确断言 replay 后 observer 记录不增加。
4. **取消电平触发**：`&dyn CancellationProbe` 原样传给 free function，没有读一次、缓存或
   转成 bool；Step 3 的三组 barrier 测试随完整 bridge suites 通过。
5. **`CleanupUnproven` 不降级**：process bridge 显式穷举映射；即使 observer 已记录失败，
   `CleanupUnproven` 仍优先返回其独立 port variant。

转换层没有重排、补发或吞掉 lifecycle。它依赖 Step 2/3 已收紧的既有生产路径来实现时序，
而不是用 trait 名称掩盖该事实。

## 6. 测试覆盖与 TDD 证据

先添加三组真实 bridge/port 测试，再加入生产实现。

### RED

定向 read 测试在实现前实际失败：shell exit `1`，唯一原因是
`E0432 unresolved import keeper_runtime_host::GlobalGovernedExecutionPort`。这是预期的
“兼容实现尚不存在”失败，不是测试语法或环境错误。

### GREEN

- read：opaque outcome 保持 Executed/Replayed、durable replay 内容、InvalidProposal 与
  RunNotBound 映射；使用 testkit `FixedCancellationProbe`。
- write：Executed/Replayed、Requested exactly once、replay 零 callback、连续两次
  observer failure 均返回 `LifecycleDeliveryFailed` 且不遗留 Busy；使用 testkit probe 与
  `RecordingWriteObserver`。
- process：Requested→Decided 顺序、reject 无 Started、Requested protocol payload、replay
  零 callback、连续两次 observer failure 释放槽位；使用 testkit probe 与
  `RecordingProcessObserver`。
- unit：三组 bridge error 全变体、四个 outcome validation error、两个 observer error、
  process Requested view→payload 转换。
- 原三组 bridge suites 完整通过：process `26/26`、read `6/6`、write `22/22`。

## 7. 命令与退出码

所有 Cargo 命令均使用 `CARGO_TARGET_DIR=<repo>/target-step4`；没有运行桌面 E2E。

| 命令 | exit | 结果 |
| --- | ---: | --- |
| 定向 read RED test | 1 | 预期 `E0432`，新 impl 尚不存在 |
| 定向 read GREEN test | 0 | `1/1` |
| 定向 write GREEN test | 0 | `1/1` |
| 定向 process GREEN test | 0 | `1/1` |
| `cargo test --locked -p keeper-runtime-host --lib governance_port::tests::` | 0 | 新增 unit `4/4` |
| `cargo test --locked -p keeper-runtime-host --test governed_read_bridge --test governed_write_bridge --test governed_process_bridge` | 0 | `6 + 22 + 26 = 54` passed |
| `cargo test --locked -p keeper-runtime-host --lib` | 0 | `26/26` |
| `cargo fmt -p keeper-runtime-host` | 0 | 只格式化白名单内 package |
| `cargo fmt -p keeper-runtime-host -- --check` | 0 | clean |
| `cargo check --locked -p keeper-runtime-host --all-targets` | 0 | clean |
| `cargo clippy --locked -p keeper-runtime-host --lib --no-deps -- -D warnings` | 0 | 新 production impl clean |
| read/write integration-test clippy（`--no-deps -D warnings`） | 0 | clean |
| process integration-test 诊断 clippy（仅豁免既有 `collapsible_if`） | 0 | 新增部分无其他 warning；非 acceptance gate |
| `cargo clippy --locked -p keeper-runtime-host --all-targets -- -D warnings` | 1 | 并行 `storage-sqlite` 的 `derivable_impls` 首先失败 |
| 同上 `--no-deps` 诊断 | 1 | 既有 `governed_process_bridge.rs:663` `collapsible_if`，不在本单 diff；规定 worker gate 不使用 `--all-targets` |
| `node scripts/run-worker-gate.mjs` | 1 | `BLOCKED-BY-CONCURRENT`；子步骤 `cargo-clippy` exit `101` |

### Worker gate 阶段结果

在失败前已通过：workspace structure、secret scan、全仓 `cargo fmt --check`、TypeScript
typecheck、package tests（前端 `419/419`）、`cargo test --workspace`。规定的
`cargo clippy --workspace -- -D warnings` 在并行 Worker 的
`crates/storage-sqlite/src/knowledge.rs:269` 因 `clippy::derivable_impls` 失败；该路径明确
禁止本单修改，因此未修。gate fail-fast，后续 ACP protocol drift 未执行。

这不是本单代码失败：本单 production lib 的 `-D warnings` clippy 已独立通过。总控需在
并行 Worker 收口后串行重跑唯一 worker gate。

## 8. 写入范围审计

本单实际写入仅有：

- `crates/runtime-host/src/governance_port.rs`；
- `crates/runtime-host/src/lib.rs`；
- `crates/runtime-host/tests/governed_read_bridge.rs`；
- `crates/runtime-host/tests/governed_write_bridge.rs`；
- `crates/runtime-host/tests/governed_process_bridge.rs`；
- 本报告。

`runtime-host/Cargo.toml` 已有 `keeper-governance-port` normal/dev dependency，未修改。
任务开始前 `Cargo.lock` 已有未提交的 Step 3 dependency diff，本单未修改它。

未写 adapter、desktop、storage-sqlite、knowledge-indexer、其他禁止 crate、scripts、
fixtures 或 `.superpowers/`；未执行 `git add/commit/stash/reset/clean/checkout/restore`；
未读取 auth/token 文件。执行期间出现的 desktop、storage-sqlite、knowledge-indexer 与其他
报告改动来自并行 Worker，本单保持不动。

## 9. 为什么仍不能关闭耦合风险

`GlobalGovernedExecutionPort` 只是把稳定 trait surface 放在现有 globals 前面：

```text
adapter（Step 5 后） → GovernedExecutionPort → GlobalGovernedExecutionPort
                                     → execute_governed_* → global registry
```

它没有证明 execution/control 来自同一个 owner-scoped service allocation，也没有原子 bind、
lease、draining/release 生命周期。若在此处宣称解耦完成，只是把 coupling 藏到 global
service locator 后面。只有后续 service/ownership/lease 迁移与 free-function 清尾才能关闭
该风险；提案规定到 Step 10 才可称 implementation complete，Step 11 后才可称 verified。

## 10. 残余风险

1. global registry 的 ambient ownership、split bind/release 与 service-locator 风险原样存在。
2. 五条时序契约仍主要由既有 bridge 实现和 conformance tests 保障，Rust trait 本身不能证明。
3. checked conversion 若发现不可能 receipt，只能在当前闭合 port surface 上返回
   `Unavailable`；本单没有引入新的 audit channel 或开放错误袋。
4. 已知 commit→event sink 非跨存储原子风险不在 Step 4 范围内，未处理。
5. workspace gate 仍需在并行 `storage-sqlite` 收口后重跑；ACP drift 本轮未到达。
6. 额外 `--all-targets` 诊断暴露一处本单 diff 外的既有 process-test lint；规定 worker
   gate 不检查 integration-test targets，但总控若采用更强 lint 门应另行处理，不能夹入本单。

## 11. 完成标记

**DECOUPLE-STEP4-COMPLETE**

workspace gate 附注：**BLOCKED-BY-CONCURRENT** — 并行
`storage-sqlite/src/knowledge.rs` clippy 失败；未修白名单外文件。
