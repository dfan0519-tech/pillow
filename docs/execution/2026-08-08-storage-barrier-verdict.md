# Pillow storage 双 transition：barrier 判定与条件修复报告

日期：2026-08-08  
执行基线：`30adf168048ef0b19daca141ad35f3aaa864ebe9`  
结论：**机制坐实；最小修复已实现并通过决定性回归；workspace 总门因仓库 HEAD 中白名单外的既有 secrets finding 阻塞。**

## 1. 决定性插桩

### Barrier 与 trace

在 `SqliteWriteProposalRepository` 实例上增加了 proposal-scoped 测试 barrier：

- barrier 只匹配一个 `proposal_id`，使用独立 `Mutex + Condvar`，默认实验为两个参与方并带 5 秒超时；没有新增全局 `static`。
- waiter 在 coordinator 锁内判断 expiry 后释放锁，再以 `waiter-expiry` 到达 barrier。
- list 在 coordinator 锁内收集 `to_expire` 后释放锁，再以 `main-list` 到达 barrier。
- barrier trace 记录“已经越过 expiry 判断且尚未进入 transition”的 proposal/source；mutation trace 记录实际进入 SQLite transition wrapper 的 proposal/source。
- barrier 自身不持有 coordinator mutex 或 SQLite connection mutex，因此没有改变生产锁顺序，也不会把测试等待放入生产临界区。
- 成功路径回归另可指定 `waiter-expiry` 为测试专用 preferred source：它完成 durable transition、live publication/removal 后才释放 `main-list`，用于确定性覆盖 loser 看见 live entry 已移除的窗口。

反向控制没有使用概率等待。waiter 在 `on_requested` 回调中通过 channel 暂停；主线程完成失败 list、counter、durable row、diff/grant quarantine 检查后才释放 waiter。

### 生产构建不存在测试插桩的证明

以下元素全部由 `#[cfg(any(test, feature = "test-support"))]` 门控：

- `Arc`/`Condvar`/测试 duration imports；
- barrier 与 trace 的 repository 字段；
- barrier state/type/implementation；
- set/clear/snapshot/wait/trace/complete 方法；
- runtime-host 的 source 参数、source 字面量、barrier 调用和带来源的 transition wrapper。

`read_durable_terminal_after_transition` 的非测试分支仍直接调用原有 `store.transition(...)`。`keeper-storage-sqlite` 与 `keeper-runtime-host` 的 `test-support` 都是非默认 feature；desktop 正常 `[dependencies]` 未启用它，只有 `[dev-dependencies]` 启用。由此 release dependency graph 中不存在这些字段、类型、方法、trace 字符串或输出。生产可见的 `terminal_transition_in_progress` 是本次修复状态，不是 mutation hook。

workspace gate 在编译前被第 7 节的既有 secrets finding 截断，因此本轮不能把 workspace release 编译作为第二份动态证明；上述结论来自逐项 cfg/source 与 Cargo feature 边界核验。

## 2. 修复前实验原始输出

执行命令：

```text
node scripts/run-cargo.mjs test -p keeper-runtime-host --features test-support --test governed_write_bridge storage_failure_after_live_never_fabricates_terminal -- --exact --nocapture --test-threads=1
```

决定性 barrier 原始关键输出：

```text
STORAGE_BARRIER_REACHED proposal_id=b9d98cb5-81c2-45a3-b157-3283d0d44b2b source=main-list
STORAGE_BARRIER_REACHED proposal_id=b9d98cb5-81c2-45a3-b157-3283d0d44b2b source=waiter-expiry
STORAGE_TRANSITION_ATTEMPT proposal_id=b9d98cb5-81c2-45a3-b157-3283d0d44b2b source=waiter-expiry
STORAGE_TRANSITION_ATTEMPT proposal_id=b9d98cb5-81c2-45a3-b157-3283d0d44b2b source=main-list
STORAGE_BARRIER_RESULT proposal_id=b9d98cb5-81c2-45a3-b157-3283d0d44b2b barrier_trace=[("b9d98cb5-81c2-45a3-b157-3283d0d44b2b", "main-list"), ("b9d98cb5-81c2-45a3-b157-3283d0d44b2b", "waiter-expiry")] transition_trace=[("b9d98cb5-81c2-45a3-b157-3283d0d44b2b", "waiter-expiry"), ("b9d98cb5-81c2-45a3-b157-3283d0d44b2b", "main-list")] attempts_before=2 attempts_after=4 delta=2
```

同一次运行中的反向控制原始关键输出：

```text
STORAGE_BARRIER_REACHED proposal_id=c562c2e2-22e0-416d-a397-eb5a418efe9c source=main-list
STORAGE_TRANSITION_ATTEMPT proposal_id=c562c2e2-22e0-416d-a397-eb5a418efe9c source=main-list
STORAGE_CONTROL_RESULT proposal_id=c562c2e2-22e0-416d-a397-eb5a418efe9c barrier_trace=[("c562c2e2-22e0-416d-a397-eb5a418efe9c", "main-list")] transition_trace=[("c562c2e2-22e0-416d-a397-eb5a418efe9c", "main-list")] attempts_before=2 attempts_after=3 delta=1
```

该修复前测试随后按“不允许两次 transition”的既有语义失败：

```text
assertion `left == right` failed: single-flight one SQLite transition
  left: 2
 right: 1
test result: FAILED. 0 passed; 1 failed
```

反向控制中，waiter 放行前已经确认：list 返回 `Gateway`、counter delta 恰好 1、live diff/grant 已 quarantine、durable row 仍为 `PendingApproval`；放行后 waiter 返回 opaque `Gateway`，counter 没有增加。

## 3. 机制判定

**坐实。**

依据不是失败概率，而是同一个 proposal 的因果 trace：

1. `main-list` 与 `waiter-expiry` 都已经越过各自的 expiry 判断；
2. 两者随后都进入了同一个 transition mutation；
3. repository-instance counter 从 2 增至 4，delta 精确为 2；
4. 暂停 waiter 后只剩 `main-list` 一笔 mutation，delta 精确为 1，排除了 main 线程链内部重试和第三写入口。

因此原判断“同一次过期被 list 与 waiter 两条离锁路径重复 transition”得到决定性验证。

## 4. 最小修复

修复只在 runtime-host live coordinator 中为 terminal transition 墓碑增加 proposal-scoped single-flight：

- `LiveWriteEntry` 增加 `terminal_transition_in_progress`。
- 每个调用在现有 coordinator mutex 下检查 expected owner 并竞争唯一 winner；loser 在现有 condvar 上等待，等待时不持锁。
- 只有 winner 离锁后执行一次 SQLite transition；失败后最多做原有的一次只读 durable recovery。
- 成功时 winner 只向同一 expected owner 的 live entry 发布 durable metadata/terminal，清除 in-progress 并唤醒 waiter/loser。
- 失败时只 quarantine expected owner 的 live entry并唤醒所有等待者；loser 只允许接受真实 durable terminal，否则同样返回 `Gateway`。
- waiter 成功收尾与 quarantine 都改为 owner-aware removal；删除 `pending_by_run` 前还确认映射仍指向该 proposal，避免 stale caller 删除另一 host 的替代 entry。

这是最小修复，因为 SQLite schema、transition 校验、持久化顺序、公开 receipt/error 映射均未改变；也没有在 coordinator 锁内执行 SQLite 或增加锁的嵌套顺序。forced write failure 下 durable row 仍为 `PendingApproval`，不会伪造 terminal，list 与 waiter 都保持 opaque `Gateway`。

## 5. 修复后回归结果

原 exact 测试保留“counter 必须恰好 +1”的断言，未放宽为范围或改写期望。最终复跑：

```text
STORAGE_BARRIER_REACHED proposal_id=72612791-04fd-4b8f-841f-a101f01516e2 source=main-list
STORAGE_BARRIER_REACHED proposal_id=72612791-04fd-4b8f-841f-a101f01516e2 source=waiter-expiry
STORAGE_TRANSITION_ATTEMPT proposal_id=72612791-04fd-4b8f-841f-a101f01516e2 source=waiter-expiry
STORAGE_BARRIER_RESULT proposal_id=72612791-04fd-4b8f-841f-a101f01516e2 barrier_trace=[("72612791-04fd-4b8f-841f-a101f01516e2", "main-list"), ("72612791-04fd-4b8f-841f-a101f01516e2", "waiter-expiry")] transition_trace=[("72612791-04fd-4b8f-841f-a101f01516e2", "waiter-expiry")] attempts_before=2 attempts_after=3 delta=1 list_error=Gateway waiter_error=Gateway
STORAGE_BARRIER_REACHED proposal_id=972136eb-5f35-434f-896b-3767f2d79685 source=main-list
STORAGE_TRANSITION_ATTEMPT proposal_id=972136eb-5f35-434f-896b-3767f2d79685 source=main-list
STORAGE_CONTROL_RESULT proposal_id=972136eb-5f35-434f-896b-3767f2d79685 barrier_trace=[("972136eb-5f35-434f-896b-3767f2d79685", "main-list")] transition_trace=[("972136eb-5f35-434f-896b-3767f2d79685", "main-list")] attempts_before=2 attempts_after=3 delta=1 list_error=Gateway waiter_error=Gateway quarantine_before_release=true durable_before_release=pending_approval
test result: ok. 1 passed; 0 failed; 19 filtered out
```

新增成功路径回归 `expiry_single_flight_waiter_winner_survives_live_removal`，强制 waiter 成为 winner 并先移除 live entry，验证 list loser 只读 durable `Expired`，不做第二笔 mutation：

```text
STORAGE_BARRIER_REACHED proposal_id=02442499-df3a-4003-905e-39be264b3cde source=main-list
STORAGE_BARRIER_REACHED proposal_id=02442499-df3a-4003-905e-39be264b3cde source=waiter-expiry
STORAGE_TRANSITION_ATTEMPT proposal_id=02442499-df3a-4003-905e-39be264b3cde source=waiter-expiry
STORAGE_SUCCESS_SINGLE_FLIGHT proposal_id=02442499-df3a-4003-905e-39be264b3cde barrier_trace=[("02442499-df3a-4003-905e-39be264b3cde", "main-list"), ("02442499-df3a-4003-905e-39be264b3cde", "waiter-expiry")] transition_trace=[("02442499-df3a-4003-905e-39be264b3cde", "waiter-expiry")] attempts_before=2 attempts_after=3 delta=1 waiter_removed_live=true list_state=expired
test result: ok. 1 passed; 0 failed; 19 filtered out
```

附加检查：

```text
node scripts/run-cargo.mjs fmt --all -- --check  -> PASS
git diff --check                                -> PASS（仅 Git 的 LF/CRLF 提示）
```

## 6. 写入范围

本任务仅修改：

- `crates/runtime-host/src/governed_write.rs`
- `crates/storage-sqlite/src/write_proposal.rs`
- `crates/runtime-host/tests/governed_write_bridge.rs`
- `docs/execution/2026-08-08-storage-barrier-verdict.md`

没有执行 desktop E2E，没有改动或运行被禁止的 acceptance 脚本，没有执行 git add/commit/stash/reset/clean/restore。工作期间出现的无关 untracked 文件 `docs/decisions/ADR-0003-settings-authority-and-s0-scope.md` 未读取、未修改、未纳入本任务 diff。

## 7. Worker gate 与阻塞

最终代码落定后按唯一合规入口重新执行：

```text
node scripts/run-worker-gate.mjs
```

原始结果：

```text
PILLOW_WORKER_GATE: start (sole compliant gate entry)

==> C:\Program Files\nodejs\node.exe scripts\check-workspace.mjs
workspace structure ok

==> C:\Program Files\nodejs\node.exe scripts\check-secrets.mjs
secret gate FAILED — 1 finding(s):
  docs/handoff/2026-08-08-pillow-controller-handoff.md:239  [user-profile-path] host user-profile path

Redact the value, or add a reviewed entry to ALLOW in scripts/check-secrets.mjs.
PILLOW_WORKER_GATE: FAILED at step "secrets" (exit 1)
```

只读核验确认该 handoff 文件相对工作树无 diff，且 `HEAD:docs/handoff/2026-08-08-pillow-controller-handoff.md` 第 239 行已有同一 `USERPROFILE=` 机器路径内容(值本身此处不复述,避免二次泄漏)。修复它需要修改 `docs/handoff/**` 或 `scripts/check-secrets.mjs`，两者均不在本任务精确写入白名单内，因此没有越界处理。gate 在 secrets 阶段提前退出，后续 workspace typecheck/lint/unit/integration 编译测试没有执行。

## 8. 残余风险与不确定项

- 当前决定性失败与成功回归均通过，双 transition 的进程内机制与修复已覆盖；跨进程同时写同一 SQLite 的行为不属于本次进程内 coordinator 实验。
- loser 在 live entry 已消失时只接受真实 durable terminal；若该只读查询本身失败，会按既有安全边界返回 `Gateway`，这是刻意的 fail-closed 行为。
- barrier 的 5 秒超时、preferred source 与 trace 都只存在于 test/test-support 构建。
- workspace gate 后续阶段尚无本轮证据；必须先由总控处理或批准处理第 7 节的白名单外基线 finding，然后从同一入口完整重跑。
- 未触碰无关 untracked ADR；总控合并/清理时必须继续保留其归属。

STORAGE-BARRIER-BLOCKED: 最终 worker gate 被 HEAD 中白名单外的 `docs/handoff/2026-08-08-pillow-controller-handoff.md:239` user-profile-path finding 截断。
