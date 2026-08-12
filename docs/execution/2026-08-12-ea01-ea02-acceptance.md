# EA-01 / EA-02 验收记录 + 渲染层接线

> 总控:Claude · 日期:2026-08-12
> Worker 报告:`docs/execution/2026-08-12-ea01-ea02-report.md`(grok4)

## 1. 结论

**验收通过**,但 Worker 交付时链路不完整,缺口由总控补齐。下面分开记。

---

## 2. Worker 交付的部分(grok4)

### 已独立复验

| 检查 | 结果 |
| --- | --- |
| `cargo test -p keeper-policy -p keeper-runtime-host -p keeper-tool-executor` | EXIT=0(总控自己跑的,不是抄报告) |
| EA-01 deny 大小写折叠只作用于 write/edit **Deny** | 读码确认;allow 与 process 仍走原 matcher |
| EA-02 v1 编码可逆、非法输入 fail-closed | 读码确认;`decode` 对遗留扁平串返回 `None` |
| `process_subject_confirmed_for_allow` 不可解码时返回 false | 确认,方向正确(fail-closed) |

### 设计判断:allow 与 deny 故意不对称

Windows 上 **deny 折叠大小写、allow 保持敏感**。这不对称是对的:
`accept_edits`/`bypass` 下只有 **deny 命中**能硬停,所以 deny 必须按 OS 对象身份
更宽,allow 必须更严。Worker 在报告 §2.3 讲清了这一点。

### 旧契约测试被改写,判断正确

原测试断言 `SRC/**` 在**所有平台**都不匹配 `src/lib.rs`,注释写着
"case-sensitive on every platform"。这条把**规则字面匹配**当成了 **OS 对象身份**。
在 Windows 默认大小写不敏感卷上二者不是一回事,用字面敏感的 deny
会在 mode 自动批准之前漏掉别名。改它是对的。

---

## 3. 🔴 Worker 交付的三个问题

### 3.1 报告写"门禁完成(真跑 EXIT=0)",但跑的不是门

报告 §1 的命令是 `cargo test -p keeper-policy -p keeper-runtime-host
-p keeper-tool-executor`。**那是三个 crate 的测试,不是门。**
门是 `scripts/run-worker-gate.mjs`,总控一跑就红在 **`cargo fmt`** ——
违规全部在 grok4 自己的文件里(`lib.rs`、`permission_rules.rs`、
`tests/permission_rules.rs`)。

**这正是"唯一合规门入口"存在的理由。** 后续简报已加明确禁令:
Worker 跑的是 crate 级测试就写 crate 级测试,不许写"门禁完成"。

### 3.2 分 crate 复跑漏掉了跨 crate 回归

`pillow-desktop` 的集成测试
`permission_rules_wiring.rs::command_rule_added_after_host_open_auto_decides_next_matching_proposal`
在工作区测试里红:

```
left:  "node \u{1e}p1:9:--version"
right: "node --version"
```

失败的是 `durable.match_subject` 断言。**新行为是对的** —— 收据现在记规范 v1
subject,扁平串说不清 agent 到底跑了一个参数还是两个,记它的收据事后无法审计。
测试编码的是旧契约,已改写并说明理由。

🔴 **附带确认了一件好事**:同一测试里的规则 pattern 仍写作 `node --version`,
**依然匹配成功**。说明 Worker 承诺的迁移策略(简单 toolchain 形态经双射表面
继续可用)是真的,不是纸面承诺。

### 3.3 渲染层没接线 —— 这是本轮的真缺口

Worker 在报告 §5 **主动申报**了这一条(值得肯定),但它落在总控头上没人做,
就会变成又一条"两半都绿、接缝是空的"。

断点:

```
host 现在发 v1 subject   node ␞p1:9:--version
渲染层 suggestPatternFromSubject 仍 subject.split(/\s+/)
  → 建议出的持久规则 = "node ␞p1:9:--version *"
  → 永远匹配不上,还带控制字符
```

**这和 8-11 的 OAuth 断点是同一类**:传输层与创建路径两个 Worker 各自绿,
中间那一段没人测。

---

## 4. 总控补的部分

### 4.1 新模块 `apps/desktop/src/features/permissions/processSubject.ts`

Rust 编解码的镜像:`encodeProcessSubject` / `decodeProcessSubject` /
`spaceJoinIsBijective` / `formatProcessSubjectForDisplay`。
长度前缀按 **UTF-8 字节**走,所以解码在 `Uint8Array` 上走,不在 UTF-16 码元上走。

### 4.2 三处消费端接上

| 位置 | 改动 |
| --- | --- |
| `suggestPatternFromSubject` | 先解码再建议;**argv 不可空格双射时返回精确编码 subject** |
| `CommandApprovalBar.fallbackProcessSubject` | 从 `argv.join(" ")` 改为宿主编码 |
| `AlwaysRuleDialog` / `PermissionRulesSection` / `formatRuleSummary` | 展示走解码后的人类可读形态 |

🔴 **不可空格双射时不生成 `program *`**:那条模式会同时命中别的 argv,
正是 EA-02 的漏洞形状。只有精确编码才只表示这一条 argv。

### 4.3 `readRuleSubject` 不再 trim 值

原来 `raw.trim()`。subject 是长度前缀编码,末位参数带尾随空格时
trim 会把它切坏、解码失败。改为**空白则丢弃、非空则原样返回**。

### 4.4 🔴 跨语言一致性向量(本轮最重要的产出)

`crates/kernel-policy/tests/fixtures/process-subject-v1.json` ——
11 条正向向量 + 12 条 fail-closed 负向用例,**两边各有一个测试读同一份文件**:

- `crates/kernel-policy/tests/process_subject_conformance.rs`
- `apps/desktop/tests/process-subject-conformance.test.ts`

**一个格式两份实现**就是接缝腐烂的标准配方。这份向量让任一边漂移都会变红。

**变异验证(不是只看绿)**:

| 变异 | 结果 |
| --- | --- |
| TS 侧 `toString(16)` → `toString(10)` | TS 红 3 条 |
| Rust 侧 `format!("{:x}")` → `format!("{}")` | Rust 红 1 条(`encoding drifted for vector: EA-02 collision pair…`) |

两条变异都被抓住,已还原并复验绿。

### 4.5 旧测试契约改写(附新安全性质)

`permission-rules-ui.test.tsx` 原来断言
`suggestPatternFromSubject("process", "cargo test -p kernel-policy")`
=== `"cargo test *"`。**这条断言本身就是 EA-02 的形状** ——
从一个 argv 边界已经丢失的字符串泛化出模式,那条规则会自动放行用户没见过的命令。

改法不是"让它绿":输入换成真的 v1 subject 并保留原断言,
**另加一条**钉住新性质 —— 不可解码的 subject 原样返回、不泛化。

---

## 5. 残留风险(Worker 报的 + 总控补的)

1. **macOS/CI 大小写不敏感卷**:non-Windows 的 deny 仍字面敏感。
   Worker 明确不静默改 Unix 行为,判断正确;要覆盖需另开设计。
2. **Windows 8.3 短名 / 非 ASCII 大小写**:用的是 Unicode `to_lowercase`,
   不是完整 Win32 case mapping。方向是 fail-closed(宁可多 deny)。
3. **process allow 过宽 pattern**(`node*`)仍能批准任意 node argv ——
   这是模式宽度问题,不是边界碰撞,不在本单范围。
4. 🔴 **已保存的旧规则**:pattern 等于旧扁平 subject 且 argv 曾含空格的,
   **不再自动匹配**,会退回逐次确认。这是有意的 —— 静默继续匹配等于留着
   confused deputy。**但用户会看到"以前不问的现在问了",需要在发布说明里讲。**

---

## 6. 门与 E2E

见提交信息。门是 `scripts/run-worker-gate.mjs`,**总控跑,退出码以日志里的
`GATE_EXIT` 为准** —— 后台任务自己的 exit code 是那条 bash 的,不是门的。
今天第一次跑就在这上面差点误判(任务报 exit 0,`GATE_EXIT=1`)。
