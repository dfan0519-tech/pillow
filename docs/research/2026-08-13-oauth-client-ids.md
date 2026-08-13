# 四家供应商 OAuth：不注册自己的应用，能不能用？

> 日期：2026-08-13
> 调研槽位：grok4（grok-4.6）
> 总控：Claude
> 状态：完成
> 性质：只读调研。除本报告外未修改任何文件；未运行门、测试或 E2E；无 git 写操作。
> 完成信号：`OAUTH-RESEARCH-COMPLETE`

## 问题

Pillow 槽位目录里，xAI / GitHub Copilot / Anthropic / OpenAI 的 `ClientIdSource` 都是 `Unknown`。UI 对 `Unknown` 不展示 OAuth，所以这四家的 OAuth 目前全部不可用。

`Unknown` 的原意是「没有取得可复用的第三方出处证明」，不是「不存在」。本报告回答：

1. 有没有 RFC 7591 动态客户端注册（最干净）？
2. 有没有供应商官方文档明确允许任意第三方使用的 `client_id`？
3. 只能自己注册时，成本是什么？
4. 这家的 OAuth 相对 API key 到底多了什么？值不值得做？

Custom OIDC 本来就是 `UserSupplied`，不在本题范围内。

## 证据口径

两列分开写，不混：

- **本次实际访问**：对本机发出的 HTTP 响应。discovery JSON 只根据响应体判断有没有 `registration_endpoint`，不靠记忆。
- **官方文档**：供应商自己的文档站 / 官方仓库用户指南。开源项目源码里出现的 `client_id`、社区博客、论坛、逆向，一律不算「官方公开 client_id」。

判定「官方公开 client_id」的标准按调研单执行：供应商官方文档必须写明某个 client_id 供任意 CLI / 第三方工具使用。看不见官方出处就维持 `Unknown`。

本报告不收录任何 client_id、token、key 或其片段。

## 总表

| 供应商 | 动态注册? | 官方公开 client_id? | 需自行注册? | OAuth 相对 API key 的增量 | 建议 `ClientIdSource` |
| --- | --- | --- | --- | --- | --- |
| xAI | **否**。已访问的 discovery **没有** `registration_endpoint` | **否**。官方文档未公布可供第三方复用的 id | **未找到**面向第三方的 OAuth 应用注册台。官方给第三方的入口是 API key；订阅登录走官方 CLI / Pillow 已有的 `grok_login` | 浏览器/设备码登录走推理代理，和按 token 计费的 API key 不是同一条路。这条路 Pillow **已经**用 `grok_login` 接了 | 维持 `Unknown` |
| GitHub Copilot | **否**。已访问的 discovery **没有** `registration_endpoint` | **否**。官方要求第三方**自己**建 OAuth / GitHub App | **是**。个人 GitHub 账号即可，无需企业资质或付费审核 | 用户 Copilot 订阅额度。细粒度 PAT 官方也认，所以增量主要是设备码登录体验和「Pillow 自己的应用身份」，不是另一条计费通道 | 改为 `UserSupplied` |
| Anthropic | **未发现**可用的 OIDC discovery；已探的 well-known 不是 404 就是 TLS 失败 | **否**。官方明确禁止第三方提供 Claude.ai 登录或转发订阅凭据 | **否**。没有面向第三方的「注册一个能走 Claude 订阅 OAuth 的应用」产品面 | 订阅额度相对 API 按量计费是真实增量，但官方不许第三方拿 | 维持 `Unknown`，并保持 `ApiKeyOnly` |
| OpenAI | **否**。已访问的 discovery **没有** `registration_endpoint` | **否**。官方 Codex 文档只描述官方客户端「用 ChatGPT 登录」，未给出第三方可用 id | **未找到**「注册一个能 Sign in with ChatGPT / Codex 的客户端」后台 | ChatGPT 订阅额度相对 Platform API 按量计费是真实增量，但没有官方第三方拿法 | 维持 `Unknown` |

一句话：四家都没有 RFC 7591 动态注册。没有任何一家满足 `PublicDocumented`。只有 GitHub 官方写明「第三方自己注册应用」。Anthropic 官方写明第三方不要做 OAuth。xAI 和 OpenAI 的订阅登录各自锁在官方客户端上。

---

## 1. xAI

### 1.1 Discovery（本次实际访问）

| 项 | 结果 |
| --- | --- |
| 试过的 URL | `https://auth.x.ai/.well-known/openid-configuration` → **200**，`application/json` |
| issuer | `https://auth.x.ai` |
| `registration_endpoint` | **键不存在** |
| `grant_types_supported` | `authorization_code`、`refresh_token`、`urn:ietf:params:oauth:grant-type:device_code` |
| `device_authorization_endpoint` | `https://auth.x.ai/oauth2/device/code` |
| `authorization_endpoint` | `https://auth.x.ai/oauth2/authorize` |
| `token_endpoint` | `https://auth.x.ai/oauth2/token` |
| `token_endpoint_auth_methods_supported` | `client_secret_basic`、`client_secret_post`、`none`（允许 public client 形态，**仍需要一个已登记的 client_id**） |
| `code_challenge_methods_supported` | `S256` |
| `scopes_supported` | `openid`、`profile`、`email`、`offline_access`、`grok-cli:access`、`team:read`、`org:read`、`api:access`、`grok-plugins:access`、`conversations:read`、`conversations:write`、`workspaces:read`、`workspaces:write` |

同主机 `https://auth.x.ai/.well-known/oauth-authorization-server` → **404**（空 body）。RFC 8414 那份 metadata 不在这里。

其它试过、失败的 URL：

- `https://accounts.x.ai/.well-known/openid-configuration`：连接被中止
- `https://api.x.ai/.well-known/openid-configuration`：404

动态注册结论：**没有。** 没有 `registration_endpoint`，也就谈不上 initial access token。设备码 grant 在 discovery 里是明文支持的。

Pillow 目录里的 device / token URL 与本次 discovery **一致**。目录里 `issuer` 仍是 `Unknown`；本次已能从活文档读到 `https://auth.x.ai`。这是目录可补的事实，不是 client_id 出处。

### 1.2 官方公开 client_id

**没有。** 查过并打开的官方材料：

- [Grok Build 概览](https://docs.x.ai/build/overview)：首次启动打开浏览器登录；无浏览器环境用 `XAI_API_KEY`。
- [企业部署 / 认证](https://docs.x.ai/build/enterprise)：四种方法是浏览器 OIDC、`grok login --device-auth`、外部 `auth_provider_command`、API key。`auth.x.ai` 被列为 OAuth2/OIDC 主机。
- 同页「Enterprise OIDC」是**反方向**：Grok 作为 RP，去连客户自己的 Entra / Okta / Auth0。`client_id = "your-client-id"` 是客户 IdP 上的客户端，不是 xAI 发给第三方的 id。
- [官方仓库用户指南 `02-authentication.md`](https://github.com/xai-org/grok-build/blob/main/crates/codegen/xai-grok-pager/docs/user-guide/02-authentication.md)：同样是「在你的 IdP 注册一个 public client」，不是「这是 xAI 的公共 client_id」。
- [API Quickstart](https://docs.x.ai/developers/quickstart)：第三方调模型的入口是 [console 里生成 API key](https://console.x.ai/team/default/api-keys)。

官方文档没有写出一个可供任意第三方 CLI 使用的 xAI `client_id`。开源工具里能看到的值，按调研单不算数。

### 1.3 自行注册成本

**未找到**「到某后台注册一个 xAI OAuth 应用、拿到自己的 client_id」的官方产品面。

- API 控制台是 [console.x.ai](https://console.x.ai)，公开文档只讲 API key。
- 企业 OIDC 要注册的是**客户自己的 IdP 应用**，不是 xAI 授权服务器上的客户端。
- 未见到企业资质 / 付费审核 / redirect URI 白名单的 OAuth 应用表单，因为没见到这张表单本身。

设备码：discovery 和官方企业文档都支持 RFC 8628 设备码。官方 CLI 用 `--device-auth`。这只说明**官方客户端**能走设备码，不说明第三方可以免注册复用官方客户端。

### 1.4 OAuth 相对 API key 值不值得

**对 Pillow 来说，不值得再做一条通用 OAuth。**

官方把两条路分开写了：

- 交互登录：浏览器或设备码，流量走 `cli-chat-proxy.grok.com`（企业文档「Required」表）。
- API key：`XAI_API_KEY` / `model.api_key`，走 `api.x.ai`，按 [API 定价](https://docs.x.ai/developers/pricing) 计费。

这是真实增量：会话登录和按 token 计费的 API key 不是同一条通道。

但 Pillow **已经**用 `grok_login` 接了官方登录，不必为了同一条增量再借一个没有出处的 client_id。通用 OAuth 只在「Pillow 自己成为 `auth.x.ai` 上的登记客户端」时才有独立意义；本次没找到登记入口。

### 1.5 未能确认

- xAI 是否在登录后的 console 里藏了 OAuth 应用管理页（未登录控制台，公开文档没有）。
- 浏览器 OAuth 会话是否精确对应 SuperGrok / X Premium 会员额度。非官方材料常这么写；本次打开的官方页只区分「浏览器/设备码登录」和「API key」，没有写出套餐名与额度对照表。
- `token_endpoint_auth_methods_supported` 含 `none` 是否意味着任意字符串都能当 client_id（未做会触发登录的探测；按 OAuth 常规，没有登记的 id 会被拒）。

---

## 2. GitHub Copilot

### 2.1 Discovery（本次实际访问）

GitHub 用户 OAuth 的 discovery **不在**站点根上。

| URL | 本次结果 |
| --- | --- |
| `https://github.com/.well-known/openid-configuration` | 404 `Not Found` |
| `https://github.com/.well-known/oauth-authorization-server` | 404 |
| `https://api.github.com/.well-known/openid-configuration` | 404 |
| `https://api.github.com/.well-known/oauth-authorization-server` | 404 |
| `https://github.com/login/.well-known/openid-configuration` | 404 |
| `https://github.com/login/oauth/.well-known/openid-configuration` | **200** JSON |
| `https://github.com/login/oauth/.well-known/oauth-authorization-server` | 连接中止，未拿到 body |
| `https://token.actions.githubusercontent.com/.well-known/openid-configuration` | 200，但是 **GitHub Actions 工作流身份**，与 Copilot 用户登录无关 |

`https://github.com/login/oauth/.well-known/openid-configuration` 实际内容：

- `issuer`: `https://github.com/login/oauth`
- `authorization_endpoint`: `https://github.com/login/oauth/authorize`
- `token_endpoint`: `https://github.com/login/oauth/access_token`
- `scopes_supported`: 只有 `openid`
- **没有** `registration_endpoint`
- **没有** `grant_types_supported`
- **没有** `device_authorization_endpoint`

动态注册结论：**没有。** 设备码也不在这份 discovery 里；设备码是官方 OAuth 文档另写的，见下一节。

Pillow 目录里的设备码 / token URL（`https://github.com/login/device/code`、`https://github.com/login/oauth/access_token`）与官方设备码文档一致，但 **OIDC discovery 并未宣告设备码**。

### 2.2 官方公开 client_id

**没有。** 官方对第三方的说法是「你自己建一个应用」。

- [Copilot SDK · GitHub OAuth setup](https://docs.github.com/en/copilot/how-tos/copilot-sdk/setup/github-oauth)：「You create a GitHub OAuth App (or GitHub App), users authorize it, and you pass their access token to the SDK。」注册入口写的是 Settings → Developer Settings → OAuth Apps → New OAuth App。
- [Authorizing OAuth apps](https://docs.github.com/en/apps/oauth-apps/building-oauth-apps/authorizing-oauth-apps)：`client_id` 是「you received from GitHub when you registered」。设备码同样要传「your app's client ID」；设备码**不需要** client secret。
- 官方没有任何一页写出「任意第三方请使用某某 client_id」。VS Code / `gh` / Copilot CLI 源码里的 id 标识的是**那些应用**，看见 ≠ 允许 Pillow 复用。

### 2.3 自行注册成本

**低，而且是官方指定路径。**

| 项 | 官方说法 |
| --- | --- |
| 在哪注册 | [创建 OAuth app](https://docs.github.com/en/apps/oauth-apps/building-oauth-apps/creating-an-oauth-app)：个人账号或有管理权的组织。入口 `https://github.com/settings/applications/new`（文档：「GitHub Settings → Developer Settings → OAuth Apps → New OAuth App」） |
| 资质 / 付费 / 审核 | 文档未要求企业资质或付费。个人账号即可创建 |
| 表单字段 | Application name、Homepage URL、Authorization callback URL；可选 Enable Device Flow |
| redirect URI | OAuth App **必须**填一个 callback URL，且不能填多个。仅走设备码时，官方仍要求创建时填写该字段；设备码交换本身不走这个回调 |
| 设备码开关 | 必须在应用设置里 **Enable Device Flow**，否则返回 `device_flow_disabled`。[Authorizing OAuth apps · Device flow](https://docs.github.com/en/apps/oauth-apps/building-oauth-apps/authorizing-oauth-apps#device-flow) |
| 内部使用限制 | 未看到「仅限内部」。组织可开 [OAuth App access restrictions](https://docs.github.com/en/organizations/managing-oauth-access-to-your-organizations-data/about-oauth-app-access-restrictions)，那是组织策略，不是注册门槛 |
| GitHub App 替代 | Copilot SDK 文档说 GitHub App 与 OAuth App 都行；新项目更推荐 GitHub App |

官方 Copilot 设备码登录（**官方 CLI 自己的应用**，不是 Pillow 的）：[Authenticating GitHub Copilot CLI](https://docs.github.com/en/copilot/how-tos/copilot-cli/set-up-copilot-cli/authenticate-copilot-cli)。第三方要同等流程，必须登记自己的应用并打开 Device Flow。

### 2.4 OAuth 相对 API key 值不值得

Copilot **没有** OpenAI / xAI 那种「模型 API key」。官方认的凭据是：

| 前缀 | 来源 | 官方是否用于 Copilot |
| --- | --- | --- |
| `gho_` | OAuth 用户 token | 是 |
| `ghu_` | GitHub App 用户 token | 是 |
| `github_pat_` | 细粒度 PAT（个人账号 + **Copilot Requests** 权限） | 是 |
| `ghp_` | 经典 PAT | **否**（已弃用 / CLI 不支持） |

出处：[Copilot SDK OAuth setup · Supported token types](https://docs.github.com/en/copilot/how-tos/copilot-sdk/setup/github-oauth#supported-token-types)、[Copilot CLI 认证](https://docs.github.com/en/copilot/how-tos/copilot-cli/set-up-copilot-cli/authenticate-copilot-cli)。

因此：

- OAuth 和合格 PAT **走的是同一条 Copilot 订阅额度**，不是另一条按 token 计费的 API。
- Pillow 现在把 Copilot 的 `api_key` 当成「粘贴一个 token」。若用户粘贴带 Copilot Requests 的细粒度 PAT，订阅通道已经在，不必等 OAuth。
- OAuth 的增量是：设备码登录、refresh、用户在 GitHub 设置里按「Pillow」撤销、token 标明是 Pillow 而不是个人 PAT。

**值得做，但前提是 Pillow 自己登记应用，`ClientIdSource` 改为 `UserSupplied`。** 不值得、也不允许去复用别人的 client_id。

### 2.5 未能确认

- `login/oauth` 的 RFC 8414 `oauth-authorization-server` 本次连接中止，未读到第二份 metadata。openid-configuration 已足够否定 DCR。
- 未实际点进「New OAuth App」看 2026-08 的表单是否新增必填项。
- 未验证「只填占位 callback、只开 Device Flow」的应用，GitHub 是否会在审核或策略上另加限制。
- Copilot 企业策略是否会拦未验证的第三方 OAuth App（组织限制存在；Pillow 用户侧能否过，未测）。

---

## 3. Anthropic

### 3.1 Discovery（本次实际访问）

没有拿到一份可用的 OIDC discovery JSON。

| URL | 本次结果 |
| --- | --- |
| `https://claude.ai/.well-known/openid-configuration` | 404 `Not Found` |
| `https://claude.ai/.well-known/oauth-authorization-server` | 404 |
| `https://claude.com/.well-known/openid-configuration` | 404 `The page could not be found` |
| `https://auth.anthropic.com/.well-known/openid-configuration` | TLS/handshake 失败，无 body |
| `https://auth.anthropic.com/.well-known/oauth-authorization-server` | TLS 失败 / 随后落到 HTML 应用壳，不是 metadata JSON |
| `https://login.anthropic.com/.well-known/openid-configuration` | TLS 失败，无 body |
| `https://console.anthropic.com/.well-known/openid-configuration` | 301 后 404 |
| `https://console.anthropic.com/.well-known/oauth-authorization-server` | 301 后 404 |
| `https://platform.claude.com/.well-known/openid-configuration` | 404（curl）；IWR 连接中止 |
| `https://platform.claude.com/.well-known/oauth-authorization-server` | 404 |
| `https://api.anthropic.com/.well-known/openid-configuration` | 404 |
| `https://api.anthropic.com/.well-known/oauth-authorization-server` | 404 |

动态注册结论：**就公开 well-known 而言，不存在。** 不能从 discovery 读 `grant_types`，也不能判断有没有 initial access token。官方客户端登录流程不在本题的公开 discovery 里。

### 3.2 官方公开 client_id

**没有。官方不但没给，而且禁止第三方走这条路。**

[Claude Code · Legal and compliance · Authentication and credential use](https://code.claude.com/docs/en/legal-and-compliance) 原文要点：

- OAuth 只给 Claude Free / Pro / Max / Team / Enterprise 订阅，用于 Claude Code 和 **其它原生 Anthropic 应用**。
- 做产品或服务、包括用 Agent SDK 的开发者，应通过 [Claude Console](https://platform.claude.com/) 或云厂商用 **API key**。
- 「Anthropic does not permit third-party developers to offer Claude.ai login or to route requests through Free, Pro, or Max plan credentials on behalf of their users.」
- 保留不经事先通知的执行权。

这不是「没写所以也许可以」。这是明确的否。

配套官方页：

- [Claude Code 认证](https://code.claude.com/docs/en/iam) / [同文在 docs.anthropic.com](https://docs.anthropic.com/en/docs/claude-code/iam)：浏览器 `/login`、`claude setup-token`、Console、云厂商。全部是官方客户端自己的登录。
- [Claude API overview](https://platform.claude.com/docs/en/api/overview)：第三方调 API 用 Console 的 `x-api-key`，或 WIF 换来的短时 `Authorization: Bearer`。
- [Platform Authentication](https://platform.claude.com/docs/en/manage-claude/authentication)：API key 与 Workload Identity Federation **对 Claude API 端点权限相同**。WIF 是企业工作负载联合身份，不是「给 Pillow 一个消费订阅 OAuth client」。

社区里能搜到的 Claude Code client_id **不算数**，而且按上面条款去用会直接违规。

### 3.3 自行注册成本

**没有这条路。**

未找到「注册一个能做 Claude.ai / Claude Code 订阅 OAuth 的第三方应用」的后台。Console 发的是 API key（[Get your API key](https://platform.claude.com/docs/en/get-api-key)、[Settings → API keys](https://platform.claude.com/settings/keys)）。

平台侧另有的 OAuth 字样：

- MCP 服务器上的动态注册 / CIMD：那是 **Claude Code 当客户端**，去连用户的 MCP 授权服务器，方向反了。[Claude Code MCP](https://docs.anthropic.com/en/docs/claude-code/mcp)
- `POST /v1/oauth/token` + WIF：企业 IdP 联合，换 API 访问，不是订阅登录。[WIF](https://platform.claude.com/docs/en/manage-claude/workload-identity-federation)

redirect URI、设备码、内部使用限制：没有第三方订阅 OAuth 产品面，这些问题不适用。

### 3.4 OAuth 相对 API key 值不值得

**增量存在，但不许拿，所以对 Pillow 不值得做 OAuth。**

- 增量：Claude Pro / Max / Team 订阅额度 vs Console API 按量计费。官方自己也把两条路分开（Claude Code 文档：订阅登录 vs Console / API key）。
- 官方给第三方的答案是 API key（或 Bedrock / Vertex / Foundry / WIF）。
- Pillow 现状 `login_method = ApiKeyOnly` 与官方条款一致，应保持。

即便以后有人把 `ClientIdSource` 改成 `UserSupplied`，让用户粘贴某个 client_id，也仍然是在「提供 Claude.ai 登录 / 转发订阅凭据」。官方禁止的是这种**行为**，不是「只有硬编码才算」。

### 3.5 未能确认

- `auth.anthropic.com` / `login.anthropic.com` 的 TLS 失败是本机/出口问题还是对方拒绝非浏览器握手。本次**没有**读到这两台主机的 discovery JSON，不能声称它们「一定没有」`registration_endpoint`；只能说公开、可访问的 well-known 里没有。
- 未登录 Claude Console / claude.ai，所以看不到登录后才出现的隐藏注册页。公开文档与合规页已经把第三方 OAuth 否决，即使有隐藏页也不改变产品建议。
- 未核 Team / Enterprise 合同是否另开官方合作客户端计划（合规页说疑问去联系销售）。

---

## 4. OpenAI

### 4.1 Discovery（本次实际访问）

| URL | 本次结果 |
| --- | --- |
| `https://auth.openai.com/.well-known/openid-configuration` | **200** JSON |
| `https://auth.openai.com/.well-known/oauth-authorization-server` | 200，但 `Content-Type: text/html`，是登录 SPA，**不是** RFC 8414 JSON |
| `https://auth0.openai.com/.well-known/openid-configuration` | 404 |
| `https://chatgpt.com/.well-known/openid-configuration` | 404 HTML |
| `https://platform.openai.com/.well-known/openid-configuration` | 200 HTML 应用壳，不是 metadata JSON |

`https://auth.openai.com/.well-known/openid-configuration` 实际内容：

- `issuer`: `https://auth.openai.com`
- `authorization_endpoint`: `https://auth.openai.com/api/accounts/authorize`
- `token_endpoint`: `https://auth.openai.com/api/accounts/oauth/token`
- `grant_types_supported`: `authorization_code`、`refresh_token`
- **没有** `urn:ietf:params:oauth:grant-type:device_code`
- **没有** `device_authorization_endpoint`
- **没有** `registration_endpoint`
- `scopes_supported`: `openid`、`profile`、`email`、`offline_access`
- `code_challenge_methods_supported`: `S256`
- `token_endpoint_auth_methods_supported`: `client_secret_basic`、`client_secret_post`、`none`

动态注册结论：**没有。** 设备码 grant 也没有。官方 Codex 登录是浏览器授权码 + PKCE。

目录对照（只记差异，不改代码）：

| 字段 | Pillow 目录 | 本次 discovery |
| --- | --- | --- |
| issuer | `https://auth.openai.com` | 相同 |
| authorize | `https://auth.openai.com/oauth/authorize` | `https://auth.openai.com/api/accounts/authorize` |
| token | `https://auth.openai.com/oauth/token` | `https://auth.openai.com/api/accounts/oauth/token` |

authorize / token 路径与活 discovery **不一致**。本报告不判断旧路径是否仍 302；若以后要接 OAuth，应以当时的 discovery 为准。

### 4.2 官方公开 client_id

**没有。**

打开过的官方认证文档：

- [Codex / ChatGPT Learn · Authentication](https://developers.openai.com/codex/auth)（抓取落在 `https://learn.chatgpt.com/docs/auth`）：两种登录是「Sign in with ChatGPT」（订阅 / 工作区）和「Sign in with an API key」（按量）。对象是 ChatGPT 桌面端、Codex CLI、IDE 扩展、Codex cloud。没有「第三方应用请使用此 client_id」。
- 同页企业「Codex access tokens」是工作区管理员给受信自动化发的令牌，不是公开 client_id。
- API key 来自 [OpenAI dashboard](https://platform.openai.com/api-keys)，按 [API pricing](https://openai.com/api/pricing/) 计费。

方向相反、容易看错的官方 OAuth：

- [Plugins / Apps 认证](https://developers.openai.com/plugins/build/auth)、GPT Actions：ChatGPT / Codex 当 **客户端**，去连**你的**授权服务器。ChatGPT 用 CIMD 或调用**你的** `registration_endpoint`。这是「给 ChatGPT 用你的应用」，不是「Pillow 用 ChatGPT 订阅调模型」。

开发者论坛上有人问 Codex OAuth 该用哪个 ClientID、也有人请求独立 public client 登记。那是社区帖，不是官方许可。按调研单不算数。

### 4.3 自行注册成本

**未找到**「注册一个能 Sign in with ChatGPT、拿到 Codex / 订阅额度的 OAuth 客户端」的官方后台。

- Platform 控制台发的是 API key。
- Apps SDK / GPT Actions / MCP 要填的 client_id 是**你作为授权服务器**发给 ChatGPT 的，不是 auth.openai.com 发给你的。
- 因此：企业资质、redirect URI 限制、仅限内部——在「第三方 ChatGPT 登录客户端」这条线上都不适用，因为这条产品线没出现。

设备码：discovery 不支持；官方客户端用浏览器回调。第三方即便有 client_id，目录里的 DeviceCode 也不适用于 OpenAI（目录已是 PKCE）。

### 4.4 OAuth 相对 API key 值不值得

**增量是真的，官方拿法不属于第三方产品。**

官方自己写明：

- ChatGPT 登录：走 ChatGPT 工作区权限、RBAC、套餐额度；Codex cloud 只能用 ChatGPT 登录。
- API key：走 Platform 组织的保留与数据设置，按 API 价计费；部分依赖 ChatGPT 工作区的功能不可用。

对 Pillow：

- 作为第三方 IDE，官方允许的是 API key。
- 把 Codex CLI 的 client_id 嵌进 Pillow，是复用另一个应用的身份，官方文档未授权。
- 把 `ClientIdSource` 改成 `UserSupplied` 会诱导用户去粘贴那个没有出处的 id，和「不发明 client_id」的现策略相悖。

**结论：维持 `Unknown`，用 API key。不要为了订阅额度去做没有官方许可的 OAuth。**

### 4.5 未能确认

- `https://auth.openai.com/oauth/authorize` 旧路径现在是 404、301 还是仍可用（未跟登录流量）。
- 是否存在未编入公开文档、需商务开通的「第三方 ChatGPT 登录」合作项目。公开开发者文档没有。
- discovery 里的 `none` 是否接受未登记 client_id（未做会触发授权页的探测）。
- ChatGPT Enterprise access token 能否被非 Codex 官方客户端使用（文档写的是 Codex 本地工作流）。

---

## 建议

### 可以立刻改 `ClientIdSource` 的

**只有 GitHub Copilot：`Unknown` → `UserSupplied`。**

依据是 GitHub 自己的文档，不是社区源码：

1. 没有 DCR（已访问的 discovery 无 `registration_endpoint`）。
2. 没有官方公共 client_id。
3. 官方明确要求第三方创建自己的 OAuth App / GitHub App，再把用户 token 交给 Copilot SDK。
4. 个人账号即可注册，设备码是官方支持的、且必须在应用设置里打开。

改成 `UserSupplied` 之后，UI 才会显示 OAuth，并要求维护者或用户填**自己的** client_id。不要把任何从别的项目抄来的 id 写进目录。

### 确认了就是要自己注册、但今天还注册不了的

- **xAI**：授权服务器活着，支持授权码 + 设备码，但公开材料里没有「给第三方发 client_id」的登记台。在 xAI 明确提供应用登记之前，不能从 `Unknown` 改走。
- **OpenAI**：授权服务器活着，支持授权码 + PKCE，但公开材料里没有「给第三方发 ChatGPT/Codex client_id」的登记台。同样维持 `Unknown`。

这两家若未来出现官方登记台，正确落点是 `UserSupplied`（用户填自己登记的 id）或——仅当官方文档写出「任意第三方使用此 id」——`PublicDocumented`。现在两格都不成立。

### 其实不需要（或不应该）做 OAuth 的

- **Anthropic**：官方要求第三方用 API key，并禁止代做 Claude.ai / 订阅登录。保持 `ApiKeyOnly` + `Unknown`。
- **xAI**：订阅/会话登录的增量已经由 `grok_login` 覆盖。再做通用 OAuth 不会多出新能力，除非目标改成「Pillow 自有 xAI 客户端」。
- **OpenAI**：API key 是官方第三方路径。OAuth 的订阅增量官方没有授权给第三方产品。

### 四家都还不能改成 `PublicDocumented`

本次没有一家的官方文档写出「这个 client_id 供任意第三方使用」。维持三家 `Unknown`、只把 GitHub 升到 `UserSupplied`，是对原 `Unknown` 含义的确认加一条有出处的修正，不是倒退。

### 与目录相关、但不在本单修改范围的观察

1. xAI 的 issuer 已能从活 discovery 读到 `https://auth.x.ai`。
2. OpenAI 目录里的 authorize / token 路径与 2026-08-13 的 discovery 不一致。
3. GitHub 的 OIDC discovery 不宣告设备码；设备码以 OAuth 文档为准。
4. 把 GitHub 改成 `UserSupplied` 是产品改动，需要另开实现单；本调研不改代码。

---

## 访问过的活 URL 清单

只列本次实际请求过、且对结论有用的：

| 主机 | 路径 | 结果 |
| --- | --- | --- |
| auth.x.ai | `/.well-known/openid-configuration` | 200 JSON，无 `registration_endpoint`，有 device_code |
| auth.x.ai | `/.well-known/oauth-authorization-server` | 404 |
| auth.openai.com | `/.well-known/openid-configuration` | 200 JSON，无 `registration_endpoint`，无 device_code |
| auth.openai.com | `/.well-known/oauth-authorization-server` | 200 HTML，非 metadata |
| github.com/login/oauth | `/.well-known/openid-configuration` | 200 JSON，无 `registration_endpoint`，无 device 字段 |
| github.com | `/.well-known/openid-configuration` | 404 |
| token.actions.githubusercontent.com | `/.well-known/openid-configuration` | 200，Actions OIDC，无关 |
| claude.ai / claude.com / api.anthropic.com / platform.claude.com | 若干 well-known | 404 或非 JSON |
| auth.anthropic.com / login.anthropic.com | well-known | TLS 失败，未读到 JSON |

官方文档 URL 见各节正文，均可按标题回查。

`OAUTH-RESEARCH-COMPLETE`
