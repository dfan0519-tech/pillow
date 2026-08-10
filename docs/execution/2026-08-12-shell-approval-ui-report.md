# Shell 命令审批呈现（渲染层）— 完成报告

Worker: grok1 · 写码单 · 只动渲染层  
完成信号: `SHELL-APPROVAL-UI-COMPLETE`

## 1. 退出码表

| 命令 | 退出码 | 说明 |
|------|--------|------|
| `corepack pnpm typecheck` | **0** | 根递归 typecheck；`@pillow/desktop` `tsc --noEmit` 通过 |
| `corepack pnpm --filter @pillow/desktop test` | **0** | 35 files / **441** tests passed |

未运行：`cargo`、门禁、desktop E2E（按条款由其他 Worker / 总控独占）。

## 2. T1 逐字符相等：如何证明

### 实现

- `apps/desktop/src/features/execution/executionTypes.ts`
  - `SHELL_PROGRAM_LABEL = "shell"`
  - `isShellProgram(programLabel)`
  - `formatCommandArgvDisplay(programLabel, argv)`：
    - **shell**：返回 `argv[0]`（空 argv → `""`）；**不做** `JSON.stringify`、trim、join、空白规范化
    - **structured**：沿用 `formatArgvDisplay`（逐参 JSON 编码 + 空格拼接）
- 审批条 / 活动行 / 时间线 / 终端输出头均改为经 `formatCommandArgvDisplay`（或等价的 `formatProcessArgvDisplay(program, argv)`）取命令体。

### 断言（正向接受，非假绿）

测试套件：`apps/desktop/tests/c4-execution-primitives.test.tsx` → `describe("shell command approval display")`

| 断言点 | 证明方式 |
|--------|----------|
| 嵌套引号命令与输入逐字符相等 | `expect(el.textContent).toBe(NESTED_QUOTE_CMD)`（**不用** `toHaveTextContent`，避免空白归一化假绿） |
| 同时钉 `data-full-command` / `title` | `getAttribute(...) === NESTED_QUOTE_CMD` |
| 与旧 JSON 转义形态不同 | `formatArgvDisplay([cmd]) !== cmd` 且渲染结果 `!==` 转义串 |
| 结构化回归 | `programLabel=node` / `argv=["-e","console.log(1)"]` → textContent 仍为 `"-e" "console.log(1)"` |
| 同套件双路径 | `rerender` shell → structured，各自 kind + 命令体正确 |
| 长命令完整可得 | 长度 > `COMMAND_DISPLAY_EXPAND_THRESHOLD` 时：DOM text / title / data-full-command 仍等于全文；存在「展开完整命令」按钮；展开后 class 含 `--expanded` 且 text 仍相等 |

辅助纯函数断言：

```ts
expect(formatCommandArgvDisplay("shell", [NESTED_QUOTE_CMD])).toBe(NESTED_QUOTE_CMD);
expect(formatCommandArgvDisplay("cargo", structured)).toBe(formatArgvDisplay(structured));
```

时间线 / 模型层：`c4-process-activity-integration.test.tsx`

- shell 模型 `argv[0]` 与密封命令相等（含 `\` 时**不**被整段 path-redact）
- `formatProcessArgvDisplay("shell", argv) === shellCmd`
- 时间线 DOM：`command-activity-kind` =「Shell 命令」+ argv textContent 逐字符相等

## 3. T3 `ruleSubject` 结论（只审计，未改宿主）

| 来源 | shell 行为 | 是否合适 |
|------|------------|----------|
| 宿主 `LiveProcessProposalView.rule_subject` | 对 `program == "shell"` 写 **空字符串**（桥接测例亦断言 empty） | **不合适**：空串不能作为规则匹配主体，也无法预填「总是允许此模式」 |
| 渲染网关 `isRuleSubjectValue` | 要求非空 string | **冲突**：宿主若字面下发 `ruleSubject: ""`，`normalizeExecProposal` 会 **整包拒收**（提案进不了审批条） |
| Workbench 回退 | `readRuleSubject` 失败时 `programLabel + " " + argv.join(" ")` → 形如 `shell <完整命令>` | 有内容可填对话框，但是 **UI 自拼**，非宿主权威 subject；且前缀 `shell ` 是否参与 process 规则匹配取决于宿主 S2 语义 |
| `CommandApprovalBar` 本地 fallback | shell 时用 **原始命令串** `argv[0]`（无 JSON 转义） | 与 Workbench 回退形状略有差异（无 `shell ` 前缀）；仅在未传入 `ruleSubject` prop 时生效 |

**需要宿主侧配合（本单未改）：**

1. 为 shell 提案提供**非空**、可匹配的 `ruleSubject`（至少让 DTO 过渲染网关）；空串要么省略字段并改网关为 optional，要么改成有意义的主体。
2. 与 S2「命令拆段 + 规则合取」对齐：主体应是**将参与规则匹配的串**（整命令或分段后的各段），且 UI 将来可能展示**多条**规则归因——本单**未**写死「只能有一条规则」的 UI 结构（仍为单 `ruleSubject` + 单 `AlwaysRuleDialog`，与现有 process 条一致，可扩展）。
3. 建议宿主与渲染层对「总是允许」预填主体统一：原始命令 vs `shell <cmd>`，避免用户保存的 pattern 与匹配器不一致。

## 4. 写入边界审计

**本 Worker 改动（仅渲染层 + 报告）：**

| 路径 | 变更 |
|------|------|
| `apps/desktop/src/features/execution/executionTypes.ts` | shell 判定 + `formatCommandArgvDisplay` |
| `apps/desktop/src/features/execution/CommandApprovalBar.tsx` | shell / structured 分流、展开、全量 title/data |
| `apps/desktop/src/features/execution/CommandActivityRow.tsx` | shell 标签 + 原样命令 |
| `apps/desktop/src/features/execution/TerminalOutputView.tsx` | 同上 |
| `apps/desktop/src/features/execution/processActivityModel.ts` | shell 不整段 redact；display 带 programLabel |
| `apps/desktop/src/features/conversations/ConversationTimeline.tsx` | 时间线 shell 呈现 |
| `apps/desktop/src/styles/execution.css` | kind / expand / wrap 样式 |
| `apps/desktop/tests/c4-execution-primitives.test.tsx` | shell 审批呈现套件 |
| `apps/desktop/tests/c4-process-activity-integration.test.tsx` | shell 模型 + 时间线 |
| `docs/execution/2026-08-12-shell-approval-ui-report.md` | 本报告 |

**未触碰：** `crates/**`（工作区另有 kernel-policy 等改动，属其他 Worker）。

**未执行：** 任何 git 写操作。

## 5. 残留风险

| 风险 | 说明 |
|------|------|
| **`summary` 仍可能 JSON 转义** | 宿主 shell 摘要形如 `"shell" <json-encoded-command>`。审批条若展示 `proposal.summary`，用户仍可能看到转义摘要；**命令体**（argv 区）已原样。未改宿主 summary 生成。 |
| **`ruleSubject` 空串 vs 网关** | 见 T3：在宿主修好前，真实 shell 提案可能在 `normalizeExecProposal` 被拒，审批条根本不出现。 |
| **Workbench 回退 vs 条内 fallback 形状不一致** | Workbench 拼 `shell <cmd>`；条内 fallback 用 `argv[0]`。产品路径几乎总是走 Workbench prop。 |
| **视觉截断 vs DOM 全文** | 默认 CSS ellipsis；DOM `textContent` / `title` / `data-full-command` 始终为全文。长命令另有「展开完整命令」。若未来改为**字符串切片**截断且只渲染截断串，会破坏硬约束——当前实现避免了这一点。 |
| **仍可能转义/歧义呈现的界面** | ① 宿主写入的 `summary` 字段本身；② 规则对话框里若用户基于错误 subject 存 pattern；③ Agent 活动披露（`AgentActivityDisclosure`）**不**画 process argv，无此问题；④ 结构化 argv 继续 JSON 编码（**有意保留**，边界清晰）。 |
| **shell argv 多元素** | 宿主契约为单元素；实现取 `argv[0]` only，不 join，避免发明第二种命令串。 |
| **S2 多规则** | 未实现多规则 UI；未假设「全局仅一条 process 规则」。 |

## 6. UI 措辞（与现有中文一致）

- 标签：**Shell 命令**
- 长命令：**展开完整命令** / **收起**
- 高风险 shell 注记：**Shell 命令按字面执行；项目路径约束不是操作系统沙箱。**
- 结构化路径：程序名 + JSON argv、按钮文案（批准一次 / 拒绝 / 查看命令）未改

---

`SHELL-APPROVAL-UI-COMPLETE`
