# P1 实盘探针结论 —— MCP 路通了,但治理边界是假的

Date: 2026-08-10
Status: **Live-probed. Two findings, one enabling and one blocking.**
Probe: `scripts/probe-mcp-tool-exposure.mjs`(opt-in `PILLOW_MCP_PROBE=1`,不进门)
Runtime: 真实 Grok CLI,`agent --no-leader stdio`,默认 profile(`grok1` 语义,未覆盖 HOME)

---

## 1. 两句话结论

1. ✅ **MCP 是可行路线。** 真实 Grok 会连接客户端提供的 MCP server、列出工具、
   调用工具并把结果用回答案。`docs/research/2026-08-10-tool-exposure-reuse.md`
   里三个 **未确定** 全部答完,而且答案都是有利的。
2. 🔴 **但 `plaintext_agent_profile()` 是无效的。** 用一模一样的生产 profile,
   真实 Grok 照样 `read_file` 读文件、`write` 写文件。
   **"原生工具已禁用"这个治理层赖以成立的前提,不成立。**

---

## 2. 逐条证据

| 问题 | 结论 | 证据 |
| --- | --- | --- |
| Q1 `tools: []` 会不会把 MCP 工具也剥掉? | **不会** | 生产 profile 下 MCP 工具照常被调用 |
| Q2 MCP 工具会不会走 `session/request_permission`? | **不会** | 四次会话零 `agentRequests` |
| Q3 禁用 `search_tool`/`use_tool` 会不会挡住 MCP? | **不会** | 两者在禁用名单里,MCP 仍完成调用 |
| Q4 denylist 挡得住原生**读**吗? | **挡不住** | 调用 `read_file`,诱饵令牌被读出并返回 |
| Q5 denylist 挡得住原生**写**吗? | **挡不住** | 调用 `write`,**文件真的落到磁盘**(读盘验证,不信模型叙述) |
| Q6 `agentProfile` 到底生不生效? | **完全不生效** | 正向 allowlist `tools:["read_file"]`(不含 write)下,写**照样成功** |

Q6 是关键区分:如果只是 denylist 坏了,正向 allowlist 应该能挡住写。它没挡住。
所以 **`_meta.agentProfile` 在 ACP 这条线上是惰性的**,不是"某个字段写错了"。

MCP 侧完整握手每次都齐:`initialize → notifications/initialized → tools/list → tools/call`,
模型侧看到的名字是 `pillowprobe__ping`(印证 README 的 `server__tool` 命名规则)。

---

## 3. 这意味着什么

C3/C4 那整套 `提案 → 一次性批准 → 宿主执行 → durable 收据 → 项目隔离`
**在真实代理下可以被整个绕过**。代理不需要发提案,它直接写盘就行。

此前的判断链是这样错下来的:

1. 先认为"最大阻塞是批准粒度" —— 错,因为没有工具就没有提案。
2. 再认为"真实代理没有手" —— **也错**。它有手,只是那双手不受我们管。
   `disallowedTools` 让我们以为把手绑住了,实际没有。

**教训重复了第三次:从配置代码推断运行时行为。** 前两次是
批准粒度和契约接缝,这次是工具禁用。规矩:**边界必须有实盘证据,不能靠读代码相信。**

---

## 4. 混淆项(诚实记录)

本机默认 profile 的 `config.toml` 含 `[ui] yolo` 与 `permission_mode`。
这**可以**解释为什么四次会话都没有 `session/request_permission`。

但它**解释不了 Q4/Q5/Q6**:权限模式决定"要不要问用户",不决定"工具存不存在"。
被真正移除的工具,任何权限模式下都调不到。而且——

> **Pillow 的安全边界不能建立在终端用户的环境配置上。**

这正是权限设计里那条"绝不从项目目录读配置授权"的同一原则。
把边界寄托在用户 `~/.grok/config.toml` 的 `yolo` 值上,和让项目自己给自己授权是一回事。

**未做**:干净 profile 下的对照。做不了 —— 新建空 HOME 会导致未认证,
而按运维手册,给 slot 2–4 单独设 home 会造出平行凭据库。不值得为此冒账号风险。

---

## 5. 下一步(顺序不能反)

1. 🔴 **先堵边界,再谈接工具。** 在边界修好之前,**不要把 Pillow 指向真实项目**。
   现在接进去,代理可以不经审批改你的代码。
2. 候选修法(需各自验证,**不要挑一个就信**):
   - Grok 侧是否有**真正生效**的工具限制入口(headless `--tools`?
     `config.toml`?ACP 之外的 profile 机制?)—— 需要再探。
   - 若无:Pillow 必须假定**代理拥有完整原生工具**,治理层不能靠"禁用"成立,
     而要靠**隔离**(代理只能看到受控工作区,真实项目在它够不到的地方)。
     这是架构级改动。
3. MCP 治理工具的接入(原任务 #19 的 1–4 步)**技术上已确认可行**,
   但在第 1 步完成前接它没有意义 —— 治理工具旁边就摆着一个不受管的 `write`。

---

## 6. 复现

```bash
PILLOW_MCP_PROBE=1 node scripts/probe-mcp-tool-exposure.mjs
```

不设 `PILLOW_MCP_PROBE=1` 时输出 `status: skipped` 并返回 0,永不进门。
探针只暴露一个无副作用的空操作工具;写探针限定在一次性临时目录内,跑完即删;
记录里的机器路径已在采集时替换为 `<path>`。
