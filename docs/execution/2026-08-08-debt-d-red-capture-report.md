# DEBT-D — D3B3 间歇失败 RED 现场捕获报告

| 字段 | 值 |
|------|-----|
| 任务 | 捕获 `cancel-tree` / `two-project-isolation` 真实 RED，而非修绿 |
| Worker | grok4 |
| 总控 | Claude |
| 日期 | 2026-08-07（会话日）/ 报告文件 2026-08-08 |
| 授权通道 | 独占 E2E：仅 `tests/e2e/c4-governed-process.spec.ts` |
| 信号 | **`DEBT-D-RED-CAPTURE-COMPLETE`** |

## 1. 结论（先读）

**本轮未复现目标间歇失败。**

- 有效完整 process 套件：**7/7 全绿**（run 2–8）。
- 本轮 **0** 份 `*timeout*.json` / `cancel-tree-red-*` 现场产物。
- **不得**据此编造产品根因；根因置信度对「当前缺陷定位」= **N/A（未抓到 RED）**。
- 对「单跑 process 套件在当前机器/负载下难以复现」= **高置信（~0.85）**。
- 历史仍有 **1 次**已记录的 `two-project-isolation` RED（上一轮 e2e-run1，无 DOM dump），风险未关闭。

## 2. 运行矩阵

授权命令（每次）：

```bash
corepack pnpm --filter @pillow/desktop exec node --test --test-concurrency=1 \
  tests/e2e/c4-governed-process.spec.ts
```

日志索引：`.superpowers/c4-cancel-tree-debug/debt-d-run-log.jsonl`  
stdout：`debt-d-run{N}.log`

| Run | 开始 (本地 +09:00) | 结束 | 耗时 (s) | 结果 | 残留 pillow/1420 | 备注 |
|-----|-------------------|------|----------|------|------------------|------|
| 1 | 19:41:12 | 19:41:22 | 10.6 | **基础设施失败** | 0/0 | grok4 slot 无 active rustup toolchain；非产品 RED |
| 2 | 19:42:24 | 19:50:00 | 456.4 | **PASS** | 0/0 | 首轮绿；写 green timeline |
| 3 | 19:50:20 | 19:57:51 | 451.7 | **PASS** | 0/0 | |
| 4 | 19:57:55 | 20:05:18 | 442.8 | **PASS** | 0/0 | |
| 5 | 20:05:18 | 20:12:24 | 425.2 | **PASS** | 0/0 | |
| 6 | 20:12:24 | 20:19:21 | 416.3 | **PASS** | 0/0 | |
| 7 | 20:19:21 | 20:26:04 | 403.5 | **PASS** | 0/0 | |
| 8 | 20:26:05 | 20:32:49 | 404.4 | **PASS** | 0/0 | |

- **有效尝试**：7（≥6 要求）。
- **连续串行**：run 2–8 之间未人工清 app-data（每次 harness 自建 temp app-data；会话间无跨 run 状态复用）。
- 每次结束后 residual 检查：`pillow.exe` 与 `:1420` 均为 0。
- 未跑 `test:e2e` / `test:e2e:c4:git` / 其它 `wave-*-acceptance.mjs`。

## 3. RED 原始现场

### 3.1 本轮

**无。** 目录 `.superpowers/c4-cancel-tree-debug/` 在本轮仅新增：

- `debt-d-run{1..8}.log` / `.err.log`
- `debt-d-run-log.jsonl`
- 7 份 `cancel-tree-green-*-timeline.json`（成功路径对照）

无 `*-timeout.json`、无 `*-timeout.png`、无 `cancel-tree-red-*`。

### 3.2 历史对照（非本轮新抓，但唯一已落盘的相关 RED）

上一轮专项（同目录 `e2e-run1.log`，约 2026-08-07 17:05）：

```text
status: failed
error: two-project-isolation: locator.waitFor: Timeout 60000ms exceeded.
       waiting for getByTestId('command-approval-bar') to be visible
```

同次运行中 **`cancel-tree` 已通过**（cancel 全程可见；当时仍有 20s「运行中」死等，已被证伪并从 harness 移除）。

该 RED **没有** DOM dump / 截图 / 相位标签 → 无法判断卡在 `prompt-a` / `prompt-b` / `restore-b-before-reject` 哪一段。本轮已为这些相位补齐失败 dump（见 §5）。

## 4. 绿路径对照数据（本轮 7 次一致）

代表样本：`cancel-tree-green-2026-08-07T11-29-53-802Z-timeline.json`

| 相位 | composer-cancel | approval bar | process activity | 备注 |
|------|-----------------|--------------|------------------|------|
| after-send | visible+enabled | 无 | 尚无 process row | run 已 active（canCancel 挂载） |
| after-approval-visible | visible | 1 可见，Approve 可点 | `待批准` | |
| after-approve | visible | 1 可见，Approve disabled | 仍 `待批准` | 与「execute 返回后才 emit process_started」一致 |
| before-cancelOwningRun | visible | 仍在 | `待批准` | marker 已出现（2） |
| after-cancel-terminal | 消失 | 无 | `已失败`* | 代理侧显示已取消；测试接受 cancelled/interrupted/**failed** |

\*绿路径上 process row 常以 `已失败` 终结，同时助手文案 `governed-process-long-tree-ok`、代理活动 `已取消`。这是 **成功路径观测**，不是本轮失败。

7 次 green 汇总指标：

| 指标 | 范围 |
|------|------|
| `msApproveToCancelStart` | 604–779 ms |
| `markersBefore` (c4p-child) | 恒为 2 |
| `cancelClicks` | 恒为 2 |
| marker poll 内 cancelVisible | 恒为 true |

**推论（绿路径）**：在当前 process 套件时序下，`composer-cancel` 从 send 到 cancel 点击前 **不消失**；不支持「approve 后 cancel 闪断」作为常见机制。

## 5. Harness 变更（白名单内）

**未改** `apps/**` / `crates/**` / git acceptance harness。  
**未**靠延长超时或弱化断言变绿。

### 5.1 `scripts/wave-c4-governed-process-acceptance.mjs`

1. **`ensureHostRustToolchainEnv`**  
   - 原因：grok4 slot 的 rustup home（`%USERPROFILE%\.grok4\.rustup`）无 active toolchain，导致 run1 `cargo metadata` 失败。  
   - 行为：slot 无 active toolchain 时回落到宿主 `HOMEDRIVE+HOMEPATH\.rustup` / `.cargo`（与 git harness 同思路，脚本内自包含副本，未改 git 文件）。  
   - 仅影响 **acceptance debug build 环境**，不改产品 runtime env。

2. **`scenarioCancelTree` 相位时间线**  
   - 失败：`red-after-send` / `red-after-approve` / `red-cancelOwningRun` + 截图。  
   - 成功：写 `cancel-tree-green-*-timeline.json`（对照基线）。  
   - marker 轮询期间采样 `cancelVisible`（防止 cancel 在 approve→cancel 间隙消失却无采样）。

3. **`scenarioTwoProjectIsolation`**  
   - 对 60s 级 `waitForCommandApproval` 传入 `debug.phase`：`prompt-a` / `restore-a-after-open` / `prompt-b` / `restore-b-before-reject`。  
   - 8s 短探测 **不**写 dump（预期恢复分支，避免假 RED 产物）。

### 5.2 `scripts/lib/c4-governed-process-acceptance.mjs`

- **`waitForCommandApproval`**：可选 `debug`；超时后 dump DOM（复用 `dumpCancelTreeUiState`）+ 可见性采样 + 截图 + 富错误信息（`everVisible`、project、activity、body snippet）。

## 6. 根因判断与置信度

### 6.1 本轮可下的判断

| 判断 | 置信度 | 依据 |
|------|--------|------|
| **未复现**目标间歇 RED | 高 (1.0) | 7/7 绿；0 red 产物 |
| 单跑 process 套件不足以稳定触发 | 高 (~0.85) | 本轮 7 + 上轮修后 4 连续绿 ≈ 11 次 process 套件绿 |
| cancel-tree 绿路径上 cancel 门闸稳定 | 高 (~0.9) | 7 份 timeline 相位一致；approve→cancel &lt;1s 且 cancel 恒可见 |
| 「等 process 运行中」是根因 | **已排除**（上轮） | 上轮绿 dump 反驳；本轮不再等待 |
| `conversations_loaded` 乱序是根因 | **已排除**（任务给定） | 修后仍复现过 cancel-tree（历史） |
| two-project-isolation 曾真实失败 | 中高 (~0.75) | e2e-run1 记录；但缺 dump，相位未知 |
| 产品当前存在确定性 cancel-tree 缺陷 | **不足以下结论** | 无新 RED |

### 6.2 仍开放的假设（未被本轮数据排除）

1. **长串行 / 跨套件累积**（完整 `test:e2e` 或 process+git 串联）才暴露 — 本轮授权禁止验证。  
2. **two-project-isolation 审批条偶发永不挂载**（fake ACP / operation_gate / 项目切换丢 conversation）— 历史 1 次，无 dump。  
3. **cancel-tree 在极端负载下 cancel 晚挂载或早卸载** — 历史总控见过 2 次，本轮 0。  
4. **机器/负载相关** — 本轮负载相对平稳；未人为加压（避免改产品行为）。

### 6.3 明确不编造

无真实 RED → **无产品补丁草案**（按任务：产品缺陷须基于 RED 才写草案且不动源码）。  
Harness 取证已加强，下次失败应能落盘相位 + 截图。

## 7. 残余风险

| 风险 | 等级 | 说明 |
|------|------|------|
| C4 §5.1 cancel-tree 间歇 | 中 | 历史 2 次；process 套件近 11 次未再抓到 |
| C4 §5.2 two-project-isolation 间歇 | 中 | 历史 1 次（e2e-run1）；本轮 7 次未再抓到 |
| 取证仍可能漏相位 | 低 | 已覆盖主要 60s approval 等待与 cancel 全相位；非 approval 失败路径（如 selectProject / openConversation）仍可能只有栈信息 |
| grok slot rustup | 低 | harness 已回落宿主 toolchain；slot 自建 toolchain 后会走原路径 |

## 8. 下一步建议（给总控）

1. **保留**本轮失败路径 dump；下次任意 RED 优先读  
   `.superpowers/c4-cancel-tree-debug/*timeout*.json` 与 `cancel-tree-red-*`。  
2. 若总控能开更长通道：在 **完整串行 E2E**（或 process 套件外再挂负载）下复捕 — 与历史复现条件更一致。  
3. 可选 harness 加压（仍不改产品）：同 session 内重复 `cancel-tree`×N / `two-project-isolation`×N；或在 marker poll 外插入纯测试侧 CPU 自旋。需新授权。  
4. **不要**在无 RED 时改产品 cancel / approval 逻辑。  
5. 历史 e2e-run1 的 isolation RED 若需定性，只能等新 RED + 本轮 phase dump。

## 9. 产物清单

| 路径 | 用途 |
|------|------|
| `docs/execution/2026-08-08-debt-d-red-capture-report.md` | 本报告 |
| `.superpowers/c4-cancel-tree-debug/debt-d-run-log.jsonl` | 每次 run 元数据 |
| `.superpowers/c4-cancel-tree-debug/debt-d-run{1..8}.log` | 套件 stdout |
| `.superpowers/c4-cancel-tree-debug/cancel-tree-green-*-timeline.json` ×7 | 绿路径相位对照 |
| `scripts/wave-c4-governed-process-acceptance.mjs` | 时间线 + isolation debug + host rustup |
| `scripts/lib/c4-governed-process-acceptance.mjs` | approval 等待失败 dump |

## 10. 合规自检

- [x] 只跑授权 E2E 命令  
- [x] 未 git add/commit/stash/reset/clean/checkout  
- [x] 未 taskkill / 批量杀进程；清理走 harness supervisor  
- [x] 未动 `apps/**` / `crates/**` / git acceptance / c4-git-acceptance 证据  
- [x] 未读 auth/token  
- [x] 默认未修产品；仅 harness 取证 + 可跑性（rustup）  
- [x] 未复现则写明，不编根因  

---

**`DEBT-D-RED-CAPTURE-COMPLETE`**
