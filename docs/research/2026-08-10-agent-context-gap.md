# Pillow — 代理到底拿到了什么上下文？

Date: 2026-08-10  
Status: 只读现状调查（无代码/构建/门禁改动）  
Scope: 用户在 Pillow composer 发一句话时，ACP 线协议上代理实际收到什么；与 Grok CLI 直用的差距  
Method: 源码追链路 + 既有 research/acceptance + 本机 `~/.grok/README.md`（只读 README，未读任何凭据文件）

完成标记：`AGENT-CONTEXT-GAP-COMPLETE`

---

## 0. 方法与边界

- **查证**：以当前仓库源码与已有协议/验收文档为准；Grok CLI 行为以 `~/.grok/README.md` 与仓库内 `docs/research/2026-07-30-grok-acp-protocol.md` 为据。
- **推测**：仅在明确标注处使用；不得与查证合并成“有上下文”一句带过。
- **本单未做**：live 启动子进程、cargo/pnpm、E2E、读 `auth.json` 或其它凭据、改任何产品文件（除本报告）。
- **路径**：报告中用 `<project-root>`、`<profile-home>`、`<worktree>` 占位，不粘贴机器绝对路径。

### 0.1 调用链（查证）

```text
Composer.tsx  onSend(content)
  → App.tsx sendTurn → conversationGateway.sendTurn(conversationId, content)
  → Tauri invoke "send_turn" { conversationId, content }
  → ConversationService::send_turn
       · resolve_working_directory(project.root_path)  // 项目根
       · create_run(user_message = trimmed content)    // SQLite
       · host.start_session(StartSession { conversation_key, working_directory })
       · host.spawn_turn(SendTurn { session_id, run_id, content })
  → GrokAcpAdapter::start_session → ACP "session/new"
  → GrokAcpAdapter::send_turn → ACP "session/prompt"
```

关键源文件：

| 层 | 路径 |
| --- | --- |
| UI | `apps/desktop/src/features/workbench/Composer.tsx` |
| 网关 | `apps/desktop/src/lib/tauriConversationGateway.ts` |
| 服务 | `apps/desktop/src-tauri/src/conversation_service/service.rs` |
| 项目根 | `apps/desktop/src-tauri/src/conversation_service/project_cwd.rs` |
| 组合 | `apps/desktop/src-tauri/src/runtime_composition.rs` |
| 适配器 | `crates/adapter-grok-acp/src/adapter.rs`, `prompt.rs`, `state.rs`, `rpc/methods.rs`, `process/*` |

---

## 1. §2.1 一次 turn 的完整输入清单

下列“代理实际收到”指 **Grok ACP 子进程在 stdio 上看到的 JSON-RPC**，以及子进程启动参数/环境。  
**有 / 无** 严格按 Pillow 代码是否发送或注入；不脑补“模型应该知道”。

### 1.1 子进程启动（进程级）

| 项 | 有/无 | 内容（查证） |
| --- | --- | --- |
| 可执行文件 | **有** | 设置/env 解析出的 Grok CLI 路径 |
| 启动 argv | **有** | 固定 `agent --no-leader stdio`（`PRODUCTION_ACP_ARGV`）；产品路径无 `--cwd`、无 `--rules`、无 `--permission-mode`、无 `--always-approve` |
| 子进程 OS cwd | **有，但不是项目根** | `spawn_child(..., cwd: None, ...)` → 继承桌面宿主当前目录，**不是** `session/new` 的项目根 |
| 环境 | **有（白名单）** | 仅 allow-list OS 名（如 `PATH`、`SYSTEMROOT`…）；另设 `HOME`/`USERPROFILE`（及可选 `APPDATA`/`LOCALAPPDATA`）= **isolated profile**（`honor_isolated_home`） |
| 父进程敏感 env | **无** | 不继承 `XAI_API_KEY` 等敏感名 |
| 在 argv 中注入 AGENTS.md | **无** | — |

### 1.2 `initialize`（连接后一次）

Pillow 发送（`initialize_params`）：

| 字段 | 有/无 | 值 |
| --- | --- | --- |
| `protocolVersion` | **有** | `1` |
| `clientInfo.name` | **有** | `"Pillow"` |
| `clientInfo.version` | **有** | adapter crate 版本 |
| `clientCapabilities.fs.readTextFile` | **有** | `true` |
| `clientCapabilities.fs.writeTextFile` | **有** | `false` |
| `clientCapabilities.terminal` | **有** | `false` |
| 系统提示 / 项目约定 | **无** | 本 RPC 不含 |
| 工具清单（客户端宣告工具目录） | **无** | 仅 fs/terminal 能力位 |

### 1.3 `session/new`（**每次** `send_turn` 都会新建）

`ConversationService::send_turn` **每次**调用 `host.start_session` → 适配器 `session/new`。  
**不**走生产路径上的 `session/load`（`load_session` 方法存在，桌面 `send_turn` 未调用）。

| 字段 | 有/无 | 值（查证） |
| --- | --- | --- |
| `cwd` | **有** | 对话所属项目的 **canonical 项目根**（SQLite `root_path` 校验后 canonicalize） |
| `mcpServers` | **有** | 固定 `[]` |
| `_meta.agentProfile.tools` | **有** | 固定 `[]` |
| `_meta.agentProfile.disallowedTools` | **有** | 固定长列表：含 `read_file`、`grep_search`、`list_dir`、`bash`、`run_terminal_command`、`search_replace`、`write_file`、`task`、`web_search`、`memory_*`、`todo_write`、`lsp` 等 |
| 系统提示字符串 | **无** | Pillow 不发送 system prompt |
| `AGENTS.md` / `CLAUDE.md` 正文 | **无** | Pillow 不读取、不注入 |
| 对话历史 | **无** | 不附带既往 user/assistant |
| 审批策略说明 | **无** | 无 “按需批准” 文案上线 |
| MCP 工具 | **无** | 空列表 |

### 1.4 `session/prompt`（用户这一句）

| 字段 | 有/无 | 值（查证） |
| --- | --- | --- |
| `sessionId` | **有** | 刚 `session/new` 返回的 ACP session id（适配器私有，不回灌 UI） |
| `prompt` | **有** | `[{ "type": "text", "text": <用户输入 trim 后原文> }]` |
| 系统提示块 | **无** | — |
| 历史消息块 | **无** | — |
| 嵌入文件 / `@` 引用 | **无** | Composer 只传纯字符串 |
| 工具清单正文 | **无** | 不在 prompt 里罗列工具 |
| 项目根路径字符串（除已通过 session cwd） | **无额外** | prompt 文本本身不含根路径，除非用户自己打出来 |

### 1.5 代理在 turn 中可请求、由 Pillow 处理的侧信道（非“输入”，是能力入口）

这些是 **agent → client** 请求；Pillow **不**在 prompt 里预告它们：

| 方法 | 适配器是否处理 | 含义 |
| --- | --- | --- |
| `fs/readTextFile` / `fs/read_text_file` | **有** | 归一化为治理读 `fs.read@1` |
| `pillow/read_tool` | **有** | `fs.list` / `fs.read` / `fs.search` |
| `pillow/write_tool` | **有** | `fs.write@1`（治理写 + 人工批准路径） |
| `pillow/process_tool` | **有** | `process.run@1` |
| `session/request_permission` | **有，且恒拒绝** | 选 `reject_*` / cancelled；**从不**因 kind=read 而 allow |
| 其它 Grok 原生工具执行 | 被 profile 禁用 + permission 拒绝 | 见 §3 |

### 1.6 Pillow 侧有、但 **未** 发给代理的数据

| 数据 | 存在位置 | 是否进入 ACP 输入 |
| --- | --- | --- |
| 用户消息 / run 事件时间线 | SQLite + UI | **否**（仅本 turn 的 `content`） |
| Composer 展示的“项目 / 运行时 / 按需批准” | UI 静态/状态 | **否** |
| 仓库 `AGENTS.md` / `CLAUDE.md` / `.grok/` | 磁盘上的项目文件 | Pillow **不读不传**（Grok 侧见 §2） |
| 工具批准 grant / 写 diff | 治理层 + UI bar | **否**（不作为 system 说明；写走独立 RPC 生命周期） |

### 1.7 一句话总结（§2.1）

> 代理在线协议上收到的“用户侧输入”本质是：**新 session 的 `cwd` + 明文 agentProfile（空 tools + 大 disallowed 列表）+ 仅含当前一句用户文本的 `session/prompt`**。  
> **没有** Pillow 组装的系统提示、**没有** 对话历史、**没有** 项目约定文件正文、**没有** 工具/权限说明文案。

---

## 2. §2.2 项目约定文件（两个来源必须分开）

### 2.1 来源 A — Pillow 产品代码

| 行为 | 结论 |
| --- | --- |
| 读取 `AGENTS.md` / `CLAUDE.md` / `Agents.md` / `Claude.md` 并注入 prompt | **无**（全仓 adapter / conversation_service 无此逻辑） |
| 读取项目 `.grok/` 规则/skills 并注入 | **无** |
| 读取用户 profile 下规则并注入 | **无**（profile 路径只用于子进程 `HOME`/`USERPROFILE`，且禁止打开 auth 文件） |
| 通过 `--rules` 或 prompt 前缀注入约定 | **无** |

**结论 A（查证）：Pillow 不负责把项目约定送进代理。**

### 2.2 来源 B — Grok CLI 自身

依据本机 **只读** `~/.grok/README.md`（章节 *AGENTS.md* / *Skills*）：

| 行为 | README 声明 |
| --- | --- |
| 项目规则文件 | 扫描并 **追加到 system prompt**：文件名含 `Agents.md`、`Claude.md`、`AGENT.md`、`AGENTS.md` |
| 扫描范围 | `~/.grok/` 全局；若在 git 仓库则从 repo root → cwd；否则仅 cwd；gitignore 跳过；单文件 cap 10k 字符 |
| Skills | 从 `./.grok/skills/`、repo `.grok/skills/`、用户 skills 等发现 |
| Agent profiles / subagents | 有独立发现与配置机制 |

ACP 侧：

- Pillow 在 `session/new` 传入 **`cwd` = 项目根**（查证）。
- 既有 live 协议研究（`docs/research/2026-07-30-grok-acp-protocol.md`）在 **未使用** 当前 plaintext `agentProfile` 的探测中，观察到代理用 **原生** `read_file` 去读 `AGENTS.md`——那是 **工具读取**，不是“已证明自动 system 注入”的同一路径。
- 当前生产 profile **显式 disallowed `read_file`**，且 permission **全拒**。

**结论 B（分层）：**

| 子项 | 状态 |
| --- | --- |
| Grok CLI 在普通 TUI/`--cwd` 下会加载 AGENTS 类文件 | **查证**（README） |
| Pillow 是否把这些文件读进线协议 | **查证：否** |
| 在 Pillow 的 ACP 路径下（`agent --no-leader stdio` + `session/new.cwd` + plaintext `agentProfile` + isolated `HOME`）Grok **是否仍**自动把项目 `AGENTS.md` 拼进 system prompt | **未 live 复测 → 未确定**（合理预期：session cwd 指向项目根时 CLI 仍可能加载，但本单无证据包；且 isolated home 会改变全局 `~/.grok` 规则来源） |

### 2.3 二者不可合并

| 问题 | 答案 |
| --- | --- |
| “Pillow 有没有把约定传给代理？” | **没有** |
| “Grok 自己会不会读？” | **文档说会；Pillow 内是否生效未在本单 live 证实** |
| “有没有项目约定上下文？” | **不能写成“有”**——Pillow 保证项为无；CLI 自载为可能、未证明 |

---

## 3. §2.3 工具与权限如何呈现

### 3.1 代理如何“知道”自己有工具？

| 通道 | Pillow 是否提供 | 说明 |
| --- | --- | --- |
| `session/prompt` 内工具清单 | **无** | 仅用户文本 |
| `_meta.agentProfile.tools` | **有，且为空** | 显式 `tools: []` |
| `disallowedTools` | **有** | 大段禁用 Grok 原生工具名（见 `rpc/methods.rs`） |
| `initialize` clientCapabilities | **有** | 声明客户端可服务 `fs.readTextFile`，不可写、无 terminal |
| MCP | **无** | `mcpServers: []` |
| 治理工具名 `pillow/*`、`fs.list`… | **未在 prompt 宣告** | 仅当代理主动发对应 server-request 时适配器才处理；**真实 Grok 是否会发这些方法：代码有入口，本单未 live 证明生产 Grok 会主动使用** |

历史验收：

- B2：`plaintext` profile、无 provider 工具；live 文本 turn 无 tool 活动。
- B3+：治理读写/进程内核与 **fake ACP 子进程** round-trip 验收；**不等于** 已证明真实 Grok 在生产 profile 下走 `pillow/*`。

### 3.2 写文件 / 跑进程

| 能力 | 线协议入口 | 人工审批 |
| --- | --- | --- |
| 治理读 | `fs/readTextFile` 或 `pillow/read_tool` → host 治理读 | 通常无 UI 批准（策略层执行） |
| 治理写 | **仅** `pillow/write_tool` | 有 UI 批准条（pending → grant/reject）；**adapter 不向模型注入“你需要等人批”的系统说明** |
| 治理进程 | **仅** `pillow/process_tool` | 有命令批准 UI 路径（同类：模型侧无说明文案） |
| Grok 原生 write/bash/… | profile 禁用 + `session/request_permission` **恒拒绝** | 代理不会拿到“批准一次即可原生写”的路径 |

### 3.3 代理知不知道“会触发人工审批”？

| 来源 | 有/无 |
| --- | --- |
| Pillow 系统提示说明审批模型 | **无** |
| UI “按需批准” 徽标 | **仅 UI**，不进 ACP |
| 运行时：写提案阻塞直到 UI 决策 | **有**（行为层）；模型只能通过工具 RPC 挂起/失败/成功结果间接感知，**无显式 policy 文案** |

---

## 4. §2.4 多轮与会话

### 4.1 同一次用户消息内部（一个 `session/prompt`）

| 能力 | 结论 |
| --- | --- |
| 协议是否允许 agent 在单次 prompt 内多次 server-request（读→写→进程→再读）直到 `stopReason` | **是**（`run_prompt_loop` 在 prompt 完成前持续处理 Incoming） |
| 在 **生产 plaintext profile + 真实 Grok** 下，多工具闭环是否可用 | **未确定 / 高度受限**：原生工具被 disallowed；治理 `pillow/*` 依赖代理是否知道并调用；B2 live 文本场景无 tool 活动 |

### 4.2 同一 Pillow 对话里用户连发多句

| 行为 | 查证 |
| --- | --- |
| 每句 `send_turn` | **新建** ACP `session/new` + **仅本句** `session/prompt` |
| 是否把 SQLite 时间线拼回 prompt | **否** |
| 是否 `session/load` 恢复同一 ACP session | **生产 send 路径：否** |
| ACP session id 跨桌面重启持久化 | **否**（适配器内存 `SessionMap`；B2 acceptance 已写 non-claim） |
| 用户感知的“对话” | UI/SQLite 连续；**模型侧每次近似冷启动**（仅可能带 Grok 自载的规则/skills，若生效） |

### 4.3 对“做一个任务要几次对话”的含义

| 场景 | 现状 |
| --- | --- |
| 用户一句里 agent 自主多步 | 协议支持；**工具可用性**才是硬门槛 |
| 用户第二句“接着改” | **模型默认看不到第一句与第一句结果**（Pillow 不传历史）；等于新任务 + 新 ACP session |
| 是否必须用户逐步发指令 | 无工具时 **几乎只能** 纯文本多轮用户驱动；有工具时单句可多步，但跨用户句仍丢上下文 |

**未确定**：真实 Grok 在 ACP 内部是否对 **同一 sessionId** 保历史——即使会，Pillow 也因 **每 turn 新 session** 用不上。

---

## 5. §2.5 与 Grok CLI 直用的差距

“Grok CLI 直用”指：交互 TUI 或 `grok --cwd <project> …` / headless，**非** Pillow 的 plaintext ACP profile。

| 项 | Grok CLI 直用 | Pillow 内 | 差距 | 影响 |
| --- | --- | --- | --- | --- |
| 项目根 / cwd | 进程 `--cwd` 或启动目录 | `session/new.cwd` = 项目根；**子进程 OS cwd 常非项目根** | 中：session 有根，进程 cwd 不一致 | 依赖进程 cwd 的行为可能偏移；会话根仍有 |
| 系统提示 / 角色 | 默认 agent + profile/skills | Pillow 不注入；依赖 CLI 默认 + 可能的规则扫描 | 中 | 产品无法保证“Pillow 代理人设” |
| `AGENTS.md` 等 | README：拼进 system prompt | Pillow 不传；CLI 自载 **未 live 证明** | **大**（若自载失败则完全无约定） | 产出可能违背仓库规矩 |
| Skills / `.grok/` | 自动发现（README） | Pillow 不注入；依赖 CLI + session cwd / profile home | 中–大 | 专项工作流可能不可用 |
| 内置工具全集 | 默认可用（读/搜/写/shell… + 权限规则） | `tools:[]` + 长 `disallowedTools`；permission 全拒原生 | **极大** | 代理难以做真实开发任务 |
| 治理工具桥 | 无（直用原生） | `pillow/*` + 治理读写进程存在 | 方向相反 | 直用能力强但不受 Pillow 治理；Pillow 治理在但代理未必会调用 |
| 人工审批 | CLI permission mode / rules / prompts | 治理写/进程 UI 批准；原生 permission 全拒 | 大 | 模型无策略说明；原生路径被掐死 |
| 对话历史 | 同 session 连续；可 resume | 每句新 ACP session；不重放历史 | **极大** | “接着改”失效 |
| MCP | 可配置 | 强制 `[]` | 大 | 无外部工具 |
| 终端 client | CLI 自有 | `terminal: false` | 大 | 无 ACP terminal |
| Auth / profile | 用户 `~/.grok` | isolated `HOME`/`USERPROFILE` = 配置的 profile 目录 | 中 | 凭据/全局规则来自 profile 槽，不是随意混用 |
| 用户消息内容 | 用户输入 | 同：仅 trim 后原文 | 无 | — |
| UI 项目名 / “按需批准” | N/A | 仅 UI | — | 不进模型 |

---

## 6. 按“阻塞真实使用”排序的缺口清单

粗略工作量：S &lt; 1d，M 数日，L 跨周/多 PR。**本单不提设计方案，仅标量级。**

| 序 | 缺口 | 影响 | 量级 | 依据 |
| --- | --- | --- | --- | --- |
| 1 | **原生工具被 session profile 整体禁用**，permission 再全拒 | 代理无法像 CLI 一样读/写/搜/跑；真实开发任务基本不可做 | L | `rpc/methods.rs` + `map/permission.rs` + B2 acceptance |
| 2 | **跨用户消息无 ACP 会话复用、无历史重放** | 每句冷启动；多步任务拆成多句时模型丢上下文 | L | `service.rs` 每 turn `start_session`；`session_prompt_params` 仅当前文本 |
| 3 | **Pillow 不注入、也不验证项目约定（AGENTS/CLAUDE）是否进入模型** | 即使工具恢复，产出也可能违反仓库规矩；“能用”的另一前提 | M | 无读取逻辑；CLI 自载未 live 证明 |
| 4 | **治理工具（`pillow/*`）对真实 Grok 的可达性未在生产 profile 下证明** | 内核/UI 可能已具备读写进程治理，模型侧仍可能调不到 | L | fake round-trip vs B2 live plaintext |
| 5 | **无工具/权限/审批策略的模型可见说明** | 代理不知“写要等人批”、不知允许调用面 | M | prompt 无说明；UI 徽标不上线 |
| 6 | **MCP / 终端 / 网络等 CLI 能力被关掉** | 复杂任务面更窄 | M | initialize + mcpServers |
| 7 | **子进程 OS cwd ≠ 项目根** | 少数依赖进程 cwd 的行为不一致 | S–M | `spawn_child(..., None)` |
| 8 | **ACP session 不跨重启** | 无法恢复供应商侧 session 记忆 | M | SessionMap 内存；B2 non-claim |

---

## 7. 未查到 / 不确定项

| 项 | 说明 |
| --- | --- |
| ACP + plaintext profile 下 Grok 是否仍自动加载 `AGENTS.md` 进 system prompt | README 有通用声明；**本单未 live 抓包/对比 token** |
| 真实 Grok（当前版本）在 `tools:[]` + disallowed 列表下是否会改用 `fs/readTextFile` / `pillow/*` | 仅有适配器与 fake 证据；**无本单 live** |
| Grok 对 `_meta.agentProfile` 的完整语义（是否清空 skills、是否改 system prompt） | 仓库只强制 tools/disallowed 形状；**无 Grok 源码** |
| 同一 ACP `sessionId` 上多 prompt 的历史行为 | Pillow 生产路径不复用 session，故 **未测** |
| isolated profile 目录内是否存在用户自建全局 `AGENTS.md`/skills | **禁止**枚举 profile 内凭据区；未查 |
| 具体模型最终 system 字节内容 | 不在 stdio 明文中完整出现；属供应商内部组装 |

---

## 8. 结论（给总控亲自试跑前的预期）

1. **用户发一句话时，Pillow 保证送上线的就是：项目根 cwd + 空工具 profile + 那一句话。**  
2. **项目约定文件：Pillow 不传。** Grok 是否自载，是另一条线，本单不能写成“有”。  
3. **工具权限：UI 写“按需批准”，模型侧没有对称说明；原生工具被产品 profile 关掉；治理桥在适配器里，但不等于代理会用。**  
4. **多轮：单次 prompt 协议上可多步；用户连发多句时模型侧基本不连续。**  
5. **与 CLI 直用的主差距：工具面 + 会话连续性 +（可能）规则/skills 加载；差在哪里就是“能做真实开发任务”的前提缺口。**

---

## 9. 关键证据索引（只读）

- `crates/adapter-grok-acp/src/rpc/methods.rs` — initialize / session profile / prompt 形状  
- `crates/adapter-grok-acp/src/adapter.rs` — session/new、send_turn  
- `crates/adapter-grok-acp/src/prompt.rs` — prompt 循环与 server-request 分发  
- `crates/adapter-grok-acp/src/map/permission.rs` — permission 恒拒绝  
- `crates/adapter-grok-acp/src/state.rs` / `process/child.rs` / `process/env.rs` — 启动 argv、cwd、env  
- `apps/desktop/src-tauri/src/conversation_service/service.rs` — 每 turn start_session + 仅 content  
- `apps/desktop/src-tauri/src/runtime_composition.rs` — isolated home、单并发 prompt  
- `docs/verification/2026-07-30-slice-b2-acceptance.md` — plaintext 边界验收  
- `docs/research/2026-07-30-grok-acp-protocol.md` — 线协议观测  
- `~/.grok/README.md` — AGENTS.md / skills / CLI 行为（只读）

`AGENT-CONTEXT-GAP-COMPLETE`
