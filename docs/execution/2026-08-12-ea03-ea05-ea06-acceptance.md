# EA-03 / EA-05 / EA-06 验收记录

> 总控:Claude · 日期:2026-08-12
> Worker 报告:`2026-08-12-ea03-report.md`(grok4)、
> `2026-08-12-ea05-role-dispatch-initiator-report.md`(codex)、
> `2026-08-12-ea06-report.md`(grok2)

## 1. 结论

**三单全部通过**,总控补一处测试缺口(§4),另记一条**先于本轮就存在**的风险(§5)。

三份报告都**没有**谎称跑了门 —— 上一轮加进简报的禁令生效了。

---

## 2. 变异验证(不是只看绿)

绿只说明没坏,不说明测试在保护什么。逐条拆掉被测的东西,看对应测试是否变红:

| # | 变异 | 结果 |
| --- | --- | --- |
| 1 | 仓储层 `validate_pattern` 不再拒空 pattern | `empty_and_whitespace_patterns_are_rejected_by_repository` **红** ✓ |
| 2 | 命令层 `command_pattern_or_reject` 不再拒空 | 见 §4 —— **原先全绿** ✗ |
| 3 | EA-05:去掉 initiator 的 Controller 角色检查 | `non_controller_initiator_is_rejected_at_native_dispatch_boundary` **红** ✓ |
| 4 | EA-05:成员归属查找对任何 claimed id 都命中 | 三条边界测试同时 **红** ✓ |
| 5 | EA-06:`escapeBidiFormattingForDisplay` 改成恒等函数 | 6 条 **红** ✓ |
| 6 | EA-03 序列 A:闭合白名单重新放行 `approvalGrant` | `rejects payloads that smuggle … raw grant fields` **红** ✓ |

全部已还原并复验绿。

---

## 3. 逐单结论

### EA-03(grok4)

**序列 A**:write DTO 的 `approval_grant` 换成 `decision_available` 布尔位,
decide 命令不再收渲染层的 grant,host 自己按 proposal 查 live grant。
渲染层删掉 `grantByProposalRef`。闭合 key 白名单同批同步 —— 这一步是必须的,
过去三次 DTO 变更没同步白名单,结果是**载荷被整包拒、UI 静默消失、门还是绿的**。

**序列 B**:空/空白 pattern 在仓储层与命令层双拒,仓储层先 trim 再存。

🔴 **它没做到的那一条,报告自己写了(§7.5)**:去掉 raw grant 之后,
**同源脚本仍然可以对一个 `decision_available` 的 pending 发 `approve_once`**。
拿不到 grant ≠ 发不出 IPC。外审验收条件里写的"一次性用户动作上下文"**没有实现**,
现状是与 process 路径**持平**——而 process 路径正是外审自己指定的一致性基准(条件 5)。

所以准确表述是:**grant 的窃取与重放关掉了,同源调用没有关。** 别对外说成后者也关了。

### EA-05(codex)

先红后绿有真退出码:修复前 `EXIT=101`(7 passed / 3 failed),修复后 `EXIT=0`(10 passed)。
四种情形都在**原生边界**上钉住:合法 Controller、随机 UUID、他项目 Controller、
本项目 Librarian,外加自派仍拒。账本记的是 **host 派生**的 initiator,
并且 assignment projection 若在 register 前被改写会返回
`role_dispatch_initiator_attribution_mismatch` 且不落账。

这一单质量最高:验收条件逐条对应到具体测试,没有含糊。

### EA-06(grok2)

取舍是**转义(人工审计显示面)+ 拒绝(协议层 Format 控制符)**,理由写得清楚:

- 只拒不转义 → 跳过该 crate 的路径(如 `tool-executor` 仍只查 `is_control`)照样能画出生字符
- 只转义不拒 → 协议层继续接受 Format 控制符,sealed proposal 里留着社工向量
- **拒的是 bidi 格式控制符,不是 RTL 字母** —— `ملف.txt` / `שלום.js` 照常通过

机器可读 argv 与收据保持原样,只在人工审计表示上转义,符合验收条件 4。

**残留(报告自陈)**:U+202C、U+200E/F、U+061C、零宽字符**未覆盖**;
`tool-executor` 与 host 侧仍只查 `is_control`。外审点名的那一组覆盖了,弱标记没有。

---

## 4. 🔴 总控补的:命令层那道检查没有测试保护

`direct_invoke_rejects_empty_and_whitespace_process_allow_patterns` 看起来在测命令层,
**实际不是**。变异验证发现:

| 拆掉哪一层 | 那条测试 |
| --- | --- |
| 仓储层校验 | 仍绿(命令层挡住了) |
| **命令层校验** | **仍绿**(仓储层挡住了) |

它是一条**行为测试**,只要两层中任意一层还在就绿。也就是说
**"双层"这个性质本身没有被测**:谁把命令层那道检查删掉,全套测试照绿。

测试断言的是精确码 `permission_rules_invalid_pattern`,本来能区分层次 ——
但 `map_rule_error` 把仓储层的 `InvalidPattern` **映射成了同一个码**,所以分不出来。

**修法**:把命令层的校验抽成可直接调用的 `command_pattern_or_reject`,
加两条直接打在它身上的单测。变异验证:拆掉该检查后,
`command_layer_rejects_blank_patterns_without_consulting_the_repository` **变红**,
而旧的那条端到端测试**仍绿** —— 正好说明它原先钉不住这一层。

**教训**:纵深防御必须**逐层可测**。两层报同一个错误码时,
端到端测试无法证明哪一层在工作,冗余就会悄悄烂掉。

---

## 5. 🔴 记一条先于本轮的风险:一行坏数据会让整个规则集加载失败

`validate_loaded_rules` 遍历规则,**第一条不合法就直接返回 Err** ——
不是跳过那一行,是整包加载失败。

grok4 让 `validate_pattern` 拒空 pattern 之后,多了一条进入该状态的路径:

> 攻击者按 EA-03 序列 B(**修复前**可行)插一条空 pattern 规则
> → 用户升级 → 规则加载永久失败 → **设置页也读不出来,没法在产品里删掉那一行**

方向是对的(fail-closed 好过静默 match-all),但**在产品内不可恢复**。
这和台账里那条"坏 Grok 路径存进设置导致启动不了"是同一个形状。

🔴 **这不是 grok4 引入的** —— 坏 uuid、坏 digest 早就会触发同样的整包失败。
它只是又开了一扇门。**真正该修的是"单行隔离 + 可恢复",不在本单范围。**
已单独记录,不阻塞本轮验收。

---

## 6. 门与证据

- 门:`node scripts/run-worker-gate.mjs` → 见提交信息的 `GATE_EXIT`
  (🔴 **以日志里的 `GATE_EXIT` 为准**,后台任务自己的 exit code 是包在外面那条 bash 的)
- 本轮之前已有一次全门 `GATE_EXIT=0`,当时树里已包含这三份产出 ——
  也就顺带解掉了 EA-06 报告 §6.6 自陈的"桌面全量测试是红的"(那是它写报告时别人还在改)

## 7. 仍然开放

1. **EA-04**(截断 diff 可批准完整 content)—— 简报已写好,与 EA-03 在 write 审批链上冲突,
   现在 EA-03 落地了可以派
2. **EA-03 §7.5 的同源 IPC 调用** —— 需要产品级决定(是否引入一次性用户动作 token),
   process 路径同样适用,应当一起做
3. **§5 的单行隔离与恢复**
4. 🔴 **用户仍未跑过一轮真实任务**
