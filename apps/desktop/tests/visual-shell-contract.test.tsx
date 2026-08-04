/**
 * Focused visual contract: fail if the shell regresses to the pre-correction
 * admin dual-rail look (filled primary nav, old accent, rectangular send,
 * two persistent left rails).
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
  it("uses upstream dark tokens and rejects the old light prototype accent", () => {
    expect(tokensCss).toMatch(/color-scheme:\s*dark/i);
    expect(tokensCss).toMatch(/--bg-main:\s*#151515/i);
    expect(tokensCss).toMatch(/--accent:\s*#8aa4ff/i);
    // Token assignment only (comments are free-form).
    expect(tokensCss).not.toMatch(/--accent:\s*#1769aa/i);
    expect(workbenchCss).not.toMatch(/#1769aa/i);
    expect(workbenchCss).not.toMatch(/--accent:\s*#1769aa/i);
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
