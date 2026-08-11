/**
 * Send-path guidance when conversation runtime is unavailable.
 * Positive: no slots vs slots-not-started each get distinct actionable copy + settings CTA.
 * Negative: ordinary failures and successful sends must not show this banner.
 */
import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it } from "vitest";
import { App } from "../src/App";
import {
  classifyRuntimeSlotGuidance,
  looksLikeRuntimeUnavailable,
  RUNTIME_SLOT_GUIDANCE_NO_SLOTS,
  RUNTIME_SLOT_GUIDANCE_NOT_STARTED,
  RUNTIME_SLOT_GUIDANCE_TITLE,
  userFacingRuntimeSlotGuidance,
} from "../src/features/settings/runtimeUnavailableGuidance";
import { MockRuntimeSlotsGateway } from "../src/features/settings/runtimeSlotsGateway";
import { MockSettingsGateway } from "../src/features/settings/settingsGateway";
import type { RuntimeSlotView } from "../src/features/settings/runtimeSlotsTypes";
import { guidanceLooksSafe } from "../src/features/settings/loginGuidance";
import { MockConversationGateway } from "../src/lib/mockConversationGateway";
import type { ProjectDto, ProjectGateway } from "../src/lib/projectGateway";
import "../src/styles/tokens.css";
import "../src/styles/settings.css";

const PROJECT = "pillow";

function fixtureGateway(): ProjectGateway {
  const project: ProjectDto = {
    id: PROJECT,
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

function fixtureGovernedSlot(
  overrides: Partial<RuntimeSlotView> = {},
): RuntimeSlotView {
  return {
    slotId: "slot-g1",
    projectId: PROJECT,
    displayName: "Grok login",
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
    createdAt: "2026-08-11T00:00:00.000Z",
    updatedAt: "2026-08-11T00:00:00.000Z",
    ...overrides,
  };
}

function fixtureExternalSlot(): RuntimeSlotView {
  return {
    slotId: "slot-ext",
    projectId: PROJECT,
    displayName: "Codex CLI",
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
    createdAt: "2026-08-11T00:00:00.000Z",
    updatedAt: "2026-08-11T00:00:00.000Z",
  };
}

const RUNTIME_UNAVAILABLE_RAW =
  "runtime_unavailable: Conversation runtime is unavailable. Start a governed runtime slot.";

async function openComposerAndSend(
  user: ReturnType<typeof userEvent.setup>,
  text: string,
): Promise<void> {
  await screen.findByTestId("app-sidebar");
  await user.click(await screen.findByRole("button", { name: "Pillow" }));
  await user.click(screen.getByRole("button", { name: "新建对话" }));
  const input = await screen.findByRole("textbox", { name: "工作请求" });
  await user.type(input, text);
  await user.click(screen.getByTestId("composer-send"));
}

describe("runtimeUnavailableGuidance pure helpers", () => {
  it("detects runtime_unavailable codes/phrases and rejects ordinary errors", () => {
    expect(looksLikeRuntimeUnavailable(RUNTIME_UNAVAILABLE_RAW)).toBe(true);
    expect(
      looksLikeRuntimeUnavailable(
        "Conversation runtime is unavailable. Start a governed runtime slot.",
      ),
    ).toBe(true);
    expect(looksLikeRuntimeUnavailable("network down")).toBe(false);
    expect(looksLikeRuntimeUnavailable("authentication_failed")).toBe(false);
    expect(looksLikeRuntimeUnavailable(null)).toBe(false);
  });

  it("classifies no governed slots vs governed-but-not-started", () => {
    expect(classifyRuntimeSlotGuidance([])).toBe("no_slots");
    expect(classifyRuntimeSlotGuidance([fixtureExternalSlot()])).toBe(
      "no_slots",
    );
    expect(classifyRuntimeSlotGuidance([fixtureGovernedSlot()])).toBe(
      "not_started",
    );
    expect(
      classifyRuntimeSlotGuidance([
        fixtureExternalSlot(),
        fixtureGovernedSlot(),
      ]),
    ).toBe("not_started");
  });

  it("maps kinds to distinct actionable copy (secret-free)", () => {
    const none = userFacingRuntimeSlotGuidance("no_slots");
    const idle = userFacingRuntimeSlotGuidance("not_started");
    expect(none).toBe(RUNTIME_SLOT_GUIDANCE_NO_SLOTS);
    expect(idle).toBe(RUNTIME_SLOT_GUIDANCE_NOT_STARTED);
    expect(none).not.toBe(idle);
    expect(none).toMatch(/还没有可启动的代理槽位/);
    expect(none).toMatch(/新增/);
    expect(idle).toMatch(/已有代理槽位/);
    expect(idle).toMatch(/启动/);
    expect(guidanceLooksSafe(none)).toBe(true);
    expect(guidanceLooksSafe(idle)).toBe(true);
  });
});

describe("App runtime-unavailable send guidance", () => {
  it("no slots: actionable tip + settings entry (not just an error code)", async () => {
    const user = userEvent.setup();
    const conversation = new MockConversationGateway({ stepDelayMs: 0 });
    conversation.failNextSend(RUNTIME_UNAVAILABLE_RAW);
    const slots = new MockRuntimeSlotsGateway([]);
    const settings = new MockSettingsGateway();

    render(
      <App
        conversationGateway={conversation}
        gateway={fixtureGateway()}
        runtimeSlotsGateway={slots}
        settingsGateway={settings}
      />,
    );

    await openComposerAndSend(user, "hello");

    const banner = await screen.findByTestId("runtime-slot-guidance-banner");
    expect(banner).toBeVisible();
    expect(banner.getAttribute("data-runtime-slot-guidance")).toBe("no_slots");
    expect(banner.textContent).toContain(RUNTIME_SLOT_GUIDANCE_TITLE);
    expect(screen.getByTestId("runtime-slot-guidance-body").textContent).toBe(
      RUNTIME_SLOT_GUIDANCE_NO_SLOTS,
    );
    // Composer shows the same actionable body — not a bare code.
    expect(screen.getByTestId("composer-send-error").textContent).toBe(
      RUNTIME_SLOT_GUIDANCE_NO_SLOTS,
    );
    expect(screen.getByTestId("composer-send-error").textContent).not.toMatch(
      /^runtime_unavailable$/,
    );
    // Must not look like a successful thinking turn.
    expect(screen.queryByTestId("composer-refresh-warning")).toBeNull();

    await user.click(screen.getByTestId("runtime-slot-guidance-open-settings"));
    expect(await screen.findByTestId("settings-panel")).toBeVisible();
    expect(await screen.findByTestId("runtime-slots-section")).toBeVisible();
  });

  it("slots exist but not started: distinct tip + settings entry", async () => {
    const user = userEvent.setup();
    const conversation = new MockConversationGateway({ stepDelayMs: 0 });
    conversation.failNextSend(RUNTIME_UNAVAILABLE_RAW);
    const slots = new MockRuntimeSlotsGateway([fixtureGovernedSlot()]);
    const settings = new MockSettingsGateway();

    render(
      <App
        conversationGateway={conversation}
        gateway={fixtureGateway()}
        runtimeSlotsGateway={slots}
        settingsGateway={settings}
      />,
    );

    await openComposerAndSend(user, "hello");

    const banner = await screen.findByTestId("runtime-slot-guidance-banner");
    expect(banner.getAttribute("data-runtime-slot-guidance")).toBe(
      "not_started",
    );
    expect(screen.getByTestId("runtime-slot-guidance-body").textContent).toBe(
      RUNTIME_SLOT_GUIDANCE_NOT_STARTED,
    );
    expect(screen.getByTestId("composer-send-error").textContent).toBe(
      RUNTIME_SLOT_GUIDANCE_NOT_STARTED,
    );
    expect(screen.getByTestId("composer-send-error").textContent).toMatch(
      /已有代理槽位/,
    );
    expect(screen.getByTestId("composer-send-error").textContent).not.toMatch(
      /还没有可启动的代理槽位/,
    );

    await user.click(screen.getByTestId("runtime-slot-guidance-open-settings"));
    expect(await screen.findByTestId("settings-panel")).toBeVisible();
    expect(
      await screen.findByTestId(`runtime-slot-needs-start-slot-g1`),
    ).toBeVisible();
    expect(screen.getByTestId("runtime-slot-probe-slot-g1")).toHaveTextContent(
      "启动",
    );
  });

  it("external-only slots classify as no chat-capable slots", async () => {
    const user = userEvent.setup();
    const conversation = new MockConversationGateway({ stepDelayMs: 0 });
    conversation.failNextSend(RUNTIME_UNAVAILABLE_RAW);
    const slots = new MockRuntimeSlotsGateway([fixtureExternalSlot()]);
    const settings = new MockSettingsGateway();

    render(
      <App
        conversationGateway={conversation}
        gateway={fixtureGateway()}
        runtimeSlotsGateway={slots}
        settingsGateway={settings}
      />,
    );

    await openComposerAndSend(user, "hello");

    const banner = await screen.findByTestId("runtime-slot-guidance-banner");
    expect(banner.getAttribute("data-runtime-slot-guidance")).toBe("no_slots");
    expect(screen.getByTestId("runtime-slot-guidance-body").textContent).toBe(
      RUNTIME_SLOT_GUIDANCE_NO_SLOTS,
    );
  });

  it("negative: ordinary send failure does not show slot guidance", async () => {
    const user = userEvent.setup();
    const conversation = new MockConversationGateway({ stepDelayMs: 0 });
    conversation.failNextSend("network down");
    const slots = new MockRuntimeSlotsGateway([]);
    const settings = new MockSettingsGateway();

    render(
      <App
        conversationGateway={conversation}
        gateway={fixtureGateway()}
        runtimeSlotsGateway={slots}
        settingsGateway={settings}
      />,
    );

    await openComposerAndSend(user, "hello");

    await waitFor(() => {
      expect(screen.getByTestId("composer-send-error")).toHaveTextContent(
        "network down",
      );
    });
    expect(screen.queryByTestId("runtime-slot-guidance-banner")).toBeNull();
  });

  it("negative: successful send never shows slot guidance", async () => {
    const user = userEvent.setup();
    const conversation = new MockConversationGateway({ stepDelayMs: 0 });
    const slots = new MockRuntimeSlotsGateway([]);
    const settings = new MockSettingsGateway();

    render(
      <App
        conversationGateway={conversation}
        gateway={fixtureGateway()}
        runtimeSlotsGateway={slots}
        settingsGateway={settings}
      />,
    );

    await openComposerAndSend(user, "hello");

    await waitFor(() => {
      expect(screen.queryByTestId("composer-send-error")).toBeNull();
    });
    expect(screen.queryByTestId("runtime-slot-guidance-banner")).toBeNull();
  });
});
