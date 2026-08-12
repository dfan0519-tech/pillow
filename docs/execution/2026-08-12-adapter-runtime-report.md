# Pillow — provider 适配层接进运行时

**Status:** complete  
**Complete signal:** `ADAPTER-RUNTIME-COMPLETE`  
**Date:** 2026-08-12  
**Worker slot:** grok4 (heavy)  
**Write boundary:** `crates/runtime-slots/**`, `crates/provider-adapters/**`,
`apps/desktop/src-tauri/**`, this report

## 0. 边界

| 项 | 结论 |
| --- | --- |
| 允许改动 | `crates/runtime-slots/**`、`crates/provider-adapters/**`、`apps/desktop/src-tauri/**`、本报告 |
| 禁止 | `apps/desktop/src/**`、`docs/references/**`、根 `Cargo.toml`、git 写操作 |
| 真登录 / 厂商端点 | **无**（假串运行时拼接；OAuth refresh 用 FakeOauthClient；不发真实请求） |
| secret 字面量 | **无** |
| 新 workspace member | **未新增**（`pillow-provider-adapters` 已在 workspace；`runtime-slots` 只加 path dep） |

## 1. 退出码表（真跑）

环境：会话默认 `RUSTUP_HOME` 无 toolchain → 指向本机已装 toolchain 的 rustup home 后执行。  
命令（无管道截断；看输出本身）：

```text
cargo test -p pillow-provider-adapters -p pillow-runtime-slots > docs/execution/_adapter-runtime-test.log 2>&1
echo EXIT=%ERRORLEVEL%

node scripts/run-cargo.mjs test -p pillow-desktop --lib > docs/execution/_adapter-runtime-desktop-test.log 2>&1
echo EXIT=%ERRORLEVEL%
```

| 步骤 | 退出码 | 结果 |
| --- | ---: | --- |
| `cargo test -p pillow-provider-adapters -p pillow-runtime-slots` | **0** | provider-adapters lib **25** + integration **8**；runtime-slots lib **40** + provider_login **13** + slot_service **21** |
| `node scripts/run-cargo.mjs test -p pillow-desktop --lib` | **0** | **201** passed；2 ignored；0 failed |
| 日志 | `_adapter-runtime-test.log` / `_adapter-runtime-desktop-test.log` | 完整 stdout/stderr |

（`cargo check` 不编译 `#[cfg(test)]`，门禁以 `cargo test` 为准。）

## 2. T1 判定表：auth × provider → 路径

路径由 `resolve_execution_path(auth)` **纯函数推导**（auth + provider_id → adapter shape），**不是**用户勾选。

### 2.1 判定规则

| Auth | Provider / shape | Path | Governance |
| --- | --- | --- | --- |
| `GrokLogin` | （用户 profile） | **GrokShell** | Full |
| `ApiKey` | 隐式 `openai-compatible` + `base_url` | **GrokShell** | Full |
| `Oauth` | `example-oidc` / `openai` / `xai` / `custom-oidc` / 未知但有 `base_url` → StandardBearer + `/chat/completions` | **GrokShell** | Full |
| `Oauth` | `chatgpt-codex`（CustomBearer 剥 Authorization + 派生 `ChatGPT-Account-Id` + 非标准端点） | **NativeAdapter** | Full（Pillow 自为 harness） |
| `LocalCli` | 任意 | **NotRunnable** | **Incomplete**（`local_cli_ungoverned`） |

Shell 可表达性（`is_grok_shell_expressible`）：仅当

1. `AuthInjection::StandardBearer`
2. 无 `derived_headers`
3. `ProtocolShape::OpenAiChatCompletions`
4. endpoint 以 `/chat/completions` 结尾

任一不满足 → **必须** NativeAdapter；**禁止**塞进 grok 壳。

### 2.2 不能表达的组合如何拒绝

| 组合 | 行为 | 错误码 | 测试 |
| --- | --- | --- | --- |
| `chatgpt-codex` OAuth 调用 `prepare_grok` | 拒绝；不写 private `config.toml` | `runtime_slot_path_not_grok_shell_expressible` | `prepare_grok_rejects_codex_rewrite_provider_not_stuffed_into_shell` |
| `reject_if_not_shell_expressible(codex)` | 显式拒绝 | 同上 | lib `reject_stuffing_codex_into_grok_shell` |
| `prepare_native_request` 传入 shell-expressible adapter | 拒绝双路径混淆 | `runtime_slot_native_prepare_failed` | `native_prepare_refuses_shell_expressible_adapter` |
| LocalCli `prepare_slot` / start | 拒绝 | `runtime_slot_local_cli_not_runnable` / `external_cli_launch_unsupported` | 既有 + `path_matrix` |
| 凭据 kind 不符（Codex 要 OAuth、给 ApiKey） | 拒绝 | `runtime_slot_credential_kind_mismatch` | `credential_kind_mismatch_on_native_prepare` |

### 2.3 统一 prepare API

```text
prepare_slot(project, slot)
  → resolve_execution_path
  → GrokShell: prepare_grok_shell_inner (既有 private home + env_key)
  → NativeAdapter: resolve_oauth_access_token (access only)
                   + prepare_native_request (provider-adapters::prepare_request)
  → NotRunnable: reject
```

`prepare_grok` 保留兼容，但对 NativeAdapter 路径 **fail-closed**（不再静默 grok 壳）。

## 3. T2：NativeAdapter 路径上治理成立的机制

### 3.1 机制（显式，不许悄悄降级）

常量 `NATIVE_ADAPTER_GOVERNANCE_MECHANISM = "host_harness_tools_write_process"`：

| 层面 | 谁负责 | 说明 |
| --- | --- | --- |
| 模型 HTTP 请求形状 | `pillow-provider-adapters` | endpoint / 头改写 / 白名单 / 派生头 |
| 工具白名单 | Pillow host（runtime-host / conversation） | **不**随 vendor CLI 旁路 |
| 写提案 | Pillow host | 写仍回宿主审批路径 |
| 进程提案 | Pillow host | 进程仍回宿主治理 |

NativeAdapter **不是**把 agent 工具面交给外部 CLI；Pillow **自己就是 harness**。因此：

- `PathGovernance::Full`
- `may_be_controller() == true`（与既有 Oauth→Full / Controller 规则一致）

### 3.2 做不到时的标注（不许悄悄降级）

| 路径 | 标注 | Controller |
| --- | --- | --- |
| LocalCli | `PathGovernance::Incomplete { LocalCliUngoverned }` | **禁止**（store + path 层） |
| NativeAdapter 若将来无法保证 host harness | 必须标 Incomplete，且 **不能** 被设为总控 | —（本票未引入这种降级） |

测试：

| 测试 | 钉住 |
| --- | --- |
| `native_adapter_governance_mechanism_is_host_harness` | 机制常量 + Incomplete 不可 Controller |
| `prepare_slot_native_injects_access_never_refresh_and_keeps_full_governance` | Native prepare → Full + mechanism + Controller 槽可建 |
| `chatgpt_codex_oauth_is_native_adapter_full` | 路径 Native + Full + may_be_controller |
| `local_cli_is_incomplete_and_not_controller` | Incomplete 不可 Controller |
| desktop `start_native_adapter_codex_slot_prepares_without_grok_shell` | start **不** `activate_grok`；governance=`full`；status=`native_adapter_prepared` |

### 3.3 Desktop start 行为

| Path | start 行为 |
| --- | --- |
| GrokShell | 既有：`prepare_grok` → `activate_grok` → status `active` |
| NativeAdapter | `prepare_slot` 校验请求形状；**绝不** activate_grok；status `native_adapter_prepared`；wire `kind` 仍为 `governed_grok`（渲染层 closed allowlist 只认该 label；列表 DTO 对 Oauth 已是该 wire kind） |
| LocalCli | `external_cli_launch_unsupported` |

完整 turn 级 Native 对话运行时（把 `PreparedRequest` 送上 HTTP 并接 ACP 事件面）是后续票；本票保证 **路径选择 + prepare 接线 + 禁止塞 grok 壳** 已通。

## 4. Token 不外泄证据

### 4.1 refresh token 永不离开 runtime-slots

| 测试 | 断言 |
| --- | --- |
| `oauth_prepare_injects_only_access_token_never_refresh`（既有） | GrokShell prepare 的 `api_key` 字节 == access；≠ refresh；config.toml 无 token |
| `prepare_slot_native_injects_access_never_refresh_and_keeps_full_governance` | Native `Authorization` 仅 access；Debug 无 access/refresh；**不写** `.grok/config.toml` |
| `oauth_prepare_refreshes_when_expired_…`（既有） | refresh 仅出现在 Fake 客户端 form；private config 无 refresh |

实现：`resolve_oauth_access_token` 只返回 access；refresh 在 form 构造后 drop；Native 路径用 `oauth_access_credential(access, account_label)` — **无 refresh 字段**。

### 4.2 token 不进 DTO / 日志 / 错误文本

| 测试 | 断言 |
| --- | --- |
| `native_prepare_uses_access_only_and_redacts_debug` | `PreparedNativeAdapter` Debug 含 REDACTED，无 access/refresh |
| `prepare_grok_rejects_codex_…` | 错误 Display 无 access/refresh |
| provider-adapters 既有 | `PreparedRequest` / `RequestHeaders` / `PrepareError` / `SecretToken` Debug 脱敏 |
| desktop start 测试 | 错误文本无 access/refresh |

## 5. 写入边界审计 + 残留风险

### 5.1 写入边界

| 操作 | 是否发生 |
| --- | --- |
| 改 `apps/desktop/src/**` | **否** |
| 改 `docs/references/**` / 根 `Cargo.toml` | **否** |
| git 写操作 | **否** |
| 真登录 / 真厂商 HTTP | **否** |
| 新建 crate | **否** |
| `runtime-slots` 依赖 `pillow-provider-adapters` | **是**（path dep） |
| 注册 `list_provider_login_catalog` | **是**（renderer 已 invoke；host 补齐注册，公开 catalog only） |

### 5.2 交付文件

```text
crates/provider-adapters/src/shell.rs          (new)
crates/provider-adapters/src/lib.rs            (export shell)
crates/runtime-slots/Cargo.toml                (+ pillow-provider-adapters)
crates/runtime-slots/src/execution_path.rs     (new)
crates/runtime-slots/src/lib.rs                (prepare_slot / path / errors)
crates/runtime-slots/tests/slot_service.rs     (T1/T2/T3 tests)
apps/desktop/src-tauri/src/runtime_slot_commands.rs
apps/desktop/src-tauri/src/runtime_slot_commands_tests.rs
apps/desktop/src-tauri/src/lib.rs              (+ list_provider_login_catalog)
docs/execution/2026-08-12-adapter-runtime-report.md
docs/execution/_adapter-runtime-test.log
docs/execution/_adapter-runtime-desktop-test.log
```

### 5.3 残留风险

| 风险 | 说明 |
| --- | --- |
| **Native turn 运行时未接满** | prepare 产出 `PreparedRequest`；desktop start 不 activate_grok，status=`native_adapter_prepared`。完整会话 turn（HTTP transport + 事件面）需后续票接 host harness。 |
| **wire kind 仍为 `governed_grok`** | 渲染层 closed allowlist 只认该 kind；路径靠 `status` + 不 activate 区分。若 UI 要显式 badge，需 renderer 票放宽 allowlist（本票不可改 `apps/desktop/src/**`）。 |
| **`chatgpt-codex` 登录 UI** | catalog 尚无独立 codex 行；OAuth 槽位可用 `provider_id=chatgpt-codex` 创建后走 Native 路径。完整登录流另票。 |
| **动态模型 allowlist** | 仍为静态 Codex 子集（见 provider-adapters 报告）。 |
| **宿主日志旁路** | 若将来 transport `eprintln` Authorization 原始值，会绕过 Debug 脱敏；传输层必须自建 redacted sink。 |
| **JWT 未验签** | 与 adapter 层一致；仅本地 claim 提取。 |

### 5.4 PUBLIC API 增量

**provider-adapters:** `is_grok_shell_expressible`, `request_adapter_for_provider`, `AdapterLookupError`

**runtime-slots:** `SlotExecutionPath`, `PathGovernance`, `ExecutionPathDecision`, `ExecutionPathError`, `PreparedNativeAdapter`, `PreparedSlot`, `resolve_execution_path`, `prepare_slot`, `resolve_path`, `prepare_native_request`, `oauth_access_credential`, `NATIVE_ADAPTER_GOVERNANCE_MECHANISM`

**desktop:** `list_provider_login_catalog`（公开 profile 列表）

---

`ADAPTER-RUNTIME-COMPLETE`
