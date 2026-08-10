# Pillow 总控交接手册(2026-08-10)

工作目录:`<repo>`
分支 / HEAD:`main` / `9b2abba`(**本地 = 远程**)
远程:`https://github.com/dfan0519-tech/pillow.git`(**private**)

> 本文件取代 `docs/handoff/2026-08-08-pillow-controller-handoff.md`。
> 旧文件描述 C4 验收期,已过时,**不得作为真相源**。
> 本机运维细节(槽位、容器、Keeper Bridge、claim/release)已移到
> **`.superpowers/RUNBOOK.md`**(gitignored)。开工前先读它。

---

## 1. 一句话状态

**权限规则那批已跑绿并提交**:门 OK,E2E **10/10**。

这一天最重要的产出不是代码,是**发现了两个让整个产品无法真用的事实**:

1. §3 —— 真实代理在 Pillow 里**没有手**(推翻了此前的优先级判断)。
2. §4.2 —— 权限核心把**整条审批链路打死了**,而门是绿的。
   两个 Worker 各自正确,门查不出来,**只有 E2E 会红**。

---

## 2. 用户已明确的意图(按时间)

1. **做工业级产品**,做框架让更多人用。
2. **上传开源社区(GitHub),先不走微软商店。** 发布不急,**他自己还没用过**。
3. 🔴 **公开时不上传提交历史** —— 测完之后上传一个可用版本。
   所以清理只需让**工作树**可发布,**不需要历史改写**。
4. 🔴 **工具调用尽量复用 grok build,不要造轮子。**
   (这条直接否掉了我们自建的 `pillow/*` 协议,见 §3。)
5. 终态工作流:我出计划 → Pillow 派活给各角色 → 我验收。
   先要能**单任务执行**,再加槽位,再多角色。

---

## 3. 🔴 最重要的发现:真实代理在 Pillow 里没有手

**已逐条查证,不是推测。**

`crates/adapter-grok-acp/src/rpc/methods.rs` 的 `plaintext_agent_profile()`:

```rust
"tools": [],
"disallowedTools": ["read_file","write_file","bash","grep_search",
                    "run_terminal_command","search_replace","web_*","lsp", ...]
```

- 禁用原生工具是**对的** —— 原生 `write_file` 会绕过整个治理层。
- 但治理版工具 `pillow/write_tool` / `pillow/process_tool` 虽然在
  `prompt.rs:348,353` 被处理,**全仓只有自建 fake agent 会发出它们**。
- 净效果:**真实 Grok 进来只能说话**。C3/C4 那整套提案/审批/收据/隔离,
  **只有 fake 代理驱动过**。

⚠️ **我此前告诉用户"最大阻塞是批准粒度、一个任务要点十几次"——那是错的。**
没有工具就没有提案,没有提案就没有批准。我从策略层代码推结论,
没查代理能不能触发提案。**教训:推优先级前先确认链路真的通。**

### 3.1 为什么调不到(`docs/research/2026-08-10-tool-exposure-reuse.md`)

- **ACP 没有"客户端任意工具目录"这个字段。**
- `agentProfile.tools` **只过滤内置工具**,不声明新工具。
  所以 `tools: []` 的含义是"内置一个都不给",不是"这里有空列表"。
- **给 Grok 加新工具的标准机制只有 ACP `session/new|load.mcpServers`** ——
  我们**已经在发这个字段,值是 `[]`**。
- 自建 `pillow/*` JSON-RPC:真实 Grok 永远不会调用。**排最后**。

### 3.2 路线(五步)

| 步 | 内容 |
| --- | --- |
| 1 | host 起 **loopback HTTP MCP 端点**,只暴露治理工具 |
| 2 | `session/new` 传 `mcpServers: [{type:"http", url, headers}]` |
| 3 | **权限映射放行 MCP 工具**(现在 `decide_permission` 全拒) |
| 4 | MCP handler 接进既有治理端口 → 提案 → 审批 → 收据 |
| 5 | **实测抓包**:真实 Grok 会不会用(任务 #19) |

**HTTP MCP 可跑在 host 进程内**,不需要额外二进制、不需要第二个权威。
`prompt.rs` 的规范化 / 响应整形 / 治理调用**全部可复用**,只换线上入口。

🔴 **隐藏拦路虎**:若 Grok 对 MCP 工具仍发 `session/request_permission`,
我们现在的**全拒策略会在 MCP 跑起来之前就拒掉**。
"接好了却被自己拒绝"——**只有实测能排除**,且正好是权限核心那块代码。

🔴 **不许走的捷径**:重开原生 `write_file`/`bash`、指向不受限的文件系统 MCP、
把 `tools: []` 当成"工具已暴露"。

---

## 4. 未提交的 49 处改动(**必须先跑门 + E2E**)

| 来源 | 内容 | 状态 |
| --- | --- | --- |
| Codex(权限核心) | `permission_rules` 表 + 求值 + 收据 + Tauri 命令 | 报 `BLOCKED-BY-CONCURRENT` |
| grok3(权限 UI) | 规则列表、审批条"总是允许"、时间线自动放行标记 | `COMPLETE` |
| **总控** | **契约接缝修复**(见 §4.1) | 编译 + typecheck 已过 |

### 4.1 我修的接缝(**这是我的失误**)

契约里我写"**决定/收据 DTO** 增 `auto_approved_by_rule`" —— **有歧义**。
Codex 放在提案视图,UI 期望在**时间线事件载荷**。

**UI 的理解才对**:要求是"自动放行必须在时间线可见",而时间线由**事件**构建;
放在提案视图上,replay 就看不见了。

修法:
- 归属类型**下沉到 `agent-protocol`**,`runtime-host` 改为**类型别名** ——
  **不复制第二份**(这条不变量已害本项目栽过多次)
- `ProcessProposalDecidedPayload` / `WriteProposalResolvedPayload` 各加可选字段
- 5 个写入侧 + 3 个进程侧发射点逐个填值
- `resolved_payload` 用**显式参数而非内部查表**:有些调用点 live entry 已消失,
  **静默发 `None` 会让规则决策与人工决策在时间线里无法区分**

⚠️ **下次写契约:说清是"命令返回的 DTO"还是"事件载荷",不要写"DTO"。**

### 4.2 🔴 权限核心把审批链路打死了(**门全绿,E2E 才红**)

**症状**:真实应用里**审批条永远不出现**。时间线显示"待批准",批准条 count=0。
即 C3/C4 整套治理审批**在真机上完全不可用**。

**根因**:Codex 给 `ExecProposalDto` / `WriteProposalDto` 加了
`rule_subject`(必有)和 `auto_approved_by_rule`(可选)。
而渲染层网关 `tauriExecGateway.ts` / `tauriWriteApprovalGateway.ts`
用**封闭键白名单**(`only()` / `hasOnlyKeys()`)校验宿主载荷,**见到生键就抛**。
→ 每次 list 都抛 → `pendingProcessProposal` 置空 → 条永远不渲染。

那道白名单**不是 bug,是防线**(挡宿主把 grant / 原生路径塞进 UI)。
所以:**给宿主 DTO 加字段 = 破坏性变更**,必须同一次改动里放宽渲染层白名单。
已写进 `AGENTS.md` 工程规则。

**为什么门查不出来**:
- Rust 侧测试不跑 TS 校验器;
- TS 侧测试的 fixture 是**自己造的**,自然不含新键;
- `c4-gateway-contract.test.ts` 原本**只断言"拒绝"** ——
  校验器把一切都拒了它照样绿。**只断言拒绝的测试套件是假绿。**

**修法**:`ruleSubject` / `autoApprovedByRule` 进两个白名单并透传;
校验器**下沉到 `permissionRulesGateway.ts` 一份**(不复制第三份);
视图类型里 `ruleSubject` 设为**必填** —— 这样以后 fixture 漏了**编译期就红**;
补正向契约测试,并用**变异探针**验证过(把白名单改回去,新测试确实红)。

⚠️ **这仍然是我的失误**:我把宿主 DTO 和渲染层拆给两个 Worker,
没说明"加宿主字段必须同步渲染层白名单"。**和 §4.1 是同一类接缝。**

---

## 5. 权限规则(设计已定,核心在途)

设计:`docs/superpowers/specs/2026-08-10-permission-rules-design.md`
**照搬 Grok 的 `--allow`/`--deny`,不自创模型**(用户指示)。

四条不许破:

1. **自动批准不跳过账本** —— 收据必须能答"**哪条规则、规则当时内容**"
2. **project 作用域只能收紧** —— global 可 allow+deny,project **只能 deny**
3. **绝不从项目目录读配置授权**(否则打开恶意项目 = 它给自己授权 `Bash(*)`)
4. **无规则命中 = 要求确认**;deny > allow

**匹配主体**(最易错):`program` + 空格拼接 argv,
所以 `Bash(cargo*)` 命中 `cargo --version`。

⚠️ **Grok 的前缀匹配没有词边界**:`Bash(git)` 会顺带授权 `gitleaks`。
Pillow 两道防线:**程序目录表仍然生效**(规则不能引入目录表外的二进制)、
**不接受 shell 串联当单个主体**。UI 必须提示这一点。

---

## 6. 下一步(按顺序)

1. ~~跑门 + E2E,提交推送~~ **已完成**(门 OK,E2E 10/10)。
2. **P1 = 任务 #19**:**顺序要调**。手册原来写 1→5 线性推,
   但第 3 步(放行 MCP 工具)那里 `decide_permission` 现在**无条件全拒**
   (`map/permission.rs:176`,已核实)。先按 1→4 建完再到第 5 步实测,
   可能白干四步。**改成先做决定性探针**:一个丢弃式 MCP server 只暴露
   一个空操作工具,经 `session_new_params` 的 `mcpServers` 传给真实 Grok
   并抓包 —— 一份工作量同时回答"会不会列出/会不会调用/会不会要权限"。
3. 任务 #20:权限规则**目前是两个独立 SQLite 连接**(host 一个、
   Tauri 命令层一个),"UI 加规则 → 下一个提案自动放行"这条路
   **从未被测过**。
3. 之后:P0 启动命令 → 我亲自试跑一个真任务 → **那时才是外审时点**
   (简报已备好:`docs/references/external-audit-brief.md`,**自带上下文,可直接粘出去**)。

⚠️ 用户明确:**审计放在"把工作推到他能上手之前的卡点"**,不是现在。

---

## 7. 台账(ADR-0004 的闭环,已在用)

```bash
node scripts/run-cargo.mjs run -q -p keeper-knowledge-indexer --bin knowledge-indexer -- \
  ledger status --db .superpowers/ledger/pillow.db --project pillow
```

当前:**在途 3、已完成 8**。子命令:`task dispatch` / `task complete` /
`verdict record` / `ledger status` / `ledger stale`。

🔴 **只存槽位标签,不存租约 id**(密钥闸拦 `grok-lease-*`)。

⚠️ **我犯过一次**:上一轮我在消息里写"已记进台账"**但没记**,派完就去写下一条消息了。
这正是 ADR-0004 预言的漂移。**规程:派完立刻记,不写进消息当口号。**

---

## 8. 我这两天踩的坑(**避免重复**)

1. **从代码推结论,不验证链路** → §3 那个错误优先级。
2. **契约写含糊** → §4.1 两个 Worker 各自实现得对却对不上。
3. **把"限制"当固定条件**:门抢 `target/` 让我串行派单一整天,
   而 `CARGO_TARGET_DIR` 十分钟就能解。**先问:这是物理限制,还是我没查。**
4. **绿门源于扫描器看不见**:`check-secrets.mjs` 曾只扫已跟踪文件,
   机器路径一提交就红。**先问"扫描器能不能看见它"。**
5. **同槽两实例 = 同一账号并发**。claim 后**必须查 `slotId`**,
   用占位租约顶掉再释放。
6. **`docs/handoff/` 里的运维内容已移走** —— 别再往回写。
7. **`cargo check` 不编译 `#[cfg(test)]`**。我说过"workspace check 通过"
   就以为收口了,门一跑 `cargo test` 立刻炸出 9 个构造点。
   **收口用 `cargo test --workspace --no-run`,不要用 check。**
8. **管道吃掉退出码**:我跑 `pnpm desktop:test:e2e 2>&1 | tail`,
   读到的是 `tail` 的 0,**把红的 E2E 当成绿的报了出去**。
   **长命令一律 `> log 2>&1; echo EXIT=$?`,不要管道接 tail。**
9. **只断言"拒绝"的测试套件是假绿**(§4.2)。写校验器测试必须钉正向。
10. **E2E 失败原来看不到渲染层报错** —— app 的 stdout/stderr 是空的,
    IPC 异常被吞成空结果,只剩"条没出现"。已给
    `wave-c4-governed-process-acceptance.mjs` 加 console/pageerror 采集,
    加完**一次就定位到** `Failed to list governed approvals.`。
    其余 wave 脚本还没加,值得补。

---

## 9. 环境注记

- 详细操作(槽位、容器、启动命令、Codex 铁律)见 **`.superpowers/RUNBOOK.md`**。
- 🔴 **Codex 单必须"先落报告骨架再跑门"** —— Step 2 那次中断把实施者视角永久抹掉了。
  Step 4/5/6 改了之后有效。
- 并行:**同时只跑 1 个写码单 + N 个只读单**,或多个写码单各自
  `CARGO_TARGET_DIR`;门仍是 workspace 级,所以 Worker 会看见邻居半成品 ——
  让它报 **`BLOCKED-BY-CONCURRENT`**,别去修别人的文件。
- E2E 约 25 分钟,**总控独占**;重负载下会间歇挂(任务 #10)。

---

## 10. 交接完成状态

- 权限规则批次**已跑绿并提交**:门 OK,E2E **10/10 pass / 0 fail**
- 在途 Worker:0;Codex 进程:0
- 任务 #19(P1)已建,含完整依据,**执行顺序已按 §6 调整**
- 任务 #20 已建:权限规则双连接 + 自动放行链路未测
- 外审简报已备好,**等 P1 之后再用**
- 磁盘:清掉了 17 个槽位 `CARGO_TARGET_DIR`(**70 GB**)和
  3048 个 Rust 单测泄漏的临时夹具目录。后者是产品缺陷,已单独开单。
