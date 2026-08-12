# Pillow — 本机 CLI 探测 crate 执行报告

日期：2026-08-12  
Worker：grok3  
状态：`CLI-DETECT-COMPLETE`

## 0. 边界

| 项 | 结论 |
| --- | --- |
| 允许改动 | `crates/cli-detect/**` + 根 `Cargo.toml` workspace members 一行 |
| 其它 crate / UI / 槽位 | **未改** |
| git 写操作 | **无** |
| 凭据 / token / cookie | **不读认证文件、不打印、不返回**；只保留脱敏后的短账号标识 |
| 进程监督 | 复用 `keeper-process-executor`（`SupervisedProcessRunner`），未自造监督 |

## 1. 退出码表（真跑）

命令（无管道截断）：

```text
cargo test -p pillow-cli-detect
```

| 步骤 | 退出码 | 结果 |
| --- | ---: | --- |
| `cargo test -p pillow-cli-detect` | **0** | lib 单测 17 passed；integration `detect_contract` 8 passed；doc-tests 0 |

再次确认：

```text
EXIT=0
```

## 2. 交付摘要

新建 crate **`pillow-cli-detect`**（路径 `crates/cli-detect`）：

| 能力 | API / 模块 |
| --- | --- |
| T1 发现 | `discover` / `discover_all` / `discover_in` — PATH + 常规安装目录；Windows 上优先 `.exe`，npm 布局下可解析 codex 原生 vendor bin、gemini JS 入口 |
| T2 账号状态 | `detect` / `detect_all` / `detect_with_timeout` — 三态 `Authenticated { account_label }` / `NotAuthenticated` / `Unknown { reason }` |
| T3 超时与清理 | 每次调用 `SupervisedProcessRunner::run`，默认硬超时 10s；超时要求 `cleanup_proven`；映射为 `Unknown { timed_out }` |
| T4 脱敏 | `sanitize_account_label` — 只保留邮箱 / 组织·plan·provider 短标识；token 形状一律丢弃 |

**不做：** OAuth、登录、改 CLI 配置、接槽位模型、UI。

## 3. 每个 CLI：发现方式 + 状态命令 + 判定信号

| CLI | 发现 | 版本命令（不查登录） | 状态命令 | 已登录信号 | 未登录信号 | 其余 |
| --- | --- | --- | --- | --- | --- | --- |
| **grok** | PATH / `~/.grok/bin` 等下的 `grok.exe` | `grok --version` | `grok models` | 输出含 `You are logged in with <label>`；label 经脱敏（如 `grok.com`） | 输出含精确串 `Not logged in. Run \`grok login\`.` | 无 banner → `Unknown`；`--version` 不参与登录判定 |
| **claude** | PATH 等下的 `claude` / `claude.exe` | `claude --version` | `claude auth status --text` | **官方合同：exit 0**；尽量抽 email / `Logged in as …`，否则 label=`authenticated` | **官方合同：exit 1** | 其它 exit / 无 exit → `Unknown`（绝不把异常 exit 当未登录） |
| **codex** | PATH；并探测 npm `@openai/codex` 的 **native** `codex.exe`（避免依赖 shell 的 `.cmd`/`.ps1`） | `codex --version` | `codex login status` | 输出含 `Logged in using <method>`（如 `ChatGPT`） | 输出含 `Not logged in` / `No credentials found` 等**明确**标记 | 无标记即使 exit 0 也是 `Unknown`（不猜） |
| **opencode** | PATH 等下的 `opencode` | `opencode --version` | `opencode auth list` | 成功列出可脱敏的 provider 名 → 本地凭据记录存在 | 空列表 / 明确 empty 文案 | 非 0 exit → `Unknown`；**仅证明本地 credential store，不证明远端会话仍有效** |
| **gemini** | PATH；并探测 npm `@google/gemini-cli` 的 `bundle/gemini.js`（经 `node` 启动） | `gemini --version` | **无**可靠 headless 状态命令 | — | — | 找到时 auth 固定 `Unknown { no_headless_status_command }` |

### 为何这样选命令

- **版本与登录分离：** 各 CLI 的 `--version` 不作为登录依据；登录只用上表状态命令。
- **grok：** 无 `whoami` / `auth status`；`models` 会打印明确的登录 / 未登录 banner（与本机二进制字符串表及 live 输出一致）。`login`/`logout` 会改状态，禁止用于探测。
- **claude：** 官方文档：`auth status` exit 0=已登录、1=未登录；`--text` 便于抽邮箱类标识。
- **codex：** 官方子命令 `login status`；实测输出 `Logged in using ChatGPT`。
- **opencode：** 官方 `auth list`/`ls` 列本地凭据 store 中的 provider。
- **gemini：** 仅有交互 `/auth`；无文档化 headless status → 一律 `Unknown`，避免把启动失败或 TUI 挂起误报成未登录。

### 进程与 shim

- 复用 `keeper-process-executor`：`env_clear` + 仅继承**非敏感**环境名；硬超时；Job Object 树清理；`cleanup_proven` 为 false → `Unknown`。
- **拒绝**直接 spawn `.cmd` / `.bat` / `.ps1`（需要 shell，会违反「不造 shell 监督」边界）→ `Spawn` → `Unknown`。
- `.js` 入口经 PATH 上的 `node`/`node.exe` 启动。

## 4. 「失败 → Unknown 而非 NotAuthenticated」证据

| 测试 | 断言 |
| --- | --- |
| `inevitable_probe_failure_is_unknown_not_not_authenticated` | 对不存在的可执行路径调用 `probe_found` → `AuthState::Unknown`，**不是** `NotAuthenticated` |
| `all_probe_error_variants_map_to_unknown` | 全部 `ProbeError` 变体映射为 `Unknown` |
| `probe::tests::probe_failure_is_never_not_authenticated` | 同上（lib 内） |
| `parse::tests::grok_garbled_output_is_unknown_not_logged_out` | 无法识别的 `grok models` 输出 → `Unknown` |
| `hard_timeout_cleans_child_and_maps_to_unknown` | `TimedOut` → `Unknown`；Windows 上对 `ping -n 30` 硬超时 150ms 且 `cleanup_proven == true` |

硬约束：**探测失败 ≠ 未登录**。沙箱拒执行、超时、解析失败、无 headless 命令、spawn 失败均不得报「明确未登录」。

## 5. 脱敏测试证据

| 测试 | 断言 |
| --- | --- |
| `sanitize::tests::drops_token_shaped_material` | 假串 `sk-fakeTESTKEY_…`、Bearer 假串、JWT 形、长 opaque 串 → `None` |
| `sanitizer_strips_token_shaped_fake_output` | 含假 token 的 grok banner → 不得在返回值中出现该 token；或 `authenticated_label_rejected` |
| `sanitize::tests::keeps_email_and_plan_labels` | `dev@example.com` / `ChatGPT` / `grok.com` 保留 |
| `live_detect_all_is_safe_and_shaped` | live `detect_all` 的 `account_label` / `version` / `reason` 不得含 `sk-` / bearer |

脱敏规则摘要：最长 128 字符；字符集受限；拒绝 `sk-`、Bearer、JWT `eyJ…`、≥32 的无空格 opaque 串。

## 6. 写入边界审计

| 操作 | 是否发生 |
| --- | --- |
| 写 CLI 配置 / auth 文件 | **否** |
| 调用 `login` / `logout` / OAuth | **否** |
| 写 SQLite / 槽位 | **否** |
| 写 UI | **否** |
| 新建源码 | 仅 `crates/cli-detect/**` |
| 改 workspace | 根 `Cargo.toml` members **仅加** `"crates/cli-detect"` |
| 读认证文件内容 | **否**（只跑官方只读 status 命令的 stdout/stderr，并脱敏） |
| 返回 token / key / cookie | **否**（测试用明显假串验证过滤） |

## 7. 残留风险

| CLI | 风险 |
| --- | --- |
| **gemini** | **无可靠 headless 判定信号**；只能发现 + 版本；账号状态恒为 `Unknown`。 |
| **opencode** | `auth list` 只反映**本地**凭据记录，不能证明远端仍有效或给出邮箱。 |
| **codex** | `login status` 常给**认证方法**（ChatGPT / API key），不一定给邮箱；未识别输出保守 `Unknown`。 |
| **claude** | 本机未安装时无法 live 校准文本格式；依赖官方 exit 0/1 合同 + 尽力抽 email。 |
| **npm shim** | 仅有 `.cmd`/`.ps1`、无 native/js 解析路径时 → spawn 拒绝 → `Unknown`（不是未登录）。 |
| **环境隔离** | 不同 `HOME`/`USERPROFILE`/配置根会导致「一处登录、另一处探测 Unknown」；这是配置根差异，不得推断成未登录。 |
| **敏感 env 剥离** | `OPENAI_*` / 含 `AUTH`/`TOKEN`/`KEY` 等名的环境变量不传入子进程；依赖纯 env key 的「已登录」可能显示为未登录或 Unknown——仍优于泄漏。 |

## 8. 模块布局

```text
crates/cli-detect/
  Cargo.toml                 # package name: pillow-cli-detect
  src/
    lib.rs
    types.rs                 # AuthState, CliDetection, ProbeError
    profiles.rs              # AgentCliKind + 命令表
    discover.rs              # PATH / 常规位置（纯文件系统）
    probe.rs                 # SupervisedProcessRunner 探测
    parse.rs                 # 纯解析（确定性信号）
    sanitize.rs              # 账号标识脱敏
  tests/
    detect_contract.rs       # 失败→Unknown、脱敏、超时清理、live 形状
```

## 9. 完成信号

**`CLI-DETECT-COMPLETE`**
