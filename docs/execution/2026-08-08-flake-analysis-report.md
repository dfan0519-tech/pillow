# Pillow 四个并发敏感 flake 综合报告

日期：2026-08-08

分析范围：`<repo>` 当前工作树
执行约束：只读源码调查。未运行 `cargo`、`pnpm`、测试、构建或 E2E。除本报告与逐项 verdict 外，未修改任何文件。

## 1. 结论摘要

§3 的共性假说需要拆成强弱两个版本：

- **弱版本成立**：四项都依赖并发 actor，且负载会扩大未握手的观察窗口或改变竞态排序
- **强版本推翻**：没有证据表明四项由同一个全局 static、同一 SQLite 文件、同一 test hook 或同一后台线程泄漏造成

共性机制不是“共享数据被串改”，而是**测试或 harness 用时间窗口代替确定性阶段握手**：

- `cancel-tree` 在必须尽快取消的窗口内做无界同步 WMI 扫描
- `two-project-isolation` 依赖一次性 renderer event，但 listener 没有 ready/catch-up 合约
- storage test 允许 list 与 waiter 两个 expiry 发起者同时进入持久化 transition，却断言全局 delta 恰好为 1
- slow-output test 未等 claim/spawn/reader 阶段，从 decide thread 刚创建就消耗约 1 秒预算

因此不存在可信的“一处代码修改同时解掉四项”。可以统一治理测试模式，但必须分别修四个责任边界。

## 2. 四项机制与修复边界

| Flake | 机制判断 | 置信度 | 并行的作用 | 首选修复边界 |
|---|---|---:|---|---|
| `cancel-tree` | 取消前无超时同步 CIM/WMI 扫描可跨过产品 60 秒时限 | 中 | 放大 WMI 和调度长尾 | E2E harness 与进程观测基建 |
| `two-project-isolation` | 一次性 runtime event 可能在 listener-ready 或 ownership gate 处丢失，之后无 catch-up | 中；历史归因低 | 扩大 listener/selection/list 的排序窗口 | renderer subscription-ready 与 durable catch-up |
| `storage_failure...` | 同一测试的 list 与 waiter 对同一过期 proposal 各做一次 transition | 高 | 改变内部竞态排序，不是跨测试计数污染 | 先明确契约，再改测试协调或 proposal single-flight |
| `slow_output...` | 从 Started 前起算的约 1 秒轮询不足，且没有最终重检 | 高 | 增加 process spawn、reader 和 coordinator 延迟 | integration test 的阶段握手与失败清理 |

逐项完整证据、可证伪预测和不确定项见 `docs/execution/2026-08-08-flake-analysis-VERDICT.md`。

## 3. 共性假说判定

### 3.1 成立的部分：并行是统一放大器

四项都不是“并行创造了不存在的状态”。并行改变的是时间与排序：

- Cargo test 默认在同一 binary 内并发运行 tests。process coordinator 是一个 binary 共享的 `OnceLock<Mutex<...>>`：`crates/runtime-host/src/host/process.rs:256-300`
- slow-output 同 binary 的 cache test 会启动 33 个 Node process：`crates/runtime-host/tests/governed_process_bridge.rs:663-689`
- write coordinator 也是一个 binary 共享的 `OnceLock`：`crates/runtime-host/src/governed_write.rs:169-198`。它能增加锁竞争，但 owner、run 与 gateway 仍隔离
- repo 的 Cargo wrapper 每 500 ms 调用一次 process supervisor capture：`scripts/run-cargo.mjs:40-49`
- E2E 外层 supervisor 同样每 500 ms capture：`scripts/lib/desktop-process.mjs:755-798`
- Windows capture 最终同步执行无 timeout 的 `Get-CimInstance Win32_Process`：`scripts/lib/desktop-process.mjs:138-154`

这些 WMI capture 运行在父进程，通常不会直接阻塞 Cargo test binary 或内层 E2E driver，但会争用 CPU、PowerShell/WMI 与系统进程枚举资源。它是可信的系统级负载放大器，不是四项共同的状态根因。

四项在串行或隔离条件下仍可被确定性触发：

- 在 cancel 关键窗口阻塞驱动超过 60 秒
- 让 requested event 落在 listener 注册前且让首次 list 早于 reserve
- 用 barrier 让 storage 的两个发起者同时越过 expiry 判断
- 延迟 slow-output 的 claim/spawn/first-chunk 超过旧轮询预算

这组预测说明“只在并行出现”不是机制定义，而是现有机器上进入窗口的概率条件。

### 3.2 推翻的部分：没有一个共享全局污染源

#### 写入计数器不是全局计数器

`write_attempts` 和 `fail_writes` 都在 `SqliteWriteProposalRepository` 实例中，`open` 时从 0/false 初始化：`crates/storage-sqlite/src/write_proposal.rs:175-198`。其他 test fixture 没有正常路径取得该实例。

#### SQLite 与临时目录没有碰撞证据

- governed-write fixture 每测创建独立 `tempfile::tempdir()`，DB 虽都叫 `pillow.db`，父路径不同：`crates/runtime-host/tests/governed_write_bridge.rs:70-82`
- governed-process fixture 同样独立：`crates/runtime-host/tests/governed_process_bridge.rs:73-115`
- `two-project-isolation` 使用独立 `app-data-iso`、`webview-iso` 与 `grok-iso`：`scripts/wave-c4-governed-process-acceptance.mjs:1999-2044`

#### `test-support` hook 不是本组共同污染源

`test-support` 通过 runtime-host self dev-dependency 对 integration tests 可见：`crates/runtime-host/Cargo.toml:8-12,29-46`。`PRE_CLAIM_HOOK` 的确是 process module 全局 one-shot：`crates/runtime-host/src/host/process.rs:1699-1730`，但仓库内没有安装它的 test call site；它也不能进入 write repository。`force_live_*_expiry_for_tests` 由 owner 与 proposal id 限定：`crates/runtime-host/src/host/process.rs:1682-1697`、`crates/runtime-host/src/governed_write.rs:1418-1431`。

#### 后台线程不是一个共同泄漏

- storage waiter 在 counter 断言时仍活跃，这是本测试设计内的直接竞争者，不是前一测试泄漏：`crates/runtime-host/tests/governed_write_bridge.rs:1022-1068`
- slow-output 的 waiter 与 decide 也在 live 断言时按设计并发：`crates/runtime-host/tests/governed_process_bridge.rs:605-641`
- 两项 Rust test 若在 join 前 panic，线程会短暂存活并加重后续负载；这属于失败后的卫生问题，不能解释首次失败
- E2E 正常与异常路径都有 supervisor cleanup。当前证据没有显示上一 desktop session 直接把 proposal 或 run 注入下一 session

### 3.3 一次修复能否同时解决多个

**不能。** 最多可以共享工程策略：

1. 用 phase-relative condition 或 explicit ready 取代固定 sleep 和一次性 event 假设
2. 所有诊断输出记录非敏感 phase、elapsed、state、count 与 thread role
3. 失败路径先释放 child、join worker，再断言或返回
4. 系统枚举设置硬截止、禁止 overlap，并保留身份不明时 fail-closed 的清理语义

这些策略不能替代四处独立补丁。特别是：

- 限制 WMI 不会修 renderer lost-wakeup、storage 双 transition 或 slow-output 起算点
- subscription-ready 不会修 cancel 或 Rust tests
- storage single-flight 不会修 process coordinator 或 E2E
- 增加 slow-output deadline 不会修其他三项，也不会修正该测试“并未证明 terminal 前 live”的语义

## 4. 给总控的实验清单

以下命令均未由本任务执行。涉及临时诊断代码时，应只记录布尔值、计数、阶段、elapsed 和 thread role；不要写 proposal、conversation、run id、grant、命令正文或路径。

### 实验 A：闭环 `cancel-tree` 的 WMI 长尾

先在 `snapshotWindowsProcessesWithCommandLine` 与 cancel timeline 中临时记录每次调用的 `startedAt`、`elapsedMs`、phase，以及 `msApproveToCancelStart`。保留现有 RED dump，然后运行：

```powershell
corepack pnpm --filter @pillow/desktop exec node --test --test-concurrency=1 tests/e2e/c4-governed-process.spec.ts
```

预期结果：RED 的 `after-approve` 显示 cancel 可见；某次 marker CIM 调用或批准到取消入口耗时接近/超过 60,000 ms；驱动恢复时 run 已 terminal、cancel count 为 0。

证伪结果：所有批准后 CIM 调用和 `msApproveToCancelStart` 都少于约 5 秒，run 仍 active、conversation ownership 正确，但 cancel count 已为 0。

决定性注入：在临时 test-only seam 中，于批准后、取消前阻塞内层 driver 超过 60 秒，但不暂停 desktop process。使用同一命令。若不能稳定产生同型 terminal 后按钮缺席，则当前机制被证伪。

### 实验 B：闭环 `two-project-isolation` 的 listener/catch-up

先新增三个确定性 renderer case：延迟 `listen()` resolve；让 requested event 发生在 ready 前；让首次 list 在 reserve 前返回空。运行：

```powershell
corepack pnpm --filter @pillow/desktop exec vitest run tests/tauri-conversation-gateway.test.ts tests/c4-app-process-refresh.test.tsx tests/c4-workbench-process-integration.test.tsx
```

预期结果：当前实现应留下 durable pending 且没有 refresh/bar；加入 listener-ready 与一次同-scope durable catch-up 后用例转绿。foreign-project event 仍不得显示。

证伪结果：当前实现即使故意丢掉 requested event，也会通过现有路径重新 list 并显示 bar；或失败样本中的 native list 本身为空。

真实桌面 RED 需要增加以下非敏感字段：phase、listenerReady、requestedSeen、ownershipMatched、refreshDelta、nativePendingCount。随后运行：

```powershell
corepack pnpm --filter @pillow/desktop exec node --test --test-concurrency=1 tests/e2e/c4-governed-process.spec.ts
```

若 listener 已 ready、event 已收到、refresh 已递增且 native list 有 pending，但 DOM 仍没有 bar，则转查 list error、DTO normalization 与 arbitration。

### 实验 C：决定性验证 storage 双 transition

在 `read_durable_terminal_after_transition` 前加仅测试可见的 proposal-scoped barrier，并给 mutation trace 标注 `main-list` 或 `waiter-expiry`。运行一次 exact serial：

```powershell
cargo test -p keeper-runtime-host --features test-support --test governed_write_bridge storage_failure_after_live_never_fabricates_terminal -- --exact --nocapture --test-threads=1
```

预期结果：同一 proposal 出现两条 `transition` attempt，counter delta 为 2。

证伪结果：第二笔来自不同 mutation/proposal，或 barrier 已证明两条路径都越过 expiry 判断后仍只有一笔 attempt。

再做反向控制：用 channel 暂停 waiter，直到主线程完成失败 list、counter 断言和 quarantine 后再释放。使用同一命令。

预期结果：delta 恰好为 1，waiter 返回 opaque `Gateway`。

证伪结果：暂停 waiter 后仍为 delta 2，说明主线程链内部有重试或第三写入口。

非决定性的概率对照可用于估计窗口，但不能代替 barrier：

```powershell
1..100 | ForEach-Object {
  cargo test -p keeper-runtime-host --features test-support --test governed_write_bridge storage_failure_after_live_never_fabricates_terminal -- --exact --test-threads=1
  if ($LASTEXITCODE -ne 0) { break }
}
```

exact serial 复现一次即可推翻“必须由其他测试污染”。连续全绿不能推翻内部竞态。

### 实验 D：验证 slow-output 的错误起算点

先给测试记录 `pendingSeen`、`approveCommitted`、`claimed`、`childResumed`、`firstChunkAppended`、每次 open 的 elapsed，以及实际 panic 行。隔离基线：

```powershell
1..30 | ForEach-Object {
  node scripts/run-cargo.mjs test -p keeper-runtime-host --test governed_process_bridge slow_output_is_visible_live_and_reopens_after_waiter_consumes_terminal -- --exact --test-threads=1
  if ($LASTEXITCODE -ne 0) { throw "isolated iteration $_ failed" }
}
```

同 binary 并行压力：

```powershell
1..10 | ForEach-Object {
  node scripts/run-cargo.mjs test -p keeper-runtime-host --test governed_process_bridge -- --test-threads=16 --nocapture
  if ($LASTEXITCODE -ne 0) { throw "parallel iteration $_ failed" }
}
```

预期结果：失败样本的 `firstChunkAppended` 晚于旧的第 100 次 open；并行失败率高于 exact serial。

证伪结果：chunk 已在旧预算内 append，但所有后续 open 都看不到；或最终 receipt/cache 也没有 marker。前者转查 host projection，后者转查 pipe capture。

决定性 regression 应把 child 改为：写 marker，等待 fixture 内唯一 sentinel，再退出。测试先等 `Started`，再 condition-wait marker，确认当时 output state 非 terminal，最后创建 sentinel 并 join。若该版本在同负载仍丢 marker，则“预算不足”被证伪。

### 实验 E：区分共同负载放大器与根因

临时为 `run-cargo.mjs` 和 `runSupervisedCommand` 增加仅诊断用开关，关闭 500 ms background process capture；不得关闭启动前/退出后的安全快照。分别重复实验 A、C、D。

预期结果：关闭 background capture 可能降低 flake 率或缩短 WMI/process 启动长尾，但 storage barrier 仍稳定得到 delta 2；slow-output 的确定性 pre-chunk delay仍可击穿旧预算。

证伪结果：若关闭 capture 能让 storage 的双 barrier 只产生一次 transition，说明当前静态推理遗漏了跨层共享状态；应停止现有修复方案并按 trace 重新追踪。

## 5. 修复建议与风险

### 5.1 `cancel-tree`

建议先改 `scripts/wave-c4-governed-process-acceptance.mjs:1307-1339`：让 fake ACP 或 child 发确定性 ready 信号，取消动作不再等待 WMI 枚举。所有 PowerShell/CIM 快照都应有硬截止与 no-overlap；观测失败必须报告 unavailable，不能伪装为“没有进程”。

风险：Pillow 的进程清理规则要求身份不明时 fail-closed。不能用按名称或裸 PID 的宽松清理替代 WMI 身份证明，也不能为了稳定测试跳过最终残留检查。

### 5.2 `two-project-isolation`

建议在 `apps/desktop/src/lib/tauriConversationGateway.ts:1106-1133` 建立 subscription-ready/error contract，并让 shell 首次 send 等待 ready。ready 后对当前 scope 做一次 durable re-list，或建立 event cursor catch-up。不要用无界 interval polling 替代事件契约。

风险：改变 `ConversationGateway.subscribe` 会波及 mock gateways 与多组 renderer tests。listener 失败不能静默，也不能无限阻塞 UI；应显示固定公共错误并允许安全重试。仅加 startup ready 不能处理运行期 emit failure，故仍需一次 durable catch-up 设计。

### 5.3 storage failure

先由总控明确“恰好一次”的契约：

- 若只要求单个 reconciliation 调用不内部 retry，修测试：用 channel 暂停 waiter，使 counter 断言只观察主线程调用；另留独立测试覆盖 waiter failure
- 若要求同一 proposal 的所有并发 expiry 发起者全局只有一次 store attempt，改 `crates/runtime-host/src/governed_write.rs:817-874`，增加 owner/proposal-scoped single-flight/finalizing 仲裁

风险：产品 single-flight 会影响 list、waiter、cancel、interrupt 与 decision 的交互。winner 写失败后如何通知、quarantine 和清理必须有专门测试。不能把断言放宽成 `+1 | +2`，那会隐藏真正的内部 retry。

### 5.4 slow-output

首选只改 `crates/runtime-host/tests/governed_process_bridge.rs:599-659`：用 lifecycle channel、test-controlled sentinel、`Instant` condition wait 和 deadline 边界最终重检。无论断言是否成立，都先释放 child、join waiter/decide、shutdown，再报告失败。

风险：`Started` 发生在 durable claim 后、runner spawn 前：`crates/runtime-host/src/host/process.rs:1474-1499`。只等待 Started 仍不够，必须等待真实 marker。仅把 1 秒调大只能降低概率，也仍不能证明 marker 在 terminal 前可见。

### 5.5 共享进程观测基建

可以独立硬化 `scripts/lib/desktop-process.mjs:138-154,755-798`：给 snapshot 子进程硬截止、串行化 capture、跳过 overlap、记录耗时分位。该工作能降低所有重负载 gate 的噪声，但不能作为四项根因修复的替代品。

风险：snapshot timeout 后必须保留“不知道就拒绝清理”的语义。不得基于 numeric PID 猜测进程身份。

## 6. 明确的不确定项

1. `cancel-tree` 没有真实 RED 的 CIM elapsed、批准到取消耗时或 terminal 状态
2. `two-project-isolation` 的失败 phase、native pending list 与 listener/token 状态未知
3. storage 缺少 thread-tagged mutation trace；第二笔来自 waiter 是高置信静态归因，尚非动态观测
4. storage 的“恰好一次”是 per-call 还是 per-proposal 契约尚未明确
5. slow-output 缺少首次 chunk、各阶段耗时与确切 panic 行
6. process coordinator 锁、WMI wrapper、杀毒软件和 Windows process/thread 枚举各自贡献的延迟比例未测量
7. 本任务遵守禁止条款，没有运行任何验证命令；所有实验结果均待总控执行

## 7. 建议执行顺序

1. 先做 storage barrier/channel 两个决定性实验，确认契约后修复
2. 再把 slow-output 改为 sentinel + condition handshake，并补失败路径 join
3. 为 cancel 加 CIM elapsed 与 deterministic post-approve delay seam，拿到闭环后移除取消关键窗口内的 WMI
4. 为 two-project 加 listener/native-list/token 诊断，再决定是 startup ready、durable catch-up，还是上游 reserve/pump 修复
5. 最后独立硬化 process snapshot 基建，并跑 controller 全量门禁
