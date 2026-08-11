# Pillow 总控交接手册(2026-08-11)

工作目录:`<repo>`
分支 / HEAD:`main` / `87e067d`(**本地 = 远程**)
远程:`https://github.com/dfan0519-tech/pillow.git`(**private**)

> 取代 `docs/handoff/2026-08-10-*.md`。那份写于边界尚未成立时,
> **§3「真实代理没有手」已被推翻**,不得作为真相源。
> 运维细节(槽位、claim/release、Codex 沙箱限制)在
> **`.superpowers/RUNBOOK.md`**(gitignored)。开工前先读。

---

## 1. 一句话状态

**代理有两只受管的手(写文件、跑命令),槽位可配自有 key 并按端点选模型。**

自 `9b2abba` 起 **25 个提交**。门 OK,E2E **10/10**。

⚠️ **有 1 个 Worker 在途**,见 §7。

---

## 2. 这一天真正发生的事

我们**以为**有治理层,**实际没有**。三条独立失效叠加:

| # | 事实 | 后果 |
| --- | --- | --- |
| 1 | `_meta.agentProfile` 缺必填 `name`/`description` → 上游解析失败 → **log 一行后静默回落全工具集** | 原生工具从未被禁用 |
| 2 | `tools: []` 在上游是 **inherit all**,不是"全关" | 即使解析成功也没关掉 |
| 3 | 禁用名写错(真实 short id 是 `write` / `run_terminal_cmd`) | 即使前两条对也拦不住 |

数月间治理链**只有 fixture agent 跑过**。是**实盘探针**发现的,不是评审。

**修的方向也错过一次**:我先建议"那就先不给进程能力"。用户否掉,理由正确 ——
**不要因为自建模型接不住就砍功能,去看市面上怎么解的。** 上游 `policy.rs` 里就有答案。

---

## 3. 边界现状(**改任何一条前先读 §4**)

```jsonc
// crates/adapter-grok-acp/src/rpc/methods.rs
tools:              ["ask_user_question", "write", "run_terminal_cmd"]
disallowedTools:    ["search_tool", "use_tool"]     // 只关 MCP 两个元工具
clientCapabilities: { fs: { readTextFile: true, writeTextFile: true }, terminal: true }
mcpServers:         []
```

| 面 | 机制 |
| --- | --- |
| **写** | 原生 `write` → `fs/write_text_file` → 既有治理链 |
| **进程** | 原生 `run_terminal_cmd` → `terminal/*` → 同一条链 |
| **MCP** | **切断**(环境侧 MCP 关不掉,禁元工具是唯一杠杆) |

**allowlist 管原生工具(fail-closed);denylist 只关一扇门,不是列举房间。**
代码里有测试:denylist 出现第三个名字就红。

### 权限规则与模式

- **规则**:保守拆段,allow **逐段合取**,任一段 deny 即拒,拆不动就问。
  `Bash(cargo*)` **不会**放行 `cargo test; rm -rf x`。
  批准时可**逐段**存规则(拆不动则不给入口,**绝不回退整串**)。
- **模式**:`default` / `accept_edits` / `bypass`。
  🔴 **`bypass` 关的是"问",不是治理** —— 提案/执行/收据一条不少,
  deny 规则仍赢,坏枚举值 fail-closed 回 `default`,项目目录不能选模式。
- 🔴 **批准权绝不交给 provider**:`methods.rs` 有测试禁止会话参数出现
  `yolomode`/`automode`/`always-approve`。交出去,收据就是虚构。

### 槽位

- `governed_grok`(受治理)+ `external_cli`(**登记用,不治理** —— 模型里是显式属性)
- API key 走 **DPAPI 加密**,Debug 脱敏、drop 清零、不进任何 DTO;非 Windows **fail-closed**
- 每槽位可拉模型目录(端点 `/models` 或 `grok models`);
  **端点不支持时返回明确的 unsupported,不是错误** —— 渲染层据此回落手敲
- 🔴 **换模型必须重启活跃 runtime**(模型是 `-m` 启动参数,会话内固定)

---

## 4. 🔴 三个"半开陷阱"(同构,全部无声)

**给了手,却没开通往宿主的线** → 代理自己动手,宿主全程看不见,没有报错。

| 陷阱 | 条件 |
| --- | --- |
| LocalFs | allowlist 有 `write` ∧ `writeTextFile=false` |
| LocalTerminal | allowlist 有 shell 工具 ∧ `terminal=false` |
| ambient MCP | 元工具未禁 |

🔴 **判定式不对称**:fs 是 `read ∧ write`(**双开**),terminal 是**单个 bool**。
`methods.rs` 有两条耦合测试钉住前两条。

---

## 5. 🔴 契约接缝:同一个坑栽了三次

三次表现**完全一样 —— 某个东西悄无声息地不出现,而门是绿的**:

1. 宿主加 `rule_subject` → 渲染层封闭键白名单不认 → 审批条全死
2. 归属放提案视图而非事件载荷 → replay 看不见
3. 宿主对 shell 发合法的空 `rule_subject` → 我写的校验器要求非空 → shell 审批条全死

**规矩:**
- 说清是**命令返回的 DTO** 还是**事件载荷**
- 宿主 DTO 加字段 = **破坏性变更**,必须同批放宽
  `apps/desktop/src/lib/tauriExecGateway.ts` 的 `only(value, proposalKeys)`
- **只断言"拒绝"的测试是假绿**

---

## 6. 实盘探针是唯一可信的边界证据

```bash
PILLOW_MCP_PROBE=1 node scripts/probe-mcp-tool-exposure.mjs
# 窄跑 PILLOW_MCP_PROBE_ONLY=<id>;"not run" 永不算过
```

不设变量时输出 `skipped` 返回 0,**永不进门**。

| id | 期望 |
| --- | --- |
| `shipped-profile-write` | 写**到达客户端**且未落盘 |
| `shipped-profile-mcp` | **零工具调用**,回答 `NO_MCP` |
| `acp-fs-localfs-trap` | **必须仍然落盘** —— 已知负例,哨兵 |
| `shell-no-handler` | `terminal/create` 到达客户端且未本地起进程 |

🔴 **判据是合取:"到达客户端" 且 "未被代理自行执行"。**
只看"没执行"会把**功能坏了**读成**治理生效**。
🔴 两个哨兵验证的是**供应商的性质,不是我们代码的性质**。

---

## 7. 在途 Worker

**grok1 —— 模型选择器 UI**(`.superpowers/round33/`,台账 `a20f1164`)。
只动 `apps/desktop/src/**`。已产出 `RuntimeSlotModelPicker.tsx` 与测试,**未提交**。

验收要点:三态(可用/不支持/失败)各有测试;`runtimeRestarted` **两个分支都如实呈现**
(不许编乐观提示);密钥不进 DOM。

---

## 8. 待办(按顺序)

1. 模型选择器 UI 验收提交
2. E2E 负载假红根因(任务 #10)
3. 🔴 **外审** —— `docs/references/external-audit-brief.md` **已按当前系统重写**,自带上下文可直接粘出去
4. **打包**(用户明确:最后一轮)

**用户从未真正用过。** 这是现在最大的未知。

---

## 9. 已知缺口(**不要当新发现重报**)

- 解释器确认表有限:不认识的 wrapper + 过宽规则 = 最可能的错放路径
- `node*` 这类规则合法地授权 `node any-script.js`
- 事件载荷只带**代表**规则,durable 收据才是全的 —— **审计看收据**
- 重启恢复的行拿不到段,故不可自动决策(刻意没把原始命令写进 durable 存储)
- **不是 OS 沙箱**:治理的是已验证的 provider 工具路径
- `search_replace` 故意未暴露

---

## 10. 我的坑位表(**新增在前**)

1. **一次性进程快照不能判定 worker 死亡。** 卡死后我据此判它死了,
   在**还在被写的树上**跑了三轮验证,自造 4 个 E2E 失败,还据此给了用户
   一个错误结论。**要观察日志/工作树是否还在增长。**
2. **门自己会假红。** `PROBE_COUNT` 是进程级 static,新测试启动 runtime 却不持锁,
   3 次全量红 1 次。**门是验收权威,它假红则后面全部判断不可信。**
   守卫要**容忍毒化** —— 否则 1 个真失败级联成 7 个,埋掉真凶。
3. **对照实验必须只差一个变量。** 我拿"两个 profile 都挡不住写"当决定性证据,
   但它们**都没解析成功** —— 在对比两个同样无效的 payload。
4. **工具名出现在调用列表 ≠ 调用成功。** 把成功的 `tasks__create` 读成"试了没成"。
5. **数量不等于对应关系。** 补丁脚本断言"3 个声明 / 3 个 catch",数量对上就放行,
   留下会掩盖真实错误的 `ReferenceError`。
6. **管道吃退出码。** `cmd | tail` 让我**把红的 E2E 报成了绿**。
   一律 `cmd > log 2>&1; echo EXIT=$?`。
7. **`cargo check` 不编译 `#[cfg(test)]`。** 收口用 `cargo test --workspace --no-run`。
8. **Worker 的门结论不可采信**(沙箱起不动门,交过自己看不见的红 clippy)。
9. **从代码推断运行时行为** —— 前后栽三次。
10. **别把自建模型当固定条件去砍需求。**
11. **已有明确指令时不要再问。** 用户早已定"照市面成熟产品做",
    我却把全局权限模式包装成需要他拍板的产品定位问题,白耗一轮。

---

## 11. 环境

- 门:`node scripts/run-worker-gate.mjs`(唯一合规入口)
- E2E:`pnpm desktop:test:e2e`,约 25 分钟,**总控独占**
- 并行:多写码单各自 `CARGO_TARGET_DIR`;门是 workspace 级,
  邻居半成品让 Worker 报 `BLOCKED-BY-CONCURRENT`,别去修
- 台账:`node scripts/run-cargo.mjs run -q -p keeper-knowledge-indexer --bin knowledge-indexer -- ledger status --db .superpowers/ledger/pillow.db --project pillow`
- **已可上手**:`target\release\pillow.exe`,启动约 1.6 秒
