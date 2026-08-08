# Pillow — 环境光晕 + 磨砂玻璃视觉系统

| 字段 | 值 |
| --- | --- |
| 日期 | 2026-08-08 |
| 性质 | 实现 + 契约加强（无桌面 E2E） |
| 工作目录 | `<repo>` |
| 白名单 | `apps/desktop/src/styles/**`（`brand-mark.css` 除外）、`apps/desktop/tests/visual-shell-contract.test.tsx`、本报告 |
| 信号 | `AMBIENT-GLASS-COMPLETE` |

---

## 0. 执行摘要

核心不是「再加一层 blur」，而是**先在应用底层铺可被透视的淡紫环境色场**，再让侧栏 / 资源栏 / 悬浮 composer / 审批条 / 模态等 **chrome** 以半透明深色 + `backdrop-filter` 读出磨砂感。内容区（对话舞台、代码、列表行）保持**不透明、小圆角、高对比**。

同时落实调研里仍采纳的部分：hover/active 去淡紫、边框加强、表面阶梯拉开。

**不采纳**调研中「深色默认关闭 blur」——本单补上了 blur 成立的前提（环境色场）。

---

## 1. 环境色场（地基）

### 实现

- **位置**：`.app-shell` 与 `.app-loading` 的 `background-image`（在 `workbench-shell.css`）。
- **层级**：色场 = 应用 canvas 底；所有 chrome 叠在其上。`.workbench` 改为 `background: transparent`，使资源栏玻璃能透视到底层色场；`.conversation-surface` 仍 `background: var(--bg-main)`，内容舞台保持冷静不透明。
- **手段**：3 团静态 `radial-gradient`（CSS only，无图片、无新依赖、无动画）。
- **令牌**（两套皮肤同名）：
  - `--ambient-orb-a` / `--ambient-orb-b` / `--ambient-orb-c`

### 参数

| 皮肤 | orb-a | orb-b | orb-c | 几何意图 |
| --- | --- | --- | --- | --- |
| 深色 | `rgba(220,203,255,0.16)` | `rgba(190,170,240,0.11)` | `rgba(220,203,255,0.08)` | 右上 / 左下 / 右下中偏，大椭圆、低不透明度 |
| 浅色 | `rgba(220,203,255,0.20)` | `rgba(200,185,235,0.12)` | `rgba(220,203,255,0.08)` | 同几何；更淡，避免浅色「糊一片」 |

渐变椭圆约：`78%×58% @ 92% 6%`、`68%×52% @ 6% 94%`、`48%×40% @ 72% 72%`，透明截止约 65–70%。

---

## 2. 磨砂表面（仅 chrome）

| 表面 | 选择器 | 参数来源 |
| --- | --- | --- |
| 侧栏 | `.app-sidebar` | `--glass-bg` + blur/saturate + 顶边 highlight + 中性 glow |
| 上下文资源栏 | `.resource-pane` | 同上 |
| 悬浮 composer | `.composer-wrap--float .composer` | `--glass-bg-strong` + shadow-composer |
| 模态 | `.modal-card` | glass-strong + 外阴影；圆角 `--radius-panel`（16px） |
| 审批条 | `.write-approval-bar` | glass-strong；圆角 `--radius-panel` |
| 工具类 | `.surface-glass` | 共享容器级玻璃 |

### 玻璃令牌

| 令牌 | 深色 | 浅色 |
| --- | --- | --- |
| `--glass-bg` | `rgba(14,14,16,0.58)` | `rgba(250,249,247,0.58)` |
| `--glass-bg-strong` | `rgba(30,30,38,0.72)` | `rgba(255,255,255,0.78)` |
| `--glass-border` | smoke @ 0.16 | graphite @ 0.12 |
| `--glass-highlight` | 顶边 inset 1px 微亮 | 顶边更亮（浅色受光） |
| `--glass-glow` | **中性**白边环（**非** lilac） | **中性** graphite 边环 |
| `--glass-blur` | **22px** | **14px**（更轻，防糊） |
| `--glass-saturate` | 1.25 | 1.10 |

面板圆角：**16px**（`--radius-panel`）用于 modal / 审批条；composer 保持 **18px**。**未**使用参考图 28–32px 移动端尺度。全高侧栏/资源栏不设大外圆角（贴边 chrome，避免空隙）。

`prefers-reduced-transparency`：上述表面关掉 blur，回落到 solid sidebar / elevated。

---

## 3. 内容区冷静（硬要求）

| 区域 | 行为 |
| --- | --- |
| `.conversation-surface` | 不透明 `--bg-main`，**无** `backdrop-filter` |
| 代码 / preview / 列表行 | 继续 `--radius-sm/md`（6–8px），无 blur |
| `.session-row` / `.project-row` | 无 blur；hover 用中性 `--bg-hover` |
| 选中会话 | 中性 `--bg-active` + **inset lilac 指示条**（选中态仍属强调色纪律） |
| 终端 / diff / 状态行 | 未改布局；仍落在内容/小控件样式，无容器级以外的 blur |

---

## 4. 强调色纪律

| 用途 | 状态 |
| --- | --- |
| 主 CTA（发送 `icon-btn--primary`、审批通过等） | 仍用 `--accent` / `--accent-hover` |
| Focus ring | `--accent` / `--border-focus`（lilac 系） |
| 品牌标 | 未动 `brand-mark.css` / `BrandMark.tsx` |
| 选中指示 | `session-row--active` inset accent bar |
| **hover / active wash** | 深色改为中性 `rgba(250,249,247,0.06/0.10)`；浅色 `rgba(62,62,70,0.06/0.10)` |
| **glass-glow** | 去 lilac，改为中性边环 |

契约测试新增 `isLilacWash` 断言：`--bg-hover` / `--bg-active` / `--glass-glow` 不得为 lilac wash。

---

## 5. 表面阶梯 — 改后实测对比（WCAG 相对亮度）

公式：\((L_1 + 0.05) / (L_2 + 0.05)\)，本地 Node 计算。

### 深色

| 相邻对 | 改前（调研） | 改后 |
| --- | --- | --- |
| `--bg-app` → `--bg-main` | ~1.05–1.07:1（`#0e0e10`→`#161618`） | **1.163:1**（`#0e0e10`→`#1e1e24`） |
| `--bg-main` → `--bg-elevated` | ~1.05–1.07:1（`#161618`→`#1c1c20`） | **1.201:1**（`#1e1e24`→`#2c2c36`） |
| app → elevated | ~1.10 量级 | **1.396:1** |

### 浅色

| 相邻对 | 改后 |
| --- | --- |
| `#faf9f7` → `#eeebe6`（app→main） | **1.130:1** |
| `#eeebe6` → `#ffffff`（main→elevated） | **1.189:1** |

契约：暗色相邻 ≥ 1.15；浅色相邻 ≥ 1.1。

---

## 6. 关键文本对比度（实测）

### 深色

| 对 | 比值 | 门限 |
| --- | --- | --- |
| `--text-primary` `#faf9f7` on `--bg-main` | **15.760:1** | ≥ 4.5 ✓ |
| `--text-primary` on `--bg-elevated` | **13.125:1** | ≥ 4.5 ✓ |
| `--text-secondary` `#c7c7ce` on main | **9.862:1** | ≥ 4.5 ✓ |
| `--text-secondary` on elevated | **8.213:1** | ≥ 4.5 ✓ |
| tertiary（smoke@0.72 叠在 main 上估算） | **~5.780:1** | ≥ 4.5 ✓ |
| primary on `--bg-code` | **18.800:1** | 内容代码底 ✓ |

### 浅色

| 对 | 比值 | 门限 |
| --- | --- | --- |
| `--text-primary` `#3e3e46` on main | **8.909:1** | ≥ 4.5 ✓ |
| primary on elevated | **10.594:1** | ≥ 4.5 ✓ |
| `--text-secondary` `#5c5c66` on main | **5.558:1** | ≥ 4.5 ✓ |
| `--text-tertiary` `#7a7a84` on main | **3.572:1** | 元数据/caption 角色；≥ 3:1 大字号门限 ✓；**不作正文** |

磨砂半透明叠在色场上时，侧栏/资源栏文字仍以 solid fallback 与较高 glass 不透明度保证可读；`prefers-reduced-transparency` 回落到 solid。

---

## 7. 模糊层数量与性能

| 约束 | 落实 |
| --- | --- |
| **禁止**滚动列表每行 blur | 契约断言 `.session-row` / `.project-row` 无 `backdrop-filter` |
| 仅容器级 | 最多同时约 **4–5** 个模糊层：侧栏 + 资源栏 + 悬浮 composer +（可选）模态或审批条 |
| 色场 | 静态 `background-image`，**无** `filter`/`animation` 动画 |
| 浅色 | blur 14px < 深色 22px，降低 GPU 成本与糊感 |

未在 diff 行、树节点、activity 行上新增 blur。

---

## 8. 两套皮肤令牌

- 同名同数量：在 `THEME_TOKEN_NAMES` 中新增 `--ambient-orb-a|b|c`，双皮肤块均声明。
- 主题不变式：`--radius-panel: 16px` 放在裸 `:root`（与其它 radius 一致）。
- 浅色：光晕更淡、blur 更轻、elevated 抬升到 `#ffffff`（app/main 仍禁止纯白）。

---

## 9. testid / 可访问名 / 几何

| 检查项 | 结果 |
| --- | --- |
| `data-testid` | **未改**任何 TSX；白名单外禁止写入 |
| 可访问名 | **未改**组件文案（上下文资源、工作请求、发送等） |
| 布局几何 | **未改** grid 列定义、sidebar 宽度、min-width 1024；仅背景/边框/阴影/圆角 token 与 glass 视觉 |
| 禁止文件 | 未动 `App.tsx`、`features/**`、`brand-mark.css`、`BrandMark.tsx`、`crates/**`、`scripts/**`、`.superpowers/` |

---

## 10. 命令与 exit code

| 命令 | Exit |
| --- | --- |
| `node scripts/run-worker-gate.mjs` | **0** |
| `corepack pnpm --filter @pillow/desktop build` | **0** |

Worker gate 含：workspace/secrets 检查、typecheck、全量 vitest（含 `visual-shell-contract` 9 tests）、cargo 相关检查、ACP drift。**未**跑桌面 E2E。

---

## 11. 改动文件

- `apps/desktop/src/styles/tokens.css` — 阶梯、中性 hover/active、ambient、glass、radius-panel
- `apps/desktop/src/styles/workbench-shell.css` — 环境色场、侧栏 glass glow、workbench 透明
- `apps/desktop/src/styles/workbench-chrome.css` — composer / resource / modal glass 与 panel 圆角
- `apps/desktop/src/styles/workbench-conversation.css` — 内容舞台注释（行为：保持 solid）
- `apps/desktop/src/styles/write-approval.css` — 审批条容器级 glass
- `apps/desktop/tests/visual-shell-contract.test.tsx` — 加强契约（阶梯对比、中性 wash、ambient、容器 blur）
- `docs/execution/2026-08-08-ambient-glass-report.md` — 本报告

---

## 12. 残余风险

1. **WebView 合成**：`backdrop-filter` 在 Tauri/WebView2 上依赖 GPU；低端机长时间开双侧玻璃栏可能略增功耗（层数已控在容器级）。
2. **资源栏透视**：依赖 `.workbench` 透明 + 色场；若未来在 workbench 上再铺不透明底，玻璃会再次「读不出来」。
3. **浅色 tertiary 3.57:1**：仅适合 caption，勿当正文。
4. **桌面 E2E / 像素截图**：本单禁止；几何与视觉回归由总控 E2E 与人工 QA 确认。
5. **Keeper boot**：本 worker 尝试 bridge boot 返回 400（环境侧）；未影响代码与门禁。

---

## 13. 信号

**AMBIENT-GLASS-COMPLETE**
