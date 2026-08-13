# Pillow — 槽位新增 / 编辑流程重做（对齐 OpenCode 加供应商）

日期：2026-08-13  
范围：新建与编辑共用一套两步流程。槽位仍可共存、同时工作。

完成信号：`SLOT-FLOW-COMPLETE`

## 退出码表

下列命令均实际跑过。未跑 `scripts/run-worker-gate.mjs`。

| 命令 | 日志 | EXIT |
| --- | --- | --- |
| `corepack pnpm --filter @pillow/desktop typecheck` | `.superpowers/round48/log1.txt` | 0 |
| `corepack pnpm --filter @pillow/desktop test` | `.superpowers/round48/log2.txt` | 0（55 files / 721 tests） |
| `node scripts/run-cargo.mjs test -p pillow-runtime-slots` | `.superpowers/round48/log3.txt` | 0 |
| `node scripts/run-cargo.mjs fmt --all -- --check` | `.superpowers/round48/log-fmt.txt` | 0 |

先红后绿对照日志：`.superpowers/round48/log-red.txt`（旧代码，3 failed）。

## 先红后绿

文件：`apps/desktop/tests/runtime-slots-connect-flow.test.tsx`

| 测试名 | 旧代码上的失败 |
| --- | --- |
| `selecting OpenAI (Unknown client_id) does not show OAuth 登录` | `expected <button> to be null`；收到 `data-testid="runtime-slots-auth-oauth"`、文案「OAuth 登录」 |
| `selecting xAI (Unknown client_id) does not show OAuth 登录` | 同上：四个并列 tab 里已经画出 OAuth |
| `selecting 自定义端点 reveals base_url` | 找不到 `runtime-slots-provider-pick-custom`（旧流程没有第一步供应商） |

旧契约错在哪：

- `oauthCapableProviders` 只排除 `api_key_only`，把 `clientId === "unknown"` 的 xAI / OpenAI / GitHub Copilot 当成可 OAuth。
- `oauth-login-section.test.tsx` 曾断言「选 OpenAI 后必须出现 client_id 必填框」。那是把走不通的路当成产品行为。
- `SLOT_AUTH_METHODS` 被画成四个全局 tab，认证方式不跟供应商走。

改完后上述三条转绿；同文件还钉了「编辑走同一套 connect 步、预填、认证锁定」。

目录侧同步：`provider_login.rs` 的 `profile_offers_oauth` / `profile_supported_auth_kinds`，以及 `oauthLoginUi.ts` 的 `catalogEntryOffersOauth`。规则一致：`Unknown` 不提供 OAuth；`UserSupplied` / `PublicDocumented` 才提供。

**没有加 host DTO 字段。** 渲染层继续用闭合白名单里已有的 `clientId` + `loginMethod` 推导。测试钉死 mock 行没有 `offersOauth` / `supportedAuthKinds` 这类新 key。

## 新旧流程对照

| | 旧 | 新 |
| --- | --- | --- |
| 第一步 | 无。打开「新建槽位」即四个认证 tab：API key / 已登录的 Grok / OAuth 登录 / 本机 CLI | 选供应商（具名 + OpenAI 兼容预设 + 自定义端点 + 自定义 OIDC + 本机 CLI） |
| 第二步 | 与 tab 无关的字段堆在同一屏。OAuth tab 摊开 client_id / issuer / base_url / 模型 id | 只显示该供应商真正支持的认证方式；OAuth 只出现在目录允许的供应商上 |
| OAuth 屏 | 再选一次提供方；Unknown 行还要用户填 client_id | 供应商已在第一步选定。当前只有「自定义 OIDC」进入 OAuth；只问该方法声明的 prompts |
| base_url / 模型 id | 混在 OAuth 屏上 | 属于 API key / 自定义端点路径。自定义 OIDC 另有「调用端点」两栏（见残留风险） |
| 已登录的 Grok / 本机 CLI | 四个 tab 之一，打开就能点 | 第一步之后才出现：Grok 在 xAI 下；本机 CLI 是单独一项 |

多个槽位仍各自独立：列表、启动、角色、治理推导未改。新建不会替换已有槽位。

## 各供应商认证方式

由目录 `clientId` 来源 + 供应商 id 推出（与 `profile_supported_auth_kinds` 一致）：

| 第一步项 | 认证方式 | OAuth 是否出现 |
| --- | --- | --- |
| xAI | API key、已登录的 Grok | 否（`clientId` Unknown） |
| OpenAI | API key | 否（Unknown） |
| Anthropic | API key | 否（`api_key_only`） |
| GitHub Copilot | API key | 否（Unknown） |
| 硅基流动 / Moonshot / DeepSeek / 智谱 / 通义 / MiniMax / OpenRouter / Groq / Together / OpenCode Zen | API key（预填 base_url） | 否 |
| 自定义 OpenAI 兼容端点 | API key（手填 base_url / 模型 id） | 否 |
| 自定义 OIDC | OAuth（user_supplied：client_id、issuer、authorize、token、scopes、账号标识） | 是 |
| 本机 CLI | 本机 CLI | 否 |

日后某具名供应商的 `clientId` 改为 `PublicDocumented` 或 `UserSupplied` 时，OAuth 会从目录自动出现，无需再给 UI 加第四个全局 tab。

## 编辑路径如何复用

同一套 `RuntimeSlotsSection` 状态机，没有第二份编辑表单。

- `formMode`: `create` \| `edit`
- `formStep`: `provider` \| `connect`
- 新建：从 `provider` 开始，点供应商进入 `connect`
- 编辑（仍仅 API-key 槽）：直接 `connect`，`inferConnectProviderId` 按 base_url / authMode 预填供应商
- 编辑锁定：供应商与认证方式不可改；名称、base_url、模型 id、角色可改；API key 留空 = 保持「已配置」，不回显

## 残留风险

1. **Host `create_slot` oauth 仍要求 base_url 与 model_id。** 本单不能改 tauri 命令。自定义 OIDC 的 OAuth 屏因此仍有「调用端点」两栏——那是槽位日后发请求的地址，不是 OIDC 授权字段。具名供应商的 OAuth 屏已拿掉这两栏（它们现在也进不了 OAuth）。
2. **未加 DTO 字段。** 认证方式由已有 `clientId` 推导。若以后要在 host 载荷上显式列出 methods，必须同批扩 `ProviderLoginCatalogEntryDto` 与渲染层闭合白名单，否则整表会被拒、UI 静默消失。
3. **GitHub Copilot 现在露出 API key 表单。** OAuth 被诚实拿掉后，这一项否则会变成死路。Copilot 并不是典型 OpenAI 兼容 key；用户可能填不通。文案已写明 OAuth 暂不可用。
4. **OAuth 槽 / 登录槽仍不能从列表「编辑」。** 与改前相同；只有 API-key 槽有编辑入口。
5. **本环境 `cargo` 需指向用户 rustup home 才能解析 toolchain。** 收口命令在该环境下 EXIT=0。
6. **未在真实桌面窗口点选验证。** 本单禁止 E2E / 门；行为由组件测试覆盖（用户看得见的按钮与字段）。设置窗的 CSS 只加了供应商宫格，未改导航几何。

SLOT-FLOW-COMPLETE
