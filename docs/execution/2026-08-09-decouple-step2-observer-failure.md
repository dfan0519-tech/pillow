# Step 2 — process observer-failure fail-closed(总控代写的验收记录)

日期:2026-08-09
实施:Codex CLI(codex1 / Sol,max effort)
验收与本文档:Claude(总控)

> ⚠️ **本文档由总控代写,不是实施者的报告。**
> Codex 会话在 workspace 门运行中途以 exit 127 结束。日志末尾它自己写的是
> "进程未结束,我会继续等实际 exit code,不把无输出当作通过或失败"
> —— 态度正确,只是没等到。
>
> **中断原因未查实。** 排除了额度与鉴权(日志无 402/429/unauthorized)。
> 用户事后指出**可能是别的线程误杀**;这正是 handoff §8.2 记过的危险
> ——「**绝不按进程名杀 codex,会误伤别的会话**」。
> 先前本文把原因写成"轮次耗尽"是**未经证实的推测,已更正**。
>
> 简报要求的报告因此**没有产出**。代码是实的,所以由总控补验证与记录。
> **本文缺少实施者视角的设计说明,这是已知缺口。**

## 1. 缺口(来自 Codex 自己的提案)

`docs/execution/2026-08-08-adapter-decoupling-proposal.md` §8.4 第 2 条:

> process Decided/Started observer failure 的清理语义不完整。
> `host/process.rs` 先把 delivered flag 置位,锁外 callback 后直接 `?` 返回;
> 它没有 Requested 路径 `fail_closed_after_reserve` 的对称清理。

提案 §9 把它列为 **Step 2**,并强调:**"这是明确行为修复,不能藏进 trait 重构"**。

## 2. 修复形态

`crates/runtime-host/src/host/process.rs`(+205 / −27):

- 新增 `fail_closed_after_observer_failure` —— 与 Requested 路径对称的清理。
- `LiveProcessEntry` 增加三个**生产**状态位:
  `observer_delivery_in_progress` / `observer_failure_cleanup_in_progress` /
  `observer_quarantine_deferred`。
  (与 storage single-flight 的 `terminal_transition_in_progress` 同形态:
  修复状态属于生产,不是 mutation hook。)
- `finish_observer_delivery` 负责成功路径的锁外交付收尾。
- `wait_for_durable_observer_failure_terminal`:
  **权威在拿到真实 durable terminal 之前不清除** —— 不伪造终态。

## 3. 回归测试(4 条,`crates/runtime-host/tests/governed_process_bridge.rs` +467)

| 测试 | 锁定什么 |
| --- | --- |
| `decided_observer_failure_before_claim_is_durable_and_restart_safe` | Decided 阶段 observer 失败 → durable fail-closed,重启安全 |
| `started_observer_failure_waits_for_real_terminal_before_clearing_authority` | Started 阶段失败 → **等到真实终态**才清权威 |
| `rejected_decided_observer_failure_preserves_terminal_and_releases_busy_slot` | 保住终态,同时释放 busy 槽 |
| `observer_failure_keeps_run_busy_until_existing_claim_has_a_durable_terminal` | 已有 claim 未终态前,run 保持 busy |

## 4. 🔴 总控的独立验证:变异探针

简报要求 **先 RED、贴出失败输出、再修**。Codex 未交报告,故无 RED 原始输出。

**直接复现 RED 不可行**:同一改动里 `set_pre_claim_hook_for_tests` 的签名
也变了(`()` → `bool`),把 `process.rs` 退回 HEAD 后新测试**编译不过**
(`E0600: cannot apply unary operator ! to type ()`)。

改用**变异探针**,把修复函数体置空:

```rust
fn fail_closed_after_observer_failure(proposal_id: &str, bound: &BoundProcessRun) {
    let _ = (proposal_id, bound);
    if true { return; }   // TEMP probe
    ...
}
```

结果:

```text
置空 → test result: FAILED. 0 passed; 4 failed; 20 filtered out
还原 → test result: ok.     4 passed; 0 failed; 20 filtered out
```

**这比 RED 更强。** RED 只证明"修复前是红的";变异探针证明
**正是这段修复让它们变绿** —— 排除了"测试恰好也被别的改动带绿"这种可能。

探针已移除,文件精确还原(diffstat 与探针前一致:+205 / −27)。

## 5. 门

| 命令 | 结果 |
| --- | --- |
| `node scripts/run-worker-gate.mjs`(合并树:K0d + Step 1 + Step 2) | **exit 0** |
| `cargo fmt --all` | 总控执行(Codex 中断时未格式化) |
| 桌面 E2E | 总控独占运行,见提交信息 |

## 6. 残余风险与缺口

1. **无实施者报告。** 设计取舍、考虑过又放弃的方案、它自己看到的风险,全部丢失。
   代码与测试俱在,但**"为什么这样修"只剩代码本身能说话**。
2. **checkpoint delta 未由实施者记录。** 本次修改了已 checkpoint 的 C4 process
   路径行为(observer 失败时的清理语义)。总控在此声明该变更属于
   **收紧 fail-closed**,不放宽任何治理语义,但这是总控的判断,不是实施者的交代。
3. 提案 §8.4 第 1 条(commit → event sink 非跨存储原子)**未处理**,仍是已知风险。
4. Step 3–11 未开始。

## 7. 流程教训

`codex exec` 会在轮次耗尽时中断,**且可能正好断在门运行中**。
结果是"代码落盘、报告缺失"。

**规程补充**:Codex 单的简报应要求**先落报告骨架再跑门**,
或至少在实施完成、跑门之前先写下设计说明 —— 否则一次中断就永久丢失实施者视角。
