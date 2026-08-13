# 设计：槽位级工具权限如何对接既有两轴模型

**Date:** 2026-08-13  
**Nature:** 设计单。唯一写入：本文件。未改产品代码 / 测试 / 脚本 / 配置。未跑门 / E2E。无 git 写操作。  
**Worker slot:** `grok3` · **总控:** Claude  
**完成信号:** `SLOT-PERMISSION-DESIGN-COMPLETE`

**代码快照说明：** 另有 Worker 正在改槽位 DTO 与设置页。下文关于现状的路径与行为以**撰写时读到的树**为准；若并行改动已合入，以合入后的代码为准复核，不把本文当运行时真值。

---

## 0. 结论（先读）

| 问题 | 结论 |
| --- | --- |
| 槽位级权限是第三轴吗？ | **不是。** 它是**轴二（权限规则）加一个 `slot` 作用域维度**，外加必须钉死的「不能提权」边界。 |
| 和轴一（批准策略）的关系 | 规则先于模式。槽位 deny 命中 → 直接拒，**不问人、不看 Bypass**。 |
| 和轴二（规则）的关系 | **直接复用规则表与求值器**，新增 `scope = slot`（或等价的 `slot_id` 维）。 |
| 和 `governance_from_auth` 的关系 | **正交且不可提权。** auth 推导 `None` 的槽位不能被槽位规则抬到 `Full`，也不能因此进入受治理工具链。 |
| 第一刀 | **槽位 deny-only 规则**（`write` / `edit` / `process`）+ 把 `slot_id` 绑进活 run + 桥接测试：Bypass 下槽位 deny 仍拒。 |

🔴 **不推翻两轴。** 不发明新审批入口。deny 始终优先。

---

## 1. 现状：三样东西各自管什么

本节每条现状陈述都带本仓路径。产品文案不代替代码。

### 1.1 轴一：批准策略（要不要问人）

**维护者拍板的目标形状**（两级，不是单开关）：

```text
有效模式 = 全局是否允许 Bypass ∧ 项目的选择
```

证据：

- `.superpowers/round44/DECISION_permission_two_tier.md` — 全局层 enable Bypass 能力；项目层在全局允许下选择；项目不能突破全局。
- 调研对照：`docs/research/2026-08-12-permission-ux.md`（OpenCode / Grok Build / Codex 均把「问不问」与「规则/沙箱」拆开）。

**代码里今天实际长什么样（读到的一刻）：**

| 层 | 行为 | 路径 |
| --- | --- | --- |
| 模式枚举 | `default` / `accept_edits` / `bypass` | `crates/storage-sqlite/src/settings.rs`（`PermissionMode`） |
| 写自动批 | `AcceptEdits \| Bypass` | 同文件 `automatically_approves_writes` |
| 命令自动批 | **仅** `Bypass` | 同文件 `automatically_approves_processes` |
| 存储约束 | `permissions.mode` **global-only**（SQLite trigger + 仓库拒绝 project 写） | `crates/storage-sqlite/migrations/0014_permission_modes.sql`；`settings.rs` `PermissionModeGlobalOnly` |
| 桥接读取 | write/process 在规则为 `RequireConfirmation` 时调 `resolve_permission_mode()` | `crates/runtime-host/src/governed_write.rs`；`crates/runtime-host/src/host/process.rs` |
| UI 两级 | 渲染器已有 `PermissionPolicyView`（`bypassAvailable` ∧ `projectMode` → `effectiveMode`）与设置文案 | `apps/desktop/src/features/settings/settingsTypes.ts`；`permissionCopy.ts`；`SettingsPanel.tsx` |
| 宿主两级命令 | 渲染器期望 `get_permission_policy` / `set_bypass_available` / `set_project_permission_mode`；缺省时回落 legacy 全局模式 | `apps/desktop/src/features/settings/settingsGateway.ts` |

**重要裂缝（设计必须显式承认，不能假装已齐）：**

- 决策与 UI 已按「两级 Bypass」叙述；
- 宿主桥接路径仍读 **installation-wide** `resolve_permission_mode()`；
- 迁移/仓库仍把 `permissions.mode` 钉在 global-only。

因此：**轴一在产品决策上是两级；在 write/process 自动批路径上，撰写时仍是全局模式生效。** 槽位设计应依赖「有效批准模式」这一**概念**，不依赖「宿主两级已完全落地」。两级落地是并行工作，不是本设计的前置否决条件。

轴一**不**控制：

- 项目目录边界（UI 明确写「只改要不要问你」）：`permissionCopy.ts` `PERMISSION_TWO_AXIS_NOTE`
- 工具主体匹配 / deny 优先：见轴二
- 槽位是否受治理：见 §1.3

### 1.2 轴二：权限规则（自动批准 / 拒绝哪些主体）

**职责：** 对**已进入提案链**的 write / edit / process 主体做匹配，产出：

- `Deny` → 规则引擎拒绝（`reject_by_rule`）
- `Allow` → 规则引擎批准（`approve_by_rule`）
- `RequireConfirmation` → 交给轴一或人类

**纯求值：** `crates/kernel-policy/src/permission_rules.rs`

- 前缀：`process` / `write` / `edit`（**没有 `read` 前缀**）
- 动作：`allow` / `deny`
- 作用域类型：`global` / `project`
- **deny 先于 allow**；同动作内输入顺序破并列
- **项目作用域 allow 在求值层被忽略**（即使脏数据传入）
- process allow 另有 `process_subject_confirmed_for_allow` 门闩；确认失败 → 回退询问，**不削弱 deny**

**持久化：** `crates/storage-sqlite/migrations/0012_permission_rules.sql`  
表 `permission_rules`：`scope ∈ {global, project}`，`project_id` 形状 CHECK，**无 `slot_id` 列**。

**仓库语义：** `crates/storage-sqlite/src/permission_rules.rs`

- 项目规则写入：`ProjectAllowForbidden`（只能 deny）
- `list_effective(project_id)`：global 全部 + 该项目 **deny** 行
- **一行坏数据不废整表**：按行分类，可读进列表，不可读进 `list_unreadable`（模块 rustdoc 写明对标 settings 自砖事故）

**宿主加载与决策：**

- 加载：`crates/runtime-host/src/permission_rules.rs` `evaluate_host_permission_rules` / `…_conjunction`
- write 决策序：规则 Allow/Deny → `decide_*_by_rule`；否则模式自动批 → `decide_*_by_permission_mode`；否则等人  
  `crates/runtime-host/src/governed_write.rs`
- process 同形：`crates/runtime-host/src/host/process.rs`
- 模式收据与规则收据分表：`0014_permission_modes.sql` vs `0012_permission_rules.sql`（模式不是一条 allow 规则）

**现有规则表不感知槽位。** 同一项目内，Controller 与 Librarian 共享同一套 effective 规则。

### 1.3 非轴：`governance_from_auth`（能力边界）

**职责：** 由认证形状**推导**治理能力 `Full | None`，**不是用户偏好**，**不是独立可写字段**。

证据：

- `crates/storage-sqlite/src/runtime_slot.rs`
  - 模块注：`One slot record shape: auth + role. Governance is derived from auth and is never stored independently`
  - `governance_from_auth`：`ApiKey | GrokLogin | Oauth` → `Full`；`LocalCli` → `None`
  - `SlotAuth::governance()` / `is_governed()` 只读派生
- 派单侧镜像：`crates/role-dispatch/src/types.rs` `SlotAuthShape::governance`；`claimed_governed_execution` 在 `None` 上硬拒  
  `crates/role-dispatch/src/result.rs`；测试 `crates/role-dispatch/tests/governance_no_elevate.rs`
- 槽位启动：`LocalCli` 不能作为受治理对话运行时  
  `crates/runtime-slots/tests/slot_service.rs`（`LocalCliUngoverned` 等）

**它管什么：** 这个槽位能不能进入 Pillow 的受治理执行叙事（对话工具链 / 声称 governed execution）。

**它不管什么：**

- 要不要弹出批准条（轴一）
- 某条 `cargo test` / `src/foo.rs` 是否匹配 allow/deny（轴二）
- 「信息管理员只读」——调研已点名缺口：  
  `docs/research/2026-08-13-multi-agent-management.md` §3.5：  
  *治理只有 auth 推导的 Full/None，没有「这个槽位只读」*

### 1.4 工具链现状（与槽位权限直接相关的缺口）

| 工具类 | 是否走提案 + 批准条 | 是否吃权限规则 | 路径 |
| --- | --- | --- | --- |
| 读 (`fs.read` 等) | **否**（绑定 run 后直接执行，无生命周期 observer） | **否** | `crates/governance-port/src/port.rs` `execute_read`；`crates/runtime-host/src/governed_read.rs` |
| 写 / 改 | **是** | **是**（write/edit 前缀） | `governed_write.rs` |
| 进程 / shell | **是** | **是**（process 前缀；shell 走 process 生命周期） | `host/process.rs` |

活 run 绑定上下文 **只有** `project_id` + `conversation_id` + `run_id`，**没有 `slot_id`**：

- `crates/agent-protocol/src/tool/types.rs` `ToolActiveContext`
- 对话入 run 注册：`apps/desktop/src-tauri/src/conversation_service/service.rs`  
  `register_governed_read_run` / `register_governed_process_run`（参数无 slot）

因此：**即便加上槽位规则表，若不在 bind/register 时钉死 active `slot_id`，运行时无法求值。**

### 1.5 派单与「谁的槽位规则」

并发语义已拍板：一个活对话 + 受治理派单  
`docs/decisions/2026-08-13-concurrency-semantics.md`。

派单是 **Controller 会话上的普通 process 提案**（同一批准条），不是目标槽位另起一条审批链：

- `ConversationService::propose_role_dispatch`  
  `apps/desktop/src-tauri/src/conversation_service/service.rs`
- 账本带 `initiator_slot_id` / `target_slot_id`  
  `crates/runtime-host/src/dispatch.rs`

**撰写时的直接含义：** 派单 process 的规则求值若只看「活 run 所属槽位」，则默认是 **initiator（总控）** 的槽位规则，不是 target 的。  
「目标 Librarian 只读」**不会**自动挡住总控派出去的那条 process，除非另有规则或另绑 target。见 §7 未决。

### 1.6 OpenClaw 对照（参考，不照抄）

`docs/research/2026-08-13-multi-agent-management.md` §3.A.5：

- 每 agent `tools.allow` / `tools.deny` + sandbox
- 子 agent 硬拒绝名单（配置外）
- exec 仍有 before-tool-call 审批

Pillow 已有：**提案链 + 规则 + 模式 + 收据**。缺的是 **按槽位收紧**，不是缺第二套审批 UI。

---

## 2. 槽位级权限的定位

### 2.1 判定：不是第三轴

**定位名称：** **轴二的槽位作用域（slot-scoped permission rules）**，并与 `governance_from_auth` 保持正交的能力天花板。

**理由：**

1. **结果空间相同** — 仍是 `Deny | Allow | RequireConfirmation`，仍写进既有 rule / mode / 人类决策收据，不新开决策类型。
2. **不变量相同** — deny 优先、项目不能 allow、坏行隔离、自动批仍有提案与收据（`DECISION_permission_two_tier.md`「不许弱化的既有性质」）。
3. **入口相同** — 决策只发生在既有 write/process 批准条上；禁止「槽位权限专用弹窗」。
4. **第三轴的代价** — 若做成「槽位批准模式」或「槽位 toolset 开关」平行于轴一/轴二，会出现：
   - Bypass × 槽位模式 的组合爆炸；
   - 用户不知谁否决谁；
   - 第二套收据/归因；
   - 与维护者已拍板的两轴叙事冲突。

### 2.2 和轴一（问不问）的关系

```text
槽位/全局/项目规则 先说话；
只有规则给出 RequireConfirmation 时，轴一（有效批准模式）才有权自动批；
人类是最后兜底。
```

- 槽位 **deny** 命中 + 全局 Bypass → **拒绝**（不问）
- 槽位无匹配 + 全局 allow 命中 → **按规则批准**（不问）
- 皆无匹配 + Bypass → **按模式批准**（不问，但仍有 mode 收据）
- 皆无匹配 + Default → **问人**

槽位权限 **不是**「这个槽位用不用 Bypass」。Bypass 仍是项目/全局批准策略。

### 2.3 和轴二（规则）的关系：直接复用 + 加维

**能**直接复用规则表与 `evaluate_permission_rules`，加 `slot` 维。  
**不是**在 `runtime_slots` 上再挂一份互不相通的 allow/deny JSON 清单（那会复制求值与收据，且违反「域状态在 SQLite 结构化表」的工程习惯）。

推荐语义（v1）：

| 作用域 | 可写动作 | 进入 `list_effective` 的条件 |
| --- | --- | --- |
| `global` | allow + deny | 总是 |
| `project` | **仅 deny**（已有） | 当前项目 |
| `slot` | **仅 deny**（本设计 v1） | 当前项目 **且** `slot_id == 活 run 绑定的槽位` |

**为何 v1 槽位也只 deny：**

- 与项目作用域对称，心智一致：「下层只能收紧」。
- 「信息管理员只读」= 对该槽位 deny `write:*` / `edit:*` / `process:*`（或三条前缀的 match-all 模式），**不需要** slot allow。
- 若 v1 就允许 slot allow，会引入「槽位 allow 能否压过项目 deny / 全局 deny」的提权叙事；即便求值 deny 优先能挡住，产品文案也更容易写错。  
  **slot allow 留作未决（§7），默认不做。**

### 2.4 和 `governance_from_auth` 的关系（提权红线）

🔴 **明确回答：auth 推导为 `None` 的槽位，不能被槽位规则「提权」。**

| 命题 | 答案 | 理由 |
| --- | --- | --- |
| LocalCli（`None`）能否因 slot allow 变成受治理对话运行时？ | **不能** | 启动/路径解析在规则求值之前失败；规则根本轮不到 |
| LocalCli 派单结果能否因 slot 规则声称 `claimed_governed_execution`？ | **不能** | `attribute_outcome` 已硬拒；与规则无关 |
| Full 槽位能否因 slot 规则获得项目外写、或绕过路径能力？ | **不能**（本设计不授予） | 规则只作用于提案决策；`ProjectCapability` / 目录边界仍是执行层 |
| Full 槽位 + slot deny 能否比「无规则」更松？ | **不能** | deny-only v1；只能更紧 |
| Full 槽位 + 全局 Bypass + slot deny 写 | **仍拒绝** | deny 优先于模式 |

**合成原则：**

```text
governance_from_auth 是入场券（能否走受治理工具链）；
轴二（含槽位）是入场后的主体防火墙；
轴一是防火墙未命中时的「问不问」。
入场券不能由防火墙发放。
```

实现约束（落地时必须有测试，本设计单不写代码）：

1. 规则 CRUD / 求值 API **不得**接受「把 governance 写成 Full」之类字段（governance 继续不可独立存储）。
2. 对 `None` 槽位写入 slot 规则：允许作为配置数据存在，但 **无受治理 run 可绑定 → 永不生效**；或创建时拒绝（二选一见 §7）。
3. 回归：`governance_no_elevate` 类测试继续为门禁。

### 2.5 明确不采用的方案

| 方案 | 为何否决 |
| --- | --- |
| 第三轴「槽位批准模式」 | 与轴一叠床架屋；Bypass×槽位模式难解释 |
| 槽位 tool allowlist 静默摘工具、不经提案 | 新失败面；无收据；违背「写能力工具走批准路径」 |
| 把 `governance` 扩成 `Full/ReadOnly/None` 可写字段 | 与「governance 仅由 auth 推导」矛盾；把偏好塞进能力边界 |
| 复制 OpenClaw sandbox 作为本切片 | 超出最小切片；Pillow 目录边界已有独立叙事 |

---

## 3. 合成顺序与判定表

### 3.1 一次 write / process 调用的判定顺序

适用于：已绑定受治理 run、提案已 durable reserve 之后（与现桥接一致）。

```text
0. 入场（更早，不在本表内）
   - 槽位能否 start / 绑定受治理 run：governance_from_auth
   - LocalCli → 不进入下列步骤（对话工具链）

1. 构造 effective 规则集
   - global 全部
   - 当前 project 的 deny
   - 活 run 绑定 slot 的 deny（本设计新增）
   - 逐行可读性隔离：坏行跳过并进 quarantine，不 fail 整集
     （保持 crates/storage-sqlite/src/permission_rules.rs 不变量）

2. 规则求值（kernel-policy，deny 优先）
   - 任 deny 命中 → RejectByRule（停）
   - 否则任 allow 命中（今日仅 global allow 可权威）→ ApproveByRule（停）
   - 否则 → RequireConfirmation

3. 批准模式（轴一；仅 RequireConfirmation）
   - effectiveMode = 决策形状「全局 Bypass 能力 ∧ 项目选择」
     （宿主未齐时，实现上可能仍是全局 mode —— 见 §1.1 裂缝）
   - write + (AcceptEdits|Bypass) → ApproveByMode
   - process + Bypass → ApproveByMode
   - 否则 → 等人

4. 人类决策（既有批准条）
   - Approve / Reject / 过期 / 取消 —— 不新开入口

5. 执行
   - 仍受 ProjectCapability / 路径隔离 / process 确认门闩等执行层约束
   - 读工具不走 1–4（现状）；「只读槽位」靠 2 的 write/edit/process deny 表达
```

**否决权（谁能否决谁）：**

| 较高否决权 | 较低 |
| --- | --- |
| auth `None`（进不了链） | 一切规则 / 模式 / 人类「批准执行」 |
| 任意 scope 的 **deny 规则** | allow 规则、批准模式、人类「同意」若在 deny 之后不应再执行* |
| 人类 **拒绝** | 模式本想自动批（人已接管的路径） |
| 模式 / 人类 **批准** | 不能推翻已匹配的 deny（deny 在步骤 2 已终局） |

\*现实现：deny 在等人之前已 `decide_by_rule` 拒绝；人类看不到「先 deny 再问」。保持这一形状。

### 3.2 判定表

图例：

- **S-deny** = 活槽位 deny 是否命中该主体  
- **GP-deny** = 全局或项目 deny 是否命中  
- **G-allow** = 全局 allow 是否命中（项目/槽位 v1 无 allow）  
- **Mode** = 有效批准模式  
- 结果列对 **write** 与 **process** 分写时用 `/`

| S-deny | GP-deny | G-allow | Mode | 结果 | 归因 |
| --- | --- | --- | --- | --- | --- |
| Y | * | * | * | **拒绝** | `reject_by_rule`（槽位规则） |
| N | Y | * | * | **拒绝** | `reject_by_rule`（全局/项目） |
| N | N | Y | * | **批准** | `approve_by_rule` |
| N | N | N | `bypass` | **批准** write 与 process | `approve_by_mode` |
| N | N | N | `accept_edits` | **批准** write / **询问** process | mode 或人类 |
| N | N | N | `default` | **询问** | 人类 |

**组合故事（产品必须能讲清的两句）：**

1. **「这个槽位只读」+「全局/项目 Bypass」同时开**  
   → 该槽位上的写/命令：规则 deny → **自动拒绝，不询问**。  
   → Bypass 只对「规则未命中」的主体免询问；**不能**打穿槽位 deny。
2. **「这个槽位只读」+ Default**  
   → 写/命令同样在规则层拒绝；用户甚至不一定看到批准条（与今日项目 deny 行为一致）。

### 3.3 读工具

现状读路径无规则、无批准条（§1.4）。  
**「只读槽位」在本设计中的操作定义：**

```text
该槽位对 write / edit / process 主体 deny（通常为前缀级全匹配）；
读仍按现路径执行（受项目能力与路径隔离约束）。
```

这与 OpenClaw「家庭 bot 只留 read」**产品结果相近**，机制不同：Pillow 不靠 tool allowlist 摘工具，而靠 **提案链上的 deny**。  
若未来要「连读也禁」，必须新增规则前缀或独立能力 —— **不在 v1**，见 §7。

---

## 4. 存储与迁移

### 4.1 推荐：扩展 `permission_rules`，不新建平行清单

**表：** 既有 `permission_rules`（`0012_permission_rules.sql`）。

**形状变更（概念迁移，非本单实现）：**

| 列 | 变更 |
| --- | --- |
| `scope` | CHECK 增加 `'slot'` |
| `slot_id` | **新增** `TEXT NULL`，有值时引用 `runtime_slots.slot_id`（或同形 UUID 文本） |
| `project_id` | slot 行 **必须**非空（槽位本就项目作用域） |
| `action` | slot 行 **仅** `deny`（Rust 写入校验 + 建议 SQL 可防住的部分） |

**CHECK 形状：**

```text
global:  project_id IS NULL  AND slot_id IS NULL
project: project_id IS NOT NULL AND slot_id IS NULL
slot:    project_id IS NOT NULL AND slot_id IS NOT NULL
```

**索引：**

- 保留 global / project 序
- 新增 slot：`(project_id, slot_id, created_at, rule_id)` 过滤 `scope = 'slot'`

**`list_effective(project_id, slot_id: Option)`：**

```text
global
∪ project deny for project_id
∪ (slot_id 有值时) slot deny for (project_id, slot_id)
```

无 `slot_id` 时行为 **等于今日**（仅 global + project deny）—— 兼容旧调用。

### 4.2 老库 / 已有行

| 已有行 | 迁移后 |
| --- | --- |
| `scope=global` | `slot_id = NULL`；语义不变 |
| `scope=project` | `slot_id = NULL`；语义不变 |
| 无 slot 行 | 全槽位共享原项目规则（与今日一致） |

迁移步骤原则：

1. `ADD COLUMN slot_id TEXT NULL`（先可空，不改语义）
2. 扩展 CHECK / 触发器（注意 SQLite 表重建惯例，与仓库既有 migration 风格一致）
3. **禁止**一次扫描失败就拒绝整表：继续走 `classify_permission_rule_row` → 坏行 quarantine  
   见 `permission_rules.rs` 模块文档「Unreadable rows are quarantined, not fatal」
4. 新 scope 枚举在 Rust `PermissionRuleScope` / 渲染器 allowlist **同 PR** 放宽（宿主 DTO 闭集规则见 `AGENTS.md`）

### 4.3 收据表

今日收据：`write_permission_rule_receipts` / `exec_permission_rule_receipts`  
`rule_scope ∈ {global, project}`，无 slot 字段（`0012_permission_rules.sql`）。

落地时二选一（推荐 A）：

| 方案 | 做法 | 代价 |
| --- | --- | --- |
| **A. 扩展收据** | `rule_scope` 增 `slot`；加 `rule_slot_id NULL` | 归因完整；历史行 NULL |
| B. 折叠显示 | 收据仍记 project，pattern 含槽位元数据 | **否决倾向**：审计说不清「哪条槽位规则」 |

模式收据表 **不动**（槽位不是模式）。

### 4.4 内核 `PermissionRuleSpec`

`crates/kernel-policy/src/permission_rules.rs` 今日 `PermissionRuleScope { Global, Project }`。

求值函数本身只看 prefix/pattern/action；**scope 不参与匹配**，只参与「project allow 忽略」。  
加 `Slot` 后：

- 求值：`Project` **与** `Slot` 的 allow 均忽略（v1 本就不写 allow）
- 匹配顺序：仍 deny 全集优先，再 allow；**不需要**「槽位规则单独第二趟」—— 放入同一 `rules: &[PermissionRuleSpec]` 即可
- 若未来 slot allow：仍不得压过任何 deny

### 4.5 不推荐：槽位行内嵌 JSON 清单

- 重复求值语言与校验
- 难做 rule 级收据与 `content_digest`
- 难复用 unreadable 隔离
- 与 OpenClaw 配置文件同构，但与 Pillow SQLite 域模型不一致

---

## 5. UI 表达

### 5.1 放哪

| 表面 | 职责 |
| --- | --- |
| **代理与槽位**（主） | 槽位编辑：预设「只读」开关 / 说明；展示「本槽位额外 deny」摘要 |
| **安全与权限**（辅） | 规则大表：可按作用域筛选 global / project / slot；高级增删改仍在此 |

理由：

- 「信息管理员只读」是 **角色/槽位身份** 问题，用户会在槽位页找。
- 规则语法与 deny 优先教育已在安全页（`permissionCopy.ts`）。
- 不把完整 glob 编辑器塞进槽位小表单；预设一键生成三条（或一条）slot deny，高级回链安全页。

### 5.2 「只读 + Bypass」怎么讲清楚

沿用两轴文案结构，**加第三句槽位收紧**（文案草案，落地可改）：

1. 既有：`权限模式只决定要不要问你；文件默认仍只能动当前项目目录。`（`PERMISSION_TWO_AXIS_NOTE`）
2. 既有：`deny 规则始终优先；自动批准仍会生成提案…`（`PERMISSION_DENY_RECEIPT_NOTE`）
3. **新增（槽位）：**  
   `槽位「只读」是额外的 deny，不是另一种 Bypass。Bypass 开着时，本槽位被 deny 的写入/命令会直接拒绝，不会变成「不问就执行」。`

槽位行上的状态芯片建议：

| 芯片 | 含义 |
| --- | --- |
| 受治理 / 未受治理 | 来自 `governance` 派生（只读展示） |
| 工具：标准 / 只读 | 是否存在 slot 级 write/edit/process deny 预设 |
| 项目有效模式：… | 链到安全页 effectiveMode（避免在槽位页再做一套模式编辑） |

### 5.3 不照抄 OpenClaw 的点

- 不要求用户编辑 `tools.allow` 长列表才能完成「只读」。
- 不用 sandbox 文案冒充只读。
- 不把子 agent 硬拒绝名单做成用户可配第三套（派单硬约束另案）。

### 5.4 与并行 DTO 工作的边界

槽位 list DTO 今日闭集字段见  
`apps/desktop/src/features/settings/runtimeSlotsTypes.ts`。  
若 UI 要展示「只读」标志：

- **要么** 派生字段进 DTO（与渲染器 allowlist **同 PR**），  
- **要么** 前端另调规则列表按 `slotId` 过滤（DTO 不变）。  

本设计不规定必须改 DTO；最小切片甚至可以 **先无 UI**（§6）。

---

## 6. 最小可用切片（第一刀）

### 6.1 切什么

**名称：** Slot deny-only on the existing proposal chain  

**包含：**

1. **存储**  
   - `permission_rules.slot_id` + `scope=slot`  
   - 写入校验：slot → deny-only + project_id 必填  
   - `list_effective(project_id, Some(slot_id))`  
   - 坏行 quarantine 行为回归

2. **运行时绑定**  
   - 对话 start / run register 时把 **active `slot_id`** 写入 write/process 的 bound run  
   - `list_effective` 求值时传入该 id  
   - 无 slot 绑定 → fail-closed 为「不加载 slot 规则」或「拒绝自动批」二选一；**推荐：不加载 slot 规则并打结构化日志**，以保持旧测试路径，但生产 start 必须绑 slot

3. **求值**  
   - 不改 deny 优先  
   - 单测 / 桥接测：  
     - 全局 Bypass  
     - 槽位 A：`edit`/`write`/`process` deny `*`（或等价全匹配）  
     - 槽位 A 发起的写/命令 → `reject_by_rule`  
     - 同项目槽位 B 无 slot 规则 → 仍可走 Bypass 自动批

4. **（可选但高价值）预设 API**  
   - `set_slot_tool_profile(project, slot, read_only | standard)`  
   - `read_only` = 确保三条前缀的 slot deny 存在  
   - `standard` = 删除该预设生成的规则（不动用户手写规则 —— 若难区分则 v1 只做「安装预设」，删除靠规则表）

**刻意不包含：**

- 槽位 allow  
- 读工具纳入规则  
- 派单按 **target** 槽位求值（见 §7）  
- 新批准条 / 新 mode  
- 完整规则编辑 UI（可用测试/命令插入规则验证）  
- 把 Librarian 角色默认强制只读（产品默认值，可第二刀）

### 6.2 为什么这一刀本身有价值

- 直接补上调研缺口：「这个槽位只读」**可表达、可审计、Bypass 下仍硬**。
- 复用全部既有安全不变量，不重开轴。
- 验证面集中：存储 + `list_effective` + 一条 write 桥接 + 一条 process 桥接即可独立红绿。
- 不依赖设置页两级宿主是否已完全接线（测试可直接 `upsert_permission_mode(Bypass)` 模拟轴一）。

### 6.3 独立验证清单（落地时；本设计单不跑）

| # | 断言 |
| --- | --- |
| V1 | 老库迁移后，既有 global/project 规则 effective 集不变（`slot_id` 空） |
| V2 | 插入一条不可读行，不阻止其它规则 effective |
| V3 | 项目 allow 仍不可写入；slot allow 写入被拒 |
| V4 | Bypass + slot write deny → 写提案 `reject_by_rule`，无 mode 收据 |
| V5 | Bypass + 无 slot deny → 写提案仍可 `approve_by_mode`（回归） |
| V6 | auth `None` 槽位不能 start 受治理对话（既有）；有无 slot 规则不改变该结论 |
| V7 | 收据能指出 slot 规则 id / scope（若选 §4.3 A） |

### 6.4 建议落地顺序（仍属设计，非本单施工）

```text
migration + repository
  → kernel-policy scope 枚举（若需要）
  → runtime-host list_effective 传 slot_id
  → conversation register 绑 slot_id
  → 桥接测试 V4/V5
  →（可选）预设 API
  →（其后）槽位 UI 芯片 + 文案
```

---

## 7. 未决问题（老实列）

### U1. 派单 process 应用谁的槽位规则？

**现状：** 派单挂在 Controller 会话的 process 链上；活 run 若只绑 initiator，则 slot deny 作用在总控。  
**两种走法：**

| 走法 | 含义 | 代价 |
| --- | --- | --- |
| A. 仅 initiator | 实现简单；「Librarian 只读」只管 **Librarian 自己聊的时候** | 总控派去跑目标可执行文件时，不吃目标只读 |
| B. 派单改用 target 规则（或 initiator∪target 的 deny 并集） | 更贴「对这个角色收紧」 | 要在 dispatch prepare/execute 显式选规则主体；账本与测试都要钉 |

**我没拍板。** 倾向：**v1 用 A**，文档写清；产品若要求「只读角色连被派单也不能写」，再做 B（deny 并集更安全）。

### U2. 槽位将来要不要 allow？

v1 **不做**。若将来「CI 槽位永远自动 `cargo test`」：

- 做 slot allow 会与「下层只收紧」文案冲突；
- 仍必须 deny 优先，且 **不得** 让 slot allow 影响 governance。  
代价是规则层出现「仅 global 与 slot 可 allow」的不对称，需要单独 UX。

### U3. 读拒绝 / 工具可见性

是否要在适配器层对只读槽位 **隐藏** write 工具（减少模型瞎调）？  
现状：deny 在提案后拒绝，模型仍可能反复提议。  
隐藏工具 = 适配器/清单层，**另一切片**；与规则 deny 可叠加，但是新面。

### U4. 轴一宿主两级未齐

槽位设计依赖「模式在规则之后」。  
不依赖两级是否完成。但 **effectiveMode 的项目层** 若与 UI 不一致，用户会把「项目没 Bypass 却像全局 Bypass」误判到槽位。  
**不确定点：** 两级落地与槽位切片的发车顺序；建议槽位切片测试钉死 `PermissionMode`，UI 文案继续指向 effectiveMode。

### U5. `None` 槽位上的 slot 规则行

允许存储（配置预置）还是创建拒绝？  
预置友好 vs 避免「以为配了只读其实从不生效」的假安全感。  
**倾向：** 允许存储 + UI 对未受治理槽位禁用「只读」并解释原因。

### U6. Librarian 角色是否默认只读？

角色枚举已有 `librarian`（`runtime_slot.rs` `SlotRole`），但 **无** 工具权限耦合。  
默认只读是产品政策，不是机制必需。机制先落地，默认值另决策。

### U7. 并发/DTO 并行改动

槽位 DTO、设置页可能正在变。  
本文引用的 `RuntimeSlotView` 字段集、OAuth 展示字段等可能漂移。  
落地 PR 必须以当时闭集 DTO 与 gateway allowlist 为准，**加字段必须同 PR 放宽解析器**（`AGENTS.md`）。

### U8. role-dispatch 的 auth 形状是否含 Oauth

`runtime_slot.rs` 的 `SlotAuth` 含 Oauth→Full；  
`role-dispatch` 的 `SlotAuthShape` 撰写时仍是 ApiKey / GrokLogin / LocalCli。  
与槽位权限正交，但派单治理测试矩阵需在 Oauth 接通后补齐 —— **标出以免误读为「Oauth 不受治理」。**

### U9. 无 active slot 的 run（测试/假适配器）

生产对话应总有 active slot。  
测试双与 fake 路径可能没有。  
需在实现时钉：缺 slot_id 时 effective 不含 slot 规则，且不得把「缺绑定」解释成「slot allow 全开」。

---

## 8. 与既有决策的兼容性声明

| 既有决策 | 本设计 |
| --- | --- |
| 两级批准策略（全局 Bypass 能力 ∧ 项目选择） | **保留**；槽位不成为第三级 mode |
| deny 始终优先 | **保留并扩展到 slot deny** |
| 自动批仍有提案/收据 | **保留** |
| governance 仅由 auth 推导 | **保留**；规则不能提权 |
| 一个活对话 + 受治理派单 | **保留**；slot 规则绑活 run；派单 target 规则见 U1 |
| 不发明新审批入口 | **保留** |
| 坏规则行隔离 | **迁移后仍必须成立** |

---

## 9. 参考路径速查

| 主题 | 路径 |
| --- | --- |
| 两级 mode 决策 | `.superpowers/round44/DECISION_permission_two_tier.md` |
| 权限 UX 调研 | `docs/research/2026-08-12-permission-ux.md` |
| 多 agent / OpenClaw 权限 | `docs/research/2026-08-13-multi-agent-management.md` §3.5 / §3.A.5 |
| 并发语义 | `docs/decisions/2026-08-13-concurrency-semantics.md` |
| 规则求值 | `crates/kernel-policy/src/permission_rules.rs` |
| 规则存储与隔离 | `crates/storage-sqlite/src/permission_rules.rs` |
| 规则 schema | `crates/storage-sqlite/migrations/0012_permission_rules.sql` |
| 模式枚举与自动批 | `crates/storage-sqlite/src/settings.rs` |
| 模式 global-only | `crates/storage-sqlite/migrations/0014_permission_modes.sql` |
| write 合成 | `crates/runtime-host/src/governed_write.rs` |
| process 合成 | `crates/runtime-host/src/host/process.rs` |
| 读路径（无规则） | `crates/runtime-host/src/governed_read.rs` |
| governance 推导 | `crates/storage-sqlite/src/runtime_slot.rs` |
| 派单不提权 | `crates/role-dispatch/tests/governance_no_elevate.rs` |
| UI 两轴文案 | `apps/desktop/src/features/settings/permissionCopy.ts` |
| 槽位 DTO（可能正被改） | `apps/desktop/src/features/settings/runtimeSlotsTypes.ts` |

---

`SLOT-PERMISSION-DESIGN-COMPLETE`
