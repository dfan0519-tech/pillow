# Step 3 — 迁移 live cancellation probe

日期:2026-08-09  
实施:Grok (worker)  
工作目录:`<repo>`  
`CARGO_TARGET_DIR=<repo>/target-step3`  
结论:**DECOUPLE-STEP3-COMPLETE**  
workspace 总门:`BLOCKED-BY-CONCURRENT`(`cargo fmt --check` 撞到白名单外
`apps/desktop/src-tauri/src/settings_commands.rs` 的并行 Worker 半成品;
本单白名单包 fmt/clippy/测试均通过)。

## 1. 一句话

runtime-host 三个 executor/waiter 从 `&AtomicBool` 改为
`&dyn CancellationProbe`;adapter 用现有 `AtomicBool` token 以 trait object
传入。**对外取消语义零变化**(电平触发,每 wait cycle 重读,SeqCst)。

## 2. 迁移了哪三处

| # | 位置 | 变更 |
|---|---|---|
| 1 | `crates/runtime-host/src/governed_read.rs` | `execute_governed_read(..., cancellation: &dyn CancellationProbe)`;`LiveProbeCancellation` 每次 `is_cancelled` 转发 probe(不缓存) |
| 2 | `crates/runtime-host/src/governed_write.rs` | `execute_governed_write` + `wait_for_resolution` → `&dyn CancellationProbe`;waiter 循环内 `cancellation.is_cancelled()` |
| 3 | `crates/runtime-host/src/host/process.rs` | `execute_governed_process` + `wait_for_resolution` → `&dyn CancellationProbe`;waiter 循环内 `cancellation.is_cancelled()` |

adapter 接入(`crates/adapter-grok-acp/src/prompt.rs`):

```rust
let cancel_probe: &dyn CancellationProbe = cancel; // cancel: &AtomicBool
execute_governed_{read,write,process}(..., cancel_probe, ...)
```

**禁止 snapshot**:adapter 不读 `cancel.load` 再传 `bool`;直接传 live probe。

## 3. Trait 如何接入

- 依赖:`keeper-governance-port` 加入
  - `crates/runtime-host/Cargo.toml` (normal + dev,dev 供 integration test 的自定义 probe)
  - `crates/adapter-grok-acp/Cargo.toml` (normal)
- 使用 Step 1 已定义的 `CancellationProbe`;生产 `AtomicBool` 继续 `SeqCst`。
- **trait 微调(有理由)**:新增 `impl CancellationProbe for Arc<AtomicBool>`。
  理由:旧 API 参数是 `&AtomicBool`,调用方传 `&Arc<AtomicBool>` 时靠 auto-deref
  成功;对象安全 `&dyn CancellationProbe` 不会对 `Arc` 做 unsized 自动解引用,
  既有测试/adapter 多线程 token 会全部编译失败。impl 仍然**每次**
  `load(SeqCst)`,不是 snapshot。

未改 `GovernedExecutionPort`、未抽 service、未动 ownership/lease、未改
adapter 对 runtime-host 的依赖边(Step 4–5)。

## 4. Barrier 测试如何证明电平触发

### 4.1 新增测试(3 条)

| 测试 | 文件 | 机制 |
|---|---|---|
| `cancel_probe_level_triggered_after_execute_entry_barrier` | `governed_write_bridge.rs` | `Barrier(2)`:on_requested 时 cancel 仍 false → 主线程翻转 `AtomicBool` SeqCst → waiter 必须见 true → `Cancelled` |
| `cancel_probe_level_triggered_after_execute_entry_barrier` | `governed_process_bridge.rs` | 同上,在 Requested lifecycle 后翻转 |
| `cancel_probe_level_triggered_not_entry_snapshot` | `governed_read_bridge.rs` | `FirstFalseThenTrue` probe:第 1 次 false、之后 true;若入口 snapshot 则 Succeeded;正确 re-probe → Cancelled 且 calls≥2 |

### 4.2 变异探针(错误实现会挂 / 正确实现变绿)

临时把 `LiveProbeCancellation::is_cancelled` 改成 **OnceLock 缓存第一次读**
(入口快照 / fail-open),跑 read barrier 测试:

**错误实现(RED)**:

```text
running 1 test
test cancel_probe_level_triggered_not_entry_snapshot ... FAILED

assertion `left == right` failed: host must re-probe cancel after first false reading; probe calls=1
  left: Succeeded
 right: Cancelled
test result: FAILED. 0 passed; 1 failed; 0 ignored; 0 measured; 4 filtered out
```

**还原后(GREEN)**:

```text
running 1 test
test cancel_probe_level_triggered_not_entry_snapshot ... ok
test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 4 filtered out

# 三条 barrier 一并:
cancel_probe_level_triggered_after_execute_entry_barrier (process) ... ok
cancel_probe_level_triggered_not_entry_snapshot (read) ... ok
cancel_probe_level_triggered_after_execute_entry_barrier (write) ... ok
```

这证明测试考的是**电平触发 / 重读**,不是空转。

## 5. 插桩与生产不可见

本单**未**在 production 路径加入任何 barrier/static/hook。

- 确定性同步全部在 **integration tests** 内(Barrier / 自定义 probe)。
- 生产代码只有 `cancellation.is_cancelled()` 与 `LiveProbeCancellation` 转发。
- 无 `#[cfg(test)]` 生产字段、无 `test-support` 新 seam。
- 因此 release 图中不存在本单测试插桩(因为根本没有插进 lib)。

## 6. 既有取消相关测试

全部保留,未删改断言。回归结果:

| 套件 | 结果 | exit |
|---|---|---|
| `governed_process_bridge` | 25 passed (含既有 cancel + Step2 observer + 新 barrier) | 0 |
| `governed_read_bridge` | 5 passed (含既有 cancel + 新 barrier) | 0 |
| `governed_write_bridge` | 21 passed (含既有 cancel/C3F + 新 barrier) | 0 |
| `keeper-runtime-host --lib` | 22 passed (含 delivery/sink C3F cancel) | 0 |
| `keeper-adapter-grok-acp` | 全部通过(含 cancel roundtrip / H5 cancel) | 0 |
| `keeper-governance-port` | 12 unit + 1 doctest | 0 |
| clippy 三包 `-D warnings` | clean | 0 |
| fmt 三包 `--check` | clean | 0 |

## 7. 各命令 exit code 摘要

```text
CARGO_TARGET_DIR=<repo>/target-step3

cargo test -p keeper-runtime-host --test governed_*_bridge     → 0
cargo test -p keeper-runtime-host --lib                        → 0
cargo test -p keeper-adapter-grok-acp                          → 0
cargo test -p keeper-governance-port                           → 0
cargo clippy -p keeper-runtime-host -p keeper-adapter-grok-acp
            -p keeper-governance-port -- -D warnings           → 0
cargo fmt -p keeper-runtime-host -p keeper-adapter-grok-acp
          -p keeper-governance-port -- --check                 → 0
node scripts/run-worker-gate.mjs                               → 1
  (FAILED at cargo-fmt; 见下节 BLOCKED-BY-CONCURRENT)
```

## 8. 没有碰哪些东西

- `crates/storage-sqlite/**`、`crates/knowledge-indexer/**`、`apps/desktop/**`
- `crates/kernel-project/**`、`crates/git-executor/**`、`crates/kernel-resource/**`
- `scripts/**`、`tests/fixtures/**`
- `docs/` 下除本报告外任何文件
- Step 2 observer-failure 逻辑(`fail_closed_after_observer_failure` 等)
- Step 4–11:`GovernedExecutionPort` impl、service 抽取、lease、adapter normal edge 删除
- 无 `git add/commit/stash/reset/clean/checkout/restore`
- 无桌面 E2E;未读 auth/token

## 9. Workspace 门:BLOCKED-BY-CONCURRENT

`run-worker-gate.mjs` 在 `cargo fmt --all -- --check` 失败,diff 指向:

```text
apps/desktop/src-tauri/src/settings_commands.rs
```

该文件**不在本单白名单**,且 diff 为 import 排序半成品(并行 Worker)。
按简报要求**未修别人的文件**。本单白名单包单独 `fmt --check` 已通过。

权威门由总控批次末尾独占重跑。

## 10. 残余风险

1. **Host 内其它 cancel 面未迁移**:`ProtocolGuardSink` / worker 仍用
   `&AtomicBool`/`load(SeqCst)`。提案 Step 3 只要求三个 executor/waiter;
   sink/worker 是 host 内部 cancel 仲裁,不在本单。
2. **Desktop 集成**:`apps/desktop` 若直接调 `execute_governed_*` 传
   `&AtomicBool`,仍合法(trait object 接受)。未跑 desktop 编译(白名单外 +
   并行 Worker)。
3. **Write barrier 在错误实现下会等到 TTL**(~600s)才 Expired 失败;read 变异
   探针已足够快地证明电平触发。后续若要 write 侧 fail-fast,可加
   force-expiry 看门狗(非本单必需)。
4. **`Arc<AtomicBool>` impl** 是兼容面扩展;若未来禁止共享 token 持有,
   可再收紧(仍不得改成 snapshot)。

## 11. 完成标记

**DECOUPLE-STEP3-COMPLETE**

workspace 总门备注:`BLOCKED-BY-CONCURRENT` —
`apps/desktop/src-tauri/src/settings_commands.rs` fmt 半成品(非本单)。
