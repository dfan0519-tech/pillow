# Pillow 槽位模型目录与切换（宿主侧）执行报告

日期：2026-08-11

实现状态：`COMPLETE`

根门禁状态：`ENV-BLOCKED`。`pnpm check` 在进入门禁步骤前被 PowerShell 执行策略拒绝；未改用其他 shim 绕过，也未修改 `scripts/`。

## 1. 交付摘要

- API-key 槽位在宿主内解密 key，并以 Bearer 认证请求 `{base_url}/models`；key 不进入任何返回 DTO、日志或错误文本。
- login 槽位由受监督进程执行 `grok models` 并解析标准输出。
- external CLI 槽位返回显式 `unsupported`，不伪造目录能力。
- 远端目录输入有三层界限：最多 256 个模型、单个 ID 最多 512 个 Unicode 字符、响应最多 262144 字节；空白包围、空 ID 和任意控制字符均拒绝。
- `/models` 返回 404、405 或 501 时返回可判定的 `unsupported` 结果，不作为命令错误；认证、限流、服务错误以及畸形成功响应保持显式错误。
- HTTP 实现禁用重定向，避免端点控制的跳转携带 Bearer secret；响应体从不写日志，错误消息固定且脱敏。
- 模型选择持久化进 SQLite。login 槽位通过迁移获得可选 `model_id`；旧 login/API-key 行在迁移、重开后保留。
- 切换活跃槽位时，宿主串行化槽位生命周期，重建 runtime；替换失败会停止旧 runtime，避免“元数据是新模型、进程仍是旧模型”的假成功。
- 既有 `update_grok_api_key_slot` 同样补上活跃 runtime 重启，封住从旧编辑命令绕过新切换语义的路径。
- 未新增第三方包：复用 lockfile 已有的 `serde_json` 与 `windows-sys`，只扩展工作区 crate 依赖/Windows feature。

## 2. 渲染层命令与闭合 DTO

本单未修改渲染层。渲染层接入时应在同一变更中扩展其闭合键 allowlist，并同时写接受方向测试。

### `get_runtime_slot_model_catalog`

请求键：

- `projectId: string`
- `slotId: string`

成功返回键（仅这些键）：

- `slotId: string`
- `projectId: string`
- `status: "available" | "unsupported"`
- `source: "openai_models" | "grok_cli" | "external_cli"`
- `models: string[]`
- `reasonCode: null | "endpoint_not_supported" | "external_cli_not_supported"`

### `set_runtime_slot_model`

请求键：

- `projectId: string`
- `slotId: string`
- `modelId: string`

成功返回键（仅这些键）：

- `slotId: string`
- `projectId: string`
- `modelId: string`
- `runtimeRestarted: boolean`
- `sessionRestore: "not_applicable" | "unavailable"`

既有 `RuntimeSlotDto.modelId` 键未新增，但 login 槽位选择模型后，该键现在会从 `null` 变为选中的模型 ID。

## 3. `/models` 不可用时的返回形状

API-key 端点明确不支持目录（404、405、501）：

```json
{
  "slotId": "<slot-uuid>",
  "projectId": "<project-uuid>",
  "status": "unsupported",
  "source": "openai_models",
  "models": [],
  "reasonCode": "endpoint_not_supported"
}
```

external CLI 槽位：

```json
{
  "slotId": "<slot-uuid>",
  "projectId": "<project-uuid>",
  "status": "unsupported",
  "source": "external_cli",
  "models": [],
  "reasonCode": "external_cli_not_supported"
}
```

渲染层在 `status === "unsupported"` 时保留/展示手敲输入。401、403、429、5xx、传输失败和畸形 2xx 响应不是“不支持”的可靠证据，分别返回固定的 `runtime_slot_model_catalog_unavailable` 或 `runtime_slot_model_catalog_invalid` 错误。

## 4. T2 重启与实际启动证据

显式验收测试不是只检查命令返回：

1. 启动使用 `model-old` 的槽位并发送一轮，使第一个真实 fake ACP 子进程产生启动 argv 证据。
2. 调用 `set_runtime_slot_model(..., "model-new")`。
3. 再发送一轮，使替换后的第二个真实 fake ACP 子进程启动。
4. 断言两条独立进程记录依次包含 `-m model-old` 与 `-m model-new`，且固定生产参数仍为 `agent --no-leader stdio`。

另外覆盖：

- inactive login 槽位切换后，重新 prepare 的启动 argv 包含所选 `-m`，但返回 `runtimeRestarted: false`。
- active API-key 槽位切换后仍保持 active，返回 `runtimeRestarted: true`。
- 新 executable 消失导致替换失败时，旧 runtime 被移除而不是继续冒充新模型。
- 既有 API-key 编辑命令改变 model 时也会重启 active runtime。

### 会话恢复结论

本单未调用 `session/load`，返回 `sessionRestore: "unavailable"`。原因不是 ACP 不支持：Grok adapter 内已有 `load_session`。阻断点是当前通用 `AgentRuntimeAdapter`/`RuntimeHost` 只暴露 Pillow 的 `RuntimeSessionId`，公共 `RuntimeSession` 不携带 provider ACP session ID；ConversationService 每轮固定调用 `start_session`，SQLite 也未持久化 provider session ID。若在本单内强接，会要求跨 agent protocol、runtime host、conversation storage 新增耐久契约，并扩大既定 ACP 边界。

后果：Pillow 的 SQLite 会话时间线仍保留，但切换后的下一轮会创建新的 provider 上下文。渲染层可在 `runtimeRestarted === true && sessionRestore === "unavailable"` 时提示用户。后续若设计 provider-session 恢复，应单独评审其持久化、项目隔离、失效和回退语义。

## 5. 正反与有界性测试

| 测试面 | 接受方向 | 拒绝/回落方向 |
|---|---|---|
| OpenAI 目录 | 正常 `data[].id` 真正解析出模型列表 | 畸形 `data` 形状返回 invalid，且错误不含响应 marker |
| Grok CLI | 普通单行输出真正解析出模型列表 | 非零、超时、截断、lossy UTF-8、控制字符返回固定错误 |
| 条目数 | 256 条可进入验证器 | 257 条拒绝 |
| ID 长度 | 512 字符以内可进入验证器 | 513 字符拒绝 |
| 控制字符 | 普通 ID 接受 | JSON 控制字符、终端转义字符、前导 tab 均拒绝 |
| 响应大小 | 有界内容解析 | 262145 字节拒绝，解析前/读取中均有上限 |
| endpoint 能力 | 正常 2xx 返回 `available` | 404/405/501 返回 `unsupported`，不是错误 |
| 宿主秘密边界 | 本地合成服务收到宿主解密后的 Bearer 并返回安全模型 ID | catalog DTO 精确键测试确认无 key，错误/Debug 不含 marker |

## 6. 退出码表

所有 Cargo 命令均使用任务指定的隔离 target 目录；为遵守本报告禁止本机绝对路径的要求，此处不展开该路径。没有任何命令通过管道接 `tail`。

| 命令 | 退出码 | 结果 |
|---|---:|---|
| `cargo test -p keeper-storage-sqlite` | 0 | 全量通过；含 0016 旧库升级、重开、login/API 模型持久化 |
| `cargo test -p pillow-runtime-slots` | 0 | 9 个 catalog 单测及 11 个 slot service 测试通过 |
| `cargo test -p keeper-adapter-grok-acp` | 0 | 全量通过；live opt-in 仍按原设计忽略 1 项 |
| `cargo test -p pillow-desktop` | 1 | 166 通过、9 失败、2 忽略；见下方基线分类 |
| `cargo test -p pillow-desktop runtime_composition::tests::path_probe_io_error_triggers_retry_then_success -- --exact --test-threads=1` | 0 | 全量并发运行中的首个探针计数失败单独复跑通过 |
| `cargo test -p pillow-desktop path_scope::tests::picker_selection_rejects_reparse_point_components -- --exact --test-threads=1` | 1 | Windows 创建目录 symlink 被系统以 1314 缺少特权拒绝 |
| `cargo test -p pillow-desktop runtime_slot_commands_tests:: -- --test-threads=1` | 0 | 本单宿主命令 17 通过、2 个显式验收默认忽略 |
| `cargo build -p keeper-adapter-grok-acp --bin fake_acp_agent` | 0 | 显式进程验收构件完成 |
| `cargo test -p pillow-desktop runtime_slot_commands_tests::login_model_catalog_command_runs_the_grok_models_subcommand -- --exact --ignored --test-threads=1` | 0 | 真执行 `grok models` 路径通过 |
| `cargo test -p pillow-desktop runtime_slot_commands_tests::active_model_switch_launches_the_next_acp_child_with_the_new_model_argument -- --exact --ignored --test-threads=1` | 0 | 两个真实子进程的旧/新 `-m` 启动证据通过 |
| `cargo fmt --all -- --check` | 0 | 最终通过 |
| `cargo clippy -p keeper-storage-sqlite -- -D warnings` | 0 | 严格通过 |
| `cargo clippy -p pillow-runtime-slots -- -D warnings` | 0 | 严格通过 |
| `cargo clippy -p keeper-adapter-grok-acp -- -D warnings` | 0 | 严格通过 |
| `cargo clippy -p pillow-desktop -- -D warnings`（首次） | 1 | 报出本单一处 `collapsible_if`；已改源码，未 suppress |
| `cargo clippy -p pillow-desktop -- -D warnings`（最终） | 0 | 严格通过 |
| `pnpm check` | 1 | `ENV-BLOCKED`：PowerShell 执行策略在门禁启动前拒绝 pnpm 脚本；按红线未绕过 |
| `git diff --check` | 0 | 无 whitespace error |

desktop 全量失败分类：独立复跑确认一个既有 reparse-point 测试受当前 Windows symlink 特权阻断；另一个既有全局 path-probe 注入测试在并发全量运行时计数受干扰、独立串行复跑通过，并在全量运行中毒化共享锁后引出 7 个连锁失败。本单未修改这些模块。

## 7. 写入边界审计

本 Worker 的写入仅限：

- `Cargo.lock`
- `crates/runtime-slots/Cargo.toml`
- `crates/runtime-slots/src/catalog.rs`
- `crates/runtime-slots/src/lib.rs`
- `crates/runtime-slots/tests/slot_service.rs`
- `crates/storage-sqlite/migrations/0016_runtime_slot_login_models.sql`
- `crates/storage-sqlite/src/lib.rs`
- `crates/storage-sqlite/src/migrations.rs`
- `crates/storage-sqlite/src/runtime_slot.rs`
- `crates/storage-sqlite/tests/runtime_slot_repository.rs`
- `apps/desktop/src-tauri/src/lib.rs`
- `apps/desktop/src-tauri/src/runtime_composition.rs`
- `apps/desktop/src-tauri/src/runtime_service_state.rs`
- `apps/desktop/src-tauri/src/runtime_slot_commands.rs`
- `apps/desktop/src-tauri/src/runtime_slot_commands_tests.rs`
- `crates/adapter-grok-acp/src/bin/fake_acp_agent/main.rs`
- `crates/adapter-grok-acp/tests/adapter_fake_child.rs`
- 本报告

审计结论：

- `crates/adapter-grok-acp/src/rpc/methods.rs` diff 为空，ACP methods 边界零差异。
- 未写 `apps/desktop/src/**` 或 `apps/desktop/tests/**`；这些目录中的并发脏改属于另一 Worker，保持原样。
- 未修改 `scripts/`，未运行 desktop E2E。
- `git diff --cached --name-only` 为空；未 stage、commit、reset、checkout 或执行其他 git 写操作。
- 未将真实 key、真实端点响应体或机器路径写入产品 DTO、日志、错误、本报告。

## 8. 残留风险

- 根门禁尚未获得有效执行结果：当前唯一合规入口为 `ENV-BLOCKED`，需总控在允许运行 pnpm 脚本的环境中原样重跑；不可把本报告中的单项绿灯当作 `pnpm check` 全绿。
- provider 会话上下文无法恢复，见第 4 节；切换后下一轮是新的 provider session。
- `/models` 不支持只按标准能力状态 404/405/501 判定。使用非标准状态或 2xx 错误体表达“不支持”的端点会返回固定错误而不会被误判；渲染层仍应保留手敲模型入口。
- `grok models` 解析器按每个非空行一个模型 ID 的实测格式严格处理；CLI 若未来改成表格/JSON，会安全失败并需要版本化解析器更新。
- 本 Worker 按总控已验事实实现，没有重新请求真实供应商端点；网络测试仅使用本地合成服务，未消耗真实 key。

`SLOT-MODEL-CATALOG-COMPLETE`
