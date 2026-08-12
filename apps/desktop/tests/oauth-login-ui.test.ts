/**
 * Pure OAuth login UI helpers: catalog filter, Unknown field requirements,
 * failure copy. No DOM, no real login.
 */
import { describe, expect, it } from "vitest";
import {
  defaultMockProviderLoginCatalog,
  isOauthLoginFailureCode,
  mapOauthLoginErrorMessage,
  oauthCapableProviders,
  oauthFieldRequiresUserInput,
  oauthFieldsRequiringUserInput,
} from "../src/features/settings/oauthLoginUi";
import {
  OAUTH_FIELD_CLIENT_ID_UNKNOWN_REASON,
  OAUTH_LOGIN_DENIED_MESSAGE,
  OAUTH_LOGIN_STATE_MISMATCH_MESSAGE,
  OAUTH_LOGIN_TIMEOUT_MESSAGE,
  type ProviderLoginCatalogEntry,
} from "../src/features/settings/runtimeSlotsTypes";

function byId(id: string): ProviderLoginCatalogEntry {
  const row = defaultMockProviderLoginCatalog().find(
    (entry) => entry.providerId === id,
  );
  if (!row) throw new Error(`missing catalog row ${id}`);
  return row;
}

describe("oauthCapableProviders — exclude api_key_only", () => {
  it("positive: pkce and device_code providers remain", () => {
    const capable = oauthCapableProviders(defaultMockProviderLoginCatalog());
    const ids = capable.map((entry) => entry.providerId);
    expect(ids).toContain("openai");
    expect(ids).toContain("xai");
    expect(ids).toContain("github-copilot");
    expect(ids).toContain("custom-oidc");
    expect(capable.every((entry) => entry.loginMethod !== "api_key_only")).toBe(
      true,
    );
  });

  it("negative: api_key_only anthropic is not listed", () => {
    const capable = oauthCapableProviders(defaultMockProviderLoginCatalog());
    expect(capable.some((entry) => entry.providerId === "anthropic")).toBe(
      false,
    );
    expect(
      defaultMockProviderLoginCatalog().some(
        (entry) =>
          entry.providerId === "anthropic"
          && entry.loginMethod === "api_key_only",
      ),
    ).toBe(true);
  });
});

describe("oauthFieldsRequiringUserInput — Unknown / Known (+/−)", () => {
  it("positive: Unknown client_id requires user fill with reason (openai, xai)", () => {
    for (const id of ["openai", "xai", "github-copilot"] as const) {
      const entry = byId(id);
      expect(entry.clientId).toBe("unknown");
      expect(entry.knownClientId).toBeNull();
      expect(oauthFieldRequiresUserInput(entry, "clientId")).toBe(true);
      const req = oauthFieldsRequiringUserInput(entry).find(
        (field) => field.fieldId === "clientId",
      );
      expect(req?.reason).toBe(OAUTH_FIELD_CLIENT_ID_UNKNOWN_REASON);
    }
  });

  it("positive: UserSupplied fields on custom-oidc all require input", () => {
    const entry = byId("custom-oidc");
    expect(oauthFieldRequiresUserInput(entry, "clientId")).toBe(true);
    expect(oauthFieldRequiresUserInput(entry, "issuer")).toBe(true);
    expect(oauthFieldRequiresUserInput(entry, "authorizeEndpoint")).toBe(true);
    expect(oauthFieldRequiresUserInput(entry, "tokenEndpoint")).toBe(true);
    expect(oauthFieldRequiresUserInput(entry, "scopes")).toBe(true);
    expect(oauthFieldRequiresUserInput(entry, "accountLabel")).toBe(true);
  });

  it("negative: Known fields must not require user input (openai issuer/endpoints)", () => {
    const entry = byId("openai");
    expect(entry.issuer).toBe("known");
    expect(entry.authorizeEndpoint).toBe("known");
    expect(entry.tokenEndpoint).toBe("known");
    expect(entry.scopes).toBe("known");
    expect(oauthFieldRequiresUserInput(entry, "issuer")).toBe(false);
    expect(oauthFieldRequiresUserInput(entry, "authorizeEndpoint")).toBe(false);
    expect(oauthFieldRequiresUserInput(entry, "tokenEndpoint")).toBe(false);
    expect(oauthFieldRequiresUserInput(entry, "scopes")).toBe(false);
    // account via token claims — not user-supplied
    expect(oauthFieldRequiresUserInput(entry, "accountLabel")).toBe(false);
  });

  it("negative: NotApplicable device endpoint does not require input on pkce", () => {
    const entry = byId("openai");
    expect(entry.deviceAuthorizationEndpoint).toBe("not_applicable");
    expect(
      oauthFieldRequiresUserInput(entry, "deviceAuthorizationEndpoint"),
    ).toBe(false);
  });
});

describe("mapOauthLoginErrorMessage — three failure modes", () => {
  it("maps timeout family codes to timeout copy", () => {
    for (const code of [
      "runtime_slot_oauth_login_timeout",
      "runtime_slot_oauth_loopback_timeout",
      "runtime_slot_oauth_http_timeout",
      "runtime_slot_oauth_device_expired",
    ]) {
      expect(mapOauthLoginErrorMessage(code)).toBe(OAUTH_LOGIN_TIMEOUT_MESSAGE);
      expect(isOauthLoginFailureCode(code)).toBe(true);
    }
  });

  it("maps denied family codes to denied copy", () => {
    for (const code of [
      "runtime_slot_oauth_login_denied",
      "runtime_slot_oauth_loopback_denied",
      "runtime_slot_oauth_device_denied",
    ]) {
      expect(mapOauthLoginErrorMessage(code)).toBe(OAUTH_LOGIN_DENIED_MESSAGE);
      expect(isOauthLoginFailureCode(code)).toBe(true);
    }
  });

  it("maps state-mismatch family codes to state mismatch copy", () => {
    for (const code of [
      "runtime_slot_oauth_login_state_mismatch",
      "runtime_slot_oauth_loopback_state_mismatch",
    ]) {
      expect(mapOauthLoginErrorMessage(code)).toBe(
        OAUTH_LOGIN_STATE_MISMATCH_MESSAGE,
      );
      expect(isOauthLoginFailureCode(code)).toBe(true);
    }
  });

  it("keeps the three user-facing strings distinct", () => {
    expect(OAUTH_LOGIN_TIMEOUT_MESSAGE).not.toBe(OAUTH_LOGIN_DENIED_MESSAGE);
    expect(OAUTH_LOGIN_TIMEOUT_MESSAGE).not.toBe(
      OAUTH_LOGIN_STATE_MISMATCH_MESSAGE,
    );
    expect(OAUTH_LOGIN_DENIED_MESSAGE).not.toBe(
      OAUTH_LOGIN_STATE_MISMATCH_MESSAGE,
    );
  });
});
