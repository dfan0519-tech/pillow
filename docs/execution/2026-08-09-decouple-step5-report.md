# Pillow — 解耦 Step 5 执行报告

日期：2026-08-09  
状态：`STEP5_COMPLETE / DECOUPLING_RISK_OPEN`  
执行范围：adapter 构造器/调用链注入，切断 `keeper-adapter-grok-acp` 对
`keeper-runtime-host` 的 normal dependency；不改变对外行为。

## 0. 先落骨架的顺序证据

本文件在任何 Step 5 源码、测试 wiring 或 Cargo 依赖修改之前创建。创建时的工作树基线：

- 分支为 `main`；
- `Cargo.lock` 已有预存修改，本单不认领、不覆盖；
- `apps/desktop/src-tauri/src/runtime_composition.rs` 在该快照下没有未提交差异；
- 此后才开始调用点盘点、依赖图 RED、源码与测试 wiring 修改。

实施期间并行 Worker 又写入 `crates/knowledge-indexer/**`、
`docs/execution/2026-08-09-k1b-handoff-render-report.md` 与
`docs/research/2026-08-09-handoff-renderability-matrix.md`。本单没有编辑、格式化或认领这些内容。

## 1. 注入设计

### 1.1 adapter 持有 execution port

`GrokAcpAdapter` 新增一个私有字段：

```rust
governance: Arc<dyn GovernedExecutionPort>
```

构造器变为 `GrokAcpAdapter::new(config, governance)`。配置仍先执行原有
`validate()`；port 只提供 `run_id + normalized proposal + live cancel/borrowed observer`，
没有 project/root/capability/grant/list/decide 等 host authority 参数。

`send_turn` 把字段借用为 `&dyn GovernedExecutionPort`，沿以下同步调用链传递同一个 handle：

```text
send_turn
  → run_prompt_loop
  → handle_incoming
  → handle_server_request
  → execute_read / handle_write_tool / handle_process_tool
```

adapter 不做 service lookup，也不持有 `RuntimeHost`。port 返回 opaque checked outcome，
adapter 只通过 `CheckedGovernedOutcome::into_receipt` 取回原协议 receipt。

### 1.2 provider 行为保持

- read/write/process 的 method、参数规范化、provider JSON-RPC code/message 均未改变；
- port errors 在 adapter 内无 wildcard 穷举映射；原 bridge 的 invalid/conflict/busy/unavailable
  表面保持原值；
- process `Requested` 已由 Step 4 compatibility impl 转成 protocol payload，adapter 删除了
  host-only view/risk 的逐字段转换；
- receipt 后的 read events、write Resolved、process Decided/Started/Terminal 及 provider reply
  顺序没有重排；
- runtime-host free functions 没有删除或修改。

`crates/adapter-grok-acp/src/**` 已无 `keeper_runtime_host` import、bridge error 或
`execute_governed_*` free-function 引用。

## 2. 17 个调用点与测试 wiring 同步迁移

最终机械计数：`CALL_COUNT=17`。

| 调用点文件 | 数量 | 注入 wiring |
|---|---:|---|
| `tests/adapter_fake_child.rs` | 4 | `PanicGovernedExecutionPort`；这些脚本不得进入治理工具 |
| `tests/h5_runtime_fake_child.rs` | 6 | `PanicGovernedExecutionPort`；H5 lifecycle/cancel 路径不得进入治理工具 |
| `tests/live_native_acceptance.rs` | 1 | 显式 `UnavailableGovernedExecutionPort`；真实 provider 若违反 text-only prompt 请求工具则 fail closed |
| `tests/governed_read_roundtrip.rs` | 3 | 真实 `GlobalGovernedExecutionPort` compatibility handle |
| `tests/governed_write_roundtrip.rs` | 1 | 真实 `GlobalGovernedExecutionPort` compatibility handle |
| `tests/governed_process_roundtrip.rs` | 1 | 真实 `GlobalGovernedExecutionPort` compatibility handle |
| `apps/desktop/src-tauri/src/runtime_composition.rs` | 1 | desktop 临时 `GlobalGovernedExecutionPort` handle |
| **总计** | **17** | 16 个 adapter tests + 1 个 desktop composition |

`cargo test -p keeper-adapter-grok-acp --no-run` 在同一工作树中编译了所有 unit、fake-child、
三组 roundtrip、live-native 与 protocol fixture targets，exit code 0；没有留下破碎调用点。

## 3. desktop 临时 handle 组装

`open_with_grok_adapter` 在构造 adapter 前临时创建：

```rust
let governance = Arc::new(GlobalGovernedExecutionPort::new());
let adapter = GrokAcpAdapter::new(config, governance)?;
```

随后仍按原路径把 adapter 交给 `ConversationService::open_with_adapter`；runtime selection、
Grok 路径校验三分支（确定不存在 / 正常 / 查不出来）、degraded/fail-closed 行为均未改动。

此零状态 handle 是 **Step 5 过渡形态**。它内部仍转发到 runtime-host globals；Step 6 才引入
owner-scoped `GovernedToolService`，不得把这里描述成最终 ownership 设计。

## 4. normal dependency graph 机械证据

先执行的 RED 断言在完整 normal tree 中命中 `keeper-runtime-host`，按预期 exit code 1。
实施后的 GREEN 命令扫描完整 normal tree；为遵守报告不得粘贴机器绝对路径的要求，只打印
first-party 节点，并在输出前把当前仓库前缀替换为 `<repo>`。匹配计数仍针对未过滤的完整 tree：

```powershell
$env:CARGO_TARGET_DIR = "<repo>/target-step5"
$tree = cargo tree -p keeper-adapter-grok-acp -e normal --prefix none
if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }
$tree |
  Where-Object { $_ -match '^keeper-' } |
  ForEach-Object { $_ -replace [regex]::Escape((Get-Location).Path), '<repo>' }
$runtimeHostHits = @($tree | Select-String -SimpleMatch 'keeper-runtime-host')
"NORMAL_RUNTIME_HOST_MATCHES=$($runtimeHostHits.Count)"
if ($runtimeHostHits.Count -ne 0) { exit 1 }
```

原始 stdout：

```text
keeper-adapter-grok-acp v0.1.0 (<repo>\crates\adapter-grok-acp)
keeper-agent-protocol v0.1.0 (<repo>\crates\agent-protocol)
keeper-policy v0.1.0 (<repo>\crates\kernel-policy)
keeper-governance-port v0.1.0 (<repo>\crates\governance-port)
keeper-agent-protocol v0.1.0 (<repo>\crates\agent-protocol) (*)
keeper-policy v0.1.0 (<repo>\crates\kernel-policy) (*)
keeper-process-executor v0.1.0 (<repo>\crates\process-executor)
NORMAL_RUNTIME_HOST_MATCHES=0
```

exit code：`0`。`keeper-runtime-host` 已从 adapter `[dependencies]` 移到
`[dev-dependencies]`，仅供三组真实 roundtrip 使用。

## 5. 五条治理时序契约

1. **callback 晚于 durable commit**：adapter 不自行决定 callback 时点；Step 4 port impl 仍委托
   已有 host reserve/decision/claim durable barriers。本次 observer 只在 port 调用它时发事件。
   write/process roundtrip 均通过。
2. **不持锁调用 callback/sink**：进入 `execute_*` 前，adapter 用于获取 active run/rpc 的
   `Inner` guard 已离开作用域。observer 只短暂持 `Inner` lock 校验 live run 并分配 sequence，
   随即释放，再构造事件并调用 sink。runtime-host coordinator lock 规则未改。
3. **replay 不回调、不重复 emit**：port contract/compatibility impl 继续保证 replay 零 callback；
   adapter 仍只在 `delivery == Executed` 时发 read lifecycle、write Resolved 与 process lifecycle。
   read replay、process replay/exactly-once roundtrip 通过。
4. **取消电平触发**：三条执行路径仍把 live `&AtomicBool` 转成
   `&dyn CancellationProbe`；没有读取成入口 bool snapshot，`SeqCst` 实现与 waiter 重探测未改。
5. **`CleanupUnproven` 不降级**：runtime-host port impl 仍在 observer context 之前优先保留
   `GovernedProcessExecutionError::CleanupUnproven`；adapter 的 exhaustive match 也保留该独立
   variant，再映射到既有固定 provider unavailable 表面，绝不伪装为 cancelled。

observer 的 `RunNoLongerLive` / `DeliveryFailed` 也显式进入
`LifecycleDeliveryFailed` fail-closed 路径，没有新增开放式错误逃生口。

## 6. 验证命令、原始摘要与 exit code

除特别说明外，Cargo 命令均使用 `CARGO_TARGET_DIR=<repo>/target-step5`。

| 命令 | exit code | 原始结果摘要 |
|---|---:|---|
| normal-tree RED 断言 | `1`（预期） | `RED: keeper-runtime-host is still present...` |
| `cargo test -p keeper-adapter-grok-acp --no-run` | `0` | 9 个 test/bin targets 全部生成 |
| `rustfmt --edition 2024 <本单 9 个 Rust 文件>` | `0` | 仅格式化白名单内本单文件 |
| `cargo test -p keeper-adapter-grok-acp` | `0` | 60 unit；19 fake-child；8 process、3 read、4 write roundtrip；6 H5；4 live-test 非 live；3 protocol fixture；0 failed；live provider 1 ignored |
| normal-tree GREEN 断言 | `0` | `NORMAL_RUNTIME_HOST_MATCHES=0` |
| `cargo check -p pillow-desktop` | `0` | desktop Rust production typecheck 完成 |
| `cargo test -p pillow-desktop --lib`（首轮） | `124` | 外层 120s 超时；编译完成后 pipe 被关闭，不是断言失败 |
| `cargo test -p pillow-desktop --lib`（完整重跑） | `0` | 146 passed，0 failed |
| `pnpm --filter @pillow/desktop typecheck` | `0` | `tsc --noEmit` |
| `pnpm --filter @pillow/desktop test` | `0` | 34 files，419 passed，0 failed |
| `cargo fmt --all -- --check` | `0` | workspace fmt clean |
| `cargo clippy -p keeper-adapter-grok-acp --all-targets -- -D warnings` | `0` | adapter production/tests clean |
| `cargo clippy -p pillow-desktop -- -D warnings` | `0` | desktop production target clean |
| `node scripts/run-worker-gate.mjs` | `0` | `PILLOW_WORKER_GATE: OK`；约 299.6s |
| 最终 corrected normal-tree 断言 | `0` | `NORMAL_RUNTIME_HOST_MATCHES=0` |
| `node scripts/check-secrets.mjs`（报告回填后） | `0` | 9 patterns self-tested，no findings |
| 最终 scoped diff/report audit | `0` | 17 calls；0 production host refs；0 staged；0 absolute paths/placeholder tokens；`diff --check` clean |

规定 worker gate 同一入口串行完成：workspace structure、secret scan、Cargo fmt、TypeScript
typecheck、package tests、`cargo test --workspace`、workspace clippy 与 ACP protocol drift。

额外诊断 `cargo clippy -p keeper-adapter-grok-acp -p pillow-desktop --all-targets -- -D warnings`
曾 exit 1，只命中 `runtime_composition.rs` 路径探测测试中的既有常量断言 lint
`assertions_on_constants`。本单没有弱化/改写该并行任务逻辑；adapter all-targets、desktop
production clippy 以及规定 worker gate 均随后 exit 0。

终审时一个过宽的“全工作树禁止路径必须为零”诊断 exit 1，只列出 5 个已知并行
knowledge-indexer diffs；改用本单显式 allowlist + 并行 diff 单列后，scoped audit exit 0。
另一个已废弃的 PowerShell 证据草稿使用了会与自动变量 `$Matches` 冲突的 `$matches`；报告中的
最终命令已改用 `$runtimeHostHits` 并重新执行，原始结果为 0。两项均不是产品测试或规定 gate 失败。

桌面 E2E 与 live Grok provider acceptance 按简报不运行；没有把 SKIP/ignored 写成 PASS。

## 7. 写入边界与未触碰项

本单写入仅有：

- `crates/adapter-grok-acp/Cargo.toml`；
- `crates/adapter-grok-acp/src/adapter.rs`、`src/prompt.rs`；
- adapter 的 6 个包含构造器调用点的 test files；
- `apps/desktop/src-tauri/src/runtime_composition.rs` 中 import 与 Grok adapter 组装点；
- 本报告。

没有修改 `crates/runtime-host/**` 或 `crates/governance-port/**`，因为 Step 4 已提供所需
compatibility impl 与 testkit，Step 5 无端口缺口。

明确没有触碰或认领：

- `Cargo.lock` 的预存 diff；
- `crates/storage-sqlite/**`、`crates/knowledge-indexer/**`；
- `crates/kernel-project/**`、`crates/git-executor/**`、`crates/kernel-resource/**`；
- `apps/desktop/src/**`、`scripts/**`、`tests/fixtures/**`、`.superpowers/**`；
- 认证、token、Keeper/Vault 材料。

staging 为空；未执行 `git add/commit/stash/reset/clean/checkout --/restore`。

## 8. 为什么 Step 5 仍不等于解耦完成

Step 5 完成的是 adapter → runtime-host **编译期 normal dependency 边**的切断。Step 4/5 的
compatibility handle 仍把真实 coupling 隐藏在 global service locator/free-function forwarding
后面；owner-scoped service、原子 all-tools bind/release、不可复制 lease、generation 与 shutdown
ownership 尚未收口。

因此本风险保持 **OPEN**。Step 6 才开始引入 `GovernedToolService`；只有完成 Step 10 的
ownership/lease 收口、替换 split register/release 并删除 globals/free-function compatibility 后，
才可称 implementation complete。本文的 `STEP5_COMPLETE` 绝不表示“解耦完成”或风险关闭。

## 9. 残余风险与最终判定

- `GlobalGovernedExecutionPort` 仍是零状态 ambient handle，无法证明 execution/control 来自同一
  owner-scoped service；这是 Step 6–10 的明确残余风险。
- adapter 的真实 roundtrip 仍需要 runtime-host dev dependency；这是保留真实 SQLite 治理证据，
  不是 normal production edge 回归。
- desktop E2E、desktop smoke 与 live provider acceptance 留给总控/后续 Step 11 继承式复验；
  本单没有越权运行。
- 并行 knowledge-indexer/K1b 工作仍在同一 dirty tree 中；worker gate exit 0，但总控接收时仍应
  以 allowlist diff 区分责任边界。

依赖图硬判据、17 点同编译、focused tests、desktop typecheck/unit 与规定 worker gate 均已满足。

`DECOUPLE-STEP5-COMPLETE`
