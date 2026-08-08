/**
 * Delivery-hardening: authentication_failed class → fixed Grok CLI login
 * guidance. Never reads auth files; never shows account identity.
 */
import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it } from "vitest";
import { App } from "../src/App";
import {
  GROK_CLI_LOGIN_GUIDANCE,
  guidanceLooksSafe,
  isAuthenticationFailedEvent,
  looksLikeAuthenticationFailure,
  looksLikeAuthenticationFailureCode,
  userFacingSendError,
} from "../src/features/settings/loginGuidance";
import { MockSettingsGateway } from "../src/features/settings/settingsGateway";
import { MockConversationGateway } from "../src/lib/mockConversationGateway";
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

describe("loginGuidance pure helpers", () => {
  it("classifies authentication_failed phrases and codes without secrets", () => {
    expect(looksLikeAuthenticationFailure("acp: unauthenticated")).toBe(true);
    expect(looksLikeAuthenticationFailure("login required")).toBe(true);
    expect(looksLikeAuthenticationFailure("timeout")).toBe(false);
    expect(looksLikeAuthenticationFailureCode("authentication_failed")).toBe(
      true,
    );
    expect(looksLikeAuthenticationFailureCode("runtime_executable_missing")).toBe(
      false,
    );
    expect(
      isAuthenticationFailedEvent({
        code: "adapter_error",
        message: "Unauthorized",
      }),
    ).toBe(true);
    expect(
      isAuthenticationFailedEvent({
        code: "timeout",
        message: "request timed out",
      }),
    ).toBe(false);
  });

  it("maps auth-class send errors to fixed CLI guidance", () => {
    const mapped = userFacingSendError(
      "acp_request_failed: unauthenticated — user person@example.com",
    );
    expect(mapped).toBe(GROK_CLI_LOGIN_GUIDANCE);
    expect(guidanceLooksSafe(mapped)).toBe(true);
    expect(mapped).toMatch(/grok login/i);
    expect(mapped).toMatch(/不存储/);
    // Must not pass through the account-looking fragment.
    expect(mapped).not.toMatch(/person@example\.com/);
    expect(mapped).not.toMatch(/auth\.json/i);
  });

  it("leaves non-auth failures unchanged", () => {
    expect(userFacingSendError("network down")).toBe("network down");
  });

  it("fixed guidance copy is secret-free", () => {
    expect(guidanceLooksSafe(GROK_CLI_LOGIN_GUIDANCE)).toBe(true);
  });
});

describe("App login guidance surface", () => {
  it("shows login guidance banner on auth-class send failure", async () => {
    const user = userEvent.setup();
    const conversation = new MockConversationGateway({ stepDelayMs: 0 });
    conversation.failNextSend("acp_request_failed: unauthenticated");
    const settings = new MockSettingsGateway();

    render(
      <App
        conversationGateway={conversation}
        gateway={fixtureGateway()}
        settingsGateway={settings}
      />,
    );

    await screen.findByTestId("app-sidebar");
    await user.click(await screen.findByRole("button", { name: "Pillow" }));
    await user.click(screen.getByRole("button", { name: "新建对话" }));
    const input = await screen.findByRole("textbox", { name: "工作请求" });
    await user.type(input, "hello");
    await user.click(screen.getByTestId("composer-send"));

    await waitFor(() => {
      expect(screen.getByTestId("runtime-login-guidance-banner")).toBeVisible();
    });
    const banner = screen.getByTestId("runtime-login-guidance-banner");
    expect(banner.textContent).toMatch(/grok login/i);
    expect(banner.textContent).toMatch(/不存储/);
    expect(banner.textContent).not.toMatch(/auth\.json|person@|sk-/i);

    const composerError = screen.getByTestId("composer-send-error");
    expect(composerError.textContent).toBe(GROK_CLI_LOGIN_GUIDANCE);

    // Degraded banner must not appear when conversation is available.
    expect(screen.queryByTestId("runtime-degraded-banner")).toBeNull();

    await user.click(screen.getByTestId("runtime-login-guidance-open-settings"));
    expect(await screen.findByTestId("settings-panel")).toBeVisible();
    expect(screen.getByTestId("settings-login-guidance")).toBeVisible();
  });

  it("does not show login guidance for ordinary send failures", async () => {
    const user = userEvent.setup();
    const conversation = new MockConversationGateway({ stepDelayMs: 0 });
    conversation.failNextSend("network down");
    const settings = new MockSettingsGateway();

    render(
      <App
        conversationGateway={conversation}
        gateway={fixtureGateway()}
        settingsGateway={settings}
      />,
    );

    await screen.findByTestId("app-sidebar");
    await user.click(await screen.findByRole("button", { name: "Pillow" }));
    await user.click(screen.getByRole("button", { name: "新建对话" }));
    const input = await screen.findByRole("textbox", { name: "工作请求" });
    await user.type(input, "hello");
    await user.click(screen.getByTestId("composer-send"));

    await waitFor(() => {
      expect(screen.getByTestId("composer-send-error")).toHaveTextContent(
        "network down",
      );
    });
    expect(screen.queryByTestId("runtime-login-guidance-banner")).toBeNull();
  });

  it("keeps degraded banner and suppresses login guidance when startup is degraded", async () => {
    const settings = new MockSettingsGateway();
    settings.setDegradedStartup(
      "runtime_executable_missing",
      "Grok executable missing",
      "打开「设置」修正 Grok 可执行文件路径。",
    );
    render(
      <App gateway={fixtureGateway()} settingsGateway={settings} />,
    );

    expect(await screen.findByTestId("runtime-degraded-banner")).toBeVisible();
    expect(screen.queryByTestId("runtime-login-guidance-banner")).toBeNull();
  });
});
