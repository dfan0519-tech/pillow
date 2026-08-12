# Provider presets — execution report

**Status:** complete  
**Worker:** grok1 (write)  
**Scope:** `apps/desktop/src/features/settings/**`, `apps/desktop/tests/**`, this report  
**Forbidden:** `crates/**`, `src-tauri/**`, git write ops, gates/E2E  

## Completion signal

`PROVIDER-PRESETS-COMPLETE`

---

## 1. Exit codes (real runs)

| Command | Exit | Observed |
| --- | ---: | --- |
| `corepack pnpm typecheck` | **0** | `@pillow/desktop` `tsc --noEmit` green |
| `corepack pnpm --filter @pillow/desktop test` | **0** | Vitest **44** files, **553** tests passed (includes new preset/T2/T3 suites) |

Commands were run without piping through `tail`/`head`. Exit codes above are the process `LASTEXITCODE` values from those runs.

---

## 2. Preset table (`baseUrl` + source URL)

Pure data module: `apps/desktop/src/features/settings/providerPresets.ts`.

| id | label | baseUrl | Source URL (authoritative for baseUrl) |
| --- | --- | --- | --- |
| `siliconflow` | 硅基流动 (SiliconFlow) | `https://api.siliconflow.cn/v1` | https://docs.siliconflow.cn/en/usercases/use-siliconcloud-in-cline |
| `moonshot` | Moonshot / Kimi | `https://api.moonshot.ai/v1` | https://platform.kimi.ai/docs/api/overview |
| `deepseek` | DeepSeek | `https://api.deepseek.com` | https://api-docs.deepseek.com/ |
| `zhipu` | 智谱 GLM | `https://open.bigmodel.cn/api/paas/v4` | https://open.bigmodel.cn/dev/api (OpenAI 兼容); also Cline/Z.AI provider docs |
| `dashscope` | 阿里通义百炼 (DashScope) | `https://dashscope.aliyuncs.com/compatible-mode/v1` | https://help.aliyun.com/zh/model-studio/compatibility-of-openai-with-dashscope |
| `minimax` | MiniMax | `https://api.minimax.io/v1` | https://platform.minimax.io/docs/api-reference/text-openai-api |
| `openrouter` | OpenRouter | `https://openrouter.ai/api/v1` | https://openrouter.ai/docs/quickstart |
| `groq` | Groq | `https://api.groq.com/openai/v1` | https://console.groq.com/docs/openai |
| `together` | Together AI | `https://api.together.ai/v1` | https://docs.together.ai/docs/inference/openai-compatibility |
| `opencode_zen` | OpenCode Zen | `https://opencode.ai/zen/v1` | https://opencode.ai/docs/zen/ (`/zen/v1/models`); Docker Agent provider page |
| `custom` | 自定义端点 | *(empty — manual)* | n/a |

Default selection: `custom` (same manual behaviour as before until the user picks a vendor).

---

## 3. T2 / T3 test evidence

### T2 — select preset → auto-fill baseUrl; user edit wins

File: `apps/desktop/tests/runtime-slots-provider-presets-ui.test.tsx`

| Case | Assertion (not “rendered only”) |
| --- | --- |
| Select SiliconFlow | `runtime-slots-base-url` value equals `https://api.siliconflow.cn/v1` |
| Select DeepSeek | baseUrl `https://api.deepseek.com` **and** model id `deepseek-chat` |
| User edits baseUrl then changes name/model/role/key | baseUrl stays `https://user-edited.example.test/openai/v1` (not re-filled from OpenRouter preset) |
| Switch to custom | baseUrl cleared to `""` |
| Create with SiliconFlow preset | `createSlot` args `baseUrl` is preset URL; slot `governance: "full"`, `kind: "governed_grok"` |

Catalog unit pins: `apps/desktop/tests/provider-presets.test.ts` (required ids, https baseUrls, custom last).

### T3 — one-click model list; fail is honest

| Case | Assertion |
| --- | --- |
| Pull success (stubbed `fetch`) | Select lists `preset-model-a` / `preset-model-b`; choosing b sets `runtime-slots-model-id` to `preset-model-b`; request hits `{baseUrl}/models` |
| Pull HTTP 404 | `runtime-slots-draft-models-error` matches 404/手输; **no** success select; manual input still works |
| Button gate | Disabled until both baseUrl and API key non-empty |

Client helper unit tests: `apps/desktop/tests/fetch-openai-compatible-models.test.ts`  
(parse shapes, blank key no network, 401/404/network/parse fail → `ok: false`, never fake models).

Post-create host catalog path (`RuntimeSlotModelPicker` + `get_runtime_slot_model_catalog`) is unchanged and still used on existing list rows.

### Secret boundary

Create-with-preset test reuses the DOM leak sentinel pattern: key never in `textContent` / `innerHTML` / input values after save. Password input; draft cleared before host await (existing create/edit paths).

---

## 4. Write-boundary audit

| Path | Action |
| --- | --- |
| `apps/desktop/src/features/settings/providerPresets.ts` | **new** pure data |
| `apps/desktop/src/features/settings/fetchOpenAiCompatibleModels.ts` | **new** draft `/models` client |
| `apps/desktop/src/features/settings/RuntimeSlotsSection.tsx` | provider select + draft pull UI |
| `apps/desktop/tests/provider-presets.test.ts` | **new** |
| `apps/desktop/tests/fetch-openai-compatible-models.test.ts` | **new** |
| `apps/desktop/tests/runtime-slots-provider-presets-ui.test.tsx` | **new** |
| `docs/execution/2026-08-12-provider-presets-report.md` | this report |

**Not touched:** `crates/**`, `src-tauri/**`, styles outside settings (no CSS file edits), gateway governance parsers, existing governance tests.

Presets affect **only** default `baseUrl` (and optional suggested `modelId` on select). Auth kind, controller rules, and ungoverned CLI copy are unchanged.

---

## 5. Residual risks

1. **Region dual endpoints (documented in UI notes, not dual presets):**
   - SiliconFlow: catalog uses `.cn`; international docs also list `api.siliconflow.com`.
   - Moonshot: catalog uses `api.moonshot.ai`; China keys may need `api.moonshot.cn`.
   - Zhipu: China `open.bigmodel.cn`; international often `api.z.ai`.
   - MiniMax: official international `api.minimax.io`; domestic host variants exist in community reports.
   - DashScope: classic `dashscope.aliyuncs.com/compatible-mode/v1` is catalogued; current Aliyun docs also push **workspace/region-specific** hosts. Notes tell the user to copy from console if needed.

2. **Draft model pull is renderer `fetch`**, not host. In Tauri webview, CORS or network policy may block `/models` even when the host Grok runtime can reach the same URL after save. UI falls back to manual entry and points users at the post-create host catalog. **We never invent a model list on failure.**

3. **No Xiaomi MiMo / other long-tail vendors** were added: no single authoritative OpenAI baseUrl was verified in this pass under the “don’t guess” rule. Users use **自定义端点**.

4. **DeepSeek official baseUrl omits `/v1`** (per api-docs.deepseek.com table). Some OpenAI clients assume `/v1`; if a future runtime is picky, user can append `/v1` manually (preset still matches official docs).

5. **Suggested model ids** are mostly empty (large/vendor-specific catalogs). Only DeepSeek ships a conservative `deepseek-chat` suggestion.

---

## Implementation summary

**T1** — `providerPresets.ts`: closed catalog + `getProviderPreset` + `DEFAULT_PROVIDER_PRESET_ID`.  
**T2** — Create form: supplier `<select>` after API-key auth; selecting a preset fills `baseUrl` (and optional model); field remains free-text; re-application only on another preset pick.  
**T3** — “拉取模型列表” calls `fetchOpenAiCompatibleModels`; success → select; failure → reason + manual input. Existing list-row host picker untouched.
