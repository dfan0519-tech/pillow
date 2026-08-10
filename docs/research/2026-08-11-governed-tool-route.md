# 边界堵死之后,治理工具还能怎么交到模型手里?

Date: 2026-08-11  
Status: **只读调研完成**（上游 shallow clone + 本仓代码 + 既有实盘报告）  
Worker: grok1（只读）  
Controller: Claude  
Upstream pin: `xai-org/grok-build` @ `75e73f3`（`SOURCE_REV` `a61c32b…`）  
Scope: 禁止改码 / 禁止 build / 禁止跑门;本单唯一写入即本文件。

完成信号:`GOVERNED-TOOL-ROUTE-COMPLETE`

---

## 1. 一句话结论

**在“不重新打开 ambient MCP”优先下,治理写/读应走 ACP 客户端反向 RPC:`clientCapabilities.fs` 双开 + allowlist 放行原生 `write`/`search_replace`/`read_file`,由宿主在 `fs/write_text_file` / `fs/read_text_file` 上做提案→批准→执行;MCP/`use_tool` 继续禁。进程走同一模式的 `terminal/*`(差距大,次优先)。`ask_user_question` 不适合作治理工具通道。**

依据:上游把 `fs/*` 与 `terminal/*` 做成“原生工具执行时的后端”,不是“allowlist 挡住 write 后的自动退路”;MCP 仍只有 `use_tool` 这一扇门,开它就连带 ambient。

> 任何“能/不能”若只有代码路径、没有实盘,标 **未确定**。工具名出现 ≠ 调用成功。

---

## 2. 处境(本单不重新论证)

| 事实 | 来源 |
| --- | --- |
| shipped profile = allowlist `ask_user_question` + denylist `search_tool`/`use_tool` | Pillow `crates/adapter-grok-acp/src/rpc/methods.rs` `plaintext_agent_profile` |
| allowlist **故意**保留 MCP 元工具 | 上游 `xai-grok-agent/src/builder.rs` L946–950;单测 `restrictive_allowlist_keeps_mcp_access` |
| `mcpServers: []` **关不掉** ambient merge | 上游 `managed_mcp.rs` L1–13, L124–160;既有 `2026-08-10-ambient-mcp-boundary.md` |
| denylist 元工具后 ambient MCP 调用面关上 | 实盘 `2026-08-10-mcp-tool-exposure-probe.md` §9 |
| 客户端 MCP 仍 spawn/list,但 **tools/call 从不发生** | 同报告 §10 变体 `shipped-profile-client-mcp` |

---

## 3. Q1 — 能不能只放行“客户端自己挂的 MCP”,挡掉环境侧?

### 查证:会话/spawn 层 **没有** “client-only MCP” 开关

合并语义(上游 `xai-grok-shell/src/session/managed_mcp.rs`):

| 层 | 行为 | 行 |
| --- | --- | --- |
| 注释合并序 | config.toml → plugins → `~/.claude.json` → `.mcp.json` → **Client `insert` 叠加** | L1–13 |
| `merge_managed_mcp_servers_with_policy` | 先装 ambient 源,再 `admit_client_mcp_servers` **插入** client | L124–140 |
| 空 client 列表 | **不清除** ambient | 同函数;空 `Vec` 只表示 client 不追加 |

结论:**`session/new.mcpServers` 只叠加,不能“只信客户端”。**

### 候选杠杆(能否由**客户端在会话/spawn 层**控制)

| 候选 | 能否会话层单独控制 | 证据 | 判据 |
| --- | --- | --- | --- |
| managed-settings `allowedMcpServers` | **否**(进程级/磁盘策略) | `xai-grok-workspace/.../permission/resolution.rs` L1026–1038;`managed_mcp.rs` L158–159 读 `managed_settings().mcp_allowlist` | **查证**:能按名/命令/URL 滤 **所有** merge 结果(含 client),但是 **managed-settings 文件 / OnceLock**,不是 ACP 字段 |
| `disabled_mcp_servers` | **否**(用户盘配置) | `managed_mcp.rs` L142;`acp_session_impl/mcp.rs` L531+ 读 `disabled_mcp_server_names(cwd)` | **查证**:命名禁用;写配置,非会话参数 |
| `GROK_MANAGED_MCP_GATEWAY_TOOLS_ENABLED` / `GROK_MANAGED_MCPS_ENABLED` | **spawn env 可注入** | `xai-grok-shell/src/config/mod.rs` L658–711;Pillow `build_child_env` + `extra_env` | **查证**:只关 **managed/gateway catalog**(`tasks`/`voice` 类)。**挡不住**用户 `config.toml` / 插件 / `.mcp.json` 本地 MCP |
| `_meta` MCP 相关 | **有,但不是 client-only** | `_meta["x.ai/mcp/servers"]` → 进程内 SDK 反向 MCP(`acp_mcp.rs` L1–13, L28–51);`parse_mcp_meta_config` 等 | **查证**:多注册/配置,不替换 ambient 全集 |
| server 命名/前缀过滤 | **无**会话级“只允许某前缀” | `use_tool` → `dispatch_mcp_tool` 无 agentProfile 再过滤(`use_tool/mod.rs` L315–376) | **查证**:模型侧 `server__tool` 命名存在,过滤点在 merge 策略/权限,不在 profile |
| `mcpInheritance` | **否**(主会话) | `config.rs` L924–940:子代理继承 | **查证** |
| `_meta.agentProfile.mcpServers` | 未作“只信 client” | agent 定义可带 `mcp_servers` 引用(`config.rs` L800–803),不是关 ambient | **查证**(语义);主会话如何叠 **未在本单逐路径钉死** → 子项 **未确定** |

### 附加:SDK 反向 MCP(`x.ai/mcp/sdk_call`)

上游 `xai-grok-mcp/src/wire.rs`:`MCP_SDK_CALL = "x.ai/mcp/sdk_call"`,`MCP_SERVERS = "x.ai/mcp/servers"`。  
宿主可在 `_meta` 挂 **in-process** 工具,经反向 RPC 执行——**仍要模型有 `use_tool` 才能发起 MCP 调用**(与 stdio client MCP 同一调用面)。开 `use_tool` 后 ambient 一并可达。

### Q1 结论

| 陈述 | 等级 |
| --- | --- |
| 无 ACP/会话字段 = “仅 `session/new.mcpServers`” | **查证** |
| env 可关 gateway 工具,不能单独关用户 ambient 文件 MCP | **查证** |
| managed `allowedMcpServers` 能滤,但是磁盘策略层 | **查证** |
| 只放 `use_tool`、靠名字只调 client server、ambient 永不被调 | **未确定**(代码无强制门;需对抗探针) |
| 隔离空 HOME + gateway off + 再开 `use_tool` 是否“实践上只剩 client MCP” | **未确定**(运维/凭据冲突已在 ambient 报告标明) |

**保留 `use_tool` 且严格只见 client MCP:当前没有干净的会话层答案。**

---

## 4. Q2 — 除了 MCP,ACP 还有哪条“客户端提供能力”的路?

### 查证:客户端反向 RPC 清单(agent → client)

上游 `xai-acp-lib/src/message.rs` L132–170 绑定:

| 方法族 | 能力门 | Pillow 今天 |
| --- | --- | --- |
| `fs/read_text_file` | `clientCapabilities.fs.readTextFile` | **true**,且接治理读(`provider_tools.rs` `READ_METHODS`) |
| `fs/write_text_file` | `clientCapabilities.fs.writeTextFile` | **false**,未实现(`methods.rs` `initialize_params`) |
| `terminal/create|output|release|wait|kill` | `clientCapabilities.terminal` | **false** |
| `session/request_permission` | 标准 | 已处理(`prompt.rs`) |
| `ext_method` 如 `x.ai/ask_user_question` | 扩展 | **未实现**(见 Q3) |
| `x.ai/mcp/sdk_call` | SDK MCP | **未用** |

Pillow 另有 **自建** `pillow/write_tool` / `pillow/process_tool`(`prompt.rs` L347–354):  
**查证** — 仅 fake agent 脚本会发;真实 Grok **不会**发这些方法名(无上游工具映射)。

### 关键:原生 `write` 被 allowlist 挡掉时,会不会退而调 `fs/write_text_file`?

**查证:不存在“工具没了就改打 ACP 写”的 fallback。**

强制点链:

1. **模型只看到 toolset 里的原生工具**  
   allowlist retain 在 `builder.rs` L901–951;`write` 不在 `tools` 里 → 装配时剥掉 → 模型无 `write` 可调。

2. **`fs/write_text_file` 是工具执行后端,不是模型工具**  
   `AcpFsAdapter`(`xai-grok-workspace/.../file_system/adapter.rs` L1–21, L50–62):  
   `write_file()` → ACP `WriteTextFileRequest`。  
   原生 `write` 工具(`opencode/write/mod.rs` L135)调的是注入的 `FileSystem.write_file`——**前提是模型已调用了 `write` 工具**。

3. **ACP FS 后端安装条件是双开**  
   `agent_ops.rs` L3956–4000:
   ```text
   use_acp_fs = client_fs_read && client_fs_write
   ```
   只开 read、关 write → **LocalFs**(子进程直写盘),**不会**发 `fs/read_text_file`/`fs/write_text_file` 做工具 IO。  
   同逻辑:spawn 侧 `client_fs_capable`(`spawn.rs` L731–738)需 capable + gateway。

4. **tools crate 内无 `fs/write_text_file` 字面**  
   对 `xai-grok-tools` 检索无命中:模型侧不会“直接 call 方法名”。

| 场景 | 行为 | 等级 |
| --- | --- | --- |
| allowlist 无 `write`,writeTextFile true/false | 模型无 write 工具 → **不会**发 `fs/write_text_file` | **查证**(代码路径) |
| allowlist 有 `write`,但 `writeTextFile=false` | LocalFs → **子进程直写盘**,绕过宿主写治理 | **查证**(路径);实盘落盘 **未确定**(本单不探针) |
| allowlist 有 `write`,`read+write` 双 true | 经 `AcpFsAdapter` → 宿主 `fs/write_text_file` | **查证**(路径);治理闭环 **未确定** |
| allowlist 无 write 时“模型猜 ACP 方法” | 无标准工具表面 | **查证**;是否幻觉乱试 **未确定**/需探针 |

**若只能实测才能钉“双开 + allowlist write 后 100% 走 ACP”→ 标 需探针(见 §6 变体)。**

### 读路径注意(Pillow 现状)

今天 `readTextFile=true` 且 `writeTextFile=false` → 按上游 **`use_acp_fs=false`**。  
故:**真实 Grok 的原生 `read_file`(若被放行)会走 LocalFs,不一定打到 Pillow 已实现的 `fs/readTextFile`。**  
现有治理读验收大量依赖 fake 发 `fs/readTextFile` / `pillow/read_tool`。  
**实盘“双 cap 前原生读是否已走 ACP”→ 未确定(代码说否)。**

---

## 5. Q3 — `ask_user_question` 能不能承载治理?

### 它如何到达 ACP 客户端

**查证** 上游路径:

1. 模型调原生工具 `ask_user_question`(`AskUserQuestionTool`)。  
2. 工具经 `UserQuestionSender` mpsc → session 协调器。  
3. 协调器发 **`ext_method`**:`x.ai/ask_user_question`  
   (`spawn.rs` L1965–2027;`types.rs` L50–61 `AskUserQuestionExtRequest`)。  
4. 载荷形状固定:`sessionId`, `toolCallId`, `questions[]`(选项/预览/multi_select), `mode`(`default`|`plan`)。  
5. 客户端应答:`outcome` ∈ `accepted` | `chat_about_this` | `skip_interview` | `cancelled`(`types.rs` L94–121)。

Pillow:allowlist **唯一**放行此工具; **未**实现 `ext_method` 处理器 → 真 Grok 调用时,协调器 `gateway.ext_method` 会失败/超时路径(代码路径 **查证**;实盘失败形态 **未确定** — 探针曾见代理可调它并结束回合,可能走 migration fallback `MIGRATION_FALLBACK=true` 的 fire-and-forget,`ask_user_question/mod.rs` L45–48)。

### 能否作“代理→宿主”结构化治理通道?

| 维度 | 评估 | 等级 |
| --- | --- | --- |
| 载荷语义 | 访谈式选择题,不是 tool call schema(无 path/content/argv) | **查证** |
| 往返 | 用户 UI 应答,非宿主静默执行器 | **查证**(types 注释 L1–9:tools/shell/pager) |
| 权限/审计 | 不进 `session/request_permission` 工具链;无 write proposal 形状 | **查证**(路径分离) |
| 滥用为 JSON 通道 | 可把整段 JSON 塞进 `question`/`options` 文本 — **协议不禁止** | **查证**(schema 是字符串);模型是否稳定配合 **未确定** |
| 与 C3/C4 | 无 sealed proposal / one-time grant / durable 收据挂钩 | **查证**(Pillow 未接) |

**结论:不合适作为治理工具交付面。**  
理由不是“感觉”,而是:**它是人机澄清 UI 协议,不是能力执行协议**;硬塞执行语义会绕开已建的 proposal 账本,且依赖未实现的 `ext_method` 与用户点击。  
它仍适合:治理前的澄清问题(在允许列表里保留有意义)。

---

## 6. Q4 — 进程执行(C4)怎么办?

### 上游 `terminal/*` 模型

当 `clientCapabilities.terminal=true` 且有 gateway:

- `select_terminal_backend_kind` → `AcpClient`(`spawn.rs` L2564–2575)  
- `AcpTerminalAdapter` 把 bash/run 类工具变成 client `terminal/create` 等(`terminal/adapter.rs` L1–2, L161+)  
- 请求形:`command` + `args[]` + `env` + `cwd` + `output_byte_limit`(`extensions/terminal.rs` L20–28)  
  另有 PTY 扩展(`x.ai/terminal/*`)

当 terminal=false:本地 `LocalTerminalBackend` / streaming — **子进程在 agent 侧起**,宿主看不见。

### 与 Pillow C4 模型的距离

| Pillow C4(既有研究 `2026-08-02-c4-terminal-process-research.md`) | 上游 terminal/* |
| --- | --- |
| 结构化 `{ program, argv[], cwd }`,禁 shell 元字符 | **command 字符串 + args**;本地路径还常 **shell -c 片段**(`streaming_local_terminal.rs` L281 注释) |
| 目录表 + ProjectCapability 封闭 | cwd 可选字符串,由 client 自己 enforce |
| approve-once proposal + 收据 | 无内置 proposal;需宿主在 `terminal/create` 上自建 |
| 非交互 one-shot 优先 | 完整 create/output/wait/kill + PTY |

**差距:大。** 不是“接上 terminal cap 就等于 C4”,而是“用同一条反向 RPC 管道,在 create 入口做严格 argv 规范化 + 拒绝 shell 串”。

### 比 MCP 更可控的路?

| 路 | 可控性 | ambient MCP 洞 |
| --- | --- | --- |
| 保持 denylist 元工具 + terminal cap + allowlist `run_terminal_command` | 宿主见每次 create | **不**因 MCP 打开(与 Q1 正交) |
| 开 `use_tool` + 进程 MCP | 同 ambient 风险 | **会**重开 |
| 仅 `pillow/process_tool` | 宿主已实现 | 真 Grok **不调用**(**查证**) |

**推荐进程与写同构:ACP 反向通道 + 宿主治理;不要为了 C4 单独重开 MCP。**  
C4.0 仍应按既有设计做 **结构化 argv**,在 `terminal/create` 适配层把上游自由 command **降维/拒绝**,而不是把 shell 原样放行。

---

## 7. Q5 — 候选路线(边界不倒退优先,其次改动小)

### 路线 R1 — ACP 原生工具 + 客户端 FS/Terminal 反向 RPC(**推荐**)

| 项 | 内容 |
| --- | --- |
| 上游依据 | `use_acp_fs = read && write` `agent_ops.rs` L3956; `AcpFsAdapter` adapter.rs L19–21; terminal 选择 `spawn.rs` L2564–2575; allowlist `builder.rs` L901–951;**继续** denylist 元工具 L882–888 |
| Pillow 粗改 | ① `initialize`: `writeTextFile=true`(与 read 同开),视 C4 再 `terminal=true` ② allowlist 增补最小集:`read_file`/`write`/`search_replace`(写),`run_terminal_command`(进程,可后置) ③ **实现** `fs/write_text_file` → 现有治理写链(可弃用对真 Grok 无意义的唯一依赖 `pillow/write_tool`) ④ `terminal/create` 等 → 治理进程,严格 argv ⑤ **保持** `disallowedTools: [search_tool, use_tool]` 与 `mcpServers: []` |
| 探针 | **能**。见 §8 `acp-fs-write-governed` / `acp-fs-localfs-trap` |
| ambient MCP 洞 | **不**因本路线打开(元工具仍禁)。新风险:**LocalFs 陷阱**——若只放 write 工具却未双开 FS cap,agent **直写盘** |

### 路线 R2 — 有条件重开 MCP(client 工具 + 压 ambient)

| 项 | 内容 |
| --- | --- |
| 上游依据 | MCP 调用唯一模型入口 `use_tool`(`use_tool/mod.rs` L375–376);merge 无 client-only;gateway env `config/mod.rs` L702–710;可选 managed allowlist |
| Pillow 粗改 | allowlist 仍紧;从 denylist **去掉** `use_tool`(±`search_tool`);`mcpServers` 挂治理 server;spawn `GROK_MANAGED_MCP_GATEWAY_TOOLS_ENABLED=false` 等; ideally 隔离 HOME / managed `allowedMcpServers` 只含治理名 |
| 探针 | **能**。`mcp-use-tool-client-only` / `mcp-use-tool-ambient-still` |
| ambient MCP 洞 | **会重新打开机制门**。仅靠 env 关 gateway **不够**;用户盘 MCP 在无隔离时仍可调。边界“不倒退”**弱于 R1** |

### 路线 R3 — SDK 反向 MCP(`_meta["x.ai/mcp/servers"]` + `x.ai/mcp/sdk_call`)

| 项 | 内容 |
| --- | --- |
| 上游依据 | `acp_mcp.rs` L1–13; `wire.rs` MCP_SDK_CALL / MCP_SERVERS |
| Pillow 粗改 | 实现 `x.ai/mcp/sdk_call`;session meta 注册 in-process tools;**仍要** `use_tool` |
| 探针 | **能**(扩展探针处理 reverse method) |
| ambient MCP 洞 | 与 R2 相同(开 `use_tool`) |

### 路线 R4 — 上游要“client MCP only”会话旗标(**长期**)

| 项 | 内容 |
| --- | --- |
| 上游依据 | **当前不存在**(Q1) |
| Pillow | 等上游;或 fork(产品未授权) |
| 探针 | 合入后必须 |
| ambient 洞 | 正确设计下可关 |

### 明确否决/降级

| 路 | 原因 |
| --- | --- |
| 只靠 `ask_user_question` 当工具总线 | Q3:**协议形态不对** |
| 只靠 `pillow/write_tool` 等 | 真 Grok **不调用** |
| 只放宽 denylist 不接宿主实现 | 手给了但落到 LocalFs/本地 bash = 边界倒退 |
| denylist 枚举更多原生工具 | 已实盘证伪(workflow/monitor) |

### 对比表与排序

| 序 | 路线 | 边界不倒退 | 改动 | 真 Grok 可达性 | 推荐 |
| --- | --- | --- | --- | --- | --- |
| 1 | **R1 ACP FS/Terminal** | 高(MCP 门继续关);须防 LocalFs 陷阱 | 中(cap+allowlist+实现 write/terminal 治理) | 高(训练过的原生工具) | **首选** |
| 2 | R2 条件 MCP | 中低(机制门重开) | 小–中(profile+env+MCP server) | 高(已探针过 MCP 路) | 仅当 R1 实盘失败且能隔离 ambient |
| 3 | R3 SDK MCP | 同 R2 | 中高 | 中(依赖 use_tool+meta) | R2 变体,非优先 |
| 4 | R4 上游旗标 | 最高(若正确) | 外部依赖 | — | 并行产品诉求 |

**推荐执行序:**  
1) 探针钉 R1 的 ACP 写路径与 LocalFs 陷阱;  
2) 产品改 R1 最小写闭环;  
3) C4 再开 terminal cap;  
4) 不默认重开 `use_tool`。

---

## 8. 探针变体建议(`PILLOW_MCP_PROBE_ONLY=<id>`)

现有脚本:`scripts/probe-mcp-tool-exposure.mjs`。本单 **不改脚本**,只给规格。  
判定原则:**读盘/事件日志,不信模型叙述;工具名出现 ≠ 成功。**

| id | 目的 | profile / caps 要点 | 通过条件 |
| --- | --- | --- | --- |
| `acp-fs-write-governed` | R1 主路径 | allowlist:`ask_user_question`+`write`(±`search_replace`);**仍 denylist 元工具**;initialize **read+write true**;client 实现 `fs/write_text_file` 记日志或写沙箱 | 见 `fs/write_text_file` 请求;**且**沙箱内容符合治理;无 ambient MCP 调用 |
| `acp-fs-localfs-trap` | 防 LocalFs 倒退 | 同 allowlist write,但 **writeTextFile=false**(或只 true read) | 若探针目录出现文件且 **无** ACP write 请求 → **陷阱成立**,禁止上线该组合 |
| `acp-fs-no-write-tool` | Q2 fallback | allowlist **无** write;writeTextFile true;prompt 强令写文件 | **不得**出现 `fs/write_text_file`;文件不得落盘 |
| `mcp-use-tool-reopen` | R2 风险基线 | 去掉 denylist 元工具;client MCP + 空 ambient 假设 | `mcpToolInvoked=true` 且 token 返回(能力恢复) |
| `mcp-use-tool-ambient-still` | R2 是否仍见 tasks/voice | 同上;prompt 诱使 `tasks__list`/`use_tool` | 若 gateway 工具成功 → 证明重开 MCP 洞(历史已有;作回归门) |
| `gateway-env-off` | env 杠杆 | 同 ambient 观察 + `GROK_MANAGED_MCP_GATEWAY_TOOLS_ENABLED=false` | tasks/voice 是否消失(**未确定**→本变体) |
| `ask-user-ext-method` | Q3 实盘 | shipped profile;prompt 调 ask_user_question | 客户端是否收到 `x.ai/ask_user_question`;无 ext 实现时的失败/超时形态 |
| `terminal-create-shape` | C4 差距 | allowlist `run_terminal_command`;terminal true;实现 terminal/* 只记参数 | 记录 command/args 原始形状,对照结构化 argv 可行性 |

运行例:

```bash
PILLOW_MCP_PROBE=1 PILLOW_MCP_PROBE_ONLY=acp-fs-write-governed node scripts/probe-mcp-tool-exposure.mjs
```

---

## 9. 证据索引(复审)

| 声明 | 等级 | 位置 |
| --- | --- | --- |
| allowlist 保留 SearchTool/UseTool | **查证** | `xai-grok-agent/src/builder.rs` L946–950 |
| denylist 先于 allowlist | **查证** | 同文件 L882–888 → L901+ |
| MCP 经 use_tool dispatch | **查证** | `xai-grok-tools/.../use_tool/mod.rs` L375–376 |
| ambient+client merge,空 client 不清除 | **查证** | `managed_mcp.rs` L1–13, L124–140 |
| managed allowedMcpServers | **查证** | `permission/resolution.rs` L1026+;`managed_mcp.rs` L158–159 |
| gateway env | **查证** | `shell/src/config/mod.rs` L658–711 |
| use_acp_fs 需 read∧write | **查证** | `agent_ops.rs` L3956–4000 |
| AcpFsAdapter 映射 write_text_file | **查证** | `file_system/adapter.rs` L50–62 |
| write 工具调 FileSystem 后端 | **查证** | `opencode/write/mod.rs` L135 |
| 无 tools 侧 fs/write 字面 fallback | **查证** | tools 树检索无命中 |
| ask_user → ext_method | **查证** | `spawn.rs` L1992–2027;`ask_user_question/types.rs` L50–121 |
| SDK MCP meta | **查证** | `acp_mcp.rs` L1–51;`wire.rs` |
| terminal cap → AcpClient | **查证** | `spawn.rs` L2564–2575 |
| Pillow profile / caps | **查证** | `adapter-grok-acp/src/rpc/methods.rs` |
| Pillow 治理方法 | **查证** | `prompt.rs` L347–354;`provider_tools.rs` L14–15 |
| client MCP 通但 call 无;元工具 denylist 有效 | **查证(实盘)** | `docs/research/2026-08-10-mcp-tool-exposure-probe.md` §9–10 |
| 双 cap + allowlist write 的实盘闭环 | **未确定** | 需 `acp-fs-write-governed` |
| 重开 use_tool 后 ambient 必破 | **代码高概率;对抗面未确定** | 需 `mcp-use-tool-ambient-still` |

---

## 10. 给总控的可执行摘要

1. **不要**为了交治理工具而默认拿掉 `search_tool`/`use_tool` denylist——那是 ambient 边界的门闩。  
2. **要**把“手”换成上游已训练的原生 IO 工具,但 **后端必须是客户端 ACP**,且 **FS 读写 cap 必须同时为 true**,否则 LocalFs = 静默越权。  
3. `ask_user_question` 留下做问答即可,别当 RPC 总线。  
4. C4 接 `terminal/*` 时按 **适配器**做,不要把上游 shell 串直接当 C4.0。  
5. 下一动作应是 **探针 R1**,不是再写一篇“MCP 是否可行”(在无边界时已答过;有边界后答案已变)。

`GOVERNED-TOOL-ROUTE-COMPLETE`
