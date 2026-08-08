# Pillow — harness flake fixes report

日期：2026-08-08

仓库：`<repo>`

范围：**仅**两条已高置信定性的 harness/测试 flake：

1. `cancel-tree` — 无界同步 WMI 扫描（任务 A）
2. `slow_output_is_visible_live_and_reopens_after_waiter_consumes_terminal` — 错误起算点（任务 B）

**明确不在范围、未触碰：**

- `two-project-isolation`（证据不足，listener/catch-up 属产品邻近）
- `storage_failure_after_live_never_fabricates_terminal`（产品邻近双 transition 竞态；契约未决）

**禁止项遵守：** 未改任何 `crates/*/src/**`、`apps/desktop/src/**`、`apps/desktop/src-tauri/src/**`；未跑桌面 E2E；未 git add/commit/stash/reset/clean/checkout/restore；未动 `.superpowers/c4-*` 既有验收证据；未读 auth/token。

---

## 1. 任务 A — `cancel-tree` WMI 硬超时与可中断预算

### 改了什么

| 文件 | 改动 |
|---|---|
| `scripts/wave-c4-governed-process-acceptance.mjs` | `snapshotWindowsProcessesWithCommandLine` 加 `timeout`；`ProcessSnapshotError`；marker 轮询预算内 per-scan 超时；环境失败写 timeline 并 fail-closed |
| `scripts/wave-c4-git-changes-acceptance.mjs` | 同源 `spawnSync` 加硬超时与环境错误分类（清理路径同源风险） |
| `scripts/lib/c4-governed-process-acceptance.mjs` | **未改**（WMI 在 wave 脚本内，不在 lib） |
| `scripts/lib/c4-git-changes-acceptance.mjs` | **未改**（同上） |

### 为什么是握手/硬上限，而不是加大超时

Codex 结论：外层「最多 5 秒」用 `performance.now()` 算 deadline，**打断不了**正在执行的同步 `spawnSync`。高负载下一次无界 WMI 可越过产品固定 60 秒进程时限；后端正常终态化 run 后 `composer-cancel` 合法卸载，后续 60 秒等待是**误导性的产品失败表象**。

正确修法是：

1. **给 `spawnSync` 明确 `timeout`**，使单次扫描有硬上限；
2. **单次 timeout ≤ 外层剩余预算**，让「最多 5 秒」成为真实可中断上限；
3. **超时不是「没有匹配进程」**。

没有把任何取消窗口、composer-cancel 等待或产品 60s 时限调大。

### 超时返回如何处理

`spawnSync` 在超时/中止时：

- `result.error`（常见 `code === "ETIMEDOUT"` / `killed`）→ 抛 `ProcessSnapshotError`（`isEnvironmentFailure: true`）
- `result.status === null`（signal 中止）→ 同样抛 `ProcessSnapshotError`
- `status !== 0` → 仍按原样报命令失败

**绝不**把上述情况映射为空数组。

### 轮询预算（cancel-tree marker poll）

- 外层预算：`5_000` ms（不变）
- 单次扫描：`min(MARKER_SCAN_MAX_MS=1500, remainingMs)`
- 剩余 `< 50` ms：不再开扫
- 连续环境失败 ≥ 3，或剩余预算已不足以再扫：`writeTimeline("red-marker-scan-environment", …)` 后 **明确失败**
- 整个 5s 内 **0 次成功扫描**：同样环境失败退出
- 成功扫描但 markers=0：允许继续 cancel（产品不要求先看到 marker）

### 取证：环境异常 vs 产品失败

| 类别 | 信号 | 输出 |
|---|---|---|
| **环境异常** | `ProcessSnapshotError` / 无成功 WMI 扫描 | timeline `red-marker-scan-environment`；错误文案含 `process marker scan environment failure` / `not a product cancel failure`；字段 `markerScanErrors`、`successfulScans`、`scanTimeoutMs` |
| **产品/DOM 失败** | cancel 按钮不可见等 | 既有 `red-cancelOwningRun` / `cancelOwningRun` dump（现额外附带 `markerScanErrors`） |
| **绿路径** | 正常 | timeline `green` 含 `markerPollSamples`、`markerScanErrors`、`successfulScans` |

### `waitForMarkedNodeProcessesGone`

清理轮询同样使用剩余预算内的 per-scan timeout；连续环境失败 fail-closed，避免「假残活」或挂死。

---

## 2. 任务 B — `slow_output` 阶段握手

### 定位

`crates/runtime-host/tests/governed_process_bridge.rs`  
`slow_output_is_visible_live_and_reopens_after_waiter_consumes_terminal`

（仅测试文件；产品 `src` 未改。）

### 旧问题

- 在 `decide` 线程刚 spawn 后立刻做 `100 × 10ms` 轮询
- 预算从 **approve / claim / Windows suspended spawn / Node 启动 / reader** 之前起算
- 高并行下 chunk 晚于第 100 次 open → `live.expect("live output")` panic
- child 固定 700ms 退出，无法证明「terminal 前 live 可见」

### 握手设计（非加大 1s）

1. **Child**：写 marker 后 **hold until test sentinel 文件存在**（60s 自毁上限防残留）
2. **Waiter lifecycle**：`GovernedProcessLifecycle::Started` 经 `mpsc` 通知主线程（claim 确认后）
3. **主线程**：`recv_timeout(30s)` 等到 Started **后**才 condition-wait marker
4. **断言 live**：`live_retained` 且 `!waiter.is_finished()`（terminal 前）
5. **释放**：写 sentinel → join decide/waiter → 断言 reopen retained 仍含 marker
6. **失败路径**：先写 sentinel、join、shutdown，再 panic（避免污染后续测试）

### 关于 post-Started 的 15s

这是 **spawn/Node 在负载下的环境挂死上界**，不是「把 1s 调到 15s 赌概率」：

- 起算点已从错误的 pre-approve 改为 **Started 后**
- 进程被 sentinel 钉住，不与 700ms 退出竞速
- 语义断言是「waiter 未完成时已见 marker」，不是「在某个魔法毫秒内碰巧扫到」

握手不可行的情况：本测 **握手可行**（lifecycle `Started` + 文件 sentinel），故未采用纯加大 timeout。

### 未弱化的断言

- marker 必须出现在 `open_process_output` chunks
- 首次观察时 `live_retained`
- 观察时 waiter 仍未结束（pre-terminal）
- terminal 后 reopen 仍含 marker
- `release_governed_process_run` 后仍 retained
- receipt `Succeeded`

---

## 3. 门禁与命令 exit code

| 命令 | exit |
|---|---:|
| `node --check scripts/wave-c4-governed-process-acceptance.mjs` | **0** |
| `node --check scripts/wave-c4-git-changes-acceptance.mjs` | **0** |
| `node --check scripts/lib/c4-governed-process-acceptance.mjs` | **0** |
| `node --check scripts/lib/c4-git-changes-acceptance.mjs` | **0** |
| `node scripts/run-cargo.mjs test -p keeper-runtime-host --test governed_process_bridge slow_output_… -- --exact --test-threads=1` | **0** |
| `node scripts/run-cargo.mjs test -p keeper-runtime-host --test governed_process_bridge`（20 tests） | **0** |
| `node scripts/run-worker-gate.mjs` | **1**（见下） |

### worker-gate 失败说明（非本单改动）

失败步：`package-tests`（`@pillow/desktop` vitest）

```
FAIL tests/visual-shell-contract.test.tsx
  expected '--bg-main: #161618'；实际 tokens 为 '#1e1e24'
```

任务白名单禁止修改 `apps/desktop/src/**`（另有 UI Worker）。该失败与 harness/WMI/slow_output **无关**，属并行 UI 主题/token 漂移。本单不修复。

---

## 4. 未碰清单

| 项 | 状态 |
|---|---|
| `two-project-isolation` 场景与相关产品 listener 代码 | 未碰 |
| `storage_failure_after_live_*` 测试与 `governed_write` 产品路径 | 未碰 |
| 任何 `crates/*/src/**` | 未碰 |
| 任何 `apps/desktop/src/**` / `src-tauri` | 未碰 |
| 桌面 E2E 执行 | 未跑（总控独占） |
| `.superpowers/c4-*` 既有证据 | 未改 |
| git 写操作 | 未做 |

---

## 5. 残余风险

1. **E2E 未由本 Worker 验证** cancel-tree 新失败路径；总控需在真实桌面负载下确认 `red-marker-scan-environment` 可区分，且绿路径 cancel 时序仍健康。
2. **git-changes** 同源 snapshot 超时后会抛错而非假空表；若清理路径偶发 WMI 超时，会以环境错误失败（优于静默漏杀或挂死）。`scripts/lib/desktop-process.mjs` 的全局 snapshot 基建硬化仍属独立后续（VERDICT 5.5），本单未改。
3. **slow_output** 仍与同 binary 的 33-process cache 测试共享 process coordinator；握手消除错误起算，不消除全局锁竞争（仅不再被 1s 误杀）。
4. **worker-gate** 当前因 UI visual contract 红；合入前需 UI 线对齐或总控确认该失败可隔离。
5. cancel-tree 在 WMI 健康但 child 尚未出现 marker 时仍会（正确地）继续 cancel；清理证明弱于「看到 marker 再 cancel」，与改前语义一致。

---

## 6. 完成信号

本单两条 harness/测试修复与定向验证已完成。

`HARNESS-FLAKE-FIXES-COMPLETE`

（备注：全量 `run-worker-gate` 因并行 UI `visual-shell-contract` 失败 exit 1；非本单改动。E2E 待总控。）
