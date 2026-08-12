# Pillow — 自有 provider 适配层执行报告

**Status:** complete  
**Complete signal:** `PROVIDER-ADAPTERS-COMPLETE`  
**Date:** 2026-08-12  
**Worker slot:** grok4 (heavy)  
**Write boundary:** `crates/provider-adapters/**` + root `Cargo.toml` members 一行

## 0. 边界

| 项 | 结论 |
| --- | --- |
| 允许改动 | 新建 `crates/provider-adapters/**`；根 `Cargo.toml` workspace members **仅加** `"crates/provider-adapters"` |
| `apps/desktop/**` / `crates/runtime-slots/**` | **未改**（另有 Worker） |
| git 写操作 | **无** |
| 网络 / 真登录 / 厂商端点 | **无**（纯领域：描述 + `prepare_request`） |
| 读真实认证文件 | **无**（尤其未读任何 `auth.json`） |
| 硬编码 client_id / secret | **无** |
| `unsafe` / 库路径 `unwrap` | **无** |
| Cargo.lock | 跑 `cargo test` 时 workspace 解析自动更新（非手改业务逻辑） |

## 1. 退出码表（真跑）

环境：会话默认 `RUSTUP_HOME` 无 toolchain → 使用本机已装 toolchain 的 rustup home 后执行。  
命令（无管道截断；日志整文件保留）：

```text
cargo test -p pillow-provider-adapters > docs/execution/_provider-adapters-test.log 2>&1
echo EXIT=%ERRORLEVEL%
```

| 步骤 | 退出码 | 结果 |
| --- | ---: | --- |
| `cargo test -p pillow-provider-adapters` | **0** | lib 单测 **19** passed；integration `prepare_contract` **8** passed；doc-tests 0；无 warning |
| 日志 | `docs/execution/_provider-adapters-test.log` | 完整 stdout/stderr |

再次确认：

```text
EXIT=0
```

（`cargo check` 不编译 `#[cfg(test)]`，故门禁以 `cargo test` 为准。）

## 2. `ProviderAdapter` 字段表 + 两个示例出处

### 2.1 描述模型

| 字段 | 类型 | 含义 |
| --- | --- | --- |
| `provider_id` | `String` | 稳定 id（如 `openai-compatible` / `chatgpt-codex`） |
| `display_name` | `String` | UI/日志用显示名 |
| `endpoint_url` | `String` | **绝对**请求 URL（可非 `/v1/chat/completions`） |
| `auth` | `AuthInjection` | `StandardBearer` 或 `CustomBearer { header_name, strip_standard_authorization }` |
| `constant_headers` | `Vec<(String,String)>` | 常量头（如 `Content-Type`） |
| `derived_headers` | `Vec<DerivedHeader>` | 从凭据派生；拿不到 → **拒绝** |
| `model_allowlist` | `Vec<String>` | **空 = 不限制**；非空 = 精确 id 匹配，不静默换模型 |
| `protocol` | `ProtocolShape` | `OpenAiChatCompletions` / `OpenAiResponses` / `AnthropicMessages` / `Other` |
| `required_credential` | `CredentialKind` | `ApiKey` 或 `OAuth` |

相关类型：

| 类型 | 作用 |
| --- | --- |
| `Credential::{ApiKey, OAuth}` | 仅内存形状；`SecretToken` 的 Debug 恒为 `SecretToken([REDACTED])` |
| `DerivedHeader::ChatGptAccountId` | 解析显式 `account_id` 或 JWT claims |
| `PreparedRequest { url, headers, body, model, protocol }` | 准备结果；**不发送** |
| `prepare_request(adapter, credential, model, body)` | 纯函数 → `PreparedRequest` 或 `PrepareError` |

### 2.2 示例 A — 标准 OpenAI 兼容

| 项 | 值 | 出处 |
| --- | --- | --- |
| 工厂 | `openai_compatible(base_url)` | `crates/provider-adapters/src/adapters.rs` |
| endpoint | `{base_url}/chat/completions`（`base_url` 去尾 `/`） | OpenAI 官方文档：Chat Completions create — `POST …/v1/chat/completions`，Bearer 认证：<https://platform.openai.com/docs/api-reference/chat/create> |
| auth | `StandardBearer` → `Authorization: Bearer <api_key>` | 同上 |
| credential | `ApiKey` | 标准 API key 路径 |
| allowlist | 空（不限制） | 兼容代理上模型集由用户配置决定 |
| protocol | `OpenAiChatCompletions` | 同上文档 |

### 2.3 示例 B — 需改写头的 Codex 订阅路径

| 项 | 值 | 出处（OpenCode MIT，commit `d470434`，只读参考） |
| --- | --- | --- |
| 工厂 | `chatgpt_codex_subscription()` | `adapters.rs` |
| endpoint | `https://chatgpt.com/backend-api/codex/responses` | `packages/opencode/src/plugin/openai/codex.ts:12`（`CODEX_API_ENDPOINT`） |
| auth | `CustomBearer`：先删标准 `Authorization`，再设 `Authorization: Bearer <access>` | `codex.ts:343–405`（delete authorization → set Bearer access） |
| derived | `ChatGPT-Account-Id` | `codex.ts:406–408`；claims 路径 `codex.ts:57–63`（`chatgpt_account_id` / nested auth / `organizations[0].id`） |
| allowlist | `gpt-5.5`, `gpt-5.3-codex-spark`, `gpt-5.4`, `gpt-5.4-mini` | `codex.ts:15`（`ALLOWED_MODELS`） |
| credential | `OAuth` | `codex.ts:331`（`auth.type !== "oauth"` 时不走改写路径） |
| protocol | `OpenAiResponses` | 端点 path `/codex/responses` |

**未写死 / 未确定：**

- OAuth `client_id`、issuer、device-auth 轮询：**故意不进本 crate**（禁止硬编码 secret；登录在 `runtime-slots`）。
- OpenCode `provider.models` 的**动态**版本过滤（`codex.ts:280–317`，如 `gpt-(\d+\.\d+) > 5.4`、`DISALLOWED_MODELS`）**未完整编码**为静态表；本 adapter 仅钉 `ALLOWED_MODELS` 子集。接运行时时若要对齐完整过滤，需另票扩展 allowlist 策略。

## 3. 三条拒绝情形 — 测试证据

| 拒绝情形 | 测试 | 断言 |
| --- | --- | --- |
| 模型不在白名单（不静默换模型） | integration `rejects_model_not_on_allowlist_without_substitution`；lib `prepare::tests::model_not_on_allowlist_is_rejected_not_substituted` | `PrepareError::ModelNotAllowed`；同凭据下 allowlisted `gpt-5.4` 仍成功且 `prepared.model == "gpt-5.4"` |
| 凭据类型不符 | integration `rejects_credential_kind_mismatch`；lib `credential_kind_mismatch_rejected` | Codex 要 OAuth、给 ApiKey → `CredentialKindMismatch`；对称：OpenAI 兼容要 ApiKey、给 OAuth → 同样拒绝 |
| 派生头拿不到 | integration `rejects_when_derived_chatgpt_account_id_unavailable`；lib `missing_derived_account_id_rejected` | 无 `account_id`、access 非 JWT → `DerivedHeaderUnavailable { header_name: "ChatGPT-Account-Id" }`；**不会**产出缺该头的 `PreparedRequest` |

日志对应行（均 `ok`）：

```text
test rejects_model_not_on_allowlist_without_substitution ... ok
test rejects_credential_kind_mismatch ... ok
test rejects_when_derived_chatgpt_account_id_unavailable ... ok
```

## 4. Token 不进 Debug / 日志 / 错误 — 测试证据

假串**运行时拼接**（源码无 secret 形状字面量），例如 `format!("{}{}", "debug-redact-", "token-dd04-SECRET")`。

| 测试 | 断言 |
| --- | --- |
| `prepared_request_debug_never_contains_token` | `format!("{:?}", prepared)` 不含 token / `Bearer {token}` |
| `codex_prepared_debug_never_contains_access_token` | Codex 成功路径 Debug 不含 access token |
| `prepare_error_messages_never_contain_token` | 三类 `PrepareError` 的 Display + Debug 均不含 token |
| `secret_token_and_credential_debug_redacted` | `SecretToken` / `Credential` Debug 脱敏 |
| lib：`types::tests::request_headers_debug_redacts_authorization` | `Authorization` 值 → `Bearer [REDACTED]` |
| lib：`prepare::tests::prepare_error_display_has_no_token` | 错误文本无 token |

实现要点：`RequestHeaders` Debug 对 `Authorization` / bearer 形值脱敏；`SecretToken` / `Credential` / `PrepareError` 永不嵌入 access material。

## 5. 写入边界审计

| 操作 | 是否发生 |
| --- | --- |
| 发 HTTP / 起进程 | **否** |
| 读/写 DPAPI 或任何 auth 文件 | **否** |
| 改 `runtime-slots` / desktop / SQLite | **否** |
| 新建源码 | 仅 `crates/provider-adapters/**` |
| workspace 注册 | 根 `Cargo.toml` 一行 `"crates/provider-adapters"` |
| 报告 | `docs/execution/2026-08-12-provider-adapters-report.md` + 测试日志 |
| 返回/打印真实 token | **否**（仅运行时假串测脱敏） |

## 6. 残留风险 — 接进 Pillow 运行时最容易接错的地方

| 风险点 | 说明 |
| --- | --- |
| **仍走 grok 壳 `base_url`+`env_key`** | Codex 端点与改写头**不能**靠私有 `config.toml` 表达。若 prepare 结果又被塞回「只认 Bearer + OpenAI chat」的子进程，订阅路径会静默失效或打错端点。 |
| **允许「缺 `ChatGPT-Account-Id` 仍发送」** | 本 crate 拒绝不完整请求；宿主若在 transport 层 fallback 掉该头，会复现 opencode 依赖的 account 绑定失败。应把 `PrepareError` 映成用户可见错误，不要吞。 |
| **模型静默降级/替换** | 白名单拒绝是契约。UI 或 runtime 若在失败时自动换一个「可用」模型，会破坏与 opencode 一致的约束。 |
| **凭据 kind 混用** | 槽位存了 API key 却选 `chatgpt-codex` adapter（或反过来）必须在 **prepare 前或 prepare 时** 失败；不要用 dummy key 骗过 StandardBearer。 |
| **动态 allowlist 漂移** | OpenCode 的版本过滤未全量移植；厂商加模型后静态表会过时 → 误拒。需产品票决定：跟上游表、可配置表、或宿主侧 catalog 交集。 |
| **JWT 未验签** | `jwt` 模块只解 payload 取 account id（与 opencode 客户端侧一致）。签名校验不在本层；伪造 JWT 只能骗本地 prepare，不能替代真 TLS 会话。 |
| **Debug 旁路** | 宿主若 `headers.get("Authorization")` 后 `eprintln`/`format` 进日志，会绕过 `RequestHeaders` Debug 脱敏。传输层日志必须自建红acted sink。 |
| **body 内嵌密钥** | `body` 原样透传；本层不扫 body。调用方勿把 token 放进 JSON body。 |
| **与 `runtime-slots` 双写** | 另一 Worker 管 DPAPI/OAuth 存储。集成时 credential 应从 slot prepare 注入，**不要**再读用户全局 auth 文件。 |

## 7. 交付清单

```text
crates/provider-adapters/Cargo.toml
crates/provider-adapters/src/lib.rs
crates/provider-adapters/src/types.rs
crates/provider-adapters/src/credential.rs
crates/provider-adapters/src/prepare.rs
crates/provider-adapters/src/adapters.rs
crates/provider-adapters/src/jwt.rs
crates/provider-adapters/src/redact.rs
crates/provider-adapters/tests/prepare_contract.rs
Cargo.toml  (+ "crates/provider-adapters")
docs/execution/2026-08-12-provider-adapters-report.md
docs/execution/_provider-adapters-test.log
```

**PUBLIC API 摘要：** `ProviderAdapter`, `AuthInjection`, `DerivedHeader`, `ProtocolShape`, `Credential`, `SecretToken`, `prepare_request`, `PreparedRequest`, `PrepareError`, `openai_compatible`, `chatgpt_codex_subscription`.

---

`PROVIDER-ADAPTERS-COMPLETE`
