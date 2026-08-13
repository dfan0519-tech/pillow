# Pillow — 调研：开源项目怎么做多 Agent 管理

Date: 2026-08-13  
Worker slot: `grok3`  
Controller: Claude  
Mode: **read-only research**（除本报告外不改任何文件；无 git 写操作；无门/E2E）  
完成信号: `MULTIAGENT-RESEARCH-COMPLETE`

上游 `main` 是可变分支。本报告引用的提交（拉代码当日）：

| 仓库 | `main` SHA |
| --- | --- |
| [openclaw/openclaw](https://github.com/openclaw/openclaw) | `7d6dfa1939b4` |
| [NousResearch/hermes-agent](https://github.com/NousResearch/hermes-agent) | `7626105380b7` |
| [crewAIInc/crewAI](https://github.com/crewAIInc/crewAI) | `27083f41319a` |
| [microsoft/autogen](https://github.com/microsoft/autogen) | `027ecf0a379b` |

正式复用前必须再钉一次完整提交。

---

## 0. 总控摘要（先读）

| 问题 | 结论（一句话） |
| --- | --- |
| 「龙虾」是谁 | **OpenClaw**（`openclaw/openclaw`）。把握：**高**。不是 Lobe。 |
| 「爱马仕」是谁 | **Hermes Agent**（`NousResearch/hermes-agent`）。把握：**高**。不是 Nous 的 Hermes 模型本身，也不是爱马仕品牌。 |
| 别人的「多 agent」是不是真并发 | 分两层。OpenClaw：**持久人格可同时接 inbound** + 子 agent 有硬并发上限。Hermes：**子 agent 线程池真并行**，默认 3。CrewAI：**默认顺序**，任务可标 `async_execution`。AutoGen：**默认轮流说话**，不是并跑。 |
| 和 Pillow 最像谁 | 建模上最像 **OpenClaw 的 `agents.entries` + bindings**（持久身份、每套 workspace/auth/session）。治理上 **没有人比得上 Pillow 的提案链**。 |
| 我们哪里在重复造轮子 | 槽位记录、角色名、派单地址——OpenClaw/CrewAI 都有。**不要再发明第三套「人格 JSON」。** |
| 我们哪里不如人 | 产品话说「槽位共存、同时工作」，宿主却是 **一个 `ConversationService` + 一个 `active_slot_id`，Start 会关掉上一个**。OpenClaw/Hermes 都把「后台孩子跑完再回来」做成一等事件。 |
| 我们哪里更好、不要改 | SQLite 域对象、治理由 auth 推导、派单走同一条 process 提案链、DPAPI 分槽位密文、LocalCli 不能升格、Controller 每项目至多一个且必须受治理。 |
| 若只做一件事 | **先书面钉死并发语义**：要么宿主按 `slot_id` 持有 N 个运行时；要么改口为「一个活对话 + 受治理的派单进程」，并把 assignment 画进时间线。现在两套说法同时成立，会把后续 UI/适配器做歪。 |

证据标记：`[代码]` 实际读过文件；`[文档]` 官方文档；`[推测]` 推断。三者不混。

---

## 1. 外号映射

| 外号 | 我认为是哪个仓库 | 判断依据（URL） | 把握程度 |
| --- | --- | --- | --- |
| 龙虾 / 小龙虾 | [openclaw/openclaw](https://github.com/openclaw/openclaw)（站点 [openclaw.ai](https://openclaw.ai/)） | BBC 中文写明 OpenClaw「因其标志为红色龙虾，被俗称龙虾」：[bbc.com](https://www.bbc.com/zhongwen/articles/c93wvdn91kxo/trad)。仓库 description 是 “The lobster way. 🦞”。中文教程/社区一律把 OpenClaw 叫小龙虾，例如 [cnblogs](https://www.cnblogs.com/wgwyanfs/p/19962884)、[hello-claw](https://github.com/datawhalechina/hello-claw)。 | **高** |
| 爱马仕 | [NousResearch/hermes-agent](https://github.com/NousResearch/hermes-agent)（文档 [hermes-agent.nousresearch.com](https://hermes-agent.nousresearch.com/docs/zh-Hans/)） | 腾讯云「Hermes Agent（爱马仕）」：[cloud.tencent.com](https://cloud.tencent.com/developer/article/2658100)。36 氪「别养龙虾了，开始养马」：[36kr](https://eu.36kr.com/zh/p/3765157512741377)。百度百科词条写「昵称：爱马仕」：[baike](https://baike.baidu.com/item/Hermes%20Agent/67487182)。飞书官方内容站同称「爱马仕 Agent」：[feishu.cn](https://www.feishu.cn/content/article/7630758640865037530)。名字来自希腊神使 Hermes，与奢侈品品牌无关（社区指南也这么写）。 | **高** |

**排除过的误认（不要再传）：**

- 「龙虾」不是 LobeChat / LobeHub。Lobe 是音译候选，但 2026 中文社区「养龙虾」专指 OpenClaw。
- 「爱马仕」不是 Nous 的 Hermes **模型** 系列，也不是 OpenHermes 数据集。指的是 2026-02 开源的 **agent runtime**。
- 社区还有大量 OpenClaw 插件/团队壳（ClawTeam、OpenCrew、Multi-OpenClaw）。那些是 **在龙虾上叠一层编排**，不是龙虾本体。本报告只把本体当「龙虾」。

外号已经对上。下面仍补两个 **确认存在、且代码里确实在管多个 agent** 的项目，用来对照框架派（角色/编排）和产品派（持久身份）的差别：

1. [crewAIInc/crewAI](https://github.com/crewAIInc/crewAI) — 角色扮演 Crew + sequential/hierarchical。
2. [microsoft/autogen](https://github.com/microsoft/autogen) — AgentChat 群聊 + Magentic-One 账本编排。

考虑过但 **本单没有深读源码** 的候选：OpenHands（产品更像 IDE，树很大，本单时间不够）、LangGraph（图编排，不是槽位管理）、OpenCode（维护者已把它当「选一个供应商」对照，本单未再核对）。不要把这三项写进横向表当已核实实现。

### 本单访问失败（明说）

| 想读的文件 | 结果 |
| --- | --- |
| Hermes `hermes_cli/profiles.py`、`acp_adapter/permissions.py` | GitHub raw **HTTP 429**，未读到正文 |
| OpenClaw 官方站点 `docs.openclaw.ai` | 工具侧 SSRF 拦截（解析到内部地址）；改读仓库内 `docs/` raw，成功 |
| OpenHands 源码 | 未拉 |

Hermes **桌面有 Profiles UI** 这件事来自仓库树（`apps/desktop/src/app/profiles/create-profile-dialog.tsx` 等），不是我读过的实现细节，标 `[推测]` 时会单独写。

---

## 2. 横向对比总表

「多 agent」在这四个项目里不是同一个词。先看它们各自管的是哪一层：

| 层 | 含义 | OpenClaw | Hermes | CrewAI | AutoGen | Pillow 现状 |
| --- | --- | --- | --- | --- | --- | --- |
| 持久身份 | 用户能新建、长期活着的 agent | `agents.entries` + workspace + `agentDir` | Profile（桌面/CLI；源码 429 未核）+ 单进程主 agent | 无；进程内 Python 对象 | 无；进程内对象 | SQLite `runtime_slot`：`auth` + `role` |
| 派生子 | 主循环再拉一个干活的 | `sessions_spawn` 子 agent | `delegate_task` 子 agent | `allow_delegation` / manager | 群聊下一名说话者 | `Assignment` → 受治理 process |
| 供应商 | 模型从哪来 | 每 agent 自己的 model + auth profile，可换 | 父继承，或 `delegation.model/provider` 全局钉死孩子 | 每 Agent 一个 `llm` 字段，可换 | 每 agent 一个 `ChatCompletionClient` | 每槽位 `model_id` / `base_url`，可换 |

### §3 六维

| 维度 | OpenClaw 龙虾 | Hermes 爱马仕 | CrewAI | AutoGen | Pillow（对照，非调研对象） |
| --- | --- | --- | --- | --- | --- |
| **3.1 怎么建模** | 配置文件 `openclaw.json` 的 `agents.entries.<id>`。一个 agent = workspace + `agentDir` + 每 agent SQLite 会话 + auth profiles。默认只有 `main`。`[代码]`/`[文档]` | 运行时 `AIAgent` 类 + `delegate_task` 拉孩子。持久层是 profile/`~/.hermes`（profile 实现未读到）。孩子是一次性会话。`[代码]` | Pydantic `Agent(role, goal, backstory, llm, tools)` + `Crew(agents, tasks, process)`。YAML 可声明。`[代码]` | `ChatAgent` 对象 + `Team`/`GroupChat`。Magentic-One 另有 Orchestrator。`[代码]` | SQLite 行。角色：Controller / Librarian / Custom。每项目至多一个 Controller。 |
| **3.2 怎么并发** | **真并发（两层）**。Gateway **一个进程**里多个 agent 同时接 inbound。顶层 `maxConcurrent` 默认 clamp 在 8–16（跟 CPU 并行度走）。子 agent 默认最多 8 个并行、每父最多 5 个直接孩子、深度默认 1。`[代码]` | **子 agent 真并行**：`ThreadPoolExecutor`，默认 3，下限 1、无上限；超 10 打成本警告。超限的 batch **报错，不静默截断**。进程重启 **不恢复正在跑的孩子**。`[代码]` | **默认顺序**。`task.async_execution` 可重叠，但 crew **末尾最多一个 async task**。`kickoff_async` 把同步 kickoff 丢进线程。`[代码]` | **默认不是并跑**。`BaseGroupChat` 用 `SingleThreadedAgentRuntime`，一次选一名说话者。`[代码]` | **记录可共存；活对话宿主一次一个。** `activate_grok` 关掉上一个 `ConversationService`。生产 `max_concurrent_turns = 1`。派单是另一条 process 链。 |
| **3.3 谁派活** | 没有产品级「总控」角色。Inbound 靠 **bindings 第一匹配**。父→子靠 `sessions_spawn`（有 schema）+ `sessions_yield` 等完成事件。`agentToAgent` 默认关。`[代码]`/`[文档]` | 主会话是隐式编排者。`delegate_task(goal, context, role=leaf\|orchestrator)`。孩子摘要回父上下文；后台完成进 `state.db` 再投递。`[代码]`/`[文档]` | sequential：任务预绑 agent。hierarchical：**manager 派活**（manager 禁止自带 tools，只用 `AgentTools`）。产出是 `TaskOutput` 链。`[代码]` | Magentic-One Orchestrator 用 **JSON 账本**（facts/plan/progress）选下一名。SelectorGroupChat 用 LLM 或自定义函数选说话者。共享群聊上下文。`[代码]` | 只有 Controller 能派。`Assignment` 有 schema；禁自派、禁角色错配。走同一条提案→批准→执行→收据。 |
| **3.4 凭据** | **每 agent 自己的 store**（`agentDir` SQLite）。禁止共用 `agentDir`。二级 agent OAuth 过期时 **会回落到 main 的同 profile 最新 token**（不拷 refresh）。要独立账号必须自己登录。`[文档]` | 孩子 **默认继承父 API key / provider / credential pool**；`delegation.provider/base_url` 可整批改孩子。切换 profile 是否重登：源码 429 未核。`[代码]`/`[文档]` | 每个 Agent 自己的 `llm`；密钥走环境变量 / 平台登录，**不是每 agent 加密保险箱**。`[代码]`/`[文档]` | 每个 agent 自己的 model client。无产品级分 agent 密钥库。`[代码]` | 每槽位 DPAPI 密文 + `secret_ref`。DTO 无 key。OAuth access/refresh 分 ref。 |
| **3.5 权限与审批** | **按 agent** `tools.allow/deny` + 每 agent sandbox。子 agent 另有硬拒绝名单（不能 `message`/`gateway`；leaf 不能再 spawn）。exec 有 before-tool-call 审批通道（allow-once / always / deny，超时 fail-closed）。`[代码]`/`[文档]` | 审批是 **session 级**（`manual` / `smart` / `off`）+ YOLO。子 agent 默认 **自动拒绝危险命令**（避免 TUI 死锁），`subagent_auto_approve` 才放行。孩子不能 `clarify`/`memory`/`send_message`/`cronjob`。`[代码]` | 框架不管人类批准。每 agent 自己的 `tools` 列表。manager 被强制清空 tools。`[代码]` | 框架不管人类批准。工具在 agent 上。`[代码]` | 每个工具调用走提案链。治理只有 auth 推导的 Full/None，**没有「这个槽位只读」**。派单不另开第二条批准条。 |
| **3.6 UI** | CLI：`openclaw agents add` → 绑 channel → restart。Control UI：父会话侧栏展开子 run；`visible: true` 的 spawn 出现在仪表盘。说话者靠 channel/bot/人格文件。`[文档]` | CLI/TUI 树状看每个孩子的工具调用；桌面有 Profiles 对话框（未读实现）。孩子进度有 live transcript。`[文档]`/`[推测]` | 开发者框架。有 `crew_run_tui`、YAML `agents.yaml`。不是终端用户槽位管理器。`[代码]` 树 | 开发者框架 + Magentic-One CLI。群聊按说话者名字区分。`[代码]` | Settings 列表。新建：provider → connect（认证+角色+名字）两步。Workbench 有派单目标选择器。宿主 list DTO **没有 active 标志**；面板用 session-local `startedSlotId`。 |

---

## 3. 每个项目

### 3.A OpenClaw（龙虾）

仓库：<https://github.com/openclaw/openclaw>  
定位：自托管个人助理。一个 Gateway 进程，多个 **隔离人格**，inbound 用 bindings 路由。不是 IDE，也不是「总控派单给信息管理员」产品。

#### 3.A.1 建模

`[文档]` 官方概念页把一个 agent 定义成完整作用域：workspace 文件、`agentDir`（auth / model registry）、SQLite 会话。默认不配就是一个 `main`。

`[代码]` 新建走 `createAgent()`：规范化 `agentId`，拒绝保留名 `main`（除非 onboarding 引导）、写 workspace / identity、可选 bindings，结果是 `created | existing | error`。

```117:133:（openclaw src/agents/agent-create.ts）
export async function createAgent(params: CreateAgentParams): Promise<CreateAgentResult> {
  // ...
  const agentId = normalizeAgentId(rawId);
  const isBootstrapMain = agentId === RESERVED_BOOTSTRAP_AGENT_ID && params.bootstrapMain === true;
  if (
    (!isBootstrapMain && agentId === RESERVED_BOOTSTRAP_AGENT_ID) ||
    isReservedSystemAgentId(agentId)
  ) {
    return createError("reserved-id", `"${agentId}" is reserved`, agentId);
  }
```

源：[`src/agents/agent-create.ts`](https://github.com/openclaw/openclaw/blob/7d6dfa1939b46cf093389355dbe67254e7bf0756/src/agents/agent-create.ts)

`[文档]` 每个 agent 可绑不同 `model`（`provider/model`）。模型不是身份的不可换部分；换模型不用换 agent。

`[文档]` workspace 是默认 cwd，**不是硬沙箱**。绝对路径仍能摸到别处，除非打开 sandbox。

用户能建多少个：配置里没有写死个数。隔离靠「不要共用 `agentDir`」。区分靠 `agentId` + binding。

#### 3.A.2 并发

这是本单最关键的一条。OpenClaw **不是**「能配多个、一次跑一个」。

`[代码]` 顶层并发默认值按 CPU 并行度算，再夹在 8 和 16 之间；非法值地板为 1。子 agent 另有一套默认：并行 8、每父 5 个直接孩子、深度 1。

```1:48:（openclaw src/config/agent-limits.ts）
const MIN_AGENT_MAX_CONCURRENT = 8;
const MAX_AGENT_MAX_CONCURRENT = 16;
// ...
export const DEFAULT_SUBAGENT_MAX_CONCURRENT = 8;
export const DEFAULT_SUBAGENT_MAX_CHILDREN_PER_AGENT = 5;
export const DEFAULT_SUBAGENT_MAX_SPAWN_DEPTH = 1;
```

源：[`src/config/agent-limits.ts`](https://github.com/openclaw/openclaw/blob/7d6dfa1939b46cf093389355dbe67254e7bf0756/src/config/agent-limits.ts)、测试 [`src/config/config.agent-concurrency-defaults.test.ts`](https://github.com/openclaw/openclaw/blob/7d6dfa1939b46cf093389355dbe67254e7bf0756/src/config/config.agent-concurrency-defaults.test.ts)

`[文档]` 多个 agent 活在 **同一个 Gateway 进程**里，不是一 agent 一容器（sandbox 才是可选容器）。隔离：每 agent workspace + `agentDir` + 每 agent SQLite；可选 `sandbox.scope: "agent"` 一 agent 一容器。

`[推测]` inbound 到不同 binding 的两条消息会同时进两个 agent loop——文档和 `maxConcurrent` 就是为这个写的。本单没有跑 Gateway 打两条 inbound 做运行时证明。

`[文档]` 子 agent 是后台 run：`sessions_spawn` 立刻返回 run id，完成后再 announce 回请求者会话。需要等结果时用 `sessions_yield`，禁止轮询。

#### 3.A.3 谁派活

没有 Pillow 那种「总控」角色。有三种派活：

1. **人对 inbound 路由**（bindings）。`[代码]` `resolve-route.ts` 按 peer → parent peer → wildcard → guild+roles → guild → team → account → channel 分层，**第一命中获胜**，否则落到 sole/default agent。

源：[`src/routing/resolve-route.ts`](https://github.com/openclaw/openclaw/blob/7d6dfa1939b46cf093389355dbe67254e7bf0756/src/routing/resolve-route.ts)、[`src/routing/bindings.ts`](https://github.com/openclaw/openclaw/blob/7d6dfa1939b46cf093389355dbe67254e7bf0756/src/routing/bindings.ts)

2. **父 spawn 子**。`[代码]` `sessions_spawn` 有 TypeBox schema：`task`、`agentId`、`model`、`runtime: subagent|acp`、`mode: run|session`、`context: isolated|fork`、`sandbox`、`visible` 等。派单是结构化的。

源：[`src/agents/tools/sessions-spawn-tool.ts`](https://github.com/openclaw/openclaw/blob/7d6dfa1939b46cf093389355dbe67254e7bf0756/src/agents/tools/sessions-spawn-tool.ts)、文档 [`docs/tools/subagents.md`](https://github.com/openclaw/openclaw/blob/7d6dfa1939b46cf093389355dbe67254e7bf0756/docs/tools/subagents.md)

3. **agent→agent 消息**。`[文档]` `tools.agentToAgent.enabled` 默认 `false`，还要 allowlist。

孩子产出：`[文档]` 完成事件带 `Result` / `Status` / token 统计，作为 **内部上下文** 回到父会话；父决定要不要对用户说话。投递失败会重试约 30 分钟，堵住的结果留 7 天。

#### 3.A.4 凭据

`[文档]` auth profiles 在每 agent 的 SQLite。警告写得很硬：

- 不要跨 agent 复用 `agentDir`（auth/session 会撞）。
- 二级 agent 的 OAuth 过期或 refresh 失败时，OpenClaw **读穿到 default/main 同 profile id 的最新 token**，不把 refresh token 拷进二级 store。
- 要完全独立的 OAuth 账号：在那个 agent 里重新登录。
- 手动拷贝只允许静态 `api_key` / `token`；OAuth refresh 默认不可移植（`copyToAgents` 显式 opt-in）。

`[代码]` 公共 barrel 暴露 `buildPortableAuthProfileStoreForAgentCopy`、`refreshOAuthCredentialForRuntime`、`withAuthProfileStoreAgentDir`，证实 store 是按 `agentDir` 分的。

源：[`src/agents/auth-profiles.ts`](https://github.com/openclaw/openclaw/blob/7d6dfa1939b46cf093389355dbe67254e7bf0756/src/agents/auth-profiles.ts)

切换 agent **不必**重登，除非那个 agent 自己没有可用凭据——那时会落到 main。这对 Pillow 是反面教材。

#### 3.A.5 权限与审批

`[文档]` 每 agent 可配 `tools.allow` / `tools.deny` 和 sandbox。文档示例：家庭 bot 只留 `read`，拒绝 `exec/write/edit`。这就是「这个只读、那个可写」。

`[代码]` 子 agent 还有硬拒绝，和配置无关：

```52:81:（openclaw src/agents/agent-tools.policy.ts）
const SUBAGENT_TOOL_DENY_ALWAYS = [
  "gateway", "agents_list", "session_status", /* automations */,
  "message", "sessions_send", "conversations_list", "conversations_send", "conversations_turn",
];
const SUBAGENT_TOOL_DENY_LEAF = [
  "subagents", "sessions_list", "sessions_history", "sessions_search", "sessions_spawn",
];
```

源：[`src/agents/agent-tools.policy.ts`](https://github.com/openclaw/openclaw/blob/7d6dfa1939b46cf093389355dbe67254e7bf0756/src/agents/agent-tools.policy.ts)

`[代码]` 工具前审批：`allow-once` / `allow-always` / `deny`；超时 **fail-closed**（`timeoutBehavior:"allow"` 已弃用并被忽略）。

源：[`src/agents/agent-tools.before-tool-call.approval.ts`](https://github.com/openclaw/openclaw/blob/7d6dfa1939b46cf093389355dbe67254e7bf0756/src/agents/agent-tools.before-tool-call.approval.ts)

`[文档]` 组织向「delegate」另有三档：只读+草稿 / 代发 / 主动定时。硬规则写在 `SOUL.md`/`AGENTS.md`，**同时**用 Gateway 工具策略兜底——人格文件会被注入，不能当唯一闸门。

#### 3.A.6 UI

`[文档]` 新增 agent 的最短路径：

1. `openclaw agents add <id>`（可带 `--workspace` / `--model` / `--bind`）
2. 给该 agent 建 channel 账号（一个 bot / 一个 WhatsApp 号）
3. 写 bindings
4. `openclaw gateway restart` + `openclaw agents list --bindings`

Control UI：有子 run 的父会话侧栏可展开；选中孩子打开孩子的聊天并保留层级。`visible: true` 的 spawn 出现在仪表盘。`/subagents list|log|info` 查当前会话的孩子。

「现在谁在说话」：靠 channel 账号（不同 bot/号码）、`IDENTITY.md` 名字/emoji、以及侧栏里的孩子行。不是 IDE 里的 speaker chip。

---

### 3.B Hermes Agent（爱马仕）

仓库：<https://github.com/NousResearch/hermes-agent>  
定位：长期跑、会自己写 skill 的单主 agent。多 agent 的一等能力是 **`delegate_task` 拉一次性孩子**，不是 Pillow 那种长期槽位团队。桌面有 Profiles，但本单没读到 `profiles.py`。

#### 3.B.1 建模

`[代码]` 一个干活的 agent 是 `AIAgent` 实例。`delegate_task` 在主线程构造孩子，再丢进线程池跑。孩子：全新对话、自己的 `task_id`/终端会话、继承父 toolset（再扣掉黑名单）、系统提示只来自 `goal` + `context`。

源：[`tools/delegate_tool.py`](https://github.com/NousResearch/hermes-agent/blob/7626105380b78bd360a20fe3ea3752046efc4a45/tools/delegate_tool.py) 文件头与 `_build_child_agent`

`[文档]` 孩子默认用父模型；`delegation.model` / `delegation.provider` / `delegation.base_url` 可把 **所有** 孩子钉到另一套供应商。没有 per-task 模型参数（kanban 才有）。

`[推测]` 用户可同时拥有多个 Profile（桌面 `create-profile-dialog.tsx` 存在）。每个 Profile 是否等于一个独立运行时、能否并跑，**未读到代码，不能当事实。**

#### 3.B.2 并发

`[代码]` **真并行**，不是假 spawn：

- 默认 `_DEFAULT_MAX_CONCURRENT_CHILDREN = 3`
- `delegation.max_concurrent_children` 或 env `DELEGATION_MAX_CONCURRENT_CHILDREN`
- 地板 1，**无上限**；`>10` 打一次成本警告
- 后台与同步 batch **共用一个上限**；满了新的 async dispatch **直接拒绝**，退回同步，避免模型把后台工作堆爆
- 实现：`DaemonThreadPoolExecutor`
- 默认深度 1（扁平）。`role="orchestrator"` 且 `max_spawn_depth>=2` 才允许再派
- 文档自己算过：`depth=3` × `concurrent=3` → 最多 27 个叶子

源：同文件 `_get_max_concurrent_children`、`_get_max_async_children`、`MAX_DEPTH`

`[文档]` 隔离默认是 **共享父工作目录**。`delegation.worktree_isolation: true` 时每个孩子一个 git worktree（`hermes-subagent/<id>`）。非 git / 远程后端则静默退回共享目录。

`[文档]` 进程消失时正在跑的孩子记成 `unknown`，不假装执行是耐久的。已完成未投递的结果可以从 `state.db` 恢复。

GitHub issue [#5204](https://github.com/NousResearch/hermes-agent/issues/5204) 曾问「subagents 是不是假 spawn」。当前 `main` 的线程池 + 可配并发，已经不是「一次一个」——但那是 **2026-04 的 issue**，以本次读到的代码为准。

#### 3.B.3 谁派活

隐式总控 = 正在跟用户说话的那个 agent。没有 Librarian 这种产品角色。

派单 schema：`delegate_task(goal, context, tasks[], role, max_iterations, …)`。`[文档]` 反复强调孩子 **零历史**，父必须把所需上下文写进 `goal`/`context`。

回来的路径：

- 顶层：立即返回 handle，完成后作为 **新一轮消息** 进会话（不塞进正在进行的 tool 结果中间，以免打坏 role 交替和 prompt cache）
- orchestrator 孩子：当前 turn 里等完再综合
- `[代码]` `agent/subagent_lifecycle.py` 给插件暴露不可变契约（`SubagentHandle` / 状态机），不把 `AIAgent` 对象递出去

源：[`agent/subagent_lifecycle.py`](https://github.com/NousResearch/hermes-agent/blob/7626105380b78bd360a20fe3ea3752046efc4a45/agent/subagent_lifecycle.py)、[`tools/async_delegation.py`](https://github.com/NousResearch/hermes-agent/blob/7626105380b78bd360a20fe3ea3752046efc4a45/tools/async_delegation.py)

#### 3.B.4 凭据

`[代码]` `_build_child_agent`：无 override 时孩子拿父的 API key / endpoint；有 `delegation.*` override 时整批孩子换供应商。注释写明用途：父走贵的 Portal，孩子走便宜的 OpenRouter。

`[文档]` 孩子继承父的 credential pool（限流时轮换 key）。

加密与「换 profile 要不要重登」：`hermes_cli/profiles.py`、`agent/credential_pool.py` 本单 **429 未读**。只知道测试树里有 `test_credential_pool_*.py` 和 `test_multiplex_credential_isolation.py`。

#### 3.B.5 权限与审批

`[代码]` `tools/approval.py` 是危险命令的单一事实源：模式检测、**按 `session_key` 的审批状态**、CLI/gateway 提示、辅助 LLM「smart」自动放行、`config.yaml` 永久允许名单。模式只有 `manual` / `smart` / `off`。`HERMES_YOLO_MODE` **在 import 时冻结**，防止 skill 中途改环境变量绕过。

源：[`tools/approval.py`](https://github.com/NousResearch/hermes-agent/blob/7626105380b78bd360a20fe3ea3752046efc4a45/tools/approval.py)（`_normalize_approval_mode`、`is_approval_bypass_active_for_session`）

`[代码]` 子 agent 跑在线程池里，**继承不了** 父 TUI 的审批 callback。默认安装 `_subagent_auto_deny`；`delegation.subagent_auto_approve: true` 才 `_subagent_auto_approve`（返回 `once`）。Gateway 会话走 per-session 队列，不走这条 TLS callback。

这不是「按 agent 授写权限」。这是「为了不卡死 TUI，孩子默认不能做危险命令」。和 Pillow 的人类一票不同。

孩子硬黑名单：`delegate_task`（leaf）、`clarify`、`memory`、`send_message`、`cronjob`。父没有的 toolset，孩子也拿不到。

#### 3.B.6 UI

`[文档]` CLI 用树看每个孩子的工具调用；gateway 把进度批量回传。每任务一份 `cache/delegation/live/<id>/task-n.log`，可 `tail -f`。

`[推测]` 桌面 Profiles：创建/重命名/删除对话框在仓库里。未读实现，不知道新建要几步、是否显示「当前 profile」。

「谁在说话」：父会话里只进孩子的 **摘要**；过程在树/log 里。不是多人格并列聊天。

---

### 3.C CrewAI（补位：确认存在的角色制多 agent）

仓库：<https://github.com/crewAIInc/crewAI>  
定位：给开发者写「一队有角色的 agent」的库。不是桌面槽位产品。放进来是因为它把 **角色 + 总控 + 任务产出回传** 写得很干净，正好对照 Pillow 的 Controller/Librarian。

#### 3.C.1 建模

`[代码]` `Agent` 是 Pydantic 模型：`role`、`goal`、`backstory`、`llm`、`tools`、`allow_delegation`、`max_iter`、`max_rpm`。`llm` 可以是字符串或 `BaseLLM`，**每 agent 可换供应商**。

源：[`lib/crewai/src/crewai/agent/core.py`](https://github.com/crewAIInc/crewAI/blob/27083f41319a7d71d4c65d541c558d79ad2a30a1/lib/crewai/src/crewai/agent/core.py)（class `Agent` 文档字符串与 `llm` 字段）

`[代码]` `Crew` 持有 `agents`、`tasks`、`process`（默认 `Process.sequential`）、可选 `manager_llm` / `manager_agent`、`max_rpm`。

源：[`lib/crewai/src/crewai/crew.py`](https://github.com/crewAIInc/crewAI/blob/27083f41319a7d71d4c65d541c558d79ad2a30a1/lib/crewai/src/crewai/crew.py)

几个：一个 Crew 里多少个都能塞。区分靠对象身份 + `role` 字符串。没有跨进程的「用户拥有 N 个长期槽位」。

#### 3.C.2 并发

`[代码]` **默认不是真并发。**

- sequential：按 `tasks` 列表走，上一任务的 output 当下一任务 context。
- `task.async_execution`：用 `Future` 重叠。校验：**crew 必须以至多一个 async 任务收尾**；`ConditionalTask` 不能 async。
- `kickoff_async`：把同步 `kickoff` 丢进线程。另有原生 async 路径 `_arun_*`。

源：同文件 `validate_end_with_at_most_one_async_task`、`_execute_tasks`、`kickoff_async`

隔离：同一 Python 进程。没有 per-agent 工作目录/容器。`max_rpm` 是速率上限，不是并发槽位数。

#### 3.C.3 谁派活

`[文档]`/`[代码]` 两种过程：

- sequential：作者预绑 `task.agent`。没有运行时总控。
- hierarchical：必须有 `manager_llm` 或 `manager_agent`。manager 的 `allow_delegation=True`，**禁止自带 tools**（有 tools 直接抛错），只用 `AgentTools(agents=...)` 去派。

产出：`TaskOutput` 链 → `CrewOutput`。这是结构化对象，不是「把自然语言贴回群聊」。

#### 3.C.4 凭据

每 Agent 一个 `llm`。密钥通常是进程环境变量或 CrewAI 平台登录（CLI 有 Auth0/Entra/Keycloak/Okta/WorkOS）。**没有**「每个 agent 一份 DPAPI 密文」。换 agent 不用登录；换的是对象上的 llm。

#### 3.C.5 权限与审批

没有人类批准。权限 = 你塞进 `tools=` 的东西。hierarchical 的 manager 被框架扒光工具，只准派不准自己下手——这是代码级分级，不是审批 UI。

#### 3.C.6 UI

开发者 UX：YAML `config/agents.yaml` + `crewai run` + 可选 TUI。没有「用户新建一个槽位」向导。说话者靠 verbose 日志里的 role 名。

---

### 3.D Microsoft AutoGen / Magentic-One（补位：确认存在的编排器）

仓库：<https://github.com/microsoft/autogen>  
定位：可编程多 agent 运行时。AgentChat 的 Team 是 **共享上下文的群聊**，一次一个说话者。Magentic-One 在上面加了账本编排器。

#### 3.D.1 建模

`[代码]` `BaseGroupChat`：参与者是 `ChatAgent | Team`，名字必须唯一。默认 runtime 是 `SingleThreadedAgentRuntime`。

源：[`python/packages/autogen-agentchat/src/autogen_agentchat/teams/_group_chat/_base_group_chat.py`](https://github.com/microsoft/autogen/blob/027ecf0a379bcc1d09956d46d12d44a3ad9cee14/python/packages/autogen-agentchat/src/autogen_agentchat/teams/_group_chat/_base_group_chat.py)

每个 agent 自己的 `ChatCompletionClient`，可换供应商。身份是代码里的 `name` + description，不是数据库行。

#### 3.D.2 并发

`[代码]` **默认只是「能配多个、一次跑一个」。** 单线程 runtime + manager 选下一名说话者。这和 README 里「multi-agent collaboration」不矛盾——协作可以是轮流的。

隔离：共享群聊消息。参与者看到彼此发布的 `BaseChatMessage`。没有 per-agent 文件系统/进程隔离。

`max_turns` 限制轮数，不是并行度。

#### 3.D.3 谁派活

`[代码]` Magentic-One Orchestrator：维护 `_task` / `_facts` / `_plan`，用提示词让模型产出 **JSON ledger**（`LedgerEntry`），再决定谁说话或是否收工。这是结构化编排，不是纯自然语言转述。

源：[`.../_magentic_one/_magentic_one_orchestrator.py`](https://github.com/microsoft/autogen/blob/027ecf0a379bcc1d09956d46d12d44a3ad9cee14/python/packages/autogen-agentchat/src/autogen_agentchat/teams/_group_chat/_magentic_one/_magentic_one_orchestrator.py)

`[代码]` `SelectorGroupChatManager`：用 LLM 或 `selector_func` 选下一名；可禁连续同一人说话。

产出：被选中的 agent 的回复发布到群聊，所有人看见。和 Pillow「孩子产出只回总控、用户只跟总控说话」相反。

#### 3.D.4–3.D.6 凭据 / 审批 / UI

凭据在各 agent 的 client 里。无人类批准。UI 是库/notebook/Magentic-One CLI。说话者 = 消息上的 agent 名。

---

### 3.E Pillow 对照（只读本仓库，用来写建议）

不是调研对象，但建议必须钉在现有代码上。

**建模。** `[代码]` 槽位是项目作用域 SQLite 行：一个形状 `auth + role`。治理由 auth 推导，不能单独存一个相反的治理字段。角色：`Controller`（每项目至多一个，必须受治理）、`Librarian`、`Custom(String)`。

源：`crates/storage-sqlite/src/runtime_slot.rs`，`crates/role-dispatch/src/types.rs`

**并发（和产品话不一致）。** `[代码]`

- `RuntimeServiceState` 只有一个 `Option<ConversationService>` 和一个 `active_slot_id`。
- `activate_grok` **先 shutdown 上一个** 再换上新的。
- 生产宿主 `HostConfig::new(1, false, 64)` → `max_concurrent_turns = 1`。
- 设置页注释写明：宿主 list DTO **没有 active 标志**，面板用 session-local `startedSlotId`。

源：`apps/desktop/src-tauri/src/runtime_service_state.rs`，`apps/desktop/src-tauri/src/runtime_composition.rs`（`production_grok_host_config`），`apps/desktop/src/features/settings/RuntimeSlotsSection.tsx`

所以：**槽位记录共存是真的；多个 Grok 对话循环同时工作不是真的。** 第二个槽位的「同时干活」只能走派单进程，不能走第二个活着的 `ConversationService`。

**派单。** `[代码]` 只有当前项目的 Controller 能发起。`build_assignment` 拒绝自派、角色错配、空任务。`ProposedCommand` 是结构化 argv（`--pillow-task`），禁止 shell 拼接。宿主把 assignment 送进 **现有的** `process.run` 提案链，不另开批准条。治理标签从目标 auth 冻结，LocalCli 的结果只能记录、不能声称受治理执行。

源：`crates/role-dispatch/src/assignment.rs`，`crates/runtime-host/src/dispatch.rs`，`apps/desktop/src-tauri/src/role_dispatch_commands.rs`

**凭据。** `[代码]` `DpapiFileSecretStore`：每条 `secret_ref` 一个密文文件，无明文，当前 Windows 用户 DPAPI，无 Pillow 主密钥。`SecretBytes` Debug 脱敏、Drop 清零。

源：`crates/runtime-slots/src/secret.rs`

**UI。** `[代码]` Settings 列表。新建两步：`provider` → `connect`（认证方法 + 角色 + 显示名）。Workbench 把派单目标选择器和 **同一条** `CommandApprovalBar` 接在一起。

---

## 4. 对 Pillow 的建议

分三类。每条都是可执行的动作，不是「可以考虑加强」。

### 4.1 可以直接借鉴的

1. **书面钉死两层「agent」，并在 UI 用两个词。**  
   OpenClaw 把「持久人格」和「sessions_spawn 孩子」分开；Hermes 把「主会话」和「delegate_task 孩子」分开。Pillow 现在用「槽位」同时指长期身份和被派去干活的运行时。  
   **动作：** 产品文案固定：槽位 = SQLite 身份（auth+role）；派单 = 一次 assignment/process。不要把 assignment 再叫「开一个槽位」。

2. **把 assignment 完成做成推送事件，不要让总控轮询。**  
   OpenClaw 的 `sessions_spawn` + `sessions_yield` + 完成 announce，比「派出去就靠人看收据」完整。Hermes 的「完成后开新 turn，不插入正在进行的 tool 结果」保护了 transcript 不变量。  
   **动作：** 在现有 process 收据落库之后，向 **发起该 assignment 的 Controller 会话** 投一条内部完成事件（assignment_id、target_slot_id、exit、截断后的 stdout）。Controller 的下一轮模型可见它。禁止为了等孩子去转 process 列表。

3. **说话者必须是时间线上的字段，不是 Settings 里的 session 变量。**  
   OpenClaw 侧栏按孩子 run 展开；AutoGen 消息带 speaker 名。Pillow 宿主 DTO 没有 active，面板用 `startedSlotId`。  
   **动作：** 每条时间线事件 DTO 带 `slotId` + `role`（闭集 key，和现有 gateway allowlist **同一 PR 放宽**）。主舞台画 speaker chip：「总控」「信息管理员：某某」。派单 run 挂在发起这条消息下面，学 OpenClaw 侧栏，不要另开一个聊天顶栏。

4. **在槽位上加工具天花板，不要塞进治理枚举。**  
   OpenClaw 的 `tools.allow/deny` 和 sandbox 证明：只读人格是配置，不是另一种 auth。Pillow 的 Full/None 只回答「副作用能不能进提案链」，不回答「这个角色该不该提出 write」。  
   **动作：** 槽位增加 `tool_allow` / `tool_deny`（或 `capability: read|read_write|process`），**独立于** `SlotAuth::governance()`。Librarian 默认 deny `write` / `process.run`。加测试：Full 治理 + deny write 的槽位，提案在进入批准条之前就被域拒绝。不要新增第三种 `RuntimeSlotGovernance`。

5. **派单带 worker 模型，不要全局钉死。**  
   Hermes 的「父用前沿、孩子用便宜模型」是对的；它错在 `delegate_task` 没有 per-task 模型，只能全局 `delegation.model`。  
   **动作：** `Assignment` / `ProposedCommand` 增加可选 `model_id`，默认用目标槽位当前模型。设置页给 Librarian/Custom 一个「派单时用这个模型」。不要加进程级 `PILLOW_WORKER_MODEL`。

6. **学 CrewAI 的一点：hierarchical 的经理不准自己带工具。**  
   CrewAI 对 manager.tools 非空直接抛错，经理只准派。  
   **动作：** Controller 对话里暴露给模型的「派单」工具保持；不要再给 Controller 一条「以 Librarian 的身份直接 write」的捷径。写文件必须是目标槽位提出的提案，或用户在总控会话里自己批的总控提案。身份不要串。

### 4.2 我们已经做得更好、不要改的

1. **不要把域状态搬进 `openclaw.json` 那种家目录大 JSON。**  
   OpenClaw 的权威配置是一份 JSON + 每 agent 文件树。Pillow 已经把槽位放在 SQLite，项目作用域。保持。bindings/人格 markdown 当补充资料可以，当身份源不行。

2. **不要换掉「提案 → 人类批一次 → 宿主执行 → 收据」。**  
   Hermes 有 YOLO / `approvals.mode: off` / 子 agent 自动拒绝或自动放行。OpenClaw 有 exec 审批，但还有「站立命令 / Tier 3 主动」可以绕过逐次批准。Pillow 把派单也接进同一条 process 链（`crates/runtime-host/src/dispatch.rs` 写明没有第二条执行器）。这是产品核。  
   **不要** 为了「更像龙虾」给子槽位开一条不经提案的 spawn。  
   **不要** 加 `subagent_auto_approve` 或产品级 YOLO。

3. **不要学 OpenClaw 的 OAuth 回落到 main。**  
   那是跨人格凭据泄漏，和 Pillow「每槽位私有 home / 拒绝被登录文件污染」相反。二级槽位缺凭证就显示未登录，不要静默用总控的 token。

4. **不要学 AutoGen 的共享群聊当产品模型。**  
   用户跟总控说话，总控派单，孩子产出回总控。不要把 Librarian 的中间推理倒进用户时间线，除非用户点开那条 assignment。

5. **不要把治理从 auth 拆成可手填字段。**  
   LocalCli 永远是 `None`，不能 Start 对话。Controller 必须受治理。矛盾不可表示。OpenClaw 用人格文件写「不许写」，再用 Gateway 策略兜底——两层会漂。Pillow 现在一层推导，保持。

6. **不要把密钥放进配置或孩子环境的可打印字段。**  
   DPAPI + `SecretBytes` + DTO 无 key，比 CrewAI/AutoGen 的「环境变量里有就行」和 OpenClaw 的「JSON 里写 `DISCORD_BOT_TOKEN_CODING` 这种占位」更干净。保持。

7. **槽位记录共存、角色可派、闭集 DTO——这些已经对了。**  
   和 OpenCode「选一个供应商」的差别，首先是 **域里能有多个身份**，不是宿主里已经有 N 个 ACP 孩子。把「记录共存」做实、把「活运行时并存」单独立项，不要混成一句广告。

### 4.3 我们做错了、应该换掉的

1. **产品话「槽位可以共存、同时工作」和宿主实现相反。换掉这句话，或换掉宿主。**  
   现在 `activate_grok` 是替换语义，生产还把单宿主 turn 并发打成 1。  
   **二选一，写进 ADR，下个迭代就做：**  
   - **A. 诚实：** 对外改成「可配置多个槽位；当前项目只有一个活对话（通常是总控）；其他角色以受治理派单的形式工作」。设置页 Start 按钮改成「设为当前对话」，并在 list DTO 加 `active`（与渲染 allowlist 同期放宽 + 接受测试）。  
   - **B. 真并跑：** `RuntimeServiceState` 改成 `HashMap<(project_id, slot_id), ConversationService>`，Start 不再 shutdown 别人；加项目级并发预算（先 2：总控 + 一个工人就够）。没有 B 之前不要在 UI 画两个「正在输入」。  
   现在的中间态（话术是 B，代码是 A）是错的。

2. **`startedSlotId` 只活在 Settings 面板内存里——换掉。**  
   刷新/换窗口就丢，list DTO 也不承认。  
   **动作：** 宿主记住每项目 `active_slot_id`（即使选 A 也要持久化），列表接口返回它。不要用 React state 冒充运行时事实。

3. **角色现在只是派单地址字符串，不能表达能力。换掉「只有 Custom 名字」这件事。**  
   Librarian 和 Custom("爬虫") 对工具层是一样的。OpenClaw 用 allow/deny 做出只读家庭 bot；CrewAI 用 manager 禁工具做出只派不干。  
   **动作：** 见 4.1.4。至少给 `librarian` 一个代码里写死的默认天花板，而不是靠提示词。

4. **不要把「Start 槽位」和「准备 NativeAdapter」挤在同一个按钮语义里。**  
   `[代码]` `start_runtime_slot_impl` 对 NativeAdapter 路径只返回 `native_adapter_prepared`，**故意不** `activate_grok`。用户以为 Start 等于「这个槽位开始干活了」。  
   **动作：** UI 把「可对话（Grok ACP 已激活）」和「可被派单（路径已准备）」分成两个状态点。NativeAdapter 槽位不要显示成已激活的对话运行时。

5. **若真要并跑写代码：默认共享项目 cwd 是错的。**  
   Hermes 已经踩过，后来才加 opt-in worktree。OpenClaw 的 workspace 也只是默认 cwd。  
   **动作：** 两个受治理槽位同时带 write 时，要么串行化 write 提案，要么第二个强制 worktree。不要「先让他们撞，再看收据」。

---

## 5. 维护者决策用的对照（极短）

别人在这块怎么组织：

- **龙虾**：一个网关、多份人格配置、inbound 路由、孩子后台跑完再 announce。工具权限按人格切。凭据按 `agentDir` 切，但 OAuth 会回落到 main。
- **爱马仕**：一个主脑 + 线程池一次性工人。工人默认继承钥匙和工具，审批为了不卡 UI 被改成自动拒/自动准。
- **CrewAI / AutoGen**：开发者编排。角色很强，产品级槽位/密钥/批准几乎没有。AutoGen 还把大家关在同一间聊天室轮流说话。

我们在重复造的：身份列表、角色名、把活派给另一个角色、每角色一个模型。

我们不如人的：活运行时并跑（话术超前）、孩子完成回主对话、按角色切工具、说话者可见性。

我们更好、别改的：SQLite + 推导治理 + 一条批准链 + 分槽位 DPAPI + Controller 约束。

---

MULTIAGENT-RESEARCH-COMPLETE
