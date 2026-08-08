# Pillow — 交付硬化报告（WebView2 + 打包 + 首次运行引导）

Date: 2026-08-09  
Worker: **grok2**  
Controller: Claude  
Work directory: `<repo>`  
Cargo isolation: `CARGO_TARGET_DIR=<repo>/target-grok2`

---

## Signal

**DELIVERY-HARDENING-COMPLETE**

> 本 worker 白名单交付项已落地并通过本包范围门禁。全量 `run-worker-gate.mjs` 在 **本机并行 worker 的 `crates/**` 未提交改动** 上被 clippy 阻断（见 §8）；非本单引入，且白名单禁止修改 `crates/**`。

---

## 0. 一句话

配置了 **NSIS + WebView2 `downloadBootstrapper`**，清理/确认打包元数据，并把「未登录」从通用失败提升为 **可操作的 `grok login` 引导**——全程不读 auth 文件。

---

## 1. WebView2 选择与取舍

| 项 | 决定 |
| --- | --- |
| **模式** | `downloadBootstrapper`（`silent: true`） |
| **配置位置** | `apps/desktop/src-tauri/tauri.conf.json` → `bundle.windows.webviewInstallMode` |

### 取舍理由

| 模式 | 联网 | 体积增量 | 本轮选择 |
| --- | --- | --- | --- |
| **`downloadBootstrapper`** | 安装时若缺 WebView2 需联网拉 Evergreen bootstrapper | **+0 MB** | **选用** |
| `embedBootstrapper` | bootstrapper 已嵌入；实际 runtime 仍可能需联网 | ~+1.8 MB | 备选（私测体积不敏感时可升） |
| `offlineInstaller` | 否 | ~+127 MB | 拒绝：分发成本高 |
| `fixedVersion` | 否 | ~+180 MB | 拒绝：钉版本维护负担大 |
| `skip` | 否 | 0 | **禁止**：装完仍可能起不来 |

**与「本地优先」是否冲突：**

- **弱冲突、可接受。** 本地优先针对的是 **产品运行时不外发项目/对话数据、无遥测**。  
- `downloadBootstrapper` 仅在 **安装器** 检测到缺 WebView2 时访问 Microsoft Evergreen 通道，**不进入 Pillow 运行时网络面**。  
- Win11 / 较新 Win10 多数已预装 WebView2；bootstrapper 是边缘机器的兜底，不是日常出站。  
- 完全离线锁定环境应改用 `offlineInstaller`（本轮不做；列入残余风险）。

---

## 2. 打包元数据检查

| 项 | 当前值 | 占位符？ | 动作 |
| --- | --- | --- | --- |
| `productName` | `Pillow` | 无 | 保持 |
| `identifier` | `com.pillow.desktop` | 无（符合 ADR-0002） | 保持 |
| `version` | `0.1.0` | 合理 pre-alpha | **未跳版本** |
| `@pillow/desktop` version | `0.1.0` | 一致 | 保持 |
| 图标 | `icons/` 品牌文件夹/盾牌/代码标（非 Tauri 默认蓝方） | 无 | 在 bundle 中显式列出 ico/icns/png |
| 可执行名 | `pillow`（Cargo `[[bin]]`） | 无 | 未改 |
| 内部命名 `keeper.db` / crate `keeper-*` | 仍在 | 用户几乎不可见 | **本单不改**（P3；见调研） |

### 版本号策略（建议，未执行）

- 保持 `0.1.0` 直至第一次对外试用包出炉。  
- 试用包：`0.1.x` patch 递增 + release notes 写 build id。  
- 公开 alpha：再考虑 `0.2.0`。  
- **本轮明确不擅自 bump。**

### MSI vs NSIS

| | 决定 |
| --- | --- |
| **选用** | **NSIS only**（`bundle.targets: ["nsis"]`） |
| **installMode** | `currentUser`（`%LOCALAPPDATA%`，默认少 UAC） |
| **MSI** | 不做：企业 GPO/静默场景非当前试用受众；WiX 主机约束更重 |

**理由：** 私测/熟人分发优先「当前用户、一键 setup.exe」；与交付调研 §2.1 / 最小试用切片 §5.1 一致。`bundle.active` 从 `false` → `true`。

**未做：** 代码签名、自动更新、中文 NSIS 自定义字符串（系统默认即可）。

---

## 3. 首次运行引导（认证失败类）

### 3.1 产品口径（固定文案）

> Grok 登录由 Grok CLI 管理，Pillow 不存储该登录。请在终端运行 `grok login`，完成后回到 Pillow 重试发送。

设置页补充：

> 对话需要 Grok CLI 已登录。请在终端运行 `grok login`；Pillow 从不读取或保存该登录。改路径后需重启 Pillow。

### 3.2 实现位置

| 层 | 文件 | 行为 |
| --- | --- | --- |
| 纯函数 | `apps/desktop/src/features/settings/loginGuidance.ts` | 短语/码分类 + 固定文案 + `userFacingSendError` |
| Shell | `apps/desktop/src/App.tsx` | send 失败映射；`turn_failed` 事件分类；顶栏 `runtime-login-guidance-banner` |
| 设置 | `SettingsPanel.tsx` | 静态 `settings-login-guidance` |
| 样式 | `settings.css` | 登录引导条（几何对齐 degraded banner，不改侧栏布局） |
| 宿主文案 | `runtime_composition.rs` | `guidance_for_authentication_failed` + 短语匹配；**不参与文件 I/O** |

### 3.3 如何确保不碰 auth 文件

1. **无路径打开：** 引导路径从不 `fs::read` / 打开 profile 下任何文件。  
2. **仅消费已有失败信号：** 适配器/RPC 已 redaction 的 message 与稳定 code（如 `authentication_failed`、`unauthenticated` 等短语）。  
3. **命中后替换全文：** `userFacingSendError` 对 auth 类 **丢弃原始 message**，只展示固定引导（避免透传邮箱/机器细节）。  
4. **测试断言：** 引导文案不得含 `auth.json`、`@`、`sk-`、`USERPROFILE`、用户路径等。  
5. **与 degraded 隔离：** 配置错误仍走 `runtime-degraded-banner`；登录引导仅在 **conversation 可用** 时出现，二者不同时抢主路径。

### 3.4 与降级启动的关系

| 状态 | UI | 对话 |
| --- | --- | --- |
| Degraded（exe/profile/release Fake） | `runtime-degraded-banner` → 开设置改路径 | 关（无 Fake 回落） |
| Ready + 未登录 | 发 turn 失败 → 登录引导条 + composer 固定文案 | 开 |
| Ready + 普通失败 | 原 message | 开 |
| `PILLOW_RUNTIME=fake` | Ready，无登录引导 | 开（E2E 生命线） |

---

## 4. §4.4 各项前提的失败表现

| 前提 | 现状 | 本轮 | 评价 |
| --- | --- | --- | --- |
| **A. WebView2** | 打包未配置 → 裸 exe 近静默 | **安装器** `downloadBootstrapper` | **改善（安装路径）**。裸 zip exe 仍可能闪退（平台限制，无品牌页）— 改不了 |
| **B. Grok 可执行** | 已降级开壳 + 设置引导（上轮） | 保持；登录引导不覆盖此态 | **清晰可操作** |
| **C. Profile** | 同上 | 保持 | **清晰可操作** |
| **D. Grok 登录** | 通用失败 | **固定 `grok login` 引导** | **改善** |
| **E. Git** | `git_unavailable`，主路径不堵 | 未改（features 白名单外） | **已清晰** |
| **F. SQLite / app data** | setup 失败，release 近静默 | 未改（非 degradable） | **仍近静默** — 列残余；safe-mode 不在本单 |
| **G. Fake** | env 生命线保留；release 存 Fake 降级 | 断言仍有效 | **E2E 生命线未断** |

---

## 5. 四项测试如何覆盖

| # | 要求 | 覆盖 |
| --- | --- | --- |
| 1 | 降级态与新引导不冲突 | `settings-panel` + `login-guidance`：degraded 时 **无** login banner；login 时 **无** degraded |
| 2 | `PILLOW_RUNTIME=fake` 生命线 | 既有 `env_fake_still_opens_ready_not_degraded` 等 140 条 desktop lib 测试全绿 |
| 3 | 引导不含账号/auth 内容 | pure helpers + banner 文案断言；`guidanceLooksSafe` / Rust `authentication_failed_guidance_is_actionable_and_secret_free` |
| 4 | 打包配置不破坏构建 | `tauri-window-config` 断言 bundle 字段；`pnpm --filter @pillow/desktop build` exit 0；desktop 编译/测试通过 |

新增/扩展测试文件：

- `apps/desktop/tests/login-guidance.test.tsx`
- `apps/desktop/tests/settings-panel.test.tsx`（degraded 扩展）
- `apps/desktop/tests/tauri-window-config.test.ts`（packaging 段）
- `runtime_composition.rs` 内 3 个 auth guidance 单测

---

## 6. 命令 exit code

| 命令 | Exit | 说明 |
| --- | --- | --- |
| `CARGO_TARGET_DIR=<repo>/target-grok2` | 已设 | 并行隔离 |
| `node scripts/run-worker-gate.mjs` | **101** | clippy 步失败：**并行 worker** 的 `crates/knowledge-indexer`（`unnecessary_sort_by`）等，**非本白名单文件** |
| gate 内 typecheck | 0 | 通过 |
| gate 内 desktop vitest | 0 | **407 passed** |
| gate 内 cargo fmt --check | 0 | 通过 |
| `cargo test -p pillow-desktop --lib` | **0** | **140 passed**（含 auth/degrade） |
| `cargo fmt --all -- --check` | **0** | 通过 |
| `corepack pnpm --filter @pillow/desktop build` | **0** | Vite production build OK |
| Keeper bridge boot | 400 | 与调研一致；不阻塞 |

**本 worker 范围 clippy：** 因依赖图仍会 check 其它 crate 的并行脏树，全量/连带 clippy 不可作为本单回归信号。desktop **测试 profile 编译与 140 单测已绿**。

---

## 7. 精确改动清单（白名单内）

| 路径 | 变更 |
| --- | --- |
| `apps/desktop/src-tauri/tauri.conf.json` | `bundle.active`、NSIS、WebView2、icons |
| `apps/desktop/src-tauri/src/runtime_composition.rs` | auth guidance + 短语匹配 + 单测 |
| `apps/desktop/src/App.tsx` | 登录引导条 + send/`turn_failed` 映射 |
| `apps/desktop/src/features/settings/loginGuidance.ts` | **新建** |
| `apps/desktop/src/features/settings/SettingsPanel.tsx` | 静态登录说明 |
| `apps/desktop/src/styles/settings.css` | 引导条与帮助块样式 |
| `apps/desktop/tests/**` | login / settings / window-config |
| `docs/execution/2026-08-09-delivery-hardening-report.md` | 本报告 |

### 没碰的东西

- `crates/**`、`scripts/**`、settings 以外的 `features/**`  
- `brand-mark.css`、审批/治理、签名、updater、遥测  
- 任何 auth/token 文件  
- `data-testid` 删除或改名（仅 **新增** `runtime-login-guidance-*` / `settings-login-guidance`）  
- 布局几何（侧栏/轨宽未改）  
- git add/commit/stash/reset/clean  
- 桌面 E2E（总控独占）  
- `lib.rs`：降级接线已足够，本轮无需再改  

---

## 8. 残余风险

1. **全量 worker gate clippy：** 当前树内并行 worker 的 `knowledge-indexer` / `runtime-host` 脏文件导致 workspace clippy 失败；需对应 worker 收尾或总控隔离后重跑。  
2. **裸 `pillow.exe` 无 WebView2：** 仍无品牌错误页（平台级）；依赖 NSIS 安装路径。  
3. **离线环境：** `downloadBootstrapper` 装机时需网；真正离线应换 `offlineInstaller`（未做）。  
4. **未签名：** SmartScreen「未知发布者」仍在（本单禁止签名）。  
5. **SQLite / app data 创建失败：** 仍可能 release 近静默（非 degradable；需后续 safe-mode）。  
6. **设置保存不校验路径存在：** 仍可能脚枪后需清 DB（调研 P0/P1；非本单范围）。  
7. **异步 turn 失败：** 已订 `turn_failed`；若 gateway 吞掉事件则仅依赖 sync send 路径。  
8. **未实测 `tauri build` 产 NSIS：** 配置已落地；完整安装包构建留给打包流水线（体积/NSIS 工具链依赖本机）。

---

## 9. 完成声明

交付调研缺口 #2（打包/WebView2）、#5（未登录引导）在本单范围内推进；#1 降级开壳由上轮保持且测试证明与登录引导不冲突。

**DELIVERY-HARDENING-COMPLETE**
