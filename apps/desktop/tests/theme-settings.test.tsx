/**
 * Theme settings: second consumer of the settings layer (precedence, env,
 * fail-closed surface on the mock gateway; FOUC stamp on documentElement).
 */
import { render, waitFor } from "@testing-library/react";
import { afterEach, describe, expect, it } from "vitest";
import { App } from "../src/App";
import { MockSettingsGateway } from "../src/features/settings/settingsGateway";
import {
  applyDocumentTheme,
  parseUiThemeLabel,
} from "../src/features/settings/settingsTypes";
import type { ProjectDto, ProjectGateway } from "../src/lib/projectGateway";
import tokensCss from "../src/styles/tokens.css?raw";
import "../src/styles/tokens.css";

function fixtureGateway(): ProjectGateway {
  const project: ProjectDto = {
    id: "pillow",
    name: "Pillow",
    rootPath: "F:\\work\\pillow",
    trust: "trusted",
    createdAt: "2026-07-29T00:00:00.000Z",
    updatedAt: "2026-07-29T00:00:00.000Z",
  };
  return {
    listProjects: async () => [project],
    createProject: async () => project,
    selectProject: async () => project,
    pickDirectory: async () => null,
    openOrRegisterProject: async () => project,
  };
}

function hexToRgb(hex: string): [number, number, number] {
  const h = hex.replace("#", "").trim();
  return [
    parseInt(h.slice(0, 2), 16),
    parseInt(h.slice(2, 4), 16),
    parseInt(h.slice(4, 6), 16),
  ];
}

function relativeLuminance(hex: string): number {
  const [r, g, b] = hexToRgb(hex).map((c) => {
    const s = c / 255;
    return s <= 0.03928 ? s / 12.92 : ((s + 0.055) / 1.055) ** 2.4;
  });
  return 0.2126 * r + 0.7152 * g + 0.0722 * b;
}

function contrastRatio(a: string, b: string): number {
  const l1 = relativeLuminance(a);
  const l2 = relativeLuminance(b);
  const [hi, lo] = l1 > l2 ? [l1, l2] : [l2, l1];
  return (hi + 0.05) / (lo + 0.05);
}

describe("theme settings consumer", () => {
  afterEach(() => {
    document.documentElement.removeAttribute("data-theme");
    document.documentElement.removeAttribute("data-theme-ready");
  });

  it("App applies resolved theme and stamps data-theme-ready (FOUC path)", async () => {
    const settings = new MockSettingsGateway(undefined, {
      theme: { value: "light", source: "global", envVar: null },
    });
    render(<App gateway={fixtureGateway()} settingsGateway={settings} />);

    await waitFor(() => {
      expect(document.documentElement.getAttribute("data-theme")).toBe("light");
      expect(document.documentElement.hasAttribute("data-theme-ready")).toBe(
        true,
      );
    });
  });

  it("env override on theme is returned by the gateway with source=env", async () => {
    const gateway = new MockSettingsGateway();
    gateway.setEnvOverride("theme", "light", "PILLOW_THEME");
    const view = await gateway.getThemeSettings();
    expect(view.theme.value).toBe("light");
    expect(view.theme.source).toBe("env");
    expect(view.theme.envVar).toBe("PILLOW_THEME");
    // Stored write must not clobber env effective value.
    const after = await gateway.setUiTheme("dark");
    expect(after.theme.value).toBe("light");
    expect(after.theme.source).toBe("env");
  });

  it("parseUiThemeLabel only maps light explicitly; everything else is dark label helper", () => {
    // Renderer helper for display only — host fail-closed is the authority.
    expect(parseUiThemeLabel("light")).toBe("light");
    expect(parseUiThemeLabel("dark")).toBe("dark");
    expect(parseUiThemeLabel(null)).toBe("dark");
  });

  it("applyDocumentTheme sets both theme and ready attributes", () => {
    applyDocumentTheme("light");
    expect(document.documentElement.getAttribute("data-theme")).toBe("light");
    expect(document.documentElement.hasAttribute("data-theme-ready")).toBe(true);
    applyDocumentTheme("dark");
    expect(document.documentElement.getAttribute("data-theme")).toBe("dark");
  });

  it("light tertiary on bg-main meets body contrast ≥ 4.5:1 (numeric)", () => {
    // Match the light *rule block* (not earlier comment mentions of data-theme=light).
    const lightBlock = tokensCss.match(
      /\[data-theme=["']light["']\]\s*\{([\s\S]*?)\n\}/,
    )?.[1];
    expect(lightBlock).toBeTruthy();
    const tertiary = lightBlock!.match(/--text-tertiary:\s*([^;]+);/)?.[1]
      ?.trim()
      .toLowerCase();
    const bgMain = lightBlock!.match(/--bg-main:\s*([^;]+);/)?.[1]
      ?.trim()
      .toLowerCase();
    expect(tertiary).toBe("#686872");
    expect(bgMain).toBe("#eeebe6");
    const ratio = contrastRatio("#686872", "#eeebe6");
    // Hand-checked WCAG relative luminance ratio; must stay ≥ 4.5.
    expect(ratio).toBeGreaterThanOrEqual(4.5);
    expect(ratio).toBeCloseTo(4.6334, 3);
  });
});
