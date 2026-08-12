# Pillow — 权限模型与设置页信息架构调研

**Date:** 2026-08-12  
**Nature:** 只读调研。唯一写入：本报告。未改 product 源码/测试/脚本/配置。未跑门/E2E。未读取任何真实认证文件。  
**Worker:** grok4 · **总控:** Claude  
**完成信号:** `PERMISSION-UX-COMPLETE`

## 调研边界与证据口径

| 来源 | 定位 | 备注 |
| --- | --- | --- |
| OpenCode 本机只读 | `<workspace>/_ref/opencode` @ `d470434` | MIT；未改 |
| Grok Build 本机 | `~/.grok/README.md` + 已装 `grok --help` | 未读 `auth.json` 等认证文件 |
| Grok 上游既有调研 | `docs/research/2026-08-10-grokbuild-permission-reuse.md` | 上游 pin 记录在该文；本单未重新 clone |
| Codex | 官方文档 + 公开仓库 schema/源码（`gh api`） | 文档：`developers.openai.com/codex/*` |
| Pillow | 本工作树 `file:line` | 以当前树为准 |

证据优先级：本仓 `file:line` → 本机参考源码 `file:line` → 官方文档 URL。拿不到证据写 **未确定**，不推测。

---

## Q1. 三个产品的权限模型各自长什么样？

### 结论（先）

| 产品 | 模式/等级 | 规则语法 | 作用域 | 优先级/冲突 | 规则存哪 |
| --- | --- | --- | --- | --- | --- |
| **OpenCode** | 规则动作 `allow` / `ask` / `deny`；CLI/TUI **auto** 把非 deny 的 ask 自动放行 | 工具键 + 可选 glob 对象；另有 `external_directory`、`doom_loop` | 全局 config、项目 config、agent 级、会话 always-grant | **最后一条匹配胜**；agent 覆盖全局；managed 最高；deny 在 auto 下仍生效 | `opencode.json` 等 config 层 + 会话内 `approved` 内存规则 |
| **Grok Build** | CLI：`default` / `acceptEdits` / `auto` / `dontAsk` / `bypassPermissions` / `plan`；UI 软默认：`always-approve` / `auto` / `ask`；另有 **sandbox 正交轴** | `ToolPrefix(glob)`：`Bash`/`Edit`/`Write`/`Read`/`Grep`/`WebFetch`/`MCPTool` | CLI 一次性规则；`~/.grok/config.toml`；受信任项目 `.grok/config.toml`；会话 remembered grant；agent profile `permissionMode` | **deny > ask > allow**（与顺序无关）；yolo/always-approve 仍受 deny（及部分 shell ask）约束；CLI flag 可压 config yolo | config.toml、CLI flags、agent profile、会话 grant；可选 OS sandbox 配置 |
| **Codex** | **两轴独立**：`approval_policy` = `untrusted` \| `on-request` \| `never` \| granular；`sandbox_mode` = `read-only` \| `workspace-write` \| `danger-full-access` | 主路径不是 `Bash(glob)` 用户 DSL；有 sandbox 可写根 / network policy / 企业 permission profiles / execpolicy 段规则（源码层） | `~/.codex/config.toml`、可信项目 `.codex/config.toml`、`--profile`、CLI/`-c`；会话/线程可同步 settings | CLI/项目/profile/用户/系统；企业 `requirements.toml` 可禁危险组合 | 上述 config 层 + thread settings；OS sandbox 运行时 |

### 1.1 OpenCode — 证据

**模式/动作**

- 规则动作三态：`allow` / `ask` / `deny`。  
  证据：`<workspace>/_ref/opencode/packages/web/src/content/docs/permissions.mdx` L14–18。
- **Auto 模式**：`opencode --auto` — 自动批准**未被显式 deny** 的请求。  
  证据：同文件 L22–36；`packages/web/src/content/docs/cli.mdx` L40。

**规则语法**

- 工具名作 key；值可为动作字符串，或 `{ pattern: action }` 对象。  
  证据：`permissions.mdx` L44–89。
- 通配：`*` 任意序列，`?` 单字符。  
  证据：`permissions.mdx` L93–99。
- 特殊键：`external_directory`（工作区外路径）、`doom_loop`（同工具同输入连打 3 次）。  
  证据：`permissions.mdx` L109–164。
- 可用工具键：`read`/`edit`/`glob`/`grep`/`bash`/`task`/`skill`/`lsp`/`question`/`webfetch`/`websearch`/…  
  证据：`permissions.mdx` L148–164。

**作用域**

- Config 合并链：remote → global `~/.config/opencode/opencode.json` → `OPENCODE_CONFIG` → 项目 `opencode.json` → `.opencode` → inline → managed。  
  证据：`packages/web/src/content/docs/config.mdx` L42–53。
- Agent 级 `permission` 与全局合并，**agent 优先**。  
  证据：`permissions.mdx` L203–205。
- 用户在 prompt 上选 `always`：把建议 pattern 写入**当前会话** `approved` 列表（进程内，非永久 config）。  
  证据：`packages/opencode/src/permission/index.ts` L142–151；`permissions.mdx` L191–199。

**优先级**

- 求值：`findLast` 匹配 permission+pattern → **最后一条匹配规则胜**；无匹配默认 `ask`。  
  证据：`packages/opencode/src/permission/index.ts` L28–37。
- ask 流程：任一 pattern `deny` → 拒绝；任一非 allow → 需要 ask；全 allow 才静默过。  
  证据：同文件 L67–84。
- Auto **不覆盖** 显式 `deny`。  
  证据：`permissions.mdx` L36–37。

**默认**

- 多数工具默认 `allow`；`doom_loop` 与 `external_directory` 默认 `ask`；`.env` 读默认 deny。  
  证据：`permissions.mdx` L168–187。

### 1.2 Grok Build — 证据

**模式/等级**

- 已装 CLI：`--permission-mode` 可选  
  `default | acceptEdits | auto | dontAsk | bypassPermissions | plan`；另有 `--always-approve`。  
  证据：本机 `grok --help`（2026-08-12 采集）。
- UI/产品「全开」：`--always-approve`、`Ctrl+O`、`/always-approve`（别名 `/yolo`）。  
  证据：本机 `~/.grok/README.md` Interactive TUI 段（约 L446–502 一带）。
- 上游源码层（既有调研）：`PermissionMode` 与 UI `always-approve`/`auto`/`ask` 两套词汇叠用；`DefaultModeEffects` 对各 mode 合成 allow 规则或 prompt policy。  
  证据：`docs/research/2026-08-10-grokbuild-permission-reuse.md` §4.3（引上游 `rules.rs` / `permissions.rs`）。

**规则语法**

- `ToolPrefix(glob_pattern)`：`Bash` / `Edit` / `Write` / `Read` / `Grep` / `WebFetch` / `MCPTool`；裸前缀匹配该类全部；支持 `*` / `**`；Claude Code 式 `Bash(cmd:*)`。  
  证据：本机 README `#### Permission Rules (--allow / --deny)` 段（约 L642–677）。
- 规则**不**移除工具；`--disallowed-tools` 才从 toolset 摘除。  
  证据：同 README 段首句。

**作用域**

- CLI 一次性 `--allow` / `--deny`（可重复）。  
  证据：README 同上。
- 全局 `~/.grok/config.toml`；项目 `.grok/config.toml`（文档表：项目侧写 MCP 等；权限规则加载细节见既有调研：受信任项目才进）。  
  证据：README File Locations；`2026-08-10-grokbuild-permission-reuse.md` §5.2。
- 进程 home 可用 `GROK_HOME` 覆盖（默认 `~/.grok`）。  
  证据：README Environment Variables 表。

**优先级**

- Deny **优先于** allow；可与 always-approve 并存时 deny 仍挡。  
  证据：README「Deny rules take precedence…」；既有调研 §4.1 / §4.3。
- yolo 解析：CLI `--permission-mode` 优先；否则 `--always-approve`；否则 config；managed pin 可强制关。  
  证据：`2026-08-10-grokbuild-permission-reuse.md` §5.2。

**Sandbox（与审批正交）**

- 配置/CLI：`--sandbox` / `GROK_SANDBOX`；profile：`off`（默认）/`workspace`/`read-only`/`strict`。  
  证据：README `## Sandbox`；`grok --help`。
- 文档明确：sandbox 用 OS 原语限制 FS/网络，**与 permission 规则不是同一开关**。  
  证据：README Sandbox + 既有调研 §4.2 表。

### 1.3 Codex — 证据

**模式/等级（两轴）**

- **Approval policy**：`untrusted` | `on-request` | `never` | granular 对象。  
  证据：官方 Config basics；schema  
  `codex-rs/app-server-protocol/schema/typescript/v2/AskForApproval.ts`  
  （`"untrusted" | "on-request" | { granular: … } | "never"`）。
- **Sandbox mode**：`read-only` | `workspace-write` | `danger-full-access`。  
  证据：同上 Config basics；schema `SandboxMode.ts`。
- 产品叙事：**Sandbox = 技术上能碰什么；Approval = 何时必须问人**。  
  证据：https://developers.openai.com/codex/agent-approvals-security （§ Sandbox and approvals）。

**规则语法**

- 用户主配置面是 **policy 枚举 + sandbox 可写根/网络**，不是 OpenCode/Grok 式 `Bash(git *)` 设置 UI。  
  证据：Config basics 常用键段；Agent approvals 文档。
- 源码层对 patch 安全：可写根约束 + approval policy；写出 workspace 外可 AskUser / Reject。  
  证据：`codex-rs/core/src/safety.rs` `assess_patch_safety`（`PATCH_REJECTED_OUTSIDE_PROJECT_REASON`）。
- 企业/进阶：permission profiles、`network_proxy` domain allow/deny、`requirements.toml`。  
  证据：Agent approvals 文档；Config reference（search snippet）。
- Shell 段级 execpolicy（既有业界调研）：token 前缀规则；与 Grok 同形拆段思想。  
  证据：`docs/research/2026-08-11-shell-approval-state-of-the-art.md` §3.1 Codex 行（上游 pin 记在该文）。

**作用域与优先级**

- 解析顺序（高→低）：CLI/`-c` → 可信项目 `.codex/config.toml`（近者胜）→ `--profile` → `~/.codex/config.toml` → 系统 → 内置默认。  
  证据：https://developers.openai.com/codex/config-basic （§ Configuration precedence）。
- 不可信项目：**跳过**项目层 `.codex/`。  
  证据：同上。
- 企业 `requirements.toml` 可限制 `approval_policy`/`sandbox_mode` 等。  
  证据：同上 managed 段。

**「全开」组合**

- 关审批：`--ask-for-approval never` / `approval_policy = "never"`。  
- 同时放宽目录/网络：需 `sandbox_mode = "danger-full-access"` 或  
  `--dangerously-bypass-approvals-and-sandbox`。  
  证据：Agent approvals § Run without approval prompts。

### Q1 对比表（3 × 6）

| 维度 | OpenCode | Grok Build | Codex |
| --- | --- | --- | --- |
| **模式/等级** | 规则三态 + session auto | 6 个 permission-mode + always-approve/yolo；sandbox 另轴 | approval_policy × sandbox_mode（可 granular） |
| **规则语法** | 工具键 + glob 对象；`external_directory` 独立键 | `Bash(…)`/`Edit(…)`/… 前缀 DSL | 主配置枚举 + writable roots/network；源码有 patch/execpolicy |
| **作用域** | global/project/agent/session always | CLI、global/project config、profile、session grant | CLI、profile、user、trusted project、thread |
| **冲突解决** | last-match-wins；deny 仍挡 auto | deny>ask>allow；CLI 压 config yolo | 配置层高者胜；sandbox 与 approval 独立求值 |
| **存哪里** | `opencode.json` 链 + 会话内存 | `~/.grok` + flags + grants | `~/.codex` + `.codex` + thread |
| **目录边界** | `external_directory` 默认 ask | 规则可 deny 路径；sandbox 可限制写根 | sandbox writable roots；出界 ask/reject |

---

## Q2. 它们怎么表达「这次全开」？是否解除目录边界？

### 结论（先）

| 产品 | 「这次全开」是什么 | 是否解除目录边界 | 自动批准 vs 可访问范围是否分轴 |
| --- | --- | --- | --- |
| **OpenCode** | 模式：CLI/TUI **auto**；或 config `permission: "allow"`；或会话 **always** 对 pattern 记 allow | **否**。auto 只把 ask→allow；**`external_directory` 仍是独立权限键**（默认 ask）。全开不等于默认允许项目外路径 | **是**：工具规则轴 + `external_directory` 轴 |
| **Grok Build** | 标志/模式：`--always-approve` / `/yolo` / `bypassPermissions` / UI always-approve | **默认不靠「全开」解除边界**。always-approve 跳过工具确认；**OS sandbox 是另一开关**（默认 off）。路径仍可用 `deny Edit(/etc/**)` 等规则收紧。是否在无 sandbox 时「全盘可写」取决于原生工具与 OS，不是 always-approve 单独声明「打开项目外」 | **是（产品层）**：permission/yolo 轴 vs sandbox 轴；规则第三层 |
| **Codex** | 关审批：`never`；全权限常需再配 sandbox | **明确分轴**。`never` **不**自动等于 `danger-full-access`。文档：`never` 可与任意 sandbox 组合；要读写/网络无界需 danger-full-access 或 danger bypass 旗标 | **是（最清晰）**：approval vs sandbox |

### 证据与含义

**OpenCode**

- Auto 定义：「Auto-approve permissions that are not explicitly denied」。  
  证据：`cli.mdx` L40；`permissions.mdx` L22–37。
- 工作区外：工具碰到 CWD 外路径触发 `external_directory`；home 展开**不**把路径算进 workspace。  
  证据：`permissions.mdx` L109–113。
- 用户若要「项目外也不问」，需**单独**给 `external_directory` 配 allow（例如 `~/projects/personal/**`），不是开 auto 就够。  
  证据：`permissions.mdx` L115–141。

**Grok Build**

- always-approve：自动批准工具执行、无需确认。  
  证据：README flag 表 + slash 表。
- Sandbox 默认 off；开 sandbox 后写范围由 profile 定（如 workspace 只写 CWD+/tmp）。  
  证据：README Sandbox 表。
- Deny 规则覆盖 always-approve 通行（既有调研 + README deny 优先）。  
  证据：`2026-08-10-grokbuild-permission-reuse.md` §4.1；README Permission Rules。
- **未在本机 README 找到**「always-approve 自动允许项目目录外写」的正面声明 → 对「全开 = 目录外也不问」：**市面产品并未把二者绑成一个旋钮**；Grok 是「少问人」+ 可选「OS 收紧」+ 规则 deny。

**Codex**

- 文档原话结构：sandbox 与 approval 两层一起工作；Auto preset = `workspace-write` + `on-request`（工作区内自动，出界/网络再问）。  
  证据：Agent approvals § Sandbox and approvals / Defaults。
- 「Run without approval prompts」：`never` 与**所有** sandbox 模式兼容；需要无沙箱全开才用 danger-full-access / dangerously-bypass。  
  证据：同页 § Run without approval prompts。
- 源码：patch 若不可约束到可写根，在 `Never` 下可 **Reject**（拒绝写项目外），而不是静默放行。  
  证据：`safety.rs` `assess_patch_safety` + `PATCH_REJECTED_OUTSIDE_PROJECT_REASON`。

### 对 Pillow 用户诉求的直接含义

用户要：**全开时项目目录外的文件也不用问**。

参考产品共识：

1. **「自动批准」≠「可访问范围」** — Codex 写得最硬；OpenCode 用 `external_directory` 单独建模；Grok 用 sandbox/规则拆开。  
2. 若 Pillow 只有 `bypass` 一个旋钮且**仍硬钉项目根**，用户会感觉「全开了却写不出去」——这与 Codex `never` + 非 danger sandbox 的体验同类，**不是 bug 叙事，是轴没拆开/没说清**。  
3. 若产品要真支持「全开含目录外」，需要**第二轴**（可写根 / external allow / danger 级），而不是把 `bypass` 语义偷偷扩成无界写。

---

## Q3. 设置页怎么分区？

### 结论（先）

| 产品 | 设置分区（用户可见） | 权限落在哪 |
| --- | --- | --- |
| **OpenCode Desktop** | 竖向 Tab：**Desktop** → General、Shortcuts；**Server** → Servers、Providers、Models。General 内再分 Appearance / Sounds / 语言 / **auto-accept permissions 开关** / Shell / 推理摘要等 | **General 里一行开关**；细规则主要在 config/agent，不在同一大表单堆路径 |
| **Grok** | **无**与 Pillow 同形的单页 Settings 堆砌；配置按 **config.toml 主题段** + CLI + TUI slash。段：`[cli]`/`[models]`/`[ui]`/`[features]`/`[session]`/`[tools]`/`[toolset.*]`/telemetry/LSP/MCP/agents/sandbox | 权限：flag/slash/config features；sandbox 独立章 |
| **Codex** | IDE：Settings → **Open config.toml**（共享 CLI 层）；TUI/线程可同步 approval、permissions profile、model 等。文档按 **model / approval / sandbox / windows / web_search / features / MCP** 讲 | approval 与 sandbox **并列顶级键**，不与主题混排 |
| **Pillow 现状** | **单面板纵向堆叠**：权限模式、主题、适配器、Grok 可执行路径、Grok profile、保存区；其下再挂 Librarian 体检、运行时槽位、权限规则 | 全部挤在 `SettingsPanel` 一个 dialog |

### 3.1 OpenCode — 证据

Tab 结构：

```text
Desktop:  general | shortcuts
Server:   servers | providers | models
```

证据：`<workspace>/_ref/opencode/packages/app/src/components/settings-v2/dialog-settings-v2.tsx` L53–110。

General 内含：

- Language  
- **Permission auto-accept**（Switch）  
- Shell  
- Reasoning summaries / shell tool parts  
- Appearance（scheme、theme、字体）  
- Sounds（agent / permissions / errors）  

证据：`settings-v2/general.tsx` L72–88、L122–176、L327–346 一带。

### 3.2 Grok — 证据

- 用户文档按功能章：Authentication、Configuration 子段、Agent Profiles、MCP、Sandbox、File Locations…  
  证据：`~/.grok/README.md` Contents + Configuration 示例（`[cli]`/`[models]`/`[ui]`/`[features]`…）。
- TUI 权限交互：`Ctrl+O`、`/always-approve`，不是塞进「主题旁边的下拉」。  
  证据：README shortcuts/slash 表。

### 3.3 Codex — 证据

- IDE：齿轮 → Codex Settings → Open config.toml。  
  证据：Config basics。  
- 常用配置在文档中分条：model、approval_policy、sandbox_mode、windows.sandbox、web_search、features、MCP。  
  证据：Config basics § Common configuration options。  
- 线程可更新：`approval_policy`、`permissions`（profile）、model、cwd…  
  证据：`codex-rs/tui/src/app/thread_settings.rs` `ThreadSettingsUpdateParams` 字段使用。

### 3.4 Pillow 现状 — 证据

`apps/desktop/src/features/settings/SettingsPanel.tsx`：

| 区域 | 内容 | 行号（约） |
| --- | --- | --- |
| 头 + 登录说明 | Grok CLI 登录提示 | L278–299 |
| 作用域切换 | 全局 / 当前项目 | L301–328 |
| 主表单同一列 | **权限模式**、**主题**、**运行时适配器**、**Grok 可执行文件路径**、**Grok profile 路径**、保存 | L333–562 |
| 表单外同 dialog | `LibrarianMonitorSection`、`RuntimeSlotsSection`、`PermissionRulesSection` | L565–579 |

权限模式选项文案：`default` / `accept_edits` / `bypass`（「全开：写入与命令都自动」）。  
证据：L78–82、L334–375。  
权限模式 **仅全局**（项目 scope 禁用）。  
证据：L233–237、L345–365。

### 3.5 分组对照表

| 关注点 | OpenCode | Grok | Codex | Pillow 今 |
| --- | --- | --- | --- | --- |
| 外观/主题 | General → Appearance | `[ui]` / flag | 非主安全页 | 与权限同表单 |
| 权限模式/自动批准 | General 一开关 | slash/flag/config | `approval_policy` | 同表单顶部 |
| 目录/沙箱 | config `external_directory` / 无 OS sandbox UI 在 settings-v2 | Sandbox 专章 | `sandbox_mode` 并列 | **无独立 UI 轴**（写死项目根） |
| 细规则 | config/agent | allow/deny + 本仓规则 UI | execpolicy/企业 profile | 同 dialog 底 `PermissionRulesSection` |
| 模型/Provider | 独立 Servers/Providers/Models tab | models/MCP 段 | config + settings | 槽位段（混在设置） |
| Agent 二进制/home | 不进 General 主路径 | 安装 PATH + `GROK_HOME` | 安装 PATH + `~/.codex` | **主表单两路径字段** |
| 槽位/多账号 | 无 Pillow 模型 | 无 | 无 | RuntimeSlotsSection |

### 3.6 针对 Pillow 的建议分区方案（只建议，不实现）

对齐参考产品的**功能邻接**，建议把当前单页拆成清晰导航（名称可再定）：

| 分区 | 放什么 | 不放什么 |
| --- | --- | --- |
| **1. 外观** | 主题（全局） | 权限、路径、槽位 |
| **2. 安全与权限** | 权限模式（全局）；权限规则（global allow/deny + project deny）；**明确文案：模式只改「问不问」，不改「项目根边界」**；若未来做第二轴，可写根/external 放这里 | Grok 路径、主题 |
| **3. 运行时与槽位** | 适配器选择；**项目槽位**（auth/role/可执行检测）；治理/非治理说明 | 主题 |
| **4. 宿主 Grok 启动（高级）** | 仅当仍需要安装级覆盖时：`Grok 可执行文件`（见 Q4）；profile 的定位写清 | 与「每槽位 private home」混成一排无说明 |
| **5. 项目健康** | Librarian 体检 | 与权限旋钮并排无分组 |

作用域控件：保留 **全局 / 当前项目**，但**只作用于当前分区里真正分 scope 的项**（今：路径/适配器可项目覆盖；权限模式/主题全局-only——已有，应在分区标题重复说明）。

---

## Q4. 那两个路径字段该怎么办？

### 结论（先）

| 字段 | 还有存在必要吗？ | 建议层 | 参考产品怎么做 |
| --- | --- | --- | --- |
| **Grok 可执行文件路径** | **有**，但是**安装/宿主启动配置**，不是「每槽位 auth」 | **全局（+ 可选项目覆盖）高级项**；槽位启动已消费该解析结果作 `executable` | 参考产品几乎都是 **PATH 发现** 或安装器；很少在「权限页」放二进制路径。OpenCode/Codex 不在 General 主路径强调 agent 二进制。Grok 自身用 PATH / 安装脚本 |
| **Grok profile 路径** | **仍有，但语义已分裂**，易与槽位 private home 冲突 | **全局高级 / 或仅登录槽默认 home 覆盖**；API key 槽应**继续用槽位 private home**，不要被该字段误导为「每槽共享 profile」 | Grok：`GROK_HOME` 覆盖 config 根（`~/.grok`），不是「每个会话一个 UI 字段」。Codex：`~/.codex` 固定布局 |

### 证据

**字段定义**

- `runtime.grok.executable` / `runtime.grok.profile`；env：`PILLOW_GROK_EXECUTABLE` / `PILLOW_GROK_PROFILE`。  
  证据：`crates/storage-sqlite/src/settings.rs` L25–46。  
- 解析优先级：`env → project → global → default`。  
  证据：同文件 L1–8。

**仍被谁消费**

1. **非槽位 / 适配器打开路径**：`discover_grok_config_with_overrides(executable, profile)` → 始终设置 `honor_isolated_home = profile`。  
   证据：`apps/desktop/src-tauri/src/runtime_composition.rs` L351–356、L448–501。  
2. **槽位启动**：`discover_grok_executable_with_override(resolved.grok_executable…)` 取**同一安装级可执行文件**，再 `build_slot_grok_config`。  
   证据：`runtime_slot_commands.rs` L695–710、L1073–1076；`runtime_composition.rs` L398–428。  
3. **槽位 home 语义（与设置 profile 不同）**：  
   - `GrokLogin`：**故意** `honor_isolated_home = None`（用当前用户 profile，不重定向）。  
   - `ApiKey`：Pillow **私有 home**（slot 级），禁止塞 login `auth.json`。  
   证据：`runtime_composition.rs` L391–419 注释与分支。

**槽位模型**

- 槽位是 project-scoped：`auth` + `role`；LocalCli 另有**自己的** `executable_path`（状态探测用，非治理对话）。  
  证据：`apps/desktop/src/features/settings/runtimeSlotsTypes.ts` L126–167；`execution_path.rs` L142–188。  
- LocalCli 的可执行路径在**槽位表单**，不是设置页那两个字段。  
  证据：`RuntimeSlotsSection` local_cli 创建流（`authKind: "local_cli"`）。

**参考产品**

- Grok：`GROK_HOME` 改 config 目录默认；可执行文件靠安装进 PATH。  
  证据：README Environment Variables；Installation。  
- Codex：用户数据 `~/.codex`；CLI 二进制独立安装；配置里无「codex.exe path」常规键（Config basics 未列）。  
- OpenCode：desktop General 无「opencode binary path」主行（settings-v2 general 证据）。

### 建议（形状，非代码）

1. **保留可执行文件路径**作为「本机 Grok CLI 在哪」——全局默认，可选项目覆盖；UI 归 **运行时/高级**，旁注：所有治理 Grok 槽共用此二进制（除非未来 per-slot override 产品化）。  
2. **profile 路径**不要再与「槽位 auth」并列呈现为同级必填：  
   - 对 **Login 槽**：说明「使用本机 Grok 登录目录；高级覆盖才填」；  
   - 对 **API key 槽**：说明「隔离 home 由 Pillow 按槽生成，不受此字段支配」。  
3. 若只保留一个字段：**优先留 executable**；profile 可降为「高级覆盖 HOME」折叠项。  
4. **不要**把 executable 塞进每个槽位的 auth 表单，除非产品要支持「项目 A 用 grok-nightly、项目 B 用 stable」——参考产品默认不这么做；LocalCli 才是 per-slot 可执行文件。

---

## Q5. 我们现在的规则语法与它们差在哪？

### Pillow 现状摘要（证据）

| 项 | Pillow | 证据 |
| --- | --- | --- |
| 模式 | `default` / `accept_edits` / `bypass` | `settings.rs` L168–235；UI `SettingsPanel.tsx` L78–82 |
| 模式效果 | accept_edits 自动批写；bypass 写+进程；**皆不取消 deny 规则叙事**（UI 警告） | `automatically_approves_*` L229–235；UI L371–373 |
| 模式作用域 | **仅全局** | `KEY_PERMISSION_MODE` L31–32；`upsert_permission_mode` L578–584 |
| 规则前缀 | `process` / `write` / `edit` | `permission_rules.rs` L37–59 |
| 规则动作 | `allow` / `deny`（无 `ask` 字面；未命中 = RequireConfirmation） | L63–84；求值 L363–403 |
| 匹配 | process：字面前缀 **或** 全串 wildcard；write/edit：portable path wildcard（`*`/`**`） | L339–352；L669–741 |
| 优先级 | **deny 先于 allow**；同动作 input 顺序；**项目 scope 的 allow 被忽略** | L355–356、L371–375 |
| 进程 allow 额外门 | `process_subject_confirmed_for_allow` 形状确认 | L358–361、L415–477 |
| 命令串 | 保守分段 + 合取 allow；复杂构造 `None`→确认 | L131–204 |
| 目录边界 | 治理写路径必须是**项目相对 portable 路径**，canonical 后 `starts_with(project_root)`；与 permission mode **无关** | `tool-executor/src/write.rs` L566–611、L672–699 |

### 具体差异与对用户的含义

| # | 差异 | 对用户意味着什么 |
| --- | --- | --- |
| 1 | **无 `ask` 规则动作**；只有 allow/deny + 默认确认 | 不能写「这类命令总是再问一次」的显式规则；只能靠「没有 allow」落到确认。OpenCode/Grok 可写 ask 规则。 |
| 2 | **前缀是 `process`/`write`/`edit`**，不是 `Bash`/`Edit`/`Read`/`MCPTool`… | 从 Grok/Claude 文档抄 `Bash(npm*)` **不能直接用**；用户心理模型与市面 DSL 不一致。 |
| 3 | **无 `Read` / 网络 / MCP / external_directory 规则面** | 读、web、MCP 不在同一规则表；**项目外访问没有一等公民开关**。 |
| 4 | **项目规则只能 deny、不能 allow**（引擎忽略 project allow） | 项目无法「为本仓库自动允许 npm」；只能全局 allow 或每次确认。OpenCode 项目 config 可放宽。 |
| 5 | **模式只有 3 档**，无 Codex 式 sandbox 轴、无 OpenCode auto 与规则表解耦的完整叙事 | 「全开」被理解成无界；实际写仍锁项目根 → **预期落差**（见 Q2）。 |
| 6 | **deny>allow（先收集再比）** vs OpenCode **last-match-wins** | 同样「先 `*` allow 再写细 deny」在两边都合理；但 OpenCode 依赖**书写顺序**，Pillow/Grok 更像**动作优先级**。用户若混读文档会配错。 |
| 7 | Process allow 有 **额外 fail-closed 形状门**（delegation/`-c`/`eval` 等） | 写了 `process: allow *` 仍可能被弹窗；比 Grok 文档字面「allow 即过」更严。安全更好，但需 UI 说明，否则像「规则坏了」。 |
| 8 | **模式收据 + 提案审计** 在 bypass 下仍生成 | UI 已提示；与「yolo 完全无痕」不同。 |
| 9 | 规则存在 **SQLite**（host 域），不在 `opencode.json`/`.grok` | 换机器/手改配置文件的习惯不适用；对「IDE 设置 = 文件」用户不直观。 |
| 10 | 与 Grok 原生规则**闸点不同**：Pillow 表管 **host 提案路径**；Grok 规则管 **Grok 进程内工具**（既有调研） | 用户以为设了 Pillow 规则就约束了「Grok 自己写盘」——若原生工具未剥净，**两边不是同一扇门**。 |

### 与「全开 + 目录外」相关的关键差异

- Codex/OpenCode：**批准轴与范围轴可分**。  
- Pillow：**批准轴（mode/rules）有**；**范围轴在执行器硬编码项目根**，mode=bypass **不**调用「放宽 root」。  
  证据：写路径策略与 mode 无交叉引用（`write.rs` 路径校验 vs `PermissionMode` 在 storage/runtime-host）。

---

## 如果只改一处最能减少困惑

### 判断

**把「权限模式」从杂物设置表单里拆成独立「安全」分区，并用一句话钉死两轴：**

> **权限模式只决定「要不要问你」；文件默认仍只能动当前项目目录。要动项目外，必须另开范围（尚未提供 / 将提供）。**

### 理由

1. 用户原话两条同时命中：**设置页无逻辑** + **权限规则/设置奇怪**。参考产品里，Codex/OpenCode 都避免把「主题 + 二进制路径 + 自动批准」排成一列同等旋钮。  
2. 「全开」文案（`SettingsPanel.tsx` L81）在中文里极易读成**无界**；而执行器与 Codex 一样**默认保项目边界**。这是**预期管理 bug**，比再设计一套 DSL 更便宜、见效更快。  
3. 路径字段与槽位模型的纠缠（Q4）是第二痛点，但改文案/分区即可先止血；真正删字段要动启动组合，超出「只改一处」的信息架构止血。  
4. 规则 DSL 对齐 Grok（`Bash(…)`）是中长期；**不先分轴**，对齐语法也会继续被「bypass 为啥还 PathRejected」淹没。

可选同一处的最小附带（仍算信息架构，非大功能）：在权限规则区复用同一句「deny 永远优先；项目只能加严不能放宽」。

---

## 残余未知

1. **Grok `agent stdio` 路径**对顶层 `--allow`/`--deny` 的接线：既有调研标「stdio 未接线」；本单未重新读上游源码确认是否已修。  
2. **Grok profile `permissionMode` 非 Bypass 值**对主会话是否完整生效：既有调研标 forward-compat / 未确定。  
3. **Codex IDE 图形设置页**除「打开 config.toml」外是否还有分 tab 的原生面板：本单只核实文档入口与 TUI thread settings，**未**做桌面应用截图级 UI 审计。  
4. **Pillow 非槽位启动路径**在「槽位已统一」产品里是否仍对终端用户暴露：代码路径仍在（`discover_grok_config_with_overrides`），产品是否仍走该路径 **未做运行时枚举**。  
5. **OpenCode managed / enterprise** 对 permission 的 UI 暴露：文档有 managed 层，settings-v2 是否展示 **未逐项核对**。  
6. 用户「全开时项目外也不问」若要产品化：写路径协议目前是 **project-relative only**（拒绝绝对路径）；支持目录外是**协议+能力**变更，不是单改 mode 枚举——本单不展开设计。

---

## 参考索引（可核对）

| 主题 | 位置 |
| --- | --- |
| Pillow 设置堆叠 | `apps/desktop/src/features/settings/SettingsPanel.tsx` |
| Pillow 规则引擎 | `crates/kernel-policy/src/permission_rules.rs` |
| Pillow 模式枚举 | `crates/storage-sqlite/src/settings.rs` L168–235 |
| Pillow 写路径边界 | `crates/tool-executor/src/write.rs` L566–699 |
| Pillow 路径设置消费 | `apps/desktop/src-tauri/src/runtime_composition.rs` L351–501 |
| OpenCode 权限文档 | `_ref/opencode/packages/web/src/content/docs/permissions.mdx` |
| OpenCode 求值 | `_ref/opencode/packages/opencode/src/permission/index.ts` |
| OpenCode 设置 Tab | `_ref/opencode/packages/app/src/components/settings-v2/dialog-settings-v2.tsx` |
| Grok CLI / README | 本机 `grok --help`；`%USERPROFILE%\.grok\README.md` |
| Grok 权限复用调研 | `docs/research/2026-08-10-grokbuild-permission-reuse.md` |
| Codex 审批与沙箱 | https://developers.openai.com/codex/agent-approvals-security |
| Codex 配置基础 | https://developers.openai.com/codex/config-basic |
| Codex schema | `AskForApproval.ts` / `SandboxMode.ts`（openai/codex） |
| Codex patch 边界 | `codex-rs/core/src/safety.rs` |

---

**PERMISSION-UX-COMPLETE**
