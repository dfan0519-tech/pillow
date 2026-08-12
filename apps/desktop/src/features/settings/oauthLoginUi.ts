/**
 * Pure OAuth login UI helpers — catalog filtering, user-field requirements,
 * failure copy. No network, no DOM, no secrets.
 */

import {
  OAUTH_FIELD_ACCOUNT_LABEL_REASON,
  OAUTH_FIELD_AUTHORIZE_REASON,
  OAUTH_FIELD_CLIENT_ID_UNKNOWN_REASON,
  OAUTH_FIELD_CLIENT_ID_USER_SUPPLIED_REASON,
  OAUTH_FIELD_DEVICE_REASON,
  OAUTH_FIELD_ISSUER_REASON,
  OAUTH_FIELD_SCOPES_REASON,
  OAUTH_FIELD_TOKEN_REASON,
  OAUTH_LOGIN_DENIED_MESSAGE,
  OAUTH_LOGIN_GENERIC_FAILURE,
  OAUTH_LOGIN_STATE_MISMATCH_MESSAGE,
  OAUTH_LOGIN_TIMEOUT_MESSAGE,
  type OauthUserFieldId,
  type OauthUserFieldRequirement,
  type ProfileFieldWire,
  type ProviderLoginCatalogEntry,
} from "./runtimeSlotsTypes";

/**
 * Providers eligible for the OAuth create list.
 * `api_key_only` must never appear (positive + negative tests pin this).
 */
export function oauthCapableProviders(
  catalog: readonly ProviderLoginCatalogEntry[],
): ProviderLoginCatalogEntry[] {
  return catalog.filter((entry) => entry.loginMethod !== "api_key_only");
}

function needsUserInput(state: ProfileFieldWire): boolean {
  return state === "unknown" || state === "user_supplied";
}

/**
 * Fields the user must fill for this catalog row.
 *
 * - `Unknown` and `UserSupplied` → require input (never auto-fill a guess).
 * - `Known` / `NotApplicable` → do not require input.
 * - `accountLabel` only when strategy is `user_supplied`.
 */
export function oauthFieldsRequiringUserInput(
  entry: ProviderLoginCatalogEntry,
): OauthUserFieldRequirement[] {
  const fields: OauthUserFieldRequirement[] = [];

  if (needsUserInput(entry.clientId)) {
    fields.push({
      fieldId: "clientId",
      label: "client_id",
      reason:
        entry.clientId === "unknown"
          ? OAUTH_FIELD_CLIENT_ID_UNKNOWN_REASON
          : OAUTH_FIELD_CLIENT_ID_USER_SUPPLIED_REASON,
    });
  }
  if (needsUserInput(entry.issuer)) {
    fields.push({
      fieldId: "issuer",
      label: "issuer",
      reason: OAUTH_FIELD_ISSUER_REASON,
    });
  }
  if (needsUserInput(entry.authorizeEndpoint)) {
    fields.push({
      fieldId: "authorizeEndpoint",
      label: "authorize_endpoint",
      reason: OAUTH_FIELD_AUTHORIZE_REASON,
    });
  }
  if (needsUserInput(entry.deviceAuthorizationEndpoint)) {
    fields.push({
      fieldId: "deviceAuthorizationEndpoint",
      label: "device_authorization_endpoint",
      reason: OAUTH_FIELD_DEVICE_REASON,
    });
  }
  if (needsUserInput(entry.tokenEndpoint)) {
    fields.push({
      fieldId: "tokenEndpoint",
      label: "token_endpoint",
      reason: OAUTH_FIELD_TOKEN_REASON,
    });
  }
  if (needsUserInput(entry.scopes)) {
    fields.push({
      fieldId: "scopes",
      label: "scopes",
      reason: OAUTH_FIELD_SCOPES_REASON,
    });
  }
  if (entry.accountIdentifier === "user_supplied") {
    fields.push({
      fieldId: "accountLabel",
      label: "账号标识",
      reason: OAUTH_FIELD_ACCOUNT_LABEL_REASON,
    });
  }

  return fields;
}

/**
 * True when this field must be collected from the user for the entry.
 * Used by +/− unit tests without rendering the full form.
 */
export function oauthFieldRequiresUserInput(
  entry: ProviderLoginCatalogEntry,
  fieldId: OauthUserFieldId,
): boolean {
  return oauthFieldsRequiringUserInput(entry).some(
    (field) => field.fieldId === fieldId,
  );
}

/**
 * Map host OAuth login error codes to distinct, secret-free Chinese copy.
 * Timeout / denied / state-mismatch each stay distinguishable.
 */
export function mapOauthLoginErrorMessage(code: string): string {
  switch (code) {
    case "runtime_slot_oauth_login_timeout":
    case "runtime_slot_oauth_loopback_timeout":
    case "runtime_slot_oauth_http_timeout":
    case "runtime_slot_oauth_device_expired":
      return OAUTH_LOGIN_TIMEOUT_MESSAGE;
    case "runtime_slot_oauth_login_denied":
    case "runtime_slot_oauth_loopback_denied":
    case "runtime_slot_oauth_device_denied":
      return OAUTH_LOGIN_DENIED_MESSAGE;
    case "runtime_slot_oauth_login_state_mismatch":
    case "runtime_slot_oauth_loopback_state_mismatch":
      return OAUTH_LOGIN_STATE_MISMATCH_MESSAGE;
    default:
      return OAUTH_LOGIN_GENERIC_FAILURE;
  }
}

/** Whether a gateway error code is one of the three OAuth login failure modes. */
export function isOauthLoginFailureCode(code: string): boolean {
  return (
    code === "runtime_slot_oauth_login_timeout"
    || code === "runtime_slot_oauth_loopback_timeout"
    || code === "runtime_slot_oauth_http_timeout"
    || code === "runtime_slot_oauth_device_expired"
    || code === "runtime_slot_oauth_login_denied"
    || code === "runtime_slot_oauth_loopback_denied"
    || code === "runtime_slot_oauth_device_denied"
    || code === "runtime_slot_oauth_login_state_mismatch"
    || code === "runtime_slot_oauth_loopback_state_mismatch"
  );
}

/**
 * Format access expiry for list display. Invalid / empty → "—".
 * Never receives token material.
 */
export function formatOauthExpiryForDisplay(
  accessExpiresAt: string | null | undefined,
): string {
  if (!accessExpiresAt || !accessExpiresAt.trim()) return "—";
  const ms = Date.parse(accessExpiresAt);
  if (!Number.isFinite(ms)) return "—";
  try {
    return new Date(ms).toLocaleString();
  } catch {
    return accessExpiresAt;
  }
}

/**
 * Built-in mock catalog matching host provider_login profiles (public only).
 * `api_key_only` anthropic is included so tests can assert it is filtered out.
 * No guessed client_id values for Unknown rows.
 */
export function defaultMockProviderLoginCatalog(): ProviderLoginCatalogEntry[] {
  return [
    {
      providerId: "xai",
      displayName: "xAI",
      loginMethod: "device_code",
      issuer: "unknown",
      authorizeEndpoint: "not_applicable",
      deviceAuthorizationEndpoint: "known",
      tokenEndpoint: "known",
      scopes: "known",
      clientId: "unknown",
      knownIssuer: null,
      knownAuthorizeEndpoint: null,
      knownDeviceAuthorizationEndpoint:
        "https://auth.x.ai/oauth2/device/code",
      knownTokenEndpoint: "https://auth.x.ai/oauth2/token",
      knownScopes: [
        "openid",
        "profile",
        "email",
        "offline_access",
        "grok-cli:access",
        "api:access",
      ],
      knownClientId: null,
      accountIdentifier: "unknown",
    },
    {
      providerId: "github-copilot",
      displayName: "GitHub Copilot",
      loginMethod: "device_code",
      issuer: "unknown",
      authorizeEndpoint: "not_applicable",
      deviceAuthorizationEndpoint: "known",
      tokenEndpoint: "known",
      scopes: "known",
      clientId: "unknown",
      knownIssuer: null,
      knownAuthorizeEndpoint: null,
      knownDeviceAuthorizationEndpoint:
        "https://github.com/login/device/code",
      knownTokenEndpoint: "https://github.com/login/oauth/access_token",
      knownScopes: ["read:user"],
      knownClientId: null,
      accountIdentifier: "unknown",
    },
    {
      providerId: "anthropic",
      displayName: "Anthropic",
      loginMethod: "api_key_only",
      issuer: "not_applicable",
      authorizeEndpoint: "not_applicable",
      deviceAuthorizationEndpoint: "not_applicable",
      tokenEndpoint: "not_applicable",
      scopes: "not_applicable",
      clientId: "not_applicable",
      knownIssuer: null,
      knownAuthorizeEndpoint: null,
      knownDeviceAuthorizationEndpoint: null,
      knownTokenEndpoint: null,
      knownScopes: null,
      knownClientId: null,
      accountIdentifier: "not_applicable",
    },
    {
      providerId: "openai",
      displayName: "OpenAI",
      loginMethod: "pkce",
      issuer: "known",
      authorizeEndpoint: "known",
      deviceAuthorizationEndpoint: "not_applicable",
      tokenEndpoint: "known",
      scopes: "known",
      clientId: "unknown",
      knownIssuer: "https://auth.openai.com",
      knownAuthorizeEndpoint: "https://auth.openai.com/oauth/authorize",
      knownDeviceAuthorizationEndpoint: null,
      knownTokenEndpoint: "https://auth.openai.com/oauth/token",
      knownScopes: ["openid", "profile", "email", "offline_access"],
      knownClientId: null,
      accountIdentifier: "token_claims",
    },
    {
      providerId: "custom-oidc",
      displayName: "Custom OIDC",
      loginMethod: "pkce",
      issuer: "user_supplied",
      authorizeEndpoint: "user_supplied",
      deviceAuthorizationEndpoint: "not_applicable",
      tokenEndpoint: "user_supplied",
      scopes: "user_supplied",
      clientId: "user_supplied",
      knownIssuer: null,
      knownAuthorizeEndpoint: null,
      knownDeviceAuthorizationEndpoint: null,
      knownTokenEndpoint: null,
      knownScopes: null,
      knownClientId: null,
      accountIdentifier: "user_supplied",
    },
  ];
}
