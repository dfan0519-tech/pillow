# Pillow — Grok Build 权限/工具治理基座复用调研

**Date:** 2026-08-10  
**Nature:** 只读调研。唯一写入：本报告。未改 product 源码/测试/脚本/配置。未跑门/未 cargo/pnpm build。  
**Upstream clone:** 临时目录（用后删除）`https://github.com/xai-org/grok-build`  
**Upstream HEAD inspected:** `75e73f3d6ac0350d211f12ae7d57c2c0aad72576`（2026-08-09 monorepo sync）  
**Upstream `SOURCE_REV`:** `a61c32b12a2b400f212221cd8762e05f9b36828d`  
**Pillow conceptual pin:** `docs/references/upstream-baseline.md` → `5da6962e…`（本单**未**升 pin）  
**Local CLI observed (help only):** `grok 1.0.0 (3cd0d0cbce) [stable]`  
**Prior live probe (not re-run):** `docs/research/2026-08-10-mcp-tool-exposure-probe.md`  
**Prior static audit:** `docs/execution/2026-08-08-upstream-grokbuild-audit.md`

**Completion signal:** `GROKBUILD-PERMISSION-REUSE-COMPLETE`

---

## 1. 一句话结论

**没有正确复用。** 上游在 `agent stdio` 上**确实有**可消费的工具/权限治理基座（`_meta.agentProfile` 解析 → `AgentDefinition` 工具过滤；`src/permission/**` 规则与模式；CLI/config 的 yolo 链），但 Pillow 当前：

1. 用**残缺**的 `_meta.agentProfile`（缺必填 `name`/`description`）→ 解析失败 → **静默回落到默认 grok-build 全工具集**；
2. 即便解析成功，`tools: []` 在上游代码里是 **Empty = inherit all**，不是“关掉所有工具”；
3. denylist 里若干名字与真实 short id 不对（`write_file` vs `write`，`run_terminal_command` vs `run_terminal_cmd` 等）；
4. 生产 argv 只传 `agent --no-leader stdio`，**不传** `--agent-profile`、**不传** 可覆盖用户 yolo 的 `--permission-mode`；
5. 自建了一层 `decide_permission` 全拒 + `kernel-policy` 规则表 + `pillow/*` JSON-RPC——在原生工具仍可用时，这些是**旁路治理**，挡不住 Grok 原生 `write`/`search_replace`/`run_terminal_cmd`。

**缺口（按致命度）：** (A) agentProfile 解析失败使“禁用原生工具”从未生效；(B) 语义/命名错误使修复 (A) 后仍可能留写盘口；(C) 用户 `<home>/.grok/config.toml` 的 `permission_mode = always-approve` 会跳过 `session/request_permission`（与本机探针零 permission 一致），但**不能**单独解释“工具仍存在”；(D) `agent stdio` 路径**未接线**顶层 `--tools` / `--disallowed-tools` / `--allow` / `--deny` 到 `cli_agent_overrides`（headless `-p` 才接）。

---

## 2. Q1 — `_meta.agentProfile` 在 `agent stdio` 上谁消费？`tools` / `disallowedTools` 绑不绑工具？

### 2.1 消费链（查证）

| 步骤 | 位置 | 行为 |
| --- | --- | --- |
| 解析 | `xai-grok-shell/src/upload/turn.rs` L284–318 `parse_agent_profile_from_meta` | 读 `session/new`（及同类）`_meta.agentProfile`：对象 → `AgentDefinition::from_json`；字符串 → `discovery::by_name`；失败 → **log error + `None`（回落默认）** |
| 选型优先级 | `…/mvp_agent/agent_ops.rs` L3738–3779 `resolve_agent_definition` | 文档与代码：ACP profile **高于** `--agent-profile` / config / env（严格 harness 的 model `agent_type` 另议） |
| 应用到会话 | `agent_ops.rs` ~L4218–4242 | `parse_agent_profile_from_meta` → `resolve_agent_definition` → 再 `cli_agent_overrides.apply_to_definition` |
| 工具过滤 | `xai-grok-agent/src/builder.rs` L882–901+ | `disallowed_tools`：从 toolset **retain 掉**匹配项；`tools`：**仅当非空**才做 allowlist |

文档层优先级（与代码一致）：

```text
// xai-grok-shell/src/agent/config.rs L1709–1715
// 1. ACP session-level `_meta.agentProfile`
// 2. CLI `--agent-profile` flag
// 3. [agent] config.toml
// 4. GROK_AGENT env
// 5. Default grok-build
```

### 2.2 `tools` / `disallowedTools` 是否绑定工具可用性？

**绑定——但前提是 profile 成功变成 `AgentDefinition` 并进入 builder。**

- **`disallowedTools`（查证）：** builder L882–889 `retain(|tc| !tool_id_matches(&definition.disallowed_tools, &tc.id))`。匹配用 short name 或 full id（`config.rs` L1284–1292 `short_tool_name` / `tool_id_eq`）。
- **`tools` allowlist（查证）：** builder 仅在 `!definition.tools.is_empty()` 时过滤（L901）。`AgentDefinition` 字段注释明确：

```text
// xai-grok-agent/src/config.rs L782
// Tool allowlist. Empty = inherit all.
```

- **README 误导（查证冲突）：** `xai-grok-agent/README.md` 写 `[] = none`；**代码与 builder 测试语义是 Empty = inherit all**。以代码为准。
- **allowlist 含无法映射的名字时（查证）：** builder L958 `keeping full grok toolset`——整表 allowlist 作废，保留全套（denylist 仍先执行）。

### 2.3 Pillow 生产 payload 为何实盘“完全无效”

Pillow：`crates/adapter-grok-acp/src/rpc/methods.rs` L5–28 `plaintext_agent_profile()`：

```json
{ "tools": [], "disallowedTools": [ "read_file", ..., "write_file", ... ] }
```

**没有 `name` / `description`。**

上游 `from_json`（`config.rs` L1669–1681）对 `AgentDefinition` 做 serde：`name`/`description` **无 default**。单测 L2411–2417 `test_from_json_missing_required_fields`：缺 `name` → `Err`。

解析失败路径（`turn.rs` L300–305）：

```text
"Failed to parse _meta.agentProfile JSON object, falling back to default agent"
→ None → resolve 落到默认 grok-build 全工具
```

**因此：** 通道本身在 stdio 上**存在且优先**；Pillow 发的对象**解析失败被丢弃**。这与探针 Q4/Q5/Q6（read/write 仍成功；正向 allowlist 仍可写）一致。

即便将来补上 `name`/`description`：

| 字段 | 代码语义 | Pillow 现状 |
| --- | --- | --- |
| `tools: []` | inherit all | 不禁用任何工具 |
| `disallowedTools` 含 `write_file` | 匹配 short id `write_file` | 真实写盘工具 short id 是 **`write`**（OpenCode `WriteTool`，`…/opencode/write/mod.rs` L71–72）；默认还会注入该工具（builder L763–766，`write_file_enabled` 默认 true） |
| `bash` / `run_terminal_command` | 需匹配 short id | bash 工具 short id 是 **`run_terminal_cmd`**（`…/bash/mod.rs` tool id），不是 `bash` / `run_terminal_command` |
| `grep_search` | 需匹配 | 默认 grep short id 为 **`grep`**（`ReadFileTool`/`GrepTool` 同类命名） |

错误 denylist 项 → builder L898 仅 warn `"disallowedTools entry matched nothing"`，**不 fail closed**。

### 2.4 上一次审计 “Still honored” 错在哪一步？

`docs/execution/2026-08-08-upstream-grokbuild-audit.md` L119：

> `_meta.agentProfile` → **Still honored**（读源码得出）

| 对的部分 | 错的部分 |
| --- | --- |
| 上游**仍有**解析与优先级，未从代码删除 | 把“源码路径存在”写成“Pillow 依赖的治理边界成立” |
| wire 兼容性审计范围合理 | **未**对照 Pillow payload 是否满足 `from_json` 必填；**未**核对 `tools:[]` 语义；**未**做实盘 |
| | 与 L336 “Pillow enforces its own permission policy” 一起，鼓励了“上游 profile 可用、我们另造一层”的分叉，而**第一层其实从未生效** |

**裁决：** 审计在“协议/路径仍存在”意义上部分正确；在“Pillow 生产配置下工具边界仍有效”意义上**错误**。实盘 + `from_json` 必填/`tools:[]` 语义推翻后者。

---

## 3. Q2 — `--agent-profile <PATH>` 格式；是否 `agent stdio` 正确通道？与 `bundled/agents` 关系

### 3.1 CLI 与接线（查证）

- `grok agent --help`：**有** `--agent-profile <PATH>`、`--always-approve`；**无** `--allow` / `--deny` / `--permission-mode` / `--tools`（这些在**顶层** `grok --help`）。
- `AgentArgs`：`xai-grok-pager/src/app/cli.rs` L257–281 `agent_profile: Option<PathBuf>`。
- `run_agent_command`：`pager-bin/src/main.rs` L1151–1154 写入 `agent_config.agent_profile_path`（canonicalize + 必须是文件，L74–86）。
- 解析：`AgentDefinition::from_file`（`config.rs` L1293+）= **Markdown + YAML frontmatter**（`---` … `---` + body）。

### 3.2 文件格式 / schema（查证）

`xai-grok-agent/README.md` Frontmatter Schema（camelCase），核心：

| Field | 说明 |
| --- | --- |
| `name`, `description` | **Required** |
| `tools` | allowlist；**代码：empty = inherit all** |
| `disallowedTools` | denylist，优先于 tools |
| `permissionMode` | `default` / `acceptEdits` / `dontAsk` / `plan` / `auto` / `bypassPermissions` |
| `promptMode`, `skills`, `agentsMd`, `toolConfig`, … | 见 README |

JSON 等价物：`from_json`（L1654–1681）用于 ACP 内联对象；同样要 `name`+`description`；无 `toolConfig` 时填 `default_grok_build_toolset()`。

**示例（frontmatter，示意）：**

```markdown
---
name: pillow-governed
description: Pillow-hosted session; native write/shell stripped
disallowedTools:
  - write
  - search_replace
  - run_terminal_cmd
  - read_file
  - list_dir
  - grep
permissionMode: default
---
Session instructions optional.
```

### 3.3 是否 stdio 正确通道？

| 通道 | stdio 是否可用 | 优先级 |
| --- | --- | --- |
| `_meta.agentProfile`（对象/名字） | **是**，且最高（普通 stock 路径） | 1 |
| `--agent-profile <PATH>` | **是**，stdio 同一 `run_agent_command` | 2（当 meta 缺席/解析失败） |
| config `[agent]` / `GROK_AGENT` | 是 | 3–4 |

**假设“通道用错了”部分成立：** Pillow **只**走了 meta 内联，且对象**非法**；合法路径包括修好 meta **或** 传 `--agent-profile`。两者在上游都是一等公民，meta 优先级更高。

### 3.4 `bundled/agents/*.md` / 内置 agents

- 公开 mirror 上 **未**看到独立 `bundled/agents/*.md` 树（本 clone 以 Rust `BuiltinAgentName` + `AgentDefinition::explore()` / `plan()` 等为主，`config.rs` L1595–1616：`explore`/`plan` 自带 curated `tool_config` + `permission_mode: Plan`）。
- Frontmatter 格式与 `--agent-profile` 文件、discovery 的 `.grok/agents/*.md` **同一 `AgentDefinition::parse` 契约**。
- 字符串形式 `"_meta": { "agentProfile": "explore" }` 走 discovery 名字解析（`turn.rs` L308–313），不是 JSON 字段补丁。

---

## 4. Q3 — `permission/**` 模型、闸点、模式语义、`session/request_permission` 条件

### 4.1 规则语法与优先级（查证）

- 解析：`xai-grok-workspace/src/permission/rules.rs` `parse_permission_rule`（约 L115+）：`Bash(...)` / `Read(...)` / `Edit`|`Write(...)` / `MCPTool(...)` / `Grep`|`Glob` / `WebFetch` / `WebSearch`；裸 `Bash` = 全 bash；`Bash(cmd:*)` 前缀惯用法。
- 求值：`policy.rs` L~230+ **deny > ask > allow**（order-independent）。
- 产品文档：`pager/docs/user-guide/22-permissions-and-safety.md` — deny 覆盖 always-approve 的通行；hooks 可先拒。

### 4.2 闸点在哪一层？

| 层 | 作用 |
| --- | --- |
| **Tool registration / builder** | `tools` / `disallowedTools` / session clamp：**工具不在模型可见列表** → 最硬“没有手” |
| **Permission manager**（`permission/manager/mod.rs`） | 执行前对 AccessKind 做 policy / yolo / auto classifier / grant / prompt |
| **ACP prompter**（`prompter.rs`） | `session/request_permission` 发给客户端 |
| **Hooks PreToolUse** | 文档：可 deny，allow 不跳过后续检查 |
| **Sandbox**（可选 OS 层） | `GROK_SANDBOX` / `--sandbox`；与 permission 正交（文档 18-sandbox） |

### 4.3 `--permission-mode` / 配置模式语义（查证）

两套词汇叠在一起：

**A. Agent / CLI 模式字符串**（`PermissionMode` in agent config，`VALID_VALUES` L996–1004）：  
`default`, `acceptEdits`, `auto`, `dontAsk`, `bypassPermissions`, `plan`。

**B. UI 软默认**（`util/config/permissions.rs`）：  
`always-approve` / `auto` / `ask`（`default`→Ask）；legacy `yolo = true`。

**DefaultModeEffects**（`rules.rs` L40–60）：

| Mode | 效果摘要 |
| --- | --- |
| `default` / `plan` | 默认 prompt policy（问） |
| `acceptEdits` | 合成 Allow Edit 规则 |
| `auto` | `PromptPolicy::Auto`（分类器） |
| `dontAsk` | `PromptPolicy::Deny`——**未预批的拒绝，不问客户端** |
| `bypassPermissions` | 合成 Allow Any（可被 managed pin 挡住） |

**产品 always-approve / yolo：** manager L1657–1665：`if yolo_mode && !shell_forced_prompt` → **Allow，不 prompt**。  
**AllowAll handle** L927–930：直接 Allow。  
**dontAsk** L2194+：`prompt_policy == Deny` → PolicyDeny，**不问客户端**。

源码注释（`config.rs` L981）：

```text
/// Permission mode. Only `BypassPermissions` is wired at spawn; others are forward-compat.
```

**查证：** 子代理路径会把 `BypassPermissions` 映射进 yolo（`subagent/handle_request.rs` ~L1087–1088）。  
**主会话 yolo** 主要来自 launch config / `_meta.yoloMode` / 运行时 set，而不是把 profile 的 `dontAsk` 完整接到 manager——**profile 上非 Bypass 的 permissionMode 对主会话是否完整生效：标 未确定（有“forward-compat”注释；未做实盘）**。工具列表过滤不依赖该字段。

### 4.4 何时发 `session/request_permission`？何时自己决定？

**发给客户端（需要 prompt）的条件（逻辑合成，查证代码路径）：**

- yolo / AllowAll **未**开；
- policy **未**直接 Allow/Deny 掉；
- `prompt_policy` **不是** DontAsk 的 Deny；
- 不是只读自动放行路径（文档：read_file/list_dir/grep 等默认不 prompt）；
- auto 模式未用分类器/fast-path 直接结论；
- 无可用 session/persisted grant 短路。

**自己决定、不问客户端：**

| 条件 | 结果 |
| --- | --- |
| yolo / always-approve | Allow（deny 规则与部分 shell ask 仍可拦） |
| policy Deny | Reject |
| policy Allow（在 yolo/auto 规则下） | Allow |
| dontAsk + 无预批 | Reject |
| AllowAll handle | Allow |
| 只读内置放行 | Allow（无 prompt） |

**与本机探针混淆项：**  
`<home>/.grok/config.toml` 实测键（非敏感）：

```toml
[ui]
yolo = false
permission_mode = "always-approve"
```

`permission_mode_from_ui_if_set`：`permission_mode` **优先于** `yolo`（`permissions.rs` L38–71）。故 **always-approve 生效** → manager 不 prompt → 探针四次会话零 `session/request_permission` **可被解释**。  
**不能**用它解释 Q4–Q6 的“工具仍被调用且写盘”——那是 **toolset 未剥除**。

---

## 5. Q4 — 优先级链；客户端能否强制用户配置不可覆盖的模式？

### 5.1 工具集（agent definition）优先级（查证）

```text
model strict harness agent_type  (特殊)
> ACP _meta.agentProfile         (对象/名)
> --agent-profile PATH
> [agent] definition/name
> GROK_AGENT
> default grok-build
然后: CliAgentOverrides.tools / disallowed_tools 覆盖主会话 definition 字段
```

### 5.2 Permission mode / yolo 优先级（查证）

`resolve_effective_yolo`（`permissions.rs` L225–238）：

```text
if CLI --permission-mode present:
  yolo = (mode ∈ {bypassPermissions, always-approve})
else if CLI --always-approve / --yolo:
  yolo = true
else:
  yolo = config/remote always-approve
然后: managed pin (disable_bypass_permissions_mode) 可强制 yolo=false
```

`resolve_permission_mode`（TOML UI > remote > Ask）：CLI 在 launch helper **之上**。

**Config 文件规则加载**（`resolution.rs` `load_config_toml_permissions`）：  
全局 `<home>/.grok/config.toml` → 受信任项目的 `.grok/config.toml`（git root→cwd）；untrusted 项目规则不进。

**`trusted_folders.toml`：** 主要闸 **项目级** config/hooks/插件信任，不是替代 permission_mode 的总开关（`folder_trust.rs` / `trust.rs`）。

### 5.3 `agent stdio` 对顶层 CLI 工具/规则 flag 的接线缺口（查证）

- `Command::Agent` → `run_agent_command(agent_args, args.permission_mode_flag, …)`（`main.rs` ~L2005）：**会**把顶层 `--permission-mode` 传入 yolo/auto launch。
- `run_agent_command` 主体（L1061–1200 段）：设置 `default_yolo_mode`、`agent_profile_path`；**未见**对 `cli_agent_overrides` 填入 `cli_tools` / `cli_disallowed_tools` / `allow_rules` / `deny_rules`。
- 这些字段在 **headless `-p` 路径**组装（`headless.rs` L817–820；`main.rs` ~L2214–2218）。

**含义：** 文档里 `grok --deny '…' -p …` 对 headless 成立；**不能默认** `grok --deny … agent --no-leader stdio` 已接线（**未**在 stdio 路径找到赋值；标 **查证：stdio 未接线**；是否 clap 仍解析但丢弃：是）。

### 5.4 🔴 客户端（Pillow）能否强制用户环境无法覆盖的模式？

| 目标 | 能否 | 依据 |
| --- | --- | --- |
| 本进程关闭用户 always-approve | **能** | 顶层 `--permission-mode default`（或非 bypass 值）使 `resolve_effective_yolo` 忽略 config yolo（L231–234） |
| 本进程打开 always-approve | **能** | `agent --always-approve` 或 `--permission-mode bypassPermissions` |
| 强制用户永久无法再开 yolo | **不能**（无 managed pin） | pin 在 requirements/managed；用户可改自己的 config |
| 仅靠 ACP meta 关掉用户 yolo | **未找到** 强制 ask 的 meta；仅有 `_meta.yoloMode: true` 打开（文档 15-agent-mode.md） | 查证文档；反向 force-ask meta **未确定/未见** |
| 硬删除工具，不依赖用户 config | **能（若 profile 合法）** | builder 过滤与 config yolo 无关；yolo 不管“工具是否存在” |
| 硬 deny 规则且用户无法用项目 config 放宽 | 部分：managed/requirements + trust 门 | 项目 untrusted 不加载项目 allow；用户全局 config 仍可读 |

**写死结论（架构）：**

1. **工具是否存在**应依赖 **合法 agent profile / builder 过滤**（或隔离 FS/sandbox），**不要**依赖用户 `<home>/.grok/config.toml` 的 yolo。  
2. **是否弹 `session/request_permission`** 会被用户 always-approve **关掉**；Pillow 可用**本进程** `--permission-mode default` 覆盖，但那只影响 prompt 管道，**不**修复非法 agentProfile。  
3. **`decide_permission` 全拒** 只在 agent **真的发起** permission 请求时有用；yolo 下该路径常不走——**不能**单独当硬边界。  
4. 若 Pillow 不控制 HOME/argv/profile，**不能**声称“用户环境无法削弱治理”。

---

## 6. Q5 — Pillow 自建 vs 上游逐项比对

| Pillow 自建 | 上游对应物 | 等价？直接用？ | 判定 |
| --- | --- | --- | --- |
| `map/permission.rs` `decide_permission` **无条件** `allowed: false`（L113–179） | ACP `session/request_permission` + manager；客户端本应回 option | 上游期望客户端**决策**；Pillow 选择**永不授予原生权威** | **保留**作为“不把原生工具权威交给 provider”的策略**当且仅当**原生写/shell 已从 toolset 剥除或 OS 隔离。否则是**假门**（agent 不询问也能写）。不应用它“替代” tool denylist。 |
| `plaintext_agent_profile()` → `_meta.agentProfile` | 同通道；或 `--agent-profile`；schema 要 `name`+`description`+正确 tool id | **通道对，payload 错** | **修 payload 或改文件 profile**；不要再发明第三套。`tools:[]` 必须改掉或改为非空 allowlist。 |
| `kernel-policy` `permission_rules` + sqlite `permission_rules` 表（Process/Write/Edit 自研 DSL） | `xai-grok-workspace/src/permission/**`（Bash/Read/Edit/MCPTool…，deny>ask>allow） | 概念同类；**语法与闸点不同**；上游规则在 **Grok 进程内执行前**；Pillow 规则在 **host 批准 pillow/* 提案时** | **不能**用 Pillow 表替代上游 tool 过滤。若原生工具仍在，表管不到。可 **保留** 管 `pillow/write_tool` / `pillow/process_tool` 的 host 路径；**删掉“复刻 Grok --allow/--deny 当作原生边界”的幻想**。 |
| `pillow/write_tool` / `pillow/process_tool` 自建 JSON-RPC（`provider_writes.rs` / `provider_processes.rs`） | 上游 MCP + 原生 Function tools；**无**同名 pillow 方法 | **不是** MCP 替代失败；是 Pillow 想要的**宿主执行**契约 | **保留**作治理写入/进程的产品面；前提是原生 `write`/`search_replace`/`run_terminal_cmd` **不可用**。MCP 是挂治理工具的推荐集成面（探针已通）。 |

**汇总判定：**

- **复用：** 修好的 agent profile（meta 或 `--agent-profile`）；可选本进程 `--permission-mode`；可选 sandbox。  
- **保留：** `decide_permission` 拒绝原生授权；`pillow/*` host 执行；host 侧 permission_rules（只治理 host 路径）。  
- **删除/收缩候选：** 把“禁用原生工具”押在当前 `plaintext_agent_profile` 上的一切注释/测试幻想；与上游 DSL **重复**且声称管原生工具的规则复制（若存在产品叙事）。

---

## 7. Q6 — 要让真实 Grok 在 `agent stdio` 下无法绕过治理，应传什么？

### 7.1 最小可执行配置（设计；**边界生效需探针复验**）

**目标：** 原生写/shell/读尽量从 toolset 消失；prompt 管道不被用户 always-approve 静默；治理走 MCP 或 `pillow/*`。

**推荐 argv（示意，无本机绝对路径）：**

```text
grok --permission-mode default agent --no-leader --agent-profile <profile.md> stdio
```

说明：

- 顶层 `--permission-mode default`：**本进程**覆盖 `<home>/.grok` 的 always-approve → 若仍有工具需批，会走 permission（查证 yolo 解析）。  
- `--agent-profile`：可靠的文件通道（不依赖 meta 必填字段手滑）。  
- Pillow 今日 `PRODUCTION_ACP_ARGV = agent --no-leader stdio`（`config.rs` L9）**不够**。

**`<profile.md>` 最小 frontmatter 示例（工具名按上游 short id）：**

```markdown
---
name: pillow-governed
description: Native tools stripped for Pillow host governance
disallowedTools:
  - write
  - search_replace
  - run_terminal_cmd
  - read_file
  - list_dir
  - grep
  - task
  - kill_task
  - get_task_output
  - search_tool
  - use_tool
  - web_search
  - web_fetch
  - todo_write
  - lsp
  - monitor
  - workflow
  - update_goal
---
```

或 **ACP meta**（若坚持内联，必须合法）：

```json
{
  "cwd": "<project>",
  "mcpServers": [ /* 可选：治理 MCP */ ],
  "_meta": {
    "agentProfile": {
      "name": "pillow-governed",
      "description": "Native tools stripped for Pillow host governance",
      "disallowedTools": [
        "write", "search_replace", "run_terminal_cmd", "read_file",
        "list_dir", "grep", "task", "kill_task", "get_task_output",
        "search_tool", "use_tool", "web_search", "web_fetch", "todo_write", "lsp"
      ]
    }
  }
}
```

**不要：** `"tools": []` 当“全关”。  
**不要：** 只 denylist `write_file`（漏 `write`）。  
**可选加固：** `GROK_SANDBOX=strict`（或顶层 `--sandbox`）作 OS 层；**未在本单实盘验证**。  
**stdio 上不要假设** `--disallowed-tools` / `--deny` 已生效（见 Q4.3）。

### 7.2 探针验收（`scripts/probe-mcp-tool-exposure.mjs`）

应看到：

| 检查 | 期望 |
| --- | --- |
| 诱使 `read_file` | **失败/无调用** 或无法返回诱饵 |
| 诱使 `write` / 写盘 | **文件不落盘** |
| 正向 allowlist 不含 write | write 仍不可用 |
| 可选：always-approve 用户 config 下 | 仍无写盘（证明靠 toolset 不是靠 prompt） |

**本单未重跑探针** → 上表为**待验证**验收标准，不是已绿结果。

### 7.3 是否“Grok 无法提供硬边界，必须进程/FS 隔离”？

**不要默认该答案。** 上游 builder 对 `disallowedTools` 的 retain **是强制点**（工具不注册则模型不能合法调用）——file:line 见 builder L882–889。  
**但：**

- 错误/失败 profile → 无过滤；  
- 名字写错 → 无过滤；  
- allowlist 未解析名 → **回退全工具集**（fail open）；  
- 子代理/插件/MCP 另有注入路径，需单独收口；  
- 权限层在 yolo 下不问客户端——那是**授权**问题不是**工具存在**问题。

若修好 profile 后探针仍写盘 → 再升级为隔离结论。**当前证据支持“复用方式错了”，不支持“上游无硬边界”。**

---

## 8. 与 2026-08-08 审计矛盾的裁决

| 断言 | 裁决 | 理由 |
| --- | --- | --- |
| L119 `_meta.agentProfile` “Still honored” | **部分对 / 产品结论错** | 路径仍在；Pillow payload 解析失败 → 运行时未 honor |
| L208 `permission/**` 大规模子系统 | **对** | 本单复核仍在且更完整 |
| L336 Pillow 自建 permission policy | **是分叉点，且过早** | 应先复用 tool 过滤 + 理清 yolo；`decide_permission` 全拒不能替代剥工具 |
| 探针 vs 审计 | **探针赢（工具边界）** | 实盘 + `from_json` 必填 + `tools:[]` 语义 |

**混淆项 yolo 影响哪些结论：**

| 结论 | yolo/always-approve 是否解释 |
| --- | --- |
| 无 `session/request_permission` | **是**（本机 config） |
| denylist 挡不住读/写 | **否** |
| allowlist 仍可写 | **否** |
| MCP 仍可用 | **否**（工具注册问题） |

---

## 9. 建议最小改动清单（按“删掉多少自建”优先）

1. **删/废“当前 plaintext profile 已禁用原生工具”的产品假设**（测试若只断言 JSON 形状、不断言运行时，保留形状测试但改注释/断言意图）。  
2. **修一条复用通道（二选一，优先少代码）：**  
   - **A.** 修正 `plaintext_agent_profile()`：加 `name`/`description`；去掉无效 `tools:[]` 或改为真实非空 allowlist；denylist 改用真实 short id（`write`、`run_terminal_cmd`…）；**或**  
   - **B.** argv 增加 `--agent-profile <Pillow 生成的 md>`，meta 可删简。  
3. **argv 增加** 顶层 `--permission-mode default`（或等价），避免用户 always-approve 吞掉 permission 观测与 deny 路径——**不**当作唯一边界。  
4. **`decide_permission` 全拒：保留**，定位改为“原生权威永不授予 host 语义”，不是“工具不存在”。  
5. **`pillow/*` + host permission_rules：保留** 管宿主路径；**不要**再扩一层“假装是 Grok Bash()/Read() DSL”除非明确只服务 host。  
6. **实盘：** 用修好的 profile 跑 `probe-mcp-tool-exposure.mjs`；通过后再谈 MCP 治理工具接入。  
7. **未做前不要：** 指向真实用户仓库的生产会话。

---

## 10. 查证 / 未确定 总表

| 项 | 状态 |
| --- | --- |
| Pillow profile 缺 name/description → from_json 失败回落 | **查证**（代码+单测） |
| `tools: []` = inherit all | **查证**（config 注释 + builder 分支） |
| README `[] = none` 与代码冲突 | **查证** |
| 写工具 short id = `write`；shell = `run_terminal_cmd` | **查证** |
| agent stdio 消费 meta + `--agent-profile` | **查证** |
| agent stdio **未**接线 `--tools`/`--deny` 到 overrides | **查证**（run_agent_command 无赋值） |
| 顶层 `--permission-mode` 覆盖 config yolo | **查证** |
| 本机 `[ui] permission_mode = always-approve` | **查证**（只读键名；未打印密钥） |
| 探针：profile 无效、可写盘 | **查证**（前序实盘文档；本单未重跑） |
| 修好 profile 后探针必绿 | **未确定**（待跑） |
| profile `permissionMode: dontAsk` 主会话完整生效 | **未确定**（“Only BypassPermissions wired” 注释） |
| MCP 在严格 allowlist 下是否仍注入 | 前序探针在**无效 profile**下通过；修好 denylist 后 **未确定** |
| sandbox strict 对 Windows 写盘的硬拦 | **未确定**（本单未测） |
| 本地 CLI `1.0.0 (3cd0d0cbce)` 与 mirror `75e73f3` 字节级一致 | **未确定**（版本带对齐 1.0.0；未比 binary） |

---

## 11. 证据索引（路径相对上游 clone / Pillow 仓库）

| 主题 | 位置 |
| --- | --- |
| Pillow argv | `crates/adapter-grok-acp/src/config.rs` L9 |
| Pillow profile | `crates/adapter-grok-acp/src/rpc/methods.rs` L5–63 |
| Pillow deny-all permission | `crates/adapter-grok-acp/src/map/permission.rs` L113–179 |
| Pillow write/process RPC | `provider_writes.rs`, `provider_processes.rs` |
| Pillow host rules | `crates/kernel-policy/src/permission_rules.rs` |
| 实盘探针 | `docs/research/2026-08-10-mcp-tool-exposure-probe.md` |
| 旧审计 | `docs/execution/2026-08-08-upstream-grokbuild-audit.md` |
| 解析 meta | `xai-grok-shell/src/upload/turn.rs` L284–318 |
| resolve 优先级 | `…/mvp_agent/agent_ops.rs` L3738–3779, L4218+ |
| from_json / 必填 | `xai-grok-agent/src/config.rs` L1669–1681, L2411–2417 |
| empty tools | 同上 L782；builder L901 |
| denylist 过滤 | builder L882–889 |
| write 注入 | builder L763–766；`opencode/write/mod.rs` L71–72 |
| yolo 短路 | `permission/manager/mod.rs` L1657–1665 |
| yolo CLI 优先 | `util/config/permissions.rs` L159–238 |
| 规则 deny>ask>allow | `permission/policy.rs` evaluate |
| agent profile CLI | `pager/src/app/cli.rs` AgentArgs；`pager-bin` L74–86, L1151–1154 |
| 用户指南 modes | `pager/docs/user-guide/22-permissions-and-safety.md` |
| ACP meta yolo | `pager/docs/user-guide/15-agent-mode.md` Session `_meta` |

---

## 12. 明确非动作

- 未修改除本报告外任何文件。  
- 未 `cargo` / `pnpm` build / 门 / E2E。  
- 未读 `auth.json` 或打印 token。  
- 未重跑 live 探针（沿用 2026-08-10 实盘文档）。  
- 临时上游 clone 应删除（报告写完后由 worker 清理）。

`GROKBUILD-PERMISSION-REUSE-COMPLETE`
