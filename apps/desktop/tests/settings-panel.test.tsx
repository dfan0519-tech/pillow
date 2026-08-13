/**
 * Settings IA: category nav, two-tier permission policy, env override surface.
 */
import { render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it } from "vitest";
import { App } from "../src/App";
import {
  MockSettingsGateway,
  parsePermissionModeSettings,
  parsePermissionPolicy,
} from "../src/features/settings/settingsGateway";
import {
  PERMISSION_DENY_RECEIPT_NOTE,
  PERMISSION_MODE_OPTIONS,
  PERMISSION_TWO_AXIS_NOTE,
} from "../src/features/settings/permissionCopy";
import {
  adapterOptionsForBuild,
  SettingsPanel,
} from "../src/features/settings/SettingsPanel";
import { assertNoSecretFields } from "../src/features/settings/settingsGateway";
import type { ProjectDto, ProjectGateway } from "../src/lib/projectGateway";
import "../src/styles/tokens.css";
import "../src/styles/settings.css";

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

async function openCategory(
  user: ReturnType<typeof userEvent.setup>,
  id: "security" | "agents" | "runtime" | "appearance" | "diagnostics",
): Promise<void> {
  await user.click(screen.getByTestId(`settings-nav-${id}`));
}

describe("SettingsPanel — DTO contracts", () => {
  it("accepts the exact permission-mode DTO and rejects unknown or missing keys", () => {
    expect(
      parsePermissionModeSettings({ mode: "bypass", source: "global" }),
    ).toEqual({ mode: "bypass", source: "global" });
    expect(() =>
      parsePermissionModeSettings({
        mode: "bypass",
        source: "global",
        nativePath: "C:\\smuggled",
      })
    ).toThrow(/invalid settings response/i);
    expect(() =>
      parsePermissionModeSettings({ mode: "bypass" })
    ).toThrow(/invalid settings response/i);
    expect(() =>
      parsePermissionModeSettings({ mode: "always-approve", source: "global" })
    ).toThrow(/invalid settings response/i);
  });

  it("accepts the exact permission-policy DTO and rejects smuggled keys", () => {
    expect(
      parsePermissionPolicy({
        bypassAvailable: true,
        bypassAvailableSource: "global",
        projectMode: "accept_edits",
        projectModeSource: "project",
        effectiveMode: "accept_edits",
      }),
    ).toEqual({
      bypassAvailable: true,
      bypassAvailableSource: "global",
      projectMode: "accept_edits",
      projectModeSource: "project",
      effectiveMode: "accept_edits",
    });
    // Fail-closed: host claiming effective bypass while capability is off.
    expect(
      parsePermissionPolicy({
        bypassAvailable: false,
        bypassAvailableSource: "global",
        projectMode: "bypass",
        projectModeSource: "project",
        effectiveMode: "bypass",
      }).effectiveMode,
    ).toBe("default");
    expect(() =>
      parsePermissionPolicy({
        bypassAvailable: true,
        bypassAvailableSource: "global",
        projectMode: "default",
        projectModeSource: "project",
        effectiveMode: "default",
        nativePath: "C:\\smuggled",
      }),
    ).toThrow(/invalid settings response/i);
  });

});

describe("SettingsPanel — category IA", () => {
  it("exposes five IDE-style categories in the left nav", async () => {
    render(
      <SettingsPanel
        gateway={new MockSettingsGateway()}
        onClose={() => undefined}
        projectId={null}
      />,
    );
    await screen.findByTestId("settings-nav");
    for (const id of [
      "security",
      "agents",
      "runtime",
      "appearance",
      "diagnostics",
    ] as const) {
      expect(screen.getByTestId(`settings-nav-${id}`)).toBeInTheDocument();
    }
    // Default category is security (two-axis permission surface).
    expect(screen.getByTestId("settings-category-security")).toBeVisible();
    expect(screen.getByTestId("settings-permission-two-axis-note")).toHaveTextContent(
      PERMISSION_TWO_AXIS_NOTE,
    );
  });

  it("places theme under appearance and runtime paths under runtime", async () => {
    const user = userEvent.setup();
    render(
      <SettingsPanel
        gateway={new MockSettingsGateway()}
        onClose={() => undefined}
        projectId={null}
      />,
    );
    await screen.findByTestId("settings-nav");
    expect(screen.queryByTestId("settings-theme")).toBeNull();
    expect(screen.queryByTestId("settings-adapter")).toBeNull();

    await openCategory(user, "appearance");
    expect(screen.getByTestId("settings-theme")).toBeVisible();

    await openCategory(user, "runtime");
    expect(screen.getByTestId("settings-adapter")).toBeVisible();
    expect(screen.getByTestId("settings-grok-executable")).toBeVisible();
    expect(screen.getByTestId("settings-runtime-advanced")).toBeVisible();
    expect(screen.getByTestId("settings-profile-scope-note").textContent).toMatch(
      /登录型槽位/,
    );
    expect(screen.getByTestId("settings-profile-scope-note").textContent).toMatch(
      /API key/,
    );
  });
});

describe("SettingsPanel — permission two-tier (T2/T3)", () => {
  it("offers three project modes with two-axis copy (no unbounded 全开 wording)", async () => {
    const user = userEvent.setup();
    const gateway = new MockSettingsGateway();
    const applied: string[] = [];
    render(
      <SettingsPanel
        gateway={gateway}
        onClose={() => undefined}
        onPermissionModeChanged={(mode) => applied.push(mode)}
        projectId="project-1"
      />,
    );

    const select = await screen.findByTestId("settings-permission-mode");
    expect([...select.querySelectorAll("option")].map((option) => option.value)).toEqual([
      "default",
      "accept_edits",
      "bypass",
    ]);
    // Labels must not imply unbounded access.
    const labels = [...select.querySelectorAll("option")].map(
      (option) => option.textContent ?? "",
    );
    for (const label of labels) {
      expect(label).not.toMatch(/无界|无限制|任意目录|整个磁盘/);
    }
    expect(screen.getByTestId("settings-permission-two-axis-note")).toHaveTextContent(
      PERMISSION_TWO_AXIS_NOTE,
    );
    expect(PERMISSION_MODE_OPTIONS.every((o) => o.detail.includes("项目"))).toBe(
      true,
    );

    await user.selectOptions(select, "accept_edits");
    await user.click(screen.getByTestId("settings-save"));
    await waitFor(() => {
      expect(gateway.getPermissionPolicySnapshot("project-1").projectMode).toBe(
        "accept_edits",
      );
      expect(gateway.getPermissionPolicySnapshot("project-1").effectiveMode).toBe(
        "accept_edits",
      );
    });
    expect(applied).toContain("accept_edits");
    expect(screen.getByText(PERMISSION_DENY_RECEIPT_NOTE)).toBeVisible();
  });

  it("T3 positive: when global Bypass is available, project can select Bypass", async () => {
    const user = userEvent.setup();
    const gateway = new MockSettingsGateway(
      undefined,
      undefined,
      undefined,
      undefined,
      undefined,
      { bypassAvailable: true },
    );
    render(
      <SettingsPanel
        gateway={gateway}
        onClose={() => undefined}
        projectId="project-1"
      />,
    );

    const select = await screen.findByTestId("settings-permission-mode");
    const bypassOption = within(select).getByRole("option", {
      name: /Bypass/,
    }) as HTMLOptionElement;
    expect(bypassOption).not.toBeDisabled();

    await user.selectOptions(select, "bypass");
    await user.click(screen.getByTestId("settings-save"));
    await waitFor(() => {
      const policy = gateway.getPermissionPolicySnapshot("project-1");
      expect(policy.bypassAvailable).toBe(true);
      expect(policy.projectMode).toBe("bypass");
      expect(policy.effectiveMode).toBe("bypass");
    });
  });

  it("T3 negative: when global Bypass is off, project Bypass option is disabled with reason", async () => {
    const user = userEvent.setup();
    const gateway = new MockSettingsGateway(
      undefined,
      undefined,
      undefined,
      undefined,
      undefined,
      { bypassAvailable: false, preferredMode: "default" },
    );
    render(
      <SettingsPanel
        gateway={gateway}
        onClose={() => undefined}
        projectId="project-1"
      />,
    );

    const select = await screen.findByTestId("settings-permission-mode");
    const bypassOption = within(select).getByRole("option", {
      name: /Bypass/,
    }) as HTMLOptionElement;
    expect(bypassOption).toBeDisabled();
    expect(screen.getByTestId("settings-bypass-project-locked")).toBeVisible();
    expect(screen.getByTestId("settings-bypass-project-locked").textContent).toMatch(
      /全局已关闭 Bypass/,
    );

    // Mock gateway also rejects forced project bypass write.
    await expect(
      gateway.setProjectPermissionMode("project-1", "bypass"),
    ).rejects.toMatchObject({ code: "permission_bypass_not_available" });

    // Toggle capability on, save, then Bypass becomes selectable.
    await user.click(screen.getByTestId("settings-bypass-available"));
    await user.click(screen.getByTestId("settings-save"));
    await waitFor(() => {
      expect(gateway.getPermissionPolicySnapshot("project-1").bypassAvailable).toBe(
        true,
      );
    });
    expect(
      (within(screen.getByTestId("settings-permission-mode")).getByRole("option", {
        name: /Bypass/,
      }) as HTMLOptionElement),
    ).not.toBeDisabled();
  });

  it("disables project mode select when no project is open", async () => {
    render(
      <SettingsPanel
        gateway={new MockSettingsGateway()}
        onClose={() => undefined}
        projectId={null}
      />,
    );
    expect(await screen.findByTestId("settings-permission-mode")).toBeDisabled();
    expect(screen.getByTestId("settings-permission-mode-no-project")).toBeVisible();
  });

  it.each([
    ["accept_edits", /接受改动/],
    ["bypass", /全开/],
  ] as const)("keeps %s visible in the workbench and resets it in one click", async (mode, label) => {
    const user = userEvent.setup();
    const settings = new MockSettingsGateway(
      undefined,
      undefined,
      undefined,
      undefined,
      { mode, source: "global" },
    );
    render(<App gateway={fixtureGateway()} settingsGateway={settings} />);

    const indicator = await screen.findByTestId("permission-mode-indicator");
    expect(indicator).toBeVisible();
    expect(indicator.textContent).toMatch(label);
    await user.click(screen.getByTestId("permission-mode-reset"));
    await waitFor(() => {
      expect(settings.getPermissionModeSnapshot().mode).toBe("default");
      expect(screen.queryByTestId("permission-mode-indicator")).toBeNull();
    });
  });
});

describe("SettingsPanel — runtime / theme / shell", () => {
  it("shows env override state and keeps the field read-only", async () => {
    const user = userEvent.setup();
    const gateway = new MockSettingsGateway();
    gateway.setEnvOverride("adapter", "fake", "PILLOW_RUNTIME");
    gateway.setEnvOverride(
      "grokExecutable",
      "C:\\env\\grok.exe",
      "PILLOW_GROK_EXECUTABLE",
    );

    render(
      <SettingsPanel
        gateway={gateway}
        onClose={() => undefined}
        projectId={null}
      />,
    );

    await openCategory(user, "runtime");
    await waitFor(() => {
      expect(screen.getByTestId("settings-adapter")).toBeDisabled();
    });

    const override = screen.getByTestId("settings-adapter-env-override");
    expect(override).toBeVisible();
    expect(override.textContent).toMatch(/PILLOW_RUNTIME/);
    expect(override.textContent).toMatch(/只读/);

    expect(screen.getByTestId("settings-executable-env-override")).toBeVisible();
    expect(screen.getByTestId("settings-grok-executable")).toBeDisabled();

    // Source badge copy required by ADR-0003 visibility amendment.
    expect(screen.getAllByText("已被环境变量覆盖").length).toBeGreaterThanOrEqual(1);

    // Profile is not env-overridden → editable.
    expect(screen.getByTestId("settings-grok-profile")).not.toBeDisabled();
  });

  it("uses a constrained adapter select, not a free-text adapter input", async () => {
    const user = userEvent.setup();
    const gateway = new MockSettingsGateway();
    render(
      <SettingsPanel
        gateway={gateway}
        onClose={() => undefined}
        projectId={null}
      />,
    );
    await openCategory(user, "runtime");
    await waitFor(() => {
      expect(screen.getByTestId("settings-adapter")).toBeInTheDocument();
    });
    const select = screen.getByTestId("settings-adapter");
    expect(select.tagName).toBe("SELECT");
    // Dev mock: Fake selectable → two options (host flag, not env hard-code).
    expect(select.querySelectorAll("option").length).toBe(2);
  });

  it("hides Fake when host reports fakeAdapterSelectable false (release gate)", async () => {
    const user = userEvent.setup();
    const gateway = new MockSettingsGateway({
      fakeAdapterSelectable: false,
    });
    render(
      <SettingsPanel
        gateway={gateway}
        onClose={() => undefined}
        projectId={null}
      />,
    );
    await openCategory(user, "runtime");
    await waitFor(() => {
      expect(screen.getByTestId("settings-adapter")).toBeInTheDocument();
    });
    const select = screen.getByTestId("settings-adapter");
    const values = [...select.querySelectorAll("option")].map(
      (opt) => (opt as HTMLOptionElement).value,
    );
    expect(values).toEqual(["grok-acp"]);
    expect(values).not.toContain("fake");
  });

  it("shows Fake when host reports fakeAdapterSelectable true (dev gate)", async () => {
    const user = userEvent.setup();
    const gateway = new MockSettingsGateway({
      fakeAdapterSelectable: true,
    });
    render(
      <SettingsPanel
        gateway={gateway}
        onClose={() => undefined}
        projectId={null}
      />,
    );
    await openCategory(user, "runtime");
    await waitFor(() => {
      expect(screen.getByTestId("settings-adapter")).toBeInTheDocument();
    });
    const values = [
      ...screen.getByTestId("settings-adapter").querySelectorAll("option"),
    ].map((opt) => (opt as HTMLOptionElement).value);
    expect(values).toContain("fake");
    expect(values).toContain("grok-acp");
  });

  it("shows degraded banner from App and keeps settings reachable (not Fake)", async () => {
    const user = userEvent.setup();
    const settings = new MockSettingsGateway();
    settings.setDegradedStartup(
      "runtime_executable_missing",
      "Grok executable is not a file: missing.exe",
      "打开「设置」，将「Grok 可执行文件路径」改为本机已存在的文件。",
    );
    render(<App gateway={fixtureGateway()} settingsGateway={settings} />);

    const banner = await screen.findByTestId("runtime-degraded-banner");
    expect(banner).toBeVisible();
    expect(banner.textContent).toMatch(/对话运行时不可用/);
    expect(banner.textContent).not.toMatch(/using fake|回落.*Fake|Fake 适配器已启用/i);
    // Login guidance must not appear on the degraded path (distinct states).
    expect(screen.queryByTestId("runtime-login-guidance-banner")).toBeNull();
    // Settings reachable without conversation service.
    await user.click(screen.getByTestId("runtime-degraded-open-settings"));
    expect(await screen.findByTestId("settings-panel")).toBeVisible();
    expect(screen.getByTestId("nav-open-settings")).toBeInTheDocument();
    // Settings always carries the CLI-login claim (no account identity).
    expect(screen.getByTestId("settings-login-guidance")).toBeVisible();
    expect(screen.getByTestId("settings-login-guidance").textContent).toMatch(
      /grok login/i,
    );
    expect(screen.getByTestId("settings-login-guidance").textContent).not.toMatch(
      /auth\.json|@|sk-/i,
    );
  });

  it("rejects secret-shaped keys on the settings wire DTO", () => {
    expect(() =>
      assertNoSecretFields({
        adapter: { value: "grok-acp", source: "default" },
        grokExecutable: { value: null, source: "default" },
        grokProfile: { value: null, source: "default" },
        projectId: null,
      }),
    ).not.toThrow();

    expect(() =>
      assertNoSecretFields({
        adapter: { value: "grok-acp", source: "default" },
        apiKey: "sk-secret",
      }),
    ).toThrow(/secret-shaped/i);

    expect(() =>
      assertNoSecretFields({
        adapter: { value: "grok-acp", source: "default", authToken: "x" },
      }),
    ).toThrow(/secret-shaped/i);
  });

  it("mounts from App without changing existing sidebar testids", async () => {
    const user = userEvent.setup();
    const settings = new MockSettingsGateway();
    render(<App gateway={fixtureGateway()} settingsGateway={settings} />);

    await screen.findByTestId("app-sidebar");
    expect(screen.getByTestId("nav-open-project")).toBeInTheDocument();
    expect(document.querySelectorAll(".app-sidebar").length).toBe(1);

    await user.click(screen.getByTestId("nav-open-settings"));
    expect(await screen.findByTestId("settings-panel")).toBeVisible();
    expect(screen.getByRole("dialog", { name: "设置" })).toBeVisible();
  });

  it("saves adapter when not env-locked", async () => {
    const user = userEvent.setup();
    const gateway = new MockSettingsGateway();
    render(
      <SettingsPanel
        gateway={gateway}
        onClose={() => undefined}
        projectId={null}
      />,
    );
    await openCategory(user, "runtime");
    await waitFor(() => {
      expect(screen.getByTestId("settings-adapter")).not.toBeDisabled();
    });
    await user.selectOptions(screen.getByTestId("settings-adapter"), "fake");
    await user.click(screen.getByTestId("settings-save"));
    await waitFor(() => {
      expect(screen.getByTestId("settings-notice")).toBeVisible();
    });
    const snap = gateway.getSnapshot();
    expect(snap.adapter.value).toBe("fake");
  });

  it("shows theme env override state and keeps the field read-only", async () => {
    const user = userEvent.setup();
    const gateway = new MockSettingsGateway();
    gateway.setEnvOverride("theme", "light", "PILLOW_THEME");

    render(
      <SettingsPanel
        gateway={gateway}
        onClose={() => undefined}
        projectId={null}
      />,
    );
    await openCategory(user, "appearance");

    await waitFor(() => {
      expect(screen.getByTestId("settings-theme")).toBeDisabled();
    });

    const override = screen.getByTestId("settings-theme-env-override");
    expect(override).toBeVisible();
    expect(override.textContent).toMatch(/PILLOW_THEME/);
    expect(override.textContent).toMatch(/只读/);
    expect(screen.getAllByText("已被环境变量覆盖").length).toBeGreaterThanOrEqual(
      1,
    );
    expect(screen.getByTestId("settings-theme")).toHaveValue("light");
  });

  it("saves global theme and applies it immediately when not env-locked", async () => {
    const user = userEvent.setup();
    const gateway = new MockSettingsGateway();
    const applied: string[] = [];
    render(
      <SettingsPanel
        gateway={gateway}
        onClose={() => undefined}
        onThemeChanged={(theme) => applied.push(theme)}
        projectId={null}
      />,
    );
    await openCategory(user, "appearance");
    await waitFor(() => {
      expect(screen.getByTestId("settings-theme")).not.toBeDisabled();
    });
    await user.selectOptions(screen.getByTestId("settings-theme"), "light");
    await user.click(screen.getByTestId("settings-save"));
    await waitFor(() => {
      expect(screen.getByTestId("settings-notice")).toBeVisible();
    });
    expect(gateway.getThemeSnapshot().theme.value).toBe("light");
    expect(gateway.getThemeSnapshot().theme.source).toBe("global");
    expect(applied).toContain("light");
    expect(document.documentElement.getAttribute("data-theme")).toBe("light");
  });

  it("does not expose system as a theme option", async () => {
    const user = userEvent.setup();
    const gateway = new MockSettingsGateway();
    render(
      <SettingsPanel
        gateway={gateway}
        onClose={() => undefined}
        projectId={null}
      />,
    );
    await openCategory(user, "appearance");
    await waitFor(() => {
      expect(screen.getByTestId("settings-theme")).toBeInTheDocument();
    });
    const select = screen.getByTestId("settings-theme");
    const values = [...select.querySelectorAll("option")].map(
      (opt) => (opt as HTMLOptionElement).value,
    );
    expect(values).toEqual(["dark", "light"]);
    expect(values).not.toContain("system");
  });

  it("adapterOptionsForBuild is driven only by host flag", () => {
    expect(adapterOptionsForBuild(false).map((o) => o.value)).toEqual([
      "grok-acp",
    ]);
    expect(adapterOptionsForBuild(true).map((o) => o.value)).toEqual([
      "grok-acp",
      "fake",
    ]);
  });
});
