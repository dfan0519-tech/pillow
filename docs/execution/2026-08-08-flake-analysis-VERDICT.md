# Pillow 四个并发敏感 flake 逐项结论

日期：2026-08-08

仓库：`<repo>`

分析方式：只读静态追踪。未运行测试、构建或 E2E，未修改产品代码。
证据口径：`高` 表示源码已闭合到唯一或近唯一机制；`中` 表示机制真实存在且能解释现象，但缺少失败现场中的决定性观测；`低` 表示只有候选链，不能归因历史失败。

## 1. `cancel-tree`

### 判定机制

最强机制是 harness 在批准后、点击取消前执行无超时的同步 Windows Management Instrumentation（WMI）扫描。高负载若让该扫描阻塞跨过产品固定 60 秒进程时限，桌面后端会继续运行并把 run 终态化。Node 驱动恢复后，`composer-cancel` 已按产品状态机合法卸载，随后对它的 60 秒等待只能超时。

证据链：

1. 场景在批准后确认 cancel 可见：`scripts/wave-c4-governed-process-acceptance.mjs:1267-1305`
2. 真正调用 `cancelOwningRun` 前，场景先做最多 5 秒的 child-marker 轮询：`scripts/wave-c4-governed-process-acceptance.mjs:1307-1339`
3. 每次 marker 查询都进入 `findMarkedNodeProcesses`：`scripts/wave-c4-governed-process-acceptance.mjs:426-433`
4. 该函数同步执行 `spawnSync("powershell.exe", ... Get-CimInstance Win32_Process ...)`，options 没有 `timeout`：`scripts/wave-c4-governed-process-acceptance.mjs:291-305`
5. 外层的 5 秒 `performance.now()` deadline 无法中断一次正在执行的 `spawnSync`。因此“5 秒轮询”实际上没有 5 秒硬上限：`scripts/wave-c4-governed-process-acceptance.mjs:1315-1329`
6. 产品进程时限固定为 60 秒：`crates/tool-executor/src/process.rs:77-84`；runner 用单调 `Instant` 独立计时并在到期时清理：`crates/process-executor/src/lib.rs:114-139`
7. fake ACP 将 process cancel/timeout/failure 结束为 cancelled prompt：`crates/adapter-grok-acp/src/bin/fake_acp_agent/scripts.rs:937-978`；adapter 随后发 terminal：`crates/adapter-grok-acp/src/prompt.rs:204-237`
8. terminal event 将 run 映射为 `completed`、`failed` 或 `cancelled`：`apps/desktop/src-tauri/src/conversation_events.rs:149-155`、`apps/desktop/src/features/conversations/applyRuntimeEvent.ts:324-344`
9. 只有 `pending | running | cancelling` 算 active：`apps/desktop/src/features/conversations/conversationReducer.ts:191-197`；`Workbench` 由此计算 `canCancel`：`apps/desktop/src/features/workbench/Workbench.tsx:219-227`
10. `canCancel` 是按钮唯一 DOM 闸门：`apps/desktop/src/features/workbench/Composer.tsx:160-172`；驱动恢复后又可等待 60 秒：`scripts/lib/c4-governed-process-acceptance.mjs:894-973`

这条机制与已知绿路径不冲突。WMI 正常返回时，批准到取消只需约 600 到 780 ms；候选机制要求的是罕见长尾，而不是普通路径中 cancel 短暂闪断。

### 置信度

**中，约 0.80。** 调用顺序和无界阻塞点均已由源码确认，且能完整解释“批准后曾可见，进入取消 helper 时永不再见”。但历史 RED 没有 WMI 耗时、`msApproveToCancelStart` 或 terminal 状态 dump，故不能写成已证实根因。

### 可证伪预测

- 若机制成立，下一次同型 RED 必须显示：`after-approve` 时 cancel 可见，随后 `msApproveToCancelStart` 接近或超过 60,000 ms，进入 `cancelOwningRun` 时按钮 count 为 0，run 已 terminal 或 long-tree 子进程已被超时清理
- 在批准后、取消前确定性阻塞驱动超过 60 秒，同时让桌面进程继续运行，应稳定得到同型失败
- 将 marker readiness 改成非 WMI 的确定性信号，并在批准后立即取消，应消除这个特定失败窗口
- 强反证是：RED 中批准到取消入口少于约 5 秒，当前 conversation 未切换，run 仍为 active，但按钮 count 已变为 0。出现该证据后，应转查选择态或 reducer 投影，而不是 WMI

### 证据不足项

- 没有历史 RED 的阶段 timeline 或 CIM 调用耗时
- 不知道长尾来自 WMI 服务、PowerShell 启动、系统负载还是其他宿主阻塞
- 外层 supervisor 也会做 WMI 快照，但它运行在父 Node 进程中；目前只能判为系统负载放大器，不能判为直接阻塞本场景驱动

## 2. `two-project-isolation`

### 判定机制

历史失败的精确触发点**证据不足**。源码中存在一个能产生同型症状的真实 lost-wakeup 机制：renderer runtime listener 异步注册但没有 ready 合约；proposal requested 只发一次；事件若在 listener-ready 前丢失，或到达时 selected-conversation ownership 暂时不成立，则 token 不递增。若同 scope 的首次 durable list 又发生在 reserve 之前，`Workbench` 没有 polling 或 catch-up，`command-approval-bar` 会永久缺席。

证据链：

1. 后端先将 runtime event 与 run status 持久化，再只向 renderer emit 一次；emit 失败不会回滚或重放：`apps/desktop/src-tauri/src/conversation_service/pipeline.rs:136-184`
2. Tauri gateway 的 `subscribe` fire-and-forget 调用 `listen`，立即返回 unsubscribe；没有 ready promise，注册失败也只写 console：`apps/desktop/src/lib/tauriConversationGateway.ts:1106-1133`
3. App 收到 process event 后，仅当 event conversation 恰好是当前选中且归属 active project 时才递增 `execProposalRefreshToken`：`apps/desktop/src/App.tsx:222-258`
4. `Workbench` 只在 project、conversation 或 refresh token 变化时 re-list；失败时清空状态，没有 retry 或 interval polling：`apps/desktop/src/features/workbench/Workbench.tsx:366-438`
5. list 结果还需是当前 conversation 的 pending process proposal，才能进入 visible arbitration：`apps/desktop/src/features/workbench/Workbench.tsx:273-329`
6. `showProcessBar` 是最终挂载条件：`apps/desktop/src/features/workbench/Workbench.tsx:657-664,775-784`
7. 场景可能在 `prompt-a`、`prompt-b` 或 `restore-b-before-reject` 三处等待同一 bar；历史 RED 没有 phase dump，无法区分：`scripts/wave-c4-governed-process-acceptance.mjs:1477-1486,1544-1552,1581-1605`

这不是 60 秒 proposal expiry。process proposal TTL 是 600 秒：`crates/kernel-policy/src/process.rs:10-11`。场景也使用独立 `app-data-iso`、WebView profile 和 Grok profile：`scripts/wave-c4-governed-process-acceptance.mjs:1999-2044`，没有源码证据支持跨项目共用 SQLite 文件。

### 置信度

**中：机制存在；低：它就是历史 RED 的触发点。** 单次事件、异步 listener 和无 catch-up 的组合已由源码确认。缺失的 RED phase、native proposal list、listener-ready 和 token 记录，使历史归因不能超过候选级。

### 可证伪预测

- 若机制成立，RED 时 active project、selected conversation 和用户消息应正确，DOM bar count 为 0，但同 conversation 的原生 `list_exec_proposals` 应返回 `pending_approval`
- 该失败前应能看到 listener 尚未 ready、requested event 未被观察，或 event 到达时 ownership predicate 为 false；之后没有 token bump 或 identity change 触发 re-list
- 确定性延迟 `listen()` 完成，并让 proposal event 落在注册前，应在当前实现稳定留下 durable pending 且无 bar；增加 ready 和一次 durable catch-up 后应稳定恢复
- 若 RED 时 native list 为空，机制被证伪，问题位于 fake ACP、operation gate、reserve 或 runtime pump 上游
- 若 listener 已 ready、event 已收到、token 已递增、native list 有 pending，但 bar 仍不出现，机制被证伪，应转查 DTO normalization、list error 或 approval arbitration

### 证据不足项

- 历史失败发生在哪个 approval phase 未知
- 没有失败时的 native pending count、listener-ready、event seen 或 refresh-token delta
- `relistApprovals` 会吞掉具体错误并清空 bar：`apps/desktop/src/features/workbench/Workbench.tsx:375-397`；一次 transient list/DTO error 也能制造相同症状

## 3. `storage_failure_after_live_never_fabricates_terminal`

### 判定机制

这是同一测试内部的双发起者竞态，不是其他并行测试直接污染全局计数器。

测试主线程与它自己创建的 waiter 都可以在强制过期后判定“需要 transition”。两条路径都在 coordinator 锁内做资格判断，释放锁后才写 SQLite。故障开关打开时，两次 `transition` 都会先把同一 repository 的计数器加一，再各自失败。失败的 `left: 4, right: 3` 正好是 snapshot 后发生两次 attempt。

证据链：

1. 测试创建 waiter，取得 live proposal，读取 counter，开启 fail-writes，强制过期，再由主线程 list：`crates/runtime-host/tests/governed_write_bridge.rs:1010-1060`
2. `write_attempts` 和 `fail_writes` 是 `SqliteWriteProposalRepository` 实例字段；`open` 时各自初始化：`crates/storage-sqlite/src/write_proposal.rs:175-198`
3. `begin_mutation` 先 `fetch_add(1)`，后检查 `fail_writes`：`crates/storage-sqlite/src/write_proposal.rs:201-219`
4. 主线程 list 进入 `reconcile_expired_for_owner`，在锁内收集 `to_expire`，离锁后调用 transition：`crates/runtime-host/src/governed_write.rs:1227-1265`
5. waiter 最多每 250 ms 自醒；看到 expiry 后先释放 coordinator 锁，再走 `finalize_expire` 和同一 transition helper：`crates/runtime-host/src/governed_write.rs:725-786,821-874`
6. `force_live_expiry_for_tests` 只改 owner 匹配的 live entry 时间，不做持久化：`crates/runtime-host/src/governed_write.rs:1418-1431`
7. fixture 每次使用独立 `TempDir/pillow.db`：`crates/runtime-host/tests/governed_write_bridge.rs:70-82`；host 每实例生成 owner UUID：`crates/runtime-host/src/host/mod.rs:155-165`

快照前的计数也可解释：host recovery 的 `recover_interrupted` 一次，proposal reserve 一次；因此 `attempts_before = 2`。对应入口为 `crates/runtime-host/src/governed_write.rs:1385-1398`、`crates/storage-sqlite/src/write_proposal.rs:825-829` 和 `crates/runtime-host/src/governed_write.rs:580-584`。

### 置信度

**高，约 0.95。** per-instance ownership 排除了正常跨测试访问；两条可达 transition 链、解锁位置和精确 `+2` 全部吻合。唯一缺口是没有失败运行的 thread-tagged mutation trace。

### 可证伪预测

- 即使 `--test-threads=1` 且只过滤这一项，只要调度恰好让 waiter 与 list 同时越过 expiry 判断，仍可出现 delta 2；并行不是必要条件
- 在两条路径离开 coordinator 后、进入 transition 前加 proposal-scoped barrier，应稳定得到同一 proposal 的两条 transition attempt 和 delta 2
- 用 channel 暂停 waiter，直到主线程完成失败 list、counter 断言和 live quarantine，再释放 waiter，应稳定得到 delta 1
- 若 tagged trace 显示第二次 attempt 不是 waiter 对同一 proposal 的 `transition`，本机制被证伪
- 若暂停 waiter 后仍得到 delta 2，则存在主线程链内重试或第三调用点，本机制被证伪

### 证据不足项

- 没有失败现场的 thread id、proposal-scoped mutation kind 记录
- 当前测试注释中的“恰好一次”究竟是“单次 list 调用不重试”，还是“全 proposal 所有并发发起者全局只尝试一次”，产品契约需要总控明确

## 4. `slow_output_is_visible_live_and_reopens_after_waiter_consumes_terminal`

### 判定机制

高概率是测试自身缺少阶段握手，并从错误起点消耗约 1 秒轮询预算。当前源码不支持正常路径中的 lost-wakeup 或 terminal-consume 丢 chunk。

证据链：

1. slow child 只有在 Node 脚本真正启动后才先写 marker，再等待 700 ms 退出；这 700 ms 不覆盖此前的 approve、claim 和 Windows spawn：`crates/runtime-host/tests/governed_process_bridge.rs:142-149`
2. 测试等到 pending 后 spawn decide 线程，随后立即做 `100 × 10 ms` 的 output 轮询；它不先等 approve、claim、child resume 或 reader ready：`crates/runtime-host/tests/governed_process_bridge.rs:599-639`
3. 最后一轮未命中后仍 sleep 10 ms，却没有最终重检：`crates/runtime-host/tests/governed_process_bridge.rs:624-639`
4. decide 路径还需持久化 approve、claim、构造环境并执行 runner：`crates/runtime-host/src/host/process.rs:1401-1499`、`crates/tool-executor/src/process.rs:776-870`
5. Windows runner 要 suspended spawn、识别线程、绑定 Job Object、创建 reader 后再 resume：`crates/process-executor/src/lib.rs:114-121,188-259`、`crates/process-executor/src/job_windows.rs:105-134`
6. reader 读到字节后同步调用 observer；observer 在 coordinator 锁内追加 chunk：`crates/process-executor/src/output.rs:149-175`、`crates/runtime-host/src/host/process.rs:1138-1160`
7. waiter 消费 terminal 时，在同一 coordinator 锁内把 live entry 的 chunks 复制进 retained cache：`crates/runtime-host/src/host/process.rs:887-923,1104-1135`
8. `open_process_output` 同时读取 live entry 和 retained terminal cache，两条路径都返回 `live_retained = true`：`crates/runtime-host/src/host/process.rs:1249-1288`
9. 同一 integration binary 还有一个循环启动 33 个 Node process 的测试，默认并行时会放大 spawn、reader 和全局 coordinator 争用：`crates/runtime-host/tests/governed_process_bridge.rs:663-689`

因此，如果 marker 在旧的 100 次查询之后才被 observer 发布，测试会在 `live.expect("live output")` 失败。terminal 快速完成不会正常地让 marker“消失”，因为 retained cache 会接住它。

### 置信度

**高，约 0.88。** 测试预算、启动链和 retained-cache 顺序均已闭合，且与“高并行失败、串行通过”吻合。仍缺一次失败的阶段时间戳，因此不能确定主要延迟发生在 SQLite、Windows spawn、Node 启动还是 reader 调度。

### 可证伪预测

- 高并行失败样本应显示首次 `append_live_output_chunk` 晚于第 100 次 `open_process_output`
- 和 33-process cache test 同 binary 并行时失败率应高于 exact serial；这只验证负载放大，不单独证明根因
- 以 `Started` channel 确认 durable claim，并让 child 写 marker 后等待 test-controlled sentinel，再做 condition wait，应消除该失败窗口且真正证明“terminal 前可见”
- 若 marker 已在旧轮询预算内进入 `append_live_output_chunk`，但之后的 `open_process_output` 始终看不到它，本机制被证伪，应转查 ownership、projection 或锁可见性
- 若 waiter 最终 receipt 和 retained cache 也缺 marker，本机制被证伪，应转查 pipe reader/capture，而不是预算

### 证据不足项

- Worker 报告没有保留阶段耗时或确切 panic 行
- 全局 process coordinator 锁等待时长没有测量
- 测试在第 639 行 panic 时不会执行后续两个 join 和 shutdown：`crates/runtime-host/tests/governed_process_bridge.rs:639-659`；这会污染失败后的后续负载，但不是首次失败的原因

FLAKE-ANALYSIS-COMPLETE
