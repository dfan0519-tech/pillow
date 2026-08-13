# 决策:Anthropic 暂不进入 Pillow,本机 CLI 一路先只做 Codex

日期:2026-08-13
状态:**已生效**(取代同日 `2026-08-13` 交接手册 §3 中"Claude 和 Codex 都走本机 CLI 槽位"的表述)

## 决定

1. **Anthropic 不作为 Pillow 的供应商出现。** 不是"OAuth 暂时不给",是**整条不在目录里**
   —— 供应商选择器没有这一行,主机登录目录没有这个 profile,API 默认值表里也没有它的
   base_url。
2. **本机 CLI 这条路,现阶段只做 Codex。**

## 为什么

- Anthropic 的条款禁止第三方客户端提供 Claude.ai 登录或转发订阅凭据(调研见
  `docs/research/2026-08-13-oauth-client-ids.md`),所以 OAuth 这条本来就不通。
- 剩下的"只能填 API key"那一行,用户明确撤回:**不想现在把 Anthropic 放进产品**。
  与其留一个我们暂时不打算支持的供应商行,不如整条移除。
- 用户设备上不用自己的账号做 Claude 登录这条约束一直有效;产品里不留入口,
  就不存在"点错了"的可能。

## 具体删了什么

| 位置 | 删除内容 |
| --- | --- |
| `crates/runtime-slots/src/provider_login.rs` | `anthropic` profile 及其 evidence 常量 |
| `apps/desktop/src/features/settings/providerPresets.ts` | `CATALOG_CONNECT_PROVIDERS` 的 Anthropic 行、`CATALOG_API_DEFAULTS.anthropic` |
| `apps/desktop/src/features/settings/oauthLoginUi.ts` | mock 目录里的 anthropic 行 |

🔴 **`CATALOG_API_DEFAULTS.anthropic` 必须一起删。** 它不只是展示数据 ——
`inferConnectProviderId` 会拿存量槽位的 base_url 去反查它。留着的话,一个旧的
`https://api.anthropic.com/v1` 槽位会推断出 `anthropic`,而选择器里已经没有这一行,
`getSlotConnectProvider` 返回 undefined,编辑表单直接空白。删掉之后它落到 `custom`,
base_url 原样保留,可编辑。测试钉住了这条(`provider-presets.test.ts`)。

## 顺带补上的两条测试

移除 Anthropic 会**顺手抹掉两个分支仅有的覆盖**,因为它是目录里唯一的
`api_key_only` 行:

- `catalogEntryOffersOauth` 的 `api_key_only → false`
- 渲染层 DTO 校验的 `api_key_only + knownClientId → 拒绝`

这两条现在由测试**自己构造**的 `api_key_only` 行覆盖,不再依赖某个在售供应商恰好
是这个形态。主机侧同理:`conversion_rejects_api_key_only_method_before_oauth_fields`
改用一个字段齐全的 `ApiKeyOnly` profile —— 旧写法用 Anthropic(所有字段
`NotApplicable`),就算守卫被删掉也会因为缺字段而报错,**红得不是因为它声称的原因**。

四条都做过变异验证:删掉守卫 → 对应测试变红,且只有它变红。

## 没有动的东西(以及为什么)

- **本机 CLI 扫描器仍然会报告机器上装了 `claude`**(`crates/cli-detect`)。
  那是"这台机器上装了什么"的事实陈述,不是 Pillow 提供的供应商;而且本机 CLI 槽位
  本来就允许手填路径,从扫描列表里拿掉只是遮住,挡不住任何事。
  🔴 **如果用户要的是连扫描都不列 claude,一句话就能改。**
- `crates/agent-protocol` 的脱敏/禁用键名单里的 `"anthropic"`、
  `process-executor` 的 `ANTHROPIC` 环境变量清理 —— **防御性名单,必须留着**。
  产品不提供 Anthropic 不代表进程里不会出现 Anthropic 的环境变量。
- `provider-adapters` 的 `ProtocolShape::AnthropicMessages` —— 一个没有任何构造点的
  枚举变体,不可达也不可选,属于协议形状而非供应商目录。

## 要恢复的话

主机 profile 和渲染层选择器**必须同时加回**;两边的一致性由测试钉着,只加一边会红。
`docs/research/2026-08-13-oauth-client-ids.md` 里的条款结论没有过期 ——
恢复的只可能是 API key 那一行,OAuth 这条路依然不通。
