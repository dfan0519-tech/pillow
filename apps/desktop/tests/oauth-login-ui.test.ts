/**
 * Pure OAuth login UI helpers: catalog filter, Unknown field requirements,
 * failure copy. No DOM, no real login.
 */
import { describe, expect, it } from "vitest";
import {
  catalogEntryOffersOauth,
  defaultMockProviderLoginCatalog,
  isOauthLoginFailureCode,
  looksLikePublicAuthorizeUrl,
  mapOauthLoginErrorMessage,
  oauthCapableProviders,
  oauthFieldRequiresUserInput,
  oauthFieldsRequiringUserInput,
  supportedAuthKindsForCatalogEntry,
} from "../src/features/settings/oauthLoginUi";
import {
  OAUTH_BROWSER_OPEN_FAILED_MESSAGE,
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

describe("oauthCapableProviders — Unknown client_id is not OAuth", () => {
  it("only UserSupplied / PublicDocumented client_id remain", () => {
    const capable = oauthCapableProviders(defaultMockProviderLoginCatalog());
    const ids = capable.map((entry) => entry.providerId);
    expect(ids).toEqual(["custom-oidc"]);
    expect(capable.every((entry) => catalogEntryOffersOauth(entry))).toBe(true);
  });

  it("Unknown client_id named providers are not listed (old contract was wrong)", () => {
    const capable = oauthCapableProviders(defaultMockProviderLoginCatalog());
    for (const id of ["openai", "xai", "github-copilot"]) {
      expect(capable.some((entry) => entry.providerId === id)).toBe(false);
      expect(catalogEntryOffersOauth(byId(id))).toBe(false);
    }
  });

  it("anthropic is not in the catalog at all", () => {
    const rows = defaultMockProviderLoginCatalog();
    expect(rows.length).toBeGreaterThan(0);
    expect(rows.some((entry) => entry.providerId === "anthropic")).toBe(false);
  });

  it("does not invent a DTO key for oauth offer — uses clientId", () => {
    const keys = Object.keys(byId("openai"));
    expect(keys).not.toContain("offersOauth");
    expect(keys).not.toContain("supportedAuthKinds");
    expect(keys).toContain("clientId");
    expect(byId("openai").clientId).toBe("unknown");
  });
});

describe("supportedAuthKindsForCatalogEntry — derived from catalog", () => {
  it("xai: api_key + grok_login, no oauth", () => {
    expect(supportedAuthKindsForCatalogEntry(byId("xai"))).toEqual([
      "api_key",
      "grok_login",
    ]);
  });

  it("openai / github-copilot: api_key only", () => {
    expect(supportedAuthKindsForCatalogEntry(byId("openai"))).toEqual([
      "api_key",
    ]);
    expect(supportedAuthKindsForCatalogEntry(byId("github-copilot"))).toEqual([
      "api_key",
    ]);
  });

  it("api_key_only is filtered even when the row carries a real client_id", () => {
    // No shipped provider is api_key_only since Anthropic was withdrawn, so
    // this row is built here. `clientId: "known"` means the only thing that
    // can exclude it is the login method.
    const entry: ProviderLoginCatalogEntry = {
      ...byId("openai"),
      providerId: "test-api-key-only",
      displayName: "Test ApiKeyOnly",
      loginMethod: "api_key_only",
      clientId: "known",
      knownClientId: "documented-client",
    };
    expect(catalogEntryOffersOauth(entry)).toBe(false);
    expect(supportedAuthKindsForCatalogEntry(entry)).toEqual(["api_key"]);
    expect(oauthCapableProviders([entry])).toEqual([]);
  });

  it("custom-oidc: oauth only", () => {
    expect(supportedAuthKindsForCatalogEntry(byId("custom-oidc"))).toEqual([
      "oauth",
    ]);
  });
});

describe("oauthFieldsRequiringUserInput — Unknown / Known (+/−)", () => {
  it("Unknown client_id providers do not offer OAuth (field helper is unused)", () => {
    for (const id of ["openai", "xai", "github-copilot"] as const) {
      const entry = byId(id);
      expect(entry.clientId).toBe("unknown");
      expect(entry.knownClientId).toBeNull();
      expect(catalogEntryOffersOauth(entry)).toBe(false);
      // Helper may still list the field, but the connect UI must not show OAuth.
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

  it("maps browser-open-failed to browser-open copy", () => {
    expect(
      mapOauthLoginErrorMessage("runtime_slot_oauth_browser_open_failed"),
    ).toBe(OAUTH_BROWSER_OPEN_FAILED_MESSAGE);
    expect(
      isOauthLoginFailureCode("runtime_slot_oauth_browser_open_failed"),
    ).toBe(true);
  });

  it("keeps the user-facing failure strings distinct", () => {
    expect(OAUTH_LOGIN_TIMEOUT_MESSAGE).not.toBe(OAUTH_LOGIN_DENIED_MESSAGE);
    expect(OAUTH_LOGIN_TIMEOUT_MESSAGE).not.toBe(
      OAUTH_LOGIN_STATE_MISMATCH_MESSAGE,
    );
    expect(OAUTH_LOGIN_DENIED_MESSAGE).not.toBe(
      OAUTH_LOGIN_STATE_MISMATCH_MESSAGE,
    );
    expect(OAUTH_BROWSER_OPEN_FAILED_MESSAGE).not.toBe(
      OAUTH_LOGIN_TIMEOUT_MESSAGE,
    );
  });

  it("looksLikePublicAuthorizeUrl accepts https authorize URLs only", () => {
    expect(
      looksLikePublicAuthorizeUrl(
        "https://as.example.test/oauth/authorize?client_id=x",
      ),
    ).toBe(true);
    expect(looksLikePublicAuthorizeUrl("not a url")).toBe(false);
    expect(looksLikePublicAuthorizeUrl("ftp://evil")).toBe(false);
  });
});
