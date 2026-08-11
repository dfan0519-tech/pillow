# Pillow 总控交接手册(2026-08-11)

工作目录:`<repo>`
分支 / HEAD:`main` / `963dcbb`(**本地 = 远程**,工作树干净)
远程:`https://github.com/dfan0519-tech/pillow.git`(**private**)

> 取代 `docs/handoff/2026-08-10-*.md`。那份写于边界尚未成立时,
> **§3「真实代理没有手」已被推翻**,不得作为真相源。
> 运维细节在 **`.superpowers/RUNBOOK.md`**(gitignored)。开工前先读。

---

## 1. 一句话状态

**代理有两只受管的手(写文件、跑命令),槽位可配自有 key 并按端点选模型。**

自 `9b2abba` 起 **34 个提交**。门 OK,E2E 10/10,**在途 Worker:0**。

🔴 **用户仍未真正跑过一轮任务。这是项目现在最大的未知。**

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
**不要因为自建模型接不住就砍功能,去看市面上怎么解的。**

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
| **写** | 原生 `write` → `fs/write_text_file` → 治理链 |
| **进程** | 原生 `run_terminal_cmd` → `terminal/*` → 同一条链 |
| **MCP** | **切断**(环境侧 MCP 关不掉,禁元工具是唯一杠杆) |

**allowlist 管原生工具(fail-closed);denylist 只关一扇门,不是列举房间。**
有测试钉死:denylist 出现第三个名字就红。

**规则**:保守拆段,allow **逐段合取**,任一段 deny 即拒,拆不动就问。
`Bash(cargo*)` **不会**放行 `cargo test; rm -rf x`。批准时可**逐段**存规则
(拆不动则不给入口,**绝不回退整串**)。

**模式**:`default` / `accept_edits` / `bypass`。
🔴 **`bypass` 关的是"问",不是治理** —— 提案/执行/收据一条不少,deny 仍赢,
坏枚举值 fail-closed 回 `default`,项目目录不能选模式。
🔴 **批准权绝不交给 provider**(`methods.rs` 有禁令测试)。

**槽位**:`governed_grok`(受治理)/ `external_cli`(**登记用,不治理**,模型里是显式属性)。
API key 走 **DPAPI 加密**,Debug 脱敏、drop 清零、不进 DTO;非 Windows fail-closed。
每槽位可拉模型目录;**端点不支持 `/models` 时返回明确 unsupported 而非错误**,
渲染层据此回落手敲。🔴 **换模型必须重启活跃 runtime**(模型是 `-m` 启动参数)。

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

**规矩**:说清是**命令返回的 DTO** 还是**事件载荷**;
宿主 DTO 加字段 = **破坏性变更**,必须同批放宽
`apps/desktop/src/lib/tauriExecGateway.ts` 的 `only(value, proposalKeys)`;
**只断言"拒绝"的测试是假绿**。

---

## 6. 实盘探针是唯一可信的边界证据

```bash
PILLOW_MCP_PROBE=1 node scripts/probe-mcp-tool-exposure.mjs
# 窄跑 PILLOW_MCP_PROBE_ONLY=<id>;"not run" 永不算过
```

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

## 7. 用户上手所需(**下一步就是这个**)

- **启动**:桌面 / 开始菜单的 **Pillow** 快捷方式 → `target\release\pillow.exe`
- **试用项目**:`<repo>-tryout`(10 通过 / 3 失败,README 有六步试用顺序)
- 🔴 **Grok 当前未登录**(`grok models` 报 not authenticated)。
  第一次**不要选登录型槽位**,用 API key 槽位:
  端点 `https://opencode.ai/zen/v1`,key 在桌面 `opencode key.txt`,
  **实测可用,61 个模型**(建议先用 `claude-haiku-4-5`)。

### 图标与快捷方式的三个坑(都已修,别再踩)

1. **cargo 不把图标当构建脚本输入** —— 重新生成图标后不 touch `build.rs`,
   二进制里还是旧图标,而**所有命令都报成功**。
   `scripts/generate-app-icons.mjs` 现在会自己 touch。
2. **Windows 图标缓存按"目标路径 + 索引"做键** —— 反复重建同一路径的快捷方式
   永远命中旧值。快捷方式现在指向 `pillow.ico`(独立文件)。
3. **运行中的 exe 无法被覆盖** —— 每次改前端都要先关 Pillow 再构建。
   用 PowerShell `CloseMainWindow()`;
   🔴 **`taskkill /IM` 在 MSYS 下会被转成 `F:/Git/IM` 而失败。**

---

## 8. 已知缺口(**不要当新发现重报**)

- 解释器确认表有限:不认识的 wrapper + 过宽规则 = 最可能的错放路径
- `node*` 这类规则合法地授权 `node any-script.js`
- 事件载荷只带**代表**规则,durable 收据才是全的 —— **审计看收据**
- 重启恢复的行拿不到段,故不可自动决策
- **不是 OS 沙箱**:治理的是已验证的 provider 工具路径
- E2E 满负载偶发假红(任务 #10),单跑通过
- `search_replace` 故意未暴露

---

## 9. 我的坑位表(**这一天付出的代价**)

1. **抑制输出后从残缺信号下结论** —— 今天犯了三次:
   `cmd | tail` 把红的 E2E 报成绿;`taskkill` 一直失败而我把 stderr 丢了;
   构建失败却按"已修好"向用户描述。
   **一律 `cmd > log 2>&1; echo EXIT=$?`,并看输出本身。**
2. **退出码不等于结果。** 图标生成 exit 0 但底板没了;构建 exit 0 但图标没进去。
   **看产物,不看返回码。**
3. **一次性进程快照不能判定 worker 死亡** —— 我据此在**还在被写的树上**
   跑了三轮验证,自造 4 个 E2E 失败,并给了用户一个错误结论。
   **要观察日志/工作树是否还在增长。**
4. **门自己会假红。** 进程级 static 计数被不持锁的新测试污染,3 次红 1 次。
   **门是验收权威,它假红则后面全部判断不可信。** 守卫要容忍毒化。
5. **对照实验必须只差一个变量。** 我对比的两个 profile **都没解析成功**。
6. **工具名出现在调用列表 ≠ 调用成功。**
7. **数量不等于对应关系**(补丁脚本断言计数相等就放行,留下 `ReferenceError`)。
8. **`cargo check` 不编译 `#[cfg(test)]`** —— 收口用 `cargo test --workspace --no-run`。
9. **Worker 的门结论不可采信**(沙箱起不动门,交过自己看不见的红 clippy)。
10. **从代码推断运行时行为** —— 前后栽三次。
11. **别把自建模型当固定条件去砍需求。**
12. **已有明确指令时不要再问。**
13. **修 UI 只修截图里那一处是不够的** —— 四个 banner 共用同一个布局前提,
    只修一个,换个状态就再撞一次。而且**要把所有 fixed 元素都算进避让**,
    我第一次只算了窗口控件,漏了悬浮的设置按钮,用户看到的是"跟没改一样"。

---

## 10. 下一步

1. 🔴 **用户跑一轮真实任务**(见 §7)
2. 任务 #10:E2E 负载假红根因
3. **外审** —— `docs/references/external-audit-brief.md` **已按当前系统重写**,
   自带上下文可直接粘出去
4. **打包**(用户明确:最后一轮)

---

## 11. 环境

- 门:`node scripts/run-worker-gate.mjs`(唯一合规入口)
- E2E:`pnpm desktop:test:e2e`,约 25 分钟,**总控独占**
- 并行:多写码单各自 `CARGO_TARGET_DIR`;门是 workspace 级,
  邻居半成品让 Worker 报 `BLOCKED-BY-CONCURRENT`
- 台账:`node scripts/run-cargo.mjs run -q -p keeper-knowledge-indexer --bin knowledge-indexer -- ledger status --db .superpowers/ledger/pillow.db --project pillow`
