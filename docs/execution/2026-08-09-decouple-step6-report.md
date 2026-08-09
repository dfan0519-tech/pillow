# Pillow — 解耦 Step 6 执行报告

日期：2026-08-09  
状态：`COMPLETE / DECOUPLING_RISK_OPEN`

## 0. 先落骨架的顺序证据

本报告在任何 Step 6 源码、测试、composition 或 Cargo 修改之前创建。创建时的实时基线为：

- 分支 `main`，HEAD `7a00214935f117bdf0d9df4352bbe49c3a20918c`；
- 工作树仅有预存 `Cargo.lock` 修改，本单未认领、未覆盖；
- Step 5 的生产依赖边切断已在 HEAD，但本单没有沿用历史结论，而是在实施后重新生成 normal 依赖图；
- 本轮未执行任何 Git 写操作，也未运行 desktop E2E。

随后才加入 RED 测试、实现 service/composition，最后运行门并回填本报告。

## 1. Service 设计与 owner scoping

### 1.1 `GovernedToolService`

`GovernedToolService` 是 runtime-host 拥有的单个具体 allocation，集中持有：

- 唯一 `owner_id`；
- read/write/process 三个 gateway `Arc`；
- retained database construction identity；
- `AtomicBool` shutdown 状态；
- 只用于 bind/release/owner shutdown 线性化的 lifecycle mutex。

`RuntimeHost` 不再分别持有 owner 与三个 gateway，而是持有
`Arc<GovernedToolService>`。控制侧的 register/release/list/decide/recover
方法转发给该 service；执行侧由同一个 allocation 实现
`GovernedExecutionPort`，再擦除为 `Arc<dyn GovernedExecutionPort>` 交给 adapter。

执行侧先 fail-closed 检查 service 是否关闭、`run_id` 是否属于该 owner，之后才进入
Step 4 的 `GlobalGovernedExecutionPort` 兼容实现。free functions 与 globals 均保留，未提前执行
Step 10 清尾。

### 1.2 prepared/factory composition

新增 `PreparedRuntimeHost` 与 `GovernedAdapterFactory`：

1. `PreparedRuntimeHost::open` 先打开 owner-scoped service；
2. `governed_execution_port` 从该 service 的具体 `Arc` clone 并擦除类型；
3. factory 用该 execution port 创建 adapter；
4. `finish` 把 adapter 与原 concrete service `Arc` 一起装进 host；
5. factory 返回错误时，prepared owner 的 `Drop` 先 shutdown service；即使 factory 留下了
   execution-port clone，该 clone 也只能收到 unavailable/shutting-down，不能继续执行。

desktop 的 Grok 生产 composition 已改走
`ConversationService::open_with_adapter_factory`；Step 5 的临时
`GlobalGovernedExecutionPort` handle 不再由 desktop 生产组装创建。adapter crate 仍只在
`dev-dependencies` 中引用 runtime-host 来承载跨 crate roundtrip 测试。

### 1.3 同一 `Arc`、持有与释放

- host 持有 concrete `Arc<GovernedToolService>`；adapter 持有从它 clone 后擦除的 trait-object
  `Arc`，pointer-identity 测试证明二者指向同一 allocation；
- service 不持有 adapter，因此 `host -> adapter -> service` 与 `host -> service` 不构成环；
- 显式 shutdown 的顺序保持为：关闭同步 admission → 发布 host shutdown/取消 worker →
  等同步调用退出 → `adapter.shutdown()` → join workers → 检查 residual →
  `GovernedToolService::shutdown()` 释放 owner bindings；
- adapter shutdown 失败时也会先完成 join、residual 检查与 service shutdown，再向调用方返回错误；
- 若跳过显式 shutdown，`RuntimeHost::Drop` 仍会幂等关闭 service；最后一个 service `Arc`
  被释放时，service 自身 `Drop` 再做幂等兜底。

## 2. Ownership / shutdown / composition 测试

TDD 的有效 RED 为：

```text
cargo test -p keeper-runtime-host --test governed_tool_service
error[E0432]: unresolved import `keeper_runtime_host::PreparedRuntimeHost`
exit code: 101
```

实现后的同一命令为 5/5 PASS，覆盖：

1. `prepared_host_gives_adapter_and_host_the_same_service_allocation`：比较擦除前后的 allocation pointer；
2. `service_execution_rejects_a_run_bound_by_another_owner`：owner B 不能借 ambient registry
   执行 owner A 的 run；
3. `failed_adapter_factory_closes_the_prepared_service_even_if_a_port_clone_escaped`：factory
   失败后的逸出 clone 只能返回 unavailable；
4. `host_shutdown_stops_adapter_before_releasing_and_closing_governed_service`：adapter shutdown
   期间 service 仍 open；host 返回前 execution 已关闭且 owner binding 已释放；
5. `dropping_host_without_explicit_shutdown_still_closes_the_shared_service`：Drop 兜底关闭。

desktop composition 另有 1/1 PASS 的 pointer-identity 测试；真实 adapter 的 read/write/process
roundtrip 全部改用 prepared composition，并随 adapter 全套测试通过。

## 3. 五条治理时序契约

1. **callback 晚于 durable commit**：service 不重写执行流程，只在进入原 Step 4 兼容实现前做
   shutdown/owner 检查；durable commit 与 observer/callback 的既有先后关系未移动。runtime-host
   bridge 与真实 adapter roundtrip 均通过。
2. **不持锁调用 callback/sink**：owner 查询取得 registry lock 后立即返回 bool；service 的 lifecycle
   mutex 只包围 bind/release/shutdown，不跨越 `execute_*`，因此不带锁进入 observer/sink。
   Step 4 已有的回调前释放 coordinator lock 行为保持不变。
3. **replay 不回调、不重复 emit**：service 仍将 replay 交给原 bridge；read/process adapter
   roundtrip 对执行次数、durable identity 与 normalized lifecycle 数量的断言原样通过。
4. **取消保持 level-triggered**：`&dyn CancellationProbe` 原样逐层借用转发，没有拍成一次性 bool
   或缓存快照；runtime-host 与 adapter 全套 cancellation 测试通过。
5. **`CleanupUnproven` 不降级**：service 不映射、不吞并 execution outcome；它转发到 Step 4
   exhaustive mapping，既有 H5 cleanup/fail-closed 测试通过，未修改任何期望值。

## 4. adapter normal dependency 边未回退

命令（`CARGO_TARGET_DIR=<repo>/target-step6`）：

```text
cargo tree -p keeper-adapter-grok-acp -e normal --depth 1
```

除把仓库前缀确定性替换为 `<repo>` 外，原始 stdout 为：

```text
keeper-adapter-grok-acp v0.1.0 (<repo>\crates\adapter-grok-acp)
├── chrono v0.4.45
├── keeper-agent-protocol v0.1.0 (<repo>\crates\agent-protocol)
├── keeper-governance-port v0.1.0 (<repo>\crates\governance-port)
├── keeper-process-executor v0.1.0 (<repo>\crates\process-executor)
├── serde v1.0.229
├── serde_json v1.0.151
├── thiserror v2.0.19
├── uuid v1.24.0
└── windows-sys v0.59.0
NORMAL_RUNTIME_HOST_MATCHES=0
STEP6_NORMAL_TREE_EXIT_CODE=0
```

结论：`keeper-runtime-host` 在 adapter normal 图中仍为 **0 次**；Step 5 成果未回退。

## 5. 验证命令与 exit code

所有 Cargo 命令均使用 `CARGO_TARGET_DIR=<repo>/target-step6`。

| 命令 | exit code | 原始结果摘要 |
|---|---:|---|
| `cargo test -p keeper-runtime-host --test governed_tool_service`（RED） | 101 | 预期缺少 `PreparedRuntimeHost` 导出 |
| 同一 focused test（GREEN） | 0 | 5 passed，0 failed |
| `cargo test -p keeper-adapter-grok-acp --no-run`（第一次） | 101 | 新测试 composition 的类型导入缺失；修正实现/测试后重跑 |
| 同一 adapter compile-only 命令（最终） | 0 | 9 个 test targets 编译成功 |
| `cargo test -p keeper-adapter-grok-acp` | 0 | lib 60、fake child 19、process 8、read 3、write 4、H5 6、live-native 4 passed/1 ignored、protocol 3；0 failed |
| `cargo test -p keeper-runtime-host` | 0 | 全部 targets 0 failed；新增 service 5/5 passed |
| desktop factory 单测 | 0 | 1 passed，0 failed |
| `cargo test -p pillow-desktop --lib` | 0 | 147 passed，0 failed |
| normal dependency graph | 0 | `NORMAL_RUNTIME_HOST_MATCHES=0` |
| scoped `rustfmt` / `cargo fmt --all -- --check` | 0 | formatting clean |
| focused clippy（诊断轮） | 101 | 新 lint 修正后暴露原测试 nested-if lint；仅机械合并条件，未改断言/期望 |
| `cargo clippy -p keeper-runtime-host --all-targets -- -D warnings`（最终） | 0 | 0 warnings |
| `node scripts/run-worker-gate.mjs`（第一次） | 101 | cargo-clippy 捕获 `prepared.rs` 的 `let_and_return`；修正后重跑完整门 |
| `node scripts/run-worker-gate.mjs`（最终） | 0 | `PILLOW_WORKER_GATE: OK`；346.1s |
| `pnpm --filter @pillow/desktop tauri build --no-bundle` | 0 | release build 完成；frontend 94 modules |
| official dev smoke（cold / warm 两次诊断） | 1 / 1 | cold 首次在 60s readiness 前仍在编译；warm 已到 readiness，但两次 finally 均被清理器对 Windows `cmd.exe` launcher 的归属拒绝所覆盖，故不记 PASS |
| target-step6 release 直启 smoke（复用官方 process/readiness/profile helpers） | 0 | readiness、临时 DB、临时 WebView profile、2500ms 启动预算均通过 |
| 最终 scoped diff/secrets/process audit | 0 | 见第 6 节 |

release 直启 smoke 的原始摘要：

```text
KEEPER_DESKTOP_SMOKE_DIRECT {"databaseBytes":385024,"mode":"release-direct-target-step6","observedReadyMs":2100,"readiness":"list-projects-and-lifecycle-command","startupBudgetMs":2500,"startupMs":2073,"webviewProfile":"temporary-and-populated","webviewProfileEntries":241}
```

它直接启动本单隔离 target 中的 release executable，并复用官方 smoke 的
`ProcessSupervisor`、baseline、readiness validator、profile snapshot 与 MSVC environment
helpers；它不是 desktop E2E。验证后目标进程数为 0，1420 端口 owner 数为 0。

按简报，**desktop E2E 未运行**。live Grok provider test 也未运行（全套测试中 1 项保持 ignored）。

## 6. 写入边界与最终审计

本单实际写入：

- `crates/runtime-host/src/`：新增 `governed_tool_service.rs`、`host/prepared.rs`；修改
  `governance_port.rs`、`governed_read.rs`、`governed_write.rs`、`host/mod.rs`、
  `host/process.rs`、`lib.rs`；
- `crates/runtime-host/tests/`：新增 `governed_tool_service.rs`；对
  `governed_process_bridge.rs` 做 clippy 要求的等价条件折叠，未改变断言或期望；
- `crates/adapter-grok-acp/tests/`：三个 governed roundtrip 改走 prepared composition；
- `apps/desktop/src-tauri/src/`：仅修改 production composition、conversation factory 与对应单测；
- 本报告。

`runtime_composition.rs` 的 diff 只有 import 与 `open_with_grok_adapter` composition 两处；
路径校验的“确定不存在 / 正常 / 查不出来”三分逻辑、重试和 `io::ErrorKind` 分支均未修改。

明确未触碰或未认领：

- 预存 `Cargo.lock` diff（本单没有 Cargo manifest 调整）；
- `crates/governance-port/src/**`；
- `crates/storage-sqlite/**`、`crates/knowledge-indexer/**`；
- `crates/kernel-project/**`、`crates/git-executor/**`、`crates/kernel-resource/**`；
- `apps/desktop/src/**`、`scripts/**`、`tests/fixtures/**`；
- `.superpowers/**`、认证/token、Keeper/Vault 材料；
- 本报告以外的 `docs/**`。审计期间出现的两个并行 untracked research 文档不是本单产物，未读取、未修改；
- staging、commit、stash、reset、clean、restore。

最终 staged 文件为 0。`git diff --check`、secrets scan、白名单审计均通过。

## 7. 为什么 Step 6 仍不等于解耦完成

本步只建立 owner-scoped service allocation 与 prepared/factory composition，并让 host/adapter
共享同一个 `Arc`。read/write/process coordinator state 仍在 globals 中，service execution
仍经 free-function compatibility wrapper。

Step 7–9 才迁移三类全局 state；Step 10 才加入单一原子 bind、不可复制的
`GovernedRunLease`、generation 与完整生命周期状态机，并删除 globals/free-function 兼容层。
因此本轮只能判定“Step 6 自身完成”，**不能**表述为“解耦完成”或风险关闭。

## 8. 残余风险与最终判定

- **OPEN — ambient state**：三个 coordinator/global registry 与 free functions 仍存在；
- **OPEN — bind/lease race**：owner pre-check 与后续 compatibility lookup 是两次访问；缺少
  generation 和不可复制 lease，TOCTOU/stale-run 风险要到 Step 10 才真正收口；
- **OPEN — split registration rollback**：read/write/process 仍是分步 bind/release，尚无单一
  all-tools 原子 bind；
- **OPEN — desktop coverage**：release 直启 smoke 已过，但按要求未跑 desktop E2E，且未跑真实
  Grok provider canary；
- **ENV RESIDUAL**：两次失败 dev-smoke 的进程树与 1420 端口均已按 launcher lineage 验证清零；
  两个隔离的 failed-dev smoke 临时目录仍留在系统临时区，因为删除请求被执行策略拒绝。

最终判定：Step 6 的 service、owner scoping、prepared/factory composition、ownership/shutdown
测试、normal dependency 证据与 worker gate 均完成；总体解耦风险保持 `OPEN`。

`DECOUPLE-STEP6-COMPLETE`
