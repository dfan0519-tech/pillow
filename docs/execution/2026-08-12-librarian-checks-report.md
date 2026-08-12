# Pillow 信息管理员可判定检查实现报告

Date: 2026-08-12  
Mode: implementation + local verification  
Scope: `crates/knowledge-indexer/**` and this report only  
Status: **COMPLETE**

## 1. Outcome

新增单一入口：

```text
knowledge-indexer audit --db PATH --project NAME --repo DIR \
  --doc PORTABLE_PATH --now RFC3339 --stale-after-hours N
```

- `--now` 为必填外部时间；库层接收显式 `DateTime<Utc>`，不读取系统时间。
- 输出为 Markdown，总表中四项检查总会各自打印 `PASS` 或 `FAIL`。
- 全部通过返回 `0`；任一检查失败返回 `3`；参数、文件、台账或本地 Git 错误返回 `1`。
- audit 只读文档、台账快照和本地 Git 历史，只向 stdout 输出报告；无网络访问。

## 2. 固定判据

| Check | PASS | FAIL |
| --- | --- | --- |
| `stale_inflight` | 每条仍为 `dispatched` 且尚无 task verdict 的记录，`now - dispatched_at <= threshold` | 至少一条上述记录满足 `now - dispatched_at > threshold` |
| `completed_without_verdict` | 台账的 `completed_without_verdict` 列表为空 | 列表非空；逐条报告 task、slot 和标题 |
| `document_ledger_consistency` | 既有 `check_stale` 返回零 finding | 出现任一既有 finding：在途计数不符、待验收未提及、文档提及幽灵任务 |
| `accepted_verdict_commit` | 每条 `accepted` verdict 的 summary 恰有一个独立空白分隔字段 `commit=<7..64 hex>`，且该前缀在 `git log --all --format=%H` 中唯一匹配 | 缺 marker、marker 非法/多重、Git 中零匹配或前缀多匹配 |

`rejected` 与 `partial` 不声称已经提交，因此不要求 commit marker。Git 只以本地只读命令读取，并设置 `GIT_OPTIONAL_LOCKS=0`。

## 3. 退出码实跑

每次都完整重定向 stdout/stderr 后读取整份日志；未使用管道或 `tail`。

命令形状：

```powershell
cmd.exe /d /c "target\debug\knowledge-indexer.exe audit ... > target\librarian-checks-logs\audit-exit-N.log 2>&1"
$auditExit = $LASTEXITCODE
Write-Output "EXIT=$auditExit"
```

| Exit | 实际触发 | 完整日志 | 观察结果 |
| ---: | --- | --- | --- |
| `0` | 一次性台账中无在途/待验收，handoff 与台账一致，accepted marker 唯一命中当前本地提交 | `target/librarian-checks-logs/audit-exit-0.log` | `EXIT=0`；总结果 `PASS`；四项均 `PASS` |
| `3` | 同一台账配不一致的旧 handoff，产生 3 个 `doc_task_missing_from_ledger` | `target/librarian-checks-logs/audit-exit-3.log` | `EXIT=3`；总结果 `FAIL`；其余三项仍明确 `PASS` |
| `1` | 指定不存在的可移植 handoff 路径 | `target/librarian-checks-logs/audit-exit-1.log` | `EXIT=1`；仅报工具读取错误，不伪装成检查 FAIL |

## 4. PASS / FAIL 双向测试证据

新增测试文件：`crates/knowledge-indexer/tests/librarian_audit.rs`。

| Check | 双向证据 |
| --- | --- |
| 陈旧在途 | `stale_inflight_passes_at_threshold_and_fails_one_second_over` 同时断言边界 PASS 与超 1 秒 FAIL；`stale_inflight_does_not_flag_a_dispatched_task_with_a_verdict` 固定“已有 verdict 不算未验收” |
| 已完成未验收 | `completed_without_verdict_has_explicit_pass_and_fail_states` 断言空列表 PASS、单条 pending FAIL，并检查两种表格输出 |
| 文档与台账 | `document_ledger_consistency_has_explicit_pass_and_fail_states` 断言计数一致 PASS、claimed=2/ledger=0 FAIL |
| 验收与提交 | `accepted_verdict_commit_has_explicit_pass_and_fail_states` 断言唯一命中 PASS、提交不存在 FAIL、缺 marker FAIL，并拒绝 `notcommit=` 假 marker |

补充负控：

- `audit_cli_distinguishes_pass_findings_and_tool_error_without_writes` 在真实二进制层断言退出码 `0/3/1`，并比较 audit 前后 handoff 文本、task rows 与 verdict rows。
- `audit_report_does_not_echo_host_paths_from_handoff_mentions` 注入不可移植提及值，确认输出只保留 finding code 与 mention kind，不回显原文。

## 5. 注入时间与阈值边界

边界夹具固定：

```text
dispatched_at = 2026-08-11T00:00:00Z
threshold     = 24h
now PASS      = 2026-08-12T00:00:00Z
now FAIL      = 2026-08-12T00:00:01Z
```

实现使用 `age > threshold`，所以恰好等于阈值为 PASS。源文件没有 `Utc::now()`、`SystemTime::now()` 或等价隐式时钟读取；CLI 缺少或无法解析 `--now` 时直接返回工具错误 `1`。

## 6. Gates

主门按要求运行：

```powershell
cmd.exe /d /c "cargo test -p keeper-knowledge-indexer > target\librarian-checks-logs\cargo-test.log 2>&1"
$gateExit = $LASTEXITCODE
Write-Output "EXIT=$gateExit"
```

观察结果：`EXIT=0`。完整日志显示：

- library unit: 31 passed
- `indexer_rules`: 20 passed
- existing `ledger_cli`: 8 passed
- new `librarian_audit`: 7 passed
- total: **66 passed, 0 failed**

附加门：

- `cargo fmt -p keeper-knowledge-indexer -- --check` → exit `0`
- `cargo clippy -p keeper-knowledge-indexer --all-targets -- -D warnings` → exit `0`

## 7. 写入边界审计

本单写入集合：

- `crates/knowledge-indexer/src/librarian_audit.rs`
- `crates/knowledge-indexer/src/bin/knowledge_indexer.rs`
- `crates/knowledge-indexer/src/lib.rs`
- `crates/knowledge-indexer/tests/librarian_audit.rs`
- 本报告

边界结论：

- audit 路径只调用 `fs::read_to_string`、`load_snapshot` 与本地 `git log`；没有文档写 API，也没有 `--report` 写文件参数。
- handoff label 在库层和 CLI 层都经过共享可移植路径校验；仓库根只用于内部定位，从不写入报告。
- Git 通过参数数组启动，无 shell 拼接、无网络命令、无可选锁写入。
- 二进制测试确认 audit 前后 handoff 内容及台账 task/verdict 行相同。
- 并行 Worker 的根 manifest、lockfile、desktop settings 与 `crates/cli-detect/**` 变更保持原样；本单未对这些路径实施补丁、回退、暂存或提交，也未为本功能增加依赖。

## 8. 残留风险与不能确定判定的范围

1. **提交存在不等于内容正确。** 当前确定判定的是“accepted 明确声明的 commit 在本地 Git 历史中唯一存在”；不验证该提交是否修改了声明路径、是否包含相应测试、是否为当前 HEAD 的祖先。做到这些需要结构化 changed-path/gate 声明。
2. **commit 关联暂借 summary marker。** 现有 verdict schema 没有 commit 字段，本单又不能改其它 crate；历史 accepted verdict 没有 `commit=` 时会诚实 FAIL。未来应把 commit/range 做成结构化字段，并同步所有宿主 DTO allowlist。
3. **文档检查只覆盖既有机械语法。** 任意自然语言事实、登录态、门结果和代码符号级陈旧不能由 `check_stale` 确定判定；未识别的叙述不等于事实为真。
4. **业务完成仍依赖台账纪律。** 没有 complete 或 verdict 的任务即使业务上已结束，超阈值后仍会 FAIL；本单不猜测工作树或外部进程状态。
5. **SQLite open 的既有行为。** audit 不写 task/verdict/document 行，但复用的 storage open 会执行标准 schema migration。对当前 schema 的测试库未观察到业务行变化；若要求数据库文件逐字节只读，需要 storage 提供专用 read-only open API。
6. **本地 Git 不可用属于工具错误。** Git 缺失、目录不是仓库或 log 失败均返回 `1`，不会降级成 PASS 或检查 FAIL。
7. 本单不做 UI、定时调度、槽位管理、外部事实探针或网络访问。

## Completion signal

`LIBRARIAN-CHECKS-COMPLETE`
