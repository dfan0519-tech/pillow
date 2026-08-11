# Pillow 槽位 UI 实施报告

状态：实现完成；前端 typecheck 与 `@pillow/desktop` 单测通过。

## 范围与硬边界

- 仅修改 `apps/desktop/src/**`、`apps/desktop/tests/**` 与本报告。
- **未**修改 `crates/**`、`apps/desktop/src-tauri/**`、`scripts/**`。
- **未**执行 cargo / 总门 / desktop E2E / git 写操作。
- 宿主命令与 DTO 键以 `docs/execution/2026-08-11-slots-foundation-report.md` 为准。

## 退出码表

命令均直接执行，未用管道截断输出（避免退出码被吞）。

| 命令 | 退出码 | 结果 |
| --- | ---: | --- |
| `corepack pnpm --filter @pillow/desktop typecheck` | 0 | PASS |
| `corepack pnpm --filter @pillow/desktop exec vitest run tests/runtime-slots-gateway.test.ts tests/runtime-slots-ui.test.tsx tests/settings-panel.test.tsx` | 0 | PASS；38 tests |
| `corepack pnpm --filter @pillow/desktop test` | 0 | PASS；37 files / 475 tests |

## 产品行为摘要

设置面板新增 **运行时槽位** 区块（项目级）：

| 能力 | 行为 |
| --- | --- |
| 增加新槽位 | 按钮文案「增加新槽位」；可选受治理 Grok（login / api_key）或外部 CLI |
| Grok login | `create_grok_login_slot`：显示名 → 使用默认 profile |
| Grok API key | `create_grok_api_key_slot`：显示名 + base_url + model id + api key |
| 外部 CLI | `create_external_cli_slot`：可执行路径 + 调用参数 + 状态参数 |
| 编辑 API-key 槽位 | 表单密钥留空 = 保持原值；gateway **省略** `apiKey` 键 |
| 探活 Grok | `start_runtime_slot`（宿主无独立网络探活命令；以激活成功/失败为探活） |
| 探活外部 CLI | `get_external_cli_status`；展示原样 stdout/stderr（有界） |
| 治理标注 | 读宿主显式 `governance` 字段，不从 kind 推断 |

## 第 6 节测试与断言（非假绿）

| # | 要求 | 测试 | 实质断言 |
| --- | --- | --- | --- |
| 1 | 新增 API-key 完整流程；保存后密钥不在任何 DOM | `runtime-slots-ui.test.tsx` → create flow | 保存后列表出现显示名与「已配置」；`document.body` 文本/HTML/所有 `input.value` **不含**测试密钥；view JSON 不含密钥；`lastCreateApiKeyArgs` 仍记录曾提交密钥（仅 mock 侧） |
| 2 | 编辑留空密钥 → 不覆盖原值 | `runtime-slots-ui.test.tsx` + `runtime-slots-gateway.test.ts` | UI 编辑保存后 `lastUpdateApiKeyArgs` **无** `apiKey` 属性；mock 内原密钥仍在；`buildUpdateGrokApiKeySlotArgs` / Tauri `invoke` 参数同样省略空密钥 |
| 3 | 外部 CLI「不受治理」明文 | `runtime-slots-ui.test.tsx` | 列表横幅与创建表单均 `toHaveTextContent` 标题与正文（含「不经过提案与审批」）；**不是**仅图标 |
| 4 | 探活成功与失败 | `runtime-slots-ui.test.tsx` | Grok 成功：文案含「探活成功」+ `active`；Grok 失败：固定「探活失败」+ 认证/密钥类提示，不含 `sk-`/`apiKey=`；外部 CLI 成功显示 stdout「Logged in using ChatGPT」，失败含「失败」 |
| 5 | 设置面板未回归 | `runtime-slots-ui.test.tsx` + 既有 `settings-panel.test.tsx` | 权限模式/主题/适配器/Grok 路径/保存仍可见可操作；全量 desktop 单测 475 绿 |

契约层额外钉死（防「校验器拒一切也绿」）：

- `parseRuntimeSlotView` **接受**完整合法 DTO，**拒绝**未知键与 `apiKey` 走私。
- allowlist 长度：slot 15 / status 8 / start 5。
- kind/governance 错配拒绝（`governed_grok` 必须 `full`，`external_cli` 必须 `none`）。

## 「不受治理」确切文案原文

**标题（heading）：**

```text
此槽位不受 Pillow 治理
```

**正文（body）：**

```text
它的读写和命令不经过提案与审批。外部 CLI 仅供登记与账号状态查询，不能作为受治理代理启动。
```

**合成横幅常量 `EXTERNAL_CLI_UNGOVERNED_BANNER`：**

```text
此槽位不受 Pillow 治理。它的读写和命令不经过提案与审批。外部 CLI 仅供登记与账号状态查询，不能作为受治理代理启动。
```

列表与创建表单均渲染标题 + 正文（`role="note"` + 警告样式）。受治理 Grok 使用徽章文案「受 Pillow 完整治理」。

## 密钥 UI 处理

1. 输入控件 `type="password"`；保存后草稿立即清空；列表只显示「已配置」/「未配置」。
2. 密钥不进 localStorage / sessionStorage / URL；仅 React 草稿态与一次性 invoke 参数。
3. 编辑留空 → `buildUpdateGrokApiKeySlotArgs` 省略 `apiKey`。
4. catch 路径从不 `console.error` 捕获值；错误映射只用 host `code` → 固定中文，**不回显** host `message`。

## 宿主侧缺字段 / 缺命令（请总控协调，本单未改宿主）

| 缺口 | 影响 | 建议 |
| --- | --- | --- |
| **无 `update_grok_api_key_slot`（或等价更新命令）** | 渲染层已实现编辑 UI 与「空密钥省略」契约；Tauri 会 invoke 该命令名，宿主未实现时映射为 `runtime_slot_update_unavailable` 固定文案 | 宿主补更新命令：输入至少 `projectId, slotId, displayName, baseUrl, modelId`，`apiKey` 可选；省略 = 保持 DPAPI 原值 |
| **无独立 Grok 网络探活命令** | UI 用 `start_runtime_slot` 兼探活/激活；成功表示配置可启动，**不等于**供应商端点 live canary | 若需「不激活、只校验端点/模型/密钥」的探活，另加只读 probe 命令与稳定错误码（endpoint / auth / model） |
| **无删除 / 轮换槽位命令** | UI 无删除入口（与 foundation 残留一致） | 补 delete + 孤儿密文清理 |
| **探活错误码粒度有限** | 启动失败时常落默认「端点/认证/模型」综合文案；无法稳定区分三者 | 宿主在失败路径返回可映射的稳定 code（且 message 仍禁止含密钥） |
| **响应含 `secretRef`** | 渲染层 allowlist 接纳该键，**UI 不展示**其值，只展示 `secretConfigured` | 可接受；若产品要进一步收紧，可改为永不下发 `secretRef` 给渲染层 |

当前宿主已有且本单已接的命令：

- `list_runtime_slots` / `create_grok_login_slot` / `create_grok_api_key_slot` / `create_external_cli_slot` / `get_external_cli_status` / `start_runtime_slot`

## 写入边界审计

| 边界 | 结论 |
| --- | --- |
| 渲染层持久化 | 不写密钥到任何浏览器存储 |
| 响应 DTO | 闭合 allowlist；拒绝 `apiKey` 等密钥键；`secretRef` 仅作不透明引用且不渲染 |
| 错误日志 | 槽位路径不打印 catch 值 |
| 外部 CLI 治理 | 显式 `governance: "none"` + 明文警告；不可启动为受治理代理 |
| Grok 治理 | 显式 `governance: "full"`；探活走 `start_runtime_slot` 激活 |
| 跨项目 | 列表/创建均带 `projectId`；无项目时仅提示，不展示表单 |
| 宿主命令层 | 本单零改动 |

## 残留风险

1. **编辑保存依赖尚未上线的宿主更新命令**；用户点「编辑」并保存时，在宿主补齐前会看到固定「更新暂不可用」类提示。创建路径完整可用。
2. **Grok 探活 = 激活**，会替换进程内活跃 runtime；不是纯只读网络检查，也可能无供应商级「模型不存在」细分码。
3. 外部 CLI 状态 stdout/stderr 为 CLI 原样输出：有界，但 Pillow **不能证明**状态命令只读（foundation 已说明）；UI 已明确不受治理。
4. 明文密钥在提交瞬间仍经过 renderer 内存与 Tauri 反序列化；与 foundation 相同的短暂暴露面。
5. 活跃槽位选择仍是进程内状态；重启后需再次探活/启动（foundation 已记）。
6. 设置面板略加宽（520px）以容纳槽位表单；未引入新设计语言。

## 主要文件

- `apps/desktop/src/features/settings/runtimeSlotsTypes.ts`
- `apps/desktop/src/features/settings/runtimeSlotsGateway.ts`
- `apps/desktop/src/features/settings/RuntimeSlotsSection.tsx`
- `apps/desktop/src/features/settings/SettingsPanel.tsx`（挂载槽位区）
- `apps/desktop/src/styles/runtime-slots.css`
- `apps/desktop/tests/runtime-slots-gateway.test.ts`
- `apps/desktop/tests/runtime-slots-ui.test.tsx`

---

**SLOTS-UI-COMPLETE**
