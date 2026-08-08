# Pillow 桌面交付形态调研：从「能跑」到「能给别人用」

Date: 2026-08-08  
Mode: **只读调研**（未改源码/配置/测试；未跑 cargo / pnpm build / 门 / E2E）  
Worker: Grok（desktop-delivery research）  
Controller: Claude  
Work directory: repository root `pillow`

---

## Signal

**DELIVERY-RESEARCH-COMPLETE**

---

## 0. 结论摘要

| 维度 | 今天 | 给陌生人用还差什么 |
| --- | --- | --- |
| 打包 | `bundle.active: false`，无 MSI/NSIS | 打开 bundler + 一次可复现的 release 打包流水线 |
| 签名 | 无 | 私测可暂绕（SmartScreen 提示）；公开发布需 Authenticode |
| 自动更新 | 无 `tauri-plugin-updater` | 私测可不做；公开需签名 + 静态托管 + 端侧插件 |
| 运行时前提 | **默认 Grok ACP**；Grok CLI / profile / WebView2 /（Git 可选） | 外部依赖未随包分发；失败路径部分是**静默启动失败** |
| 设置层 S0 | 已接线；可配路径 | **存错路径 → 重启后可能无法启动 UI**（无法再进设置改） |
| 用户文档 | `docs/` 全是内部工程文档 | 无安装/首次运行说明 |
| 产品声明 | ADR-0003 允许「Grok CLI 管登录」的临时对外说法 | 设置 UI 已有文案；**首次启动与错误路径未落地** |

**一句话：**能力上已到「开发者机器可工业验收」；交付上仍停在 Stage 1 路线图 **Slice E（Industrial Hardening）未开工**。最小对外试用不是宏大发布，而是：**可安装包 + 前置条件清单 + 失败可恢复**。

---

## 1. 现状盘点

### 1.1 打包配置实际能产出什么

权威配置：`apps/desktop/src-tauri/tauri.conf.json`

| 字段 | 当前值 | 含义 |
| --- | --- | --- |
| `productName` | `Pillow` | 窗口/产品名 |
| `version` | `0.1.0` | 与 workspace / `@pillow/desktop` 一致 |
| `identifier` | `com.pillow.desktop` | 与 ADR-0002 一致 |
| `build.beforeBuildCommand` | `pnpm build` | 打前端 dist 后再编原生 |
| `build.frontendDist` | `../dist` | Vite 产物 |
| `app.windows[0]` | 1440×960，无系统边框 | 自定义 chrome |
| **`bundle.active`** | **`false`** | **bundler 关闭：`tauri build` 默认不产安装包** |

进一步查证：

- `apps/desktop/src-tauri/Cargo.toml`：依赖 `tauri = { version = "2", features = [] }`，**无** updater / dialog 等分发插件。
- 仓库内**无** `tauri-plugin-updater`、无 Windows 签名配置、无 `bundle.targets` / `windows.webviewInstallMode`。
- 二进制名：`[[bin]] name = "pillow"`（ADR-0002）。
- Release 子系统：`main.rs` 在非 debug 时 `windows_subsystem = "windows"` → **无控制台窗口**。

**因此：配置层今天只保证「开发/验收可跑的桌面应用」；不保证「可安装分发物」。**  
若有人本地手动 `tauri build` 且改配置打开 bundle，才能产 MSI/NSIS；**当前树内配置不会。**

### 1.2 版本号、产品名、标识符、图标

| 项 | 状态 | 残留？ |
| --- | --- | --- |
| 产品/窗口标题 | `Pillow` | 无占位符 |
| 版本 | `0.1.0`（workspace + desktop package + tauri.conf） | 合理的 pre-alpha，非假值 |
| Identifier | `com.pillow.desktop` | 稳定，符合 ADR-0002 |
| 可执行名 | `pillow` / `pillow.exe` | 符合 ADR-0002 |
| 图标集 | `src-tauri/icons/` 齐全（ico/icns/png/Android/iOS/Store 系列） | **视觉上是 Pillow 文件夹/盾牌/代码标识**，不是默认 Tauri 蓝色方标；图标文件非空（如 `icon.ico` ~54KB、`icon.png` ~190KB） |
| SQLite 文件名 | 仍为 **`keeper.db`**（app data 下） | **命名残留**（对用户几乎不可见；对支持文档/诊断有混淆成本） |
| Crate 名 | 大量 `keeper-*` | ADR-0002 已声明为过渡实现标识，非用户可见 |

**未发现** `TODO` / `MyApp` / `com.tauri.dev` 一类产品标识占位符残留。

### 1.3 构建产物体积量级（推断，未实测构建）

本调研**禁止**跑 build，故下列为**堆栈推断**，非本机 `pillow.exe` 实测：

| 组成部分 | 量级推断 | 依据 |
| --- | --- | --- |
| 前端 `dist/` | 通常 **数 MB 内** | Vite + React 壳，无大型编辑器内核嵌入 |
| 原生 `pillow.exe` | 典型 Tauri 2 + 多 crate + **bundled rusqlite** → 常见 **约 10–40 MB 量级** | 工作区含 runtime-host / adapter / process / git / storage 等；未计量 |
| NSIS 安装包（不含 WebView2 离线包） | 约等于 exe + 前端 + NSIS 开销 | Tauri 文档：downloadBootstrapper 额外 **0 MB** |
| + embedBootstrapper | **+ ~1.8 MB** | [Tauri Windows Installer](https://v2.tauri.app/distribute/windows-installer/) |
| + offlineInstaller | **+ ~127 MB** | 同上 |
| + fixedVersion WebView2 | **+ ~180 MB** | 同上 |

**不确定：**本仓库尚未有提交的 release 产物尺寸记录；上述不可当作门禁数字。

### 1.4 机器前提与缺失时的失败表现（核心）

以下按「陌生人机器上打开 Pillow」的依赖列出。  
**清晰报错 vs 静默怪异** 直接决定支持成本。

#### A. WebView2 运行时

| | |
| --- | --- |
| **用途** | Tauri/Wry 渲染壳；无 WebView2 则无 UI |
| **现状** | 开发/验收机通常已有；**打包未配置** `webviewInstallMode`（bundle 关闭） |
| **缺失表现** | **查证（产品侧）：** 本仓库未实现自定义缺省 WebView2 引导页。**查证（平台侧）：** 无运行时则 WebView 创建失败，应用起不来。**推测：** 直接双击未安装器包装的 `pillow.exe` 时，用户可能只看到闪退或系统级错误，**无 Pillow 品牌错误页**。 |
| **安装器路径** | 若将来用 Tauri 默认 `downloadBootstrapper`：安装时联网拉 Evergreen 引导；`skip` 则安装成功后仍可能起不来。 |

#### B. Grok CLI 可执行文件

| | |
| --- | --- |
| **用途** | 默认 runtime = Grok ACP；子进程 `grok agent --no-leader stdio`（`PRODUCTION_ACP_ARGV`） |
| **解析顺序** | settings 链：`env → project → global → default`；路径 `None` 时在 `PATH` 上找 `grok` / `grok.exe` |
| **配置入口** | 设置面板「Grok 可执行文件路径」；env `PILLOW_GROK_EXECUTABLE` |
| **保存时校验** | **仅**非空 + 长度 ≤ 4096（`settings_invalid_path`）；**不检查文件是否存在** |
| **生效时机** | UI 明文：**「运行时适配器与 Grok 路径在重启 Pillow 后生效」** |
| **启动时校验** | `require_executable_file`：必须是现有文件，否则 `runtime_executable_missing` |
| **缺失 / 错路径 — 启动** | `open_conversation_service` 在 `setup` 内 `?` 失败 → **应用初始化失败**。Release 为 `windows` 子系统 → **无控制台 stderr 可见性** → 对双击用户近似 **静默起不来**（支持成本极高）。 |
| **缺失 — 运行中** | 路径在**当前进程**启动时已固定；改设置不热切换。错路径只有重启才暴露。 |
| **失败码（清晰，但未必到 UI）** | `runtime_executable_missing`；消息含 PATH / settings / `fake` 提示（**给开发者的英文消息**，非用户引导页） |
| **绝不做的事** | 缺 Grok **不会**静默回落 Fake（`runtime_composition` 硬约束；ADR-0003 / S0b 报告一致） |

#### C. Grok profile / process home

| | |
| --- | --- |
| **用途** | 子进程 `HOME`/`USERPROFILE` 隔离（`honor_isolated_home`）；**只做路径存在性检查，从不打开 auth 文件** |
| **默认** | 未配置时用当前用户主目录（Windows：`USERPROFILE` 且必须是已存在目录） |
| **配置** | 设置「Grok profile 路径」；env `PILLOW_GROK_PROFILE` |
| **保存时** | 同可执行路径：不验证目录是否存在 |
| **错路径 / 空主目录** | 启动失败码：`runtime_profile_invalid`（路径不是目录）或 `runtime_profile_missing`（解析不出 home）→ 同 **setup 失败 / release 近静默** |
| **配「槽位 2–4」式容器路径** | 产品未文档化；配错 = 登录态找不到 → 见下一节「已登录」 |

#### D. Grok 登录态（CLI 管，Pillow 不存）

| | |
| --- | --- |
| **产品口径（已定）** | ADR-0003 Q9 + 设计 §4.3：**「Grok authentication is managed by the Grok CLI; Pillow does not store that login」** |
| **UI 落地** | 设置面板副标题已有中文：**「Grok 登录由 Grok CLI 管理，Pillow 不存储该登录。」** |
| **适配器行为** | 健康探测：`transport_ready_auth_unverified` → 成功 prompt 后 `healthy`；识别到 auth 类错误 → `authentication_failed`（`health.rs` 短语匹配，不读凭据文件） |
| **桌面 UI** | **未发现** renderer 消费 `probe_state` / `authentication_failed` 的专用「请去 CLI 登录」引导 |
| **未登录时表现** | **推测为主：** 应用能启动（exe + profile 目录在），首次发 turn 失败；错误经 redaction 后进 timeline / `sendError`，用户可能看到通用失败而非「请在终端运行 grok login」 |
| **与设置错路径的对比** | 未登录 = **能进 UI，对话失败**（可支持）；错 exe/profile = **可能进不了 UI**（更糟） |

#### E. Git 可执行文件（可选能力）

| | |
| --- | --- |
| **用途** | 只读 Git Changes（discover / status / diff） |
| **解析** | `PILLOW_HOST_GIT`（若是文件）→ 标准路径 `C:\Program Files\Git\cmd\git.exe` 等 → `PATH` 上 `git.exe`/`git` |
| **缺失表现** | **清晰 fail-closed，应用仍启动。** `GitCommandState.executor = None` → 命令返回 `git_unavailable` / 文案 *「Git read capability is unavailable.」* |
| **是否阻塞主路径** | **否**（对话 / 资源 / 写入审批不依赖 Git） |

#### F. SQLite / 应用数据目录

| | |
| --- | --- |
| **路径** | Tauri `app_data_dir` + `keeper.db`（可用测试覆盖 env，产品默认走 OS app data） |
| **失败** | 无法创建目录或打开 DB → setup 失败（与 Grok 同类启动失败） |
| **用户可见** | Release 近静默；消息侧有 opaque 错误约定，不应泄漏绝对路径 |

#### G. 开发/自动化专用（不应出现在陌生人机器）

| 前提 | 缺失/误用表现 |
| --- | --- |
| `PILLOW_RUNTIME=fake` | 显式 Fake；无真实模型。设置 UI **对所有构建暴露「Fake（开发/测试）」**（与 ADR-0003 Q2「dev builds only」意向**不一致**——见缺口清单） |
| `PILLOW_HOST_GIT` / `PILLOW_TEST_GIT` | 测试/非标 Git；产品 UI 不暴露 |
| 源码树 + Rust/Node 工具链 | 仅开发者路径；陌生人无安装包则不需要 |

#### H. 依赖关系总图（首次运行）

```text
WebView2 ──缺失──► 无 UI（近静默）
    │
    ▼
Pillow setup
    │
    ├─ SQLite ──失败──► 无 UI
    │
    ├─ resolve runtime（默认 grok-acp）
    │     ├─ exe 缺失/非文件 ──► setup 失败 / 无 UI（runtime_executable_missing）
    │     └─ profile 无效 ──► setup 失败 / 无 UI
    │
    ▼
Workbench UI（可开设置、开项目）
    │
    ├─ 发 turn + Grok 未登录 ──► 对话失败（无专用引导，中等清晰度）
    ├─ Git 缺失 ──► Git 面板不可用（清晰码，不拖垮主路径）
    └─ 设置改路径 ──► 仅重启后生效；若改错 ──► 下次无法启动（最坏环）
```

### 1.5 设置层（S0）与「交付模式」口径

设计权威：`docs/superpowers/specs/2026-08-08-settings-credentials-design.md` **§4.3**（ADR-0003 采纳 Q9，未再抄整表）：

| Delivery mode | Credentials in Pillow? | Who can use it | Cost / risk |
| --- | --- | --- | --- |
| **Grok-CLI-auth only（推荐临时）** | No（S0 only） | 已装 Grok CLI 并完成 CLI 登录；Pillow 只指 exe + profile | 无法 onboarding 纯 API-key provider；profile 配错是脚枪 |
| Env 注入密钥（不推荐） | 无持久化 | 运维设 env | 密钥进进程环境；非产品级 |
| Full S1 | Yes | 多 provider / 工业凭据 | 实现 + 安全测试成本 |

**S0 已在树内落地（handoff §4.7–4.8）：** SQLite settings、precedence、设置 UI、composition 读取、env 覆盖可见。  
**仍未解决交付：**「能配路径」≠「配错还能活着改回来」。

### 1.6 文档与产品终点

- `README.md` / `docs/`：**工程验收与设计**，无用户安装手册。
- Roadmap Slice E：*packaging, updater, security review, performance, diagnostics and public alpha documentation* — **Not started**（`docs/roadmap/2026-07-31-stage1-overall-status.md`）。
- 产品终点（handoff §2）：工业级、更多人用、三阶段仍在 Stage 1。

---

## 2. Windows 交付链取舍

### 2.1 MSI vs NSIS（Tauri 2）

**查证（Tauri 文档）：** Windows 可选 WiX **MSI** 或 NSIS **`-setup.exe`**；MSI 仅能在 Windows 主机上构建。

| | MSI (WiX) | NSIS setup.exe |
| --- | --- | --- |
| 形态 | `.msi` | `*-setup.exe` |
| 企业/IT 熟悉度 | 高（GPO、静默 `/quiet`） | 中 |
| 权限模型 | 常 per-machine 叙事 | 默认 **当前用户**（`%LOCALAPPDATA%`），可不提权；可选 perMachine / both |
| 跨平台交叉编译 | 弱 | 文档侧更完整（仍推荐 Windows CI） |
| 与 updater 关系 | 二者都可作为更新包形态（需插件 + 签名） | 同左 |
| **Pillow 私测建议** | 次选 | **首选 NSIS current-user**：少 UAC、分发简单 |

### 2.2 代码签名

**无签名会怎样（查证 + 常识）：**

- 用户常遇到 **SmartScreen / 「未知发布者」**；需「仍要运行」才能继续。
- 企业环境可能直接拦截。
- 安装转化率显著下降（行业共识；具体比例未在本调研计量）。

**取得签名的现实路径（混合：官方文档 + 市场信息；价格标为推测/区间）：**

| 路径 | 说明 | 成本量级 |
| --- | --- | --- |
| OV / 标准 Authenticode | 组织或个人身份校验；CI 可用云 HSM / Azure Trusted Signing | **推测约 $100–400/年** 量级（供应商差异大） |
| EV | 历史上 SmartScreen 声誉更快；**Microsoft 现已说明 EV 不再保证即时 SmartScreen 豁免**（[MS Learn SmartScreen reputation](https://learn.microsoft.com/en-us/windows/apps/package-and-deploy/smartscreen-reputation)，2026 文档口径） | **推测约 $300–600+/年**；对「只为躲 SmartScreen」性价比下降 |
| Azure Artifact / Trusted Signing | Tauri 文档支持；适合 CI | 按 Azure 计费（**未在本单查实时价**） |
| Microsoft Store | 走商店信誉与策略；需 Store 包装与账号 | 商店年费/政策 **未逐项查证** |

**对 Pillow：**

- **少数熟人试用：** 可接受未签名 + 安装说明里写清 SmartScreen 点击路径。
- **公开发布：** 签名几乎硬要求；EV 不再是银弹，重点是**稳定发布者身份 + 下载信誉积累**。

### 2.3 自动更新（Tauri updater）

**机制（查证：Tauri updater 插件文档摘要）：**

- 构建时对安装包生成 **`.sig` 签名**；客户端校验后替换安装。
- 需要：**端侧插件**、**更新公钥配置**、**托管 `latest.json` + 安装包的 HTTPS 端点**（或兼容的 static hosting）。
- Windows 上复用 MSI/NSIS 产物。

**与「本地优先 / 不外发数据」：**

| 关切 | 是否冲突 |
| --- | --- |
| 检查更新 = 客户端请求版本元数据 | **弱冲突**：出站仅版本通道，不上传项目内容；需产品透明披露 |
| 强制遥测 / 账号绑定更新 | **非 Tauri 默认**；若自建则另论 |
| 完全离线环境 | 自动更新不可用；需手工装包通道 |
| 签名私钥 | 不得进仓库；CI secret（符合 AGENTS 密钥规则） |

**建议：** 私测 **不做** auto-update；公开 alpha 再加「可选检查更新」，默认说明只拉取版本清单。

### 2.4 WebView2 分发

| 模式 | 联网 | 体积增量 | 适用 |
| --- | --- | --- |
| `downloadBootstrapper`（默认） | 要 | 0 | **Win10/11 私测默认推荐** |
| `embedBootstrapper` | 要 | ~1.8 MB | 改善部分老系统 MSI 场景 |
| `offlineInstaller` | 否 | ~127 MB | 离线/锁定环境 |
| `fixedVersion` | 否 | ~180 MB | 要钉死 WebView 版本 |
| `skip` | 否 | 0 | **不推荐**（装完可能跑不了） |

Win11 / 较新 Win10 多数已带 Evergreen WebView2；私测仍应用 bootstrapper，避免边缘机器。

---

## 3. 首次运行体验缺口

### 3.1 陌生人第一次打开会看到什么（按今天行为推断）

假设对方拿到 **未签名的 setup 或裸 exe**，且机器是普通 Win11：

1. **SmartScreen** 警告（未签名）→ 需手动继续。  
2. **WebView2** 若无：安装器或启动失败（当前无品牌引导）。  
3. **若未装 Grok 或不在 PATH：**  
   - setup 组合 Grok 失败 → **窗口可能根本不出现**（release 无控制台）。  
4. **若 Grok 在 PATH、profile 默认可解析：**  
   - 进入 workbench（**不是**全屏 onboarding；`App.tsx` 注释：始终进完整 workbench）。  
   - 需自行「打开/注册项目」。  
   - 点设置可见 Grok 登录声明与路径框。  
5. **若未 `grok login`：** 发消息失败，**无**「去 CLI 登录」一等引导。  
6. **Git 未装：** 主流程可用；Git Changes 报 `git_unavailable`。

### 3.2 引导目前是零；最小可用引导应做到什么

| 层级 | 最小内容 |
| --- | --- |
| **安装前（一页纸）** | 系统：Win10/11；装 **Grok CLI** 并完成登录；可选 Git；WebView2 由安装器处理 |
| **安装后首次启动** | 检测 runtime：缺 exe → **可操作错误屏**（打开设置 / 打开文档 / 退出），**禁止静默 setup 失败** |
| **设置** | 已有路径 + 登录声明；应补：存在性校验、保存后预检、「如何登录」外链/步骤 |
| **第一次发消息失败** | 将 `authentication_failed` 类映射为固定中文行动项：在终端完成 Grok CLI 登录后重试 |
| **Fake 适配器** | 正式/试用构建 **隐藏**；仅 dev/test |

### 3.3 ADR-0003 说法如何落地才不困惑

已有：**设置页副标题**。

仍缺：

1. **首次启动或空状态**一句同样口径（工作台顶栏/空项目态）。  
2. **错误映射：** 未登录 ≠ Pillow 丢密码 ≠ 网络随便失败。  
3. **明确不做的事：** 不提供「在 Pillow 内粘贴 API key」直到 S1。  
4. **Profile 文案：** 「进程主目录」对用户难懂 → 试用文档写「一般保持默认；仅多账号隔离时才改」。  
5. **重启生效：** 改路径后必须重启；改错会导致无法启动 —— 必须在改之前做 **exists 校验** 或提供 **安全模式**。

---

## 4. 缺口清单（按阻塞对外交付排序）

工作量：S ≈ 0.5–2 人日，M ≈ 3–8 人日，L ≈ 2 周+（粗估，单人熟悉本仓）。

| # | 缺口 | 阻塞度 | 影响 | 工作量 | 临时绕过 |
| --- | --- | --- | --- | --- | --- |
| 1 | **Grok/composition 失败导致 setup 失败 + release 无控制台** | **P0 硬阻塞** | 陌生人缺 Grok/路径错 → 以为软件坏了；无法进设置自救 | M（延迟组合 + 错误屏；或 safe-mode） | 安装说明强制「先装 Grok 并保证 PATH」；改错后需教用户清 settings DB / 环境变量（支持成本高） |
| 2 | **`bundle.active: false`，无安装包** | **P0** | 只能源码/开发者产物；无标准卸载与 WebView2 引导 | S–M | 熟人 zip 内附 `pillow.exe` + 口头依赖清单（脆弱） |
| 3 | **无面向用户的安装/首次运行文档** | **P0** | 内部 `docs/` 不可发给陌生人 | S | 一页 `TRIAL-README`（私测足够） |
| 4 | **设置保存不校验路径存在 + 仅重启生效** | **P0/P1** | 脚枪：保存成功 → 重启死亡 | S–M | 人工叮嘱；提供「恢复默认」说明 |
| 5 | **未登录无专用引导** | **P1** | 支持工单：「能开不能聊」 | S–M | 文档写 `grok login`（或 CLI 等价流程） |
| 6 | **UI 暴露 Fake 适配器** | **P1** | 试用用户误选后「AI 是假的」 | S | 文档写「勿选 Fake」 |
| 7 | **无代码签名** | **P1 公开 / P2 熟人** | SmartScreen；企业拦 | M + 年费 | 说明「仍要运行」；商店/内网信任 |
| 8 | **无自动更新** | **P2 私测 / P1 公开** | 版本碎片 | L | 每次发新 NSIS + 版本号 |
| 9 | **Git 非安装依赖但能力降级无说明** | **P2** | 用户以为 Git 坏了 | S | 文档「Git 可选」 |
| 10 | **Slice E 安全/性能/诊断未做** | **P2 私测 / P0 公开工业** | 崩溃难收集、性能未知 | L | 私测人工收集 |
| 11 | **`keeper.db` 等内部命名** | **P3** | 诊断混淆 | S | 文档对照表 |
| 12 | **多 runtime / S1 凭据** | **非本阶段阻塞**（ADR 已允许 Grok-only 临时交付） | API-key provider 不可用 | L | 明确声明范围 |

---

## 5. 最小可试用切片（给少数外部用户，非公开发布）

**目标：** 5–20 名技术向试用者能「装上 → 打开 → 连上已有 Grok 登录 → 在一个本地项目里完成一轮对话与审批写」。  
**非目标：** 商店、自动更新、多 provider、精美 onboarding 动效、零 SmartScreen。

### 5.1 切片内容（增量可验收）

1. **打包**  
   - `bundle.active: true`  
   - targets: **NSIS only**  
   - `webviewInstallMode: downloadBootstrapper`（或 embedBootstrapper）  
   - installMode: **current user**  
   - 版本保持 `0.1.x`，在 release 说明写清 build id  

2. **启动韧性（切片成败关键）**  
   - Grok 不可用时：**仍进入 UI**，展示固定错误态 + 打开设置；**禁止** release 静默 setup 失败  
   - 或提供 `--safe-mode` / 环境开关仅用于恢复（文档化）  

3. **设置脚枪收敛**  
   - 保存 exe：必须 `is_file`；profile：必须 `is_dir`  
   - 可选：保存后「测试启动」按钮（spawn initialize，不读 auth 内容）  

4. **试用文档一页**（可放 `docs/user/` 或 release 附件，不要求重写全书）  
   - 安装 Grok CLI → 登录 → 安装 Pillow → SmartScreen 步骤 → 设置路径（通常默认 PATH）→ 打开项目 → 已知限制（只读 Git 可选、无 API key 托管、Fake 勿选）  

5. **隐藏 Fake（release）**  
   - 与 ADR-0003 Q2 对齐  

6. **分发方式**  
   - 私链 / 邮件 / 内网盘发送 NSIS；**不签也可以**，但文档必须写 SmartScreen  
   - **不做** updater  

7. **验收标准（试用门，不是全量工业门）**  
   - 干净 Win11 VM：无 dev toolchain  
   - 预装 Grok 并登录  
   - 安装包安装 → 启动 → 打开文件夹项目 → 一轮真实对话  
   - 故意卸载 Grok 或清 PATH：应看到**可理解错误**而非空白闪退  
   - 故意保存错误路径：应**拒保存**或可恢复，而不是下次永久起不来  

### 5.2 明确不做（本切片）

- Authenticode / EV / Store  
- Auto-update 基础设施  
- S1 凭据  
- 完整 public alpha 文档站  
- macOS/Linux  
- 把 Grok CLI 打进安装包（许可与体积；**未查证** xAI 再分发条款 → 默认 **不捆绑**）

### 5.3 粗工作量

**约 1–2 周单人**（含 VM 验收与文档），前提：不与全量 workspace 并发抢 `target/`（本仓已知 LNK 锁问题）。

---

## 6. 查证 vs 推测（显式区分）

### 6.1 查证（本仓库或官方文档可读）

| 事实 | 来源 |
| --- | --- |
| `bundle.active: false` | `tauri.conf.json` |
| 版本 / productName / identifier | 同上 + ADR-0002 |
| 默认 Grok、无静默 Fake | `runtime_composition.rs`、ADR-0003、S0b 报告 |
| 路径启动期 fail-closed 错误码 | `runtime_composition.rs` |
| 设置保存不验存在性 | `storage-sqlite` `settings.rs` validate |
| 设置 UI 登录声明 + 重启生效文案 | `SettingsPanel.tsx` |
| setup 内 `open_conversation_service` | `lib.rs` |
| release 无控制台 | `main.rs` `windows_subsystem` |
| Git 缺失 → `git_unavailable`，应用仍可起 | `git_commands.rs` + `gitGateway.ts` |
| 无 updater 插件 | Cargo/配置检索 |
| S0 临时对外交付允许 | ADR-0003 Q9；设计 §4.3 表 |
| Slice E 未开始 | stage1 overall status |
| Tauri MSI/NSIS、WebView2 模式与体积增量 | [Tauri Windows Installer](https://v2.tauri.app/distribute/windows-installer/) |
| EV 不再保证即时 SmartScreen | [MS Learn](https://learn.microsoft.com/en-us/windows/apps/package-and-deploy/smartscreen-reputation) |
| 图标为 Pillow 品牌图形 | 读取 `icons/icon.png` 视觉 |

### 6.2 推测 / 未实测

| 项 | 说明 |
| --- | --- |
| `pillow.exe` / 安装包精确 MB | **未跑构建**；仅给堆栈区间 |
| 无 WebView2 时系统对话框原文 | 未在干净机复现 |
| setup 失败时 Explorer 双击的精确 UX | 推断为无窗口；未抓 HRESULT UI |
| 未登录时 timeline 最终用户可见句 | 有 probe 分类，但无专用 UI 映射；**表现粒度未 E2E 取证** |
| 签名证书美元价 | 市场区间，**非报价** |
| 捆绑分发 Grok CLI 的许可合法性 | **未查** xAI 条款 |
| 试用转化率 / SmartScreen 劝退比例 | 无数据 |
| Azure Trusted Signing 实时价 | 未查 |

---

## 7. 不确定 / 需总控拍板

1. **启动失败策略：** 延迟 Grok 组合（推荐试用） vs 硬依赖 PATH（文档纪律）？  
2. **Fake 在 release 是否必须隐藏？**（ADR 意向 vs 当前 UI）  
3. **试用是否接受未签名？** 受众若含企业机，可能直接 P0。  
4. **Profile 默认是否永远等于用户主目录？** 多 Grok 账号用户是否要一等 UI 说明？  
5. **安装包是否需要中文 NSIS 字符串？** 产品 UI 已中英混合。  
6. **`keeper.db` 是否在对外前改名？** 涉及迁移，不宜塞进最小切片 unless 必须。  
7. **Grok CLI 安装来源 URL / 版本钉扎** 以谁的文档为准？（本仓未维护用户向链接）

---

## 8. 与产品顺序的对齐

- Stage 1 仍是工业本地 AI IDE；**打包属于 Slice E**，不阻塞 C4 能力本身。  
- ADR-0003 已解除「必须 S1 凭据才能对外」的误判；**真正阻塞是分发 + 依赖失败 UX**，不是 keychain。  
- Keeper 仅协调/记录；**不得**把试用安装包构建塞进 Keeper 职责（符合 Agents.md 边界）。

---

## 9. 本调研边界与合规

- 未修改任何源码/配置/测试。  
- 未执行 cargo / pnpm build / gate / E2E。  
- 未读 auth/token 文件。  
- 报告未粘贴机器用户配置文件绝对路径。  
- Keeper bridge boot 在本环境返回 400；调研不依赖 bridge 状态。

---

**DELIVERY-RESEARCH-COMPLETE**
