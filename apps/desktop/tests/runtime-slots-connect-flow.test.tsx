/**
 * Two-step slot connect flow (OpenCode-shaped):
 * 1. pick a provider
 * 2. only that provider's real auth methods appear
 *
 * UI assertions: what the user can see, not internal state.
 */
import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it } from "vitest";
import { RuntimeSlotsSection } from "../src/features/settings/RuntimeSlotsSection";
import { MockRuntimeSlotsGateway } from "../src/features/settings/runtimeSlotsGateway";
import type { RuntimeSlotView } from "../src/features/settings/runtimeSlotsTypes";
import "../src/styles/tokens.css";
import "../src/styles/settings.css";
import "../src/styles/runtime-slots.css";

const PROJECT = "project-slot-connect-flow-1";
const NOW = "2026-08-13T12:00:00.000Z";

describe("slot connect flow — Unknown client_id must not offer OAuth", () => {
  it("selecting OpenAI (Unknown client_id) does not show OAuth 登录", async () => {
    const user = userEvent.setup();
    const gateway = new MockRuntimeSlotsGateway();
    render(
      <RuntimeSlotsSection gateway={gateway} projectId={PROJECT} />,
    );

    await user.click(await screen.findByTestId("runtime-slots-add"));

    const openaiPick = screen.queryByTestId(
      "runtime-slots-provider-pick-openai",
    );
    if (openaiPick) {
      await user.click(openaiPick);
    }

    expect(screen.queryByTestId("runtime-slots-auth-oauth")).toBeNull();
    expect(screen.queryByText("OAuth 登录")).toBeNull();
  });

  it("selecting xAI (Unknown client_id) does not show OAuth 登录", async () => {
    const user = userEvent.setup();
    const gateway = new MockRuntimeSlotsGateway();
    render(
      <RuntimeSlotsSection gateway={gateway} projectId={PROJECT} />,
    );

    await user.click(await screen.findByTestId("runtime-slots-add"));

    const xaiPick = screen.queryByTestId("runtime-slots-provider-pick-xai");
    if (xaiPick) {
      await user.click(xaiPick);
    }

    expect(screen.queryByTestId("runtime-slots-auth-oauth")).toBeNull();
    expect(screen.queryByText("OAuth 登录")).toBeNull();
  });
});

describe("slot connect flow — custom endpoint shows base_url", () => {
  it("selecting 自定义端点 reveals base_url (and not an OAuth field dump)", async () => {
    const user = userEvent.setup();
    const gateway = new MockRuntimeSlotsGateway();
    render(
      <RuntimeSlotsSection gateway={gateway} projectId={PROJECT} />,
    );

    await user.click(await screen.findByTestId("runtime-slots-add"));
    await user.click(
      await screen.findByTestId("runtime-slots-provider-pick-custom"),
    );

    expect(screen.getByTestId("runtime-slots-base-url")).toBeInTheDocument();
    expect(screen.getByTestId("runtime-slots-model-id")).toBeInTheDocument();
    expect(screen.getByTestId("runtime-slots-api-key")).toBeInTheDocument();
    expect(screen.queryByTestId("runtime-slots-auth-oauth")).toBeNull();
    expect(
      screen.queryByTestId("runtime-slots-oauth-field-clientId"),
    ).toBeNull();
    expect(screen.queryByTestId("runtime-slots-oauth-field-issuer")).toBeNull();
  });
});

describe("slot connect flow — edit reuses the same form", () => {
  it("edit opens the connect step with prefilled fields and locked auth", async () => {
    const user = userEvent.setup();
    const slot: RuntimeSlotView = {
      slotId: "slot-edit-1",
      role: "custom",
      roleCustomName: "写码",
      projectId: PROJECT,
      displayName: "Existing",
      kind: "governed_grok",
      governance: "full",
      authMode: "api_key",
      baseUrl: "https://api.siliconflow.cn/v1",
      modelId: "keep-model",
      secretRef: "secret-ref-opaque",
      secretConfigured: true,
      executablePath: null,
      launchArgs: null,
      statusArgs: null,
      createdAt: NOW,
      updatedAt: NOW,
      accountLabel: null,
      accessExpiresAt: null,
    isActive: false,
    };
    const gateway = new MockRuntimeSlotsGateway([slot]);
    render(
      <RuntimeSlotsSection gateway={gateway} projectId={PROJECT} />,
    );

    await user.click(
      await screen.findByTestId(`runtime-slot-edit-${slot.slotId}`),
    );

    expect(screen.getByTestId("runtime-slots-form-step")).toHaveTextContent(
      "connect",
    );
    expect(screen.getByTestId("runtime-slots-connect-provider")).toHaveTextContent(
      /硅基流动|SiliconFlow/u,
    );
    expect(screen.getByTestId("runtime-slots-auth-api_key")).toBeDisabled();
    expect(screen.queryByTestId("runtime-slots-auth-oauth")).toBeNull();
    expect(screen.getByTestId("runtime-slots-base-url")).toHaveValue(
      "https://api.siliconflow.cn/v1",
    );
    expect(screen.getByTestId("runtime-slots-model-id")).toHaveValue(
      "keep-model",
    );
    expect(screen.getByTestId("runtime-slots-api-key")).toHaveValue("");
    expect(screen.queryByTestId("runtime-slots-provider-step")).toBeNull();
  });
});
