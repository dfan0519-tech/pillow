/**
 * Runtime slots UI — create API-key flow (secret not in DOM), edit keep-on-blank,
 * external CLI ungoverned copy, probe success/fail, settings panel no regression.
 */
import { render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it } from "vitest";
import { RuntimeSlotsSection } from "../src/features/settings/RuntimeSlotsSection";
import {
  MockRuntimeSlotsGateway,
} from "../src/features/settings/runtimeSlotsGateway";
import {
  EXTERNAL_CLI_UNGOVERNED_BODY,
  EXTERNAL_CLI_UNGOVERNED_HEADING,
  SECRET_CONFIGURED_LABEL,
  type RuntimeSlotView,
} from "../src/features/settings/runtimeSlotsTypes";
import { SettingsPanel } from "../src/features/settings/SettingsPanel";
import { MockSettingsGateway } from "../src/features/settings/settingsGateway";
import "../src/styles/tokens.css";
import "../src/styles/settings.css";
import "../src/styles/runtime-slots.css";

const PROJECT = "project-slots-1";
const NOW = "2026-08-11T12:00:00.000Z";
/**
 * Distinct sentinel used only in create/edit tests — must never appear in DOM.
 *
 * Deliberately not shaped like a real provider key. The value only has to be
 * findable if it leaks; making it look authentic would trip the repository
 * secret gate, and silencing that gate with an allow entry is how a real key
 * eventually walks past it.
 */
const TEST_API_KEY = "pillow-test-sentinel-dom-leak-9f3a2b1c";

function fixtureApiKeySlot(
  overrides: Partial<RuntimeSlotView> = {},
): RuntimeSlotView {
  return {
    slotId: "slot-api-1",
    role: "custom",
    roleCustomName: "未分配",
    projectId: PROJECT,
    displayName: "Grok API",
    kind: "governed_grok",
    governance: "full",
    authMode: "api_key",
    baseUrl: "https://api.example.test/v1",
    modelId: "grok-test",
    secretRef: "secret-ref-opaque",
    secretConfigured: true,
    executablePath: null,
    launchArgs: null,
    statusArgs: null,
    createdAt: NOW,
    updatedAt: NOW,
    ...overrides,
  };
}

function fixtureExternalSlot(
  overrides: Partial<RuntimeSlotView> = {},
): RuntimeSlotView {
  return {
    slotId: "slot-ext-1",
    role: "custom",
    roleCustomName: "未分配",
    projectId: PROJECT,
    displayName: "Codex",
    kind: "external_cli",
    governance: "none",
    authMode: null,
    baseUrl: null,
    modelId: null,
    secretRef: null,
    secretConfigured: false,
    executablePath: "C:\\tools\\codex.exe",
    launchArgs: [],
    statusArgs: ["login", "status"],
    createdAt: NOW,
    updatedAt: NOW,
    ...overrides,
  };
}

/** Assert no DOM text node / attribute value contains the secret. */
function assertSecretAbsentFromDom(secret: string): void {
  expect(document.body.textContent ?? "").not.toContain(secret);
  const html = document.body.innerHTML;
  expect(html).not.toContain(secret);
  // password inputs must be empty after save (draft cleared).
  for (const input of document.querySelectorAll("input")) {
    expect(input.value).not.toBe(secret);
    expect(input.getAttribute("value") ?? "").not.toContain(secret);
  }
}

describe("RuntimeSlotsSection — create API-key flow", () => {
  it("creates an API-key slot and never puts the key in any DOM node after save", async () => {
    const user = userEvent.setup();
    const gateway = new MockRuntimeSlotsGateway();
    render(
      <RuntimeSlotsSection gateway={gateway} projectId={PROJECT} />,
    );

    await screen.findByTestId("runtime-slots-add");
    await user.click(screen.getByTestId("runtime-slots-add"));

    await user.type(screen.getByTestId("runtime-slots-display-name"), "My Grok");
    await user.type(
      screen.getByTestId("runtime-slots-base-url"),
      "https://api.example.test/v1",
    );
    await user.type(screen.getByTestId("runtime-slots-model-id"), "grok-test");
    await user.type(screen.getByTestId("runtime-slots-api-key"), TEST_API_KEY);
    await user.click(screen.getByTestId("runtime-slots-form-save"));

    await waitFor(() => {
      expect(screen.getByTestId("runtime-slots-list")).toBeInTheDocument();
    });

    expect(screen.getByText("My Grok")).toBeInTheDocument();
    expect(screen.getByText(SECRET_CONFIGURED_LABEL)).toBeInTheDocument();
    assertSecretAbsentFromDom(TEST_API_KEY);

    const created = gateway.getSlotsSnapshot()[0]!;
    expect(created.secretConfigured).toBe(true);
    expect(JSON.stringify(created)).not.toContain(TEST_API_KEY);
    expect(gateway.lastCreateApiKeyArgs.at(-1)?.apiKey).toBe(TEST_API_KEY);
  });
});

describe("RuntimeSlotsSection — edit keep secret", () => {
  it("omits apiKey from update gateway args when the edit field is left blank", async () => {
    const user = userEvent.setup();
    const initial = fixtureApiKeySlot();
    const originalSecret = "sk-original-must-keep";
    const gateway = new MockRuntimeSlotsGateway([initial]);
    gateway.seedSecret(initial.secretRef!, originalSecret);

    render(
      <RuntimeSlotsSection gateway={gateway} projectId={PROJECT} />,
    );

    await screen.findByTestId(`runtime-slot-edit-${initial.slotId}`);
    await user.click(screen.getByTestId(`runtime-slot-edit-${initial.slotId}`));

    const name = screen.getByTestId("runtime-slots-display-name");
    await user.clear(name);
    await user.type(name, "Grok API renamed");
    // Leave API key blank.
    expect(screen.getByTestId("runtime-slots-api-key")).toHaveValue("");

    await user.click(screen.getByTestId("runtime-slots-form-save"));

    await waitFor(() => {
      expect(gateway.lastUpdateApiKeyArgs.length).toBeGreaterThan(0);
    });

    const args = gateway.lastUpdateApiKeyArgs.at(-1)!;
    expect(args).not.toHaveProperty("apiKey");
    expect(args.displayName).toBe("Grok API renamed");
    expect(gateway.getMockSecret(initial.secretRef!)).toBe(originalSecret);
    assertSecretAbsentFromDom(originalSecret);
  });
});

describe("RuntimeSlotsSection — external CLI ungoverned copy", () => {
  it("renders the exact ungoverned text (not icon-only)", async () => {
    const gateway = new MockRuntimeSlotsGateway([fixtureExternalSlot()]);
    render(
      <RuntimeSlotsSection gateway={gateway} projectId={PROJECT} />,
    );

    const banner = await screen.findByTestId("runtime-slot-ungoverned-slot-ext-1");
    expect(banner).toHaveTextContent(EXTERNAL_CLI_UNGOVERNED_HEADING);
    expect(banner).toHaveTextContent(EXTERNAL_CLI_UNGOVERNED_BODY);
    expect(banner).toHaveTextContent("不经过提案与审批");

    // Create form also shows the same warning before save.
    const user = userEvent.setup();
    await user.click(screen.getByTestId("runtime-slots-add"));
    await user.click(screen.getByTestId("runtime-slots-kind-external"));
    const createWarn = screen.getByTestId(
      "runtime-slots-create-ungoverned-warning",
    );
    expect(createWarn).toHaveTextContent(EXTERNAL_CLI_UNGOVERNED_HEADING);
    expect(createWarn).toHaveTextContent("不经过提案与审批");
  });
});

describe("RuntimeSlotsSection — probe paths", () => {
  it("shows success for governed Grok start probe", async () => {
    const user = userEvent.setup();
    const slot = fixtureApiKeySlot();
    const gateway = new MockRuntimeSlotsGateway([slot]);
    gateway.setProbeMode("success");
    render(
      <RuntimeSlotsSection gateway={gateway} projectId={PROJECT} />,
    );

    const probeBtn = await screen.findByTestId(
      `runtime-slot-probe-${slot.slotId}`,
    );
    expect(probeBtn).toHaveTextContent("启动");
    expect(probeBtn.className).toMatch(/runtime-slots-start--primary/);
    expect(
      screen.getByTestId(`runtime-slot-needs-start-${slot.slotId}`),
    ).toHaveTextContent(/未启动/);
    await user.click(probeBtn);
    const result = await screen.findByTestId(
      `runtime-slot-probe-result-${slot.slotId}`,
    );
    expect(result).toHaveTextContent(/探活成功/u);
    expect(result).toHaveTextContent(/active/u);
    expect(
      await screen.findByTestId(`runtime-slot-started-${slot.slotId}`),
    ).toHaveTextContent("已启动");
    expect(probeBtn).toHaveTextContent("重新启动");
  });

  it("shows actionable failure for governed Grok probe without host secret text", async () => {
    const user = userEvent.setup();
    const slot = fixtureApiKeySlot();
    const gateway = new MockRuntimeSlotsGateway([slot]);
    gateway.setProbeMode("fail", "runtime_slot_secret_missing");
    render(
      <RuntimeSlotsSection gateway={gateway} projectId={PROJECT} />,
    );

    const probeBtn = await screen.findByTestId(
      `runtime-slot-probe-${slot.slotId}`,
    );
    expect(probeBtn).toHaveTextContent("启动");
    await user.click(probeBtn);
    const result = await screen.findByTestId(
      `runtime-slot-probe-result-${slot.slotId}`,
    );
    expect(result).toHaveTextContent(/探活失败/u);
    expect(result).toHaveTextContent(/认证|密钥/u);
    expect(result.textContent ?? "").not.toMatch(/sk-|apiKey=/i);
    // Failed start must not claim the slot is running.
    expect(
      screen.queryByTestId(`runtime-slot-started-${slot.slotId}`),
    ).toBeNull();
    expect(
      screen.getByTestId(`runtime-slot-needs-start-${slot.slotId}`),
    ).toBeVisible();
  });

  it("shows external CLI status stdout on success and failure summary on fail", async () => {
    const user = userEvent.setup();
    const slot = fixtureExternalSlot();
    const gateway = new MockRuntimeSlotsGateway([slot]);
    gateway.setExternalStatusMode("success", "Logged in using ChatGPT");
    render(
      <RuntimeSlotsSection gateway={gateway} projectId={PROJECT} />,
    );

    const probeBtn = await screen.findByTestId(
      `runtime-slot-probe-${slot.slotId}`,
    );
    // external_cli must never get a Start affordance.
    expect(probeBtn).toHaveTextContent("查询账号状态");
    expect(probeBtn).not.toHaveTextContent("启动");
    expect(
      screen.queryByTestId(`runtime-slot-needs-start-${slot.slotId}`),
    ).toBeNull();
    await user.click(probeBtn);
    let result = await screen.findByTestId(
      `runtime-slot-probe-result-${slot.slotId}`,
    );
    expect(result).toHaveTextContent("Logged in using ChatGPT");
    expect(result).toHaveTextContent(/成功/u);

    gateway.setExternalStatusMode("fail");
    await user.click(probeBtn);
    result = await screen.findByTestId(
      `runtime-slot-probe-result-${slot.slotId}`,
    );
    await waitFor(() => {
      expect(result).toHaveTextContent(/失败/u);
    });
  });
});

describe("SettingsPanel — slots section without regressing existing controls", () => {
  it("still offers permission mode and theme while showing slots for a project", async () => {
    const user = userEvent.setup();
    const settings = new MockSettingsGateway();
    const slots = new MockRuntimeSlotsGateway([
      fixtureApiKeySlot(),
      fixtureExternalSlot(),
    ]);
    render(
      <SettingsPanel
        gateway={settings}
        onClose={() => undefined}
        projectId={PROJECT}
        runtimeSlotsGateway={slots}
      />,
    );

    // Existing settings controls.
    expect(await screen.findByTestId("settings-permission-mode")).toBeVisible();
    expect(screen.getByTestId("settings-theme")).toBeVisible();
    expect(screen.getByTestId("settings-adapter")).toBeVisible();
    expect(screen.getByTestId("settings-grok-executable")).toBeVisible();
    expect(screen.getByTestId("settings-save")).toBeVisible();

    // Slots section present with list.
    const section = await screen.findByTestId("runtime-slots-section");
    expect(within(section).getByText("运行时槽位")).toBeVisible();
    expect(within(section).getByText("Grok API")).toBeVisible();
    expect(within(section).getByText("Codex")).toBeVisible();
    expect(
      within(section).getByTestId("runtime-slot-ungoverned-slot-ext-1"),
    ).toHaveTextContent(EXTERNAL_CLI_UNGOVERNED_HEADING);

    // Save existing settings still works.
    await user.selectOptions(screen.getByTestId("settings-theme"), "light");
    await user.click(screen.getByTestId("settings-save"));
    await waitFor(() => {
      expect(screen.getByTestId("settings-notice")).toBeVisible();
    });
  });

  it("shows no-project hint when projectId is null", async () => {
    render(
      <SettingsPanel
        gateway={new MockSettingsGateway()}
        onClose={() => undefined}
        projectId={null}
        runtimeSlotsGateway={new MockRuntimeSlotsGateway()}
      />,
    );
    expect(await screen.findByTestId("runtime-slots-no-project")).toBeVisible();
  });
});
