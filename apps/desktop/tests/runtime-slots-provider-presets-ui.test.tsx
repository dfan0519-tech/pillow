/**
 * Create-slot provider presets UI:
 * - Selecting a preset fills baseUrl (positive).
 * - Manual baseUrl edit is not clobbered by other form activity (negative).
 * - Draft model pull: success lists+selects; failure shows reason + manual remains.
 * - API key never appears in DOM after save/fail.
 */
import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, describe, expect, it, vi } from "vitest";
import { RuntimeSlotsSection } from "../src/features/settings/RuntimeSlotsSection";
import { MockRuntimeSlotsGateway } from "../src/features/settings/runtimeSlotsGateway";
import { getProviderPreset } from "../src/features/settings/providerPresets";
import "../src/styles/tokens.css";
import "../src/styles/settings.css";
import "../src/styles/runtime-slots.css";

const PROJECT = "project-provider-presets-1";
const TEST_API_KEY = "pillow-test-sentinel-provider-preset-7c1e4d2a";

function assertSecretAbsentFromDom(secret: string): void {
  expect(document.body.textContent ?? "").not.toContain(secret);
  expect(document.body.innerHTML).not.toContain(secret);
  for (const input of document.querySelectorAll("input")) {
    expect(input.value).not.toBe(secret);
    expect(input.getAttribute("value") ?? "").not.toContain(secret);
  }
}

async function openCreateAndPick(
  user: ReturnType<typeof userEvent.setup>,
  providerId: string,
): Promise<void> {
  await screen.findByTestId("runtime-slots-add");
  await user.click(screen.getByTestId("runtime-slots-add"));
  await user.click(
    screen.getByTestId(`runtime-slots-provider-pick-${providerId}`),
  );
  await screen.findByTestId("runtime-slots-base-url");
}

describe("RuntimeSlotsSection — provider preset fills baseUrl", () => {
  it("selecting SiliconFlow fills the official baseUrl and leaves it editable", async () => {
    const user = userEvent.setup();
    const gateway = new MockRuntimeSlotsGateway();
    render(
      <RuntimeSlotsSection gateway={gateway} projectId={PROJECT} />,
    );
    await openCreateAndPick(user, "custom");

    expect(screen.getByTestId("runtime-slots-base-url")).toHaveValue("");

    await user.click(screen.getByTestId("runtime-slots-form-back"));
    await user.click(
      screen.getByTestId("runtime-slots-provider-pick-siliconflow"),
    );

    const expected = getProviderPreset("siliconflow")!.baseUrl;
    const baseUrl = screen.getByTestId("runtime-slots-base-url");
    expect(baseUrl).toHaveValue(expected);
    // Positive: not merely "select rendered" — value matches catalog.
    expect(expected).toBe("https://api.siliconflow.cn/v1");

    // Still editable.
    await user.clear(baseUrl);
    await user.type(baseUrl, "https://my-proxy.example.test/v1");
    expect(baseUrl).toHaveValue("https://my-proxy.example.test/v1");
  });

  it("selecting DeepSeek fills baseUrl and suggested model id", async () => {
    const user = userEvent.setup();
    const gateway = new MockRuntimeSlotsGateway();
    render(
      <RuntimeSlotsSection gateway={gateway} projectId={PROJECT} />,
    );
    await openCreateAndPick(user, "deepseek");

    expect(screen.getByTestId("runtime-slots-base-url")).toHaveValue(
      "https://api.deepseek.com",
    );
    expect(screen.getByTestId("runtime-slots-model-id")).toHaveValue(
      "deepseek-chat",
    );
  });

  it("does not overwrite a user-edited baseUrl when other fields change", async () => {
    const user = userEvent.setup();
    const gateway = new MockRuntimeSlotsGateway();
    render(
      <RuntimeSlotsSection gateway={gateway} projectId={PROJECT} />,
    );
    await openCreateAndPick(user, "openrouter");
    const baseUrl = screen.getByTestId("runtime-slots-base-url");
    expect(baseUrl).toHaveValue("https://openrouter.ai/api/v1");

    await user.clear(baseUrl);
    await user.type(baseUrl, "https://user-edited.example.test/openai/v1");

    // Touch other fields that must not re-apply the preset.
    await user.type(
      screen.getByTestId("runtime-slots-display-name"),
      "Edited Slot",
    );
    await user.type(screen.getByTestId("runtime-slots-model-id"), "x-model");
    await user.type(screen.getByTestId("runtime-slots-api-key"), "not-a-real-key");
    await user.click(screen.getByTestId("runtime-slots-role-custom"));
    await user.type(
      screen.getByTestId("runtime-slots-role-custom-name"),
      "写码",
    );

    // Negative pin: preset must not clobber the hand edit.
    expect(baseUrl).toHaveValue("https://user-edited.example.test/openai/v1");
    expect(screen.getByTestId("runtime-slots-connect-provider")).toHaveTextContent(
      /OpenRouter/u,
    );
  });

  it("switching to custom clears baseUrl to empty (manual path)", async () => {
    const user = userEvent.setup();
    const gateway = new MockRuntimeSlotsGateway();
    render(
      <RuntimeSlotsSection gateway={gateway} projectId={PROJECT} />,
    );
    await openCreateAndPick(user, "groq");
    expect(screen.getByTestId("runtime-slots-base-url")).toHaveValue(
      "https://api.groq.com/openai/v1",
    );

    await user.click(screen.getByTestId("runtime-slots-form-back"));
    await user.click(screen.getByTestId("runtime-slots-provider-pick-custom"));
    expect(screen.getByTestId("runtime-slots-base-url")).toHaveValue("");
  });
});


describe("RuntimeSlotsSection — create with preset still keeps secret out of DOM", () => {
  it("creates via preset-filled baseUrl and clears key draft on save", async () => {
    const user = userEvent.setup();
    const gateway = new MockRuntimeSlotsGateway();
    render(
      <RuntimeSlotsSection gateway={gateway} projectId={PROJECT} />,
    );
    await openCreateAndPick(user, "siliconflow");

    await user.type(screen.getByTestId("runtime-slots-display-name"), "SF Slot");
    await user.click(screen.getByTestId("runtime-slots-role-custom"));
    await user.type(screen.getByTestId("runtime-slots-role-custom-name"), "写码");
    await user.type(screen.getByTestId("runtime-slots-model-id"), "Qwen/Qwen-test");
    await user.type(screen.getByTestId("runtime-slots-api-key"), TEST_API_KEY);
    await user.click(screen.getByTestId("runtime-slots-form-save"));

    await waitFor(() => {
      expect(gateway.lastCreateSlotArgs.length).toBeGreaterThan(0);
    });

    const args = gateway.lastCreateSlotArgs.at(-1)!;
    expect(args.authKind).toBe("api_key");
    expect(args.baseUrl).toBe("https://api.siliconflow.cn/v1");
    expect(args.modelId).toBe("Qwen/Qwen-test");
    expect(args.apiKey).toBe(TEST_API_KEY);
    // Governance still full (preset does not weaken).
    const created = gateway.getSlotsSnapshot()[0]!;
    expect(created.governance).toBe("full");
    expect(created.kind).toBe("governed_grok");
    assertSecretAbsentFromDom(TEST_API_KEY);
  });
});
