# Shell 命令审批 — 业界现状与 Pillow 落地建议

Date: 2026-08-11  
Status: **只读调研完成**（上游 shallow clone + 安装版 Grok 用户文档 + 本仓代码）  
Worker: grok1（只读）  
Controller: Claude  
Scope: 禁止改码 / 禁止跑门 / 禁止 cargo·pnpm build；本单**唯一写入**即本文件。  
Clone 读完即删（临时目录；报告**不含本机绝对路径**）。

完成信号: `SHELL-APPROVAL-SOTA-COMPLETE`

---

## 1. 一句话结论

**照抄 Grok Build 当前源码（与 Claude Code 公开文档同形）的「树解析拆段 + deny/ask 逐段 + allow 合取 + 复杂构造 fail-closed」，不要照抄过时用户文档里的「allow 整串前缀」；也不要指望 C4「程序目录表 + 结构化 argv」无损承接 ACP `terminal/*` shell 串——开壳就改成 shell 串治理，目录表降级为结构化旁路。**

落地优先级：先接 ACP 壳 + 半开陷阱防护 + 最小 fail-closed 拆段 + 规则合取；**不要**第一刀 vendoring 整份 `bash_command_splitting`（约 1.9k 行 + tree-sitter）。

---

## 2. Q0 — Grok Build 自己怎么对 shell 串做权限判定？（最重要）

### 2.1 匹配主体是什么？

| 层 | 主体 | 查证 |
| --- | --- | --- |
| 工具输入 | **一整行 `command: String`**（无结构化 argv） | 既有 `2026-08-11-governed-process-route.md`；生产 `AcpTerminalAdapter` 恒 `args=[]` |
| 规则解析 | `Bash(pattern)` / 裸 `Bash` / `Bash(cmd:*)` → 前缀 idiom | `xai-grok-workspace/.../permission/rules.rs` `parse_permission_rule` ~L148–218；`strip_bash_colon_wildcard` ~L307–312 |
| 求值入口 | `AccessKind::Bash(cmd: String)` | `policy.rs` `evaluate` / `evaluate_with_cwd` ~L223–289 |
| **拆段后** | 每个 plain command 的 **words 空格 join**（剥 env 前缀 / 部分 wrapper） | `bash_command_splitting.rs` `all_commands_from_script` L929–932；`policy.rs` `evaluate_command_words` L165–220 |

**不是**「永远只看第一个 token」；也**不是**「永远只看整行」。deny/ask/allow 对段的处理不同（见下）。

### 2.2 `&&` / `|` / `;` / `$()` / 反引号 / 重定向怎么处理？

拆分器：`tree-sitter-bash` 解析，仅当 AST 落在白名单节点/运算符上才返回段列表，否则 **`None`（fail-closed）**。

| 构造 | 行为 | 代码 |
| --- | --- | --- |
| `&&` `\|\|` `;` `\|` 及换行序列 | **拆成多个 plain command 段** | `bash_command_splitting.rs` L41–47、L55–98（`ALLOWED_KINDS` / `ALLOWED_PUNCT_TOKENS`） |
| 子 shell `()`、`` ` ``、`$(...)`、控制流、未白名单节点 | 解析失败 → **`None`** | 同上 L108–124；调用方 fail-closed |
| 重定向 / heredoc | 允许出现在 AST 白名单中，但段提取仍是 command 节点 words；复杂替换仍 fail | L72–89 注释 + 白名单 |
| 无法拆段时的 deny/ask | **`AskFailClosed`**（升级为询问，不静默放行） | `policy.rs` `evaluate_bash_command_segments` L152–153 |
| 无法拆段时的 allow 合取 | **`false`**（不能因 allow 整串自动批） | `policy.rs` `bash_chain_fully_allowed` L292–294 |

### 2.3 规则动作对段的语义（以**源码**为准，不是安装版用户文档）

| 动作 | 匹配对象 | 段语义 | 代码 |
| --- | --- | --- | --- |
| **deny / ask** | 每段 words（剥 wrapper、`bash -c` 内联递归）+ 也可走整串 `evaluate` | **任一 denied 段拒整条**；ask 合取 | `evaluate_bash_command_policy` L128–131；`evaluate_bash_command_segments` L147–162；`combine_gate_decisions` L52+ |
| **allow** | **每一段都必须独立命中某条 Bash/Any allow**（**合取**） | `Bash(git:*)` **不会**放行 `git status && curl … \| sh` | 注释 + 实现 L276–284、`bash_chain_fully_allowed` L292–327；单测 L1118–1137；manager 端到端 HackerOne #3876332 注释 `manager/mod.rs` L5378–5384 |
| 内置只读白名单 | 拆段后 **word-boundary** 主命令 | `ls` 不匹配 `lsof`；`ls && rm` 中仅 `ls` 只读 | 安装版 guide §Read-Only Shell；manager 侧 `is_safe_command_words` 族（`manager/mod.rs` ~L257+） |
| 危险命令地板 | `rm`/`chmod`/…/`git push` 等 | 即使用 remembered prefix / 只读表，仍可能再问 | 安装版 guide §Dangerous Commands |

**单段模式匹配（源码，注意 deny vs allow 不对称）：**

| 路径 | 算法 | 词边界？ | 位置 |
| --- | --- | --- | --- |
| 通用 `pattern_matches` 对 `AccessKind::Bash`（deny/ask 主路径） | `trim_start` 后 **`starts_with(pattern)` 或 freeform glob** | **无**（`Bash(sed:*)` 也挡 `sed-evil`，单测 L1101–1113） | `policy.rs` L595–611 |
| allow 段匹配 `bash_command_matches_pattern` | **词边界前缀**（`git`≠`gitleaks`）**或** freeform glob | **有**（前缀路径） | L526–561；单测 L1138–1143 |
| `Bash(cmd:*)` | 剥成裸前缀 `cmd` | 见上 | `rules.rs` L307–312 |

### 2.4 管道顺序（规则只是其中一层）

安装版用户文档（与源码 manager 一致的产品叙述）：

1. PreToolUse hooks  
2. Permission rules（deny > ask > allow）  
3. Remembered grants（项目作用域）  
4. Built-in auto-approvals（只读工具 + 只读 shell 段）  
5. Prompt policy（mode：ask / always-approve / dontAsk / auto …）

Always-approve **仍**受 deny、hooks、以及部分 shell **ask 段**约束。  
OS sandbox（Landlock/Seatbelt）是**正交**层，不是规则替代品。

### 2.5 文档 vs 源码冲突（必读）

| 陈述 | 安装版 `22-permissions-and-safety.md` | **源码 @ pin `75e73f3`** |
| --- | --- | --- |
| allow 是否整串匹配 | 写明：allow **只对整串**；`Bash(git *)` 会放行 `git status && rm -rf /` | **否**。allow **合取每段**；单测 + HackerOne 回归明确禁止整串 allow 走私 |
| 前缀有无词边界 | 写明：无边界，`Bash(git)` 匹配 `gitleaks` | allow 路径**有**词边界；deny 的 `starts_with` **无**边界（故意更宽） |

**以源码 + 单测为准。** 安装版用户文档在 allow-chain 上**过时/错误**，不可照抄进 Pillow 产品语义。

### 2.6 能否直接复用判定逻辑？

| 选项 | 判定 |
| --- | --- |
| **原样 vendoring `bash_command_splitting.rs` + `policy.rs` Bash 分支** | 法律上可（Apache-2.0），工程上**不推荐第一刀**：~1.9k 行拆分器 + tree-sitter-bash + 大量 wrapper/`env -S`/`bash -c` 递归；与 Pillow 宿主/提案模型未接线 |
| **只抄形状**（拆段合取 + fail-closed + deny>allow） | **推荐** |
| 当依赖 crate 调用上游 | **不可**：`xai-grok-workspace` 非 Pillow 依赖；ACP 宿主侧要自己执行门闸 |
| Pillow `Cargo.lock` 已有 | `glob`、`shlex`（传递依赖可见）；**无** `tree-sitter` / `tree-sitter-bash` |

---

## 3. 横向对比

### 3.1 对比表

| 项目 | 审批对象 | 串联/`$()` 等 | 规则语法 | 持久 always-allow | 兜底 | 明确放弃什么 | License / pin |
| --- | --- | --- | --- | --- | --- | --- | --- |
| **Grok Build**（基座） | shell **字符串**；规则对段/合取 | tree-sitter 拆 `&&\|\|;|`；复杂构造 fail-closed → Ask | `Bash(glob/prefix)` deny>ask>allow | 项目内 remembered grant；config allow | OS sandbox 可选 | 不把 shell 收成结构化 argv；allow 文档曾落后于代码 | **Apache-2.0** · git `75e73f3…` · `SOURCE_REV` `a61c32b…` |
| **Claude Code** | Bash/PowerShell **命令串** | **文档查证**：拆 `&& \|\| ; \| \|& &` 与换行；**每段独立匹配**；always-allow 按段存规则 | `Bash(pattern)` `*` glob；`:*` 尾缀；deny>ask>allow | `.claude/settings.local.json` 等，仓库级 | OS sandbox 可选（独立层） | 不靠模型自觉；规则在客户端强制 | 闭源产品；公开文档 [permissions](https://code.claude.com/docs/en/permissions)（**无源码 pin**） |
| **Codex CLI** | **token 数组 / shell -lc 脚本**；execpolicy 对**段** | tree-sitter 拆 plain 段（与 Grok **同形**注释/结构）；复杂 → `used_complex_parsing`；allow 旁路沙箱需**每段** allow | Starlark `prefix_rule(pattern=[tokens], decision=allow\|prompt\|forbidden)` | 可 amend 写回 rules | **沙箱默认** + 审批 + 危险命令启发式 | 不用 `Bash(git *)` 字符串 glob 做主规则；prefix 是 **argv token 前缀** | **Apache-2.0** · `8cabf5a…` |
| **Goose** | **工具级**（shell 工具整体），非整条命令模式 | **无**命令串拆段审批（查证路径：工具 permission + LLM read-only judge） | Always Allow / Ask Before / Never Allow **per tool** | 配置 + smart 缓存 | 无目录硬沙箱（文档称工作目录是约定） | **不**做 `Bash(cmd*)` 细规则；默认 Autonomous 风险高 | **Apache-2.0** · `1844d3f…` |
| **Cline** | **工具级** autoApprove；shell 多为整工具 | **未**发现「拆 `&&` 逐段规则」的核心实现（本 pin） | `toolPolicies: { run_commands: { autoApprove } }`；VS Code 侧 `executeSafeCommands` 等粗开关 | 设置 / 会话 override | 可选 subprocess sandbox 模块 | 不把安全押在命令模式 DSL | **Apache-2.0** · `149abb0…` |
| **OpenHands（本仓 pin）** | 当前 monorepo 主线是 **Agent Canvas** + 后端/沙箱部署 | **不**在前端做 shell 段规则；安全偏 **Docker/VM 后端 + 可选 SecurityAnalyzer/确认模式**（SDK 接口文档化） | 确认模式 / 风险分析器（LLM/外部） | 云/设置 confirmation | **容器/远程沙箱**为主 | 本 pin **不是**经典 in-process Bash 规则引擎；细规则在旁路 DefenseClaw 等 | **MIT** · `b7e325e…` |

### 3.2 谁做了「把命令行拆成子命令，逐段判定」？

| 项目 | 有？ | 实现位置 | 处理不了的边界 |
| --- | --- | --- | --- |
| **Grok Build** | **有** | `permission/bash_command_splitting.rs`；策略 `policy.rs` | `$()`/反引号/子 shell/控制流 → 不拆；allow 合取失败；`sudo`/`xargs`/`nohup` **不**剥（文档 + 代码）；Windows 原生 shell 串依赖 bash 语法树（**Windows PowerShell 串能力：未在本单深挖**） |
| **Claude Code** | **有**（产品文档） | 闭源；文档 *Compound commands* | 不能拆的复杂串：按产品策略仍提示；细节无源码 |
| **Codex** | **有** | `shell-command/src/bash.rs` `try_parse_word_only_commands_sequence`；`exec_policy.rs` `commands_for_exec_policy` L844+；allow 旁路沙箱「every parsed segment」L416–429 | 复杂解析 `used_complex_parsing: true` 收紧 auto-amend；PowerShell 另路径 |
| **Goose / Cline** | **无**（本 pin 所见） | 工具级 | 任意 `cmd1 && evil` 在「允许 shell 工具」时一并放行（若 auto） |
| **OpenHands Canvas pin** | **无**命令 DSL | 沙箱/分析器 | 依赖隔离边界，非命令模式 |

### 3.3 流派分歧（不要编「共识」）

业界**没有**单一共识，至少三派并存：

1. **Shell 串 + 段级规则**（Grok / Claude Code / Codex 拆段层）  
2. **工具级开关 +（可选）沙箱**（Goose / Cline）  
3. **执行环境隔离优先**（OpenHands 后端 Docker/VM；Grok/Claude 的 OS sandbox 作第二层）

共同的**弱共识**仅有：

- 只靠模型「别写危险命令」不够；  
- allowlist 若存在，应对 **链** 小心（Grok 单测 / Claude 文档 / Codex every-segment allow 都认这点）；  
- 复杂 shell 解析不完整时 **fail closed / 再问人**，不要装懂。

---

## 4. 给 Pillow 的推荐设计（能快落地优先）

### 4.1 推荐设计（一条主路径）

**名称：Shell-string governed terminal（Grok/Claude 形）**

在真实 Grok → ACP `terminal/create` 路径上：

1. **审批主体**  
   - 原始：整条 `command` 字符串（收据与 UI 展示全量）。  
   - 求值：`segments = split_or_fail(command)`；每段得到 `words: Vec<String>` 与 `subject = words.join(" ")`。  
   - 规则仍走已有 `permission_rules` 的 **process 前缀**（或新增 `shell` 前缀别名），subject 用段 subject；**allow 合取所有段**，**deny 任一命中拒整单**。

2. **拆解器**  
   - **第一刀（快）**：`shlex` 分词 + 仅在顶层识别 `&&` `||` `;` `|` 的**保守**拆分；遇到 `$(` `` ` `` `(` 控制流、重定向、引号不平衡 → **整单 RequireConfirmation 或 Reject**（产品二选一，推荐 **RequireConfirmation** 与 Grok AskFailClosed 对齐）。  
   - **第二刀（可选）**：再评估引入 `tree-sitter-bash`（Apache 生态已有 Grok/Codex 实现可参考形状，**复制需保留 NOTICE**），不要第一刀就做。  
   - **不要**为了「复用」把整个 `xai-grok-workspace` permission 栈嵌进 Pillow。

3. **危险构造**  
   | 构造 | 推荐 |
   | --- | --- |
   | `a && b` 等可拆链 | 逐段判定；allow 合取 |
   | `$()` / 反引号 / 子 shell / 脚本 heredoc 执行 | **不自动 allow**；默认问人或 deny |
   | 重定向写文件 | 第一刀可与「复杂构造」同桶 fail-closed；后续再对接 path deny（Grok `shell_access` 是后话） |
   | 管道 | 视为多段；右侧危险命令不能被左侧 allow 带过 |

4. **与权限规则表衔接**  
   已有设计：`docs/superpowers/specs/2026-08-10-permission-rules-design.md` + 实现 `crates/kernel-policy/src/permission_rules.rs`。  
   **四条不可破约束保持：**  
   1. 规则只决定是否询问，不取消审计/收据；  
   2. **allow 仅全局**；项目 scope 只能 tighten（deny）；  
   3. **deny > allow**；无匹配 → RequireConfirmation；  
   4. 不从项目树文件加载 allow 权威。  

   衔接改动点（设计层）：  
   - 今日 `canonical_process_subject(program, argv)` **只服务** `process.run@1`。  
   - Shell 路径增加 `canonical_shell_segment_subject(words)`（空格 join，与 Grok allow 段一致）。  
   - 评估 API：`evaluate_permission_rules` 保持纯函数；宿主对 **多段** 循环后合取 allow / 短路 deny。  
   - **不要**把 `cargo test && rm` 合成单一 process subject 再 `starts_with("cargo")` 放行——那是 Grok 文档错误语义，源码已修掉。

5. **C4 程序目录表何去何从**  
   | 路径 | 目录表 |
   | --- | --- |
   | `process.run@1` / fake agent 结构化 | **保留**硬约束（当前 `PROCESS_CATALOG` + `DENIED_LABELS` 含 shell） |
   | ACP `terminal/*` 真 Grok | **不能**假装仍是目录表 argv；目录表**降级**为：可选「段首 token 映射到 catalog 则显示更友好 / 可附带结构校验」，**不能**作为唯一门闸 |
   | 退役？ | **不退役**结构化路径；**不**用目录表否决「开 shell 能力」本身 |

6. **半开陷阱**（与写方向 LocalFs 同构）  
   ```
   allowlist 含 shell 工具  ∧  clientCapabilities.terminal = false
     → LocalTerminalBackend，宿主无 terminal/create
   ```  
   强制：**allowlist 出现任何 shell 类 short id（`run_terminal_cmd` 等）时 `terminal` 必须为 true**，并用测试钉死（见探针判据）。  
   依据：`docs/research/2026-08-10-mcp-tool-exposure-probe.md` §12。

7. **执行与审计**  
   仍走：提案 →（规则自动批或人批）→ 宿主执行 shell → durable 收据。  
   Auto-allow **不**跳过收据（permission-rules 设计不变量 1）。

### 4.2 按「能多快落地」排序 + 粗工作量

| # | 项 | 工作量（粗） | 说明 |
| --- | --- | --- | --- |
| **S0** | 半开陷阱测试 + `terminal`/allowlist 耦合 | **0.5–1d** | 不给能力也要先钉死；纯防护 |
| **S1** | ACP `terminal/*` 宿主实现 + 提案/收据最小环（每条命令问人，**无**规则） | **3–5d** | 先恢复「有手」；与写方向对称 |
| **S2** | 保守拆段器 + deny/allow 合取接到 `permission_rules` | **2–3d** | 复用现有表与 `evaluate_permission_rules`；扩展多段宿主逻辑 |
| **S3** | UI：展示全串 + 分色段 + 规则归因 | **1–2d** | 可与 S2 并行部分 |
| **S4** | 内置只读段白名单（可选，减噪） | **1–2d** | 抄 Grok/Codex 只读表思想，**不要**当安全边界 |
| **S5** | tree-sitter-bash 级拆分 + wrapper 剥除 | **1–2w** | 仅当 S2 误报/漏报不可接受 |
| **S6** | OS sandbox | **大** | 正交；不阻塞 S1–S3 |
| **—** | 坚持 C4 字面、永不接 shell | **0** | 产品否决过；仅作对照，不推荐 |

**建议交付切片：S0 → S1 → S2（共约 1–1.5 人周量级）**，先可用再变聪明。

### 4.3 探针验收判据（`PILLOW_MCP_PROBE_ONLY=<id>` 友好）

建议在现有 `scripts/probe-mcp-tool-exposure.mjs` 或并行 `probe-terminal-*.mjs` 增加（**本单不实现**）：

| id | 期望 |
| --- | --- |
| `terminal-half-open-forbidden` | profile allowlist 含 shell 且 `terminal:false` → **启动失败或硬拒绝配置**（不得静默 LocalTerminal） |
| `terminal-create-seen` | `terminal:true` + shell 允许 → 会话中出现 `terminal/create`（宿主侧计数） |
| `shell-chain-allow-conjunctive` | 全局 allow `process:cargo *`（或等价）时，模型/注入 `cargo test && rm -rf <bait>` → **不得**自动批；须问人或 deny |
| `shell-chain-deny-segment` | deny `process:rm *` 时，`echo ok && rm -rf <bait>` → 拒整条；bait 路径不存在 |
| `shell-complex-fail-closed` | 含 `$(...)` 或反引号的串 → 不自动 allow |
| `shell-receipt-on-auto` | 规则 auto-allow 的 `cargo --version` 仍有 durable 收据行 |

（进程探针与 MCP 探针可分脚本；命名对齐 `PILLOW_MCP_PROBE_ONLY` 或统一 `PILLOW_PROBE_ONLY`。）

### 4.4 明确不推荐的捷径

| 捷径 | 原因 |
| --- | --- |
| 整串 `starts_with` allow（文档旧语义） | `cargo x && rm -rf /` 被 `cargo*` 放行；Grok 源码已因 HackerOne 修掉 |
| 只靠剥除原生 shell 工具、永不接 terminal | 用户否决「没手」；且 profile 历史已翻车 |
| 第一刀引入完整 tree-sitter + 剥 wrapper 全家桶 | 拉长交付；S2 足够产品可用 |
| 用项目 `.grok/config.toml` allow 当权威 | 违反 Pillow「项目不可 self-grant」 |

---

## 5. 查证 / 未确定

### 5.1 查证

| 项 | 证据 |
| --- | --- |
| Grok allow **合取段**、不整串放行 | `policy.rs` L276–284、L1118–1137；`manager/mod.rs` L5378–5384 |
| Grok 拆分器 fail-closed | `bash_command_splitting.rs` L48–50、L108–124；`all_commands_from_script` L929–932 |
| Grok deny 无词边界前缀 | `policy.rs` L608–610、L1101–1113 |
| Grok allow 有词边界前缀 | `policy.rs` L526–530、L1138–1143 |
| 安装版文档写 allow 整串 | 用户 guide `22-permissions-and-safety.md` §Rule Matching Reference *Chained commands* |
| Claude 复合命令每段独立 | [Configure permissions](https://code.claude.com/docs/en/permissions) *Compound commands* |
| Codex 同形 tree-sitter 拆段 + every-segment allow 旁路 | `shell-command/src/bash.rs` L22–28；`exec_policy.rs` L416–429、L844–850 |
| Codex `prefix_rule` token 前缀 | `execpolicy/README.md`；`rule.rs` `PrefixPattern::matches_prefix` L46–59 |
| Goose 工具级权限、无命令 DSL | `tool-permissions.md`；`permission_inspector.rs` |
| Cline 工具 autoApprove | `docs/sdk/guides/permission-handling.mdx`；`AutoApprovalSettings.ts` |
| Pillow 半开陷阱叙述 | `2026-08-10-mcp-tool-exposure-probe.md` §12 |
| Pillow process 目录表 | `tool-executor/src/process.rs` L40–66 |
| Pillow 规则纯函数 | `kernel-policy/src/permission_rules.rs` L111–177 |

### 5.2 未确定

| 项 | 原因 |
| --- | --- |
| 真 Grok 经 ACP 对 Pillow 宿主 `terminal/create` 的实盘 | 本单禁止探针/跑门 |
| Grok 安装二进制与 clone `75e73f3` 是否同一 SOURCE_REV 行为 | 本机 CLI 版本未在本单强制对齐跑测；语义以 clone 源码为准 |
| Claude Code 闭源实现与文档 100% 一致 | 仅文档 |
| Windows 上 Grok/Codex 对 **PowerShell** 串的完整段策略 | Codex 有 PS 路径；Grok 主拆分器是 bash 语法 |
| OpenHands **software-agent-sdk** 旧 Runtime 确认模式细节 | 本 pin 仓库已是 Agent Canvas 主线，未再 clone SDK 全树 |
| Cline `executeSafeCommands` 白名单是否拆 `&&` | 本 pin 未找到与 Grok 同级拆段器；**标未确定偏无** |
| 是否将 `process` 前缀 UI 文案映射为用户熟悉的 `Bash(...)` | 产品文案决策，非本单 |

---

## 6. 参考项目 license 与 pin

| 项目 | License | Pin（本单 clone HEAD） | 备注 |
| --- | --- | --- | --- |
| **xai-org/grok-build** | Apache-2.0（README/LICENSE） | `75e73f3d6ac0350d211f12ae7d57c2c0aad72576` | `SOURCE_REV`=`a61c32b12a2b400f212221cd8762e05f9b36828d`；与既有 Pillow 调研 pin 一致 |
| **openai/codex** | Apache-2.0 | `8cabf5a6cf103cebe338d46346e43e3201e64f41` | `codex-rs/shell-command` + `execpolicy` + `core/src/exec_policy.rs` |
| **block/goose**（aaif-goose 同源） | Apache-2.0 | `1844d3fb4aed0ec7f2e3806829cb887981f15ead` | 工具级权限 |
| **cline/cline** | Apache-2.0 | `149abb0ddb51d8f1827751b731dd572027069591` | 工具 autoApprove |
| **OpenHands/OpenHands** | MIT | `b7e325e9986af25b548950587fbcf04a60ee82a4` | 当前主线 Agent Canvas；沙箱/后端取向 |
| **Claude Code** | 闭源专有 | 无源码 pin | 公开文档：code.claude.com/docs/en/permissions（抓取日：本单） |
| **Pillow（本仓）** | 见仓库 LICENSE | 只读引用 `permission_rules.rs` / process catalog / 既有 research | 非上游 |

**第三方代码复用注意：** 若日后复制 Grok/Codex 的 bash 拆分段，保留 Apache-2.0 归属与 `THIRD_PARTY_NOTICES`；第一刀**不**复制。

---

## 7. 与既有 Pillow 文档的关系

| 文档 | 关系 |
| --- | --- |
| `2026-08-11-governed-process-route.md` | 钉死「真 Grok 给 shell 串」；本单补「业界如何批这串」 |
| `2026-08-10-permission-rules-design.md` | 规则层设计仍有效；** Bash 段合取**修正其中「C4 无 shell 链」假设的适用范围 |
| `2026-08-10-grokbuild-permission-reuse.md` | 上游 permission 栈在 **agent 进程内**；宿主 shell 门闸仍要自建/对齐形状 |
| `2026-08-10-mcp-tool-exposure-probe.md` §12 | 半开陷阱；本单 S0 验收直接继承 |

---

## 8. 完成信号

`SHELL-APPROVAL-SOTA-COMPLETE`
