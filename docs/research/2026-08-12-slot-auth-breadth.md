# Pillow 槽位认证广度与可治理性矩阵

> 日期：2026-08-12  
> 调研槽位：codex1  
> 状态：完成  
> 仓库快照：`570828c`  
> 性质：只读调研；除本报告外未修改文件；未运行门、测试或 E2E。

## 调研边界与证据口径

本文把三件经常被混在一起的事情分开：

1. **认证广度**：某个运行时能否得到调用特定模型服务所需的凭据。
2. **协议可驱动性**：外部客户端能否通过 ACP 或类似协议给 agent 发消息、接收事件。
3. **副作用可治理性**：agent 的文件读写和进程执行是否真的回到 Pillow，由 Pillow 在 UI 以下实施提案、批准、执行和审计。

“可完全治理”采用严格合取：**原生工具可被限制** ∧ **文件读与写均交回客户端** ∧ **进程/terminal 交回客户端**。缺任一项即不是完全治理；能看到工具调用、收到权限请求或记录 terminal 输出，不等于宿主执行了该副作用。这个判据与仓库已有的三个半开陷阱一致：`docs/handoff/2026-08-11-pillow-controller-handoff.md:75-85`。

证据优先级如下：本次本机实际输出；Pillow 当前工作树 `file:line`；上游官方仓库源文件/官方文档 URL。上游 `main`/`dev` 是可变分支，正式复用前必须固定完整提交。本文没有读取任何认证文件，也没有输出任何 token、key、cookie 或其片段。

## Q1. grok build 能接受哪些认证

### 结论

**回答要拆成两层：**

- 对“是否存在第三方 OAuth 机制”：**能**。当前 grok-build 除 xAI/grok.com Web 登录和 API key 外，还有通用 customer OIDC、OAuth 2.1 Authorization Code + PKCE，以及由外部认证提供器返回短期 bearer 的 `External` 模式。
- 对“能否把 Claude Pro/Max 或 ChatGPT Plus/Pro 的订阅登录直接塞进 Grok 壳”：**不能，至少没有上游支持的直接机制**。代码中没有 Anthropic/OpenAI 消费者订阅适配器。通用 OIDC 需要明确的 issuer、client registration、scopes、audience/resource，并且目标 API 或受控代理必须接受该发行者签发的凭据；“能接任意 OIDC”不等于“能把另一家消费订阅凭据当成 xAI/API 凭据”。

因此，总控倾向“Claude 订阅不能直接塞进 grok 壳”得到证实；但更宽泛的说法“grok-build 完全不支持第三方 OAuth”是错误的。

### 证据

- 上游认证配置把首选方法定义为 `ApiKey` 或 `Oidc`，并在 customer 配置中出现 `oidc`、`oauth2`、`auth_provider_command`；OIDC 必要字段包括 issuer、client ID、scopes、audience，OAuth 配置明确为授权码 + PKCE：[grok-build `auth/config.rs:41-130`](https://github.com/xai-org/grok-build/blob/main/crates/codegen/xai-grok-shell/src/auth/config.rs#L41-L130)。
- 上游运行时认证模式不是二选一，而是 `WebLogin`、`Oidc`、`External`、`ApiKey`：[grok-build `auth/model.rs:23-97`](https://github.com/xai-org/grok-build/blob/main/crates/codegen/xai-grok-shell/src/auth/model.rs#L23-L97)。
- 外部认证提供器执行后，只把其 stdout 解析为会话凭据；刷新有短超时，失败返回无凭据。它是一个“凭据取得钩子”，不是 Claude/OpenAI 订阅转换器：[grok-build `auth/external_auth.rs:9-84`](https://github.com/xai-org/grok-build/blob/main/crates/codegen/xai-grok-shell/src/auth/external_auth.rs#L9-L84)。
- 官方用户指南分别描述 xAI 登录、customer OIDC 和 external provider，并给出认证选择优先级：[认证指南 `02-authentication.md:50-141`](https://github.com/xai-org/grok-build/blob/main/crates/codegen/xai-grok-pager/docs/user-guide/02-authentication.md#L50-L141)、[同文件 `:253-266`](https://github.com/xai-org/grok-build/blob/main/crates/codegen/xai-grok-pager/docs/user-guide/02-authentication.md#L253-L266)。
- Pillow 当前 API-key 路径只生成每项目/每槽位私有 `.grok/config.toml`，字段为 `model`、`base_url`、`env_key`、`name`，并拒绝被登录文件污染的私有 home：`crates/runtime-slots/src/grok_config.rs:13-56`。这证明 OpenAI-compatible endpoint 路径存在，但没有证明第三方消费订阅可移植。

### 决策含义

可接受的等价扩展是“Pillow 作为正式 OAuth client，或调用经过审计的企业 IdP/凭据 broker，取得**目标端点明确接受**的受众限定凭据”。不可接受的等价扩展是“读取 Claude/Codex CLI 的订阅会话，再把 bearer 送到另一个端点”。后者既缺少协议证据，也有账号和凭据泄露风险。

## Q2. 各 CLI 的可治理性矩阵

### 结论

在本次严格判据下，已取得完整证据的只有 **Pillow 当前 Grok ACP 适配器**可标为“可完全治理”。Claude Agent ACP、Codex ACP、OpenCode ACP、Gemini ACP 都能被外部客户端驱动，但没有同时证明“工具限制 + 双向 client fs + client terminal”三项；它们应当作为**只能记录产出/半开**的 external runtime，不能因有 ACP 或权限事件就升级为受治理槽位。

这里的“只能记录产出”是 Pillow 的信任标签，不是否定这些工具自身的权限、沙箱或审计功能。

### 矩阵

| CLI / 适配器 | ACP / 类似协议 | 客户端侧 fs（读 ∧ 写） | 客户端侧 terminal | 原生工具白名单/限制 | 严格结论 |
| --- | --- | --- | --- | --- | --- |
| Pillow-hosted Grok | 有，JSON-RPC/ACP | **有**；声明 read + write，且与原生 `write` 同 profile 耦合 | **有**；声明 terminal，且与原生 shell 工具同 profile 耦合 | **有**；固定 allowlist，MCP 元工具另行拒绝 | **可完全治理** |
| Claude Agent ACP | 有 | **未确定**；适配器实现 client fs 回调，但未找到 Claude 原生 Read/Write 必然走这些回调的证据 | **无已证实交回**；找到的是运行时 terminal 事件/输出，不是 client 创建和执行 | **有**；SDK options 有 `tools`、`disallowedTools`、`canUseTool` | **只能记录产出（半开）** |
| Codex ACP | 有 | **无已证实交回**；初始化未宣告 reverse fs，源中未找到 client `readTextFile`/`writeTextFile` 调用 | **无已证实交回**；有 terminal 输出事件，不等于 client terminal | **无闭合逐工具 allowlist 证据**；只有 coarse approval/sandbox mode | **只能记录产出** |
| OpenCode ACP | 有 | **不满足**；连接类型有可选 client write，但没有成对 client read；初始化也未宣告完整 fs | **无已证实交回** | **有**；工具与 permission 支持 allow/ask/deny 和通配规则 | **只能记录产出（半开）** |
| Gemini CLI ACP | 有 | **有文档证据**；ACP 模式可代理 fs 到客户端 | **无**；官方 shell 工具仍由 Gemini 进程使用本地 PTY/child process | **有**；core tools allowlist/排除能力 | **只能记录产出（半开）** |

### 共同协议边界

- ACP 架构是客户端与 agent 之间的 stdio JSON-RPC；协议可以承载 tool call、permission request 和 session update，但协议本身不是 OS 沙箱：[ACP Architecture](https://agentclientprotocol.com/get-started/architecture)。
- ACP 对“额外目录”的设计说明明确区分 client-mediated fs 与 agent 直接访问文件系统；后者要靠 OS/runtime sandbox 限制，不能由“用了 ACP”自动推出：[ACP Additional Directories](https://agentclientprotocol.com/rfds/additional-directories)。

### Pillow-hosted Grok

- 固定原生工具集合为 `ask_user_question`、`write`、`run_terminal_cmd`，并拒绝能接触环境 MCP 的两个元工具：`crates/adapter-grok-acp/src/rpc/methods.rs:5-52`。
- 初始化同时声明 `fs.readTextFile=true`、`fs.writeTextFile=true`、`terminal=true`：`crates/adapter-grok-acp/src/rpc/methods.rs:55-66`。
- session new/load 都带同一 profile，且不由调用方注入 MCP：`crates/adapter-grok-acp/src/rpc/methods.rs:69-87`。
- 两条耦合测试把“允许 native write 必须同时开放 client write”和“允许 native terminal 必须同时开放 client terminal”钉在一起：`crates/adapter-grok-acp/src/rpc/methods.rs:127-194`。
- 当前槽位模型仍然明确区分：`ApiKey`/`GrokLogin` 导出 `Full`，`LocalCli` 导出 `None`：`crates/storage-sqlite/src/runtime_slot.rs:44-97`。外部 CLI 测试只执行 status 命令，随后拒绝启动对话：`crates/runtime-slots/tests/slot_service.rs:215-252`。

### Claude Agent ACP

- 官方适配器声明自己是 Claude Agent SDK 的 ACP server，并支持 permission requests、terminal updates 和 client MCP：[官方 README](https://github.com/agentclientprotocol/claude-agent-acp#features)。
- 适配器确有 client `readTextFile`/`writeTextFile` 包装：[Claude ACP `acp-agent.ts:1437-1490`](https://github.com/agentclientprotocol/claude-agent-acp/blob/main/src/acp-agent.ts#L1437-L1490)、[`acp-agent.ts:4985-4992`](https://github.com/agentclientprotocol/claude-agent-acp/blob/main/src/acp-agent.ts#L4985-L4992)。但本次搜索没有找到 native Read/Write 与这些包装的强制耦合，故 fs 只能写“未确定”。
- SDK options 能设置 `tools`、`disallowedTools`、`canUseTool`：[同文件 `:5932-5963`](https://github.com/agentclientprotocol/claude-agent-acp/blob/main/src/acp-agent.ts#L5932-L5963)。
- 本次在适配器源中未找到 `terminal.create` 或等价 client terminal 执行；找到的是运行时自己产生 terminal 元数据/输出。显示结果不满足“进程交回客户端”。

### Codex ACP

- 官方适配器把 Codex app-server 暴露成 ACP，并能转发 shell/file/permission/terminal-output 事件：[官方 README](https://github.com/agentclientprotocol/codex-acp#codex-acp)。
- 初始化保存 client capabilities，但返回的 agent capabilities 未宣告 reverse fs/terminal；同一源中未找到 client `readTextFile`、`writeTextFile` 或 `terminal.create` 调用：[Codex ACP `CodexAcpServer.ts:246-302`](https://github.com/agentclientprotocol/codex-acp/blob/main/src/CodexAcpServer.ts#L246-L302)。
- `ReadOnly`、`Agent`、`FullAccess` 映射的是 approval/sandbox 组合，不是闭合的逐工具 allowlist：[Codex ACP `AgentMode.ts:1-60`](https://github.com/agentclientprotocol/codex-acp/blob/main/src/AgentMode.ts#L1-L60)。

### OpenCode ACP

- OpenCode 官方文档提供 `opencode acp`：[OpenCode ACP 文档](https://opencode.ai/docs/acp/)。
- ACP service 的 client connection 只有可选 permission 与 write 回调，没有成对 read 和 terminal；初始化也没有宣告完整 client fs/terminal：[OpenCode `acp/service.ts:50-129`](https://github.com/anomalyco/opencode/blob/dev/packages/opencode/src/acp/service.ts#L50-L129)。
- OpenCode 自身能用 `tools` 和 `permission` 对工具设 allow/ask/deny、通配规则；这只满足“限制 native tools”一半：[OpenCode Agents/Tools](https://opencode.ai/docs/agents/#tools)、[Permissions](https://opencode.ai/docs/permissions/)。

### Gemini CLI ACP

- 官方 ACP mode 文档明确写有 stdio server 与 client-proxied filesystem：[Gemini CLI `acp-mode.md:203-249`](https://github.com/google-gemini/gemini-cli/blob/main/docs/cli/acp-mode.md#L203-L249)。
- 官方 shell tool 文档说明 `run_shell_command` 由 Gemini CLI 使用本地 `node-pty`/child process 执行，同时可通过 core-tools allowlist 控制可用工具：[Gemini CLI `shell.md:228-335`](https://github.com/google-gemini/gemini-cli/blob/main/docs/tools/shell.md#L228-L335)。因此 fs 交回了客户端，terminal 没有。

## Q3. 本机 CLI 安装与登录状态

### 结论

**本次 worker 上唯一可直接执行并取得版本的 CLI 是 Grok。** Codex 和 Gemini 的用户级 npm shim 存在，但在本 worker 中启动被操作系统拒绝；Claude、OpenCode 和其余采样 CLI 在当前 PATH/已知 shim 位置未发现。`Get-Command=false` 只证明“当前进程不可发现”，不证明机器其他目录绝对未安装。

**Grok 的机器级登录结论仍是“未确定”，而不是“未登录”。** 本 worker 的 `grok models` 给出明确未认证信号；总控在同机另一个执行上下文得到明确 grok.com 已登录信号。两条结果都成立，说明二者解析到的 profile/config root 或进程环境不同。在未核对解析路径前，不能拿其中一条覆盖另一条。账号邮箱/组织名没有取得。

### 检测方法

1. **定位**：`Get-Command <name> -ErrorAction SilentlyContinue`；再对已知用户 npm shim 做 `Test-Path`。只输出布尔值、命令类型和叶文件名，不输出用户目录。
2. **安装/版本**：执行官方 `--version`；只把退出 0 和版本文本当成“可运行”，不把它当成登录检查。
3. **认证状态**：只用官方 status/list/models 命令，并按其文档定义解释退出码/明确 banner。认证文件存在、字段存在或工具名出现在列表中都不是活会话证据。
4. **账号标识**：只取官方 status/UI 明确返回的邮箱、组织或 workspace label；不读取凭据文件。本次没有取得任何账号标识。

### 本次实际输出（已脱敏为结构/布尔值）

发现命令：

```text
COMMAND_DISCOVERY
grok|PATH_FOUND=true|KIND=Application|UNDER_USERPROFILE=True|LEAF=grok.exe
claude|PATH_FOUND=false
codex|PATH_FOUND=false
opencode|PATH_FOUND=false
gemini|PATH_FOUND=false
qwen|PATH_FOUND=false
aider|PATH_FOUND=false
goose|PATH_FOUND=false
amp|PATH_FOUND=false
cursor-agent|PATH_FOUND=false
cody|PATH_FOUND=false
continue|PATH_FOUND=false
cline|PATH_FOUND=false

KNOWN_SHIMS
codex|APPDATA_NPM_SHIM=True
gemini|APPDATA_NPM_SHIM=True
claude|APPDATA_NPM_SHIM=False
opencode|APPDATA_NPM_SHIM=False
```

Grok 版本：

```text
COMMAND: grok --version
grok 1.0.0 (3cd0d0cbce) [stable]
EXIT=0
```

Grok 状态与帮助的分类输出：

```text
COMMAND: grok models
GROK_MODELS|EXIT=0|EXPLICIT_UNAUTH=True|EXPLICIT_GROK_COM_LOGIN=False|DEFAULT_GROK_4_5=True

COMMAND: grok --help
EXIT=0|LINES=176|HAS_LOGIN=True|HAS_LOGOUT=True|HAS_MODELS=True

COMMAND: grok login --help
EXIT=0|LINES=11|HAS_OAUTH=True|HAS_AUTH_X_AI=True|HAS_DEVICE=True
```

总控在同机另一执行上下文的已给定实测：

```text
COMMAND: grok models
You are logged in with grok.com
DEFAULT_MODEL=grok-4.5
```

Codex/Gemini shim 启动：

```text
CODEX_--VERSION|RESULT=APPLICATIONFAILEDEXCEPTION
CODEX_LOGIN_STATUS|RESULT=APPLICATIONFAILEDEXCEPTION
CODEX_VERSION|INNER=Win32Exception|NATIVE_CODE=5|FQID=NativeCommandFailed
GEMINI_--VERSION|RESULT=APPLICATIONFAILEDEXCEPTION
GEMINI_--HELP|RESULT=APPLICATIONFAILEDEXCEPTION
```

Windows native code 5 是 access denied；所以这些结果只能写“worker 无法执行”，不能写“未安装”或“未登录”。

### CLI 状态矩阵

| CLI | 当前 worker 可发现/可执行 | 本次登录结论 | 账号标识 |
| --- | --- | --- | --- |
| Grok | PATH 可发现；版本退出 0 | **未确定（上下文冲突）**：worker 明确未认证；总控上下文明确定义为 grok.com 已登录 | 未确定 |
| Codex | PATH 未发现；已知 npm shim 存在；启动 access denied | 未确定 | 未确定 |
| Gemini | PATH 未发现；已知 npm shim 存在；启动 access denied | 未确定 | 未确定 |
| Claude | PATH 与已知 npm shim 均未发现 | 未确定（不是“未登录”） | 未确定 |
| OpenCode | PATH 与已知 npm shim 均未发现 | 未确定（不是“未登录”） | 未确定 |
| Qwen、Aider、Goose、Amp、Cursor Agent、Cody、Continue、Cline | 当前 PATH 未发现 | 未确定 | 未确定 |

### 各 CLI 的确定性信号

| CLI | 安装信号 | 官方认证信号 | 确定性边界 |
| --- | --- | --- | --- |
| Grok | `grok --version` 退出 0 | `grok models` 的明确登录/未登录 banner；`login --help` 证明 xAI OAuth/device flow 存在 | `--version` 不查登录；必须同时记录解析到的 executable/config root 元数据 |
| Claude | `claude --version` | `claude auth status --text`；官方定义退出 0=logged in，1=not logged in | 状态可解析 JSON/文本中的非敏感 account label；不要读认证文件。[CLI reference](https://code.claude.com/docs/en/cli-usage#authentication-commands) |
| Codex | `codex --version` | `codex login status` 返回当前认证方法 | status 可能只给方法、不一定给邮箱；账号/workspace 需官方 profile surface。`--version` 不查登录。[Codex authentication](https://developers.openai.com/codex/auth/) |
| OpenCode | `opencode --version` | `opencode auth list`/`ls` | 官方定义是列出本地 credentials store 中的 provider；只能证明本地记录，不能证明远端仍有效或账号身份。[CLI auth](https://opencode.ai/docs/cli/#auth) |
| Gemini | `gemini --version` | 交互 `/auth` 可查看/切换方法；未找到官方 headless status 命令 | 只能在可认领交互 shell 中查看，再经获批的无副作用请求验证；本次未运行。[Commands](https://github.com/google-gemini/gemini-cli/blob/main/docs/reference/commands.md) |

## Q4. OpenCode 认证层能不能扒来复用

### 结论

**从代码许可证看，可以有条件复用；从服务授权和账号安全看，不能把“MIT”当成任何 provider OAuth/订阅的授权。** OpenCode 仓库是 MIT（SPDX：`MIT`），Pillow 是 Apache-2.0（`LICENSE:1-3`），二者代码许可证兼容。若复制或实质改编认证代码，必须保留上游 copyright 与 MIT permission notice；Pillow 现有惯例还要求固定完整 commit、逐文件映射、说明改动，并在 `THIRD_PARTY_NOTICES.md` 收录全文。依赖、生成代码和外部 npm 插件必须分别审计，不能自动继承仓库根 MIT 结论。

技术上也不建议“整层扒来”：OpenCode 的 auth union 和 provider plugin 分层值得复用**模型形状**；它的全局 JSON 凭据存储、任意插件加载、消费订阅 OAuth client 注册则不符合 Pillow 的项目隔离、DPAPI 和 fail-closed 边界。

### Provider 与认证类型

先区分两个目录：

- **模型/provider 目录**：官方文档称由 AI SDK + Models.dev 支持 75+ provider；大多数 provider 是 API key、云凭据链或本地无凭据模型。这是“怎么调用模型”的元数据，不等于 OpenCode 为每家实现了 OAuth。
- **内置认证 plugin**：当前 `plugin/index.ts` 直接装入 OpenAI Codex、GitHub Copilot、Modal、GitLab、Poe、Cloudflare、Azure、DigitalOcean、Snowflake Cortex、xAI 等插件：[OpenCode `plugin/index.ts:11-22,60-78`](https://github.com/anomalyco/opencode/blob/dev/packages/opencode/src/plugin/index.ts#L11-L22)。

已核实的认证类型：

| Provider / 家族 | OpenCode 已核实方式 | 判断 |
| --- | --- | --- |
| 通用 Models.dev provider、OpenCode Zen/Go、多数 SaaS | API key；部分云 provider 走各自 SDK credential chain | API key / cloud chain |
| OpenAI | 手工 API key；内置 Codex plugin 另有 ChatGPT browser/headless OAuth | API key + OAuth |
| GitHub Copilot | device OAuth | OAuth |
| GitLab Duo | OAuth 或 PAT | OAuth + token |
| DigitalOcean | OAuth 或 model-access key | OAuth + key |
| Snowflake Cortex | browser OAuth 或 PAT/JWT | OAuth + token/JWT |
| xAI | SuperGrok device OAuth 或 API key | OAuth + API key |
| Anthropic | 当前官方主路径是 API key；官方文档说明 Claude Pro/Max 插件路径已被移除/禁止，不应作为可复用订阅 OAuth | API key；订阅 OAuth **不应复用** |
| Modal、Poe、Cloudflare、Azure | 内置 plugin 存在；本次未完成每个 plugin 的 flow/credential type 逐行核实 | **未确定** |

证据：

- provider 总目录、75+ 范围、`/connect` 与凭据存储说明：[OpenCode Providers](https://opencode.ai/docs/providers/)。
- 内置 auth union 只有 `oauth`、`api`、`wellknown` 三类；OAuth 结构含 refresh/access/expiry 及可选 account/enterprise 元数据，API 与 well-known 结构含各自 key/token 字段。这里仅列字段名，不列值：[OpenCode `auth/index.ts:9-31`](https://github.com/anomalyco/opencode/blob/dev/packages/opencode/src/auth/index.ts#L9-L31)。
- 同文件把 provider-ID → auth object 写到全局 data 目录 JSON，权限模式为 `0600`；它不是 OS keychain，也不是项目作用域：[`auth/index.ts:52-79`](https://github.com/anomalyco/opencode/blob/dev/packages/opencode/src/auth/index.ts#L52-L79)。
- OpenAI Codex OAuth 与 API-key 选择逻辑：[OpenCode `plugin/openai/codex.ts:297-506`](https://github.com/anomalyco/opencode/blob/dev/packages/opencode/src/plugin/openai/codex.ts#L297-L506)。
- 各 provider 的官方流程说明均在同一官方目录，包含 Anthropic、DigitalOcean、GitLab Duo、GitHub Copilot、Snowflake Cortex、OpenAI、xAI：[OpenCode Providers](https://opencode.ai/docs/providers/)。

### Provider 目录来自哪里，运行时还是内置

**结论：数据谱系来自 Models.dev，装载方式是“内置快照 + 磁盘缓存 + 运行时刷新”的混合模式。** 当前源码默认从 OpenCode 自己的 models 镜像 URL 取 `api.json`，不是每次都直连 `models.dev` 域名；模块、schema 与官方文档仍明确把它当 Models.dev 目录。

- 官方文档明确“AI SDK + Models.dev”并列为 provider 数据来源：[OpenCode Providers:170-185](https://opencode.ai/docs/providers/#overview)。
- Models.dev 自身说明 provider/model 数据由仓库 TOML 生成并通过 API 发布：[models.dev repository](https://github.com/anomalyco/models.dev)。
- 当前加载器的默认 source、5 分钟磁盘 freshness、`api.json` 拉取、磁盘优先、编译时 snapshot fallback、禁网开关、强制 refresh 与每 60 分钟后台刷新位于：[OpenCode `packages/core/src/models-dev.ts:140-225`](https://github.com/anomalyco/opencode/blob/dev/packages/core/src/models-dev.ts#L140-L225)。

这意味着目录可以用于 UI 建议与能力元数据，但不能成为认证授权源；目录新增 provider 也不得自动扩大 Pillow 的可用 auth variant 或工具权限。

### 许可证与合法复用条件

- 上游 LICENSE URL：[OpenCode LICENSE](https://github.com/anomalyco/opencode/blob/dev/LICENSE)。文件明确为 MIT；SPDX 标识为 `MIT`。
- Pillow 当前许可证：Apache License 2.0，`LICENSE:1-3`。
- MIT 的核心条件是：软件副本或实质部分必须携带 copyright notice 与 permission notice。Apache-2.0 项目可以包含 MIT 代码，但必须把两套声明保留下来。
- **能合法复用的条件**：固定完整上游 commit；只映射确实复制/改编的文件；保留文件内既有版权头；在分发物中携带上游 MIT copyright + permission；在 `THIRD_PARTY_NOTICES.md` 记录路径映射和 Pillow 改动；对每个外部 plugin/依赖单独核证许可证。
- **许可证不能解决的条件**：OAuth client ID/redirect URI 的注册权、provider 品牌/服务条款、消费者订阅凭据用途、账号共享或自动化限制。它们需要官方授权/条款核验。

本结论是工程许可证核对，不是法律意见。

### 建议的 `THIRD_PARTY_NOTICES.md` 条目

以下只是一条建议形状；在没有实际复制文件和固定 commit 前，不应落入正式 notices：

```markdown
## OpenCode authentication adaptations

Repository: `anomalyco/opencode`（链接到官方仓库）

Fixed commit: `<集成时固定的完整提交>`

License source: `LICENSE` at the fixed commit above (SPDX: MIT).

### Authentication adaptations

| Pillow path | Upstream source (exact) | What was reused |
| --- | --- | --- |
| `<实际 Pillow 路径>` | `<固定提交下的准确上游路径>` | `<仅陈述实际复制/改编的认证形状>` |

Changes: `<列明 Pillow 的项目作用域、secret_ref、DPAPI、治理与 fail-closed 改动>`

### MIT License

<逐字保留固定提交 LICENSE 中的 copyright 与完整 MIT permission notice>
```

这与仓库现有“仓库 → fixed commit → license source → 精确路径映射 → 改动范围 → 许可证全文”的格式一致：`THIRD_PARTY_NOTICES.md:3-61`。

## Q5. 主流产品的模型形状

### 结论

成熟产品的共同做法不是把所有 provider 压成一个“登录字符串”，而是把以下对象分离：

1. provider/model catalog（非敏感）；
2. credential kind 与 credential owner；
3. secret store/reference；
4. account/session status；
5. agent runtime/adapter 及其治理 capabilities。

Zed 对“API access”和“已有订阅/外部 agent”明确分路；OpenCode 用 provider catalog + auth union + provider plugin；Goose 用 provider config + secret-store abstraction + 可隔离的 profile root。Pillow 应采用这一模型形状，同时比 OpenCode/Goose 更严格：durable object 项目作用域、OS secret store、无静默 plaintext fallback、治理由实测能力合取导出。

### Zed

**模型形状：host-owned API access 与 agent/CLI-owned subscription 是两个访问路径。**

- API key 属于 Zed-owned AI features，保存在 system keychain；非空环境变量优先于 keychain。External Agents/Terminal Threads 通常由 agent/CLI 自己配置模型访问：[Zed API Access:11-42](https://zed.dev/docs/ai/use-api-access)。
- Zed 明确把 Claude Pro/Max、ChatGPT Plus/Pro 与 API billing 区分，订阅通过相应 external agent/harness 使用，而不是把消费者订阅转成通用 API key：[Zed Existing Subscription](https://zed.dev/docs/ai/use-an-existing-subscription)。
- 可借鉴字段：`access_path`（host API / host subscription / external-agent-owned / gateway）、`provider_id`、`model_id`、`secret_ref`、`credential_owner`；环境变量覆盖必须在 UI 中显式显示来源。

### OpenCode

**模型形状：大目录 + 小认证 union + provider-specific plugin。**

- Models.dev 负责 provider/model 元数据；`Auth.Info` 负责 `oauth | api | wellknown`；plugin 负责某一家 OAuth/device/enterprise 的具体流程。
- 优点是 provider 扩展不需要给 UI 加一棵分支树。缺点是当前 auth 文件是全局 JSON、不是项目域，也不是 OS keychain；Pillow 只能借鉴分层，不能照搬存储边界。
- 证据见 Q4 的 `auth/index.ts`、`plugin/index.ts`、`models-dev.ts`。

### Goose

**模型形状：provider 配置与 credential store 分离，并允许完整 profile root 隔离。**

- Goose 文档说明系统 keyring 是默认秘密存储，也允许显式禁用 keyring 并回退到本地 secrets 文件；后者必须被 Pillow 视为高风险而非无声降级：[Goose environment variables](https://github.com/aaif-goose/goose/blob/main/documentation/docs/guides/environment-variables.md#credential-storage)。
- `GOOSE_PATH_ROOT` 一次性隔离 config/data/state，适合多 profile 和 CI；文档给出各平台根目录与隔离语义：[同文件 `GOOSE_PATH_ROOT`](https://github.com/aaif-goose/goose/blob/main/documentation/docs/guides/environment-variables.md#development--testing)。
- 可借鉴字段：`credential_store_kind`、`credential_ref`、`config_root_ref`、`profile_id`、`source_precedence`；Pillow 不应提供静默 plaintext fallback。

### 对 Pillow 的直接约束

| 维度 | 成熟做法 | Pillow 应保留的边界 |
| --- | --- | --- |
| 目录 | catalog 与 credential 分开 | catalog 只影响可选项，不授予权限 |
| 认证 | typed union + provider-specific flow | 不把订阅、API key、CLI-owned session 混成字符串 |
| 秘密 | keychain/secret-store reference | SQLite 只存 `secret_ref` 和非敏感元数据；Windows 继续 DPAPI/fail-closed |
| 多账号 | profile/account 独立 | 每槽位独立 config root，记录 credential owner 与 account label |
| 外部 agent | agent 自己拥有登录 | 明确标为 external/ungoverned，除非治理能力合取经 fixture/live probe 证明 |

## 如果要做到所有槽位平等，认证枚举应该长什么样

### 结论

“平等”不应表示每种 auth 都假装拥有同样能力，而应表示：所有槽位使用同一顶层 schema、生命周期、账号状态和 capability 评估；认证方式是可扩展 variant；治理标签由实际 adapter capability 合取导出。凭据对象与槽位绑定应分开，避免同一凭据被隐式复制到多个项目或运行时。

### 建议 variant 与必要字段（事实/形状，不是实现代码）

| Variant | 必要非敏感字段 | 必要秘密引用/生命周期字段 |
| --- | --- | --- |
| `ApiKey` | `provider_id`、`endpoint/base_url`、`model_or_catalog_ref`、`protocol/header_scheme`、可选 `organization_label/project_label` | `secret_ref`、`last_verified_at` |
| `OAuthAuthorizationCodePkce` | `provider_id`、`issuer`、`client_id`、`scopes`、`audience/resource`、`redirect_strategy`、`account_id/display_label`、`support_policy` | `credential_ref`、`expires_at`、`refresh_state`；PKCE verifier 只作瞬时状态 |
| `OAuthDeviceCode` | `provider_id`、`issuer`、`client_id`、`scopes`、`audience/resource`、`account_id/display_label` | `credential_ref`、`expires_at`、`refresh_state`；device/user code 不持久化 |
| `ExternalCredentialProvider` | `provider_id`、**受审计 adapter ID**、`issuer`、`audience/resource`、`refresh_contract`、`execution_policy` | `credential_ref`、`expires_at`；不得存任意 shell 文本作为“认证器” |
| `CliOwnedSession` | `cli_kind`、`executable_ref`、`isolated_config_root_ref`、`adapter_protocol/version`、`status_probe_id`、`account_label`、`credential_owner=cli` | Pillow 不复制 CLI 凭据；只保存状态时间与非敏感引用 |
| `CloudCredentialChain` | `provider_id`、`chain_kind`（AWS/Azure/GCP 等）、`profile/tenant/subscription_ref`、`region` | 仅保存各 secret-store reference 与到期状态 |
| `AnonymousOrLocal` | `endpoint`、`model_or_catalog_ref`、`protocol` | 无秘密；仍需 endpoint trust policy |
| `UnsupportedExternalRegistration` | `executable_ref`、`status_probe_id`、`account_label` | 无 Pillow-owned 凭据；固定 `governance=None`，不可启动受治理对话 |

所有 variant 的公共字段至少应有：

- `auth_id`、`project_id`、`provider_id`、`credential_owner`；
- `status = unknown | authenticated | expired | revoked | error`、`last_verified_at`、非敏感 `account_id/display_label`；
- `source`、`secret_ref/credential_ref`、`endpoint_binding`、`issuer/audience/scopes`（适用时）；
- `terms/support_policy`，明确 official、enterprise-configured、unsupported；
- `governance_capabilities = tool_allowlist ∧ client_fs_read ∧ client_fs_write ∧ client_terminal` 的逐项证据与 adapter version。

最后一项必须是**派生事实**，不能由用户勾一个 `governed=true` 存入数据库。当前 Pillow 已遵守“由 auth 导出 governance”的方向：`crates/storage-sqlite/src/runtime_slot.rs:64-97`；但现有 auth variant 只有 `ApiKey`、`GrokLogin`、`LocalCli`，尚未表达其他 OAuth/CLI-owned/cloud chain：同文件 `:44-62`。Renderer 虽已改成 data-driven 统一 create input，实际选项仍只有三类，且 local CLI 明确不可作 controller：`apps/desktop/src/features/settings/runtimeSlotsTypes.ts:113-148,175-189`。

## 风险

1. **非官方消费订阅 OAuth**：抓取、复用或代理 Claude/ChatGPT/Gemini 等消费订阅会话，可能违反服务条款并触发账号限制。Gemini 官方 FAQ 明确警告第三方 OAuth harvesting/piggyback 可能导致 suspension；支持的第三方路径是 API key/Vertex：[Gemini CLI FAQ](https://github.com/google-gemini/gemini-cli/blob/main/docs/resources/faq.md)。OpenCode 文档对 Anthropic Pro/Max 插件路径也给出禁止/移除说明：[OpenCode Anthropic provider](https://opencode.ai/docs/providers/#anthropic)。
2. **把订阅凭据发给第三方端点**：bearer 通常绑定 issuer、audience、client 和 endpoint；交给代理/兼容端点可能直接泄露账号控制权。不得把“兼容 OpenAI API”解释成“兼容 OpenAI 登录凭据”。
3. **任意 external auth command**：grok-build 的外部提供器会从子进程输出取得凭据。若 Pillow 接受任意命令文本，会引入命令注入、凭据回显、错误日志泄露和无边界 refresh；只能允许固定、签名/审计过的 adapter，并校验 issuer/audience。
4. **并行凭据存储和 profile 冲突**：同一个 CLI 被多个槽位共享 home/config root，可能出现 refresh token 竞争、一个槽位登出另一个槽位、账号误归属。每槽位必须隔离 root，并在启动前核对实际解析来源。
5. **“文件存在”冒充“已登录”**：本地 auth 结构、provider 名或 tool 列表只证明记录存在，不能证明远端 entitlement 有效。状态应为 `unknown`，直到官方 status 或获批的无副作用 canary 成功。
6. **环境变量静默覆盖**：Zed 明确非空 env 优先于 keychain。Pillow 若支持类似优先级，UI 必须显示“当前生效来源”，否则会用错账号/组织。
7. **明文 fallback**：OpenCode 的全局 JSON（即使权限为 `0600`）和 Goose 可选 secrets 文件会进入备份、同步、进程读取面。Pillow 应继续 OS-backed secret store；不可静默降级为明文。
8. **状态输出泄露身份**：status 可能包含邮箱、组织、workspace。host DTO 必须 closed-allowlist 且最小化字段；日志只保留必要 label，不应转储完整 JSON。
9. **半开治理假象**：权限弹窗、tool event 或 terminal output 让 UI 看起来“在治理”，但 agent 仍可直接写磁盘/起进程。必须用协议 fixture 加真实副作用 trap 证明交回，不得从代码中出现方法名推断成功。
10. **目录刷新扩大权力**：Models.dev/远程 catalog 更新只能改变展示和能力建议，不能自动启用新认证、模型工具、endpoint 或 credential flow。
11. **OAuth 回调与设备码钓鱼**：必须使用官方注册 client、state/PKCE、受限 redirect、清晰 provider 域名；不能让插件提供任意认证 URL 后自动信任返回结果。
12. **跨项目共享 durable credential**：AGENTS.md 要求每个 durable object project-scoped。共享 account 可由多个 project binding 引用同一受控 secret object，但不能复制秘密或让项目隐式继承授权。
13. **代码许可不等于服务许可**：MIT 允许复制代码，不授予 OAuth client registration、商标、provider API 或消费者订阅用途。

## 残余未知与取证下一步

| 未知 | 为什么仍未知 | 取得确定证据的下一步 |
| --- | --- | --- |
| Claude native Read/Write 是否始终路由到 ACP client fs | 适配器有回调包装，但未找到与原生工具的强耦合；方法存在不能证明调用成功 | 固定 commit，写协议 fixture；在隔离临时目录用 deny-capability trap 观察是否出现 client RPC。需要另单获准运行，不在本次执行 |
| Claude ACP 是否可把 terminal 创建/执行交给 client | 当前只找到运行时 terminal metadata/output | 向上游确认 reverse terminal 支持，或对固定版本抓 JSON-RPC trace；无 `terminal.create` 之前保持否定标签 |
| Codex/OpenCode 是否有尚未文档化的 reverse fs/terminal extension | 当前公开初始化和源搜索没有证据 | 固定 release/commit，全源搜索 capability negotiation，并做最小协议 probe |
| 本机 Claude/Codex/OpenCode/Gemini 的确切版本、登录账号 | 当前 worker 只有 Grok 可执行；Codex/Gemini shim 被 access denied | 总控在可认领 shell 中运行本文“确定性信号”命令，只回传退出码、版本、auth method、邮箱/组织 label；不回传凭据 |
| 同机 Grok 为何一处已登录、一处未认证 | 两个执行上下文给出相反明确 banner，说明解析环境不同 | 分别记录 resolved executable、版本、进程身份、config root **路径元数据/文件存在布尔值**；不读文件内容，再比较 profile isolation |
| OpenCode 的 Modal、Poe、Cloudflare、Azure 精确认证 flow | 本次确认 plugin 存在，但未逐插件完成 flow 追踪 | 固定 OpenCode commit，逐个从 plugin `auth.methods` 到 credential store 做 source trace，并核对 provider 官方条款 |
| 上游 mutable branch 后续是否改变能力 | 本文外部证据多指向 `main`/`dev` | 实施前将所有来源固定到完整 SHA，重跑静态审计和协议 fixture |
| 各 provider 对嵌入式/第三方 OAuth 的合同许可 | 开源许可证无法回答服务条款 | 仅采用 provider 官方公开支持的 client/SDK；企业 OIDC 走用户自有注册；边缘情形交法务/provider 支持书面确认 |

本次按任务约束没有运行任何 gate、test、desktop smoke、登录流或 ACP live probe；因此以上需要运行时实验的项目保留为“未确定”，没有从静态代码推断成功。

## 证据索引

- Pillow 现行 auth/governance：`crates/storage-sqlite/src/runtime_slot.rs:44-97`
- Pillow 私有 Grok endpoint config：`crates/runtime-slots/src/grok_config.rs:13-56`
- Pillow Grok ACP allowlist/capabilities：`crates/adapter-grok-acp/src/rpc/methods.rs:5-87,127-194`
- external CLI 不可启动对话：`crates/runtime-slots/tests/slot_service.rs:215-252`
- Renderer auth catalog/unified input：`apps/desktop/src/features/settings/runtimeSlotsTypes.ts:113-148,175-189`
- 半开陷阱定义：`docs/handoff/2026-08-11-pillow-controller-handoff.md:75-85`
- Pillow 既有第三方声明格式：`THIRD_PARTY_NOTICES.md:3-61`
- Pillow 许可证：`LICENSE:1-3`

AUTH-BREADTH-COMPLETE
