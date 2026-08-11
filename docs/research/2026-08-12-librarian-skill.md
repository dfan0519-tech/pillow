# Pillow — 信息管理员角色与内置知识管理 skill（只读调研）

Date: 2026-08-12  
Worker slot: `grok3`  
Controller: Claude  
Mode: **read-only research**（除本报告外不改任何文件；无 git 写操作；无门/E2E）  
完成信号: `LIBRARIAN-SKILL-COMPLETE`

---

## 0. 总控摘要（先读）

| 问题 | 结论（一句话） |
| --- | --- |
| Q1 skill 形状 | 市面收敛到 **`SKILL.md` + 渐进披露（metadata 常驻 / 正文按需）**；**优先级主要靠目录层级与名字去重，不是 numeric priority 字段** |
| Q1 用户关键 | 「装很多 skill 后内置仍优先被读到」≠ 靠 description 相关度；要保证可读性必须 **常驻 catalog 名额 / 目录覆盖 / 或根本不走 skill 匹配** |
| Q2 知识层 | 值得抄的是 **注册台账 + 机械陈旧检测 + 入口视图 + 显式 supersession**；双链是可选增强。**`knowledge-indexer` 已覆盖大半，缺口在事实校验与自主调度** |
| Q3 监控什么 | 可判定检查项见 §Q3 表；台账实测：**在途 7、待验收 9、最新 handoff 声称在途 0 → `ledger stale` 报 10 条 STALE** |
| Q4 槽位 vs 内置 | **推荐：核心监控内置（零 token 机械检查）+ 可选 skill/角色解释报告**；纯槽位角色对新用户与无 token 场景失败 |
| 若只做一件事 | **把已有 `ledger status` / `ledger stale` / `scan` 做成可定时跑的内置审计产物（带 fresh 与 stale 两种确定输出）**，不要先写新 skill 叙事 |

---

## Q1. 「skill」在市面上到底是什么形状？

### 结论

成熟实现（Claude Agent Skills / Claude Code、OpenAI Codex / ChatGPT Skills、Grok CLI）高度同构：

1. **磁盘形状**：`skills/<name>/SKILL.md`（YAML frontmatter + Markdown 正文），可选 `scripts/`、`references/`、`assets/`。
2. **发现**：扫固定目录树；同名按 **scope 优先级去重或并列**。
3. **注入**：几乎都是 **渐进披露（progressive disclosure）**——启动只注入 `name`+`description`（~50–100 tokens/skill），命中后再读全文与引用。
4. **规模上限**：靠「正文懒加载」；当 skill 数量极大时，Codex **还会截短甚至从初始列表省略 skill**——这意味着「相关度」不是可靠优先级。
5. **优先级真实机制**：**目录/作用域层级** 与 **名字冲突规则** 为主；`description` 只驱动「是否自动触发」；**几乎没有**通用的 `priority: 100` 排序字段保证「内置永远更先被读」。

### 对比表（3 实现 × 4 维度 + 优先级）

| 维度 | Claude Agent Skills / Claude Code | OpenAI Codex / ChatGPT Skills | Grok CLI（README + 本机 discover 行为） |
| --- | --- | --- | --- |
| **文件形状** | 目录 + 必填 `SKILL.md`；frontmatter：`name`、`description`（必填）；正文为 instructions；可带 `references/`、`scripts/` 等 | 同构于 [Agent Skills standard](https://agentskills.io)；`SKILL.md` + 可选 `scripts/`、`references/`、`assets/`、`agents/openai.yaml` | 同构：`SKILL.md` 需 `name` + `description`；目录 `skills/<name>/` |
| **如何被发现** | Claude Code：`~/.claude/skills/`（personal）、`.claude/skills/`（project）、enterprise managed、plugin skills；嵌套 monorepo 子目录 skills 也可在触及该树时可用 | REPO：`$CWD` 与向上至 repo root 的 `.agents/skills`；USER：`$HOME/.agents/skills`；ADMIN：`/etc/codex/skills`；SYSTEM：随 Codex 内置 | 文档声明扫描顺序：`./.grok/skills/`（最高）→ `<repo>/.grok/skills/` → `~/.grok/skills/` / `~/.claude/skills/`（最低）；另有 config `[skills].paths`；兼容 Claude 路径 |
| **如何注入上下文** | **三级渐进披露**：L1 metadata 启动进 system prompt；L2 `SKILL.md` 正文在触发时 bash 读取；L3 引用/脚本按需（脚本只进 stdout） | 启动只列 name+description（+path）；触发后再读全文；`allow_implicit_invocation` 可关隐式触发 | 自动：模型据 `description` 决定是否加载；显式：`/skills`、`/skill-name` 注入。本会话形态与 Claude 同类：列表常驻、读 `SKILL.md` 才进正文 |
| **规模上限** | 文档宣称：未触发时只占 ~100 tokens/skill；bundle 可很大但未读不进上下文；建议 `SKILL.md` 体量可控、重内容放 `references/` | **硬预算**：初始 skills 列表最多约占模型上下文 **2%**，未知窗口时 **8000 字符**；先缩短 description，仍过多则 **省略部分 skill 并告警** | README 未写 token 预算数字；实践上依赖 description 匹配 + 按需读正文。**未确定**是否有「列表截断省略」实现 |
| **优先级 / 冲突** | **名字冲突**：官方 Claude Code 文档写 **enterprise > personal > project**（可覆盖 bundled 同名）；plugin 用 `plugin:skill` 命名空间。**不是** numeric priority。自动选用靠 description 与用户意图匹配 | **同名不强制合并**：两边都可出现在选择器。系统层有 SYSTEM（bundled）路径。**列表预算**会在 skill 很多时丢掉一些——**相关度/预算优先于「保证每条都被看到」** | **同名去重：更高优先级目录覆盖更低**。Local > Repo > User。**没有**公开的 `priority` 字段。Agent profile 另有独立优先级链（与 skill 不同） |

#### 证据锚点

- Claude progressive disclosure 与 ~100 tokens/skill：  
  https://platform.claude.com/docs/en/agents-and-tools/agent-skills/overview  
- Claude Code 路径与冲突规则（enterprise > personal > project；可覆盖 bundled）：  
  https://code.claude.com/docs/en/skills  
  （第三方二次总结亦引用同一顺序，例如 HackerNoon *Navigating Claude Code* 文章。）
- Codex 目录层级 + **2% / 8000 字符** 列表预算 + 可能省略：  
  https://learn.chatgpt.com/docs/build-skills  
- Agent Skills 开放标准（`SKILL.md` 规范）：  
  https://agentskills.io / https://agentskills.io/specification  
- OpenAI API Skills 指南（渐进挂载、frontmatter 校验）：  
  https://developers.openai.com/api/docs/guides/tools-skills  
- Grok 发现路径与同名覆盖：本机 `~/.grok/README.md` **§ Skills → Skill Locations**（Local highest → Repo → User lowest；same name dedupe）。
- Grok 与 Claude 路径兼容：同 README **§ Claude Code Compatibility**（`.claude/skills/` 亦作 skills 加载）。
- 本仓库先前对 Grok 约定注入边界的只读结论：`docs/research/2026-08-10-agent-context-gap.md` §2（Pillow **不**注入 skills；是否在 ACP 路径下仍由 CLI 自载为 **未 live 复证**）。

### 对「内置必须优先可读」的直接含义（仍是事实，不是设计）

| 策略（市面真实做法） | 能否保证「装了一百个 skill 仍优先看到内置」 |
| --- | --- |
| 目录覆盖（local/repo 覆盖 user） | **只能保证同名替换**，不能保证「不同名时排序靠前」 |
| description 相关度触发 | **不能保证**；噪声 skill 多时易漏触发（Codex 甚至会从列表删掉） |
| progressive disclosure | **保证不撑爆上下文**，**不保证**内置被选中 |
| always-on 规则文件（AGENTS.md / CLAUDE.md 一类） | **更接近「永远进上下文」**——但这是 **rules**，不是 skills 的标准形状 |
| Codex 列表预算 | **反例**：skill 过多时部分 skill 会从初始列表消失 |

**结论（Q1 重点）**：若产品要求是「信息管理员那条在用户 skill 膨胀后仍优先被读到」，成熟实现给出的可靠机制是：

1. **把它做成 always-on 规则/宿主注入**，或  
2. **保留在 SYSTEM/bundled 且永不参与「可省略 catalog」**，或  
3. **根本不靠 skill 自动匹配**，而由宿主定时跑确定性工具。

仅放一个高 priority 文案的 `SKILL.md` **不符合**市面已验证的保证模型。

---

## Q2. Obsidian 式知识管理，值得抄的是什么？

### 结论

不要抄 Obsidian 产品本身。值得抄的是 **「可编译、可审计的知识层」** 最小要素（与 Karpathy LLM Wiki 模式、本仓 ADR-0004 一致）：

| 要素 | 收益 | 维护成本 | 本仓现状 |
| --- | --- | --- | --- |
| **注册表（元数据在 SQLite，正文在文件）** | 可查询角色/状态/链接 | 扫描与 upsert | **已有** `knowledge-indexer scan` |
| **角色与目录约定** | Agent 知道读什么 | 分类规则要保守 | **已有** `classify.rs`（目录优先；不确定 → `unknown`） |
| **显式 supersession / 归档默认不可检索** | 陈旧可退役且不污染默认视图 | 需显式链接；禁止「零入链=可删」 | **已有** supersession 提取 + ADR-0004 §5 archive 默认排除 |
| **入口视图 / 渲染 handoff** | 降低「手写状态」腐烂 | 只能渲染已注册事实 | **已有** `render_handoff_view` / `render_status_view`，且诚实列出「替不掉什么」 |
| **台账 byproduct（dispatch/complete/verdict）** | 在途与验收不靠记忆 | 控制器必须调用 CLI | **已有** `task` / `verdict` / `ledger status` |
| **机械陈旧检测** | 文档与台账对得上 | 模式有限 | **已有** `ledger stale`（3 类 + fresh 负控） |
| 单文件单事实 vs 长文档 | 原子事实易校验 | 写成本高 | **未强制**；execution 仍是长报告为主 |
| 双向链接 / 反向链接 | 发现关联、孤儿 | 手维链接贵；纯 LLM 推断不可信 | **审计用文本入链计数**；几乎无 `[[wikilink]]` 体系（见 2026-08-08 知识层调研） |
| frontmatter（类型/时间/来源） | 机器可读生命周期 | 全量回填贵 | **docs 正文历史上基本无 frontmatter**；权威在 registry 行而非文件头 |
| 全文检索 | 找内容 | 不解决「是否过时」 | 非 knowledge-indexer 职责；未做 |
| **陈旧检测（事实级）** | 知道「写错了」 | 需要可执行 oracle | **仅 handoff↔ledger 与审计桶**；**不能**校验「手册里 Grok 登录状态」这类外部事实 |

外部模式补充（非本仓实现）：

- Karpathy LLM Wiki：raw 源不可变 → LLM 编译 wiki → 周期 lint（矛盾、孤儿、陈旧声明、缺页）。  
  证据：https://gist.github.com/karpathy/442a6bf555914893e9891c11519de94f  
- 社区实践强调：`hot.md`/index 作入口、`last_updated` frontmatter、定期 contradiction-check——**都是调度 + 机械/半机械 lint，不是双链本身**。

### `knowledge-indexer`：现有能力 / 缺口清单

**包位置**：`crates/knowledge-indexer`（bin `knowledge-indexer`）。  
**产品定位**（`src/lib.rs` 文件头）：classify / register / audit-only / handoff render / ledger CLI / stale / retirement **candidates**；**永不自动删改 docs 生命周期**。

#### 能做什么（有代码与 CLI 证据）

| 能力 | 入口 | 证据 |
| --- | --- | --- |
| 扫描 `docs/**/*.md` 并注册到 SQLite | `knowledge-indexer [scan]` | `src/bin/knowledge_indexer.rs` `run_scan`；`src/scan.rs` |
| 角色/状态分类（保守；不确定 → unknown） | scan 内 `classify_document` | `src/classify.rs` |
| 内容 digest 变更检测 | scan 摘要 `digest_changed` | `src/scan.rs` / `src/digest.rs` |
| 显式 supersession 提取 + 链接 | scan | `src/supersession.rs` |
| 审计报告：矛盾、孤儿分桶、未配对 brief、superseded 仍被引用、不可分类、retirement candidates | scan 报告 | `src/audit.rs`（verification/decision 零入链标为 **expected orphan，非问题**） |
| 状态视图 / 合成 handoff 渲染 | scan stdout 或 `--handoff`；`render_*` | `src/render.rs`；§5「本渲染仍替不掉什么」 |
| 派单 / 完成 / 验收写入台账 | `task dispatch` / `task complete` / `verdict record` | CLI + `src/ledger.rs` |
| 台账状态 | `ledger status` | 在途 / 待验收 / 最近结论 / status inventory |
| handoff vs 台账陈旧 | `ledger stale --doc` | `src/stale.rs`：3 类 finding；**fresh → exit 0**；**STALE → exit 3**；负控测试 `consistent_document_not_flagged` |
| 隐私边界 | status/stale 输出 | 仅 slot label、title、task_id、portable path；禁 lease/session/host path |

#### 不能做什么 / 诚实缺口（代码自承或实测）

| 缺口 | 说明 | 证据 |
| --- | --- | --- |
| **不**自动写 handoff / 不改 docs | 仅 report-only | `stale.rs` / CLI help |
| **不**自动 archive | K1a 需显式 storage 调用 | `lib.rs` |
| **不**校验事实性声明 | 无法验证「Grok 已登录」「门已绿」是否为真 | 无对应模块；属外部 oracle |
| **不**对比验收结论与 git 提交内容 | verdict 只存 summary/gate/e2e 字符串 | `ledger.rs` / storage 形状 |
| **不**判断「在途是否其实早已完成」除非：状态被 complete、或 handoff 数字不一致 | 旧 `dispatched` 会一直挂着 | 台账实测（下节） |
| 渲染 **替不掉** 情景叙事、git HEAD、runbook、坑日志、排序阅读清单 | 固定表 | `render.rs` `render_cannot_replace_section` |
| 无调度 / 无守护进程 | 必须有人/控制器调用 | CLI 形态 |
| Pillow 产品 UI **未**绑定该 CLI 为内置一键 | 槽位计划与 indexer 分离 | `docs/superpowers/plans/2026-08-11-slots-foundation.md` 无 knowledge skill 耦合 |
| 全文语义检索 / 双链图 / frontmatter 权威 | 不在范围 | 对比 2026-08-08 知识层调研 |

#### 台账与 handoff 实测（2026-08-12，只读命令）

命令（相对仓库根；**未改库**）：

```text
knowledge-indexer ledger status --db .superpowers/ledger/pillow.db --project pillow
knowledge-indexer ledger stale  --db .superpowers/ledger/pillow.db --project pillow \
  --doc docs/handoff/2026-08-11-pillow-controller-handoff.md
```

**实际输出摘要**：

| 指标 | 值 |
| --- | ---: |
| `dispatched`（在途） | **7** |
| `completed` without verdict（待验收） | **9** |
| 最新 handoff 声称 | **在途 Worker:0**（`docs/handoff/2026-08-11-pillow-controller-handoff.md`） |
| `ledger stale` | **STALE，10 findings**，exit **3** |
| findings 构成 | 1× `inflight_count_mismatch` + 9× `completed_without_verdict_unmentioned` |

在途 7 条中，标题时间上明显「可能早已结束但仍是 dispatched」的包括（台账打印的 `dispatched_at` 跨 08-09～08-11）：

- Permission rules UI / core（round21）
- Make the working tree publishable（round20）
- Changes panel …（round32）

另 3 条为当日 round35 派单（含本调研任务）。  
用户口述「4 条陈旧在途」与「旧 round 未 complete 的 dispatched」数量级一致；**精确数字以本次 status 的 7 为准（含当前真实在途）**。

### 还缺什么（相对「信息管理员自主监控」）

**不要重建** scan/ledger/stale。缺的是：

1. **调度**：谁周期跑 `status`/`stale`/`scan`，产物落到哪。  
2. **「在途超时」规则**：`dispatched_at` + 完成信号/工作树证据 → complete 或 flagged（今日 **未实现**）。  
3. **事实探针类检查**（登录态、门结果）——**indexer 范围外**，需独立 deterministic probes。  
4. **验收 vs 提交** 的结构化关联（commit hash 进 verdict / task）——**今日无字段**。  
5. **优先可读的审计 skill / 内置路径**——见 Q4。

---

## Q3. 「信息管理员自己去监控」具体监控什么？

### 结论

信息管理员在 ADR-0004 中的定位是 **auditor，不是 scribe**（`docs/decisions/ADR-0004-knowledge-layer-actors-and-registration.md` §4）。  
监控应落成 **可判定检查项**：每项必须同时能输出 **PASS（没问题）** 与 **FAIL（有问题）**。  
只报红、永远找不到绿的检查 = 假绿测试同构，应拒绝。

下列检查项优先 **复用已有 CLI**；新增项标明「今日无实现」。

### 检查项表

| ID | 检查项 | 输入 | 判据 | PASS 输出 | FAIL 输出 | 今日实现 |
| --- | --- | --- | --- | --- | --- | --- |
| **C1** | 在途计数与 handoff 一致 | handoff markdown 文本 + ledger snapshot | 每个 `在途 Worker: N`（及 stale.rs 认可变体）= `count(status=dispatched)` | `fresh` 报告段；exit 0；无 `inflight_count_mismatch` | `inflight_count_mismatch`：claimed vs ledger_dispatched + excerpt；exit 3 | **`ledger stale` 已有** |
| **C2** | 待验收不被 handoff 静默 | ledger `completed_without_verdict` + handoff 文本 | 每个待验收任务的 title 或 task_id 在 handoff 中被提及 | 无 `completed_without_verdict_unmentioned`；若待验收列表空，status 明确 `_None — every completed…_` | 每条未提及任务：title/task_id/slot + 行动建议 | **已有** |
| **C3** | handoff 不提及幽灵任务 | handoff 显式 task 提及 + ledger 全任务 | 每个 `task-title`/`task-id`/表格提及 ∈ ledger | 无 `doc_task_missing_from_ledger` | 提及值 + excerpt；exit 3 | **已有** |
| **C4** | 台账在途清单可枚举 | 仅 ledger | `ledger status` 打印 Count 与表（含 Count=0 的 **无在途** 文案） | Count=N 且表完整；N=0 时明确 empty | 命令失败 exit 1（非「假装无在途」） | **`ledger status` 已有** |
| **C5** | 陈旧在途（超时仍 dispatched） | ledger 每条 `dispatched` 的 `dispatched_at` + 墙钟阈值 T；可选 completion_signal/报告路径存在性 | 若 `now - dispatched_at > T` 且无 complete/verdict → FAIL；否则 PASS | `stale_inflight=0` + 仍在 SLA 内的列表 | 列出超时 task_id/title/slot/age | **未实现**（今日只有状态枚举，无超时策略） |
| **C6** | 文档注册与审计桶 | `scan` 对 docs 的 registry + audit | 规则集合各自有绿：例如 unpaired brief 列表可为空；contradictions 可为空；unknown_ratio 可报告为 0% | 报告含 total、role/status 分布、空桶明确为空 | 非空桶列出路径；unknown_ratio 超阈（阈值若产品定义） | **scan/audit 已有形状**；阈值告警策略未产品化 |
| **C7** | supersession / 退休候选诚实 | registry links + 显式 supersedes | retirement_candidates **仅**来自已解析的显式 supersedes；零入链 verification/decision **不**进退休 | 桶分离正确（expected orphans 标 non-problem） | 若未来实现误把 expected orphan 当退休 → FAIL（需防回归测试） | **audit 策略已有** |
| **C8** | 交接手册「环境事实」仍成立 | 手册中可解析的事实探针声明 + 探针命令输出 | 例：声明「provider 未登录」时，登录探针必须失败；声明「已登录」时必须成功 | `claim=X probe=Y match` | `claim=X probe=Y mismatch`（**正是「Grok 未登录」误导上手的失败模式**） | **未实现**；需 claim 语法 + 探针 |
| **C9** | 验收结论与提交对齐 | verdict 行（outcome/summary/gate）+ 关联 commit/range（需结构化字段） | accepted ⇒ 声明的路径变更存在于目标 commit；rejected ⇒ 不声称已合入 | `verdict_id` 对齐 PASS | 声明文件/测试在 commit 中不存在或相反 | **未实现**（verdict 无 commit 字段） |
| **C10** | 渲染 handoff 与手写 handoff 双轨差 | `render_handoff_view` 输出 vs 手写 handoff 或仅以渲染为准 | 若产品切换「handoff 以渲染为准」：渲染 §1–§3 与 ledger 一致即为 PASS | 渲染在途/待验收与 status 一致 | 手写与渲染冲突（今日 stale 已捕获手写侧） | **部分**：渲染一致性靠纯函数；手写差靠 C1–C3 |

#### 负控原则（写进角色契约）

每个检查的测试/实现必须包含：

- **构造 PASS 夹具**（例如 `stale.rs` 的 `consistent_document_not_flagged`）。  
- **构造 FAIL 夹具**（三类 stale 正例已有）。  
- **禁止**「找不到问题就算过」而无空结果断言。

### 与用户候选的映射

| 用户原话 | 对应检查 |
| --- | --- |
| 文档与代码脱节 | C6 +（未来）报告路径/digest vs 代码符号——**符号级脱节今日未做** |
| 交接手册事实仍成立 | **C8**（高优先级产品痛点；indexer 外） |
| 验收 vs 提交 | **C9** |
| 台账在途早已完成 | **C5** + 人工/控制器 `task complete` 纪律；今日 7 条 dispatched 中有多条跨日 |

---

## Q4. 槽位角色 vs 内置能力？

### 两种做法

| 维度 | A. 槽位角色（用户配 CLI/key，对话跑监控） | B. Pillow 内置（不依赖外部 provider） |
| --- | --- | --- |
| **新用户零配置** | 否：无槽位/无 key 则无监控 | 是：安装即有 `knowledge-indexer` 类命令/按钮 |
| **token 成本** | 每次审计烧模型；易在上下文压力下跳过 | 机械检查 **0 token**；可选 LLM 只解释报告 |
| **确定性** | 依赖模型是否加载 skill、是否偷懒 | exit code + 结构化报告；可进 CI/门 |
| **失败可理解性** | 「模型说没事」难追；假绿风险高 | `STALE (10 finding(s))` / exit 3 可脚本化 |
| **与 ADR-0004** | 审计可用槽位做深读 | **注册 byproduct + 机械审计** 本就应在路径上，不依赖「记得叫信息管理员」 |
| **优先级（Q1）** | 用户 skill 膨胀后，角色 skill **可能不再被触发** | 内置调度不走 description 彩票 |
| **深度判断** | 强：矛盾叙事、跨文档语义 | 弱：仅机械规则；深度仍要人/总控 |
| **成本/复杂度** | 产品已有 slots 方向，但绑定角色策略 | 已有 crate；缺 UI/调度/包装 |

### 推荐

**混合，但默认重心在 B：**

1. **内置机械监控（必须先有）**  
   - 包装已有：`ledger status`、`ledger stale`、`scan` 审计。  
   - 产出固定报告路径 + exit code（0/3/1）。  
   - 可选：定时/打开项目/总控回合开始时自动跑。  
   - **不依赖任何对话槽位。**

2. **信息管理员作为可选「角色标签」+ skill（增强，非唯一路径）**  
   - skill 正文只教：如何读报告、何时升级给总控、禁止把审计当登记。  
   - 若走 Grok skill 形态：放 **repo `.grok/skills/`** 以覆盖 user 同名；但 **仍不能**替代 (1)。  
   - 深度语义审计（C8/C9）由总控或专用槽位在 **机械报告非空时** 触发。

3. **不要**把「登记台账」做成信息管理员职责（ADR-0004 已否决 scribe 主路径）。

判据对齐用户关心点：

- 无槽位新用户：**能用**（B）。  
- 监控是否费 token：**默认不费**（B）；解释性对话可选。  
- 失败是否看得懂：**exit 3 + 分条 finding**（已有 stale 格式）。

---

## 推荐：如果只做一件事

### 先做

**把已有知识台账检查产品化为「一键/定时审计」内置能力**（仍零新算法）：

- 固定调用：`ledger status` + 对当前权威 handoff（或渲染视图）的 `ledger stale` + 可选 `scan` 审计摘要。  
- 固定双态输出：**全部 PASS** vs **带 finding 列表的 FAIL**（沿用 exit 0/3）。  
- 把「在途 7 / 待验收 9 / handoff 写 0」这种 **今天已可自动发现** 的腐烂，从「总控记忆」变成「打开即见」。

### 为什么不是先做 skill

1. Q1 证据表明 skill **不保证**在 skill 膨胀后仍被读到；Codex 甚至会省略列表项。  
2. 机械检查 **已实现且有负控**；缺的是调度与产品入口，不是再写一份 SKILL.md。  
3. ADR-0004：信息管理员是 **auditor**；审计的第一刀应是确定性工具。  
4. 用户优先级句「内置 skill 优先」在工程上应翻译为 **内置 always-available 能力 + catalog 常驻**，而不是再赛一段 description。

### 第二步（非本单实现）

- C5 陈旧在途超时策略。  
- C8 手册事实探针语法。  
- 可选：repo 内 `librarian` skill **只**指向如何解读上述报告（且接受：它不能单独保证优先级）。

---

## 残余未知 + 怎么才能拿到

| 未知 | 为何未知 | 如何拿到 |
| --- | --- | --- |
| Grok 是否在 skill 极多时截断初始列表 | README 未写 Codex 式 2% 预算 | 构造 100+ 假 skill 后 `grok inspect` / 会话 skill 列表对照 |
| Pillow ACP 路径下 CLI 是否仍加载 `.grok/skills` 与 AGENTS | `docs/research/2026-08-10-agent-context-gap.md` 标未 live 复证；且 isolated HOME 改规则源 | 实盘 ACP 抓 system/user 前缀或 `grok inspect` 在 pillow session cwd 下 |
| Claude Code enterprise>personal>project 是否全版本一致 | 文档与社区对 project/personal 谁优先有过歧义叙述 | 以 code.claude.com 当前页 + 同名 skill 对照实验为准 |
| 台账 7 条 dispatched 中哪些「业务上已完成」 | 无 complete 调用则机械上仍是在途 | 总控对旧 task 做 `task complete`/`verdict` 或 C5 策略 |
| C8/C9 的权威探针集 | 产品未定义 claim 语法 | 设计短表：登录/门/E2E/HEAD 等允许的 claim → 命令映射 |
| 用户是否要把 handoff **完全**改为渲染视图 | 渲染自承大量「替不掉」 | 产品决策：双轨多久、何时删手写在途段 |

---

## 证据索引（可复查）

| 主题 | 位置 |
| --- | --- |
| ADR 信息管理员=审计员 | `docs/decisions/ADR-0004-knowledge-layer-actors-and-registration.md` |
| 知识层量化与过期手段 | `docs/research/2026-08-08-knowledge-layer-research.md` |
| K0e CLI / stale 语义 | `docs/execution/2026-08-09-k0e-ledger-cli-report.md`；`crates/knowledge-indexer/src/stale.rs` |
| CLI 子命令 | `crates/knowledge-indexer/src/bin/knowledge_indexer.rs` |
| 渲染诚实缺口 | `crates/knowledge-indexer/src/render.rs` `render_cannot_replace_section` |
| 台账路径约定 | `docs/handoff/2026-08-11-pillow-controller-handoff.md`（ledger status 命令行） |
| 本次 ledger 实测 | 本报告 Q2 实测节（dispatched=7, pending=9, stale exit=3） |
| Grok skill 发现优先级 | `~/.grok/README.md` § Skills（本机只读；报告不写绝对路径） |
| Claude / Codex / standard | 上文 URL |
| Pillow 不注入 skills | `docs/research/2026-08-10-agent-context-gap.md` §2 |

---

## 完成信号

`LIBRARIAN-SKILL-COMPLETE`
