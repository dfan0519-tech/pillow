# Pillow — 槽位模型选择器（渲染层）执行报告

日期：2026-08-11

实现状态：`COMPLETE`

Worker 槽位：grok1（写码）。总控：Claude。

## 0. 范围

- **允许写入**：`apps/desktop/src/**`、`apps/desktop/tests/**`、本报告。
- **禁止写入**：`crates/**`、`apps/desktop/src-tauri/**`、`scripts/**`。
- **禁止运行**：cargo、根门禁、desktop E2E。
- **验证命令**：`corepack pnpm typecheck`、`corepack pnpm --filter @pillow/desktop test`。
- **禁止 git 写操作**。报告不含本机绝对路径。

## 1. 目标

将 `RuntimeSlotsSection` 中仅手敲的模型 id，接上宿主已就绪命令：

- `get_runtime_slot_model_catalog` → 目录三态：可用 / 不支持 / 失败
- `set_runtime_slot_model` → 切换并**如实**呈现 `runtimeRestarted`

不支持时回落手敲；`external_cli` 不提供模型选择器；密钥永不进 DOM。

## 2. 实现摘要

### 2.1 闭合网关契约

在 `runtimeSlotsTypes.ts` / `runtimeSlotsGateway.ts` 中接入宿主 DTO（与宿主报告一致）：

**Catalog**（`RUNTIME_SLOT_MODEL_CATALOG_DTO_KEYS`，6 键）：

- `slotId`, `projectId`, `status` (`available` | `unsupported`), `source`, `models`, `reasonCode`

**Switch**（`RUNTIME_SLOT_MODEL_SWITCH_DTO_KEYS`，5 键）：

- `slotId`, `projectId`, `modelId`, `runtimeRestarted`, `sessionRestore`

解析器同时钉住正反：

- 接受：available 真返回 `models[]`；unsupported 为空列表 + `reasonCode`
- 拒绝：smuggled keys / secret 字段；available 带 reason；unsupported 带 models；`runtimeRestarted` 与 `sessionRestore` 配对错误

### 2.2 UI

新增 `RuntimeSlotModelPicker`，挂在列表中每个 **`governed_grok`** 槽位行上（api_key 与 login 均有）：

| 能力 | 行为 |
|---|---|
| 打开/刷新 | 拉目录；显式「刷新目录」按钮 |
| 可用 | `<select>` 列出模型，选中即 `setRuntimeSlotModel`；可选「手输未列出的模型 id」 |
| 不支持 | 非错误：说明原因 + 手敲输入 + 应用 |
| 失败 | alert 固定文案 + 手敲 + 刷新可恢复 |
| 快速切换 | 列表内完成，不必进编辑表单 |
| `runtimeRestarted: true` | 固定文案：已重启、会话上下文不可恢复 |
| `runtimeRestarted: false` | 固定文案：已保存、未激活、下次启动生效（不暗示当前会话已换模） |
| external_cli | **不挂选择器**（不弱化「不受治理」标记） |
| 超长 id | `truncateModelIdForDisplay` + `title` 保留全文 + CSS ellipsis |

创建/编辑表单仍保留手输 `modelId`（创建时尚无 slotId，无法拉目录）。

### 2.3 密钥边界

- 目录/切换 DTO 与既有 slot DTO 同样走 `assertNoSecretPayloadFields`
- 槽位视图仍仅 `secretConfigured` 布尔；无 key 回显

## 3. 退出码表

（无管道接 `tail`；退出码原样。）

| 命令 | 退出码 | 结果 |
|---|---:|---|
| `corepack pnpm typecheck` | 0 | 通过 |
| `corepack pnpm --filter @pillow/desktop test` | 0 | 38 files / **503 tests** 全绿 |

## 4. 三态测试证据

| 状态 | 测试 | 证据（正 + 反） |
|---|---|---|
| **可用** | `runtime-slot-model-picker.test.tsx` → `lists catalog models and selects one via setRuntimeSlotModel` | 正：`<option>` 含 `grok-test` / `grok-beta` / `custom-listed`；`selectOptions` 后 `lastSetModelArgs.modelId === "grok-beta"` 且 snapshot 更新。反：不允许只断言「渲染了」。 |
| **可用 + 手输未列出** | 同文件 → `allows manual entry for an unlisted model id while catalog is available` | 正：选「手输…」后 apply `my-private-finetune-9` 成功写入 mock。 |
| **不支持** | 同文件 → `falls back to manual input with endpoint reason (not an error)` | 正：`MODEL_CATALOG_UNSUPPORTED_ENDPOINT` 文案 + 手输可 apply。反：`select` 不存在；`catalog-error` 不存在（不是失败态）。 |
| **失败** | 同文件 → `shows failure copy, keeps manual entry, and refresh can recover` | 正：error 含「无法拉取模型目录」+ 手输存在；刷新后真正列出 `recovered-a`/`recovered-b`。反：失败时无 select。 |
| **网关闭合** | `runtime-slots-gateway.test.ts` → model catalog/switch parsers + invoke | 正：available 返回真实 models；unsupported 接受。反：smuggled key / 配对错误 / secret 字段拒绝。 |

## 5. `runtimeRestarted` 分支证据

| 分支 | 测试 | 证据 |
|---|---|---|
| `true` | `states runtime restarted when host returns runtimeRestarted true` | 文案 = `MODEL_SWITCH_RESTARTED_MESSAGE`（含「已重启」「无法恢复」）。 |
| `false` | `states saved-for-next-start when host returns runtimeRestarted false` | 文案 = `MODEL_SWITCH_SAVED_INACTIVE_MESSAGE`（含「未激活」「下次启动」）。反：不含「已重启」。 |
| 列表快速切换 | `quick-switches model from the list without opening the edit form` | 表单未打开；`setRuntimeSlotModel` 生效；notice 走 restarted 文案；current 标签更新。 |
| 解析配对 | gateway test `pins runtimeRestarted/sessionRestore pairing` | true→unavailable、false→not_applicable 接受；交叉配对拒绝。 |

## 6. 写入边界审计

`git status` / `git diff --name-only` 仅见：

- `apps/desktop/src/features/settings/RuntimeSlotModelPicker.tsx`（新）
- `apps/desktop/src/features/settings/RuntimeSlotsSection.tsx`
- `apps/desktop/src/features/settings/runtimeSlotsGateway.ts`
- `apps/desktop/src/features/settings/runtimeSlotsTypes.ts`
- `apps/desktop/src/styles/runtime-slots.css`
- `apps/desktop/tests/runtime-slot-model-picker.test.tsx`（新）
- `apps/desktop/tests/runtime-slots-gateway.test.ts`
- `docs/execution/2026-08-11-model-picker-ui-report.md`

确认：

- 未改 `crates/**`、`apps/desktop/src-tauri/**`、`scripts/**`
- 未 stage / commit / reset / checkout 或其他 git 写操作
- 未跑 cargo / 根门禁 / desktop E2E
- 报告无本机绝对路径、无密钥材料

## 7. 残留风险

- **创建表单无目录**：新建槽位时仍手输 model id（尚无 slotId）。创建后列表选择器可刷新。
- **Provider 会话不可恢复**：与宿主一致；UI 在 restarted 时明确说明，下一轮为新 provider 上下文。
- **目录失败 vs 不支持**：非 404/405/501 的远端错误走 fail + 手敲，不会误判为「不支持」。
- **超长 id 显示截断**：展示截断至约 48 字符；完整 id 在 `title` 与提交值中保留。宿主仍有 512 字符上界。
- **未做 desktop E2E / 截图门**：按总控独占；本单仅 unit/typecheck。

`MODEL-PICKER-UI-COMPLETE`
