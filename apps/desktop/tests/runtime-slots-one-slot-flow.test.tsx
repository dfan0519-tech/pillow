/**
 * One-slot flow: name → auth → role via createSlot;
 * guidance (no slots / no controller); CLI cannot be controller (+/−);
 * role reassignment; dual controller rejected with human copy.
 */
import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it, vi } from "vitest";
import { RuntimeSlotsSection } from "../src/features/settings/RuntimeSlotsSection";
import {
  MockRuntimeSlotsGateway,
  mapRuntimeSlotErrorMessage,
} from "../src/features/settings/runtimeSlotsGateway";
import {
  CONTROLLER_REQUIRES_GOVERNED_AUTH_HINT,
  GUIDANCE_NO_CONTROLLER_BODY,
  GUIDANCE_NO_CONTROLLER_HEADING,
  GUIDANCE_NO_SLOTS_BODY,
  GUIDANCE_NO_SLOTS_HEADING,
  SLOT_AUTH_METHODS,
  type RuntimeSlotView,
  RuntimeSlotsGatewayError,
} from "../src/features/settings/runtimeSlotsTypes";
import "../src/styles/tokens.css";
import "../src/styles/settings.css";
import "../src/styles/runtime-slots.css";

const PROJECT = "project-one-slot-1";
const NOW = "2026-08-12T12:00:00.000Z";
const TEST_API_KEY = "pillow-test-sentinel-dom-leak-one-slot-7c4e";

function fixtureApiKeySlot(
  overrides: Partial<RuntimeSlotView> = {},
): RuntimeSlotView {
  return {
    slotId: "slot-api-1",
    role: "custom",
    roleCustomName: "写码",
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
    accountLabel: null,
    accessExpiresAt: null,
    ...overrides,
  };
}

function fixtureExternalSlot(
  overrides: Partial<RuntimeSlotView> = {},
): RuntimeSlotView {
  return {
    slotId: "slot-ext-1",
    role: "custom",
    roleCustomName: "CLI 助手",
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
    accountLabel: null,
    accessExpiresAt: null,
    ...overrides,
  };
}

function assertSecretAbsentFromDom(secret: string): void {
  expect(document.body.textContent ?? "").not.toContain(secret);
  const html = document.body.innerHTML;
  expect(html).not.toContain(secret);
  for (const input of document.querySelectorAll("input")) {
    expect(input.value).not.toBe(secret);
    expect(input.getAttribute("value") ?? "").not.toContain(secret);
  }
}

describe("One-slot flow — unified create entry", () => {
  it("has a single 新建槽位 control and data-driven auth methods (no kind forks)", async () => {
    const user = userEvent.setup();
    const gateway = new MockRuntimeSlotsGateway();
    render(
      <RuntimeSlotsSection gateway={gateway} projectId={PROJECT} />,
    );

    const addButtons = await screen.findAllByTestId("runtime-slots-add");
    expect(addButtons).toHaveLength(1);
    expect(addButtons[0]).toHaveTextContent("新建槽位");

    await user.click(addButtons[0]!);

    // Step 1: provider picker, not the old four global auth tabs.
    expect(screen.getByTestId("runtime-slots-provider-step")).toBeInTheDocument();
    expect(screen.queryByTestId("runtime-slots-auth-api_key")).toBeNull();
    expect(screen.queryByTestId("runtime-slots-auth-oauth")).toBeNull();
    expect(screen.queryByTestId("runtime-slots-auth-local_cli")).toBeNull();

    await user.click(screen.getByTestId("runtime-slots-provider-pick-custom"));
    expect(screen.getByTestId("runtime-slots-auth-api_key")).toHaveTextContent(
      "API key",
    );
    expect(screen.queryByTestId("runtime-slots-auth-oauth")).toBeNull();
    expect(screen.queryByTestId("runtime-slots-auth-local_cli")).toBeNull();

    // Negative: legacy kind forks must not appear.
    expect(screen.queryByTestId("runtime-slots-kind-grok")).toBeNull();
    expect(screen.queryByTestId("runtime-slots-kind-external")).toBeNull();
    expect(screen.queryByTestId("runtime-slots-auth-login")).toBeNull();
    expect(SLOT_AUTH_METHODS.length).toBeGreaterThan(0);
  });

  it("creates via createSlot once with name, auth, and role (API key)", async () => {
    const user = userEvent.setup();
    const gateway = new MockRuntimeSlotsGateway();
    const createSpy = vi.spyOn(gateway, "createSlot");
    const legacyLogin = vi.spyOn(gateway, "createGrokLoginSlot");
    const legacyApi = vi.spyOn(gateway, "createGrokApiKeySlot");
    const legacyCli = vi.spyOn(gateway, "createExternalCliSlot");

    render(
      <RuntimeSlotsSection gateway={gateway} projectId={PROJECT} />,
    );

    await user.click(await screen.findByTestId("runtime-slots-add"));
    await user.type(screen.getByTestId("runtime-slots-display-name"), "My Grok");
    await user.click(screen.getByTestId("runtime-slots-provider-pick-custom"));
    await user.click(screen.getByTestId("runtime-slots-auth-api_key"));
    await user.type(
      screen.getByTestId("runtime-slots-base-url"),
      "https://api.example.test/v1",
    );
    await user.type(screen.getByTestId("runtime-slots-model-id"), "grok-test");
    await user.type(screen.getByTestId("runtime-slots-api-key"), TEST_API_KEY);
    await user.click(screen.getByTestId("runtime-slots-role-controller"));
    await user.click(screen.getByTestId("runtime-slots-form-save"));

    await waitFor(() => {
      expect(screen.getByTestId("runtime-slots-list")).toBeInTheDocument();
    });

    expect(createSpy).toHaveBeenCalledTimes(1);
    expect(legacyLogin).not.toHaveBeenCalled();
    expect(legacyApi).not.toHaveBeenCalled();
    expect(legacyCli).not.toHaveBeenCalled();

    const args = gateway.lastCreateSlotArgs.at(-1)!;
    expect(args).toMatchObject({
      projectId: PROJECT,
      displayName: "My Grok",
      authKind: "api_key",
      role: "controller",
      baseUrl: "https://api.example.test/v1",
      modelId: "grok-test",
    });
    expect(args.apiKey).toBe(TEST_API_KEY);
    assertSecretAbsentFromDom(TEST_API_KEY);
    expect(screen.getByText("总控")).toBeInTheDocument();
  });

  it("does not paint a slot when createSlot fails (no optimistic success)", async () => {
    const user = userEvent.setup();
    const gateway = new MockRuntimeSlotsGateway();
    vi.spyOn(gateway, "createSlot").mockRejectedValueOnce(
      new RuntimeSlotsGatewayError(
        "runtime_slot_input_invalid",
        mapRuntimeSlotErrorMessage("runtime_slot_input_invalid"),
      ),
    );

    render(
      <RuntimeSlotsSection gateway={gateway} projectId={PROJECT} />,
    );

    await user.click(await screen.findByTestId("runtime-slots-add"));
    await user.type(screen.getByTestId("runtime-slots-display-name"), "Broken");
    await user.click(screen.getByTestId("runtime-slots-provider-pick-custom"));
    await user.click(screen.getByTestId("runtime-slots-auth-api_key"));
    await user.type(
      screen.getByTestId("runtime-slots-base-url"),
      "https://api.example.test/v1",
    );
    await user.type(screen.getByTestId("runtime-slots-model-id"), "m");
    await user.type(screen.getByTestId("runtime-slots-api-key"), TEST_API_KEY);
    await user.click(screen.getByTestId("runtime-slots-role-custom"));
    await user.type(screen.getByTestId("runtime-slots-role-custom-name"), "x");
    await user.click(screen.getByTestId("runtime-slots-form-save"));

    await waitFor(() => {
      expect(screen.getByTestId("runtime-slots-error")).toBeVisible();
    });
    expect(screen.queryByTestId("runtime-slots-list")).toBeNull();
    expect(screen.queryByText("Broken")).toBeNull();
    expect(gateway.getSlotsSnapshot()).toHaveLength(0);
    assertSecretAbsentFromDom(TEST_API_KEY);
  });
});

describe("One-slot flow — T3 guidance (no slots vs no controller)", () => {
  it("shows no-slots guidance when empty; hides no-controller guidance", async () => {
    const gateway = new MockRuntimeSlotsGateway();
    render(
      <RuntimeSlotsSection gateway={gateway} projectId={PROJECT} />,
    );

    const banner = await screen.findByTestId("runtime-slots-guidance-no-slots");
    expect(banner).toHaveTextContent(GUIDANCE_NO_SLOTS_HEADING);
    expect(banner).toHaveTextContent(GUIDANCE_NO_SLOTS_BODY);
    expect(banner).toHaveTextContent("无法在 Pillow 里对话");

    // Negative: the other banner must not appear.
    expect(
      screen.queryByTestId("runtime-slots-guidance-no-controller"),
    ).toBeNull();
    expect(screen.queryByText(GUIDANCE_NO_CONTROLLER_HEADING)).toBeNull();
  });

  it("shows no-controller guidance when slots exist without controller; hides no-slots", async () => {
    const gateway = new MockRuntimeSlotsGateway([
      fixtureApiKeySlot({ role: "custom", roleCustomName: "写码" }),
      fixtureExternalSlot(),
    ]);
    render(
      <RuntimeSlotsSection gateway={gateway} projectId={PROJECT} />,
    );

    const banner = await screen.findByTestId(
      "runtime-slots-guidance-no-controller",
    );
    expect(banner).toHaveTextContent(GUIDANCE_NO_CONTROLLER_HEADING);
    expect(banner).toHaveTextContent(GUIDANCE_NO_CONTROLLER_BODY);
    expect(banner).toHaveTextContent("当前已有槽位");

    // Negative: empty-state banner must not appear.
    expect(screen.queryByTestId("runtime-slots-guidance-no-slots")).toBeNull();
    expect(screen.queryByText(GUIDANCE_NO_SLOTS_HEADING)).toBeNull();
  });

  it("hides both controller-missing banners when a controller exists", async () => {
    const gateway = new MockRuntimeSlotsGateway([
      fixtureApiKeySlot({ role: "controller", roleCustomName: null }),
    ]);
    render(
      <RuntimeSlotsSection gateway={gateway} projectId={PROJECT} />,
    );

    await screen.findByTestId("runtime-slots-list");
    expect(screen.queryByTestId("runtime-slots-guidance-no-slots")).toBeNull();
    expect(
      screen.queryByTestId("runtime-slots-guidance-no-controller"),
    ).toBeNull();
    // Soft librarian suggestion still ok when no librarian.
    expect(
      screen.getByTestId("runtime-slots-guidance-librarian"),
    ).toBeVisible();
  });
});

describe("One-slot flow — CLI cannot be controller (+/−)", () => {
  it("disables 总控 when 本机 CLI is selected and shows the reason", async () => {
    const user = userEvent.setup();
    const gateway = new MockRuntimeSlotsGateway();
    render(
      <RuntimeSlotsSection gateway={gateway} projectId={PROJECT} />,
    );

    await user.click(await screen.findByTestId("runtime-slots-add"));
    await user.click(screen.getByTestId("runtime-slots-provider-pick-local_cli"));

    const controllerBtn = screen.getByTestId("runtime-slots-role-controller");
    expect(controllerBtn).toBeDisabled();
    expect(
      screen.getByTestId("runtime-slots-controller-cli-disabled"),
    ).toHaveTextContent(CONTROLLER_REQUIRES_GOVERNED_AUTH_HINT);

    // Librarian / custom remain available.
    expect(screen.getByTestId("runtime-slots-role-librarian")).not.toBeDisabled();
    expect(screen.getByTestId("runtime-slots-role-custom")).not.toBeDisabled();
  });

  it("allows 总控 when API key is selected (positive counterpart)", async () => {
    const user = userEvent.setup();
    const gateway = new MockRuntimeSlotsGateway();
    render(
      <RuntimeSlotsSection gateway={gateway} projectId={PROJECT} />,
    );

    await user.click(await screen.findByTestId("runtime-slots-add"));
    await user.click(screen.getByTestId("runtime-slots-provider-pick-custom"));
    await user.click(screen.getByTestId("runtime-slots-auth-api_key"));

    const controllerBtn = screen.getByTestId("runtime-slots-role-controller");
    expect(controllerBtn).not.toBeDisabled();
    // Negative: CLI-only hint must not show for governed auth.
    expect(
      screen.queryByTestId("runtime-slots-controller-cli-disabled"),
    ).toBeNull();

    await user.click(controllerBtn);
    expect(controllerBtn).toHaveAttribute("aria-pressed", "true");
  });

  it("re-disables 总控 when switching from API key to CLI (auto-downgrade)", async () => {
    const user = userEvent.setup();
    const gateway = new MockRuntimeSlotsGateway();
    render(
      <RuntimeSlotsSection gateway={gateway} projectId={PROJECT} />,
    );

    await user.click(await screen.findByTestId("runtime-slots-add"));
    await user.click(screen.getByTestId("runtime-slots-provider-pick-custom"));
    await user.click(screen.getByTestId("runtime-slots-auth-api_key"));
    await user.click(screen.getByTestId("runtime-slots-role-controller"));
    expect(
      screen.getByTestId("runtime-slots-role-controller"),
    ).toHaveAttribute("aria-pressed", "true");

    await user.click(screen.getByTestId("runtime-slots-form-back"));
    await user.click(screen.getByTestId("runtime-slots-provider-pick-local_cli"));
    expect(
      screen.getByTestId("runtime-slots-role-controller"),
    ).toBeDisabled();
    expect(
      screen.getByTestId("runtime-slots-role-controller"),
    ).toHaveAttribute("aria-pressed", "false");
    expect(
      screen.getByTestId("runtime-slots-controller-cli-disabled"),
    ).toBeVisible();
  });
});

describe("One-slot flow — role reassignment (T2)", () => {
  it("changes an existing governed slot role without delete/recreate", async () => {
    const user = userEvent.setup();
    const slot = fixtureApiKeySlot({
      role: "custom",
      roleCustomName: "写码",
    });
    const gateway = new MockRuntimeSlotsGateway([slot]);
    const createSpy = vi.spyOn(gateway, "createSlot");
    const updateSpy = vi.spyOn(gateway, "updateSlot");

    render(
      <RuntimeSlotsSection gateway={gateway} projectId={PROJECT} />,
    );

    await user.click(
      await screen.findByTestId(`runtime-slot-role-edit-${slot.slotId}`),
    );
    await user.click(
      screen.getByTestId(
        `runtime-slot-role-option-${slot.slotId}-controller`,
      ),
    );
    await user.click(
      screen.getByTestId(`runtime-slot-role-save-${slot.slotId}`),
    );

    await waitFor(() => {
      expect(updateSpy).toHaveBeenCalledTimes(1);
    });
    expect(createSpy).not.toHaveBeenCalled();
    expect(gateway.lastUpdateSlotArgs.at(-1)).toMatchObject({
      slotId: slot.slotId,
      role: "controller",
      keepAuth: true,
    });
    await waitFor(() => {
      expect(
        screen.getByTestId(`runtime-slot-role-${slot.slotId}`),
      ).toHaveTextContent("总控");
    });
  });

  it("refuses controller on an existing CLI slot (+ and UI disable)", async () => {
    const user = userEvent.setup();
    const slot = fixtureExternalSlot();
    const gateway = new MockRuntimeSlotsGateway([slot]);

    render(
      <RuntimeSlotsSection gateway={gateway} projectId={PROJECT} />,
    );

    await user.click(
      await screen.findByTestId(`runtime-slot-role-edit-${slot.slotId}`),
    );
    const controllerOpt = screen.getByTestId(
      `runtime-slot-role-option-${slot.slotId}-controller`,
    );
    expect(controllerOpt).toBeDisabled();
    expect(
      screen.getByTestId(`runtime-slot-controller-cli-hint-${slot.slotId}`),
    ).toHaveTextContent(CONTROLLER_REQUIRES_GOVERNED_AUTH_HINT);

    // Librarian still selectable (positive for non-controller roles).
    expect(
      screen.getByTestId(
        `runtime-slot-role-option-${slot.slotId}-librarian`,
      ),
    ).not.toBeDisabled();
  });
});

describe("One-slot flow — dual controller human error", () => {
  it("surfaces controller_already_exists as human copy and keeps existing slot", async () => {
    const user = userEvent.setup();
    const existing = fixtureApiKeySlot({
      slotId: "slot-ctrl",
      role: "controller",
      roleCustomName: null,
      displayName: "Existing Controller",
    });
    const gateway = new MockRuntimeSlotsGateway([existing]);

    render(
      <RuntimeSlotsSection gateway={gateway} projectId={PROJECT} />,
    );

    await user.click(await screen.findByTestId("runtime-slots-add"));
    await user.type(screen.getByTestId("runtime-slots-display-name"), "Second");
    await user.click(screen.getByTestId("runtime-slots-provider-pick-custom"));
    await user.click(screen.getByTestId("runtime-slots-auth-api_key"));
    await user.type(
      screen.getByTestId("runtime-slots-base-url"),
      "https://api.example.test/v1",
    );
    await user.type(screen.getByTestId("runtime-slots-model-id"), "m2");
    await user.type(screen.getByTestId("runtime-slots-api-key"), TEST_API_KEY);
    await user.click(screen.getByTestId("runtime-slots-role-controller"));
    await user.click(screen.getByTestId("runtime-slots-form-save"));

    const alert = await screen.findByTestId("runtime-slots-error");
    expect(alert).toHaveTextContent(
      mapRuntimeSlotErrorMessage("runtime_slot_controller_already_exists"),
    );
    // Still only the original controller — no silent second slot.
    expect(gateway.getSlotsSnapshot()).toHaveLength(1);
    expect(screen.getByText("Existing Controller")).toBeInTheDocument();
    expect(screen.queryByText("Second")).toBeNull();
    assertSecretAbsentFromDom(TEST_API_KEY);
  });
});
