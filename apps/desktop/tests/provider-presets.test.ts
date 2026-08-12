/**
 * Provider preset catalog — pure data pins.
 * Ensures required vendors exist, custom fallback is last, and baseUrl
 * shapes are stable (no empty non-custom endpoints).
 */
import { describe, expect, it } from "vitest";
import {
  DEFAULT_PROVIDER_PRESET_ID,
  PROVIDER_PRESETS,
  getProviderPreset,
  type ProviderPresetId,
} from "../src/features/settings/providerPresets";

const REQUIRED_IDS: readonly ProviderPresetId[] = [
  "siliconflow",
  "moonshot",
  "deepseek",
  "zhipu",
  "dashscope",
  "minimax",
  "openrouter",
  "groq",
  "together",
  "opencode_zen",
  "custom",
];

describe("PROVIDER_PRESETS catalog", () => {
  it("covers every required vendor id exactly once", () => {
    const ids = PROVIDER_PRESETS.map((p) => p.id);
    for (const required of REQUIRED_IDS) {
      expect(ids).toContain(required);
    }
    expect(new Set(ids).size).toBe(ids.length);
  });

  it("keeps custom last with empty baseUrl (manual fallback)", () => {
    const last = PROVIDER_PRESETS.at(-1);
    expect(last?.id).toBe("custom");
    expect(last?.baseUrl).toBe("");
    expect(DEFAULT_PROVIDER_PRESET_ID).toBe("custom");
  });

  it("gives every non-custom preset a non-empty https baseUrl and docs link", () => {
    for (const preset of PROVIDER_PRESETS) {
      if (preset.id === "custom") continue;
      expect(preset.baseUrl.startsWith("https://"), preset.id).toBe(true);
      expect(preset.docsUrl.startsWith("https://"), preset.id).toBe(true);
      expect(preset.label.trim().length, preset.id).toBeGreaterThan(0);
    }
  });

  it("pins authoritative baseUrls for mainstream vendors", () => {
    // Positive: known-good endpoints from official docs (see providerPresets.ts).
    expect(getProviderPreset("siliconflow")?.baseUrl).toBe(
      "https://api.siliconflow.cn/v1",
    );
    expect(getProviderPreset("moonshot")?.baseUrl).toBe(
      "https://api.moonshot.ai/v1",
    );
    expect(getProviderPreset("deepseek")?.baseUrl).toBe(
      "https://api.deepseek.com",
    );
    expect(getProviderPreset("zhipu")?.baseUrl).toBe(
      "https://open.bigmodel.cn/api/paas/v4",
    );
    expect(getProviderPreset("dashscope")?.baseUrl).toBe(
      "https://dashscope.aliyuncs.com/compatible-mode/v1",
    );
    expect(getProviderPreset("minimax")?.baseUrl).toBe(
      "https://api.minimax.io/v1",
    );
    expect(getProviderPreset("openrouter")?.baseUrl).toBe(
      "https://openrouter.ai/api/v1",
    );
    expect(getProviderPreset("groq")?.baseUrl).toBe(
      "https://api.groq.com/openai/v1",
    );
    expect(getProviderPreset("together")?.baseUrl).toBe(
      "https://api.together.ai/v1",
    );
    expect(getProviderPreset("opencode_zen")?.baseUrl).toBe(
      "https://opencode.ai/zen/v1",
    );
  });

  it("returns undefined for unknown ids", () => {
    expect(getProviderPreset("not-a-real-vendor")).toBeUndefined();
  });
});
