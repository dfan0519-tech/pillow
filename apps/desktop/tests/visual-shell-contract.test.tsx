/**
 * Focused visual contract: fail if the shell regresses to the pre-correction
 * admin dual-rail look (filled primary nav, old accent, rectangular send,
 * two persistent left rails). Extended for dual skins (dark default + light).
 */
import { render, screen } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import { App } from "../src/App";
import type { ProjectDto, ProjectGateway } from "../src/lib/projectGateway";
import tokensCss from "../src/styles/tokens.css?raw";
import workbenchShellCss from "../src/styles/workbench-shell.css?raw";
import workbenchChromeCss from "../src/styles/workbench-chrome.css?raw";
import workbenchConversationCss from "../src/styles/workbench-conversation.css?raw";
import "../src/styles/tokens.css";
import "../src/styles/workbench.css";

/** Composed workbench stylesheet (cascade order matches workbench.css imports). */
const workbenchCss = `${workbenchShellCss}\n${workbenchConversationCss}\n${workbenchChromeCss}`;

/** Semantic color tokens that must exist with identical names in both skins. */
const THEME_TOKEN_NAMES = [
  "--bg-app",
  "--bg-sidebar",
  "--bg-sidebar-solid",
  "--bg-main",
  "--bg-aside",
  "--bg-elevated",
  "--bg-hover",
  "--bg-active",
  "--bg-input",
  "--bg-code",
  "--bg-user-bubble",
  "--border-subtle",
  "--border-strong",
  "--border-focus",
  "--text-primary",
  "--text-secondary",
  "--text-tertiary",
  "--text-inverse",
  "--accent",
  "--accent-muted",
  "--accent-hover",
  "--accent-fg",
  "--success",
  "--danger",
  "--warning",
  "--ambient-orb-a",
  "--ambient-orb-b",
  "--ambient-orb-c",
  "--glass-bg",
  "--glass-bg-strong",
  "--glass-border",
  "--glass-highlight",
  "--glass-glow",
  "--glass-blur",
  "--glass-saturate",
  "--shadow-composer",
  "--canvas",
  "--panel",
  "--panel-muted",
  "--line",
  "--line-strong",
  "--text",
  "--muted",
  "--accent-soft",
] as const;

/** Parse #rrggbb (or #rgb) to sRGB 0–255. */
function hexToRgb(hex: string): [number, number, number] {
  const h = hex.replace("#", "").trim();
  if (h.length === 3) {
    return [
      parseInt(h[0] + h[0], 16),
      parseInt(h[1] + h[1], 16),
      parseInt(h[2] + h[2], 16),
    ];
  }
  return [
    parseInt(h.slice(0, 2), 16),
    parseInt(h.slice(2, 4), 16),
    parseInt(h.slice(4, 6), 16),
  ];
}

/** WCAG 2.x relative luminance for sRGB. */
function relativeLuminance(hex: string): number {
  const [r, g, b] = hexToRgb(hex).map((c) => {
    const s = c / 255;
    return s <= 0.03928 ? s / 12.92 : ((s + 0.055) / 1.055) ** 2.4;
  });
  return 0.2126 * r + 0.7152 * g + 0.0722 * b;
}

/** WCAG contrast ratio between two hex colors. */
function contrastRatio(a: string, b: string): number {
  const l1 = relativeLuminance(a);
  const l2 = relativeLuminance(b);
  const [hi, lo] = l1 > l2 ? [l1, l2] : [l2, l1];
  return (hi + 0.05) / (lo + 0.05);
}

/** True when a token value is a lilac/purple wash (accent leak into chrome). */
function isLilacWash(value: string | undefined): boolean {
  if (!value) return false;
  const v = value.toLowerCase().replace(/\s+/g, "");
  // LILAC HAZE rgb(220,203,255) family in rgba/hsla washes.
  if (/rgba?\(\s*220\s*,\s*203\s*,\s*255/.test(v)) return true;
  if (/#dccbff/.test(v)) return true;
  return false;
}

/**
 * Extract custom-property declarations from a CSS rule body (between braces).
 * Returns Map of token name → first declared value (trimmed).
 */
function tokenMapFromBlock(block: string): Map<string, string> {
  const map = new Map<string, string>();
  const re = /(--[a-z0-9-]+)\s*:\s*([^;]+);/gi;
  let m: RegExpExecArray | null;
  while ((m = re.exec(block)) !== null) {
    const name = m[1].toLowerCase();
    if (!map.has(name)) {
      map.set(name, m[2].trim());
    }
  }
  return map;
}

/**
 * Slice the rule body for a selector matching `selectorPattern`.
 * Pattern may optionally end with `{`; brace depth is tracked from that open.
 */
function ruleBodyAfter(css: string, selectorPattern: RegExp): string {
  const sel = css.match(selectorPattern);
  if (!sel || sel.index === undefined) {
    return "";
  }
  const matched = sel[0];
  let open: number;
  if (matched.endsWith("{")) {
    open = sel.index + matched.length - 1;
  } else {
    open = css.indexOf("{", sel.index + matched.length);
  }
  if (open < 0) {
    return "";
  }
  let depth = 0;
  for (let i = open; i < css.length; i++) {
    if (css[i] === "{") depth++;
    else if (css[i] === "}") {
      depth--;
      if (depth === 0) {
        return css.slice(open + 1, i);
      }
    }
  }
  return "";
}

vi.mock("@tauri-apps/api/window", () => ({
  getCurrentWindow: () => ({
    minimize: vi.fn().mockResolvedValue(undefined),
    toggleMaximize: vi.fn().mockResolvedValue(undefined),
    close: vi.fn().mockResolvedValue(undefined),
    isMaximized: vi.fn().mockResolvedValue(false),
    onResized: vi.fn().mockResolvedValue(vi.fn()),
  }),
}));

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

describe("grok shell visual contract", () => {
  it("uses Pillow brand dark tokens and rejects the old light prototype accent", () => {
    expect(tokensCss).toMatch(/color-scheme:\s*dark/i);
    // Ambient glass: stepped BLACK INK / GRAPHITE surfaces + LILAC HAZE accent.
    expect(tokensCss).toMatch(/--bg-main:\s*#1e1e24/i);
    expect(tokensCss).toMatch(/--bg-elevated:\s*#2c2c36/i);
    expect(tokensCss).toMatch(/--bg-app:\s*#0e0e10/i);
    expect(tokensCss).toMatch(/--accent:\s*#dccbff/i);
    // Token assignment only (comments are free-form).
    expect(tokensCss).not.toMatch(/--accent:\s*#1769aa/i);
    expect(tokensCss).not.toMatch(/--accent:\s*#8aa4ff/i);
    expect(workbenchCss).not.toMatch(/#1769aa/i);
    expect(workbenchCss).not.toMatch(/--accent:\s*#1769aa/i);
  });

  it("defines dual skins with matching token names and locked values", () => {
    // Explicit theme selectors required (not only bare :root).
    expect(tokensCss).toMatch(/\[data-theme=["']dark["']\]/);
    expect(tokensCss).toMatch(/\[data-theme=["']light["']\]/);

    const darkBody = ruleBodyAfter(
      tokensCss,
      /:root\s*,\s*\[data-theme=["']dark["']\]\s*\{|\[data-theme=["']dark["']\]\s*\{/,
    );
    // Prefer the combined :root, [data-theme="dark"] block if present.
    const darkCombined = ruleBodyAfter(
      tokensCss,
      /:root\s*,\s*\[data-theme=["']dark["']\]\s*\{/,
    );
    const darkMap = tokenMapFromBlock(darkCombined || darkBody);
    const lightMap = tokenMapFromBlock(
      ruleBodyAfter(tokensCss, /\[data-theme=["']light["']\]\s*\{/),
    );

    expect(darkMap.size).toBeGreaterThan(0);
    expect(lightMap.size).toBeGreaterThan(0);

    // Same names and same count across both skins.
    const darkNames = [...darkMap.keys()].sort();
    const lightNames = [...lightMap.keys()].sort();
    expect(lightNames).toEqual(darkNames);

    for (const name of THEME_TOKEN_NAMES) {
      expect(darkMap.has(name), `dark missing ${name}`).toBe(true);
      expect(lightMap.has(name), `light missing ${name}`).toBe(true);
    }

    // Locked dark values (ambient-glass surface ladder — do not relax).
    expect(darkMap.get("--bg-app")?.toLowerCase()).toBe("#0e0e10");
    expect(darkMap.get("--bg-main")?.toLowerCase()).toBe("#1e1e24");
    expect(darkMap.get("--bg-elevated")?.toLowerCase()).toBe("#2c2c36");
    expect(darkMap.get("--accent")?.toLowerCase()).toBe("#dccbff");

    // Locked light brand values.
    expect(lightMap.get("--bg-app")?.toLowerCase()).toBe("#faf9f7");
    expect(lightMap.get("--bg-main")?.toLowerCase()).toBe("#eeebe6");
    expect(lightMap.get("--bg-elevated")?.toLowerCase()).toBe("#ffffff");
    expect(lightMap.get("--text-primary")?.toLowerCase()).toBe("#3e3e46");
    expect(lightMap.get("--accent")?.toLowerCase()).toBe("#dccbff");
    // Light canvas must not collapse to pure white.
    expect(lightMap.get("--bg-app")?.toLowerCase()).not.toBe("#ffffff");
    expect(lightMap.get("--bg-main")?.toLowerCase()).not.toBe("#ffffff");

    // Hover/active must be neutral (no lilac wash) in both skins.
    expect(isLilacWash(darkMap.get("--bg-hover"))).toBe(false);
    expect(isLilacWash(darkMap.get("--bg-active"))).toBe(false);
    expect(isLilacWash(lightMap.get("--bg-hover"))).toBe(false);
    expect(isLilacWash(lightMap.get("--bg-active"))).toBe(false);
    // Structural glass glow must not be lilac either.
    expect(isLilacWash(darkMap.get("--glass-glow") ?? "")).toBe(false);
    expect(isLilacWash(lightMap.get("--glass-glow") ?? "")).toBe(false);

    // Ambient orbs present and lilac-family (color field foundation).
    for (const name of ["--ambient-orb-a", "--ambient-orb-b", "--ambient-orb-c"] as const) {
      expect(darkMap.get(name) ?? "").toMatch(/rgba?\(/i);
      expect(lightMap.get(name) ?? "").toMatch(/rgba?\(/i);
    }
    expect(darkMap.get("--ambient-orb-a")?.toLowerCase() ?? "").toMatch(
      /220\s*,\s*203\s*,\s*255/,
    );

    // Glass blur: dark stronger, light lighter (mud avoidance).
    const darkBlur = Number.parseFloat(darkMap.get("--glass-blur") ?? "0");
    const lightBlur = Number.parseFloat(lightMap.get("--glass-blur") ?? "0");
    expect(darkBlur).toBeGreaterThanOrEqual(18);
    expect(lightBlur).toBeGreaterThan(0);
    expect(lightBlur).toBeLessThan(darkBlur);

    // Surface ladder: adjacent surfaces must be perceptibly stepped.
    const darkAppMain = contrastRatio(
      darkMap.get("--bg-app")!,
      darkMap.get("--bg-main")!,
    );
    const darkMainElev = contrastRatio(
      darkMap.get("--bg-main")!,
      darkMap.get("--bg-elevated")!,
    );
    expect(darkAppMain).toBeGreaterThanOrEqual(1.15);
    expect(darkMainElev).toBeGreaterThanOrEqual(1.15);

    const lightAppMain = contrastRatio(
      lightMap.get("--bg-app")!,
      lightMap.get("--bg-main")!,
    );
    const lightMainElev = contrastRatio(
      lightMap.get("--bg-main")!,
      lightMap.get("--bg-elevated")!,
    );
    expect(lightAppMain).toBeGreaterThanOrEqual(1.1);
    expect(lightMainElev).toBeGreaterThanOrEqual(1.1);

    // Body text contrast ≥ 4.5:1 on main surfaces (both skins).
    expect(
      contrastRatio(darkMap.get("--text-primary")!, darkMap.get("--bg-main")!),
    ).toBeGreaterThanOrEqual(4.5);
    expect(
      contrastRatio(darkMap.get("--text-secondary")!, darkMap.get("--bg-main")!),
    ).toBeGreaterThanOrEqual(4.5);
    expect(
      contrastRatio(lightMap.get("--text-primary")!, lightMap.get("--bg-main")!),
    ).toBeGreaterThanOrEqual(4.5);
    expect(
      contrastRatio(lightMap.get("--text-secondary")!, lightMap.get("--bg-main")!),
    ).toBeGreaterThanOrEqual(4.5);

    // Light tertiary was #7a7a84 (3.57:1) — strengthened to body-size ≥4.5:1.
    // Dark tertiary already ≥4.5:1; do not weaken.
    const lightTertiary = lightMap.get("--text-tertiary")!.toLowerCase();
    expect(lightTertiary).toBe("#686872");
    expect(
      contrastRatio(lightTertiary, lightMap.get("--bg-main")!),
    ).toBeGreaterThanOrEqual(4.5);
    // Strengthening only: must not regress to the failing legacy value.
    expect(lightTertiary).not.toBe("#7a7a84");

    // color-scheme switches with theme.
    expect(darkCombined || darkBody).toMatch(/color-scheme:\s*dark/i);
    expect(
      ruleBodyAfter(tokensCss, /\[data-theme=["']light["']\]\s*\{/),
    ).toMatch(/color-scheme:\s*light/i);

    // No domain storage in the skin layer.
    expect(tokensCss).not.toMatch(/localStorage|sessionStorage|indexedDB/i);
  });

  it("paints a static ambient color field under chrome, not per-row glass", () => {
    // Three radial orbs on the shell canvas (CSS only, no image assets).
    const shellBlock =
      workbenchShellCss.match(/\.app-shell\s*\{[^}]+\}/)?.[0] ?? "";
    expect(shellBlock).toMatch(/background-image:/);
    expect(shellBlock).toMatch(/--ambient-orb-a/);
    expect(shellBlock).toMatch(/--ambient-orb-b/);
    expect(shellBlock).toMatch(/--ambient-orb-c/);
    expect(shellBlock).toMatch(/radial-gradient/i);
    // Static — no animation on ambient field.
    expect(shellBlock).not.toMatch(/animation:/i);
    // No bitmap ambient.
    expect(tokensCss).not.toMatch(/url\s*\(\s*['"]?[^)'"]+\.(png|jpg|webp|svg)/i);
    expect(workbenchCss).not.toMatch(
      /url\s*\(\s*['"]?[^)'"]+\.(png|jpg|webp|svg)/i,
    );

    // Workbench shell is transparent so orbs reach resource-pane glass.
    const workbenchBlock =
      workbenchShellCss.match(/\.workbench\s*\{[^}]+\}/)?.[0] ?? "";
    expect(workbenchBlock).toMatch(/background:\s*transparent/);

    // Content stage stays solid/calm (no backdrop-filter on conversation).
    const convoBlock =
      workbenchConversationCss.match(/\.conversation-surface\s*\{[^}]+\}/)?.[0] ??
      "";
    expect(convoBlock).toMatch(/background:\s*var\(--bg-main\)/);
    expect(convoBlock).not.toMatch(/backdrop-filter/);

    // Container-level glass only on chrome selectors — not list rows.
    expect(workbenchShellCss).toMatch(
      /\.app-sidebar\s*\{[^}]*backdrop-filter:/s,
    );
    expect(workbenchChromeCss).toMatch(
      /\.resource-pane\s*\{[^}]*backdrop-filter:/s,
    );
    expect(workbenchChromeCss).toMatch(
      /\.composer-wrap--float\s+\.composer\s*\{[^}]*backdrop-filter:/s,
    );
    expect(workbenchChromeCss).toMatch(
      /\.modal-card\s*\{[^}]*backdrop-filter:/s,
    );
    // Dense rows must not carry blur.
    expect(workbenchShellCss).not.toMatch(
      /\.session-row[^{]*\{[^}]*backdrop-filter:/s,
    );
    expect(workbenchShellCss).not.toMatch(
      /\.project-row[^{]*\{[^}]*backdrop-filter:/s,
    );

    // Panel-level radius for chrome cards (16–20), not mobile 28–32.
    expect(tokensCss).toMatch(/--radius-panel:\s*16px/);
    expect(tokensCss).toMatch(/--radius-sm:\s*6px/);
    expect(tokensCss).toMatch(/--radius-md:\s*8px/);
    expect(workbenchChromeCss).toMatch(
      /\.modal-card\s*\{[^}]*border-radius:\s*var\(--radius-panel/s,
    );
  });

  it("applies light theme tokens when data-theme=light is set on documentElement", () => {
    document.documentElement.setAttribute("data-theme", "light");
    // Force style recalc against imported stylesheet.
    const appBg = getComputedStyle(document.documentElement)
      .getPropertyValue("--bg-app")
      .trim()
      .toLowerCase();
    const textPrimary = getComputedStyle(document.documentElement)
      .getPropertyValue("--text-primary")
      .trim()
      .toLowerCase();
    const colorScheme = getComputedStyle(document.documentElement).colorScheme;

    expect(appBg).toBe("#faf9f7");
    expect(textPrimary).toBe("#3e3e46");
    expect(colorScheme).toMatch(/light/i);

    document.documentElement.setAttribute("data-theme", "dark");
    const darkApp = getComputedStyle(document.documentElement)
      .getPropertyValue("--bg-app")
      .trim()
      .toLowerCase();
    expect(darkApp).toBe("#0e0e10");

    document.documentElement.removeAttribute("data-theme");
  });

  it("keeps .nav-new ghost (no filled primary project nav row)", () => {
    // Solid blue CTA used `background: var(--accent)` on a dedicated .nav-new rule.
    expect(workbenchCss).not.toMatch(
      /\.nav-new\s*\{[^}]*background:\s*var\(--accent\)/s,
    );
    // Shared ghost rule for .nav-new / .nav-item.
    expect(workbenchCss).toMatch(
      /\.nav-new,\s*\n\.nav-item\s*\{[^}]*background:\s*transparent/s,
    );
  });

  it("uses a circular icon-only send control, not a rectangular text button", async () => {
    render(<App gateway={fixtureGateway()} />);
    await screen.findByTestId("empty-workbench");

    const send = screen.getByTestId("composer-send");
    expect(send).toHaveClass("icon-btn", "icon-btn--primary");
    expect(send).toHaveAttribute("aria-label", "发送");
    // No visible "发送" text label on the control body.
    expect(send.textContent?.replace(/\s/g, "")).not.toBe("发送");

    const iconBtnBlock = workbenchCss.match(/\.icon-btn\s*\{[^}]+\}/)?.[0] ?? "";
    expect(iconBtnBlock).toMatch(/border-radius:\s*999px/);
    expect(workbenchCss).toMatch(/\.icon-btn--primary\s*\{/);
  });

  it("mounts a single continuous left sidebar, not two persistent left rails", async () => {
    render(<App gateway={fixtureGateway()} />);
    await screen.findByTestId("app-sidebar");

    expect(document.querySelectorAll(".app-sidebar").length).toBe(1);
    expect(document.querySelector(".project-sidebar")).toBeNull();
    expect(document.querySelector(".conversation-sidebar")).toBeNull();
    expect(document.querySelector(".titlebar")).toBeNull();
    expect(document.querySelectorAll(".sidebar-brand-row")).toHaveLength(1);
    // Workbench is stage + optional resource only (no second rail column).
    expect(workbenchCss).toMatch(
      /\.workbench\s*\{[^}]*grid-template-columns:\s*minmax\(0,\s*1fr\)/s,
    );
    expect(workbenchCss).not.toMatch(
      /\.workbench\s*\{[^}]*grid-template-columns:\s*200px/s,
    );
  });

  it("ports composer radius and soft float shadow language", () => {
    expect(tokensCss).toMatch(/--radius-composer:\s*18px/);
    expect(workbenchCss).toMatch(/border-radius:\s*var\(--radius-composer\)/);
    expect(workbenchCss).toMatch(/composer-wrap--float/);
    // No hard footer divider on composer.
    const footerBlock = workbenchCss.match(/\.composer__footer\s*\{[^}]+\}/)?.[0] ?? "";
    expect(footerBlock).not.toMatch(/border-top:/);
    const focusWithinBlock =
      workbenchChromeCss.match(/\.composer:focus-within\s*\{[^}]+\}/)?.[0] ?? "";
    expect(focusWithinBlock).toMatch(/border-color:\s*var\(--border-strong\)/);
    expect(focusWithinBlock).toMatch(/box-shadow:/);

    const focusVisibleBlock =
      workbenchChromeCss.match(
        /\.composer textarea:focus-visible\s*\{[^}]+\}/,
      )?.[0] ?? "";
    const focusToken =
      tokensCss.match(/--border-focus:\s*([^;]+);/)?.[1]?.trim() ?? "";
    const outlineWidth = Number(
      focusVisibleBlock.match(
        /outline(?:-width)?:\s*([0-9.]+)(?:px|rem|em)\b/,
      )?.[1] ?? 0,
    );
    const outlineStyle =
      focusVisibleBlock.match(
        /outline:\s*[0-9.]+(?:px|rem|em)\s+([a-z-]+)/,
      )?.[1] ??
      focusVisibleBlock.match(/outline-style:\s*([a-z-]+)/)?.[1] ??
      "";

    expect(focusVisibleBlock).toMatch(/var\(--border-focus\)/);
    expect(focusToken).not.toMatch(/\btransparent\b/i);
    expect(focusToken).not.toMatch(/rgba\([^)]*,\s*0(?:\.0+)?\s*\)/i);
    expect(outlineWidth).toBeGreaterThan(0);
    expect(outlineStyle).toMatch(/^(?:solid|dashed|dotted|double)$/);
  });

  it("keeps collapsed run activity as a quiet inline disclosure", () => {
    const activityBlock = workbenchConversationCss.match(
      /\.run-activity\s*\{[^}]+\}/,
    )?.[0] ?? "";
    const summaryBlock = workbenchConversationCss.match(
      /\.run-activity summary\s*\{[^}]+\}/,
    )?.[0] ?? "";
    expect(activityBlock).toMatch(/border:\s*0/);
    expect(activityBlock).toMatch(/background:\s*transparent/);
    expect(summaryBlock).toMatch(/display:\s*inline-flex/);
  });
});
