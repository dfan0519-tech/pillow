# Pillow 槽位基础实施报告

状态：实现完成；Worker 总门为 `ENV-BLOCKED`，原因与绿色聚焦证据分列如下。

## 范围与硬边界

- 本单实现宿主侧项目槽位模型、Windows 用户级密钥存储、按槽位准备/启动 Grok Build、外部 CLI 登记与账号状态查询，以及命令层 DTO。
- 未修改 `apps/desktop/src/**`，未运行 desktop E2E，未修改 `scripts/**`，未执行 git 写操作。
- `governed_grok` 固定为 `governance: "full"`；`external_cli` 固定为 `governance: "none"`，且本单不能作为 agent 启动。
- 激活项目槽位后，conversation、write、exec 命令不能把该槽位跨项目复用；不匹配时以 `runtime_slot_project_mismatch` 失败。
- 所有 Cargo 命令均使用任务指定的隔离 target 目录；下表不写出本机绝对路径。

## 退出码表

所有命令均直接执行，未通过管道截断输出。

| 命令 | 退出码 | 结果 |
| --- | ---: | --- |
| `cargo fmt --all -- --check` | 0 | PASS |
| `cargo test -p keeper-storage-sqlite --locked --offline` | 0 | PASS；包含槽位仓库 5/5 |
| `cargo test -p keeper-process-executor --locked --offline` | 0 | PASS；含专用密钥环境通道 |
| `cargo test -p pillow-runtime-slots --locked --offline` | 0 | PASS；6/6 |
| `cargo test -p keeper-adapter-grok-acp --locked --offline` | 0 | PASS；单元 73/73，集成测试通过；显式 live E2E 保持 ignored |
| `cargo test -p pillow-desktop --lib runtime_slot_commands_tests --locked --offline` | 0 | PASS；7/7，含命令级正向激活 |
| `cargo test -p pillow-desktop --locked --offline` | 1 | `ENV-BLOCKED`；161/162 通过，既有 reparse-point 测试创建目录符号链接时被 Windows 错误 1314 拒绝（缺少特权）；未绕过、未改测试 |
| `cargo clippy -p keeper-storage-sqlite --locked --offline -- -D warnings` | 0 | PASS |
| `cargo clippy -p keeper-process-executor --locked --offline -- -D warnings` | 0 | PASS |
| `cargo clippy -p pillow-runtime-slots --locked --offline -- -D warnings` | 0 | PASS |
| `cargo clippy -p keeper-adapter-grok-acp --locked --offline -- -D warnings` | 0 | PASS |
| `cargo clippy -p pillow-desktop --locked --offline -- -D warnings` | 0 | PASS |
| `git diff --exit-code -- crates/adapter-grok-acp/src/rpc/methods.rs` | 0 | PASS；治理 profile/capability/MCP 边界零差异 |
| `pnpm check` | 1 | `ENV-BLOCKED`；PowerShell 执行策略在总门启动前拒绝加载 pnpm 脚本入口；按任务条款未换入口、未绕过、未改 `scripts/` |

TDD 红绿过程：槽位仓库、专用进程密钥通道、Grok model/secret 配置和宿主命令测试均先因 API 不存在而退出 1，最小实现后转绿。新 crate 最初选用的 TOML 版本不在离线缓存中，离线解析退出 101；随后改用锁文件中已存在且本机已缓存的版本，没有联网取包。另一次 Windows 命令夹具自身退出 1，改为等价且退出 0 的固定输出夹具后，产品断言通过。

## 槽位模型与持久化

- `runtime_slots` 表中的每条记录都带 `project_id` 外键，并在项目删除时级联删除。
- SQLite 只保存显示名、显式 `kind`、显式 `governance`、认证模式、非秘密端点/模型配置、外部 CLI 配置和不透明 `secret_ref`；没有明文密钥列。
- SQL CHECK 与 Rust 类型共同约束两种合法组合：
  - `governed_grok` + `full` + `login | api_key`
  - `external_cli` + `none`，无 Grok 认证字段
- repository 的 `get`/`list` 同时要求项目 ID 与槽位 ID，测试覆盖重启持久化、跨项目不可见、非法类型组合拒绝和项目删除级联。

## 密钥落点与保护边界

### 加密方式与落点

- API key 只以 Windows DPAPI `CryptProtectData` 生成的密文落盘；未设置 machine scope，因此绑定当前 Windows 用户，并使用固定 Pillow entropy 与禁止 UI 提示标志。
- 密文文件位于 Pillow app-data 下的 `secrets/<opaque-ref>.dpapi`。文件名只含随机 UUID；SQLite 仅保存同一个不透明引用。
- DPAPI 解密得到的系统分配明文缓冲区在 `LocalFree` 前显式清零。宿主侧 `SecretBytes`、adapter 内部 secret 与专用进程环境值均不可 `Debug`/序列化，并在 drop 时做 volatile 清零。
- API-key 槽位的非秘密配置位于 Pillow app-data 下的 `runtime-slots/<project-id>/<slot-id>/.grok/config.toml`。配置只写 `model`、`base_url`、`env_key` 与显示名；不写 `api_key`。
- `env_key` 固定指向 `PILLOW_GROK_SLOT_API_KEY`。启动时宿主解密，再通过专用、不可序列化的进程环境通道只注入受控 Grok 子进程。
- Pillow 不写用户全局 Grok `config.toml`，也不读取任何登录凭据文件。API-key 私有 home 若出现 `.grok/auth.json` 或根级 `auth.json` 会拒绝启动。

### 谁能读到

- 具有 app-data 文件读取权限的主体可以读取 DPAPI 密文；通常只有绑定的 Windows 用户上下文可解密。管理员、凭据恢复和已控制该用户会话的攻击者不在 DPAPI 的防护边界之外。
- 明文只在填入时的 renderer/IPC 临时对象、Pillow 宿主内存、进程创建环境和被启动的 Grok 子进程内出现。它不进入 DTO、SQLite、TOML、日志、错误文本、runtime event 或 process receipt。

### 依赖说明

- 新 crate 直接使用 `windows-sys 0.59` 调用 DPAPI；仓库没有现成的用户级加密抽象，而任务明确要求 Windows 用户级加密。该包此前已在锁文件中。
- 新 crate 直接使用 `toml 0.9.12` 结构化序列化用户可控的模型名、端点和显示名，避免手拼 TOML 的转义/注入问题。该包此前也已在锁文件和离线缓存中。
- `Cargo.lock` 没有新增第三方 package 条目，只增加新 workspace package 对既有包的引用。

## 命令层 DTO 与下一单渲染层键清单

本单只实现 Rust/Tauri 命令，不修改 renderer gateway。下一单必须在 renderer 的闭合 allowlist 中同步加入以下精确 camelCase 键，并同时测试接受方向与拒绝未知键方向。

### 命令输入

| 命令 | 输入键 |
| --- | --- |
| `list_runtime_slots` | `projectId` |
| `create_grok_login_slot` | `projectId`, `displayName` |
| `create_grok_api_key_slot` | `projectId`, `displayName`, `baseUrl`, `modelId`, `apiKey` |
| `create_external_cli_slot` | `projectId`, `displayName`, `executablePath`, `launchArgs`, `statusArgs` |
| `get_external_cli_status` | `projectId`, `slotId` |
| `start_runtime_slot` | `projectId`, `slotId` |

`apiKey` 只允许作为创建命令的一次性输入，不能进入任何 response DTO、客户端持久化、日志或遥测。

### `RuntimeSlotDto`

`slotId`, `projectId`, `displayName`, `kind`, `governance`, `authMode`, `baseUrl`, `modelId`, `secretRef`, `secretConfigured`, `executablePath`, `launchArgs`, `statusArgs`, `createdAt`, `updatedAt`

其中只有 `secretRef` 和 `secretConfigured` 表达密钥状态；不存在 `apiKey` 或其他密钥值字段。

### `ExternalCliStatusDto`

`slotId`, `projectId`, `governance`, `exitCode`, `stdout`, `stderr`, `timedOut`, `truncated`

`governance` 对此 DTO 固定为 `none`。stdout/stderr 是该外部 CLI 状态命令的有界原样输出。

### `RuntimeSlotStartDto`

`slotId`, `projectId`, `kind`, `governance`, `status`

成功值只可能是 `kind: "governed_grok"`、`governance: "full"`、`status: "active"`。对 `external_cli` 的启动请求固定返回 `external_cli_launch_unsupported`。

## `extra_env` 规则

- 原有 “Extra allow-listed env pairs for tests (never secrets)” 注释没有修改，其不变量继续成立。
- 密钥没有塞进 `extra_env`。新增的是独立的 `SecretEnvironment` 通道：只接受被敏感名分类器认可的名称，不实现 `Debug`、`Clone` 或序列化，普通 `ProcessSpec.env` 仍拒绝敏感名称；本单 Grok adapter 只使用一个固定名称。
- 新规则是：非秘密测试环境仍走 `extra_env`；产品密钥只能从宿主私有字段进入专用进程密钥通道。两条路径不可互换。

## 按槽位启动与写入边界审计

- `governed_grok/login`：沿用默认登录 profile，不提供不同的 home 路径；adapter 在清洁环境中保留当前 profile 变量的原值。
- `governed_grok/api_key`：写私有 `config.toml`，固定加入 `-m <model-id>`，再注入固定 env key。构造出的仍是同一个 Grok ACP adapter 与完整治理 host。
- `external_cli`：只登记并运行账号状态命令；使用结构化 argv、无 shell、项目根目录 cwd、敏感父环境过滤、10 秒超时、共享有界输出与已证明的进程树清理。它在模型、DTO 和启动拒绝路径上均明确“不受治理”。
- 槽位激活先构造替代 runtime，再关闭旧 runtime 并替换；构造失败保留旧 runtime，关闭或恢复失败则保持无 runtime 的 fail-closed 状态。
- `plaintext_agent_profile()`、`disallowedTools`、`mcpServers: []`、`clientCapabilities` 所在文件零差异；换模型没有换治理边界。
- 工作区检查确认 `apps/desktop/src/**` 与 `scripts/**` 无本单修改。全局 Grok 配置不在任何写入路径中。

## 正反测试证据

- DTO 正向：配置完成的 API-key 槽位序列化出 `secretRef`、`secretConfigured: true` 与 `governance: "full"`，精确键集测试通过。
- DTO 反向：同一序列化结果不含输入密钥值，也没有 `apiKey` 字段。
- 配置正向：测试读取并解析实际生成的 TOML，确认模型表、`model`、`base_url`、固定 `env_key` 均可用；启动配置确认固定 `-m <model-id> agent --no-leader stdio`。
- 启动正向：`start_runtime_slot` 聚焦测试把已配置的 API-key 槽位激活为项目绑定的 `governed_grok/full/active` runtime，并正常关闭该宿主；不依赖真实 CLI 或网络。
- 配置反向：TOML、SQLite 文件与 adapter Debug 均不含测试密钥值；私有 home 出现 `auth.json` 时拒绝。
- 密钥存储正向：DPAPI 密文往返成功；落盘字节不含输入明文。
- 治理正反：Grok 槽位只能与 `full` 组合，外部 CLI 只能与 `none` 组合；Rust API 与 SQL 约束都拒绝错配。
- 项目边界：repository 的跨项目 get/list 不可见；已激活槽位不能跨项目服务 conversation/write/exec。
- 外部 CLI：状态输出按 stdout/stderr 原样返回并有界；尝试作为 agent 启动固定拒绝。

## 残留风险与未做事项

- Worker 总门尚未真正启动，原因是 PowerShell 执行策略；完整 desktop Rust 套件还受到目录符号链接特权限制。两者均记录为 `ENV-BLOCKED`，不是绿色门。控制器应在允许脚本入口且具备对应 Windows 权限的环境中重跑 `pnpm check`。
- 未使用真实 API key 或真实自定义端点做 live canary；任务禁止 desktop E2E。当前正向证据覆盖 DPAPI、真实 TOML 解析、固定 model argv 和实际子进程 secret-env 注入，但不等于供应商网络验收。
- 明文在 renderer 输入、Tauri 反序列化、宿主/adapter 内存、Windows 进程创建缓冲区与 Grok 子进程环境中短暂存在。自有缓冲区会清零，但无法保证标准库/操作系统临时副本、交换文件或崩溃转储被清零；同用户高权限进程也可能检查内存或环境。
- DPAPI 密文依赖对应 Windows 用户 profile，跨用户或缺少该 profile 的恢复不可用；app-data 目录沿用现有 ACL，本单没有额外收紧 ACL。
- 密钥先写密文、后写 SQLite；异常崩溃可能留下不可达密文。数据库写失败时会尝试删除，但本单没有孤儿扫描、轮换、更新或删除 UI/API。
- 私有 TOML 通过临时文件写入并 sync；Windows 上替换既有配置包含一次删除再 rename 的短暂缺口。文件不含秘密且可再生，但并非完全无间隙替换。
- 外部 CLI 的“状态命令”语义无法由 Pillow 证明为只读；因此它明确标为 `governance: "none"`。结构化 argv、无 shell、环境过滤和进程监督只限制执行机制，不把它变成受治理 ACP。
- 活跃槽位选择目前是进程内状态，重启后需由下一单 UI 再次选择/启动；槽位定义与密钥本身会持久化。
- 工作树中出现了不属于本单的并行改动；本单未回退、暂存、提交或声称接管这些文件。
