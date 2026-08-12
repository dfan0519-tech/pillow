# Pillow 外部代码审计报告

> 审计结论：**NOT_CLOSED**
> 报告日期：2026-08-12（Asia/Tokyo）
> 审计类型：治理边界、授权完整性与审计归因的只读外部审计
> 原始简报：[external-audit-brief.md](./external-audit-brief.md)
> 处置原则：本报告只报告问题，不包含源码修复

---

## 1. 交付摘要

本次审计以“agent 的副作用必须先到达宿主、由人类或既有明确规则作出决定，再由宿主执行并留下可信记录”为核心不变量。审计没有把测试数量或历史绿灯当作接受证据，而是从 provider 输入一路追踪到 permission rule、自动模式、原生执行器、renderer DTO、角色派发和持久化归因边界。

在简报第 7 节列出的 13 项已知缺口之外，本次确认了 6 个新的主问题：

| ID | 优先级 | 问题 | 最直接后果 |
| --- | --- | --- | --- |
| EA-01 | **P1 / High** | Windows 大小写别名绕过项目 deny | `accept_edits` / `bypass` 可在没有新的人类决定时修改 deny 目录中的同一文件 |
| EA-02 | **P1 / High** | process rule subject 丢失 argv 边界 | 已保存的 allow rule 可批准语义不同的结构化 argv |
| EA-03 | **P2 / Medium** | renderer 实际持有写审批能力，且原生规则入口依赖 renderer-only 校验 | renderer 被执行任意同源代码后可批准待处理写入或植入空 pattern 全局规则 |
| EA-04 | **P2 / Medium** | 截断 diff 仍可批准完整 sealed content | 用户无法看到尾部内容，却可以批准写入全部内容 |
| EA-05 | **P2 / Medium** | role dispatch initiator 未绑定当前项目 Controller | 可伪造发起者、规避 self-dispatch 比较并形成错误归因 |
| EA-06 | **P3 / Low** | Unicode 双向控制字符可伪装进程审批文本 | 人工看到的参数顺序可能不同于实际结构化 argv |

**发布建议：**在 EA-01 和 EA-02 关闭前，不应把“项目 deny 始终优先”或“持久化 process allow 精确代表用户批准的 argv”作为安全承诺；在 EA-03 和 EA-04 关闭前，不应把 renderer 描述为纯 view，也不应把截断写入预览视为完整的人类授权。

---

## 2. 审计对象与实时状态

### 2.1 代码快照

- 简报声明的审计快照：`ce017a5`。
- 审计时实时 `HEAD`：`038dc07ce67e91c919e120168ba8f128a9675e70`。
- `ce017a5..HEAD` 的已提交差异仅涉及 `AGENTS.md`、`README.md` 和审计/交接文档；本报告涉及的产品代码没有落入这组提交差异。
- 每个主发现涉及的候选源码均单独检查为与 `HEAD` 一致，未把并发未提交修改误当作已提交产品行为。

### 2.2 工作树边界

审计开始时工作树为干净状态。审计期间，另一工作流并发修改了 OAuth、runtime-slot、settings UI 与相关测试，并删除了若干 prototype 截图；这些变更不是本次审计产生的，也没有被本次审计覆盖、暂存、清理或复位。

因此：

1. 本报告的结论适用于列明证据的已提交治理边界代码。
2. 本报告不对审计期间持续变化的 OAuth/settings 实现给出完整性或发布结论。
3. 未运行会混合并发修改结果的全量 workspace gate、桌面 E2E 或正式发布构建。

### 2.3 审计方法

- 从真实 host-side effect 倒推授权链，而不是从 UI 或测试名称推断。
- 将 provider 传入值、规则匹配 subject、sealed payload 和操作系统实际解析对象逐层对比。
- 对每个问题分别记录前置条件、具体序列、代码允许的结果、实机观察和未执行部分。
- 使用只读控制实验排除容易形成误报的候选，例如 `cmd.exe /D /S /C` 下的单纯 LF/CRLF 分隔。
- 不重复简报第 7 节已经承认的 NativeAdapter、真实任务、dispatch ledger 持久化等已知缺口。

---

## 3. 详细发现

## EA-01 — Windows 路径大小写别名绕过项目 deny

**优先级：P1 / High**
**类型：授权规则与操作系统对象解析不一致**
**状态：代码链确认；关键文件系统谓词已实机观察；未执行破坏性写入**

### 前置条件

1. Pillow 运行于 Windows 的默认大小写不敏感文件系统语义下。
2. 项目存在 `write:` 或 `edit:` deny，例如 `edit:blocked/**`。
3. permission mode 为 `accept_edits` 或 `bypass`。
4. Agent 能以不同大小写拼写同一项目相对路径。

### 具体序列

1. 实际文件为 `blocked/secret.txt`，项目 deny 为 `edit:blocked/**`。
2. Agent 提交 modify proposal，路径写成 `BLOCKED/SECRET.TXT`。
3. provider 路径归一化仅清理组件并用 `/` 连接，保留原始大小写。
4. runtime host 用原始 `args.path` 进行规则评估。
5. portable path wildcard 在所有平台均大小写敏感，因此 deny 不命中，结果退化为 `RequireConfirmation`。
6. `accept_edits` 或 `bypass` 将该结果自动批准。
7. executor 把路径组件交给 Windows；Windows 将大小写别名解析为原来的 `blocked/secret.txt`。
8. 同一物理文件被修改，但项目 deny 没有胜出，也没有新的人工决定。

### 代码证据

- [`permission_rule_matches`](../../crates/kernel-policy/src/permission_rules.rs#L339-L352) 对 write/edit 使用 portable wildcard，没有 Windows case folding。
- 现有测试明确固定了 [`SRC/**` 不匹配 `src/lib.rs`](../../crates/kernel-policy/tests/permission_rules.rs#L144-L149)，并标注“matching is case-sensitive on every platform”。
- ACP 路径归一化 [`project_relative_path`](../../crates/adapter-grok-acp/src/provider_tools.rs#L327-L360) 保留组件大小写。
- ACP write 根据 `project_root.join(&path).try_exists()` 判定 create/modify，[没有先获得统一规则 subject](../../crates/adapter-grok-acp/src/provider_writes.rs#L156-L180)。
- host 使用 [`&args.path`](../../crates/runtime-host/src/governed_write.rs#L679-L715) 评估规则。
- [`accept_edits` 和 `bypass` 自动批准 writes](../../crates/storage-sqlite/src/settings.rs#L229-L234)。
- host 对未命中规则的 proposal 执行 [permission-mode 自动决定](../../crates/runtime-host/src/governed_write.rs#L773-L799)。
- executor 的 [`target_path`](../../crates/tool-executor/src/write.rs#L672-L700) 逐组件交给操作系统解析；路径策略校验没有把用户规则 subject 统一为 Windows 对象身份。

### 观察与未观察

**已观察：**在审计主机上，准确路径 `<repo>\crates\kernel-policy\Cargo.toml` 和全大写别名 `<repo>\CRATES\KERNEL-POLICY\CARGO.TOML` 同时可访问，SHA-256 完全相同。

**代码允许：**deny 大小写失配后，自动模式进入宿主写入链并解析到同一文件。

**未执行：**没有对 deny 目录中的真实文件运行 alternate-case modify/create，以免为了证明问题而修改项目文件。

### 影响

- 直接推翻简报中的“deny rules still win”这一无条件陈述。
- 影响 create 和 modify：modify 可命中同一文件；create 可通过不同大小写的既有父目录避开目录 deny。
- receipt 即使记录了实际执行，也无法补回执行前缺失的人类授权。

### 建议验收条件

1. 规则匹配 subject 与 Windows 最终对象解析使用同一套规范化语义。
2. deny 在规范化后的项目相对对象身份上重新评估，并且发生在任何 mode 自动批准之前。
3. 增加真实 Windows integration test：deny 使用一种大小写，proposal 使用另一种大小写，断言无写入、无 mode approval receipt。
4. 同时覆盖 existing-file modify 和 existing-parent create。

---

## EA-02 — process rule subject 丢失 argv 边界

**优先级：P1 / High**
**类型：授权 subject 非单射 / confused deputy**
**状态：编码冲突已由代码和现有测试确认；未运行恶意脚本**

### 前置条件

1. 用户曾对一个结构化 process proposal 选择持久化 allow。
2. Agent 可控制后续 argv 内容。
3. 项目中存在一个文件名包含空格的可执行脚本或 operand。

### 具体序列

假设项目中同时存在：

```text
safe.js
safe.js --version
```

第一次请求：

```json
{ "program": "node", "argv": ["safe.js", "--version"] }
```

host 生成 subject：

```text
node safe.js --version
```

UI 推荐的 durable pattern 为：

```text
node safe.js *
```

第二次请求：

```json
{ "program": "node", "argv": ["safe.js --version"] }
```

该请求生成完全相同的 subject：

```text
node safe.js --version
```

规则和安全 shape 检查会把它视为已批准的同一命令；executor 却把原始单元素 argv 交给 Node，打开的是文件 `safe.js --version`，而不是以 `--version` 作为第二参数执行 `safe.js`。

### 代码证据

- [`canonical_process_subject`](../../crates/kernel-policy/src/permission_rules.rs#L326-L336) 直接使用 ASCII 空格连接 argv，没有长度前缀、转义或结构化编码。
- [`process_subject_confirmed_for_allow`](../../crates/kernel-policy/src/permission_rules.rs#L415-L477) 随后又按空格拆分 subject，无法恢复原始边界。
- argv 校验 [允许单个参数包含普通空格](../../crates/agent-protocol/src/tool/validate.rs#L516-L535)。
- UI pattern 建议基于 [`subject.split(/\s+/)`](../../apps/desktop/src/features/permissions/permissionPattern.ts#L53-L63)，进一步固化了扁平 subject。
- runtime host 通过 [`canonical_process_subject(sealed.program(), sealed.argv())`](../../crates/runtime-host/src/host/process.rs#L984-L1003) 进行规则评估。
- executor 最终使用 [`sealed.execution_argv()`](../../crates/tool-executor/src/process.rs#L1130-L1137)，即原始 argv，而不是规则看到的拆分结果。
- 现有测试明确固定 [`["two words", "a/b"] -> "python two words a/b"`](../../crates/kernel-policy/tests/permission_rules.rs#L27-L39)。

### 观察与未观察

**已观察：**focused permission test 通过，并明确固定含空格参数被无转义扁平化的行为。

**代码允许：**两个结构化含义不同的 argv 得到相同 subject，命中同一 allow rule，实际执行仍保持不同 argv。

**未执行：**没有创建并运行恶意 Node 文件；不需要依赖 shell parsing 即可形成冲突。

### 影响

- 在已经保存 allow rule 的场景下，后续语义不同的进程可以不经过新的人类决定而执行。
- durable receipt 可以记录原始 argv，但只能事后揭示错误，不能恢复授权有效性。
- 该问题不同于简报第 7.9 项“解释器确认是有限表”：即使解释器表完整，只要 subject 丢失边界，冲突仍然存在。

### 建议验收条件

1. 规则 subject 改为边界无歧义的规范编码，例如受版本控制的 JSON array、长度前缀编码或逐 argv component 的结构化 rule model。
2. UI 不再从显示字符串反推规则；规则建议直接消费结构化 program/argv。
3. 增加 collision matrix，至少固定空格、空参数、连续空格、引号、反斜杠和 Unicode 参数。
4. receipt、规则匹配和审批 UI 应共享同一个有版本的 canonical representation。

---

## EA-03 — renderer 实际持有 authority，且原生入口依赖 renderer-only 校验

**优先级：P2 / Medium**
**类型：信任边界声明与代码不一致 / defense-in-depth 缺口**
**状态：两条独立原生边界证据均确认；未发现 agent-to-renderer 注入入口**

### 前置条件

- WebView 同源 renderer 中出现任意脚本执行，或攻击者能够直接调用已暴露的 Tauri IPC。
- 对写审批路径：当前 conversation 存在 pending write proposal。
- 对规则路径：调用者能调用 `add_permission_rule`。

### 具体序列 A：回收并使用 write approval grant

1. renderer 调用 list-write-proposals。
2. host DTO 返回原始 `approval_grant`。
3. renderer 把 grant 保存在 ref map。
4. 同源代码读取/截获 grant，直接调用 decide-write-proposal，并提交 `approve_once`。
5. host 无法区分真实用户点击与同源脚本直接 invoke。

### 具体序列 B：绕过前端空 pattern 校验

1. renderer 绕过 TypeScript gateway，直接调用 `add_permission_rule`。
2. 传入 global process allow 和空字符串 pattern。
3. Tauri command 不检查空值，SQLite validator 也允许空值。
4. kernel 明确把空 pattern 视为 match-all。
5. 后续可确认 shape 的 process subject 自动命中该规则。

### 代码证据

- write proposal DTO 直接返回 [`approval_grant`](../../apps/desktop/src-tauri/src/conversation_service/write_service.rs#L137-L163)。
- renderer 在 [`grantByProposalRef`](../../apps/desktop/src/features/workbench/Workbench.tsx#L351-L365) 中保存 raw grant，并在批准时 [回送给 gateway](../../apps/desktop/src/features/workbench/Workbench.tsx#L580-L602)。
- process DTO 形成直接对照：它只返回 `decision_available`，并明确声明 [raw grants stay host-private](../../apps/desktop/src-tauri/src/conversation_service/exec_service.rs#L17-L38)。
- native [`add_permission_rule_impl`](../../apps/desktop/src-tauri/src/permission_rule_commands.rs#L85-L112) 不拒绝空 pattern。
- SQLite [`validate_pattern`](../../crates/storage-sqlite/src/permission_rules.rs#L483-L489) 只检查长度上限和 NUL/CR/LF，没有非空约束。
- 只有 TypeScript gateway 在 [`pattern.trim()` 后拒绝空值](../../apps/desktop/src/lib/permissionRulesGateway.ts#L300-L334)。
- kernel 将 [`pattern.is_empty()` 直接判定为匹配](../../crates/kernel-policy/src/permission_rules.rs#L339-L343)。

### 观察与未观察

**已观察：**raw grant 确实进入 renderer DTO；原生 command/storage 接受空 pattern 的代码路径成立。

**代码允许：**任意同源 renderer 代码可利用上述能力，不需要修改 Rust 进程内存。

**未观察：**审查的已提交 baseline 中，agent 消息通过 React 文本节点渲染；未发现 `dangerouslySetInnerHTML`、程序化 fetch/XHR/WebSocket 或已成立的 agent-to-renderer script injection。因此本发现不能表述为“agent 目前可直接利用”。

### 影响

- 简报“renderer is a view; it never holds authority”的声明不成立。
- 一旦未来出现 renderer 注入或第三方 UI 依赖被攻破，攻击面会从 UI compromise 直接扩展到宿主写入和持久化 allow rule。
- 前端校验不是授权边界；同一约束未在 host/storage 重复执行即属于 fail-open seam。

### 建议验收条件

1. write grant 保持 host-private；renderer 只获得非 authority 的 availability bit。
2. 决定命令由 host 通过 proposal ID、window/project scope 和一次性用户动作上下文查找 grant，而不是接收 renderer 提供的 raw grant。
3. pattern 的非空、trim、长度、scope/action 不变量同时在原生 command 和 repository/domain 层验证。
4. 增加 direct-invoke 负向测试，证明绕过 TypeScript gateway 仍无法创建空 pattern 规则。
5. 保留 process DTO 当前的 host-private grant 设计作为 write DTO 的一致性基准。

---

## EA-04 — 截断 diff 仍可批准完整 sealed content

**优先级：P2 / Medium**
**类型：批准对象与可见对象不一致**
**状态：截断和完整写入链均由代码与 focused tests 确认；未点击执行真实写入**

### 前置条件

1. Agent 提交不超过 262,144 UTF-8 bytes 的 write content。
2. unified diff 超过 4,000 行，或因旧内容、diff 标记与输出开销超过 262,144 bytes。
3. 用户在看到“差异已截断”后仍点击“批准一次”。

### 具体序列

1. Agent 把易于接受的内容放在前 4,000 个 diff 输出行中。
2. 需要隐藏的尾部内容放在截断点之后。
3. host 只把 bounded unified diff 和 `truncated=true` 发给 renderer，不提供完整 proposed content。
4. UI 显示“差异已截断”，但批准按钮只检查一般 decision lock，不检查 `diff.truncated`。
5. 用户点击批准后，executor 把完整 `sealed.proposed_content()` 写入磁盘。

### 代码证据

- write content cap 为 [262,144 bytes](../../crates/agent-protocol/src/tool/types.rs#L61-L68)。
- diff cap 为 [262,144 bytes / 4,000 lines](../../crates/tool-executor/src/write_diff.rs#L11-L17)，超过任一上限即 [应用 truncation marker](../../crates/tool-executor/src/write_diff.rs#L200-L213)。
- UI 只显示 [`差异已截断`](../../apps/desktop/src/features/workbench/WriteDiffPreview.tsx#L40-L47)。
- 批准按钮 [`disabled={decisionLocked}`](../../apps/desktop/src/features/conversations/WriteApprovalBar.tsx#L218-L227)，没有 diff/truncated 输入。
- renderer DTO 只有 bounded [`unified` 和 `truncated`](../../apps/desktop/src-tauri/src/conversation_service/write_service.rs#L50-L60)，没有完整 proposed content。
- executor 最终写入完整 [`sealed.proposed_content()`](../../crates/tool-executor/src/write.rs#L446-L481)。

### 观察与未观察

**已观察：**`keeper-tool-executor` 的 write-diff focused suite 12/12 通过，其中包含 line/byte truncation 行为。

**代码允许：**人类看到的对象是 bounded diff，实际执行对象是完整 sealed content。

**未执行：**没有通过桌面 UI 批准一个真实的 4,000+ 行恶意提案。

### 影响

- 该路径仍然包含一次用户点击，因此不是严格意义上的 no-decision bypass。
- 但用户无法检查自己正在批准的完整对象，审批的知情性和内容绑定不成立。
- `truncated=true` 只是警告，不能替代可审核的完整内容或 fail-closed gate。

### 建议验收条件

1. 截断状态下禁止批准，直到用户可查看完整内容或完整、可分页、可验证的表示。
2. 用户决定绑定到明确的 content digest，并在 UI 中显示该 digest 对应的完整审查对象。
3. 新增 UI + host integration test：`truncated=true` 时 approve 不可达；完整内容取回/确认后才恢复。
4. create 和 modify 都覆盖大行数、超长旧内容以及 UTF-8 多字节边界。

---

## EA-05 — role dispatch initiator 未绑定当前项目 Controller

**优先级：P2 / Medium**
**类型：身份与审计归因校验缺失**
**状态：代码确认；未直接调用 Tauri 命令制造记录**

### 前置条件

1. 调用者能直接调用 `propose_role_dispatch` Tauri command。
2. 提供一个语法有效但不属于当前项目 Controller 的 UUID。
3. target slot 是当前项目中的合法目标。

### 具体序列

1. 调用者传入随机 UUID 作为 `initiator_slot_id`。
2. Tauri command 正确检查 conversation/project/window，并读取 project slots。
3. `build_assignment_input` 只解析 initiator UUID；它仅使用 project slots 解析 target。
4. domain 层只判断 initiator UUID 是否恰好等于 target UUID。
5. 随机 UUID 因而绕过 self-dispatch 条件。
6. dispatch record 原样保存伪造 initiator，形成错误的责任归属。

### 代码证据

- command 获取当前项目 slots 并传给 [`build_assignment_input`](../../apps/desktop/src-tauri/src/role_dispatch_commands.rs#L91-L112)。
- initiator 仅通过 [`parse_dispatch_slot_id`](../../apps/desktop/src-tauri/src/role_dispatch_commands.rs#L117-L150)，没有在 `project_slots` 中查找，也没有验证角色为 Controller。
- domain [`build_assignment`](../../crates/role-dispatch/src/assignment.rs#L112-L159) 只拒绝 `initiator_slot_id == target.slot_id`。
- runtime ledger [直接复制 initiator](../../crates/runtime-host/src/dispatch.rs#L172-L196)。
- 正常 UI 使用 [`controller.slotId`](../../apps/desktop/src/features/conversations/RoleDispatchPanel.tsx#L230-L235)，但该约束没有下沉到原生边界。

### 观察与未观察

**已观察：**`pillow-role-dispatch` 的 35 个 focused tests 全部通过，但没有测试 initiator 必须属于项目且为 Controller。

**代码允许：**可创建错误 initiator 归因，并用随机 UUID 避开 self-dispatch 相等比较。

**未观察：**未通过真实 WebView/Tauri invoke 建立伪造记录；也没有证明该缺口单独绕过后续 process approval。若 proposal 进入现有 process chain，执行仍应要求该链自己的批准。

### 影响

- 违反简报把错误执行记录也视为 governance bug 的判定标准。
- 审计人员无法可靠回答“哪个 Controller 发起了该派发”。
- self-dispatch 保护可以被无项目身份的随机 UUID 绕过，即使最终执行审批仍存在。

### 建议验收条件

1. initiator 不由 renderer 自由提供；由 conversation/current project 的唯一 Controller 在 host 侧派生。
2. 若 API 仍保留 initiator 参数，必须验证其属于项目、角色为 Controller、与 conversation 的 active controller 一致。
3. 增加随机 UUID、其他项目 slot、Librarian slot、target-self 四组 native-boundary 测试。
4. receipt/ledger 必须记录 host 派生的 initiator，而不是调用者声称的 initiator。

---

## EA-06 — Unicode 双向控制字符可伪装进程审批文本

**优先级：P3 / Low**
**类型：审批 UI 视觉完整性 / social engineering**
**状态：输入分类和字符串序列化已实机观察；未做 WebView screenshot**

### 前置条件

1. Agent 可在 process argv 或项目文件名中放入 U+202A/U+202B/U+202D/U+202E 或 U+2066–U+2069 等 bidi formatting characters。
2. 用户依据审批 UI 中的视觉顺序判断命令。

### 具体序列

1. Agent 提交包含 bidi formatting character 的 filename/argv。
2. Rust 校验只拒绝 `char::is_control`；这些字符属于 Unicode `Format`，不是 `Control`。
3. `JSON.stringify` 保留该 Unicode 字符的字面值。
4. `<code>` 节点直接渲染该字符串；CSS 没有 `unicode-bidi: plaintext` 或隔离/转义机制。
5. 浏览器双向算法改变视觉顺序，用户批准的视觉文本可能不对应实际 argv 顺序。

### 代码证据

- process argv 只检查 [`char::is_control`](../../crates/agent-protocol/src/tool/validate.rs#L516-L535)。
- display formatter 使用 [`JSON.stringify(arg)` 并保留 Unicode](../../apps/desktop/src/features/execution/executionTypes.ts#L106-L120)。
- approval UI [原样渲染 `commandText`](../../apps/desktop/src/features/execution/CommandApprovalBar.tsx#L260-L270)。
- command CSS 只有 overflow/whitespace 处理，[没有 bidi isolation](../../apps/desktop/src/styles/execution.css#L205-L224)。

### 观察与未观察

**已观察：**.NET 将上述字符分类为 `Format` 且 `IsControl=False`；Node `JSON.stringify` 输出保留了 U+202E 的 UTF-8 字面值。

**代码允许：**这些字符进入审批文本并参与浏览器 bidi layout。

**未观察：**没有捕获真实 WebView2 截图，也没有运行包含该参数的进程。

### 影响

- 该问题不会直接绕过 host approval，但可以污染人类决定所依据的显示内容。
- 与普通省略号不同，即使展开完整命令，视觉重排仍可能存在。

### 建议验收条件

1. 审批安全边界优先把 bidi formatting characters 显式转义为 `\\uXXXX` 或拒绝。
2. UI 对每个 argv token 使用隔离容器并设置安全的 `unicode-bidi` 规则；CSS 只能作为第二层保护。
3. 增加包含 RTL override/isolate、混合 LTR/RTL 文件名和扩展名的 screenshot test。
4. receipt 的机器可读 argv 保持原始结构，同时提供安全转义后的人工审计表示。

---

## 4. 已验证仍有效的机制

以下结论不是“看起来没问题”，而是本次追踪到了具体执行机制；它们不抵消前述发现：

1. **精确大小写 deny 优先。** `evaluate_permission_rules` 在同一 subject 上先返回 deny，再考虑 allow；focused exact-case host test 通过。EA-01 的问题是 Windows 同一对象可以有不同 subject。
2. **项目文件边界具有 lexical、canonical、symlink/reparse 多层检查。** write executor 在最终原子操作前再次验证 root、parent 和 target。EA-01 不依赖越出项目根目录。
3. **结构化 process 保留 sealed argv。** executor 不把 argv 拼成 shell 字符串；EA-02 正是规则 subject 和执行 argv 表示不同导致的问题。
4. **shell rule 使用保守分段和 conjunctive matching。** 未确认的 shell 形态回退到询问，显式 deny segment 优先。
5. **role task text 没有被发现可进入 program/cwd。** target 构造仍使用结构化 proposed command；EA-05 只涉及 initiator 身份和归因。
6. **provider permission request 不能自授予宿主批准。** ACP permission server path 保持 reject/fail-closed。
7. **已提交 renderer baseline 未发现 agent 消息 HTML 执行链。** agent 文本由 React 文本节点渲染；未发现 `dangerouslySetInnerHTML` 或程序化 renderer outbound network 调用。

---

## 5. 新鲜验证记录

以下命令在本次审计中独立运行，并记录真实退出结果：

| 命令 | 结果 |
| --- | --- |
| `cargo test -p keeper-policy --test permission_rules` | exit 0，16 passed |
| `cargo test -p keeper-tool-executor --test write_diff` | exit 0，12 passed |
| `cargo test -p keeper-storage-sqlite --test permission_rules_repository` | exit 0，8 passed |
| `cargo test -p pillow-role-dispatch` | exit 0，35 passed；doctests 0 |
| `cargo test -p keeper-runtime-host --test governed_write_bridge bypass_write_still_obeys_project_deny_and_never_creates_mode_receipt -- --exact` | exit 0，1 passed，25 filtered |
| scoped `git diff --check` on finding files | exit 0 |

这些绿灯的正确解释是：

- 它们确认当前实现和测试固定的行为。
- EA-01 和 EA-02 利用了测试没有变化的输入维度，而不是要求现有测试变红。
- exact-case deny 测试不能证明 alternate-case Windows alias 也被 deny。
- literal-space-join 测试通过恰好确认 EA-02 的冲突前提。

### 未运行

- `node scripts/run-worker-gate.mjs`
- 全量 `pnpm check`
- 完整桌面 WebView2 E2E
- `PILLOW_MCP_PROBE=1` 真实 provider probe
- NativeAdapter live conversation
- 付费模型或外部网络调用
- 任何会真实修改 deny 路径、执行恶意脚本或生成正式 receipt 的 destructive exploit

未运行原因不是将其视为通过，而是：当前工作树存在与本次审计无关的并发产品修改；真实 provider 和破坏性复现也超出只读审核授权。

---

## 6. 被排除的候选误报

### shell CR/LF 注入

代码只拒绝 NUL，最初看起来可能允许 newline 把一条 `cmd.exe` 字符串分成两条命令。为避免只根据字符串形态报告问题，审计做了单变量控制实验：

- 使用与产品一致的 `cmd.exe /D /S /C` 形态。
- LF 与 CRLF 后的第二条命令均未执行。
- 只有显式 `&` 才执行第二条命令。

因此没有把 CR/LF 作为 process bypass 报告。该结果只适用于本次 Windows `cmd.exe` 观察，不应被扩大为所有 shell/platform 的永久安全证明。

---

## 7. 整改顺序与项目方验收门

### Release-blocking

1. **EA-01：**在 Windows deny alias integration test 通过前，不发布“deny 始终优先”的安全声明。
2. **EA-02：**在 argv canonical representation 无碰撞前，暂停依赖宽泛 durable process allow 作为无提示执行依据。

### Boundary-hardening before security claim

3. **EA-03：**host-private grant + native pattern validation；完成后用 direct-Tauri-call 负向测试验收。
4. **EA-04：**截断内容不得进入可批准状态；验收必须覆盖实际 UI 与 host 执行对象绑定。
5. **EA-05：**initiator 由 host 派生并写入 receipt；项目外、随机、非 Controller ID 全部 fail closed。

### Approval UX hardening

6. **EA-06：**危险 Unicode 可见转义，并以真实 WebView2 screenshot 固定显示结果。

### 每项关闭时必须提供

- 修复前可失败、修复后通过的 focused test。
- Windows 实机结果；不能只给平台无关 unit test。
- 对应 host/native 边界测试；不能只给 renderer gateway test。
- 真实退出码和测试数量。
- 修复涉及文件的 `git diff --check`。
- 未运行的 destructive/live-provider gate 必须继续标为 `NOT_RUN`，不能写成通过。

---

## 8. 审计范围限制

1. 本次是 source audit + focused tests + read-only OS observations，不是渗透测试或发布接受。
2. 没有对 provider 上游源码作独立审计；仅审查 Pillow 声明和宿主控制面。
3. 没有证明 `NativeAdapter` 可用；这是简报已知缺口，未重复列为新发现。
4. 没有审计正在并发变化的 OAuth/browser login 和 settings information architecture 实现。
5. 没有验证生产签名、安装包、自动更新、真实账户或真实设备路径。
6. 报告中的“代码允许”表示完整静态控制流存在；只有明确标注“已观察”的项目才是本次实机动态证据。

---

## 9. 项目方可直接拆分的工单标题

```text
EA-01 [P1] Normalize Windows path identity before project deny and mode auto-approval
EA-02 [P1] Replace space-joined process rule subjects with boundary-safe canonical argv
EA-03 [P2] Keep write grants host-private and enforce rule invariants below renderer
EA-04 [P2] Fail closed when write diff is truncated
EA-05 [P2] Derive and validate role-dispatch initiator in the host
EA-06 [P3] Escape or reject bidi formatting characters in approval text
```

---

## 10. 最终结论

Pillow 的主治理链并非整体失效：proposal、host-side executor、project capability、sealed argv、receipt 和多处 fail-closed 机制都真实存在。但当前仍有两类可绕过授权语义的高优先级差异：

1. **路径规则看到的字符串，不等于 Windows 最终解析的文件对象。**
2. **进程规则看到的扁平字符串，不等于 executor 最终使用的结构化 argv。**

这两类差异均可在已有自动模式或持久化 allow rule 下，使实际副作用不再对应最近一次有效的人类决定。因此本次外部审核的最终状态是：

> **NOT_CLOSED — 不应以当前实现宣称治理边界已完整闭合。**
