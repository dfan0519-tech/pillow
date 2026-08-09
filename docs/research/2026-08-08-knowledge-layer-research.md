# Pillow Stage 2 知识层调研（只读）

Date: 2026-08-08  
Status: **Research complete — evidence for controller red-line decisions**  
Owner: knowledge-research worker  
Controller: Claude  
Work directory: repository root (`pillow`)  
Mode: **read-only** (no source/tests/builds; this file is the only write)

---

## 0. 结论摘要（给总控）

| 问题 | 证据结论 |
| --- | --- |
| `docs/` 是否已是「知识层问题的活样本」？ | **是。** 11 个日历日内 **287** 个 markdown；约 **46.7%** 文件在其它文档中零引用；淘汰靠人手写「本文件取代 X」。 |
| 过期如何识别？ | 几乎只有 **显式 supersession 句** + **新 handoff 覆盖旧 handoff**；无 front-matter、无 `last_verified`、无衰减分、无 wiki 双链。 |
| SQLite 能否承载知识层？ | **不能直接承载正文**；现有 0001–0009 是项目/会话/工具/提案/设置。可 **复用** project 隔离、proposal 收据、settings 的 scope/precedence **形状**。 |
| 红线 1（SQLite 权威 / Obsidian 投影） | **建议倾向采纳**，但必须把「投影延迟」与「人类手改 vault」冲突写进产品；反对方的核心风险是双写撕裂。 |
| 红线 2（自动淘汰 = 标记+归档；真删审批） | **建议采纳**；业界与本仓 vault schema 都更支持 archive-not-delete。反对方的代价是存储与检索噪声，可用检索排除缓解。 |
| 最小切片 | **K0：文档注册表（SQLite 元数据）+ 固定路径/角色约定 + 只读 lint（superseded/orphan/brief-without-report）**；不做 Obsidian 挂载、不做自动删除、不做写回。约 2–3 轮可关门。 |

完成标记见文末。

---

## 1. 调研范围与方法

### 1.1 必读已覆盖

- `docs/handoff/2026-08-08-pillow-controller-handoff.md` §2 产品终点与两条红线  
- `docs/decisions/ADR-0003-settings-authority-and-s0-scope.md`  
- `AGENTS.md`（仓库）与共享 vault `AGENTS.md`（只读依赖，未修改）  
- `crates/storage-sqlite/migrations/0001`–`0009`  
- `docs/` 全树量化扫描（文件枚举、命名、交叉引用、supersession）  

### 1.2 方法边界

| 做了 | 没做（硬禁止） |
| --- | --- |
| 读源码/迁移/文档 | 改任何源码/样式/测试/脚本 |
| 目录枚举与文本交叉引用统计 | `cargo` / `pnpm build` / gate / 测试 / E2E |
| 外部网页检索（Obsidian 归档、Karpathy LLM Wiki） | `git add/commit/stash/reset` |
| 只读浏览共享 vault 结构与 `AGENTS.md` | 修改任何 out-of-tree vault / legacy Keeper 内容 |

### 1.3 引用计数方法（避免误读）

- **入链**：在其它 `docs/**/*.md` 中出现目标文件 **basename（无扩展名）** 计 1 次文档级提及。  
- **局限**：漏计仅写相对路径片段、仅写短 ID 未含文件名的情况；ADR 另用 `ADR-000N` 短 ID 复核。  
- **不是** git blame / 运行时读取日志；纯静态语料。

---

## 2. §3.1 本仓库现状诊断（量化）

### 2.1 规模与目录分布

| 路径 | markdown 数 | 角色（从内容与命名推断） |
| --- | ---: | --- |
| `docs/execution/` | **220** | Worker 简报 / 交付报告 / 诊断（主体积） |
| `docs/verification/` | **26** | 波次验收与 checkpoint |
| `docs/superpowers/` | **18** | plans + specs（设计与实施计划） |
| `docs/research/` | **12** | 调研（本文件写入前） |
| `docs/handoff/` | **4** | 总控交接（真相源链条） |
| `docs/decisions/` | **3** | ADR |
| `docs/roadmap/` | **2** | Stage 1 计划/状态 |
| `docs/migration/` | **1** | 遗留能力清单 |
| `docs/references/` | **1** md + **1** json | 上游基线 / ACP 锁 |
| **合计** | **287 md + 1 json = 288** | handoff 所称「261+」已过时；现 **287** md |

**体量特征**

- 行数：min 22 / median **79** / max **869**；合计约 **33 418** 行。  
- 最大文件集中在 `docs/research/*` 与长 design（如 settings 设计 ~37KB）。  
- **无** YAML front-matter：`docs` 下 **0/287** 文件以 `---` 开头。  
- **无** Obsidian 双链体系：`[[...]]` 出现约 **1** 次；`*.md` 相对链接约 **25** 次。  
- **无** 文件名级 tag 约定（`#` 出现 0）。

### 2.2 命名约定：大体一致，角色堆叠

| 约定 | 符合度 | 证据 |
| --- | --- | --- |
| 日期前缀 `YYYY-MM-DD-` | 高 | 有日期前缀 **281**；无日期前缀 **6**（3 ADR + `legacy-capability-inventory` + `upstream-baseline` + `worker-gate-entry`） |
| `execution` 全量日期前缀 | **220/220** | 一致 |
| wave / c3-c4 切片命名 | 高 | `execution` 中 `wave-` **197**；c3/c4/slice 相关 **192** |
| brief / report 双轨 | 半一致 | brief **115**、report **96**（含 verdict 类约 98）；**同 stem 配对** brief→report **72**；brief 无同 stem report **43**；report 无同 stem brief **24** |
| 中英混排 | 常见 | handoff/部分 verdict 中文；多数 execution 英文 |

**解读**：这不是「混乱的文件夹」，而是 **高吞吐的执行日志库**。问题不是「找不到文件名规则」，而是 **没有生命周期角色（active / superseded / archive）**，于是 220 个 execution 文件对 Agent 一视同仁地抢上下文。

### 2.3 时间分布与衰减速度

| 日期 | 当日新增带日期前缀文件数 |
| --- | ---: |
| 2026-07-29 | 3 |
| 2026-07-30 | 15 |
| 2026-07-31 | 41 |
| **2026-08-01** | **66**（峰值） |
| 2026-08-02 | 46 |
| 2026-08-03 | 13 |
| 2026-08-04 | 41 |
| 2026-08-05 | 23 |
| 2026-08-06 | 2 |
| 2026-08-07 | 7 |
| **2026-08-08** | **24** |

- 跨度：**11** 个日历日（07-29 → 08-08）。  
- 日均（仅有日期前缀）：约 **25.5** 文件/天。  
- 峰值日约 **66** 文件：在单日多 Worker 并行下，**「文档比记忆更新还快」** 是常态而非例外。

#### 同日衰减的活证据（非推测）

**证据 A — 显式 supersession 链（handoff）**

| 文件 | 声明 |
| --- | --- |
| `docs/handoff/2026-08-07-pillow-controller-handoff.md` | 本文件取代 `2026-08-01-…` |
| `docs/handoff/2026-08-08-pillow-controller-handoff.md` | 本文件取代 `2026-08-07-…` **与** `2026-08-08-round5-tech-debt-status.md` |

同一天内，`round5-tech-debt-status` 与新 handoff **同时存在**，且后者宣布前者过时。

**证据 B — 同日事实冲突（段落级过期）**

| 文件 | 对 `cancel-tree` 的陈述 |
| --- | --- |
| `docs/handoff/2026-08-08-round5-tech-debt-status.md` §1 | 「`cancel-tree` 根因仍未知」 |
| `docs/handoff/2026-08-08-pillow-controller-handoff.md` §4.5 | 「**高置信, harness bug** … **已修**」 |

两文件同日、同主题、**互相矛盾**。这不是「过几个月变旧」，而是 **交接窗口内段落已死**。  
同日还有 `flake-analysis-report` / `flake-analysis-VERDICT` / `harness-flake-fixes-report` / `storage-barrier-verdict` 等并行产物——**权威靠人脑排序**，机器无法知道该信哪份。

**证据 C — 过期识别手段盘点**

| 机制 | 本仓 `docs/` 现状 |
| --- | --- |
| 人手「本文件取代 X」 | **仅 handoff 链 + 极少数 report 句**（中文 supersession **2 处图**；英文 “supersedes prior” 零星） |
| front-matter `status` / `superseded_by` | **0** |
| `last_verified` | **0** |
| 衰减分 / 归档目录 | **无**（`docs/archive/` 不存在） |
| 索引页（类似 vault `index.md` / `hot.md`） | **无** |
| 双链图 / 反向链接 | **无**（链接极少） |

**可量化的「已显式过期」集合（今天就能机械认出）** 只有：

1. `docs/handoff/2026-08-01-pillow-controller-handoff.md`  
2. `docs/handoff/2026-08-07-pillow-controller-handoff.md`  
3. `docs/handoff/2026-08-08-round5-tech-debt-status.md`  

其余 **大量实质过期**（旧 wave 状态、被 fix 报告推翻的 residual risk、过时「在途 Worker」叙述）**没有机器可读标记**。

### 2.4 交叉引用：谁从未被提、谁被反复提

#### 直方图（文档级入链）

| 入链数 | 文件数 | 占比 |
| ---: | ---: | ---: |
| **0** | **134** | **46.7%** |
| 1 | 85 | 29.6% |
| 2–3 | 38 | 13.2% |
| 4–9 | 28 | 9.8% |
| 10+ | 2 | 0.7% |

#### 零引用按目录

| 目录 | 零引用数 | 说明 |
| --- | ---: | --- |
| `execution/` | **119** | 主体；其中 brief 占绝对多数 |
| `verification/` | 6 | 见下表 |
| `superpowers/` | 5 | 多为旧 plan |
| `decisions/` | 2 | ADR-0001/0002 按 **basename** 零引用（见短 ID 复核） |
| `research/` | 2 | 早期协议/边界调研 |

#### 零引用按文件名角色

| 角色 | 零引用数 |
| --- | ---: |
| `*brief*` | **101** |
| `*report*` | 13 |
| plan/implementation | 7 |
| 其它 | 13 |

**解读**：零引用 **不等于垃圾**。`brief` 是「派发合同」；验收后通常只被对应 report/handoff 间接消费，**入链为 0 是结构预期**。但若 Agent 把 101 份 brief 与 ADR 同等加载，就会制造上下文污染——这正是「自动淘汰」要解的问题。

#### 反复引用（Top，basename 入链）

| 入链 | 文件 |
| ---: | --- |
| 12 | `docs/references/upstream-baseline.md` |
| 11 | `docs/roadmap/2026-07-31-stage1-overall-status.md` |
| 9 | `docs/superpowers/specs/2026-08-05-c4-git-changes-design.md` |
| 8 | `docs/superpowers/specs/2026-08-01-c3-governed-write-design.md` |
| 7 | `docs/superpowers/specs/2026-07-29-pillow-first-design.md` |
| 7 | `docs/superpowers/specs/2026-08-02-c4-governed-process-git-design.md` |
| 7 | `docs/superpowers/plans/2026-08-02-c4-governed-process-git-implementation.md` |
| 7 | `docs/execution/2026-08-01-wave-c3-integration-report.md` |
| 6 | `docs/verification/2026-08-01-wave-c2-checkpoint.md` 等 |

**模式**：被反复引用的是 **设计权威（specs）**、**路线图**、**上游基线**、**checkpoint/关键 integration report**——不是海量 brief。

#### ADR 短 ID 复核（避免 basename 误杀）

| ID | 提及该 ID 的文档数（含自身） |
| --- | ---: |
| ADR-0001 | 1（基本只有自身） |
| ADR-0002 | 1 |
| ADR-0003 | 5 |

**发现**：Accepted ADR 中 **0001/0002 几乎不被后续文档引用**。不是「无用」，而是 **决策未进入可检索知识图**——Agent 不会因为它们存在就加载；人靠记忆。Stage 2 若不做索引/注册表，ADR 会变成「写了等于没写」。

#### 零引用但 **不可当垃圾** 的样本（误杀高风险）

- `docs/decisions/ADR-0001-clean-successor.md`  
- `docs/decisions/ADR-0002-pillow-identity-boundary.md`  
- `docs/research/2026-07-30-grok-acp-protocol.md`  
- `docs/research/2026-07-30-grok-adapter-boundary.md`  
- `docs/verification/2026-07-30-slice-b2-acceptance.md` 等 6 份验收  
- `docs/execution/2026-08-08-storage-barrier-verdict.md` 类 **方法论样本**（handoff 点名「值得复用」）  
- `docs/superpowers/plans/2026-07-29-slice-a-desktop-foundation.md`  

### 2.5 若今天就上「自动淘汰」，会杀掉谁？会误杀谁？

以下是 **假设性策略模拟**（标注 **推测：策略本身**；命中列表基于今日静态扫描 **查证**）。

#### 策略 H1 — 仅信任显式 supersession（安全）

| 动作 | 文件 |
| --- | --- |
| 标记 `superseded` / 可归档 | `handoff/2026-08-01-…`、`handoff/2026-08-07-…`、`handoff/2026-08-08-round5-tech-debt-status.md` |
| 真删除 | **无**（若遵守红线 2） |

**误杀风险：极低。** 覆盖面：极窄（3 文件）。

#### 策略 H2 — 「零入链 + `*brief*`」→ 归档（激进但仍可辩护）

| 指标 | 数 |
| --- | ---: |
| 候选归档 | 约 **101** 个 brief |
| 其中有同 stem report 的 brief | 配对全集 72 中大部分会落入此集（brief 常零入链） |

**合理归档**：已交付 wave 的 action brief（合同已执行完毕）。  
**误杀风险：中**

- **43** 个 brief **无同 stem report**：可能是未完成工单、改名 report、或合并进其它 report——归档会抹掉「未完成证据」。  
- 少数 brief 可能仍是 **唯一任务说明**（无 report 时）。

#### 策略 H3 — 「零入链 ⇒ 过期」（错误）

| 候选 | ~**134** 文件 |
| --- | --- |
| 必误杀 | ADR-0001/0002、关键 research、部分 verification、部分 plan、当日尚未被 handoff 引用的 verdict |

**不可采用。** 零入链在本仓 **首先是 brief 生命周期形态**，不是垃圾信号。

#### 策略 H4 — 「超过 N 天」纯时间衰减（对本仓过早）

全库仅 **11** 天历史。N=7 会把 **07-29～07-31** 的 foundation 验收与早期 research 整批打成 aging，同时 **08-08 同日冲突** 完全抓不到。  
**时间衰减对本仓现阶段几乎无效；supersession 与角色（brief/report/handoff/ADR）才有效。**

#### 推荐的「今日可自动做」集合（不删除）

| 优先级 | 自动动作 | 对象规模 | 依据 |
| --- | --- | ---: | --- |
| P0 | 标记 `superseded` | 3 | 显式「本文件取代」 |
| P1 | 标记 `role=brief` + 若存在同 stem report 则 `status=consumed` | ~72 | 命名约定 + 配对 |
| P2 | 标记 `role=brief` + `status=orphan_brief`（需人审） | ~43 | 无配对 report |
| P3 | 标记 `unreferenced` 供检索降权，**不**归档 | ~134 含 ADR 等 | 仅信号 |
| 禁止 | 物理删除任何上述集合 | — | 无收据、无审批则不可逆 |

### 2.6 诊断一句话

> Pillow 的 `docs/` 是 **高写放大的执行语料库**，不是治理过的知识库：有日期与 wave 命名，**没有** 状态、替代边、索引与衰减；过期靠手写 supersession，且 **同日冲突已发生**。Stage 2 的第一用户问题不是「如何嵌入 Obsidian UI」，而是 **如何让 harness 知道哪 10 份该读、哪 200 份不该读**。

---

## 3. §3.2 外部实践调研（查证 vs 推测）

### 3.1 查证：共享 vault schema（本地只读依赖）

路径语义：共享知识库根下的 `AGENTS.md` / `index.md` / `hot.md` / `wiki/` / `archive/` / `scripts/`（**未修改**）。

| 机制 | 解决什么 | 是否依赖人类持续维护 |
| --- | --- | --- |
| 固定路径 `wiki/{projects,entities,concepts,decisions}` | 角色与检索入口 | 低（约定可机械校验） |
| front-matter：`status` / `last_verified` / `decay_score` / `superseded_by` | 生命周期机器可读 | **中高**：字段要人/Agent 写对 |
| `hot.md` L0 + `index.md` L1 渐进披露 | Token 纪律、先索引后全文 | **高**：需 ingest 后刷新 |
| QUERY → INGEST → LINT → ARCHIVE → DECAY | 闭环 | Lint/Archive **设计上**脚本化；**查证**：`index.md` 写「归档页面：0」——规则在、执行弱 |
| 归档 ≠ 删除（移到 `archive/`，原位存根） | 可恢复、检索可排除 | 中 |
| 衰退公式 `time + ref + tech` | 保鲜排序 | 依赖 `last_verified` 与引用统计质量 |

**对 Pillow 的可迁移点（查证导向）**

1. **角色分目录 / 分 role** 比「全塞 docs/」更利于自动策略。  
2. **`superseded_by` + 归档存根** 直接对应红线 2。  
3. **L0/L1 索引** 是「自动填充」的最小形态（不是全文灌进 prompt）。  
4. vault 自称整合 Karpathy LLM Wiki + claude-obsidian + obsidian-sync——与 Stage 2 方向同构，但 **Pillow 不得整库复制**；应通过契约重做。

### 3.2 查证：Karpathy LLM Wiki（公开 gist / 社区二次阐述）

公开材料描述的核心模式：

- 知识是 **编译产物**（wiki），不是每次 query 重读 raw。  
- 操作：**ingest**（增量写入结构化页）、**query**、**lint**（矛盾、孤儿页、缺页）。  
- 人负责 sourcing 与提问；LLM 负责维护交叉引用与综合。  
- 痛点原话方向：**bookkeeping 比思考更累**——与 Pillow handoff 抱怨一致。

**可迁移**：ingest/lint 由 harness 做；**不可直接迁移**：假定「LLM 可自由改 vault 文件」——与 Pillow C3 审批模型冲突，必须改成 **proposal 写回** 或 **仅写注册表**。

### 3.3 查证：Obsidian / PKM 社区对陈旧内容

| 做法 | 来源类型 | 要点 |
| --- | --- | --- |
| 独立 `_archive` 文件夹 + 搜索排除 | 博客/实践文 | 退役 ≠ 删除；查询时 `-path:_archive` |
| archive, don't delete | 论坛/Reddit 共识倾向 | 删库后悔成本高 |
| fleeting → processed → archive tag | 永久笔记 vs 闪念 | **高度依赖人类**整理习惯 |
| Note archiver 等插件 | 插件生态 | 手动/半自动挪动 |
| 「活库不是档案馆」持续编辑 | 使用技巧文 | 与工业审计 **部分冲突**（审计要冻结证据） |

### 3.4 概念工具箱：各自解决什么（合成，标推测处）

| 工具 | 解决的问题 | 工业 IDE 适配 |
| --- | --- | --- |
| **固定路径约定** | 角色可预测、脚本可扫 | **高** — 本仓已有弱约定，缺 lifecycle 目录 |
| **front-matter** | 状态/替代/验证日机器可读 | **高** — 本仓 0 使用，是最大缺口 |
| **双链 `[[w]]`** | 发现与反向链接 | **中** — 本仓几乎不用；Agent 更吃 **注册表边** 而非 UI 图谱 |
| **标签** | 软分类 | **低–中** — 易通胀；不如 `role` 枚举 |
| **衰减分** | 检索排序 | **中** — 需事件（打开/引用）喂数据；冷启动弱 |
| **归档目录** | 降噪且可恢复 | **高** |
| **真删除** | 合规/泄密 | **必须审批 + 收据**（产品安全） |

### 3.5 哪些做法依赖人类、因而不适合「harness 自动填充/淘汰」

| 依赖人类 | 原因 | 替代（面向 harness） |
| --- | --- | --- |
| 手工写「本文件取代 X」散文 | 易漏、不可扫 | 结构化 `supersedes:` / 注册表边 |
| 每周人工 lint | 做不到（本仓 25+ 文件/天） | 每次 ingest / CI 只读 lint |
| Zettelkasten 式精炼原子笔记 | 成本高于 wave 交付 | 分离 **raw 执行语料** vs **编译知识页** |
| 纯标签整理 | 无强制 schema | 闭枚举 `role` + `status` |
| 信任「零链接=无用」 | 本仓会误杀 ADR | 角色感知策略 |

### 3.6 推测（明确标注）

1. **推测**：工业用户更接受「项目内 `knowledge/` 投影」而非绑定 Obsidian 商业同步；Obsidian 是 **编辑器/图谱前端**，不是权威存储。  
2. **推测**：若 Stage 2 先做全文 embedding RAG 而不做 supersession 图，会 **放大** 过期 handoff 的检索命中率（旧文更长、关键词更全）。  
3. **推测**：vault `archive=0` 暗示「有 schema 无执行」是常态风险；Pillow 必须把 lint 挂到 **可重复的 host 命令/门**，不能只写 AGENTS 段落。

---

## 4. §3.3 与既有架构的接缝

### 4.1 现有 migrations 能力地图

| Migration | 承载 | 与知识层关系 |
| --- | --- | --- |
| `0001_projects` | 项目身份、`root_path`、trust | **知识对象必须 project-scoped** 的锚点 |
| `0002_conversations` + messages/runs/events | 会话域权威 | 知识 **不是** 会话；但 ingest 可关联 `run_id` 作来源 |
| `0003_tool_calls` | 只读 fs 工具账本 | 模式：reservation + digest + 状态机 — 可类比「知识变更账本」 |
| `0004_resource_recents` | 最近打开路径 | **弱信号**可作 ref_factor；不是知识权威 |
| `0005_write_proposals` + transitions + approval_decisions | C3 写提案/一次性批准/收据 | **知识文件写回** 的直接模板 |
| `0006` / `0008` | run_events 写/进程 kind 扩展 | 可扩展 knowledge 事件 kind（若需要审计流） |
| `0007_exec_proposals` | C4 进程提案 | 知识层一般不直接复用；「跑 lint 脚本」若走 process 则相关 |
| `0009_settings` | global/project 设置、非密钥 | **策略开关**（知识根路径、是否启用投影、衰减阈值）可放 settings |

**查证：migrations 中无 knowledge/wiki/document 表。**  
知识层需要 **新 migration**（建议 `0010+`，以落地时序号为准），而不是挤进 `settings_entries.value_json` 当文档库。

### 4.2 设置层 scope/precedence 能否复用（ADR-0003）

**可复用的形状**

| 设置层概念 | 知识层类比 |
| --- | --- |
| `scope ∈ {global, project}` | 全局知识策略 vs 项目知识树 |
| 部分唯一索引（global key / project key） | 全局模板键 vs 项目文档键 |
| 优先级 `env → project → global → default` | 例如 CI 强制 `PILLOW_KNOWLEDGE_ROOT` 覆盖；**UI 必须显示谁赢**（ADR-0003 教训） |
| 设置 **不** 走 C3（用户第一方） | 用户改「默认检索深度」类偏好 → 直接写 settings |
| 凭据 S1 走 grant+receipt | 类比：敏感知识删除/外发 → 必须走提案 |

**不合身处**

- Settings 值小（`value_json` ≤ 16KiB）且非文档正文。  
- Settings 无 supersession 图、无多版本正文。  
- ADR-0003 明确：设置编辑不进 C3，以免 **冲淡审批信号**；知识 **Agent 写 vault** 则 **必须** 进审批——两者策略相反，不能共用一个「什么都走设置」通道。

### 4.3 治理原语在知识写回路径上的复用

C3 形状（已落地）：

```text
proposal → 一次性人类批准 → host 执行 → 持久收据 → 项目隔离
```

| 知识操作 | 是否套 C3 形状 | 说明 |
| --- | --- | --- |
| 扫描 `docs/` 写入 **SQLite 注册表** | **否**（host 只读扫描） | 类似 settings composition：域权威在 SQLite，不改文件 |
| harness **新建/改** markdown 投影 | **是** | 与 `write_proposals` 同族；path/mode/digest/收据 |
| 标记 `superseded` / `decay`（仅 DB） | **否** | 元数据转换；需审计字段 `updated_by` |
| 文件系统 **归档移动**（rename 到 `archive/`） | **是** 或「受限 host 维护操作 + 收据」 | 改的是树结构，不是纯 DB；应用户红线 2 至少要收据 |
| **真删除** | **是，且应更严**（可单独 decision 类型） | 不可恢复；建议二次确认文案 |
| 人类在 Obsidian 手改文件 | 不经 C3 | 下次扫描出现 **内容 digest 漂移** → 冲突状态；需合并策略（见不确定项） |

**不合身细节**

1. **写提案 content cap**（迁移注释中 262144 字节级）对长 design 可能不够——知识写回可能要 **外置 blob + digest** 或提高 cap（产品决策）。  
2. C3 `mode ∈ {create, modify}`；归档可能是 **rename**——现枚举不够，需扩展或建模为 modify+侧车。  
3. 知识更新频率高（25 文件/天）若每文件弹审批，**审批疲劳** 会毁产品（ADR-0003 对 settings 的同一警告）。  
   - **缓解（推测+设计建议）**：批量 proposal、或「仅编译页/handoff 需批；execution raw 只注册不投影」。  
4. 会话绑定：`write_proposals` 强绑 `conversation_id/run_id`；后台 ingest 可能无 conversation——需 **system actor** 或维护会话模型。

### 4.4 产品顺序约束（查证）

`AGENTS.md` / ADR-0001：Stage 1 IDE → Stage 2 治理 Obsidian → Stage 3 公司操作系统。  
**不得** 在 Stage 2 切片里塞一人公司编排；知识层应服务 **项目上下文治理**，不是第二套 Keeper。

---

## 5. 两条红线：正反理由与选错代价

> 总控待拍板；本节给 **证据与权衡**，不替总控拍板。若认为红线有问题会直说。

### 5.1 红线 1  
**「SQLite 保持域权威，Obsidian 是投影 + 人类可编辑入口，写回走审批路径。」**

#### 支持

1. **与全仓已接受架构一致**：ADR-0001/0003、`AGENTS.md`「Domain state belongs in SQLite」。双权威是 Keeper 遗产痛点，Pillow 为逃离它而建。  
2. **治理可测**：状态机、收据、项目隔离已有 C3/C4 测试文化；文件树当权威则 E2E 变成「扫盘对拍」。  
3. **Obsidian 可替换**：投影意味着用户可换编辑器；框架产品不应把商业/本地 vault 实现写进域内核。  
4. **本仓 `docs/` 现状**：无 front-matter/无索引，**文件不是合格权威**，只是语料。

#### 反对

1. **人类在 Obsidian 的编辑体验期望「保存即真理」**；审批写回有延迟与拒绝，像「假本地」。  
2. **投影延迟 / 冲突**：人改文件 vs SQLite 注册表 digest 不一致时，谁赢？处理不好则双写撕裂。  
3. **实现成本**：同步器、冲突 UI、审批批处理都是真功能，不是开关。  
4. **反方极简替代**：「vault 文件即权威，SQLite 只缓存索引」——更贴经典 PKM，但与 Pillow 安全叙事摩擦。

#### 选错代价

| 若错误 **采纳** 红线 1（其实不该 SQLite 权威） | 若错误 **拒绝** 红线 1（文件当权威） |
| --- | --- |
| 用户恨审批、绕过产品直接改盘且冲突频发；支持成本高 | Agent 与 UI 对「当前真相同」分歧；难做审计/回放；重演 legacy 多含义 JSON/文件；工业交付难言治理 |
| 过早建同步器拖垮 Stage 2 进度 | 安全门与知识门分裂，C3 变成摆设 |

#### 调研者判断（非拍板）

**倾向支持红线 1**，但必须配套：

- 明确 **哪些路径是 raw 语料（可只注册）** vs **编译知识（投影+审批）**；  
- 人类手改的冲突策略写进 ADR，而不是留给 Worker 发挥。  

若总控想「Obsidian 内编辑零摩擦」，更干净的说法是：**人类编辑不经审批（第一方），Agent 写经审批**——与 ADR-0003 settings 逻辑同构。这仍可保留 SQLite 为 **元数据与生命周期权威**，正文以文件为载体但 **带 digest 登记**。  
（这是对红线 1 的 **细化建议**，不是推翻。）

### 5.2 红线 2  
**「『自动淘汰』只做标记衰减 + 归档；真删除必须走审批并留收据。」**

#### 支持

1. **本仓误杀面积极大**：零入链 134 文件含 ADR/验收/研究；自动真删会破坏审计与方法论样本。  
2. **同日冲突** 说明「过期」常是 **暂时状态**；删除不可逆，标记可回滚。  
3. **外部实践**（Obsidian 社区、vault schema）主流是 archive-not-delete。  
4. **与 C3 收据文化一致**：删除是高危 mutation。  
5. 用户原话要的是 **解决混乱与沟通效率**，不是「磁盘最省」。

#### 反对

1. **归档膨胀**：按 25 文件/天，一年可上千；检索与备份变重。  
2. **合规/泄密场景** 有时必须真删；只归档不够。  
3. **「归档」若仍可被错误检索命中**，等于没淘汰——必须检索层排除 + 降权。  
4. **反方**：对 `role=brief` 且已 consumed 的文件，自动删可能可接受——但 **推测** 仍不如进 `archive/` 稳。

#### 选错代价

| 若错误 **采纳**（其实该大胆自动删） | 若错误 **拒绝**（允许自动真删） |
| --- | --- |
| 磁盘与索引变胖；要做分层存储/冷归档 | **误删 ADR/验收/未完成 brief**；无法证明历史决策；工业客户审计失败；同日「误判过期」永久销毁证据 |
| 用户仍看到「很多文件」（若 UI 不隐藏 archive） | 信任崩盘，比上下文混乱更严重 |

#### 调研者判断（非拍板）

**支持红线 2，且认为这是两条里更不该放松的一条。**  
唯一要补的产品句：归档后 **默认检索不可见**，否则「淘汰」是假的。

### 5.3 若我认为某条「错了」会怎么说？

- 红线 1 **不是错**，但字面若理解成「人类改 Obsidian 也必须审批」则 **错在过严**——应区分 actor。  
- 红线 2 **基本正确**；反对「自动真删 brief」类捷径。  
- 两条都 **未** 被用户逐条确认（handoff §2 已写）；应用本报告证据重新问，而不是默认开工。

---

## 6. §3.4 最小可交付切片（2–3 轮）

### 6.1 切片名称：K0 — Knowledge Registry & Lint（无 Obsidian 挂载）

**目标**：让 harness/总控 **机械知道** 文档角色与替代关系；**不** 解决编辑器体验。

| 轮 | 交付 | 门的意义 |
| --- | --- | --- |
| **K0a** | SQLite migration：`knowledge_documents`（或等价名）— `project_id`, portable `path`, `role`, `status`, `content_digest`, `supersedes`/`superseded_by`, `updated_at`, `source_run_id?`；repository + 单测 | 域权威落点；无 UI 也可测 |
| **K0b** | Host **只读扫描器**：扫项目内约定树（先映射现有 `docs/**`），填充/更新注册表；解析可选 front-matter + 显式 supersession 句 | 不改用户文件亦可工作；对 handoff 三件套打出 `superseded` |
| **K0c** | Lint 报告（host 命令或 `docs` 生成物二选一，**权威在 SQLite**）：P0 superseded、P1 consumed brief、P2 orphan brief、冲突 digest；**零删除** | 总控可在派单前看「该读什么」 |

**明确不做（防止膨胀）**

- Obsidian 安装/同步/双链 UI  
- 自动移动到 archive/ 或真删  
- Agent 写回 markdown  
- embedding RAG  
- 衰退分机器学习  
- 与 Keeper vault 自动双向同步  

### 6.2 为什么这是最小？

1. **卡在真实瓶颈**：不是缺编辑器，是缺 **生命周期元数据**（§2）。  
2. **只加一张表 + 扫描 + lint**，复用 project 隔离与测试风格，不碰 C4/process。  
3. **每步可单独过门**：K0a 纯存储；K0b 只读 IO；K0c 只读聚合。  
4. **为红线预留接缝**：将来投影/审批写回/归档 rename 都挂在同一 `path` 主键上。  
5. **用本仓当夹具**：handoff 链与 101 brief 立刻产生可断言夹具，无需虚构数据集。

### 6.3 成功标准（可验收）

- 扫描后注册表行数 ≈ 项目内知识树文件数。  
- 三份被取代 handoff 的 `status=superseded` 且指向最新 handoff。  
- Lint 对「零入链 ADR」**不得** 建议删除；最多 `unreferenced`.  
- 无 `cargo` 知识逻辑放进 desktop 渲染层（符合壳不拥域逻辑）。

### 6.4 后续自然增量（不在本切片）

K1 固定 `knowledge/` 角色树 + front-matter 规范  
K2 归档 rename + 检索排除（仍要收据）  
K3 Agent 编译页写回走 write_proposal  
K4 Obsidian 作为投影前端  

---

## 7. 不确定项（明确列出）

1. **Keeper Bridge boot** 本环境返回 HTTP 400；未取得 bridge contextPack。研究不依赖它，但总控若要求强制 boot 记录，需修 bridge 或改契约。  
2. **入链统计** 用 basename，可能漏/误（短文件名碰撞风险低，因多数带长日期与 wave 名）。  
3. **git 历史未分析**：无法量化「创建后多久被取代」的小时级分布；同日冲突靠内容对比。  
4. **`.superpowers/round*` 简报** 多在 gitignore / 仓外——未计入 287；真实派发语料可能更大。  
5. **vault 脚本是否在生产节奏运行**：只见 `archive=0` 与脚本存在，**未执行** 脚本验证 decay 实现是否与文档一致。  
6. **知识正文存文件 vs 将来可能的 blob 表**：未做性能/备份设计。  
7. **多项目共享知识**（跨 repo 经验）属 Stage 3 / 共享 vault 边界，K0 有意不碰。  
8. **用户是否接受「execution 永不编译、只注册」**：影响审批量与投影范围——需总控问用户。  
9. **S0 设置是否已在 HEAD 完全落地**：handoff 描述 S0a/S0b 已验收；本调研 **未跑测试** 复核，以 handoff 与 migration 文件为准。  

---

## 8. 给总控的提问清单（拍板用）

1. 红线 1：Agent 写 vs 人类写是否采用 **不对称审批**（人第一方直写文件 + digest 登记，Agent 走 C3）？  
2. 红线 2：归档是否要求 **默认对 Agent 检索不可见**？  
3. K0 知识树是否 **先只收管现有 `docs/`**，还是并行新建 `knowledge/`？  
4. orphan brief（约 43）是 **人审队列** 还是一律 `consumed`？  
5. 真删除是否要 **独立 decision 类型**（比普通 write 更显眼）？  

---

## 9. 证据索引（关键路径）

| 主题 | 路径 |
| --- | --- |
| 产品终点与红线 | `docs/handoff/2026-08-08-pillow-controller-handoff.md` |
| 同日过时 | `docs/handoff/2026-08-08-round5-tech-debt-status.md` vs 上者 §4.5 |
| 设置权威 | `docs/decisions/ADR-0003-settings-authority-and-s0-scope.md` |
| 设置设计（排 Stage 2） | `docs/superpowers/specs/2026-08-08-settings-credentials-design.md` |
| 迁移 | `crates/storage-sqlite/migrations/0001`–`0009_*.sql` |
| 共享知识规则 | vault 根 `AGENTS.md`、`index.md`、`hot.md` |
| 仓库产品序 | `AGENTS.md` |

---

## 10. 完成标记

**KNOWLEDGE-RESEARCH-COMPLETE**

（若总控要求 bridge 强制成功记录，可另开：`KNOWLEDGE-RESEARCH-BLOCKED: Keeper Bridge boot HTTP 400`——**不**阻塞本报告作为只读证据包的可用性。）
