# Pillow 总控交接手册(2026-08-11)

工作目录:`<repo>`
分支 / HEAD:`main` / `729c11a`(**本地 = 远程**)
远程:`https://github.com/dfan0519-tech/pillow.git`(**private**)

> 本文件取代 `docs/handoff/2026-08-10-pillow-controller-handoff.md`。
> 那份写于边界尚未成立时,**§3「真实代理没有手」已被推翻**,不得作为真相源。
> 本机运维细节(槽位、claim/release、Codex 沙箱限制)在
> **`.superpowers/RUNBOOK.md`**(gitignored)。开工前先读它。

---

## 1. 一句话状态

**代理现在有两只受管的手:写文件、跑命令,每一次都要人工批准。**

自 `9b2abba` 起 17 个提交,HEAD `7b2e50c`。门 OK,**E2E 10/10**
(含 shell 场景,`7b2e50c` 那次是它们第一次真正执行)。

规则也能用了:`Bash(cargo*)` **不会**放行 `cargo test; rm -rf x` ——
allow 逐段合取,拆不动就要求确认。

⚠️ **有一个 Worker 在途**,见 §7。

---

## 2. 这一天真正发生的事

我们**以为**有一个治理层,**实际上没有**。三条独立的失效叠在一起:

| # | 事实 | 后果 |
| --- | --- | --- |
| 1 | `_meta.agentProfile` 缺必填 `name`/`description` → 上游 `from_json` 失败 → **log 一行然后静默回落全工具集** | 原生工具从未被禁用过 |
| 2 | `tools: []` 在上游是 **inherit all**,不是"全关"(README 与自己的代码矛盾) | 即使解析成功也没关掉 |
| 3 | 禁用名写错(真实 short id 是 `write` / `run_terminal_cmd`) | 即使前两条对了也拦不住 |

**修的方向也错过一次:** 我先建议"那就先不给进程能力"。用户否掉了,理由正确 ——
**不要因为自建模型接不住就砍功能,去看市面上怎么解的。**
上游 `policy.rs` 里就有答案(拆段 + allow 合取 + fail-closed),**轮子在我们脚下**。

---

## 3. 边界现状(**改任何一条前先读 §4**)

```json
// crates/adapter-grok-acp/src/rpc/methods.rs
tools:            ["ask_user_question", "write", "run_terminal_cmd"]
disallowedTools:  ["search_tool", "use_tool"]
clientCapabilities: { fs: { readTextFile: true, writeTextFile: true }, terminal: true }
mcpServers:       []
```

| 面 | 机制 |
| --- | --- |
| **写** | 原生 `write` → `fs/write_text_file` → 既有 `execute_write` 治理链 |
| **进程** | 原生 `run_terminal_cmd` → `terminal/*` → 同一条链,**每条命令人工批准** |
| **MCP** | **切断**(禁两个元工具);环境侧 MCP 关不掉,这是唯一的杠杆 |

**allowlist 管原生工具,fail-closed;denylist 只关 MCP 那一扇门。**
两者职责不同,代码里有测试钉死:denylist 出现第三个名字就红。

---

## 4. 🔴 三个"半开陷阱"(**同构,都无声**)

形状一致:**给了手,却没开通往宿主的那条线** → 代理自己动手,宿主全程看不见。

| 陷阱 | 条件 | 后果 |
| --- | --- | --- |
| LocalFs | allowlist 有 `write` ∧ `writeTextFile=false` | 子进程直接写盘,`agentRequests` 为空 |
| LocalTerminal | allowlist 有 shell 工具 ∧ `terminal=false` | 代理自己起进程,宿主看不到 `terminal/create` |
| ambient MCP | 元工具未禁 | 用户环境的 MCP server 可达(实测调通过账号网关 `tasks__create`) |

`methods.rs` 里有**两条耦合测试**钉住前两条(变异证据齐)。

🔴 **注意 fs 与 terminal 的判定式不同**:
`use_acp_fs = read ∧ write`(**双开**),`terminal` 是**单个 bool**。
别以为一样。

---

## 5. 🔴 契约接缝:同一个坑栽了三次

三次表现**完全一样 —— 某个东西悄无声息地不出现,而门是绿的**:

1. 宿主加 `rule_subject` → 渲染层封闭键白名单不认 → 每次 list 都抛 → **审批条全死**
2. 归属放在提案视图而非事件载荷 → replay 看不见
3. 宿主对 shell 发 `rule_subject: ""`(合法)→ 我写的校验器要求非空 → **shell 审批条全死**

**规矩:**
- 说清是**命令返回的 DTO** 还是**事件载荷**,不要只写"DTO"
- 宿主 DTO 加字段 = **破坏性变更**,必须同批放宽
  `apps/desktop/src/lib/tauriExecGateway.ts` 的 `only(value, proposalKeys)`
- **只断言"拒绝"的测试是假绿** —— 校验器把一切都拒了它照样绿

---

## 6. 实盘探针是唯一可信的边界证据

```bash
PILLOW_MCP_PROBE=1 node scripts/probe-mcp-tool-exposure.mjs
# 窄跑:PILLOW_MCP_PROBE_ONLY=<id>,以验收判据为准,"not run" 永不算过
```

不设环境变量时输出 `skipped` 返回 0,**永不进门**。

**关键变体**(改边界后必跑):

| id | 期望 |
| --- | --- |
| `shipped-profile-write` | 写**到达客户端**且未落盘 |
| `shipped-profile-mcp` | **零工具调用**,回答 `NO_MCP` |
| `acp-fs-localfs-trap` | **必须仍然落盘** —— 已知负例,是哨兵 |
| `shell-no-handler` | `terminal/create` 到达客户端且未本地起进程 |

🔴 **判据是合取:必须"到达客户端"且"未被代理自行执行"。**
只看"没执行"会把**功能坏了**读成**治理生效** —— 写方向差点这样过关。

⚠️ 陷阱变体**必须一直落盘**。哪天它不落盘了,说明上游语义变了。
🔴 `shell-no-handler` 验证的是**供应商的性质,不是我们代码的性质**。

---

## 7. 在途 Worker(接手先看)

**codex1 —— shell 命令的逐段「总是允许」**(`.superpowers/round28/`,台账 `3a6ddabe`)。

缺口:S2 让规则对 shell 生效了(`ApprovalPolicy::ShellRules` 逐段合取求值),
**但审批条上没有入口** —— 宿主对 shell 的 `rule_subject` 仍是空串
(`host/process.rs:2244`),所以「总是允许此模式」拿不到主体。
用户只能去设置面板手敲,**最自然的路径(批准时顺手加规则)不可用**。

🔴 **这是一次跨宿主/渲染层的改动 —— 就是 §5 那个栽了三次的接缝。**
契约已在简报里定死:新增可选 `rule_segments`,**拆不动就省略**,
**绝不回退成整串一条规则**(否则 `cargo test && rm -rf x` 会变成一条可保存的 allow)。
验收时**先查两侧是否同批落地**:宿主发的键必须在
`tauriExecGateway.ts` 的 `only(value, proposalKeys)` 白名单里。

已完成并验收的前一批:S2 拆段(变异证据:去掉逐段要求 → 两个测试红,
其中一个同时钉正反两个方向)+ shell → UI 的 E2E 覆盖。

---

## 8. 已知缺口

- ~~E2E 无 shell → UI 覆盖~~ **已补**(`shell-approve` / `shell-reject`,
  且在 spec 的 REQUIRED 列表里 —— 场景缺失会红,不会静默通过)
- **规则有了,入口还没有**:见 §7,在途
- **归属只显示一条规则**:合取可能由多条规则分别放行不同段,
  durable 收据记了完整的逐段快照,**事件载荷只带代表规则**。
  账本是全的,UI 不全。审计要看收据,不要看事件。
- **解释器 deny 集是有限的**:不认识的 wrapper/applet 加上一条过宽的规则,
  是目前最可能的错放路径(S2 报告残留风险 #3)
- **进程不是 OS 沙箱**:治理的是**已验证的上游工具路径**,不声称进程级封闭
- **`search_replace` 故意未暴露**,将来要加必须先实盘证明收敛到同一条客户端 RPC
- E2E 满负载偶发假红(任务 #10);harness 的控制字符缺陷已修

---

## 9. 我的坑位表(**新增的在前**)

1. **对照实验必须只差一个变量。** 我拿"两个 profile 都挡不住写"当决定性证据,
   但它们**都缺必填字段、都没解析成功** —— 我在对比两个同样无效的 payload。
2. **工具名出现在调用列表 ≠ 调用成功。** 我把成功的 `tasks__create` 读成"试了没成"。
3. **数量不等于对应关系。** 补丁脚本断言"3 个声明 / 3 个 catch",数量对上就放行,
   但没验证它们在同一个函数里 —— 留下一个会掩盖真实错误的 `ReferenceError`。
4. **管道吃退出码。** `cmd | tail` 让我**把红的 E2E 报成了绿**。
   一律 `cmd > log 2>&1; echo EXIT=$?`。
5. **`cargo check` 不编译 `#[cfg(test)]`。** 收口用 `cargo test --workspace --no-run`。
6. **Worker 的门结论不可采信。** Codex 沙箱起不动门,交过一次自己看不见的红 clippy。
7. **从代码推断运行时行为** —— 前后栽了三次(批准粒度 → 代理没有手 → disallowedTools)。
8. **别把自建模型当固定条件去砍需求。**

---

## 10. 下一步

1. 两单落地 → 跑门 + E2E → 提交
2. S2 之后:**你可以设"总是允许 `cargo test`"这类规则**,不必每条都点
3. 之后才是外审(简报已备:`docs/references/external-audit-brief.md`)

**用户已可上手**:`target/release/pillow.exe`,启动约 1.6 秒。
