/**
 * Slot model picker — catalog three states + runtimeRestarted honesty.
 * Positive and negative pins: available lists+selects; unsupported falls back
 * to manual; fail shows error + manual; external_cli gets no picker.
 */
import { render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it } from "vitest";
import { RuntimeSlotModelPicker } from "../src/features/settings/RuntimeSlotModelPicker";
import { RuntimeSlotsSection } from "../src/features/settings/RuntimeSlotsSection";
import { MockRuntimeSlotsGateway } from "../src/features/settings/runtimeSlotsGateway";
import {
  MODEL_CATALOG_UNSUPPORTED_ENDPOINT,
  MODEL_SWITCH_RESTARTED_MESSAGE,
  MODEL_SWITCH_SAVED_INACTIVE_MESSAGE,
  type RuntimeSlotView,
} from "../src/features/settings/runtimeSlotsTypes";
import "../src/styles/tokens.css";
import "../src/styles/settings.css";
import "../src/styles/runtime-slots.css";

const PROJECT = "project-model-picker-1";
const NOW = "2026-08-11T12:00:00.000Z";

function fixtureApiKeySlot(
  overrides: Partial<RuntimeSlotView> = {},
): RuntimeSlotView {
  return {
    slotId: "slot-api-model-1",
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

function fixtureLoginSlot(
  overrides: Partial<RuntimeSlotView> = {},
): RuntimeSlotView {
  return {
    slotId: "slot-login-model-1",
    role: "custom",
    roleCustomName: "未分配",
    projectId: PROJECT,
    displayName: "Grok Login",
    kind: "governed_grok",
    governance: "full",
    authMode: "login",
    baseUrl: null,
    modelId: null,
    secretRef: null,
    secretConfigured: false,
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
    slotId: "slot-ext-model-1",
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

describe("RuntimeSlotModelPicker — catalog available", () => {
  it("lists catalog models and selects one via setRuntimeSlotModel", async () => {
    const user = userEvent.setup();
    const slot = fixtureApiKeySlot({ modelId: "grok-test" });
    const gateway = new MockRuntimeSlotsGateway([slot]);
    gateway.setCatalogMode("available", {
      models: ["grok-test", "grok-beta", "custom-listed"],
    });
    gateway.setModelSwitchRestarted(false);

    const switched: string[] = [];
    render(
      <RuntimeSlotModelPicker
        currentModelId={slot.modelId}
        gateway={gateway}
        onSwitched={(result) => {
          switched.push(result.modelId);
        }}
        projectId={PROJECT}
        slotId={slot.slotId}
      />,
    );

    const select = await screen.findByTestId(
      `runtime-slot-model-select-${slot.slotId}`,
    );
    // Positive: real options from catalog, not just "rendered without crash".
    expect(within(select as HTMLElement).getByRole("option", {
      name: "grok-test",
    })).toBeInTheDocument();
    expect(within(select as HTMLElement).getByRole("option", {
      name: "grok-beta",
    })).toBeInTheDocument();
    expect(within(select as HTMLElement).getByRole("option", {
      name: "custom-listed",
    })).toBeInTheDocument();

    await user.selectOptions(select, "grok-beta");

    await waitFor(() => {
      expect(gateway.lastSetModelArgs.at(-1)).toEqual({
        projectId: PROJECT,
        slotId: slot.slotId,
        modelId: "grok-beta",
      });
    });
    expect(switched).toEqual(["grok-beta"]);
    expect(gateway.getSlotsSnapshot()[0]!.modelId).toBe("grok-beta");

    const notice = await screen.findByTestId(
      `runtime-slot-model-switch-notice-${slot.slotId}`,
    );
    expect(notice).toHaveTextContent(MODEL_SWITCH_SAVED_INACTIVE_MESSAGE);
    // Must not imply current session already uses the new model.
    expect(notice.textContent ?? "").not.toMatch(/已生效于当前会话|会话内立即生效/u);
  });

  it("allows manual entry for an unlisted model id while catalog is available", async () => {
    const user = userEvent.setup();
    const slot = fixtureApiKeySlot({ modelId: "grok-test" });
    const gateway = new MockRuntimeSlotsGateway([slot]);
    gateway.setCatalogMode("available", {
      models: ["grok-test", "grok-beta"],
    });

    render(
      <RuntimeSlotModelPicker
        currentModelId={slot.modelId}
        gateway={gateway}
        projectId={PROJECT}
        slotId={slot.slotId}
      />,
    );

    const select = await screen.findByTestId(
      `runtime-slot-model-select-${slot.slotId}`,
    );
    await user.selectOptions(select, "__pillow_custom_model__");

    const manual = await screen.findByTestId(
      `runtime-slot-model-manual-${slot.slotId}`,
    );
    await user.clear(manual);
    await user.type(manual, "my-private-finetune-9");
    await user.click(
      screen.getByTestId(`runtime-slot-model-apply-${slot.slotId}`),
    );

    await waitFor(() => {
      expect(gateway.lastSetModelArgs.at(-1)?.modelId).toBe(
        "my-private-finetune-9",
      );
    });
    expect(gateway.getSlotsSnapshot()[0]!.modelId).toBe("my-private-finetune-9");
  });
});

describe("RuntimeSlotModelPicker — catalog unsupported", () => {
  it("falls back to manual input with endpoint reason (not an error)", async () => {
    const user = userEvent.setup();
    const slot = fixtureApiKeySlot();
    const gateway = new MockRuntimeSlotsGateway([slot]);
    gateway.setCatalogMode("unsupported_endpoint");

    render(
      <RuntimeSlotModelPicker
        currentModelId={slot.modelId}
        gateway={gateway}
        projectId={PROJECT}
        slotId={slot.slotId}
      />,
    );

    const unsupported = await screen.findByTestId(
      `runtime-slot-model-catalog-unsupported-${slot.slotId}`,
    );
    expect(unsupported).toHaveTextContent(MODEL_CATALOG_UNSUPPORTED_ENDPOINT);
    expect(unsupported).toHaveTextContent(/手输/u);

    // Positive fallback: manual field present; select must not claim a catalog.
    expect(
      screen.queryByTestId(`runtime-slot-model-select-${slot.slotId}`),
    ).not.toBeInTheDocument();
    const manual = screen.getByTestId(
      `runtime-slot-model-manual-${slot.slotId}`,
    );
    await user.clear(manual);
    await user.type(manual, "hand-typed-model");
    await user.click(
      screen.getByTestId(`runtime-slot-model-apply-${slot.slotId}`),
    );

    await waitFor(() => {
      expect(gateway.lastSetModelArgs.at(-1)?.modelId).toBe("hand-typed-model");
    });
    // Unsupported is not a failure banner.
    expect(
      screen.queryByTestId(`runtime-slot-model-catalog-error-${slot.slotId}`),
    ).not.toBeInTheDocument();
  });
});

describe("RuntimeSlotModelPicker — catalog fail", () => {
  it("shows failure copy, keeps manual entry, and refresh can recover", async () => {
    const user = userEvent.setup();
    const slot = fixtureApiKeySlot();
    const gateway = new MockRuntimeSlotsGateway([slot]);
    gateway.setCatalogMode("fail", {
      failCode: "runtime_slot_model_catalog_unavailable",
    });

    render(
      <RuntimeSlotModelPicker
        currentModelId={slot.modelId}
        gateway={gateway}
        projectId={PROJECT}
        slotId={slot.slotId}
      />,
    );

    const error = await screen.findByTestId(
      `runtime-slot-model-catalog-error-${slot.slotId}`,
    );
    expect(error).toHaveTextContent(/无法拉取模型目录|手输/u);
    expect(
      screen.getByTestId(`runtime-slot-model-manual-${slot.slotId}`),
    ).toBeInTheDocument();
    expect(
      screen.queryByTestId(`runtime-slot-model-select-${slot.slotId}`),
    ).not.toBeInTheDocument();

    // Recover via refresh after host becomes available.
    // Include the current model so the picker can exit manual fallback.
    gateway.setCatalogMode("available", {
      models: ["grok-test", "recovered-a", "recovered-b"],
    });
    await user.click(
      screen.getByTestId(`runtime-slot-model-refresh-${slot.slotId}`),
    );

    const select = await screen.findByTestId(
      `runtime-slot-model-select-${slot.slotId}`,
    );
    expect(within(select as HTMLElement).getByRole("option", {
      name: "recovered-a",
    })).toBeInTheDocument();
    expect(within(select as HTMLElement).getByRole("option", {
      name: "recovered-b",
    })).toBeInTheDocument();
    expect(
      screen.queryByTestId(`runtime-slot-model-catalog-error-${slot.slotId}`),
    ).not.toBeInTheDocument();
  });
});

describe("RuntimeSlotModelPicker — runtimeRestarted honesty", () => {
  it("states runtime restarted when host returns runtimeRestarted true", async () => {
    const user = userEvent.setup();
    const slot = fixtureApiKeySlot({ modelId: "model-old" });
    const gateway = new MockRuntimeSlotsGateway([slot]);
    gateway.setCatalogMode("available", {
      models: ["model-old", "model-new"],
    });
    gateway.setModelSwitchRestarted(true);

    render(
      <RuntimeSlotModelPicker
        currentModelId="model-old"
        gateway={gateway}
        projectId={PROJECT}
        slotId={slot.slotId}
      />,
    );

    const select = await screen.findByTestId(
      `runtime-slot-model-select-${slot.slotId}`,
    );
    await user.selectOptions(select, "model-new");

    const notice = await screen.findByTestId(
      `runtime-slot-model-switch-notice-${slot.slotId}`,
    );
    expect(notice).toHaveTextContent(MODEL_SWITCH_RESTARTED_MESSAGE);
    expect(notice).toHaveTextContent(/已重启/u);
    expect(notice).toHaveTextContent(/无法恢复/u);
  });

  it("states saved-for-next-start when host returns runtimeRestarted false", async () => {
    const user = userEvent.setup();
    const slot = fixtureLoginSlot({ modelId: null });
    const gateway = new MockRuntimeSlotsGateway([slot]);
    gateway.setCatalogMode("available", {
      models: ["grok-4.5", "grok-beta"],
    });
    gateway.setModelSwitchRestarted(false);

    render(
      <RuntimeSlotModelPicker
        currentModelId={null}
        gateway={gateway}
        projectId={PROJECT}
        slotId={slot.slotId}
      />,
    );

    const select = await screen.findByTestId(
      `runtime-slot-model-select-${slot.slotId}`,
    );
    await user.selectOptions(select, "grok-4.5");

    const notice = await screen.findByTestId(
      `runtime-slot-model-switch-notice-${slot.slotId}`,
    );
    expect(notice).toHaveTextContent(MODEL_SWITCH_SAVED_INACTIVE_MESSAGE);
    expect(notice).toHaveTextContent(/未激活/u);
    expect(notice).toHaveTextContent(/下次启动/u);
    // Negative: must not claim a live session restart.
    expect(notice.textContent ?? "").not.toMatch(/已重启/u);
  });
});

describe("RuntimeSlotsSection — model picker placement", () => {
  it("shows picker on governed slots and never on external_cli", async () => {
    const api = fixtureApiKeySlot();
    const login = fixtureLoginSlot();
    const external = fixtureExternalSlot();
    const gateway = new MockRuntimeSlotsGateway([api, login, external]);
    gateway.setCatalogMode("available", {
      models: ["grok-test", "grok-beta"],
    });

    render(
      <RuntimeSlotsSection gateway={gateway} projectId={PROJECT} />,
    );

    await screen.findByTestId(`runtime-slot-model-picker-${api.slotId}`);
    expect(
      screen.getByTestId(`runtime-slot-model-picker-${login.slotId}`),
    ).toBeInTheDocument();
    // Red line: external_cli must not receive a model picker.
    expect(
      screen.queryByTestId(`runtime-slot-model-picker-${external.slotId}`),
    ).not.toBeInTheDocument();
    expect(
      screen.getByTestId(`runtime-slot-ungoverned-${external.slotId}`),
    ).toBeInTheDocument();
  });

  it("quick-switches model from the list without opening the edit form", async () => {
    const user = userEvent.setup();
    const slot = fixtureApiKeySlot({ modelId: "grok-test" });
    const gateway = new MockRuntimeSlotsGateway([slot]);
    gateway.setCatalogMode("available", {
      models: ["grok-test", "switched-from-list"],
    });
    gateway.setModelSwitchRestarted(true);

    render(
      <RuntimeSlotsSection gateway={gateway} projectId={PROJECT} />,
    );

    // Edit form closed.
    expect(screen.queryByTestId("runtime-slots-form")).not.toBeInTheDocument();

    const select = await screen.findByTestId(
      `runtime-slot-model-select-${slot.slotId}`,
    );
    await user.selectOptions(select, "switched-from-list");

    await waitFor(() => {
      expect(gateway.getSlotsSnapshot()[0]!.modelId).toBe(
        "switched-from-list",
      );
    });
    expect(screen.queryByTestId("runtime-slots-form")).not.toBeInTheDocument();

    const notice = await screen.findByTestId(
      `runtime-slot-model-switch-notice-${slot.slotId}`,
    );
    expect(notice).toHaveTextContent(MODEL_SWITCH_RESTARTED_MESSAGE);

    const current = screen.getByTestId(
      `runtime-slot-model-current-${slot.slotId}`,
    );
    expect(current).toHaveTextContent("switched-from-list");
  });

  it("truncates over-long model ids in the current display without dropping the full title", async () => {
    const longId = `very-long-model-id-${"x".repeat(80)}`;
    const slot = fixtureApiKeySlot({ modelId: longId });
    const gateway = new MockRuntimeSlotsGateway([slot]);
    gateway.setCatalogMode("available", { models: [longId, "short"] });

    render(
      <RuntimeSlotsSection gateway={gateway} projectId={PROJECT} />,
    );

    const current = await screen.findByTestId(
      `runtime-slot-model-current-${slot.slotId}`,
    );
    expect(current.getAttribute("title")).toBe(longId);
    // Display is truncated (ellipsis) — full id must not inflate layout text.
    expect(current.textContent ?? "").toContain("…");
    expect((current.textContent ?? "").length).toBeLessThan(longId.length + 10);
  });
});
