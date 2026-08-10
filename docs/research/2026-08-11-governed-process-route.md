# 治理**进程**执行该走哪条路?

Date: 2026-08-11  
Status: **只读调研完成**（上游 shallow clone + 本仓代码 + 既有实盘/路线报告）  
Worker: grok1（只读）  
Controller: Claude  
Upstream pin: `xai-org/grok-build` @ `75e73f3`（`SOURCE_REV` `a61c32b…`）  
Scope: 禁止改码 / 禁止 build / 禁止跑门;本单唯一写入即本文件。  
Clone 读完即删（临时目录,报告不写本机绝对路径）。

完成信号:`GOVERNED-PROCESS-ROUTE-COMPLETE`

---

## 1. 一句话结论

**真实 Grok 的进程手是原生 shell 工具（`run_terminal_command` / `run_terminal_cmd`）→ 仅当 `clientCapabilities.terminal=true` 且 gateway 存在时走 ACP `terminal/*`;生产路径恒发 `command` 字符串且 `args=[]`。这与 Pillow C4「程序目录表 + 结构化 argv、禁 shell 串」无法无损对齐。同构半开陷阱是:allowlist 放行 shell 工具但 `terminal=false` → 代理 `LocalTerminalBackend` 自己起进程,宿主全程无声。若坚持 C4 不变量不丢,进程方向在 ACP 上不能无损承载——那是合法产品决策点,不要为了“有路”硬凑。**

依据:上游 `select_terminal_backend_kind` / `AcpTerminalAdapter` / 工具 `BashToolInput.command: String`;Pillow `PROCESS_CATALOG` + `SealedProcessSpec`。  
写方向双开已实盘(`docs/research/2026-08-10-mcp-tool-exposure-probe.md` §11);进程路径**本单未探针**,凡“模型会不会真的发 create”标 **未确定**。

> 任何“能/不能”若只有代码路径、没有实盘,标 **未确定**。工具名出现 ≠ 调用成功。

---

## 2. 处境(本单不重新论证)

| 事实 | 来源 |
| --- | --- |
| 写方向: `use_acp_fs = read ∧ write` 双开 + allowlist `write` → `fs/write_text_file` 实测通 | 实盘 `2026-08-10-mcp-tool-exposure-probe.md` §11 |
| MCP 元工具 denylist 关 ambient;开 `use_tool` 会重开门 | 同报告 §9–10;`2026-08-11-governed-tool-route.md` |
| Pillow 进程模型:目录表 + 结构化 argv + 提案→批准→宿主执行→收据 | `crates/tool-executor/src/process.rs`;`crates/runtime-host/src/host/process.rs`;`crates/kernel-policy/src/process.rs` |
| 今日 Pillow `initialize`: `terminal: false`;profile 仅 `ask_user_question` + denylist 元工具 | `crates/adapter-grok-acp/src/rpc/methods.rs` L53–59, L43–50 |
| `pillow/process_tool` 仅 fake agent 会发;真 Grok 无映射 | 既有 `2026-08-11-governed-tool-route.md` Q4 |

Pillow C4.0 不变量(本单**不建议推翻**,除非产品明确改模型):

1. **程序目录表**(`PROCESS_CATALOG`: cargo/rustc/node/python/python3 等),不是自由 PATH 搜索任意 basename。  
2. **结构化 argv**(不是 shell 串;不接受串联/管道/重定向作为一次批准对象)。  
3. **提案 → 一次性批准 → 宿主执行 → durable 收据 → 项目隔离**。

---

## 3. Q1 — ACP `terminal/*` 完整契约

### 3.1 标准方法集(agent → client 反向 RPC)

| 方法 | 常量(schema) | 请求要点 | 响应要点 |
| --- | --- | --- | --- |
| `terminal/create` | `TERMINAL_CREATE_METHOD_NAME` | `sessionId`, **`command: String`**, `args: String[]`(默认空), `env[{name,value}]`, `cwd?`(绝对路径), `outputByteLimit?` | `terminalId` |
| `terminal/output` | … | `sessionId`, `terminalId` | `output`, `truncated`, `exitStatus?` |
| `terminal/wait_for_exit` | … | `sessionId`, `terminalId` | `exitCode?`, `signal?` |
| `terminal/kill` | … | `sessionId`, `terminalId` | kill 结果(库类型 `KillTerminalResponse`) |
| `terminal/release` | … | `sessionId`, `terminalId` | 空/释放确认 |

**查证** 方法名与结构:  
`agent-client-protocol-schema` `src/v1/client.rs` L1127–1176(`CreateTerminalRequest`), L2290–2298(方法名常量), L1737–1741(`ClientCapabilities.terminal` 注释:“support **all** `terminal/*` methods”)。  
上游绑定:`xai-acp-lib/src/message.rs` L152–176。

### 3.2 `command` 是字符串还是 argv?

**两者协议都支持,语义分叉:**

| 层 | 形状 | 等级 |
| --- | --- | --- |
| ACP 线协议 | `command: String` + **可选** `args: Vec<String>` | **查证** schema L1137–1143 |
| 上游本地 piped 实现 | `args` 非空 → **直接 spawn program+argv**;`args` 空 → **shell 片段**(`bash -c` / Windows shell) | **查证** `streaming_local_terminal.rs` L280–286 |
| 真实 Grok 工具→ACP 路径 | `AcpTerminalAdapter` **恒** `.args(vec![])` + 把整段 `command` 当 shell 串(Unix 再包一层 `shell -lc`) | **查证** `terminal/adapter.rs` L114–127, L161–173, L186–187 |

结论:**协议能承载结构化 argv;生产 Grok 路径不发。** 客户端若只实现“空 args = shell 串”,与上游本地实现一致。

### 3.3 `clientCapabilities.terminal` 语义

| 陈述 | 等级 | 证据 |
| --- | --- | --- |
| 单一布尔,表示客户端支持全部 `terminal/*` | **查证** | schema `ClientCapabilities.terminal` L1737–1741 |
| 从 `initialize` 读出;可被 session `_meta.clientTerminal` 覆盖(leader 多客户端) | **查证** | `agent_ops.rs` `resolve_client_io_caps` L3885–3900;`leader/protocol.rs` L158–162 注释 |
| `true` + gateway → 工具后端选 `AcpClient` | **查证** | 见 Q2 |
| `false` → 本地后端,子进程在 **agent 侧** | **查证** | 同 Q2 |

与 FS 对照:**terminal 没有 read∧write 双开式**;一个 bool。

### 3.4 生命周期与取消

**前台(典型 `AcpTerminalAdapter::run`)** — **查证** `adapter.rs` L185–251:

```text
terminal/create
  → wait_for_exit (带工具 timeout)
       超时 → terminal/kill
  → terminal/output
  → terminal/release
```

**后台** — **查证** `run_background` L254–301 + `exit_watcher.rs` L1–3,L70+:

```text
terminal/create
  → 异步 watch: wait_for_exit + 轮询 terminal/output
  → 完成后 release
```

**取消 / 会话清理** — agent 侧有 `kill_and_release_all_for_session`(`streaming_local_terminal.rs` L190);ACP 客户端侧取消时需对已发 `terminalId` 做 kill/release。Pillow 若实现,应绑定 run 取消与 Job Object 清理(本仓已有进程安全基线)。**实盘取消形态 → 未确定**(本单不探针)。

### 3.5 扩展方法(非标准 ACP 主路径)

上游另有 **agent 侧** `x.ai/terminal/*` 扩展(`extensions/terminal.rs` L185–348):`create`/`kill`/`output`/`wait_for_exit`/`release`/`background`/PTY 族。那是 shell 进程对**自己** registry 的扩展入口,不是“模型调原生工具时的客户端反向通道”。C4 宿主应盯 **标准** `terminal/*`(client methods),不要把 ext 与 client 方法混成一条。

---

## 4. Q2 — 真实 Grok 何时走 `terminal/*` 而非本地 shell?

### 4.1 强制判定式(对标写方向的 `use_acp_fs`)

**FS(已知):**

```text
use_acp_fs = client_fs_read && client_fs_write     # agent_ops.rs L3956
client_fs_capable = client_fs_read && client_fs_write  # 传入 spawn L4602
```

**Terminal(本单钉死):**

```text
# 1) 解析能力
terminal = meta.clientTerminal ?? init.client_capabilities.terminal
# agent_ops.rs resolve_client_io_caps L3889–3891

# 2) 传入 spawn(注意:FS 在这里做了 ∧,terminal 不做)
client_terminal_capable = client_terminal          # L4601
client_fs_capable       = client_fs_read && client_fs_write  # L4602

# 3) 工具 TerminalBackend 选择
select_terminal_backend_kind(
  is_subagent, has_parent_backend,
  client_terminal_capable, has_gateway, cursor_harness
)
# spawn.rs L2564–2579:
#   subagent+parent → ReuseParent
#   client_terminal_capable && has_gateway → AcpClient   ← 唯一 ACP 路径
#   else cursor_harness → LocalPersistent
#   else → LocalNonPersistent

# 4) AcpClient 装配
AcpTerminalAdapter::new(gateway, session_id)      # spawn.rs L697–701
```

单测钉死 gateway 必要性:`client_terminal_uses_acp_only_with_gateway` `spawn.rs` L2671–2678  
(`terminal=true, gateway=false` → 仍 LocalPersistent)。

另有旧式 `AsyncTerminalRunner` 分支:`client_terminal` → `AcpTerminalRunner` else 本地 `TerminalRunner`(`agent_ops.rs` L4005–4019)。主工具链用的是 **`TerminalBackend`** 选择(上表),不是这条 runner 单独决定 allowlist 工具。

### 4.2 模型侧前置条件

| 条件 | 无此条件时 | 等级 |
| --- | --- | --- |
| allowlist 含 shell 工具(`run_terminal_command` / `run_terminal_cmd` / bash 别名) | 模型无手 → **不会**发 `terminal/create` | **查证**(builder retain);是否幻觉乱试 **未确定**/需探针 |
| `terminal=true` + gateway | 有工具则走 **Local\*** 自起进程 | **查证** 代码路径 |
| `terminal=true` 但无 shell 工具 | 无 create(与“只开 writeTextFile 不放 write”同构) | **查证** 路径 |

工具输入:**只有** `command: String`(+ timeout/description/background 等),**无 argv 数组**。  
**查证** `grok_build/bash/mod.rs` `BashToolInput` L259–307;`opencode/bash` `BashInput` 同为 `command: String`。

### 4.3 🔴 半开陷阱(与写方向 LocalFs **同构**)

| 陷阱 | 组合 | 实际行为 | 宿主是否看见 |
| --- | --- | --- | --- |
| **LocalTerminal 半开**(进程主陷阱) | allowlist **有** shell 工具 + `clientCapabilities.terminal=false` | `LocalTerminalBackend` / 本地 shell **agent 侧 spawn** | **否** — 无 `terminal/create` |
| **空 cap 无工具** | terminal=true + allowlist **无** shell | 模型通常无工具可调 | 无 create(安全但无能力) |
| **无 gateway** | terminal=true 但 gateway 缺失 | 仍 Local(单测 L2677–2678) | 否 |
| 写方向对照 LocalFs | allowlist write + 未双开 FS | agent 直写盘 | 否 `fs/write_*` |

**没有** terminal 版的“双开半开”(不存在 terminalRead∧terminalWrite)。陷阱形状更简单:

```text
工具放行 ∧ capability 关  →  代理自己动手、全程无声
```

这与写方向 `acp-fs-localfs-trap` **同构**,只是单 flag。  
**上线任何“放行 shell 工具”的 profile 之前,必须先 `terminal=true` 且客户端实现 `terminal/*` 治理入口;否则是看起来有 allowlist、实际无治理的版本。**

### 4.4 实盘层

| 问题 | 等级 |
| --- | --- |
| 代码路径:上述选择逻辑 | **查证** |
| 真 Grok 在 terminal=true + allowlist shell 下 **100%** 发 `terminal/create` | **未确定** → 需探针 `terminal-create-shape` / `terminal-acp-governed` |
| 真 Grok 在 terminal=false + allowlist shell 下 **是否真落子进程** | **未确定**(代码说会;需探针 `terminal-local-trap` 用副作用令牌,如写探针沙箱文件) |

---

## 5. Q3 — `terminal/*` 能否承载「目录表 + 结构化 argv」?

### 5.1 协议层

| 能力 | 结论 | 等级 |
| --- | --- | --- |
| create 可带 `command`+`args[]` | 可以 | **查证** schema |
| 空 args = shell;非空 args = 直接 exec(上游本地实现) | 可以 | **查证** streaming_local L280–286 |

### 5.2 真实 Grok 发射层

| 事实 | 等级 |
| --- | --- |
| 工具 schema 只有 command 字符串 | **查证** BashToolInput |
| 工具描述**鼓励** `&&` 串联、`;` 顺序、并行多 call | **查证** opencode bash DESCRIPTION L92–96 |
| `AcpTerminalAdapter` 恒 `args=[]`,Unix 再 `shell -lc` 包一层 | **查证** adapter L114–127, L169 |
| 上游**没有**“结构化命令 / 程序目录表”产品概念 | **查证**(工具与 TerminalRunRequest.command:String,`computer/types.rs` L80–81) |

### 5.3 客户端侧还原/校验可行性

| 策略 | 能否可靠 | 丢失/风险 |
| --- | --- | --- |
| 要求 `args` 非空才执行 | 对真 Grok **恒失败**(永远空 args) | 能力面归零 |
| shlex/PowerShell 拆 `command` → program+argv | **不可靠** | 引号、转义、Windows shell 方言、环境差异;误伤合法参数 |
| 拒绝含 `&&` `\|` `;` `` ` `` `$` 等元字符 | 可做 fail-closed | **误伤**模型训练出的正常串联;且描述仍教模型用 `&&` |
| 拆完后 basename ∈ 目录表 | 可做 | 全路径/`.exe`/PATH 解析与 C4 `resolve_program_executable` 可能不一致 |
| 整串 shell 作为批准对象 | 能跑 | **直接放弃**结构化 argv + 目录表作为提案身份 |

### 5.4 若硬接,会丢哪些 C4 不变量?

| C4 不变量 | 在真实 Grok→`terminal/*` 路径上 |
| --- | --- |
| 程序目录表(模型只提 catalog label) | **丢** — 模型提自由 shell 文本;目录表最多变成客户端**事后过滤** |
| 结构化 argv(提案身份 = program+argv digest) | **丢** — 线协议上身份是 shell 串(或有损 parse 结果) |
| 禁串联/管道/重定向(一次批准 = 一个 exec) | **默认丢** — 模型被鼓励串联;客户端可拒绝但能力面变窄 |
| 提案→批准→宿主执行→收据 | **可保留** — 挂在 `terminal/create` 入口(与写挂 `fs/write_text_file` 同构) |
| 项目隔离 / 清洁 env / Job 清理 | **可保留** — 宿主执行器已有 |

**结论(Q3):**  
- **无损承载:不能。** 不是“客户端再努力一点就能无损”,而是**生产发射层从不产生结构化 argv**。  
- **有损承载:能** — 在 create 上 fail-closed 解析/拒绝,把残余映射进现有 `process.run@1` 网关;治理闭环可立,表示层不变量必让步。  
- **标 未确定 的只有**:真模型在严格拒绝策略下是否仍能完成有用任务(需探针,不是代码能定)。

---

## 6. Q4 — 有没有比 `terminal/*` 更贴近的路?

| 候选 | 结论 | 证据 | 等级 |
| --- | --- | --- | --- |
| 原生 shell 工具 + “客户端拦截”但不走 ACP | **不存在**可靠拦截 | cap 关 → Local 自起;cap 开 → 就是 `terminal/*` | **查证** |
| 其它 ACP 反向 RPC 承载执行 | **无**标准方法族 | client methods: fs + terminal + permission + session/update(+ ext) | **查证** message.rs L178–191 |
| 上游“结构化命令”概念 | **无** | BashToolInput / TerminalRunRequest 均为 command 字符串 | **查证** |
| `pillow/process_tool` | 宿主已实现;真 Grok **不调用** | provider_processes + 既有路线报告 | **查证**(路径);实盘“从不尝试” **未确定**/历史一致 |
| MCP 进程工具 | 技术可通;要开 `use_tool` → ambient 洞 | 既有 MCP 探针 | **查证** 边界风险 |
| SDK 反向 MCP `x.ai/mcp/sdk_call` | 仍要 `use_tool` | 既有 tool-route | **查证** |
| `session/request_permission` 当执行通道 | 权限 UX,不是 exec 载荷 | 不承载 program/argv | **查证**(角色) |
| `ask_user_question` / ext | 访谈 UI,非执行 | 既有 tool-route Q3 | **查证** |

**更贴近 C4 表示层的协议面:不存在于上游。**  
**更贴近 C4 治理闭环的管道:仍是 `terminal/*` create 入口**(与写的 `fs/write_text_file` 同构),但表示层必须有损或改产品模型。

---

## 7. Q5 — 候选路线(边界不倒退 + 不变量不丢优先)

### 排序原则

1. 不重开 MCP 元工具 / ambient  
2. 不制造半开陷阱(工具开、cap 关)  
3. 尽量不丢 C4 不变量  
4. 真 Grok 可达性  

### 路线对照

| 序 | 路线 | 上游依据 | Pillow 粗改 | 探针? | 边界(MCP) | C4 不变量 | 推荐 |
| --- | --- | --- | --- | --- | --- | --- | --- |
| **1** | **P0 承认无损不可行,进程对真 Grok 暂缓;保持 terminal=false 且 allowlist 永不放 shell 工具** | 本报告 Q2–Q3;今日 `methods.rs` terminal=false | 产品决策;文档/路线图写明;防回归测试:profile 不得含 run_terminal* | 可选 `terminal-must-stay-closed` | **不倒退** | **不丢**(因不接真 Grok 进程) | **若坚持 C4 字面:首选** |
| **2** | **P1 有损 ACP:`terminal=true` + allowlist 最小 shell 工具 + create 入口严格降维/拒绝 → 现有 process 网关** | adapter 恒 shell 串;select L2573–2574;schema 有 args 但 Grok 不用 | initialize terminal true;实现 terminal/*;create→parse/reject→seal/reserve/approve/exec;profile 放行 `run_terminal_command`(名以实盘为准) | **能** §8 | **不因 MCP 倒退**;须防 LocalTerminal 半开 | **丢** 模型侧目录表+结构化 argv 作为提案源;治理闭环可保留 | **若必须给真 Grok 进程手:工程首选** |
| **3** | **P2 改产品模型:批准对象改为 shell 串(+ 可选粗策略)** | 工具与描述即 shell 世界 | 新 proposal 身份;弱化/替换 PROCESS_CATALOG 语义;UI/审计按 shell 展示 | **能** | 同 P1 防半开 | **明确放弃** C4.0 结构化模型 | 仅当产品接受“IDE shell 治理”而非“目录表 exec” |
| **4** | **P3 重开 MCP 挂治理 process 工具** | use_tool 唯一 MCP 门 | denylist 去掉 use_tool;mcpServers 挂治理 server | **能** | **倒退**(ambient) | 表示层可自定 schema 贴近 C4 | **不推荐**;仅 P1 实盘失败且能隔离 ambient 时 |

### 明确否决 / 危险

| 组合 | 原因 |
| --- | --- |
| allowlist 放 shell + `terminal=false` | 🔴 **半开陷阱** — 代理本地起进程 |
| 只开 `terminal=true` 不放 shell 工具 | 无手;与只开 writeTextFile 不放 write 同构 |
| 只实现 `pillow/process_tool` 期望真 Grok | 真 Grok 不发 |
| 靠 denylist 枚举更多 shell 别名 | 历史已证 denylist 不足;应用 allowlist |
| 重开 use_tool “只给自己 MCP” | 无会话层 client-only;见 tool-route Q1 |

### 推荐(给总控)

1. **默认产品立场:P0** — 在未接受“有损表示”或“改 C4 模型”之前,**不要**给真 Grok 进程手。  
2. **若 Stage 要求真 Grok 可跑构建命令:P1** — 与写同构的 ACP 反向通道,但在 `terminal/create` **fail-closed** 降维;把“丢失的不变量”写进设计变更,而不是假装仍是纯 `process.run@1` 提案源。  
3. **不要**为了进程单独重开 MCP(P3)。  
4. **任何实现 PR 的第一测试必须是半开陷阱探针**(§8 `terminal-local-trap`),对标写方向的 `acp-fs-localfs-trap`。

---

## 8. 探针变体建议(`PILLOW_MCP_PROBE_ONLY=<id>`)

现有脚本:`scripts/probe-mcp-tool-exposure.mjs`(已支持 `PILLOW_MCP_PROBE_ONLY` 过滤)。本单 **不改脚本**,只给规格。  
判定:**读盘/RPC 日志,不信模型叙述;方法名出现 ≠ 成功执行。**

| id | 目的 | profile / caps 要点 | 通过 / 失败判据 |
| --- | --- | --- | --- |
| `terminal-local-trap` | 🔴 半开陷阱 | allowlist:`ask_user_question`+`run_terminal_command`(及实盘别名);**仍 denylist 元工具**;`terminal:false`;prompt 令在沙箱写副作用文件(如 `echo TOKEN>probe_out`) | 若沙箱出现 TOKEN 且 **无** `terminal/create` → **陷阱成立,禁止上线该组合** |
| `terminal-acp-governed` | P1 主路径 | 同上 allowlist;`terminal:true`;client **实现** create/output/wait/kill/release,create 可只记日志或走沙箱 exec | 见 `terminal/create`;**无**沙箱落盘除非 create 处理程序写入;记录 command/args 原样 |
| `terminal-create-shape` | Q3 形状 | 同 governed;create handler dump 原始 JSON | 断言生产是否 `args==[]`、command 是否含 `&&`/`\|`(样本统计,非法断定“永不”) |
| `terminal-no-shell-tool` | cap  alone 不够 | allowlist **无** shell;`terminal:true`;prompt 强令跑命令 | **不得**出现 create;沙箱无副作用 |
| `terminal-strict-reject` | P1 降维策略 | governed + client **拒绝**含 shell 元字符 / 非目录表 | 模型是否改写为简单单命令、或任务失败(行为 **未确定**,收集样本) |
| `terminal-cancel-kill` | 生命周期 | governed + 中途 cancel | client 是否收到 kill/release;无孤儿(需进程快照,注意安全清理) |

运行例:

```bash
PILLOW_MCP_PROBE=1 PILLOW_MCP_PROBE_ONLY=terminal-local-trap node scripts/probe-mcp-tool-exposure.mjs
```

工具名注意:上游同时存在 `run_terminal_command` 与 `run_terminal_cmd` 命名(builder 测试 Bash→`run_terminal_command`)。探针 allowlist **应同时覆盖实盘出现的短名**,以一次 `no-profile` 会话列出的工具名为准(**未确定** 当前 binary 暴露哪一个 — 探针先记名再钉 allowlist)。

---

## 9. 🔴 半开陷阱专节(实现前必读)

### 9.1 写方向已证实的形状

```text
allowlist 放行 write  ∧  writeTextFile=false
  → use_acp_fs=false → LocalFs → 代理直写盘
  → 宿主 fs/write_text_file 从不出现
  → “治理写”UI/代码可以存在,但真代理从不走进去
```

实盘:`acp-fs-localfs-trap` vs `acp-fs-write-governed`  
(`2026-08-10-mcp-tool-exposure-probe.md` §11)。

### 9.2 进程方向的同构陷阱

```text
allowlist 放行 run_terminal_*  ∧  clientCapabilities.terminal=false
  → select_terminal_backend_kind → Local*
  → LocalTerminalBackend 在 agent 进程树内 shell -c / pwsh -Command
  → 宿主 terminal/create 从不出现
  → “治理进程”账本可以存在,但真代理从不走进去
```

强制点:

| 步骤 | file:line(上游 pin `75e73f3`) |
| --- | --- |
| 能力解析 | `agent_ops.rs` L3885–3900 |
| FS 双开 vs terminal 单开传入 | `agent_ops.rs` L4601–4602 |
| 后端选择 | `spawn.rs` L2564–2579, 单测 L2671–2689 |
| ACP 适配器恒 shell 串 | `terminal/adapter.rs` L114–127, L161–173 |
| 本地 shell spawn | `computer/local/terminal.rs` `spawn_shell_command` L3147+ |

### 9.3 其它相关陷阱候选

| ID | 组合 | 风险 |
| --- | --- | --- |
| T-term-1 | shell 工具开 + terminal 关 | **主陷阱**(上表) |
| T-term-2 | terminal 开 + shell 工具开 + client **未实现** terminal/* | agent 发 create → 方法失败/超时;模型可能重试或改行为 — 失败形态 **未确定**;不得静默当成功 |
| T-term-3 | terminal 开 + gateway 丢 | 回落 Local(单测) — 等同 T-term-1 |
| T-term-4 | 只实现 create 就“放行执行”,wait/kill/release 不做 | 取消/泄漏;破坏进程安全基线 |
| T-term-5 | create 上宽松 exec shell 串,却对 UI 宣称“目录表 argv” | **语义造假** — 比半开更糟(审计谎言) |
| T-term-6 | 为进程重开 `use_tool` | ambient MCP 回归(边界倒退) |
| T-fs-term-mix | 写已双开治理,进程半开 | 代理用 shell 改文件绕过写治理 — **交叉洞** |

### 9.4 实现门禁(建议总控写入验收)

1. **禁止** merge:profile 含 shell 工具而 `initialize.terminal!=true`。  
2. **禁止** merge:terminal=true 但 client 未注册全部 `terminal/*` 五法。  
3. **必过** 探针 `terminal-local-trap`(陷阱必须可复现为失败门)。  
4. **必过** 探针 `terminal-acp-governed`(主路径必须见 create)。  
5. 文档/UI 不得在 P1 有损模式下声称“模型提交结构化 argv”,应写“宿主从 shell 串降维/拒绝后的治理执行”。

---

## 10. 证据索引(复审)

| 声明 | 等级 | 位置 |
| --- | --- | --- |
| ACP terminal 五方法名 | **查证** | schema client.rs L2290–2298 |
| CreateTerminalRequest: command+args | **查证** | schema L1134–1143 |
| ClientCapabilities.terminal = all terminal/* | **查证** | schema L1737–1741 |
| 能力解析 meta 覆盖 | **查证** | agent_ops.rs L3885–3900 |
| use_acp_fs 双开 | **查证** | agent_ops.rs L3956 |
| terminal 单开传入 spawn | **查证** | agent_ops.rs L4601–4602 |
| select_terminal_backend_kind | **查证** | spawn.rs L2564–2579 |
| AcpTerminalAdapter args 恒空 + shell wrap | **查证** | adapter.rs L114–127, L169, L186–187 |
| 本地空 args→shell / 非空→argv | **查证** | streaming_local_terminal.rs L280–286 |
| BashToolInput.command:String | **查证** | grok_build/bash/mod.rs L259–307 |
| 工具描述鼓励 && | **查证** | opencode/bash DESCRIPTION L92–96 |
| TerminalRunRequest.command:String | **查证** | computer/types.rs L80–81 |
| Pillow PROCESS_CATALOG / seal | **查证** | tool-executor process.rs L40–41, L554–586 |
| Pillow initialize terminal false | **查证** | adapter methods.rs L53–59 |
| 写方向双开实盘 | **查证(实盘)** | mcp-tool-exposure-probe.md §11 |
| 真 Grok 发 create 的比例/命令形状分布 | **未确定** | 需 §8 探针 |
| 严格拒绝策略下任务是否仍可完成 | **未确定** | 需 `terminal-strict-reject` |

---

## 11. 对总控的直接答复

**问:进程方向在 ACP 上能否无损承载 Pillow C4?**  
**答:不能。** 强制点在工具 schema 与 `AcpTerminalAdapter` 发射层,不是 Pillow 客户端“还没写好”。

**问:那还有没有工程上可做的路?**  
**答:有,但是有损(P1)或改产品模型(P2);无损只剩不对真 Grok 开放进程(P0)。**

**问:会不会再踩“看起来有治理、实际没有”?**  
**答:会,形态已写明(§9 LocalTerminal 半开)。实现前用 `terminal-local-trap` 门禁,不要只靠读代码相信。**

---

GOVERNED-PROCESS-ROUTE-COMPLETE
