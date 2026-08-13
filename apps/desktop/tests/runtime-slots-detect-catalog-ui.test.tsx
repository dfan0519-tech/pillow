/**
 * Create-form UI: local CLI detect selection + draft host model probe.
 * unknown auth is 未确定; apiKey never in DOM after probe/fail.
 */
import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it } from "vitest";
import { RuntimeSlotsSection } from "../src/features/settings/RuntimeSlotsSection";
import { MockRuntimeSlotsGateway } from "../src/features/settings/runtimeSlotsGateway";
import {
  LOCAL_CLI_AUTH_NOT_AUTHENTICATED_LABEL,
  LOCAL_CLI_AUTH_UNKNOWN_LABEL,
  type LocalCliDetectionView,
} from "../src/features/settings/runtimeSlotsTypes";
import "../src/styles/tokens.css";
import "../src/styles/settings.css";
import "../src/styles/runtime-slots.css";

const PROJECT = "project-detect-catalog-1";
const TEST_API_KEY = "pillow-test-sentinel-detect-catalog-ui-4b9e";

function detectFixture(): LocalCliDetectionView[] {
  return [
    {
      kind: "codex",
      found: true,
      executablePath: "C:/tools/codex.exe",
      version: "0.2.0",
      authStatus: "authenticated",
      accountLabel: "ChatGPT",
      authReason: null,
    },
    {
      kind: "gemini",
      found: true,
      executablePath: "C:/tools/gemini.js",
      version: "1.0.0",
      authStatus: "unknown",
      accountLabel: null,
      authReason: "no_headless_status_command",
    },
    {
      kind: "claude",
      found: true,
      executablePath: "C:/tools/claude.exe",
      version: null,
      authStatus: "not_authenticated",
      accountLabel: null,
      authReason: null,
    },
    {
      kind: "grok",
      found: false,
      executablePath: null,
      version: null,
      authStatus: "unknown",
      accountLabel: null,
      authReason: "not_found",
    },
    {
      kind: "opencode",
      found: false,
      executablePath: null,
      version: null,
      authStatus: "unknown",
      accountLabel: null,
      authReason: "not_found",
    },
  ];
}

describe("RuntimeSlotsSection — local CLI detect", () => {
  it("scan lists results; select fills path; unknown is 未确定 not 未登录", async () => {
    const user = userEvent.setup();
    const gateway = new MockRuntimeSlotsGateway();
    gateway.seedDetectResults(detectFixture());
    render(
      <RuntimeSlotsSection gateway={gateway} projectId={PROJECT} />,
    );

    await screen.findByTestId("runtime-slots-add");
    await user.click(screen.getByTestId("runtime-slots-add"));
    await user.click(screen.getByTestId("runtime-slots-provider-pick-local_cli"));
    await user.click(screen.getByTestId("runtime-slots-detect-scan"));

    await waitFor(() => {
      expect(screen.getByTestId("runtime-slots-detect-list")).toBeInTheDocument();
    });

    const geminiAuth = screen.getByTestId("runtime-slots-detect-auth-gemini");
    expect(geminiAuth).toHaveTextContent(LOCAL_CLI_AUTH_UNKNOWN_LABEL);
    expect(geminiAuth).toHaveTextContent("no_headless_status_command");
    // Negative: unknown must not be labeled 未登录.
    expect(geminiAuth.textContent).not.toContain(
      LOCAL_CLI_AUTH_NOT_AUTHENTICATED_LABEL,
    );

    const claudeAuth = screen.getByTestId("runtime-slots-detect-auth-claude");
    expect(claudeAuth).toHaveTextContent(LOCAL_CLI_AUTH_NOT_AUTHENTICATED_LABEL);

    await user.click(screen.getByTestId("runtime-slots-detect-select-codex"));
    expect(screen.getByTestId("runtime-slots-executable")).toHaveValue(
      "C:/tools/codex.exe",
    );
    expect(screen.getByTestId("runtime-slots-status-args")).toHaveValue(
      "login status",
    );
  });
});

describe("RuntimeSlotsSection — draft host model catalog", () => {
  it("success lists models and selects into model id without fetch", async () => {
    const user = userEvent.setup();
    const gateway = new MockRuntimeSlotsGateway();
    gateway.setDraftProbeMode("available", {
      models: ["preset-model-a", "preset-model-b"],
    });
    render(
      <RuntimeSlotsSection gateway={gateway} projectId={PROJECT} />,
    );

    await screen.findByTestId("runtime-slots-add");
    await user.click(screen.getByTestId("runtime-slots-add"));
    await user.click(screen.getByTestId("runtime-slots-provider-pick-custom"));
    await user.click(screen.getByTestId("runtime-slots-auth-api_key"));
    await user.type(
      screen.getByTestId("runtime-slots-base-url"),
      "https://api.example.test/v1",
    );
    await user.type(screen.getByTestId("runtime-slots-api-key"), TEST_API_KEY);

    const pull = screen.getByTestId("runtime-slots-draft-catalog-pull");
    expect(pull).not.toBeDisabled();
    await user.click(pull);

    const select = await screen.findByTestId(
      "runtime-slots-draft-models-select",
    );
    expect(select).toHaveTextContent("preset-model-a");
    expect(select).toHaveTextContent("preset-model-b");
    await user.selectOptions(select, "preset-model-b");
    expect(screen.getByTestId("runtime-slots-model-id")).toHaveValue(
      "preset-model-b",
    );
    // Key may still be in the password field (draft for save) — not in text nodes.
    expect(document.body.textContent ?? "").not.toContain(TEST_API_KEY);
    expect(gateway.lastProbeModelCatalogCalls[0]?.apiKeyLength).toBe(
      TEST_API_KEY.length,
    );
  });

  it("unsupported shows reason; does not auto-fill model", async () => {
    const user = userEvent.setup();
    const gateway = new MockRuntimeSlotsGateway();
    gateway.setDraftProbeMode("unsupported_endpoint");
    render(
      <RuntimeSlotsSection gateway={gateway} projectId={PROJECT} />,
    );

    await screen.findByTestId("runtime-slots-add");
    await user.click(screen.getByTestId("runtime-slots-add"));
    await user.click(screen.getByTestId("runtime-slots-provider-pick-custom"));
    await user.click(screen.getByTestId("runtime-slots-auth-api_key"));
    await user.type(
      screen.getByTestId("runtime-slots-base-url"),
      "https://api.example.test/v1",
    );
    await user.type(screen.getByTestId("runtime-slots-api-key"), TEST_API_KEY);
    await user.click(screen.getByTestId("runtime-slots-draft-catalog-pull"));

    const unsupported = await screen.findByTestId(
      "runtime-slots-draft-models-unsupported",
    );
    expect(
      screen.queryByTestId("runtime-slots-draft-models-select"),
    ).not.toBeInTheDocument();
    expect(screen.getByTestId("runtime-slots-model-id")).toHaveValue("");
    // Key stays in the password draft (still needed for save) — must not appear
    // in status/error copy or other non-password text.
    expect(unsupported.textContent ?? "").not.toContain(TEST_API_KEY);
    expect(document.body.textContent ?? "").not.toContain(TEST_API_KEY);
  });

  it("fail shows error; does not invent models; key not in error text", async () => {
    const user = userEvent.setup();
    const gateway = new MockRuntimeSlotsGateway();
    gateway.setDraftProbeMode("fail", {
      failCode: "runtime_slot_model_catalog_unavailable",
    });
    render(
      <RuntimeSlotsSection gateway={gateway} projectId={PROJECT} />,
    );

    await screen.findByTestId("runtime-slots-add");
    await user.click(screen.getByTestId("runtime-slots-add"));
    await user.click(screen.getByTestId("runtime-slots-provider-pick-custom"));
    await user.click(screen.getByTestId("runtime-slots-auth-api_key"));
    await user.type(
      screen.getByTestId("runtime-slots-base-url"),
      "https://api.example.test/v1",
    );
    await user.type(screen.getByTestId("runtime-slots-api-key"), TEST_API_KEY);
    await user.click(screen.getByTestId("runtime-slots-draft-catalog-pull"));

    const err = await screen.findByTestId("runtime-slots-draft-models-error");
    expect(err.textContent).toMatch(/模型目录|手输|不可用/u);
    expect(err.textContent).not.toContain(TEST_API_KEY);
    expect(
      screen.queryByTestId("runtime-slots-draft-models-select"),
    ).not.toBeInTheDocument();
    expect(screen.getByTestId("runtime-slots-model-id")).toHaveValue("");
  });

  it("pull button disabled until baseUrl and apiKey are filled", async () => {
    const user = userEvent.setup();
    const gateway = new MockRuntimeSlotsGateway();
    render(
      <RuntimeSlotsSection gateway={gateway} projectId={PROJECT} />,
    );
    await screen.findByTestId("runtime-slots-add");
    await user.click(screen.getByTestId("runtime-slots-add"));
    await user.click(screen.getByTestId("runtime-slots-provider-pick-custom"));
    await user.click(screen.getByTestId("runtime-slots-auth-api_key"));

    expect(
      screen.getByTestId("runtime-slots-draft-catalog-pull"),
    ).toBeDisabled();

    await user.type(
      screen.getByTestId("runtime-slots-base-url"),
      "https://api.example.test/v1",
    );
    expect(
      screen.getByTestId("runtime-slots-draft-catalog-pull"),
    ).toBeDisabled();

    await user.type(screen.getByTestId("runtime-slots-api-key"), "x");
    expect(
      screen.getByTestId("runtime-slots-draft-catalog-pull"),
    ).not.toBeDisabled();
  });
});
