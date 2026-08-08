# Pillow — 桌面 AI IDE「高级感」视觉调研报告

| 字段 | 值 |
| --- | --- |
| 日期 | 2026-08-08 |
| 性质 | **纯调研，零代码改动**（本文件为唯一产出） |
| 工作目录 | `<repo>` |
| 现状依据 | `apps/desktop/src/styles/**`、`docs/references/upstream-baseline.md` |
| 品牌基线 | Concept 08「Mirage Loop」：LILAC HAZE `#DCCBFF` + OFF-WHITE / SMOKE / GRAPHITE / BLACK INK |
| 信号 | `UI-RESEARCH-COMPLETE` |

---

## 0. 执行摘要

用户反馈「纯黑纯白不高级」，与当前 token 实测一致：**不是缺毛玻璃，而是层次阶梯过密、分隔线过弱、强调色泄漏到 hover/active 灰阶、深色下 backdrop-filter 无可模糊内容**。

| 问题 | 现状（代码） | 可执行方向 |
| --- | --- | --- |
| 表面无深度 | `--bg-app`→`--bg-main`→`--bg-elevated` 相对亮度差约 **0.003–0.004**，表面对比约 **1.05–1.07:1**（本报告本地 WCAG 公式计算） | 拉开表面阶梯到 **ΔL\* ≈ 3–6** 或相邻表面对比 **≥1.15–1.25:1** |
| 玻璃读不出 | sidebar/resource/composer 使用 `backdrop-filter: blur(18px)`，但背后是近纯色面板 | 深色默认 **取消依赖 blur 的通透感**；改用 hairline + 顶边高光 + 噪声/微渐变 + 不透明阶梯 |
| 淡紫廉价感风险 | `--bg-hover` / `--bg-active` 直接用 lilac alpha；`glass-glow` 也是 lilac | hover/active 改中性灰；lilac 只留给 **1 个主 CTA + focus + 品牌标 + 选中指示条** |
| 令牌不完整 | spacing 只有 4/8/12；shadow 几乎只有 composer；无 light 主题块 | 补 4px 基阶、阴影阶、motion 令牌、浅色镜像 |

**优先级一句话**：先做「不透明表面阶梯 + 边框纪律 + 强调色纪律」，毛玻璃/亚克力放在验证窗口透明与性能之后。

---

## 1. 现状审计（仓库只读）

### 1.1 已做对的地方

- **品牌色已从蓝原型切到 lilac**（`tokens.css` 注释与 `--accent: #dccbff`）。
- **容器级玻璃**（sidebar / resource-pane / floating composer），注释明确禁止列表行 per-row blur——这是正确约束。
- **选中态用 inset 指示条而非整行紫底**（`.session-row--active`）：方向对，可保留并收敛其它 lilac 泄漏。
- **圆角已成小体系**：6 / 8 / 12 / composer 18。
- **已有** `prefers-reduced-transparency` / `prefers-reduced-motion` 降级。
- **正文对比充足**（见 §5 计算）：primary/secondary 远高于 4.5:1。

### 1.2 做错或做弱的地方（下一轮必改）

| ID | 现象 | 位置 | 为何削弱高级感 |
| --- | --- | --- | --- |
| W1 | 表面阶梯过近 | `--bg-app #0e0e10`、`--bg-main #161618`、`--bg-elevated #1c1c20`、sidebar solid `#141416` | 眼晴几乎分不出「舞台 vs 侧栏 vs 抬升」；像一块漆黑平板 |
| W2 | 深色 glass 依赖 blur | `.app-sidebar`、`.resource-pane`、`.composer-wrap--float` 的 `backdrop-filter` | 背后无可读纹理时，blur ≈ 无；只剩半透明灰片 |
| W3 | hover/active 染色为 lilac | `--bg-hover: rgba(220,203,255,.06)`、`--bg-active: ….1` | 滚动列表一扫全是淡紫雾，强调色贬值 |
| W4 | glass 也掺 lilac | `--glass-glow: … lilac .04` | 结构性表面带品牌色 → 廉价 |
| W5 | 分隔线过淡 | `--border-subtle: smoke @ .10` | 结构线承担 elevaton 职责时对比不足（模拟 ~1.26:1） |
| W6 | spacing/shadow/motion 令牌不全 | 仅 `--space-1..3`、`--shadow-composer`、全局 `120ms ease` | 无法统一节奏；易出现「有的硬、有的飘」 |
| W7 | 字重 550/650 | shell/conversation 多处 | 非标准命名；系统字体可能回落到 500/600，层级不稳定（**判断**） |
| W8 | 无 light 主题 token 块 | 仅 `color-scheme: dark` | 双皮肤并行时易「反相」导致气质断裂 |
| W9 | baseline 仍写「quiet neutral light」 | `docs/references/upstream-baseline.md` | 与 Concept 08 品牌方案需在实现层显式切换叙事，避免实现者继续抄 Grok 浅灰中性默认 |

---

## 2. 六问逐条回答

### Q1. 「高级感」在桌面生产力工具里由什么构成？

不要回答「用毛玻璃」。可实施维度与**数值范围**如下（范围来自公开设计系统抽取 + 通用桌面 UI 惯例；标注「判断」处为本报告综合建议）。

#### A. 层次与深度

| 手段 | 建议数值 / 规则 | 出处 / 性质 |
| --- | --- | --- |
| 表面阶梯（暗色） | 至少 **4 级**：canvas / sidebar|panel / elevated / overlay；相邻级 **ΔL\* 3–6** 或 hex 步进约 **+6–12** 于 R/G/B 同向抬升 | Material / 业界 dark surface 惯例 + 本报告对 Linear 抽取的对照（Refero：`#08090a → #0f1011 → #161718`） |
| 深度表达优先级（暗色） | **1)** 表面亮度阶梯 **2)** hairline 边框 **3)** 1px 顶边 inset 高光 **4)** 仅浮层用外阴影 | Linear 产品 UI 惯例（Refero DESIGN 抽取：elevation 主要靠 border，不是重阴影） |
| 浮层阴影（暗色） | sm: `0 2px 4px rgba(0,0,0,.40)`；xl/modal: `0 4px 32px rgba(0,0,0,.55–.60)` | Refero 抽取 Linear shadow tokens（非官方 Linear 文档，第三方抽取） |
| 绝对不要 | 所有面板同时 blur + 重阴影 + 彩色 glow | **判断** |

**落到 Pillow**：改 `--bg-app` / `--bg-main` / `--bg-elevated` / `--bg-sidebar-solid` 拉开；新增 `--bg-overlay`；面板分隔默认 **border**，shadow 仅 modal/popover/composer。

#### B. 留白节奏

| 令牌建议 | 值 | 用途 |
| --- | --- | --- |
| base | **4px** | 所有间距的倍数 |
| 紧密行内 | 2 / 4 | icon-gap、chip 内边 |
| 控件 | 8 / 12 | 行高余量、列表 item padding |
| 区块 | 16 / 24 | 面板 padding、section |
| 舞台外边 | 20–24 | composer 区水平 padding（现状 22 可保留） |
| 列表行 gap | **1–2px**（现状 session `gap: 1px` 合理） | 紧凑生产力密度 |

Raycast 公开 token 抽取（awesome-design-md / DesignMD 类仓库）：base **8px**，辅以 2/4/12；section 级 96 仅用于营销页，**产品 chrome 不套 96**。

**落到 Pillow**：扩展 `--space-0: 2px` … `--space-6: 24px`（或 4 的倍数命名），禁止裸写随机 7px/9px（现状 project-row `padding: 7px 8px` 可收敛到 8）。

#### C. 字体层级与字距

| 角色 | 字号 | 字重 | 行高 | 字距 |
| --- | --- | --- | --- | --- |
| UI caption / meta | 11–12px | 500–600 | 1.3–1.4 | `+0.02em`～`+0.04em`（全大写标签可 `0.04–0.08em`） |
| 默认 UI / 列表 | **13px**（现状 `--text-sm`） | 400–500 | 1.4–1.5 | `0`～`+0.01em` |
| 正文消息 | 14–15px | 400 | **1.5–1.6** | `0` |
| 面板标题 | 13–14px | 600 | 1.3 | `0`～`+0.01em` |
| 显示/品牌字 | ≥20px | 500–600 | 1.1–1.25 | 略负：`-0.01em`～`-0.02em` |

对照：

- **Linear**（Refero）：体重偏轻（约 400–590），大标题负字距约 `-0.022em`；**避免 700+**。
- **Raycast**（DesignMD 抽取）：body 行高约 **1.6**，字距略正 **0.1–0.4px**。

**落到 Pillow**：

- 去掉 `font-weight: 650/550`，改为 **600 / 500**。
- 增加 `--text-xs/sm/md`、`--leading-tight/normal`、`--tracking-label`。
- 字体栈可保留 system-ui；若要更「精致」，再评估 Inter Variable + `cv01/ss03`（Linear 路径）——**非必须**，成本在字体加载。

#### D. 边框 / 分隔线

| 类型 | 暗色建议 | 浅色建议 |
| --- | --- | --- |
| hairline 结构 | `1px solid` 中性，约 **smoke @ 12–16%** 或固定 `#2a2a30`–`#32323a` | graphite @ 10–14% 或 `#E6E4E0` 级 |
| 强分隔（分区） | smoke @ **20–28%** | graphite @ 16–22% |
| focus ring | **2px** accent 或 accent@55–70% + **offset 2px**（现状合理） | 同结构，accent 需加深（见 Q4） |
| 列表嵌套线 | 保持 1px subtle；不要双线 | 同 |

**纪律**：数据面/侧栏/表格 **用线不用阴影**；只有 **浮动** 元素用阴影（Linear 模式，Refero 明确写 Do/Don’t）。

**落到 Pillow**：把 `--border-subtle` 从 `0.10` 提到约 **`0.14–0.16`**；`--border-strong` 到 **`0.24–0.28`**；验证非文本对比（UI 边界）尽量靠近 **3:1** 对「可操作轮廓」（WCAG 1.4.11），装饰分割可低于 3:1。

#### E. 圆角尺度体系

建议固定 **4 档 + pill**（与现状接近，只需命名与用途锁死）：

| Token | 值 | 用途 |
| --- | --- | --- |
| `--radius-xs` | **4px** | badge、小 chip |
| `--radius-sm` | **6px** | 按钮、输入、icon 按钮方底 |
| `--radius-md` | **8px** | 列表行、小卡片 |
| `--radius-lg` | **12px** | 面板、modal |
| `--radius-composer` / xl | **16–20px**（现状 18 可保留） | 唯一「软」表面：composer |
| pill | **999px** | 发送钮、状态点 |

**不要**：同一层级混用 6/8/10/14 随机值（conversation 气泡现有 `14px` 应并入 md 或独立 `--radius-bubble: 12px`）。

#### F. 阴影与高光的物理一致性

暗色「假玻璃」配方（**不依赖 blur 也成立**）：

```text
background: <elevated solid or 92–96% opaque>
border: 1px solid <hairline>
box-shadow:
  inset 0 1px 0 rgba(255,255,255,0.04–0.07),  /* 顶边受光 */
  0 1px 0 rgba(0,0,0,0.20),                   /* 底边接地（可选） */
  [仅浮层] 0 8px 28px rgba(0,0,0,0.32–0.40);
```

规则：

- 高光 **只在顶边**（或左上 1px），不要四边均匀 glow。
- 外阴影 **只向下**（y > 0），暗色透明度 **0.25–0.45**。
- 彩色外发光（lilac glow）**仅**：焦点、运行中状态、品牌 mark——面积 ≤ 控件本身。

现状 `--shadow-composer` 方向正确；缺 `--shadow-sm/md/lg` 体系。

#### G. 微交互与动效

| 类型 | 时长 | 曲线 | 来源 |
| --- | --- | --- | --- |
| 颜色/透明度 hover | **80–120ms** | `ease` 或 `cubic-bezier(0.2, 0, 0, 1)` | 现状 120ms 可接受；NN/g：简单反馈约 100ms |
| 面板展开 / 侧栏 | **160–220ms** | standard decelerate `cubic-bezier(0.2, 0, 0, 1)` | Material 3 easing 惯例 |
| Modal 入场 | **200–280ms** | 同上 | NN/g：大变化 200–300ms |
| Modal 退场 | **120–180ms** | accelerate `cubic-bezier(0.3, 0, 1, 1)` | 退场快于入场 |
| 禁止 | \>400ms 的 chrome 动画；弹性 overshoot 用于导航 | 生产力工具克制 | **判断** |

**落到 Pillow**：

```css
--ease-out: cubic-bezier(0.2, 0, 0, 1);
--ease-in: cubic-bezier(0.3, 0, 1, 1);
--duration-1: 90ms;
--duration-2: 160ms;
--duration-3: 240ms;
```

全局 `button { transition: … 120ms ease }` 可保留为 `--duration-1` + `--ease-out`；**不要**对 `*` 上 transform 做长动画。

---

### Q2. 毛玻璃在深色主题下为什么容易「读不出来」？替代手法

#### 机制（物理/实现）

`backdrop-filter: blur()` 只对 **元素背后已绘制的像素** 做滤波。要看出「玻璃」，需要同时满足：

1. 前景表面 **半透明**；
2. 背后有 **足够空间频率**（边缘、文字、图片、渐变噪声）；
3. 前景与后景有 **色度/亮度差** 可被模糊后残留。

深色 IDE 典型失败模式（Pillow 正中招）：

- 后景是 **大面积近黑平坦色**（`--bg-main` / `--bg-app` 几乎同阶）；
- blur(18px) 之后仍是均匀深灰 → 视觉上等于 `background: #141416`；
- 半透明再叠一层，对比更糊，**没有通透，只有发脏**。

Microsoft Fluent **Acrylic** 文档写明材料由多层组成：半透明 + blur + **noise 纹理** + tint +（原生里还有 exclusion 对比层）。Web 上只抄 `blur()` 而不抄噪声/ tint，在暗色会特别空。

NN/g 对 glassmorphism 的产品警告：材料应用在 **复杂背景** 上才有层次；上下文菜单盖在桌面壁纸上有效，盖在纯色面板上无效。

#### 深色通透感：替代 / 补充（按推荐顺序）

| 优先级 | 手法 | 可执行参数 | 成本 |
| --- | --- | --- | --- |
| P0 | **不透明表面阶梯** + hairline | 拉开 bg 阶梯；sidebar 用 solid `--bg-sidebar-solid`，去掉无效 blur | 低 |
| P0 | **顶边 inset 高光** + 底/侧微阴影 | `inset 0 1px 0 rgba(250,249,247,0.05–0.08)` | 低 |
| P1 | **1px 渐变描边**（假高光边） | `border-image` 或 mask 渐变：`rgba(255,255,255,.10) → transparent` | 中 |
| P1 | **静态噪声纹理**（Acrylic 核心） | 2–4% opacity 的 tile noise PNG/`svg`，`mix-blend-mode: overlay` | 中 |
| P1 | **极微垂直渐变** | 面板 `linear-gradient(180deg, #1a1a1e 0%, #141416 100%)`，Δ 约 4–8 RGB | 低 |
| P2 | **内容本身制造可模糊源** | 主舞台消息流、代码高亮、软光 orb（**极低 opacity 2–4%** 的 lilac 径向光，放在 canvas 层） | 中 |
| P2 | **blur 仅用于浮层** | modal / popover 盖在 **有内容的** 主舞台上时再开 `blur(12–20px)` + 更高不透明度底（70–85%） | 中 |
| P3 | **系统级 Mica/Acrylic**（窗口） | Tauri `Effect.Mica` / `Acrylic`（见 Q6）——与 CSS blur 是不同管线 | 高 |

**明确建议（Pillow）**：

1. 深色默认：`--glass-blur` **视为可选增强**，solid fallback 为真源。
2. 若保留 class `.surface-glass`：深色下改为  
   `background: var(--bg-elevated); backdrop-filter: none;`  
   仅在检测到「背后有滚动内容」的浮层再启用 blur。
3. 浅色主题才更适合默认玻璃（后景通常更花、更亮）。

---

### Q3. 参考对象拆解（3–5 个）

> **信息来源声明**：下列数值来自公开第三方设计抽取、厂商文档与广泛产品观察。**非**各产品官方完整 Design Token 发布；凡具体 hex/px 均标注来源。未实测安装包像素。

#### 1) Linear（项目管理桌面/Web）

| 维度 | 观察 | 来源 |
| --- | --- | --- |
| 层次 | 近黑 canvas + 小步抬升；**elevation 靠 0.5–1px hairline**，数据面几乎不用大阴影 | Refero Styles「Linear design system」抽取（2025–2026 页面） |
| 表面例 | Void `#08090a`、Carbon `#0f1011`、Obsidian `#161718`、Graphite 边 `#23252a` | 同上 |
| 强调色 | 产品 UI 常见 **单点高对比强调**（Refero 记 acid-lime `#e4f222` 作主行动；营销侧常记 lavender-blue `#5e6ad2`）——**稀缺使用：主 CTA / 指示，不作大面积 wash** | Refero；另有 DesignMD/oh-my-design 记 `#5e6ad2`（营销向，可能与产品 DevTools 主题不同） |
| 圆角 | 按钮/输入 ~6px，卡片 ~12px，pill 9999 | Refero |
| 字体 | Inter Variable，字重上限约 590，大标题负 tracking | Refero |
| 深浅一致 | 同一「精密仪器」气质：浅色也是 **细线 + 克制强调 + 紧凑 8px 节奏**，不是反相彩虹 | 产品公开 UI 观察 + 抽取文档 |

**可学**：表面阶梯 + hairline elevation + 强调色手电筒法则。  
**勿学盲抄**：acid-lime 与 Pillow lilac 色相不同；学纪律不学色值。

#### 2) Raycast（启动器 / 桌面工具）

| 维度 | 观察 | 来源 |
| --- | --- | --- |
| 层次 | 深色 canvas，浮层命令面板；**正字距微开** 让深色 UI 透气 | VoltAgent awesome-design-md / DesignMD Raycast 抽取 |
| 间距 | base 8px，2/4/12 紧凑级 | 同上 |
| 字体 | Inter 系 + OpenType `ss03` 等；body 行高 ~1.6；letter-spacing +0.1–0.4px | 同上 |
| 强调 | 高饱和强调用于选中行/关键，其余中性 | 产品公开截图/观察 |
| 深浅 | 以深色为主产品；浅色若有则保持同一字距与圆角语法 | 观察 |

**可学**：深色下用 **字距与行高** 做「透气」而不是减对比；命令面板浮层阴影有内容可衬。  
**对 Pillow**：composer / 命令类浮层可对标 Raycast 的「一块浮起」，侧栏不要硬抄全屏玻璃。

#### 3) Arc（浏览器）

| 维度 | 观察 | 来源 |
| --- | --- | --- |
| 层次 | 侧栏与内容区分离；**圆角内容舞台**（页面区 inset）制造「窗口中的窗口」 | 公开产品 UI / 社区讨论（非官方 token） |
| 强调 | 空间/文件夹色可个人化，但 chrome 本身克制 | 观察 |
| 材质 | 半透明侧栏在 **有壁纸/多彩内容** 时才高级；纯色失败同 Q2 | 观察 + glass 理论 |

**可学**：主内容区 **inset + 大圆角容器**（例如 stage 对 window 内缩 6–10px、`radius 12–16`）比整窗 blur 更稳。  
**不确定**：Arc 具体 CSS/原生材料参数未公开，不编造 px。

#### 4) Things 3（任务管理，macOS）

| 维度 | 观察 | 来源 |
| --- | --- | --- |
| 层次 | 大量 **留白 + 轻分组**；分割线极克制；层次靠排版而非阴影 | 长期产品 UI 共识 / 公开截图 |
| 强调 | 功能色（完成勾选等）局部出现 | 观察 |
| 气质 | 「纸感、安静」——浅色默认更突出 | 观察 |

**可学**：空状态与列表的 **垂直节奏**；Pillow 对话空态可更 Things、少 chrome。  
**勿抄**：Things 密度低于 IDE，不能把 248px 侧栏改成杂志留白。

#### 5) Craft（文档）

| 维度 | 观察 | 来源 |
| --- | --- | --- |
| 层次 | 卡片化页面块、柔和阴影与圆角；偏内容沉浸 | 公开产品 UI |
| 强调 | 品牌色点缀选择态 | 观察 |
| 深浅 | 浅色纸感 / 深色仍保持卡片圆角语法一致 | 观察 |

**可学**：文档/预览 pane 可用略「内容产品」的卡片；**IDE chrome 仍应更 Linear**。  
**判断**：Pillow 应 **chrome = Linear/Raycast，内容预览 = 略 Craft**，两套密度。

#### 对照小结（给实现）

| 产品 | 深度主手段 | 强调色 | 暗色玻璃 |
| --- | --- | --- | --- |
| Linear | 表面阶 + hairline | 极稀 | 基本不用 |
| Raycast | 浮层阴影 + 字距 | 选中行 | 面板级，背后有桌面/内容 |
| Arc | inset 舞台 + 圆角 | 个性化次级 | 依赖多彩后景 |
| Things | 留白 + 排版 | 功能点 | 少 |
| Craft | 卡片阴影 | 点缀 | 中等 |

Pillow 定位 **工业本地 AI IDE** → 主对齐 **Linear 精密 + Raycast 浮层**，Arc 只借 inset stage，不借全窗亚克力。

---

### Q4. 单一强调色（淡紫 `#DCCBFF`）用法纪律

#### 何时用（允许清单）

| 场景 | 用法 | 备注 |
| --- | --- | --- |
| 主行动 CTA | 实心 fill：发送钮、对话框主按钮 | **每个视图最多 1 个** 实心 lilac 主按钮 |
| 键盘焦点 | `outline: 2px solid` / ring | 可 `accent @ 0.55–0.70` |
| 选中指示 | **2–3px** inset 边条或下划线 | 现状 session-row 正确 |
| 品牌 mark | 图标单色 / 小面积 | `brand-mark.css` |
| 运行中 / live | 小圆点或细进度，可带软 ring | 面积小 |
| 链接（可选） | 正文链接用 **略深 lilac** 或中性+下划线 | 勿大面积 |

#### 何时绝对不用

- ❌ 列表 hover / active 大面积 wash（现状 `--bg-hover` / `--bg-active`）
- ❌ 侧栏、资源 pane、代码区背景 tint
- ❌ 所有 border 默认色
- ❌ 大标题长文着色
- ❌ 成功/警告/错误（保持 `--success/--warning/--danger` 语义独立）
- ❌ 滚动条、分隔线、输入框默认边

#### 避免「到处都是紫色」

1. **中性交互灰阶**与 **品牌色** 分轨：  
   - hover: `rgba(250,249,247,0.04–0.06)`（暗）  
   - active: `rgba(250,249,247,0.08–0.10)`  
2. 统计：**一屏内实心 lilac 控件 ≤ 1；带 lilac 的指示器 ≤ 3**（**判断**启发式）。
3. 需要「选中填充」时用 **中性 elevated**，只留 lilac 边条。
4. `glass-glow` 去掉 lilac，改中性 inset 或删除。

#### 深色 / 浅色明度调整

| 角色 | 深色主题 | 浅色主题 |
| --- | --- | --- |
| 实心 CTA 底 | `#DCCBFF` 可用（亮，衬 `#0E0E10` 字） | **降低明度/提高饱和**：建议区间约 `#7B5FC7`–`#6E56B8`（**判断区间**，需对比校验） |
| CTA 上文字 | `#0E0E10`（现状 `--accent-fg`，对比约 **12.9:1** ✅） | `#FFFFFF` 或 OFF-WHITE，保证 ≥4.5:1 |
| 图标/链接线 | 可用 `#DCCBFF` 或略亮 `#EBE0FF` | 用加深色 `#5C4A9A` 级，**不要**直接用 `#DCCBFF` 做小字（浅底上过亮、偏粉且对比可能贴边） |
| soft fill | `accent @ 10–14%` 仅 focus 内底 | `accent-dark @ 8–12%` |
| hover 实心 | 略提亮 `#EBE0FF`（现状） | 略加深 4–8% L |

**原则**（暗色设计常识 + FourZeroThree 等 dark theme 文）：浅色主题的 brand 应落在色板上 **更深的一级**，暗色主题落在 **更亮的一级**，而不是同一 hex 双向硬套。

`#C7C7CE` 在 `#FAF9F7` 上对比约 **1.6:1** → **绝不能**当浅色次要正文；浅色次要字应近 GRAPHITE `#3E3E46`（约 **10:1** ✅）。

---

### Q5. 无障碍与「高级感」的冲突点

#### 规范锚点（WCAG 2.2）

| 对象 | AA 要求 | 来源 |
| --- | --- | --- |
| 正常文本 | **≥ 4.5:1** | WCAG 1.4.3 |
| 大文本（约 ≥18pt/24px 或 ≥14pt 粗） | **≥ 3:1** | 同上 |
| UI 组件与图形边界（可辨边界） | **≥ 3:1** 对相邻色 | WCAG 1.4.11 Non-text Contrast |
| 纯装饰、禁用、logo | 可豁免 | 1.4.3 例外 |

#### 现状计算（本报告脚本，相对亮度公式）

| 对 | 对比 | 结论 |
| --- | --- | --- |
| `#FAF9F7` on `#161618` | **17.2:1** | 正文极佳 |
| `#C7C7CE` on `#161618` | **10.8:1** | secondary 安全 |
| tertiary 近似 `#9A9AA1` on main | **~6.5:1** | 尚可；再淡需小心 |
| `#DCCBFF` on main | **12.1:1** | 强调可读 |
| 表面 app vs main | **~1.07:1** | 无障碍不考核表面，但 **高级感死穴** |
| 浅色 `#C7C7CE` on `#FAF9F7` | **1.6:1** | 若当字 → **失败** |

#### 在不违规前提下仍显精致的做法

1. **把「高级的低对比」用在非文本**：表面阶梯 1.15–1.25:1、装饰分割 <3:1 可以；**字与可点轮廓** 不降。
2. **两级次要字**：  
   - secondary ≥ 4.5:1（操作标签、会话标题）  
   - tertiary 仅 meta（时间戳），目标 ≥ 4.5:1；做不到就升字号或加字重到「大文本」再吃 3:1。
3. **focus 不用低对比灰环**，用 accent ring（已满足可感知）。
4. **禁用态**可低对比（豁免），但需 **形状/位置** 仍可辨，不只靠变灰。
5. **高对比模式 / 强制颜色**：保留 solid 表面；玻璃与噪声在 `prefers-contrast: more` 或 reduced-transparency 下关闭（现状已有 transparency 降级，可扩展 contrast）。
6. **不要用「减字色」制造层次**，用 **字重 400/500/600 + 字号 + 间距**。
7. 结构边框：默认可低于 3:1；**输入框、checkbox、可拖拽分隔条** 在默认态应尽量 **≥3:1** 或依赖 focus 时达标（实现时二选一并写进验收）。

---

### Q6. Tauri / WebView2 实际约束

#### `backdrop-filter` 性能与行为

| 点 | 状态 | 来源 |
| --- | --- | --- |
| WebView2/Chromium 支持 `backdrop-filter` | 支持（Edge 内核） | MDN / 历史 web.dev 文章 |
| 大面积 blur（侧栏全高 + 资源栏） | 滚动/resize 时 GPU 成本高；低端机可能掉帧 | 业界经验；**未在本机 benchmark** → 标不确定量级 |
| 透明窗 + CSS blur | **经常失效或表现异常** | Tauri issue #15512：透明窗口上 `backdrop-filter` 不可用，需窗口级 effect；WebView2Feedback #4945：透明背景时 blur 渲染异常 |
| 列表行 per-row blur | 应禁止（仓库已禁止） | 正确 |

#### 窗口透明 / Acrylic / Mica（Windows）

Tauri 2 `Effect` 枚举（官方文档 `v2.tauri.app`）：

| Effect | 平台 | 官方备注 |
| --- | --- | --- |
| **Acrylic** | Win10/11 | **resize/drag 性能差**（Win10 v1903+、Win11 build 22000） |
| **Blur** | Win7/10/11(22H1) | Win11 22621+ resize 性能差 |
| **Mica** | **Win11 only** | 更现代的系统材质 |
| Tabbed / TabbedDark / TabbedLight | Win11 | 系统 tab 材质变体 |

代价与可行性：

1. 需 `transparent: true` + HTML/CSS 背景透明，材质才透出桌面——与 **自绘不透明 IDE 底** 冲突；全窗 Mica 适合设置页/轻工具，**重 IDE 通常只局部用或不用**。
2. `window-vibrancy` / Tauri effects：**圆角+亚克力**组合历史上有限制（社区教程明确 rounded + acrylic 问题）；以当前官方文档为准做 spike。
3. WebView2 `DefaultBackgroundColor`：alpha **仅 0 或 255**（微软文档）——不能做「半透明 WebView 底」，只能全透明或全不透明。
4. 系统 Acrylic ≠ CSS blur：系统材料采样 **桌面壁纸/其后窗口**；CSS blur 采样 **WebView 内位图**。混用要理解两套坐标系。

#### WebView2 上不可靠或需实测的效果

| 效果 | 可靠性 | 说明 |
| --- | --- | --- |
| 不透明 CSS 表面 + 阴影 + 边框 | ✅ 可靠 | 主路径 |
| `backdrop-filter` 在 **不透明窗** 内、后景为应用内内容 | ⚠️ 通常可用 | 仍需防大面积 |
| `backdrop-filter` + **透明窗** | ❌ 易坏 | 改用窗口 Effect 或放弃 |
| `mix-blend-mode` 复杂栈 | ⚠️ | 截图/录屏/部分 GPU 异常；慎用 |
| `color-mix()` | ✅ 现代 WebView2 | 现状已用；注意旧 runtime |
| subpixel / 0.5px border | ⚠️ | DPR 缩放发虚；**优先 1px + 透明度** |
| `font-weight: 550/650` | ⚠️ | 视字体可变轴 |
| 整窗 Acrylic 拖动 | ❌ 官方性能警告 | 勿作默认 |

#### 不确定项（本条）

- Pillow 当前 Tauri 窗口是否 `transparent`、是否已设 effects：**本调研未跑应用、未读可能含环境信息的 conf 运行时**；实现前应只读检查 `tauri.conf` 的 window 段（允许，但本轮以样式层为主）。
- 具体机型上 blur 18px 的帧耗时：**未测量**。

**工程建议**：高级感 **不依赖** 窗口亚克力；CSS 层先做实。若做 Win11 Mica，仅实验性、可开关，并写清 Win10 fallback solid。

---

## 3. 可执行建议清单（按优先级）

成本：低 = 只改 token/少量 CSS；中 = 多文件样式 + 组件 class；高 = 原生窗口/材质/性能验证。  
收益：对「高级感」主观提升与用户反馈匹配度的 **判断**。

| # | 建议 | 改哪里 | 建议数值 | 成本 | 收益 | 对应问题 |
| --- | --- | --- | --- | --- | --- | --- |
| 1 | **拉开暗色表面阶梯** | `tokens.css` `--bg-*` | 例：app `#0c0c0e`、sidebar `#141418`、main `#18181c`、elevated `#222228`、overlay `#2a2a32`（最终用对比工具微调，目标相邻 **≥1.15:1**） | 低 | **极高** | W1, Q1 |
| 2 | **hover/active 去 lilac** | `--bg-hover` `--bg-active` | `rgba(250,249,247,0.05)` / `0.09` | 低 | **高** | W3, Q4 |
| 3 | **border 略加强** | `--border-subtle/strong` | subtle smoke **0.14–0.16**；strong **0.24–0.28** | 低 | 高 | W5, Q1 |
| 4 | **深色关闭无效 glass blur** | `.app-sidebar` `.resource-pane` 等 | 默认 solid + highlight；blur 仅 modal/composer-over-content | 低–中 | **高** | W2, Q2 |
| 5 | **删除 lilac glass-glow** | `--glass-glow` | 中性或移除 | 低 | 中 | W4, Q4 |
| 6 | **强调色使用审计** | 全 `styles/**` | 实心 accent 仅 primary button；选中用 inset bar | 中 | 高 | Q4 |
| 7 | **补 spacing / radius / shadow / motion 令牌** | `tokens.css` | 见 Q1 表 | 低 | 中 | W6 |
| 8 | **字重标准化 + tracking 令牌** | shell/conversation | 500/600；label `letter-spacing: 0.04em` | 低 | 中 | W7 |
| 9 | **噪声纹理材料（可选）** | 新 utility + 资源 | 2–4% noise overlay on sidebar/elevated | 中 | 中 | Q2 |
| 10 | **主舞台 inset 圆角容器** | `workbench-shell` / conversation | stage 内边距 8–12；内容柱 `radius-lg` | 中 | 中–高 | Q3 Arc |
| 11 | **浅色主题 token 镜像** | `tokens.css` `[data-theme=light]` | OFF-WHITE canvas；GRAPHITE 字；accent 加深 | 中 | 高（双皮肤） | Q4/Q5 |
| 12 | **非文本对比验收脚本** | 测试或 design checklist | 正文 4.5、控件边界 3.0 | 中 | 中（质量门） | Q5 |
| 13 | **Win11 Mica/Acrylic spike** | Tauri window effects | 可关；Win10 solid fallback | 高 | 低–中 | Q6 |
| 14 | **字体 Inter Variable（可选）** | 桌面资源 | 非必须 | 中 | 低–中 | Q1/Q3 |

### 建议「下一轮 UI 优化」最小切片（1 个 PR 就能感知）

只做 **#1 + #2 + #3 + #4 + #5**：纯 token 与 glass 策略，不改交互逻辑。预期：从「漆黑一块 + 紫雾」变为「可分层的石墨仪器」。

---

## 4. 映射到具体令牌（草稿，供实现参考，非本轮修改）

```css
/* 暗色 —— 示例目标，实现时用对比工具锁定 */
--bg-app: #0c0c0e;
--bg-sidebar-solid: #121216;
--bg-main: #18181c;
--bg-elevated: #222228;
--bg-hover: rgba(250, 249, 247, 0.05);   /* 中性，非 lilac */
--bg-active: rgba(250, 249, 247, 0.09);
--border-subtle: rgba(199, 199, 206, 0.15);
--border-strong: rgba(199, 199, 206, 0.26);
--accent: #dccbff;                       /* 深色 CTA / focus */
--accent-muted: rgba(220, 203, 255, 0.12); /* 仅 focus 内底等 */
/* glass：深色默认不透明 */
--glass-bg: var(--bg-sidebar-solid);
--glass-blur: 0px;                       /* 或仅 .is-floating 开启 16px */

--radius-xs: 4px;
--radius-sm: 6px;
--radius-md: 8px;
--radius-lg: 12px;
--radius-composer: 18px;

--space-1: 4px;
--space-2: 8px;
--space-3: 12px;
--space-4: 16px;
--space-5: 20px;
--space-6: 24px;

--shadow-sm: 0 2px 4px rgba(0, 0, 0, 0.36);
--shadow-md: 0 8px 24px rgba(0, 0, 0, 0.40);
--shadow-composer: /* 保留多层，去掉彩色外光 */

--duration-1: 90ms;
--duration-2: 160ms;
--ease-out: cubic-bezier(0.2, 0, 0, 1);
```

浅色镜像（草稿）：

```css
--bg-app: #faf9f7;
--bg-main: #f3f2ef;
--bg-elevated: #ffffff;
--bg-sidebar-solid: #f0eeea;
--text-primary: #0e0e10;
--text-secondary: #3e3e46;
--text-tertiary: rgba(62, 62, 70, 0.72);
--accent: #6e56b8;           /* 加深，需校验 */
--accent-fg: #faf9f7;
--border-subtle: rgba(62, 62, 70, 0.12);
```

---

## 5. 来源说明

| 来源 | 用于 |
| --- | --- |
| 仓库 `apps/desktop/src/styles/tokens.css` 等 | 现状审计 |
| `docs/references/upstream-baseline.md` | Grok App 交互/视觉基线叙事 |
| [WCAG 2.2 Understanding 1.4.3 / 1.4.11](https://www.w3.org/WAI/WCAG22/Understanding/contrast-minimum) | 对比度要求 |
| 本报告 Python 相对亮度脚本 | Pillow token 对比测算 |
| [Refero Linear design system](https://styles.refero.design/style/90ce5883-bb24-4466-93f7-801cd617b0d1) | Linear 表面/圆角/阴影/强调纪律（第三方抽取） |
| VoltAgent awesome-design-md / DesignMD Raycast 条目（搜索摘要） | Raycast 间距/字距/行高 |
| [Microsoft Acrylic 文档](https://learn.microsoft.com/en-us/windows/apps/design/style/acrylic) | blur+noise+tint 材料组成 |
| [Fluent 2 Material](https://fluent2.microsoft.design/material) | Acrylic 使用场景（瞬时表面） |
| [NN/g Glassmorphism](https://www.nngroup.com/articles/glassmorphism/) | 复杂背景依赖 |
| [NN/g Animation duration](https://www.nngroup.com/articles/animation-duration/) | 100ms / 200–300ms |
| [Material 3 Easing and duration](https://m3.material.io/styles/motion/easing-and-duration) | 曲线与时长体系 |
| [Tauri 2 Window Effect API](https://v2.tauri.app/reference/javascript/api/namespacewindow/) | Acrylic/Mica 与性能备注 |
| Tauri #15512、WebView2Feedback #4945（搜索摘要） | 透明窗与 backdrop-filter 问题 |
| [WebView2 DefaultBackgroundColor](https://learn.microsoft.com/en-us/microsoft-edge/webview2/reference/win32/icorewebview2controller2) | alpha 仅 0/255 |
| MDN `backdrop-filter` | 属性语义 |
| 产品公开 UI 观察（Arc / Things / Craft） | 无官方 token 时的定性拆解 |
| **判断** | 未找到权威数值时的工程建议区间 |

---

## 6. 不确定项清单

1. Arc / Things / Craft 的 **精确** spacing/radius token（未反编译、无官方完整公开表）。
2. Linear 产品内强调色：第三方同时出现 acid-lime 与 lavender-blue——可能来自 **不同表面（产品 vs 营销 vs DevTools）**；纪律仍成立，色值勿盲抄。
3. Raycast 抽取文档是否与最新正式版像素级一致。
4. Pillow 当前窗口 `transparent` / 系统 effect 实际配置（未在本轮打开 conf 做原生侧结论）。
5. 目标机群上 `blur(18px)` 全侧栏的 **帧时间**。
6. 浅色 accent 推荐 `#6E56B8` 区间需 **实机对比 + 色相是否仍像 Mirage Loop** 的设计确认。
7. `font-weight: 550/650` 在 Win 默认 Segoe UI 上的最终渲染（可能已回落）。
8. Keeper Bridge worker boot 需 `parentSessionId`——本 worker 未获派发槽位/父会话，**未写入 Keeper 会话记录**（不影响本报告内容）。

---

## 7. 结论

「高级感」对 Pillow 而言，**不是加更多玻璃**，而是：

1. **能被眼睛测量的表面阶梯**（现状最大短板）；  
2. **中性交互灰 + 稀缺淡紫**（避免廉价紫雾）；  
3. **hairline + 顶高光** 代替无效 blur；  
4. **完整但短小的 radius / space / motion 令牌**；  
5. **双皮肤共用语法、分轨明度**；  
6. 原生亚克力仅作可选增强，且正视 Win 性能与 WebView2 透明限制。

完成信号：`UI-RESEARCH-COMPLETE`
