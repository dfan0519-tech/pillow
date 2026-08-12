# Pillow — 设置页 IA 重做 + 权限两轴化 · 执行报告

**Date:** 2026-08-12  
**Worker:** grok4（写码）· **总控:** Claude  
**完成信号:** `SETTINGS-IA-COMPLETE`

## 0. 边界

| 约束 | 遵守 |
| --- | --- |
| 只改 `apps/desktop/src/features/settings/**`、`styles/**`、`tests/**`、本报告 | 是 |
| 不碰 `crates/**`、`src-tauri/**` | 是 |
| 不碰 OAuth 登录入口文件（`oauthLoginUi.ts`、槽位 OAuth 表单逻辑等） | 是 — 仅把 `RuntimeSlotsSection` 挂到「代理与槽位」分类，未改其内部 OAuth |
| 禁止 git 写 | 是 |
| 不跑门 / E2E | 是 |
| 收口：typecheck + desktop test | 是 |

---

## 1. 退出码表（真跑）

| 命令 | 退出码 | 结果摘要 |
| --- | --- | --- |
| `corepack pnpm typecheck` | **0** | `@pillow/desktop` `tsc --noEmit` 通过 |
| `corepack pnpm --filter @pillow/desktop test` | **0** | **53** files / **651** tests passed |

> 日志以 vitest / tsc 输出本身为准（`Test Files 53 passed`、`Tests 651 passed`）。未接 `tail` 管道截断。

---

## 2. 参考了哪几个 IDE、它们的分组是什么

调研方式：公开文档 / 博客 / 既有 permission-ux 调研（不重做权限模型调研）。

| IDE | 设置骨架 | 来源 |
| --- | --- | --- |
| **VS Code** | 左栏分类导航 + 右侧设置列表；每项一行（标题 + 说明）；顶部 User / Workspace 作用域；可搜索 | 官方 UI 文档、Settings Editor 教程（分类如 Text Editor / Workbench / Extensions） |
| **Cursor** | 继承 VS Code Settings Editor；另有 **Cursor Settings** 侧栏分块（Rules、Models、Features 等），权限/规则与外观拆开 | Cursor Settings → Rules 文档与产品设置入口描述 |
| **Zed** | Settings UI 按功能分组列表（Editor / Theme / Terminal / AI…）；博客 *How We Rebuilt Settings in Zed* 描述「分类浏览 + 单项说明」 | zed.dev/blog/settings-ui；All Settings 参考按段分组 |
| **OpenCode Desktop**（补充，来自 permission-ux 调研） | 竖向 Tab：General / Shortcuts / Servers / Providers / Models；权限在 General 一行，不与路径堆叠 | 本仓 `docs/research/2026-08-12-permission-ux.md` §3 |

**共同模式（本单照做）：**

1. **左侧分类导航**，一次只看一个功能区  
2. **右侧内容**为「标题 + 短说明 + 控件」行  
3. **安全 / 外观 / 运行时** 分家，不把主题和权限排成同一列旋钮  
4. 安装级 / 高级路径单独标出  

---

## 3. Pillow 最终分组表 + 归属依据

| 分类 id | 中文名 | 内容 | 归属依据 |
| --- | --- | --- | --- |
| `security` | **安全与权限** | 两轴说明横幅；全局 Bypass 能力开关；项目层权限模式；权限规则表 | Codex/OpenCode：审批与范围叙事独立；VS Code 把安全相关从外观拆出；用户决策两级模型 |
| `agents` | **代理与槽位** | `RuntimeSlotsSection`（角色、模型、登录/API key/OAuth 入口原样） | Cursor 把 Agent/Rules 独立；槽位是项目运行实体，不是主题 |
| `runtime` | **运行时（高级）** | 作用域（全局/项目）；适配器；Grok 可执行文件；Grok profile（高级） | Q4 结论：路径是安装级覆盖；profile **只影响登录型槽位**，API key 用私有 home |
| `appearance` | **外观** | 主题（全局 only） | 三家 IDE 均把 Theme/Appearance 独立 |
| `diagnostics` | **维护与诊断** | Librarian 体检 | 健康检查与权限旋钮解耦 |

**相对建议稿的调整：**

- 「运行时」与「代理与槽位」拆开（建议稿可合并）：槽位是日常操作，路径是高级安装覆盖，VS Code 也常把 Extensions/Features 与 Advanced 分开。  
- 登录说明条仍挂在对话框顶（`settings-login-guidance`），避免只进「代理」才看见 CLI 登录提示。  
- 默认打开 **安全与权限**（本单最重要面）。

**UI 骨架文件：**

- `apps/desktop/src/features/settings/SettingsPanel.tsx`  
- `apps/desktop/src/styles/settings.css`（加宽面板 + 左导航栅格）  
- 文案：`apps/desktop/src/features/settings/permissionCopy.ts`

---

## 4. T2 文案原文

### 两轴主句（必现）

```text
权限模式只决定要不要问你；文件默认仍只能动当前项目目录。要动项目外，必须另开范围。
```

`data-testid="settings-permission-two-axis-note"`

### 模式选项（不再使用「全开 = 无界」读法）

| value | label | detail |
| --- | --- | --- |
| `default` | 默认：写入与命令都询问 | 每次写入与命令都询问。只改「要不要问你」，不放宽项目目录边界。 |
| `accept_edits` | 接受改动：写入自动，命令询问 | 写入自动批准，命令仍询问。只改「要不要问你」，文件默认仍只能动当前项目目录。 |
| `bypass` | Bypass：写入与命令都自动批准 | 写入与命令都自动批准（仍生成收据；deny 优先）。只改「要不要问你」，不解除项目目录边界。 |

### 保留的既有安全声明

```text
deny 规则始终优先；自动批准仍会生成提案、执行记录与持久收据。
```

### 两级模型说明

- 全局：`允许使用 Bypass` — 「全局层：决定 Bypass 是否可用。关闭后，任何项目都不能选择 Bypass；项目不能突破全局。」  
- 项目锁定：`全局已关闭 Bypass，此项目不可选择该模式。请先在全局层开启「允许使用 Bypass」。`  
- 无项目：`打开项目后可在项目层选择权限模式。全局层只控制 Bypass 是否可用。`

**禁止复现：** 旧文案「全开：写入与命令都自动」已从设置页移除（易读成无界）。

---

## 5. T3 正反测试证据

测试文件：`apps/desktop/tests/settings-panel.test.tsx`

| 用例 | 断言 | 结果 |
| --- | --- | --- |
| **正** `T3 positive: when global Bypass is available, project can select Bypass` | `bypassAvailable: true` → Bypass option **enabled**；保存后 `projectMode`/`effectiveMode` = `bypass` | ✓ |
| **反** `T3 negative: when global Bypass is off, project Bypass option is disabled with reason` | `bypassAvailable: false` → Bypass option **disabled**；可见 `settings-bypass-project-locked`；Mock `setProjectPermissionMode(..., "bypass")` 抛 `permission_bypass_not_available`；勾选全局能力并保存后 option 恢复可选 | ✓ |
| DTO fail-closed | `parsePermissionPolicy` 在 `bypassAvailable:false` 且 `effectiveMode:"bypass"` 时把 effective 压成 `default` | ✓ |
| 两轴文案 | 横幅等于 `PERMISSION_TWO_AXIS_NOTE`；选项 label 不含「无界/无限制/任意目录」 | ✓ |

求值式（renderer + mock）：

```text
effectiveMode = resolve(bypassAvailable, projectMode)
// bypass ∧ !available → default（fail-closed）
// 项目不能突破全局
```

---

## 6. 宿主契约缺口清单（需总控接线）

**现状：** 宿主仍是 installation-wide 单键 `get_permission_mode` / `set_permission_mode`（`PermissionModeSettingsDto { mode, source }`）。本单 **未改 src-tauri**。

Renderer 已就绪：

| 方法 | 命令名（建议） | DTO（closed keys） |
| --- | --- | --- |
| `getPermissionPolicy(projectId?)` | `get_permission_policy` | 见下 |
| `setBypassAvailable(available)` | `set_bypass_available` | 返回完整 policy |
| `setProjectPermissionMode(projectId, mode)` | `set_project_permission_mode` | 返回完整 policy；全局关 Bypass 时拒绝 `bypass` |
| 兼容 | `get_permission_mode` / `set_permission_mode` | 工作台指示器仍用；语义应变为 **effectiveMode** |

### `PermissionPolicyView` 精确键

```ts
{
  bypassAvailable: boolean;
  bypassAvailableSource: "global" | "default";
  projectMode: "default" | "accept_edits" | "bypass" | null;
  projectModeSource: "project" | "default" | null;
  effectiveMode: "default" | "accept_edits" | "bypass";
}
```

Parser：`parsePermissionPolicy`（闭集 allowlist；多键即抛）。  
**加字段 = 破坏性变更**，须与 renderer allowlist 同 PR。

### 宿主实现要点

1. **存储**  
   - 全局：`permission.bypass_available`（bool）  
   - 项目：`permission.mode` @ project scope（`default` \| `accept_edits` \| `bypass`）  
2. **求值** `effective = available ∧ (projectMode == bypass) ? bypass : projectMode`（bypass 被全局关 → fail-closed 到 `default` 或产品约定的次级模式）  
3. **拒绝** 全局 `available=false` 时 `set_project_permission_mode(..., bypass)` → 明确错误码（renderer 已用 `permission_bypass_not_available`）  
4. **Legacy**  
   - 读：无新命令时 Tauri gateway **合成** policy（`synthesizePolicyFromLegacy`：把 `bypassAvailable` 暂当 `true`，projectMode 镜像全局 mode）— 仅保证 UI 可加载  
   - 写：`set_bypass_available` / `set_project_permission_mode` 在宿主缺失时 → `permission_policy_host_missing`（设置页展示错误，不静默谎存）  
5. **执行器** 仍只守项目根；本单不把 Bypass 扩成目录外写。若产品要第二轴「可访问范围」，另开设计。

### 与 grok2 OAuth 的冲突面

- **本单未改：** `oauthLoginUi.ts`、`RuntimeSlotsSection` OAuth 表单/目录、`runtimeSlotsGateway` OAuth 路径。  
- 若 grok2 同时改 `SettingsPanel` 挂载方式或 settings 顶栏登录条，需在合并时保留：左导航骨架 + 安全分类两级权限 + 登录条 testid。

---

## 7. 写入边界审计 + 残留风险

### 写入清单

| 路径 | 变更 |
| --- | --- |
| `apps/desktop/src/features/settings/SettingsPanel.tsx` | 分类导航 + 两级权限 UI |
| `apps/desktop/src/features/settings/settingsTypes.ts` | `PermissionPolicyView`、gateway 接口、求值 helper |
| `apps/desktop/src/features/settings/settingsGateway.ts` | parse/mock/tauri 两级契约 |
| `apps/desktop/src/features/settings/permissionCopy.ts` | **新建** T2 文案常量 |
| `apps/desktop/src/styles/settings.css` | IDE 式布局样式 |
| `apps/desktop/tests/settings-panel.test.tsx` | IA + T2/T3 |
| `apps/desktop/tests/runtime-slots-ui.test.tsx` | 分类导航适配 |
| `apps/desktop/tests/librarian-monitor-section.test.tsx` | 进诊断分类 |
| `apps/desktop/tests/runtime-unavailable-guidance.test.tsx` | 进代理分类看槽位 |
| `docs/execution/2026-08-12-settings-ia-report.md` | 本报告 |

### 未写入

- `crates/**`、`apps/desktop/src-tauri/**`  
- OAuth 登录入口实现文件  
- `Workbench.tsx` 指示器（见下）

### 残留风险

1. **宿主未接线前**：设置页「保存」两级权限会失败（`permission_policy_host_missing`）或仅 mock 生效；legacy 合成把 `bypassAvailable` 暂视为 true，**真实全局关 Bypass 需宿主落库**。  
2. **工作台指示器仍写「全开」**（`Workbench.tsx`，本单边界外）。设置页已去无界读法；指示器短标签待另单与 T2 对齐。  
3. **`App.tsx` 仍只 `getPermissionMode()`**（无 project 上下文）。有效模式在项目切换后是否刷新，依赖宿主把 legacy DTO 的 `mode` 映射为当前项目 effective，或后续给 App 传 projectId。  
4. **目录边界第二轴** 未做 UI/执行器；用户若仍期望「Bypass = 写项目外」，需产品另开范围轴（调研已否决绑在 Bypass 上）。  
5. **从槽位引导进设置** 默认落在「安全」分类；用户需再点「代理与槽位」（测试已跟进）。可选 UX：openSettings 深链分类（要动 App，本单未做）。

---

## 8. 完成信号

**SETTINGS-IA-COMPLETE**
