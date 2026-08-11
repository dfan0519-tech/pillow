/**
 * S0 settings UI: env override is visible + read-only; no secret fields.
 */
import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it } from "vitest";
import { App } from "../src/App";
import {
  MockSettingsGateway,
  parsePermissionModeSettings,
} from "../src/features/settings/settingsGateway";
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

describe("SettingsPanel", () => {
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

  it("offers exactly three global permission modes and saves the selection", async () => {
    const user = userEvent.setup();
    const gateway = new MockSettingsGateway();
    const applied: string[] = [];
    render(
      <SettingsPanel
        gateway={gateway}
        onClose={() => undefined}
        onPermissionModeChanged={(mode) => applied.push(mode)}
        projectId={null}
      />,
    );

    const select = await screen.findByTestId("settings-permission-mode");
    expect([...select.querySelectorAll("option")].map((option) => option.value)).toEqual([
      "default",
      "accept_edits",
      "bypass",
    ]);
    await user.selectOptions(select, "accept_edits");
    await user.click(screen.getByTestId("settings-save"));
    await waitFor(() => {
      expect(gateway.getPermissionModeSnapshot().mode).toBe("accept_edits");
    });
    expect(applied).toContain("accept_edits");
  });

  it("keeps permission mode global-only when project scope is selected", async () => {
    const user = userEvent.setup();
    const gateway = new MockSettingsGateway();
    render(
      <SettingsPanel
        gateway={gateway}
        onClose={() => undefined}
        projectId="project-1"
      />,
    );

    await screen.findByTestId("settings-permission-mode");
    await user.click(screen.getByRole("button", { name: "当前项目" }));
    expect(screen.getByTestId("settings-permission-mode")).toBeDisabled();
    expect(screen.getByTestId("settings-permission-mode-global-only")).toBeVisible();
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

  it("shows env override state and keeps the field read-only", async () => {
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
    const gateway = new MockSettingsGateway();
    render(
      <SettingsPanel
        gateway={gateway}
        onClose={() => undefined}
        projectId={null}
      />,
    );
    await waitFor(() => {
      expect(screen.getByTestId("settings-adapter")).toBeInTheDocument();
    });
    const select = screen.getByTestId("settings-adapter");
    expect(select.tagName).toBe("SELECT");
    // Dev mock: Fake selectable → two options (host flag, not env hard-code).
    expect(select.querySelectorAll("option").length).toBe(2);
  });

  it("hides Fake when host reports fakeAdapterSelectable false (release gate)", async () => {
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
    const gateway = new MockSettingsGateway();
    gateway.setEnvOverride("theme", "light", "PILLOW_THEME");

    render(
      <SettingsPanel
        gateway={gateway}
        onClose={() => undefined}
        projectId={null}
      />,
    );

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
    const gateway = new MockSettingsGateway();
    render(
      <SettingsPanel
        gateway={gateway}
        onClose={() => undefined}
        projectId={null}
      />,
    );
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
