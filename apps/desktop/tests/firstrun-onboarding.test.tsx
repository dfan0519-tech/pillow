/**
 * First-run onboarding: never-configured vs misconfigured vs configured.
 * Dismiss persists through the settings layer mock (same path as Tauri).
 */
import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it } from "vitest";
import { App } from "../src/App";
import {
  FIRSTRUN_GROK_BODY,
  FIRSTRUN_PROJECT_BODY,
  FIRSTRUN_TITLE,
} from "../src/features/onboarding/onboardingCopy";
import {
  isRuntimeDegraded,
  shouldShowFirstRunOnboarding,
} from "../src/features/onboarding/shouldShowFirstRunOnboarding";
import { MockSettingsGateway } from "../src/features/settings/settingsGateway";
import { guidanceLooksSafe } from "../src/features/settings/loginGuidance";
import type { ProjectDto, ProjectGateway } from "../src/lib/projectGateway";
import tokensCss from "../src/styles/tokens.css?raw";
import onboardingCss from "../src/styles/onboarding.css?raw";
import "../src/styles/tokens.css";
import "../src/styles/settings.css";
import "../src/styles/onboarding.css";

function emptyProjectGateway(): ProjectGateway {
  return {
    listProjects: async () => [],
    createProject: async () => {
      throw new Error("not used");
    },
    selectProject: async () => {
      throw new Error("not used");
    },
    pickDirectory: async () => null,
    openOrRegisterProject: async () => {
      throw new Error("not used");
    },
  };
}

function configuredProjectGateway(): ProjectGateway {
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

function tokenMap(block: string): Map<string, string> {
  const map = new Map<string, string>();
  for (const match of block.matchAll(/--([a-z0-9-]+):\s*([^;]+);/gi)) {
    map.set(`--${match[1]}`, match[2]!.trim());
  }
  return map;
}

describe("shouldShowFirstRunOnboarding pure trigger", () => {
  const readyStartup = {
    conversationAvailable: true,
    selection: "grok-acp",
    degradedCode: null,
    degradedMessage: null,
    degradedGuidance: null,
  };

  it("shows only for empty shell, not dismissed, not degraded", () => {
    expect(
      shouldShowFirstRunOnboarding({
        dismissed: false,
        projectsCount: 0,
        runtimeStartup: readyStartup,
        shellReady: true,
      }),
    ).toBe(true);
  });

  it("suppresses for configured users (negative control)", () => {
    expect(
      shouldShowFirstRunOnboarding({
        dismissed: false,
        projectsCount: 1,
        runtimeStartup: readyStartup,
        shellReady: true,
      }),
    ).toBe(false);
  });

  it("suppresses when dismissed", () => {
    expect(
      shouldShowFirstRunOnboarding({
        dismissed: true,
        projectsCount: 0,
        runtimeStartup: readyStartup,
        shellReady: true,
      }),
    ).toBe(false);
  });

  it("suppresses when runtime is degraded (misconfigured path)", () => {
    const degraded = {
      conversationAvailable: false,
      selection: null,
      degradedCode: "runtime_executable_missing",
      degradedMessage: "missing",
      degradedGuidance: "open settings",
    };
    expect(isRuntimeDegraded(degraded)).toBe(true);
    expect(
      shouldShowFirstRunOnboarding({
        dismissed: false,
        projectsCount: 0,
        runtimeStartup: degraded,
        shellReady: true,
      }),
    ).toBe(false);
  });
});

describe("App first-run onboarding surface", () => {
  it("1 — first run with no projects and not skipped → guidance appears", async () => {
    const settings = new MockSettingsGateway();
    render(
      <App gateway={emptyProjectGateway()} settingsGateway={settings} />,
    );

    const card = await screen.findByTestId("firstrun-onboarding");
    expect(card).toBeVisible();
    expect(card.getAttribute("role")).toBe("region");
    expect(card.textContent).toContain(FIRSTRUN_TITLE);
    expect(screen.getByTestId("firstrun-project-body").textContent).toBe(
      FIRSTRUN_PROJECT_BODY,
    );
    expect(screen.getByTestId("firstrun-open-project")).toBeVisible();
    expect(screen.getByTestId("firstrun-dismiss")).toBeVisible();
    // Must not look like a red error alert.
    expect(card.getAttribute("role")).not.toBe("alert");
    expect(screen.queryByTestId("runtime-degraded-banner")).toBeNull();
  });

  it("2 — dismiss persists on remount (settings layer; restart simulation)", async () => {
    const user = userEvent.setup();
    const settings = new MockSettingsGateway();

    const first = render(
      <App gateway={emptyProjectGateway()} settingsGateway={settings} />,
    );
    expect(await screen.findByTestId("firstrun-onboarding")).toBeVisible();

    await user.click(screen.getByTestId("firstrun-dismiss"));
    await waitFor(() => {
      expect(screen.queryByTestId("firstrun-onboarding")).toBeNull();
    });
    // Gateway holds the durable mark (same store path as host).
    expect((await settings.getOnboardingSettings()).dismissed).toBe(true);
    expect(settings.getOnboardingSnapshot().source).toBe("global");

    first.unmount();

    // Remount with the same gateway = process restart with persisted settings.
    render(
      <App gateway={emptyProjectGateway()} settingsGateway={settings} />,
    );
    await screen.findByTestId("empty-workbench");
    await waitFor(() => {
      expect(screen.queryByTestId("firstrun-onboarding")).toBeNull();
    });
  });

  it("3 — negative control: configured user never sees guidance", async () => {
    const settings = new MockSettingsGateway(
      undefined,
      undefined,
      undefined,
      { dismissed: false, source: "default" },
    );
    render(
      <App
        gateway={configuredProjectGateway()}
        settingsGateway={settings}
      />,
    );

    await screen.findByTestId("app-sidebar");
    // Give async onboarding + startup reads a tick.
    await waitFor(() => {
      expect(settings.getOnboardingSnapshot().dismissed).toBe(false);
    });
    expect(screen.queryByTestId("firstrun-onboarding")).toBeNull();
    expect(screen.queryByTestId("runtime-degraded-banner")).toBeNull();
  });

  it("4 — misconfigured startup uses degraded banner, not first-run", async () => {
    const settings = new MockSettingsGateway();
    settings.setDegradedStartup(
      "runtime_executable_missing",
      "Grok executable is not a file: missing.exe",
      "打开「设置」修正 Grok 可执行文件路径。",
    );
    render(
      <App gateway={emptyProjectGateway()} settingsGateway={settings} />,
    );

    expect(await screen.findByTestId("runtime-degraded-banner")).toBeVisible();
    expect(screen.queryByTestId("firstrun-onboarding")).toBeNull();
    // Degraded is alert (error path); first-run would have been region.
    expect(
      screen.getByTestId("runtime-degraded-banner").getAttribute("role"),
    ).toBe("alert");
  });

  it("5 — copy never implies Pillow stores Grok login (ADR-0003 claim)", async () => {
    const settings = new MockSettingsGateway();
    render(
      <App gateway={emptyProjectGateway()} settingsGateway={settings} />,
    );

    const grokBody = await screen.findByTestId("firstrun-grok-body");
    expect(grokBody.textContent).toBe(FIRSTRUN_GROK_BODY);
    expect(FIRSTRUN_GROK_BODY).toMatch(/Grok 登录由 Grok CLI 管理/);
    expect(FIRSTRUN_GROK_BODY).toMatch(/不存储/);
    expect(FIRSTRUN_GROK_BODY).toMatch(/grok login/i);
    expect(FIRSTRUN_GROK_BODY).not.toMatch(
      /Pillow (保存|存储|代管|管理).{0,8}登录|粘贴.*API|auth\.json/i,
    );
    expect(guidanceLooksSafe(FIRSTRUN_GROK_BODY)).toBe(true);
    expect(guidanceLooksSafe(FIRSTRUN_PROJECT_BODY)).toBe(true);
  });

  it("6 — dual-skin body contrast ≥ 4.5:1 for onboarding text tokens", () => {
    // Onboarding body uses --text-primary on mix of --accent and --bg-elevated.
    // Bound the worst case with solid elevated surfaces used as panel base.
    const darkBlock =
      tokensCss.match(
        /:root\s*,\s*\[data-theme=["']dark["']\]\s*\{([\s\S]*?)\n\}/,
      )?.[1]
      ?? tokensCss.match(/\[data-theme=["']dark["']\]\s*\{([\s\S]*?)\n\}/)?.[1];
    const lightBlock = tokensCss.match(
      /\[data-theme=["']light["']\]\s*\{([\s\S]*?)\n\}/,
    )?.[1];
    expect(darkBlock).toBeTruthy();
    expect(lightBlock).toBeTruthy();

    const dark = tokenMap(darkBlock!);
    const light = tokenMap(lightBlock!);

    const darkPrimary = dark.get("--text-primary")!;
    const darkElevated = dark.get("--bg-elevated")!;
    const lightPrimary = light.get("--text-primary")!;
    const lightElevated = light.get("--bg-elevated")!;

    expect(darkPrimary.toLowerCase()).toMatch(/^#/);
    expect(lightPrimary.toLowerCase()).toMatch(/^#/);

    const darkRatio = contrastRatio(darkPrimary, darkElevated);
    const lightRatio = contrastRatio(lightPrimary, lightElevated);
    expect(darkRatio).toBeGreaterThanOrEqual(4.5);
    expect(lightRatio).toBeGreaterThanOrEqual(4.5);

    // Styles pin body to --text-primary (not tertiary / muted).
    expect(onboardingCss).toMatch(
      /\.firstrun-onboarding__body p\s*\{[^}]*color:\s*var\(--text-primary\)/s,
    );
    // No content blur; lilac only on CTA class.
    expect(onboardingCss).not.toMatch(/backdrop-filter|filter:\s*blur/);
    expect(onboardingCss).toMatch(/\.firstrun-onboarding__cta/);
  });

  it("close control uses the same dismiss path", async () => {
    const user = userEvent.setup();
    const settings = new MockSettingsGateway();
    render(
      <App gateway={emptyProjectGateway()} settingsGateway={settings} />,
    );
    await screen.findByTestId("firstrun-onboarding");
    await user.click(screen.getByTestId("firstrun-close"));
    await waitFor(() => {
      expect(screen.queryByTestId("firstrun-onboarding")).toBeNull();
    });
    expect((await settings.getOnboardingSettings()).dismissed).toBe(true);
  });

  it("open-settings CTA reaches settings without storing credentials", async () => {
    const user = userEvent.setup();
    const settings = new MockSettingsGateway();
    render(
      <App gateway={emptyProjectGateway()} settingsGateway={settings} />,
    );
    await screen.findByTestId("firstrun-onboarding");
    await user.click(screen.getByTestId("firstrun-open-settings"));
    expect(await screen.findByTestId("settings-panel")).toBeVisible();
    // Settings uses the same ADR claim (CLI owns login; Pillow does not hold it).
    expect(screen.getByTestId("settings-login-guidance").textContent).toMatch(
      /从不读取或保存|不存储/,
    );
    expect(screen.getByTestId("settings-login-guidance").textContent).toMatch(
      /grok login/i,
    );
  });
});
