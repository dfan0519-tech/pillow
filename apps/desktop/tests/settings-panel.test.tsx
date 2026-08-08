/**
 * S0 settings UI: env override is visible + read-only; no secret fields.
 */
import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it } from "vitest";
import { App } from "../src/App";
import { MockSettingsGateway } from "../src/features/settings/settingsGateway";
import { SettingsPanel } from "../src/features/settings/SettingsPanel";
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
    expect(select.querySelectorAll("option").length).toBe(2);
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
});
