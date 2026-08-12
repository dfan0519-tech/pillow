# Pillow 总控交接手册(2026-08-11)

分支 / HEAD(撰写时):`main` / `963dcbb`
远程:`https://github.com/dfan0519-tech/pillow.git`(**private**)

> 取代 `docs/handoff/2026-08-10-*.md`。那份写于边界尚未成立时,
> **§3「真实代理没有手」已被推翻**,不得作为真相源。
> 本机运维(多 Worker 槽位农场、claim/release、私有 runbook)不在公开产品
> 契约内;见根 `AGENTS.md` 的 multi-runtime isolation 原则。
>
> **2026-08-12 外审前清扫补丁**:§3「槽位」段已按当前代码重写(统一记录 /
> 角色 / 执行路径 / OAuth / 派单)。§7 本机路径与凭据位置已脱敏。已知缺口
> (§8)未删。更细的执行报告在 `docs/execution/2026-08-12-*.md`。

---

## 1. 一句话状态

**代理有两只受管的手(写文件、跑命令);槽位是统一记录(`auth`+`role`),
可配 key / 登录 / OAuth / 本地 CLI,并按 auth 推导治理与执行路径。**

自 `9b2abba` 起持续合入。撰写时门与 E2E 曾绿;以当前工作树重新跑门为准。

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

**槽位**(统一记录,不是两种独立 kind):
- **一条记录** = `auth` 字段 + `role` 字段。**治理从 auth 推导**,不单独存、
  不可与 auth 矛盾(`governance_from_auth`;迁移 0017 起无独立 governance 列)。
- **auth**:`ApiKey` / `GrokLogin` / `LocalCli` / `Oauth`
  (`crates/storage-sqlite/src/runtime_slot.rs` `SlotAuth`)。
- **role**:`Controller` / `Librarian` / `Custom(name)`。
  Controller **项目内唯一**(部分唯一索引)且 **必须受治理**
  (`LocalCli`+`Controller` 拒绝)。
- **执行路径**(由 auth + provider request shape **推导**,不是 UI 开关):
  `GrokShell` / `NativeAdapter` / `NotRunnable`
  (`crates/runtime-slots/src/execution_path.rs`)。
  - `GrokLogin` / shell-expressible `ApiKey`/`Oauth` → GrokShell + Full
  - 不可用 `base_url`+`env_key` 表达的 OAuth(如 `chatgpt-codex`) →
    NativeAdapter + Full(Pillow 自为 harness;**禁止**塞进 grok 壳)
  - `LocalCli` → NotRunnable + Incomplete(仅登记/状态,不能起对话)
  - 🔴 **NativeAdapter 启动目前只到 `native_adapter_prepared`**,
    **不会** `activate_grok`;完整对话 runtime 仍主要走 GrokShell。
- 渲染层 wire label 仍可能是 `governed_grok` / `external_cli`
  (兼容封闭白名单的**派生标签**,不是存储层的两种 kind)。
- 相关 crate:`pillow-provider-adapters`(纯请求形状)、
  `pillow-role-dispatch`(派单 → 结构化进程提案)、
  `pillow-cli-detect`(本地 CLI 发现/只读探针)。
- **派单** = 普通的受治理进程提案链(proposal → 一次性人批 → 宿主执行 → 收据),
  **不是**第二套审批机制。
API key / OAuth token 走 **DPAPI 加密**,Debug 脱敏、drop 清零、不进 DTO;
非 Windows fail-closed。每槽位可拉模型目录;**端点不支持 `/models` 时返回明确
unsupported 而非错误**,渲染层据此回落手敲。
🔴 **换模型必须重启活跃 runtime**(GrokShell 路径下模型是 `-m` 启动参数)。

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

- **启动**:桌面 / 开始菜单的 **Pillow** 快捷方式,或本仓库 release 构建产物
  `target/release/pillow.exe`(相对仓库根)。
- **试用项目**:维护者本机另有 tryout 目录(不进本仓库);其 README 有六步试用
  顺序。产品内任意项目根亦可。
- **Grok 登录状态依本机 profile 而定**(不要把某一 worker 的
  `grok models` 结果当成全机真相)。登录型槽位、API key 槽位、OAuth 槽位
  在代码路径上均可用;LocalCli 仅登记/状态。
- API key 槽位:任意 OpenAI-compatible `base_url` + DPAPI 保管的 key;
  端点是否暴露 `/models` 与否见上文(unsupported → 手敲模型 id)。
  具体第三方端点与密钥文件**不要写进公开交接**;只在本机私有笔记里。

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

- 门:`node scripts/run-worker-gate.mjs`(唯一合规入口;见
  `docs/verification/worker-gate-entry.md`)
- E2E:`pnpm desktop:test:e2e`,约 25 分钟,**总控独占**
- 并行:多写码单各自隔离 `CARGO_TARGET_DIR`;门是 workspace 级,
  邻居半成品让 Worker 报 `BLOCKED-BY-CONCURRENT`
- 知识台账:通过 `knowledge-indexer` 的 `ledger status` / `ledger stale`
  (库路径由维护者本地配置;不在公开文档写死私有路径)
