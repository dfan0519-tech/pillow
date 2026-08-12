/**
 * Pure data module: OpenAI-compatible provider presets for API-key slots.
 *
 * Presets only supply a default `baseUrl` (and optional suggested model / docs).
 * They never change governance, auth kind, or host secret handling.
 *
 * Each non-custom entry documents an authoritative source URL for `baseUrl`
 * in a line comment. Prefer omitting an entry over guessing an endpoint.
 */

export type ProviderPresetId =
  | "siliconflow"
  | "moonshot"
  | "deepseek"
  | "zhipu"
  | "dashscope"
  | "minimax"
  | "openrouter"
  | "groq"
  | "together"
  | "opencode_zen"
  | "custom";

export type ProviderPreset = {
  id: ProviderPresetId;
  /** UI label (zh-CN primary product audience). */
  label: string;
  /**
   * Default OpenAI-compatible base URL.
   * Empty string for the custom fallback (user types the endpoint).
   */
  baseUrl: string;
  /** Where the user can create / copy an API key (or provider home). */
  docsUrl: string;
  /**
   * Optional suggested default model id.
   * Empty when the catalog is large or ids are account-specific —
   * user should pull models or type an id.
   */
  suggestedModelId: string;
  /** Short UI note under the selector. */
  notes: string;
};

/**
 * Ordered catalog for the create-slot provider select.
 * Keep `custom` last as the always-available fallback.
 */
export const PROVIDER_PRESETS: readonly ProviderPreset[] = [
  // Source: https://docs.siliconflow.cn/en/usercases/use-siliconcloud-in-cline
  // (Base Url: https://api.siliconflow.cn/v1). EN quickstart also lists
  // https://api.siliconflow.com/v1 — CN product audience uses .cn.
  {
    id: "siliconflow",
    label: "硅基流动 (SiliconFlow)",
    baseUrl: "https://api.siliconflow.cn/v1",
    docsUrl: "https://cloud.siliconflow.cn/account/ak",
    suggestedModelId: "",
    notes: "模型 id 因账号可见目录而异；建议拉取目录后选择。国际站可用 api.siliconflow.com。",
  },

  // Source: https://platform.kimi.ai/docs/api/overview
  // (SDK base_url: https://api.moonshot.ai/v1). China-region keys may need
  // https://api.moonshot.cn/v1 — user can edit after selecting.
  {
    id: "moonshot",
    label: "Moonshot / Kimi",
    baseUrl: "https://api.moonshot.ai/v1",
    docsUrl: "https://platform.kimi.ai/",
    suggestedModelId: "",
    notes: "中国区密钥若连不上，可改 base_url 为 https://api.moonshot.cn/v1。",
  },

  // Source: https://api-docs.deepseek.com/
  // (base_url OpenAI: https://api.deepseek.com — official table, no /v1 suffix)
  {
    id: "deepseek",
    label: "DeepSeek",
    baseUrl: "https://api.deepseek.com",
    docsUrl: "https://platform.deepseek.com/api_keys",
    suggestedModelId: "deepseek-chat",
    notes: "官方 OpenAI 兼容 base_url 不含 /v1；也可在控制台确认当前模型 id。",
  },

  // Source: https://open.bigmodel.cn/dev/api (OpenAI SDK 兼容)
  // Common OpenAI-compatible base used by Zhipu BigModel / Cline docs:
  // https://open.bigmodel.cn/api/paas/v4
  {
    id: "zhipu",
    label: "智谱 GLM",
    baseUrl: "https://open.bigmodel.cn/api/paas/v4",
    docsUrl: "https://open.bigmodel.cn/usercenter/proj-mgmt/apikeys",
    suggestedModelId: "",
    notes: "请手输或拉取模型 id（如 glm-4 系列）；国际站可能是 api.z.ai。",
  },

  // Source: https://help.aliyun.com/zh/model-studio/compatibility-of-openai-with-dashscope
  // Long-standing mainland compatible-mode endpoint. Official docs also list
  // workspace/region-specific hosts — user may need to replace from console.
  {
    id: "dashscope",
    label: "阿里通义百炼 (DashScope)",
    baseUrl: "https://dashscope.aliyuncs.com/compatible-mode/v1",
    docsUrl: "https://bailian.console.aliyun.com/?tab=model#/api-key",
    suggestedModelId: "",
    notes: "地域/业务空间专属域名以控制台为准；连不上时按文档改 base_url。",
  },

  // Source: https://platform.minimax.io/docs/api-reference/text-openai-api
  // (OPENAI_BASE_URL=https://api.minimax.io/v1)
  {
    id: "minimax",
    label: "MiniMax",
    baseUrl: "https://api.minimax.io/v1",
    docsUrl: "https://platform.minimax.io/user-center/basic-information/interface-key",
    suggestedModelId: "",
    notes: "模型 id 以控制台 /models 为准；国内域名若不同请手改 base_url。",
  },

  // Source: https://openrouter.ai/docs/quickstart
  // (baseURL: 'https://openrouter.ai/api/v1')
  {
    id: "openrouter",
    label: "OpenRouter",
    baseUrl: "https://openrouter.ai/api/v1",
    docsUrl: "https://openrouter.ai/keys",
    suggestedModelId: "",
    notes: "模型 id 形如 vendor/model；请拉取目录或对照 OpenRouter 模型页手输。",
  },

  // Source: https://console.groq.com/docs/openai
  // (base_url: https://api.groq.com/openai/v1)
  {
    id: "groq",
    label: "Groq",
    baseUrl: "https://api.groq.com/openai/v1",
    docsUrl: "https://console.groq.com/keys",
    suggestedModelId: "",
    notes: "模型 id 以 Groq 控制台目录为准。",
  },

  // Source: https://docs.together.ai/docs/inference/openai-compatibility
  // (base_url: https://api.together.ai/v1)
  {
    id: "together",
    label: "Together AI",
    baseUrl: "https://api.together.ai/v1",
    docsUrl: "https://api.together.ai/settings/api-keys",
    suggestedModelId: "",
    notes: "模型 id 通常为 org/model 形式；建议拉取目录。",
  },

  // Source: https://opencode.ai/docs/zen/
  // (models endpoint https://opencode.ai/zen/v1/models → base https://opencode.ai/zen/v1)
  // Also documented by Docker Agent providers: Base URL https://opencode.ai/zen/v1
  {
    id: "opencode_zen",
    label: "OpenCode Zen",
    baseUrl: "https://opencode.ai/zen/v1",
    docsUrl: "https://opencode.ai/auth",
    suggestedModelId: "",
    notes: "OpenAI 兼容；模型 id 以 /models 或 OpenCode 文档为准。",
  },

  // Fallback — empty baseUrl, full manual entry (current behaviour).
  {
    id: "custom",
    label: "自定义端点",
    baseUrl: "",
    docsUrl: "",
    suggestedModelId: "",
    notes: "任意 OpenAI 兼容端点：自行填写 base_url、模型 id 与 API key。",
  },
] as const;

export function getProviderPreset(
  id: ProviderPresetId | string,
): ProviderPreset | undefined {
  return PROVIDER_PRESETS.find((preset) => preset.id === id);
}

/** Default selection when opening the API-key create form. */
export const DEFAULT_PROVIDER_PRESET_ID: ProviderPresetId = "custom";
