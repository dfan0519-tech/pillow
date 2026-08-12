/**
 * OAuth login entry in RuntimeSlotsSection: data-driven auth row, provider
 * filter, Unknown fields, in-progress + failure (no optimistic slot), logged-in
 * display without tokens in DOM. Injectable mock gateway only.
 */
import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it, vi } from "vitest";
import { RuntimeSlotsSection } from "../src/features/settings/RuntimeSlotsSection";
import { MockRuntimeSlotsGateway } from "../src/features/settings/runtimeSlotsGateway";
import {
  GOVERNED_GROK_BADGE,
  OAUTH_AUTH_BADGE,
  OAUTH_BROWSER_OPEN_FAILED_MESSAGE,
  OAUTH_FIELD_CLIENT_ID_UNKNOWN_REASON,
  OAUTH_LOGIN_DENIED_MESSAGE,
  OAUTH_LOGIN_IN_PROGRESS,
  OAUTH_LOGIN_STATE_MISMATCH_MESSAGE,
  OAUTH_LOGIN_TIMEOUT_MESSAGE,
  SLOT_AUTH_METHODS,
  RuntimeSlotsGatewayError,
  type RuntimeSlotView,
} from "../src/features/settings/runtimeSlotsTypes";
import "../src/styles/tokens.css";
import "../src/styles/settings.css";
import "../src/styles/runtime-slots.css";

const PROJECT = "project-oauth-ui-1";
const NOW = "2026-08-12T12:00:00.000Z";
/** Sentinel that must never appear in DOM (simulates token/code material). */
const TOKEN_SENTINEL = "pillow-test-oauth-token-sentinel-a1b2c3d4";
const CODE_SENTINEL = "pillow-test-oauth-code-sentinel-e5f6g7h8";
const REFRESH_SENTINEL = "pillow-test-oauth-refresh-sentinel-i9j0k1l2";

function fixtureOauthSlot(
  overrides: Partial<RuntimeSlotView> = {},
): RuntimeSlotView {
  return {
    slotId: "slot-oauth-1",
    role: "custom",
    roleCustomName: "写码",
    projectId: PROJECT,
    displayName: "OpenAI OAuth",
    kind: "governed_grok",
    governance: "full",
    authMode: "oauth",
    baseUrl: "https://api.openai.com/v1",
    modelId: "gpt-4o",
    secretRef: null,
    secretConfigured: true,
    executablePath: null,
    launchArgs: null,
    statusArgs: null,
    createdAt: NOW,
    updatedAt: NOW,
    accountLabel: "dev@example.test",
    accessExpiresAt: "2026-09-01T00:00:00.000Z",
    ...overrides,
  };
}

function assertMaterialAbsentFromDom(...materials: string[]): void {
  const text = document.body.textContent ?? "";
  const html = document.body.innerHTML;
  for (const material of materials) {
    expect(text).not.toContain(material);
    expect(html).not.toContain(material);
    for (const input of document.querySelectorAll("input, textarea, select")) {
      const el = input as HTMLInputElement;
      expect(el.value).not.toContain(material);
      expect(el.getAttribute("value") ?? "").not.toContain(material);
    }
  }
}

describe("OAuth auth method — data-driven SLOT_AUTH_METHODS", () => {
  it("includes oauth as a fourth equal method with controller allowed", () => {
    const oauth = SLOT_AUTH_METHODS.find((method) => method.id === "oauth");
    expect(oauth).toBeDefined();
    expect(oauth?.allowsController).toBe(true);
    expect(SLOT_AUTH_METHODS.some((method) => method.id === "local_cli")).toBe(
      true,
    );
    const cli = SLOT_AUTH_METHODS.find((method) => method.id === "local_cli");
    expect(cli?.allowsController).toBe(false);
  });

  it("renders oauth auth button from the catalog (no separate branch control)", async () => {
    const user = userEvent.setup();
    const gateway = new MockRuntimeSlotsGateway();
    render(
      <RuntimeSlotsSection gateway={gateway} projectId={PROJECT} />,
    );

    await user.click(await screen.findByTestId("runtime-slots-add"));
    const oauthBtn = screen.getByTestId("runtime-slots-auth-oauth");
    expect(oauthBtn).toHaveTextContent("OAuth 登录");
    await user.click(oauthBtn);
    expect(oauthBtn).toHaveAttribute("aria-pressed", "true");
    expect(
      screen.getByTestId("runtime-slots-auth-description"),
    ).toHaveTextContent(/OAuth|浏览器/u);
  });
});

describe("OAuth provider list — api_key_only excluded; Unknown fields required", () => {
  it("lists oauth-capable providers and never anthropic (api_key_only)", async () => {
    const user = userEvent.setup();
    const gateway = new MockRuntimeSlotsGateway();
    render(
      <RuntimeSlotsSection gateway={gateway} projectId={PROJECT} />,
    );

    await user.click(await screen.findByTestId("runtime-slots-add"));
    await user.click(screen.getByTestId("runtime-slots-auth-oauth"));

    await waitFor(() => {
      expect(
        screen.getByTestId("runtime-slots-oauth-provider"),
      ).toBeInTheDocument();
    });

    const ids = screen.getByTestId("runtime-slots-oauth-provider-ids")
      .textContent ?? "";
    expect(ids).toContain("openai");
    expect(ids).toContain("xai");
    expect(ids).toContain("custom-oidc");
    expect(ids).not.toContain("anthropic");

    const select = screen.getByTestId(
      "runtime-slots-oauth-provider",
    ) as HTMLSelectElement;
    const optionValues = Array.from(select.options).map((opt) => opt.value);
    expect(optionValues).not.toContain("anthropic");
  });

  it("positive: Unknown client_id shows required input + reason (openai)", async () => {
    const user = userEvent.setup();
    const gateway = new MockRuntimeSlotsGateway();
    render(
      <RuntimeSlotsSection gateway={gateway} projectId={PROJECT} />,
    );

    await user.click(await screen.findByTestId("runtime-slots-add"));
    await user.click(screen.getByTestId("runtime-slots-auth-oauth"));
    await waitFor(() => {
      expect(
        screen.getByTestId("runtime-slots-oauth-provider"),
      ).toBeInTheDocument();
    });
    await user.selectOptions(
      screen.getByTestId("runtime-slots-oauth-provider"),
      "openai",
    );

    expect(
      screen.getByTestId("runtime-slots-oauth-field-clientId"),
    ).toBeInTheDocument();
    expect(
      screen.getByTestId("runtime-slots-oauth-field-reason-clientId"),
    ).toHaveTextContent(OAUTH_FIELD_CLIENT_ID_UNKNOWN_REASON);
    // No auto-filled guessed client_id
    expect(
      (screen.getByTestId("runtime-slots-oauth-field-clientId") as HTMLInputElement)
        .value,
    ).toBe("");
  });

  it("negative: Known fields (issuer etc.) are not shown as required for openai", async () => {
    const user = userEvent.setup();
    const gateway = new MockRuntimeSlotsGateway();
    render(
      <RuntimeSlotsSection gateway={gateway} projectId={PROJECT} />,
    );

    await user.click(await screen.findByTestId("runtime-slots-add"));
    await user.click(screen.getByTestId("runtime-slots-auth-oauth"));
    await waitFor(() => {
      expect(
        screen.getByTestId("runtime-slots-oauth-provider"),
      ).toBeInTheDocument();
    });
    await user.selectOptions(
      screen.getByTestId("runtime-slots-oauth-provider"),
      "openai",
    );

    expect(
      screen.queryByTestId("runtime-slots-oauth-field-issuer"),
    ).toBeNull();
    expect(
      screen.queryByTestId("runtime-slots-oauth-field-authorizeEndpoint"),
    ).toBeNull();
    expect(
      screen.queryByTestId("runtime-slots-oauth-field-tokenEndpoint"),
    ).toBeNull();
    expect(
      screen.queryByTestId("runtime-slots-oauth-field-scopes"),
    ).toBeNull();
  });

  it("positive: custom-oidc requires issuer/endpoints/scopes/accountLabel", async () => {
    const user = userEvent.setup();
    const gateway = new MockRuntimeSlotsGateway();
    render(
      <RuntimeSlotsSection gateway={gateway} projectId={PROJECT} />,
    );

    await user.click(await screen.findByTestId("runtime-slots-add"));
    await user.click(screen.getByTestId("runtime-slots-auth-oauth"));
    await waitFor(() => {
      expect(
        screen.getByTestId("runtime-slots-oauth-provider"),
      ).toBeInTheDocument();
    });
    await user.selectOptions(
      screen.getByTestId("runtime-slots-oauth-provider"),
      "custom-oidc",
    );

    for (const field of [
      "clientId",
      "issuer",
      "authorizeEndpoint",
      "tokenEndpoint",
      "scopes",
      "accountLabel",
    ] as const) {
      expect(
        screen.getByTestId(`runtime-slots-oauth-field-${field}`),
      ).toBeInTheDocument();
      expect(
        screen.getByTestId(`runtime-slots-oauth-field-reason-${field}`),
      ).toBeInTheDocument();
    }
  });
});

describe("OAuth login failures — timeout / denied / state mismatch", () => {
  async function fillOpenaiOauthForm(
    user: ReturnType<typeof userEvent.setup>,
  ): Promise<void> {
    await user.click(await screen.findByTestId("runtime-slots-add"));
    await user.type(screen.getByTestId("runtime-slots-display-name"), "OA Slot");
    await user.click(screen.getByTestId("runtime-slots-auth-oauth"));
    await waitFor(() => {
      expect(
        screen.getByTestId("runtime-slots-oauth-provider"),
      ).toBeInTheDocument();
    });
    await user.selectOptions(
      screen.getByTestId("runtime-slots-oauth-provider"),
      "openai",
    );
    await user.type(
      screen.getByTestId("runtime-slots-oauth-field-clientId"),
      "user-owned-client-id",
    );
    await user.type(
      screen.getByTestId("runtime-slots-oauth-base-url"),
      "https://api.openai.com/v1",
    );
    await user.type(
      screen.getByTestId("runtime-slots-oauth-model-id"),
      "gpt-4o",
    );
    await user.click(screen.getByTestId("runtime-slots-role-custom"));
    await user.type(
      screen.getByTestId("runtime-slots-role-custom-name"),
      "写码",
    );
  }

  it("timeout: shows timeout copy and does not create a slot", async () => {
    const user = userEvent.setup();
    const gateway = new MockRuntimeSlotsGateway();
    gateway.setOauthLoginFail("runtime_slot_oauth_login_timeout");
    const createSpy = vi.spyOn(gateway, "createSlot");
    render(
      <RuntimeSlotsSection gateway={gateway} projectId={PROJECT} />,
    );

    await fillOpenaiOauthForm(user);
    await user.click(screen.getByTestId("runtime-slots-form-save"));

    await waitFor(() => {
      expect(screen.getByTestId("runtime-slots-error")).toHaveTextContent(
        OAUTH_LOGIN_TIMEOUT_MESSAGE,
      );
    });
    expect(createSpy).toHaveBeenCalled();
    expect(gateway.getSlotsSnapshot()).toHaveLength(0);
    expect(screen.queryByTestId("runtime-slots-list")).toBeNull();
    expect(screen.queryByText("已登录")).toBeNull();
    expect(screen.queryByText(/已通过 OAuth 创建/u)).toBeNull();
  });

  it("denied: shows denied copy and does not create a slot", async () => {
    const user = userEvent.setup();
    const gateway = new MockRuntimeSlotsGateway();
    gateway.setOauthLoginFail("runtime_slot_oauth_login_denied");
    render(
      <RuntimeSlotsSection gateway={gateway} projectId={PROJECT} />,
    );

    await fillOpenaiOauthForm(user);
    await user.click(screen.getByTestId("runtime-slots-form-save"));

    await waitFor(() => {
      expect(screen.getByTestId("runtime-slots-error")).toHaveTextContent(
        OAUTH_LOGIN_DENIED_MESSAGE,
      );
    });
    expect(gateway.getSlotsSnapshot()).toHaveLength(0);
  });

  it("state mismatch: shows state-mismatch copy and does not create a slot", async () => {
    const user = userEvent.setup();
    const gateway = new MockRuntimeSlotsGateway();
    gateway.setOauthLoginFail("runtime_slot_oauth_login_state_mismatch");
    render(
      <RuntimeSlotsSection gateway={gateway} projectId={PROJECT} />,
    );

    await fillOpenaiOauthForm(user);
    await user.click(screen.getByTestId("runtime-slots-form-save"));

    await waitFor(() => {
      expect(screen.getByTestId("runtime-slots-error")).toHaveTextContent(
        OAUTH_LOGIN_STATE_MISMATCH_MESSAGE,
      );
    });
    expect(gateway.getSlotsSnapshot()).toHaveLength(0);
  });

  it("browser open failed: shows readable copy + copyable authorize URL, no slot", async () => {
    const user = userEvent.setup();
    const gateway = new MockRuntimeSlotsGateway();
    const publicUrl =
      "https://as.example.test/oauth/authorize?client_id=test&state=abc";
    gateway.setOauthLoginFail(
      "runtime_slot_oauth_browser_open_failed",
      publicUrl,
    );
    render(
      <RuntimeSlotsSection gateway={gateway} projectId={PROJECT} />,
    );

    await fillOpenaiOauthForm(user);
    await user.click(screen.getByTestId("runtime-slots-form-save"));

    await waitFor(() => {
      expect(screen.getByTestId("runtime-slots-error")).toHaveTextContent(
        OAUTH_BROWSER_OPEN_FAILED_MESSAGE,
      );
    });
    const urlInput = screen.getByTestId(
      "runtime-slots-oauth-authorize-url",
    ) as HTMLInputElement;
    expect(urlInput.value).toBe(publicUrl);
    expect(urlInput).toHaveAttribute("readonly");
    expect(gateway.getSlotsSnapshot()).toHaveLength(0);
    expect(screen.queryByTestId("runtime-slots-list")).toBeNull();
    // No token-shaped material in the DOM fallback surface.
    assertMaterialAbsentFromDom(
      TOKEN_SENTINEL,
      REFRESH_SENTINEL,
      CODE_SENTINEL,
    );
  });

  it("shows in-progress copy while login is outstanding (no optimistic slot)", async () => {
    const user = userEvent.setup();
    const gateway = new MockRuntimeSlotsGateway();
    let rejectLogin!: (reason: Error) => void;
    const deferred = new Promise<RuntimeSlotView>((_resolve, reject) => {
      rejectLogin = reject;
    });
    vi.spyOn(gateway, "createSlot").mockImplementation(() => deferred);

    render(
      <RuntimeSlotsSection gateway={gateway} projectId={PROJECT} />,
    );

    await fillOpenaiOauthForm(user);
    await user.click(screen.getByTestId("runtime-slots-form-save"));

    await waitFor(() => {
      expect(
        screen.getByTestId("runtime-slots-oauth-in-progress"),
      ).toHaveTextContent(OAUTH_LOGIN_IN_PROGRESS);
    });
    // Still no list row while pending — failure/success must not paint early.
    expect(screen.queryByTestId("runtime-slots-list")).toBeNull();
    expect(gateway.getSlotsSnapshot()).toHaveLength(0);

    rejectLogin(
      new RuntimeSlotsGatewayError(
        "runtime_slot_oauth_login_timeout",
        OAUTH_LOGIN_TIMEOUT_MESSAGE,
      ),
    );

    await waitFor(() => {
      expect(screen.getByTestId("runtime-slots-error")).toHaveTextContent(
        OAUTH_LOGIN_TIMEOUT_MESSAGE,
      );
    });
    expect(gateway.getSlotsSnapshot()).toHaveLength(0);
    expect(screen.queryByTestId("runtime-slots-list")).toBeNull();
  });
});

describe("OAuth success display — account + expiry; tokens never in DOM", () => {
  it("creates oauth slot via createSlot and shows governed badge + account/expiry", async () => {
    const user = userEvent.setup();
    const gateway = new MockRuntimeSlotsGateway();
    gateway.setOauthLoginSuccessMeta({
      accountLabel: "dev@example.test",
      accessExpiresAt: "2026-09-01T00:00:00.000Z",
    });
    // Seed mock secrets that must never surface.
    gateway.seedSecret("access-ref", TOKEN_SENTINEL);
    gateway.seedSecret("refresh-ref", REFRESH_SENTINEL);

    const createSpy = vi.spyOn(gateway, "createSlot");
    render(
      <RuntimeSlotsSection gateway={gateway} projectId={PROJECT} />,
    );

    await user.click(await screen.findByTestId("runtime-slots-add"));
    await user.type(
      screen.getByTestId("runtime-slots-display-name"),
      "OA Success",
    );
    await user.click(screen.getByTestId("runtime-slots-auth-oauth"));
    await waitFor(() => {
      expect(
        screen.getByTestId("runtime-slots-oauth-provider"),
      ).toBeInTheDocument();
    });
    await user.selectOptions(
      screen.getByTestId("runtime-slots-oauth-provider"),
      "openai",
    );
    await user.type(
      screen.getByTestId("runtime-slots-oauth-field-clientId"),
      "user-owned-client-id",
    );
    await user.type(
      screen.getByTestId("runtime-slots-oauth-base-url"),
      "https://api.openai.com/v1",
    );
    await user.type(
      screen.getByTestId("runtime-slots-oauth-model-id"),
      "gpt-4o",
    );
    await user.click(screen.getByTestId("runtime-slots-role-controller"));
    await user.click(screen.getByTestId("runtime-slots-form-save"));

    await waitFor(() => {
      expect(screen.getByTestId("runtime-slots-list")).toBeInTheDocument();
    });

    expect(createSpy).toHaveBeenCalledWith(
      expect.objectContaining({
        authKind: "oauth",
        providerId: "openai",
        oauthSupplement: expect.objectContaining({
          clientId: "user-owned-client-id",
        }),
      }),
    );

    const slot = gateway.getSlotsSnapshot()[0]!;
    expect(slot.authMode).toBe("oauth");
    expect(slot.governance).toBe("full");
    expect(slot.role).toBe("controller");

    expect(
      screen.getByTestId(`runtime-slot-governed-${slot.slotId}`),
    ).toHaveTextContent(GOVERNED_GROK_BADGE);
    expect(screen.getByText(OAUTH_AUTH_BADGE)).toBeInTheDocument();
    expect(
      screen.getByTestId(`runtime-slot-oauth-account-${slot.slotId}`),
    ).toHaveTextContent("dev@example.test");
    expect(
      screen.getByTestId(`runtime-slot-oauth-expiry-${slot.slotId}`),
    ).not.toHaveTextContent(/^$/u);

    assertMaterialAbsentFromDom(
      TOKEN_SENTINEL,
      REFRESH_SENTINEL,
      CODE_SENTINEL,
      "mock-access-never-on-view",
      "mock-refresh-never-on-view",
    );
    // View itself must not carry token-shaped keys.
    expect(JSON.stringify(slot)).not.toContain(TOKEN_SENTINEL);
    expect(JSON.stringify(slot)).not.toMatch(/access_token|refresh_token/iu);
  });

  it("lists pre-seeded oauth slot with account label and never paints secrets", async () => {
    const gateway = new MockRuntimeSlotsGateway([
      fixtureOauthSlot({
        accountLabel: "listed@example.test",
        accessExpiresAt: "2026-10-01T12:00:00.000Z",
      }),
    ]);
    render(
      <RuntimeSlotsSection gateway={gateway} projectId={PROJECT} />,
    );

    await screen.findByTestId("runtime-slots-list");
    expect(
      screen.getByTestId("runtime-slot-oauth-account-slot-oauth-1"),
    ).toHaveTextContent("listed@example.test");
    expect(
      screen.getByTestId("runtime-slot-oauth-expiry-slot-oauth-1"),
    ).toBeInTheDocument();
    assertMaterialAbsentFromDom(TOKEN_SENTINEL, CODE_SENTINEL, REFRESH_SENTINEL);
  });
});

describe("OAuth controller vs local CLI (existing rule not weakened)", () => {
  it("allows 总控 when OAuth is selected", async () => {
    const user = userEvent.setup();
    const gateway = new MockRuntimeSlotsGateway();
    render(
      <RuntimeSlotsSection gateway={gateway} projectId={PROJECT} />,
    );

    await user.click(await screen.findByTestId("runtime-slots-add"));
    await user.click(screen.getByTestId("runtime-slots-auth-oauth"));

    const controllerBtn = screen.getByTestId("runtime-slots-role-controller");
    expect(controllerBtn).not.toBeDisabled();
  });
});
