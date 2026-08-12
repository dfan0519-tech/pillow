# Provider 登录目录实现报告

**状态：** complete  
**日期：** 2026-08-12  
**Worker：** codex1（写码）  
**总控：** Claude  
**完成信号：** `PROVIDER-REGISTRY-COMPLETE`

## 0. 范围与结论

本单新增一个纯数据、纯转换的 provider 登录目录：

- `ProviderLoginProfile` 明确记录登录方式、issuer、授权/设备授权/token
  端点、scopes、`client_id` 来源分类、账号标识策略和逐字段出处。
- `resolve_oauth_transport_parameters` 把 profile 与用户补充的公开元数据转换成
  通用 OAuth transport 的 PKCE 或 device-code 参数；缺字段或方式不支持时返回固定的
  typed error。
- 没有新增网络代码，没有读取认证文件，没有执行真实登录，没有向厂商端点发请求。
- 没有把参考实现中的任何真实 `client_id` 搬进 Pillow；xAI、GitHub Copilot、
  OpenAI 均保持 `Unknown`。
- `SlotAuth::Oauth`、SQLite schema 和 migration 均未修改；OAuth/legacy round-trip 与
  legacy migration 定向测试继续通过。

参考仓库用只读 `git show` 固定在
`d4704347465c1ee63d0c213ed00e648e7f0231c5`，且参考工作树无本地改动。

### 固定 commit 与简报括注的差异

简报把 `plugin/xai.ts` 括注为 PKCE，但上述固定 commit 的实际内容在
`packages/opencode/src/plugin/xai.ts:8-17,297-324` 明确实现 RFC 8628 device-code。
本目录以固定 commit 的源码事实为准，将 xAI 标为 `device_code`；未猜测一个不存在于
该版本文件中的 PKCE authorize endpoint。

## 1. 退出码表（真实运行）

所有 Cargo 命令均以 `> log 2>&1; echo EXIT=$LASTEXITCODE` 方式捕获，未把输出管道接到
`tail`。临时日志在审阅后删除，不作为产品文件保留。一次向 workspace 外临时目录写
日志被沙箱拒绝，Cargo 未启动，因此不列为测试结果。

| 阶段 | 命令 | EXIT | 观测结果 |
| --- | --- | ---: | --- |
| TDD RED：目录 | `cargo test -p pillow-runtime-slots --test provider_login catalog_` | **101** | 预期失败：新 profile API 尚不存在 |
| TDD GREEN：目录 | `cargo test -p pillow-runtime-slots --test provider_login`（首轮） | **0** | 3 passed |
| TDD RED：转换 | `cargo test -p pillow-runtime-slots --test provider_login conversion_` | **101** | 预期失败：新转换 API 尚不存在 |
| TDD GREEN：转换 | 同上 | **0** | 4 passed |
| TDD RED：未知 client guard | `cargo test -p pillow-runtime-slots --test provider_login conversion_ignores_embedded_client_id_when_source_is_unknown` | **101** | 预期失败：转换曾接受不可信内嵌值 |
| TDD GREEN：未知 client guard | 同上 | **0** | 1 passed |
| TDD RED：账号标识 | `cargo test -p pillow-runtime-slots --test provider_login account_identifier_strategy_is_explicit_and_never_inferred` | **101** | 预期失败：Anthropic 曾被无出处地标成用户提供 |
| TDD GREEN：账号标识 | 同上 | **0** | 1 passed |
| 格式化 | `cargo fmt -p pillow-runtime-slots` | **0** | 仅该 package |
| 格式终验 | `cargo fmt -p pillow-runtime-slots -- --check` | **0** | 无格式差异 |
| 聚焦终验 | `cargo test -p pillow-runtime-slots --test provider_login` | **0** | **13 passed; 0 failed** |
| 指定 crate 收口 | `cargo test -p pillow-runtime-slots` | **0** | lib 31 + provider 13 + slot_service 16；共 **60 passed; 0 failed** |
| OAuth/legacy round-trip | `cargo test -p keeper-storage-sqlite oauth_and_legacy_auth_variants_round_trip_without_token_material` | **0** | 目标测试 1 passed |
| legacy migration round-trip | `cargo test -p keeper-storage-sqlite legacy_slot_rows_migrate_round_trip_preserving_secret_ref` | **0** | 目标测试 1 passed |

没有用 `cargo check` 代替测试；最终指定 crate gate 编译并运行了 `#[cfg(test)]`。

## 2. Profile 表：登录方式、结果与 `client_id` 分类

`Known` 表示固定 commit 有明确字段出处；`UserSupplied` 表示必须由用户提供；
`Unknown` 表示未取得足以采用的可信值；`N/A` 表示该登录方式不使用该字段。

| provider | 登录方式 | issuer | authorize / device endpoint | token endpoint | scopes | `client_id` 来源 | 登录后账号标识 |
| --- | --- | --- | --- | --- | --- | --- | --- |
| xAI | `device_code` | `Unknown` | device: `https://auth.x.ai/oauth2/device/code` | `https://auth.x.ai/oauth2/token` | `openid profile email offline_access grok-cli:access api:access` | `Unknown`，值为空 | `Unknown`；参考成功结果只保存 token/expiry，没有账号字段 |
| GitHub Copilot | `device_code` | `Unknown` | device: `https://github.com/login/device/code` | `https://github.com/login/oauth/access_token` | `read:user` | `Unknown`，值为空 | `Unknown`；参考成功结果只返回 access material，没有账号字段 |
| Anthropic | `api_key_only` | N/A | N/A | N/A | N/A | `Unknown`（OAuth 不适用，未填值） | `Unknown`；参考只证明 API-key header，不推测账号标识 |
| OpenAI | `pkce` | `https://auth.openai.com` | authorize: `https://auth.openai.com/oauth/authorize` | `https://auth.openai.com/oauth/token` | `openid profile email offline_access` | `Unknown`，值为空 | 先检查 ID token，再检查 access token；claim 顺序为 `chatgpt_account_id`、namespaced account id、首个 organization id |
| Custom OIDC | `pkce` | `UserSupplied` | authorize: `UserSupplied` | `UserSupplied` | `UserSupplied` | `UserSupplied` | `UserSupplied` account label |

本单没有任何 `PublicDocumented` client ID。参考源码中出现一个值不等于厂商官方文档明确
允许第三方复用；因此 xAI、GitHub Copilot、OpenAI 的值一律没有复制。

## 3. 逐项出处

| provider / 字段 | 出处 | 采用结论 |
| --- | --- | --- |
| xAI：client 声明、device/token endpoint、scopes | `../_ref/opencode/packages/opencode/src/plugin/xai.ts:5-17` | 端点/scopes 可记录；client 值没有厂商第三方复用证明，分类 `Unknown` 且不复制 |
| xAI：provider 与登录方式、成功结果 | `../_ref/opencode/packages/opencode/src/plugin/xai.ts:208,297-324` | `device_code`；成功结构没有账号标识，标 `Unknown` |
| GitHub Copilot：provider、device/token endpoint | `../_ref/opencode/packages/opencode/src/plugin/github-copilot/copilot.ts:9-23,60-61` | 固定 public GitHub.com 端点；issuer 未由源码声明，标 `Unknown` |
| GitHub Copilot：method、scope、token result | `../_ref/opencode/packages/opencode/src/plugin/github-copilot/copilot.ts:182-305` | `device_code`、`read:user`；client 值不复制；账号标识 `Unknown` |
| Anthropic：provider 与 API-key auth | `../_ref/opencode/packages/llm/src/providers/anthropic.ts:7-17` | 仅 `api_key_only`；OAuth 字段不编造 |
| OpenAI：issuer、authorize/token endpoint、scopes、PKCE | `../_ref/opencode/packages/opencode/src/plugin/openai/codex.ts:10-11,78-133,429-454` | 记录公开元数据形状；client 值不复制，分类 `Unknown` |
| OpenAI：账号 claim 顺序 | `../_ref/opencode/packages/opencode/src/plugin/openai/codex.ts:38-75` | ID token 优先、access token fallback；三个 claim path 按源码记录 |
| Custom OIDC：discovery 字段 | `https://openid.net/specs/openid-connect-discovery-1_0.html` | issuer、authorization endpoint、token endpoint、scopes 由用户自己的 IdP 配置提供 |
| Custom OIDC：PKCE | `https://www.rfc-editor.org/rfc/rfc7636` | 使用标准 PKCE；client ID 由用户为自己注册的应用提供 |

目录本身携带 `ProviderLoginEvidence { fields, source }`；测试
`every_known_or_user_supplied_profile_field_has_evidence` 保证每个 `Known` / `UserSupplied`
字段都有映射，`every_profile_has_relative_or_official_evidence` 禁止本机绝对路径进入证据。

## 4. T3：profile → 通用 OAuth 参数

纯函数：

```text
resolve_oauth_transport_parameters(profile, supplement)
  -> OauthTransportParameters::Pkce { issuer, authorize_endpoint,
                                      token_endpoint, scopes, client_id }
   | OauthTransportParameters::DeviceCode { issuer,
                                             device_authorization_endpoint,
                                             token_endpoint, scopes, client_id }
   | ProviderLoginConversionError
```

规则：

1. profile 的 `Known` issuer/endpoint/scopes 不能被用户补充值覆盖。
2. `UserSupplied` 和 `Unknown` 字段只从补充项取值，空白或控制字符按缺失处理。
3. `client_id` 额外按来源分类处理：只有 `PublicDocumented` 才能采用目录内值；
   `UserSupplied` / `Unknown` 即使被错误构造成带内嵌值，也忽略该值并要求用户补充。
4. 输出只含公开 OAuth 参数，没有 API key、client secret、authorization code、
   access token 或 refresh token。

### 明确拒绝证据

| 拒绝原因 | error | 测试 |
| --- | --- | --- |
| 缺 `client_id` | `MissingClientId` | `conversion_rejects_missing_client_id` |
| `Unknown` 来源夹带内嵌 client 值 | `MissingClientId`，且错误文本不回显该值 | `conversion_ignores_embedded_client_id_when_source_is_unknown` |
| 缺 issuer | `MissingIssuer` | `conversion_rejects_missing_issuer` |
| `api_key_only` 送入 OAuth transport | `UnsupportedLoginMethod(ApiKeyOnly)` | `conversion_rejects_api_key_only_method_before_oauth_fields` |
| 缺 authorize endpoint | `MissingAuthorizeEndpoint` | `conversion_rejects_each_other_missing_transport_field` |
| 缺 device authorization endpoint | `MissingDeviceAuthorizationEndpoint` | 同上 |
| 缺 token endpoint | `MissingTokenEndpoint` | 同上 |
| scopes 缺失或为空 | `MissingScopes` | 同上 |

正向证据：

- `known_openai_profile_expands_to_pkce_transport_parameters`：固定字段优先，用户不能覆盖。
- `xai_profile_expands_to_device_code_transport_parameters`：补齐 issuer 与自有 client 后输出
  device-code 参数。
- `custom_oidc_uses_only_user_supplied_public_metadata`：全部 transport 字段来自用户补充。

## 5. 没有取得的可信配置，以及用户必须提供什么

### xAI

- **未取得：** 可供 Pillow 第三方复用的官方 client ID；源码也没有声明 issuer 字段；
  登录后账号标识策略未知。
- **用户要提供：** 自己拥有或厂商明确授权其使用的 client ID，以及与已列 device/token
  端点相符的 issuer。需要持久账号显示时，还需由上层另行提供经过验证的 account label。
- **不能做：** 直接复用参考 CLI 的 client ID。

### GitHub Copilot

- **未取得：** 可供 Pillow 复用的官方 client ID；源码没有独立 issuer 字段；成功结果没有
  账号标识。
- **用户要提供：** 自己注册且启用 device flow 的 GitHub OAuth app client ID，以及
  GitHub.com issuer。当前 profile 只覆盖 public GitHub.com；Enterprise 域名没有猜入目录。

### Anthropic

- **未取得：** OAuth issuer、authorize/token endpoints、scopes、client ID 或 OAuth 后账号
  标识；固定 commit 只证明 API-key auth。
- **用户要提供：** 通过既有 API-key 流程提供自己的 key；不能把本 profile 转成 OAuth。
  本目录和转换 DTO 都不承载该 secret。

### OpenAI

- **未取得：** 厂商公开声明可供 Pillow 第三方使用的 client ID。
- **用户要提供：** 自己拥有或厂商明确授权其使用的 OAuth client ID。
- **重要边界：** 参考插件还使用专有 consumer backend 和额外 authorize 参数；它们不在
  通用 OAuth transport 参数内。本 profile 不代表 ChatGPT Plus/Pro 或 Codex consumer
  backend 已接入 Pillow，也不应拿参考 client ID 冒充参考客户端。

### Custom OIDC

- **目录故意不提供：** issuer、authorize endpoint、token endpoint、scopes、client ID。
- **用户要提供：** 从自己 IdP 的官方 discovery/admin 配置取得全部字段，并使用自己注册
  的 public client；账号显示 label 也由用户/上层提供。

## 6. Secret、DTO 与网络边界

- `ProviderLoginSupplement` 与 `OauthTransportParameters` 只含公开 metadata；没有 secret
  字段，也没有接收 token/code 的 API。
- `ProviderLoginConversionError` 使用固定文本，不拼接用户输入。
- 测试 client 值均由无敏感含义的分片在运行时拼接，测试 URL 使用保留的 `.invalid`
  域名；源码扫描确认没有参考实现的 client ID。
- 新模块与 13 个新测试没有 socket/fetch/WinHTTP 调用。完整 crate 的既有网络测试只打
  `127.0.0.1` 本地测试服务器。
- 未读取、打印或写入任何真实认证文件；未执行浏览器登录；未调用真实厂商端点。

## 7. `SlotAuth::Oauth` 兼容证据

- 本单没有修改 storage crate、migration、`SlotAuth::Oauth` variant 或任何持久化字段。
- `lib.rs` 只新增 module 声明与 public re-export。
- 定向 round-trip：`oauth_and_legacy_auth_variants_round_trip_without_token_material`，EXIT 0。
- 定向 legacy migration：`legacy_slot_rows_migrate_round_trip_preserving_secret_ref`，EXIT 0。
- `pillow-runtime-slots` 完整 gate 的 `slot_service` 16/16 继续绿，包括 OAuth access-only
  injection 与 refresh fail-closed 测试。

## 8. `THIRD_PARTY_NOTICES.md` 建议补充（本单不修改根文件）

现有 OpenCode MIT 条目与 fixed commit 已存在，但其 provider-plugin 描述尚未覆盖本单的
metadata adaptation。建议在现有 credential-model table 追加：

```markdown
| `crates/runtime-slots/src/provider_login.rs` | `packages/opencode/src/plugin/xai.ts`; `packages/opencode/src/plugin/github-copilot/copilot.ts`; `packages/opencode/src/plugin/openai/codex.ts`; `packages/llm/src/providers/anthropic.ts` | Adapted fixed-commit provider login metadata: observed login method, public endpoint/scope shape, and account-identifier evidence. No upstream client ID, token, auth-file I/O, provider transport, or proprietary backend was copied. |
```

同时建议把现有 “provider plugins not claimed as ports” 说明收窄为：provider transport、
poll loop、callback page 和专有 backend 没有 port；本单只把上述 fixed-commit 文件作为
可审计 metadata 来源。现有 MIT license 文本无需重复。

## 9. 写入边界审计

### 本 Worker 的变更

| 路径 | 变更 |
| --- | --- |
| `crates/runtime-slots/src/provider_login.rs` | 新 profile 类型、五条目录数据、出处、纯转换与 typed errors |
| `crates/runtime-slots/tests/provider_login.rs` | 13 个目录/转换/拒绝/来源测试 |
| `crates/runtime-slots/src/lib.rs` | module 注册与 re-export |
| `docs/execution/2026-08-12-provider-registry-report.md` | 本报告 |

### 未由本 Worker 修改

- `apps/desktop/**`
- `crates/provider-adapters/**`
- `crates/knowledge-indexer/**`
- 根 `Cargo.toml`
- `THIRD_PARTY_NOTICES.md`
- 认证文件与参考仓库

执行期间工作树出现了其他 worker 的 root manifest、lockfile、desktop 与
provider-adapters 并发变更。本 Worker 没有覆盖、回退、暂存或归并这些改动；边界审计
只针对上表四个路径。没有执行 git 写操作。

## 10. 残留风险

1. **目录不等于可用登录。** xAI、GitHub Copilot、OpenAI 因可信 client ID 缺失而默认
   fail closed；只有用户补齐自己有权使用的 client 后转换才返回参数。
2. **端点来源层级。** xAI/GitHub/OpenAI 的已知字段来自固定 OpenCode commit，不是本单
   在线访问厂商文档所得；上线前总控仍应做厂商官方文档/注册控制台复核。
3. **账号标识尚未执行。** profile 记录 OpenAI claim 策略，但本单不实现 JWT claim
   提取或验证；xAI、GitHub Copilot、Anthropic 继续明确 `Unknown`。
4. **OpenAI 专有 flow。** 通用 PKCE 参数不能证明 consumer backend 可用；额外 authorize
   参数、专有 API endpoint、header rewrite 与 model allowlist 均未实现。
5. **GitHub Enterprise。** 当前固定 profile 只覆盖 GitHub.com。Enterprise 域名、issuer
   与 endpoints 必须另行设计并从管理员/官方配置取得，不能字符串拼接后默认信任。
6. **未来 UI/host wiring。** 必须在 UI 下方继续执行 issuer/authorize/token endpoint
   校验，并保持 client ID 来源标签；不能把 `Unknown` 渲染成可直接登录。

---

`PROVIDER-REGISTRY-COMPLETE`
