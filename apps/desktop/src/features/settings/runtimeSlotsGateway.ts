/**
 * Runtime-slots gateway — closed-key host DTO validation + secret-safe invoke.
 * apiKey is create/update input only; responses never carry key material.
 * Renderer never logs caught host values (may carry secrets).
 */

import { invoke } from "@tauri-apps/api/core";
import {
  defaultMockProviderLoginCatalog,
  mapOauthLoginErrorMessage,
} from "./oauthLoginUi";
import {
  type AccountIdentifierWire,
  type CreateExternalCliSlotInput,
  type CreateGrokApiKeySlotInput,
  type CreateGrokLoginSlotInput,
  type CreateSlotInput,
  DEFAULT_CUSTOM_ROLE_NAME,
  type ExternalCliStatusView,
  type GrokSlotAuthMode,
  LOCAL_CLI_AUTH_AUTHENTICATED_LABEL,
  LOCAL_CLI_AUTH_NOT_AUTHENTICATED_LABEL,
  LOCAL_CLI_AUTH_UNKNOWN_LABEL,
  type LocalCliAuthStatus,
  type LocalCliDetectionView,
  type LocalCliKind,
  MODEL_CATALOG_UNSUPPORTED_ENDPOINT,
  MODEL_CATALOG_UNSUPPORTED_EXTERNAL_CLI,
  MODEL_SWITCH_RESTARTED_MESSAGE,
  MODEL_SWITCH_SAVED_INACTIVE_MESSAGE,
  type ProbeModelCatalogView,
  type ProfileFieldWire,
  type ProviderLoginCatalogEntry,
  type ProviderLoginMethodWire,
  type RuntimeSlotGovernance,
  type RuntimeSlotKind,
  type RuntimeSlotModelCatalogReasonCode,
  type RuntimeSlotModelCatalogSource,
  type RuntimeSlotModelCatalogStatus,
  type RuntimeSlotModelCatalogView,
  type RuntimeSlotModelSessionRestore,
  type RuntimeSlotModelSwitchView,
  type RuntimeSlotRole,
  type RuntimeSlotStartView,
  type RuntimeSlotView,
  type RuntimeSlotsGateway,
  RuntimeSlotsGatewayError,
  type UpdateGrokApiKeySlotInput,
  type UpdateSlotInput,
} from "./runtimeSlotsTypes";

const INVALID_RESPONSE =
  "Pillow received an invalid runtime slot response.";
const FIXED_COMMAND_FAILED = "运行时槽位命令失败。";
const FIXED_UPDATE_UNAVAILABLE =
  "槽位更新命令尚未由宿主提供，请通过总控协调补齐后再编辑已有槽位。";

const MAX_LIST = 256;
const MAX_ID_CHARS = 128;
const MAX_NAME_CHARS = 256;
const MAX_URL_CHARS = 4_096;
const MAX_MODEL_CHARS = 4_096;
/** Host catalog cap: single model id length (Unicode chars). */
const MAX_CATALOG_MODEL_CHARS = 512;
/** Host catalog cap: model count. */
const MAX_CATALOG_MODELS = 256;
const MAX_PATH_CHARS = 1_024;
const MAX_ARGS = 64;
const MAX_ARG_CHARS = 1_024;
const MAX_STATUS_TEXT = 32_768;
const MAX_TS_CHARS = 128;

/** Exact closed allowlist for `RuntimeSlotDto` (host report). */
export const RUNTIME_SLOT_DTO_KEYS = [
  "slotId",
  "projectId",
  "displayName",
  "kind",
  "governance",
  "authMode",
  "baseUrl",
  "modelId",
  "secretRef",
  "secretConfigured",
  "executablePath",
  "launchArgs",
  "statusArgs",
  "role",
  "roleCustomName",
  "createdAt",
  "updatedAt",
  /** OAuth non-secret account label; null for non-OAuth. */
  "accountLabel",
  /** OAuth access-token expiry (RFC3339); null for non-OAuth. Never token text. */
  "accessExpiresAt",
] as const;

/** Exact closed allowlist for `ProviderLoginCatalogEntryDto`. */
export const PROVIDER_LOGIN_CATALOG_DTO_KEYS = [
  "providerId",
  "displayName",
  "loginMethod",
  "issuer",
  "authorizeEndpoint",
  "deviceAuthorizationEndpoint",
  "tokenEndpoint",
  "scopes",
  "clientId",
  "knownIssuer",
  "knownAuthorizeEndpoint",
  "knownDeviceAuthorizationEndpoint",
  "knownTokenEndpoint",
  "knownScopes",
  "knownClientId",
  "accountIdentifier",
] as const;

/** Exact closed allowlist for `ExternalCliStatusDto`. */
export const EXTERNAL_CLI_STATUS_DTO_KEYS = [
  "slotId",
  "projectId",
  "governance",
  "exitCode",
  "stdout",
  "stderr",
  "timedOut",
  "truncated",
] as const;

/** Exact closed allowlist for `RuntimeSlotStartDto`. */
export const RUNTIME_SLOT_START_DTO_KEYS = [
  "slotId",
  "projectId",
  "kind",
  "governance",
  "status",
] as const;

/** Exact closed allowlist for `RuntimeSlotModelCatalogDto`. */
export const RUNTIME_SLOT_MODEL_CATALOG_DTO_KEYS = [
  "slotId",
  "projectId",
  "status",
  "source",
  "models",
  "reasonCode",
] as const;

/** Exact closed allowlist for `RuntimeSlotModelSwitchDto`. */
export const RUNTIME_SLOT_MODEL_SWITCH_DTO_KEYS = [
  "slotId",
  "projectId",
  "modelId",
  "runtimeRestarted",
  "sessionRestore",
] as const;

/** Exact closed allowlist for `LocalCliDetectionDto` (host detect_local_clis). */
export const LOCAL_CLI_DETECTION_DTO_KEYS = [
  "kind",
  "found",
  "executablePath",
  "version",
  "authStatus",
  "accountLabel",
  "authReason",
] as const;

/** Exact closed allowlist for `ProbeModelCatalogDto` (host probe_model_catalog). */
export const PROBE_MODEL_CATALOG_DTO_KEYS = [
  "status",
  "source",
  "models",
  "reasonCode",
] as const;

const KINDS: readonly RuntimeSlotKind[] = ["governed_grok", "external_cli"];
const GOVERNANCE: readonly RuntimeSlotGovernance[] = ["full", "none"];
const AUTH_MODES: readonly GrokSlotAuthMode[] = ["login", "api_key", "oauth"];
const ROLES: readonly RuntimeSlotRole[] = ["controller", "librarian", "custom"];
const PROFILE_FIELD_WIRES: readonly ProfileFieldWire[] = [
  "known",
  "user_supplied",
  "unknown",
  "not_applicable",
];
const LOGIN_METHODS: readonly ProviderLoginMethodWire[] = [
  "pkce",
  "device_code",
  "api_key_only",
];
const ACCOUNT_IDENTIFIERS: readonly AccountIdentifierWire[] = [
  "token_claims",
  "user_supplied",
  "unknown",
  "not_applicable",
];
const CATALOG_STATUSES: readonly RuntimeSlotModelCatalogStatus[] = [
  "available",
  "unsupported",
];
const CATALOG_SOURCES: readonly RuntimeSlotModelCatalogSource[] = [
  "openai_models",
  "grok_cli",
  "external_cli",
];
const CATALOG_REASON_CODES: readonly RuntimeSlotModelCatalogReasonCode[] = [
  "endpoint_not_supported",
  "external_cli_not_supported",
];
const SESSION_RESTORES: readonly RuntimeSlotModelSessionRestore[] = [
  "not_applicable",
  "unavailable",
];
const LOCAL_CLI_KINDS: readonly LocalCliKind[] = [
  "grok",
  "claude",
  "codex",
  "opencode",
  "gemini",
];
const LOCAL_CLI_AUTH_STATUSES: readonly LocalCliAuthStatus[] = [
  "authenticated",
  "not_authenticated",
  "unknown",
];

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === "object" && value !== null && !Array.isArray(value);
}

function hasOnlyKeys(
  value: Record<string, unknown>,
  keys: readonly string[],
): boolean {
  const allowed = new Set(keys);
  return Object.keys(value).every((key) => allowed.has(key));
}

function isNonEmptyString(value: unknown): value is string {
  return typeof value === "string" && value.trim().length > 0;
}

function isBoundedString(
  value: unknown,
  maxChars: number,
): value is string {
  return typeof value === "string" && value.length <= maxChars;
}

function isOptionalBoundedString(
  value: unknown,
  maxChars: number,
): value is string | null | undefined {
  return value === null
    || value === undefined
    || isBoundedString(value, maxChars);
}

function isFiniteTimestamp(value: unknown): value is string {
  if (!isBoundedString(value, MAX_TS_CHARS) || value.length === 0) {
    return false;
  }
  return Number.isFinite(Date.parse(value));
}

function isStringArray(
  value: unknown,
  maxItems: number,
  maxItemChars: number,
): value is string[] {
  if (!Array.isArray(value) || value.length > maxItems) return false;
  return value.every(
    (item) => typeof item === "string" && item.length <= maxItemChars,
  );
}

function isOptionalStringArray(
  value: unknown,
  maxItems: number,
  maxItemChars: number,
): value is string[] | null | undefined {
  return value === null
    || value === undefined
    || isStringArray(value, maxItems, maxItemChars);
}

function isKind(value: unknown): value is RuntimeSlotKind {
  return typeof value === "string" && (KINDS as string[]).includes(value);
}

function isGovernance(value: unknown): value is RuntimeSlotGovernance {
  return typeof value === "string"
    && (GOVERNANCE as string[]).includes(value);
}

function isAuthMode(value: unknown): value is GrokSlotAuthMode {
  return typeof value === "string"
    && (AUTH_MODES as string[]).includes(value);
}

function isRole(value: unknown): value is RuntimeSlotRole {
  return typeof value === "string" && (ROLES as string[]).includes(value);
}

function invalidResponse(): RuntimeSlotsGatewayError {
  return new RuntimeSlotsGatewayError(
    "invalid_runtime_slot_response",
    INVALID_RESPONSE,
  );
}

/**
 * Reject response payloads that smuggle secret-shaped keys beyond the
 * host-approved status fields (`secretRef`, `secretConfigured`).
 * `apiKey` / tokens / codes / password etc. are never valid on slot DTOs.
 */
export function assertNoSecretPayloadFields(
  value: unknown,
  path = "",
): void {
  if (Array.isArray(value)) {
    value.forEach((item, index) => {
      assertNoSecretPayloadFields(item, `${path}[${index}]`);
    });
    return;
  }
  if (!isRecord(value)) return;

  for (const [key, child] of Object.entries(value)) {
    const lower = key.toLowerCase().replace(/[_-]/gu, "");
    if (
      lower === "apikey"
      || lower === "password"
      || lower === "token"
      || lower === "bearer"
      || lower === "authorization"
      || lower === "credential"
      || lower === "privatekey"
      || lower === "accesstoken"
      || lower === "refreshtoken"
      || lower === "idtoken"
      || lower === "authorizationcode"
      || lower === "authcode"
      || lower === "codeverifier"
      || lower === "clientsecret"
      || lower === "devicecode"
    ) {
      throw new RuntimeSlotsGatewayError(
        "invalid_runtime_slot_response",
        `Runtime slot response must not include secret field \`${path}${key}\`.`,
      );
    }
    // Nested objects only (top-level secretRef / secretConfigured are allow-listed).
    assertNoSecretPayloadFields(child, `${path}${key}.`);
  }
}

export function parseRuntimeSlotView(value: unknown): RuntimeSlotView {
  assertNoSecretPayloadFields(value);
  if (!isRecord(value) || !hasOnlyKeys(value, RUNTIME_SLOT_DTO_KEYS)) {
    throw invalidResponse();
  }
  if (
    !isNonEmptyString(value.slotId)
    || value.slotId.length > MAX_ID_CHARS
    || !isNonEmptyString(value.projectId)
    || value.projectId.length > MAX_ID_CHARS
    || !isBoundedString(value.displayName, MAX_NAME_CHARS)
    || !isKind(value.kind)
    || !isGovernance(value.governance)
    || !(
      value.authMode === null
      || value.authMode === undefined
      || isAuthMode(value.authMode)
    )
    || !isOptionalBoundedString(value.baseUrl, MAX_URL_CHARS)
    || !isOptionalBoundedString(value.modelId, MAX_MODEL_CHARS)
    || !isOptionalBoundedString(value.secretRef, MAX_ID_CHARS)
    || typeof value.secretConfigured !== "boolean"
    || !isOptionalBoundedString(value.executablePath, MAX_PATH_CHARS)
    || !isOptionalStringArray(value.launchArgs, MAX_ARGS, MAX_ARG_CHARS)
    || !isOptionalStringArray(value.statusArgs, MAX_ARGS, MAX_ARG_CHARS)
    || !isRole(value.role)
    || !(
      value.roleCustomName === null
      || value.roleCustomName === undefined
      || isBoundedString(value.roleCustomName, 64)
    )
    || !isFiniteTimestamp(value.createdAt)
    || !isFiniteTimestamp(value.updatedAt)
    || !isOptionalBoundedString(value.accountLabel, 256)
    || !(
      value.accessExpiresAt === null
      || value.accessExpiresAt === undefined
      || isFiniteTimestamp(value.accessExpiresAt)
    )
  ) {
    throw invalidResponse();
  }

  // Host invariants mirrored for fail-closed UI (do not invent governance).
  if (value.kind === "governed_grok" && value.governance !== "full") {
    throw invalidResponse();
  }
  if (value.kind === "external_cli" && value.governance !== "none") {
    throw invalidResponse();
  }
  // custom requires a non-blank name; controller/librarian require null name.
  if (value.role === "custom") {
    const name = value.roleCustomName;
    if (typeof name !== "string" || name.trim().length === 0) {
      throw invalidResponse();
    }
  } else if (value.roleCustomName != null) {
    throw invalidResponse();
  }
  // Controller cannot be ungoverned LocalCli (half-open trap).
  if (value.role === "controller" && value.governance !== "full") {
    throw invalidResponse();
  }

  return {
    slotId: value.slotId,
    projectId: value.projectId,
    displayName: value.displayName,
    kind: value.kind,
    governance: value.governance,
    authMode: value.authMode === undefined ? null : value.authMode,
    baseUrl: value.baseUrl ?? null,
    modelId: value.modelId ?? null,
    secretRef: value.secretRef ?? null,
    secretConfigured: value.secretConfigured,
    executablePath: value.executablePath ?? null,
    launchArgs: value.launchArgs ?? null,
    statusArgs: value.statusArgs ?? null,
    role: value.role,
    roleCustomName: value.roleCustomName ?? null,
    createdAt: value.createdAt,
    updatedAt: value.updatedAt,
    accountLabel:
      value.accountLabel === undefined ? null : value.accountLabel,
    accessExpiresAt:
      value.accessExpiresAt === undefined ? null : value.accessExpiresAt,
  };
}

export function parseExternalCliStatusView(
  value: unknown,
): ExternalCliStatusView {
  assertNoSecretPayloadFields(value);
  if (
    !isRecord(value)
    || !hasOnlyKeys(value, EXTERNAL_CLI_STATUS_DTO_KEYS)
  ) {
    throw invalidResponse();
  }
  if (
    !isNonEmptyString(value.slotId)
    || value.slotId.length > MAX_ID_CHARS
    || !isNonEmptyString(value.projectId)
    || value.projectId.length > MAX_ID_CHARS
    || value.governance !== "none"
    || !(
      value.exitCode === null
      || value.exitCode === undefined
      || (
        typeof value.exitCode === "number"
        && Number.isSafeInteger(value.exitCode)
      )
    )
    || !isBoundedString(value.stdout, MAX_STATUS_TEXT)
    || !isBoundedString(value.stderr, MAX_STATUS_TEXT)
    || typeof value.timedOut !== "boolean"
    || typeof value.truncated !== "boolean"
  ) {
    throw invalidResponse();
  }
  return {
    slotId: value.slotId,
    projectId: value.projectId,
    governance: "none",
    exitCode: value.exitCode === undefined ? null : value.exitCode,
    stdout: value.stdout,
    stderr: value.stderr,
    timedOut: value.timedOut,
    truncated: value.truncated,
  };
}

export function parseRuntimeSlotStartView(
  value: unknown,
): RuntimeSlotStartView {
  assertNoSecretPayloadFields(value);
  if (
    !isRecord(value)
    || !hasOnlyKeys(value, RUNTIME_SLOT_START_DTO_KEYS)
  ) {
    throw invalidResponse();
  }
  if (
    !isNonEmptyString(value.slotId)
    || value.slotId.length > MAX_ID_CHARS
    || !isNonEmptyString(value.projectId)
    || value.projectId.length > MAX_ID_CHARS
    || value.kind !== "governed_grok"
    || value.governance !== "full"
    || !isNonEmptyString(value.status)
    || value.status.length > 64
  ) {
    throw invalidResponse();
  }
  return {
    slotId: value.slotId,
    projectId: value.projectId,
    kind: "governed_grok",
    governance: "full",
    status: value.status,
  };
}

function isCatalogStatus(
  value: unknown,
): value is RuntimeSlotModelCatalogStatus {
  return typeof value === "string"
    && (CATALOG_STATUSES as string[]).includes(value);
}

function isCatalogSource(
  value: unknown,
): value is RuntimeSlotModelCatalogSource {
  return typeof value === "string"
    && (CATALOG_SOURCES as string[]).includes(value);
}

function isCatalogReasonCode(
  value: unknown,
): value is RuntimeSlotModelCatalogReasonCode {
  return typeof value === "string"
    && (CATALOG_REASON_CODES as string[]).includes(value);
}

function isSessionRestore(
  value: unknown,
): value is RuntimeSlotModelSessionRestore {
  return typeof value === "string"
    && (SESSION_RESTORES as string[]).includes(value);
}

/**
 * Closed parse for host model catalog. `unsupported` is a valid success shape
 * (empty models + reasonCode). Rejects smuggled keys and over-long ids.
 */
export function parseRuntimeSlotModelCatalogView(
  value: unknown,
): RuntimeSlotModelCatalogView {
  assertNoSecretPayloadFields(value);
  if (
    !isRecord(value)
    || !hasOnlyKeys(value, RUNTIME_SLOT_MODEL_CATALOG_DTO_KEYS)
  ) {
    throw invalidResponse();
  }
  if (
    !isNonEmptyString(value.slotId)
    || value.slotId.length > MAX_ID_CHARS
    || !isNonEmptyString(value.projectId)
    || value.projectId.length > MAX_ID_CHARS
    || !isCatalogStatus(value.status)
    || !isCatalogSource(value.source)
    || !(
      value.reasonCode === null
      || value.reasonCode === undefined
      || isCatalogReasonCode(value.reasonCode)
    )
    || !Array.isArray(value.models)
    || value.models.length > MAX_CATALOG_MODELS
    || !value.models.every(
      (item) =>
        typeof item === "string"
        && item.length > 0
        && item.length <= MAX_CATALOG_MODEL_CHARS,
    )
  ) {
    throw invalidResponse();
  }

  // Host invariants: available must not carry a reason; unsupported must.
  if (value.status === "available" && value.reasonCode != null) {
    throw invalidResponse();
  }
  if (value.status === "unsupported") {
    if (value.reasonCode == null) {
      throw invalidResponse();
    }
    if (value.models.length > 0) {
      throw invalidResponse();
    }
  }

  return {
    slotId: value.slotId,
    projectId: value.projectId,
    status: value.status,
    source: value.source,
    models: value.models.slice(),
    reasonCode: value.reasonCode ?? null,
  };
}

/**
 * Closed parse for model switch result. `runtimeRestarted` drives honest UI.
 */
export function parseRuntimeSlotModelSwitchView(
  value: unknown,
): RuntimeSlotModelSwitchView {
  assertNoSecretPayloadFields(value);
  if (
    !isRecord(value)
    || !hasOnlyKeys(value, RUNTIME_SLOT_MODEL_SWITCH_DTO_KEYS)
  ) {
    throw invalidResponse();
  }
  if (
    !isNonEmptyString(value.slotId)
    || value.slotId.length > MAX_ID_CHARS
    || !isNonEmptyString(value.projectId)
    || value.projectId.length > MAX_ID_CHARS
    || !isNonEmptyString(value.modelId)
    || value.modelId.length > MAX_CATALOG_MODEL_CHARS
    || typeof value.runtimeRestarted !== "boolean"
    || !isSessionRestore(value.sessionRestore)
  ) {
    throw invalidResponse();
  }

  // Host: active restart → sessionRestore unavailable; inactive → not_applicable.
  if (value.runtimeRestarted && value.sessionRestore !== "unavailable") {
    throw invalidResponse();
  }
  if (!value.runtimeRestarted && value.sessionRestore !== "not_applicable") {
    throw invalidResponse();
  }

  return {
    slotId: value.slotId,
    projectId: value.projectId,
    modelId: value.modelId,
    runtimeRestarted: value.runtimeRestarted,
    sessionRestore: value.sessionRestore,
  };
}

/** Short display for untrusted remote model ids (layout safety). */
export function truncateModelIdForDisplay(
  modelId: string,
  maxChars = 48,
): string {
  if (modelId.length <= maxChars) return modelId;
  return `${modelId.slice(0, Math.max(0, maxChars - 1))}…`;
}

/** Map catalog reason codes to user-facing fallback copy. */
export function mapModelCatalogUnsupportedReason(
  reasonCode: RuntimeSlotModelCatalogReasonCode | null,
): string {
  switch (reasonCode) {
    case "endpoint_not_supported":
      return MODEL_CATALOG_UNSUPPORTED_ENDPOINT;
    case "external_cli_not_supported":
      return MODEL_CATALOG_UNSUPPORTED_EXTERNAL_CLI;
    default:
      return "此槽位不支持自动拉取模型列表。请手输模型 id。";
  }
}

/**
 * Format host auth three-state for UI.
 *
 * Critical: `unknown` must render as 未确定 (+ optional reason), never 未登录.
 * Absence of evidence is not negative evidence.
 */
export function formatLocalCliAuthLabel(
  view: Pick<LocalCliDetectionView, "authStatus" | "accountLabel" | "authReason">,
): string {
  if (view.authStatus === "authenticated") {
    const label = view.accountLabel?.trim();
    return label
      ? `${LOCAL_CLI_AUTH_AUTHENTICATED_LABEL}（${label}）`
      : LOCAL_CLI_AUTH_AUTHENTICATED_LABEL;
  }
  if (view.authStatus === "not_authenticated") {
    return LOCAL_CLI_AUTH_NOT_AUTHENTICATED_LABEL;
  }
  // unknown — and any unexpected value fail-closed to 未确定, never 未登录.
  const reason = view.authReason?.trim();
  return reason
    ? `${LOCAL_CLI_AUTH_UNKNOWN_LABEL}（${reason}）`
    : LOCAL_CLI_AUTH_UNKNOWN_LABEL;
}

/** Closed parse for one local CLI detection row. */
export function parseLocalCliDetectionView(
  value: unknown,
): LocalCliDetectionView {
  assertNoSecretPayloadFields(value);
  if (
    !isRecord(value)
    || !hasOnlyKeys(value, LOCAL_CLI_DETECTION_DTO_KEYS)
  ) {
    throw invalidResponse();
  }
  if (
    typeof value.kind !== "string"
    || !(LOCAL_CLI_KINDS as string[]).includes(value.kind)
    || typeof value.found !== "boolean"
    || !isOptionalBoundedString(value.executablePath, MAX_PATH_CHARS)
    || !isOptionalBoundedString(value.version, 128)
    || typeof value.authStatus !== "string"
    || !(LOCAL_CLI_AUTH_STATUSES as string[]).includes(value.authStatus)
    || !isOptionalBoundedString(value.accountLabel, 256)
    || !isOptionalBoundedString(value.authReason, 256)
  ) {
    throw invalidResponse();
  }
  // Host invariants mirrored fail-closed.
  if (value.authStatus === "authenticated") {
    // accountLabel may be a generic "authenticated" string; reason must be null.
    if (value.authReason != null) {
      throw invalidResponse();
    }
  } else if (value.accountLabel != null) {
    throw invalidResponse();
  }
  if (value.authStatus === "not_authenticated" && value.authReason != null) {
    throw invalidResponse();
  }

  return {
    kind: value.kind as LocalCliKind,
    found: value.found,
    executablePath: value.executablePath ?? null,
    version: value.version ?? null,
    authStatus: value.authStatus as LocalCliAuthStatus,
    accountLabel: value.accountLabel ?? null,
    authReason: value.authReason ?? null,
  };
}

function isProfileFieldWire(value: unknown): value is ProfileFieldWire {
  return typeof value === "string"
    && (PROFILE_FIELD_WIRES as string[]).includes(value);
}

function isLoginMethodWire(value: unknown): value is ProviderLoginMethodWire {
  return typeof value === "string"
    && (LOGIN_METHODS as string[]).includes(value);
}

function isAccountIdentifierWire(
  value: unknown,
): value is AccountIdentifierWire {
  return typeof value === "string"
    && (ACCOUNT_IDENTIFIERS as string[]).includes(value);
}

/**
 * Closed parse for one provider login catalog row.
 * Rejects smuggled secret keys and unknown keys; never invents client_id.
 */
export function parseProviderLoginCatalogEntry(
  value: unknown,
): ProviderLoginCatalogEntry {
  assertNoSecretPayloadFields(value);
  if (
    !isRecord(value)
    || !hasOnlyKeys(value, PROVIDER_LOGIN_CATALOG_DTO_KEYS)
  ) {
    throw invalidResponse();
  }
  if (
    !isNonEmptyString(value.providerId)
    || value.providerId.length > MAX_ID_CHARS
    || !isBoundedString(value.displayName, MAX_NAME_CHARS)
    || !isLoginMethodWire(value.loginMethod)
    || !isProfileFieldWire(value.issuer)
    || !isProfileFieldWire(value.authorizeEndpoint)
    || !isProfileFieldWire(value.deviceAuthorizationEndpoint)
    || !isProfileFieldWire(value.tokenEndpoint)
    || !isProfileFieldWire(value.scopes)
    || !isProfileFieldWire(value.clientId)
    || !isOptionalBoundedString(value.knownIssuer, MAX_URL_CHARS)
    || !isOptionalBoundedString(value.knownAuthorizeEndpoint, MAX_URL_CHARS)
    || !isOptionalBoundedString(
      value.knownDeviceAuthorizationEndpoint,
      MAX_URL_CHARS,
    )
    || !isOptionalBoundedString(value.knownTokenEndpoint, MAX_URL_CHARS)
    || !isOptionalBoundedString(value.knownClientId, MAX_ID_CHARS)
    || !isAccountIdentifierWire(value.accountIdentifier)
    || !(
      value.knownScopes === null
      || value.knownScopes === undefined
      || isStringArray(value.knownScopes, 64, 128)
    )
  ) {
    throw invalidResponse();
  }

  // Fail closed: unknown client_id must not smuggle a knownClientId guess.
  if (value.clientId === "unknown" && value.knownClientId != null) {
    throw invalidResponse();
  }
  // api_key_only must not advertise OAuth endpoints as known for login.
  if (value.loginMethod === "api_key_only" && value.knownClientId != null) {
    throw invalidResponse();
  }

  return {
    providerId: value.providerId,
    displayName: value.displayName,
    loginMethod: value.loginMethod,
    issuer: value.issuer,
    authorizeEndpoint: value.authorizeEndpoint,
    deviceAuthorizationEndpoint: value.deviceAuthorizationEndpoint,
    tokenEndpoint: value.tokenEndpoint,
    scopes: value.scopes,
    clientId: value.clientId,
    knownIssuer: value.knownIssuer ?? null,
    knownAuthorizeEndpoint: value.knownAuthorizeEndpoint ?? null,
    knownDeviceAuthorizationEndpoint:
      value.knownDeviceAuthorizationEndpoint ?? null,
    knownTokenEndpoint: value.knownTokenEndpoint ?? null,
    knownScopes: value.knownScopes ? value.knownScopes.slice() : null,
    knownClientId: value.knownClientId ?? null,
    accountIdentifier: value.accountIdentifier,
  };
}

/** Closed parse for draft model catalog probe result. */
export function parseProbeModelCatalogView(
  value: unknown,
): ProbeModelCatalogView {
  assertNoSecretPayloadFields(value);
  if (
    !isRecord(value)
    || !hasOnlyKeys(value, PROBE_MODEL_CATALOG_DTO_KEYS)
  ) {
    throw invalidResponse();
  }
  if (
    !isCatalogStatus(value.status)
    || !isCatalogSource(value.source)
    || !(
      value.reasonCode === null
      || value.reasonCode === undefined
      || isCatalogReasonCode(value.reasonCode)
    )
    || !Array.isArray(value.models)
    || value.models.length > MAX_CATALOG_MODELS
    || !value.models.every(
      (item) =>
        typeof item === "string"
        && item.length > 0
        && item.length <= MAX_CATALOG_MODEL_CHARS,
    )
  ) {
    throw invalidResponse();
  }
  if (value.status === "available" && value.reasonCode != null) {
    throw invalidResponse();
  }
  if (value.status === "unsupported") {
    if (value.reasonCode == null) {
      throw invalidResponse();
    }
    if (value.models.length > 0) {
      throw invalidResponse();
    }
  }
  return {
    status: value.status,
    source: value.source,
    models: value.models.slice(),
    reasonCode: value.reasonCode ?? null,
  };
}

/** Honest switch outcome copy — never invent "already applied to session". */
export function mapModelSwitchResultMessage(
  result: RuntimeSlotModelSwitchView,
): string {
  if (result.runtimeRestarted) {
    return MODEL_SWITCH_RESTARTED_MESSAGE;
  }
  return MODEL_SWITCH_SAVED_INACTIVE_MESSAGE;
}

/**
 * Create invoke args — apiKey is always required and always sent once.
 * Caller must not log the returned object.
 */
export function buildCreateGrokApiKeySlotArgs(
  input: CreateGrokApiKeySlotInput,
): {
  projectId: string;
  displayName: string;
  baseUrl: string;
  modelId: string;
  apiKey: string;
} {
  return {
    projectId: input.projectId,
    displayName: input.displayName,
    baseUrl: input.baseUrl,
    modelId: input.modelId,
    apiKey: input.apiKey,
  };
}

/**
 * Update invoke args. Empty/whitespace apiKey is **omitted** so the host
 * keeps the stored secret (blank is not clear). Non-empty apiKey is sent.
 */
export function buildUpdateGrokApiKeySlotArgs(
  input: UpdateGrokApiKeySlotInput,
): {
  projectId: string;
  slotId: string;
  displayName: string;
  baseUrl: string;
  modelId: string;
  apiKey?: string;
} {
  const args: {
    projectId: string;
    slotId: string;
    displayName: string;
    baseUrl: string;
    modelId: string;
    apiKey?: string;
  } = {
    projectId: input.projectId,
    slotId: input.slotId,
    displayName: input.displayName,
    baseUrl: input.baseUrl,
    modelId: input.modelId,
  };
  const trimmed = input.apiKey.trim();
  if (trimmed.length > 0) {
    args.apiKey = trimmed;
  }
  return args;
}

/**
 * Map host error codes to actionable, secret-free Chinese copy.
 * Never surface host `message` text (may echo paths, argv, or key fragments).
 */
export function mapRuntimeSlotErrorMessage(code: string): string {
  switch (code) {
    case "runtime_slot_input_invalid":
      return "配置无效：请检查端点 URL、模型 id、可执行路径或参数。";
    case "runtime_slot_not_found":
      return "找不到该槽位（可能已删除或不属于当前项目）。";
    case "runtime_slot_not_governed_grok":
      return "该操作仅适用于受治理的 Grok 槽位。";
    case "runtime_slot_store_unavailable":
      return "槽位存储不可用，请稍后重试。";
    case "runtime_slot_project_unavailable":
    case "project_not_found":
      return "项目不可用：请先打开并信任项目。";
    case "runtime_slot_secret_conflict":
      return "密钥存储冲突，请新建槽位或联系维护者清理。";
    case "runtime_slot_secret_missing":
      return "密钥未配置或已丢失：请重新填写 API key。";
    case "runtime_slot_secret_unavailable":
      return "无法读取已保存的密钥（当前 Windows 用户可能无法解密）。";
    case "runtime_slot_secret_unsupported":
      return "当前平台不支持密钥存储。";
    case "runtime_slot_private_home_contaminated":
      return "槽位私有目录含有禁止的登录文件，请删除后重试。";
    case "runtime_slot_config_unavailable":
      return "无法写入槽位模型配置。";
    case "runtime_slot_model_catalog_unavailable":
      return "无法拉取模型目录：端点不可达、认证失败或服务暂时不可用。请手输模型 id 或稍后刷新。";
    case "runtime_slot_model_catalog_invalid":
      return "模型目录响应无效。请手输模型 id，或检查端点返回格式后刷新。";
    case "external_cli_status_unavailable":
      return "无法查询外部 CLI 账号状态：请检查可执行路径与状态参数。";
    case "external_cli_launch_unsupported":
      return "外部 CLI 槽位不能作为受治理代理启动，仅可查询账号状态。";
    case "runtime_slot_id_invalid":
      return "槽位 id 无效。";
    case "runtime_slot_project_mismatch":
      return "槽位不属于当前项目，已拒绝跨项目使用。";
    case "runtime_slot_update_unavailable":
      return FIXED_UPDATE_UNAVAILABLE;
    case "runtime_slot_controller_already_exists":
      return "该项目已有总控槽位，请先降级或删除现有总控后再指定。";
    case "runtime_slot_controller_requires_governed_auth":
      return "总控角色必须使用受治理认证（API key、Grok 登录或 OAuth），不能使用本地 CLI。";
    case "runtime_slot_oauth_login_timeout":
    case "runtime_slot_oauth_loopback_timeout":
    case "runtime_slot_oauth_http_timeout":
    case "runtime_slot_oauth_device_expired":
    case "runtime_slot_oauth_login_denied":
    case "runtime_slot_oauth_loopback_denied":
    case "runtime_slot_oauth_device_denied":
    case "runtime_slot_oauth_login_state_mismatch":
    case "runtime_slot_oauth_loopback_state_mismatch":
      return mapOauthLoginErrorMessage(code);
    case "runtime_slot_provider_login_catalog_unavailable":
      return "无法加载 OAuth 提供方目录，请稍后重试。";
    case "invalid_runtime_slot_response":
      return INVALID_RESPONSE;
    default:
      return FIXED_COMMAND_FAILED;
  }
}

/**
 * Probe failure copy for governed Grok start path — actionable, no host dump.
 */
export function mapGrokProbeFailureMessage(code: string): string {
  switch (code) {
    case "runtime_slot_input_invalid":
      return "探活失败：端点或模型配置无效。";
    case "runtime_slot_secret_missing":
    case "runtime_slot_secret_unavailable":
      return "探活失败：认证失败或密钥不可用，请检查 API key。";
    case "runtime_slot_config_unavailable":
      return "探活失败：无法准备模型配置。";
    case "runtime_slot_private_home_contaminated":
      return "探活失败：私有配置目录异常。";
    case "runtime_slot_not_found":
      return "探活失败：槽位不存在。";
    default:
      // Cover endpoint / auth / model without echoing host text.
      return "探活失败：无法启动该槽位。请核对端点是否可达、认证是否有效、模型 id 是否存在。";
  }
}

/**
 * Classify external CLI status for UI. Raw stdout is shown only when present;
 * never invent success from missing exit code alone when timed out.
 */
export function classifyExternalCliStatus(
  status: ExternalCliStatusView,
): { ok: boolean; summary: string } {
  if (status.timedOut) {
    return {
      ok: false,
      summary: "状态命令超时。请检查可执行文件是否卡住或路径错误。",
    };
  }
  if (status.exitCode === 0) {
    return { ok: true, summary: "状态命令成功。" };
  }
  if (status.exitCode === null) {
    return {
      ok: false,
      summary: "状态命令未返回退出码，请检查可执行路径。",
    };
  }
  return {
    ok: false,
    summary: `状态命令失败（退出码 ${status.exitCode}）。`,
  };
}

export function normalizeRuntimeSlotsError(
  error: unknown,
): RuntimeSlotsGatewayError {
  if (error instanceof RuntimeSlotsGatewayError) return error;
  if (isRecord(error) && typeof error.code === "string") {
    const code = error.code;
    // Never use host message text — may carry secrets or paths.
    return new RuntimeSlotsGatewayError(
      code,
      mapRuntimeSlotErrorMessage(code),
    );
  }
  return new RuntimeSlotsGatewayError(
    "runtime_slots_command_failed",
    FIXED_COMMAND_FAILED,
  );
}

async function invokeSlotCommand<T>(
  command: string,
  args: Record<string, unknown>,
  parse: (payload: unknown) => T,
): Promise<T> {
  try {
    const payload = await invoke<unknown>(command, args);
    return parse(payload);
  } catch (cause) {
    // Never log `cause` — host errors may carry secret-shaped detail.
    throw normalizeRuntimeSlotsError(cause);
  }
}

/** Tauri runtime-slots gateway. */
export class TauriRuntimeSlotsGateway implements RuntimeSlotsGateway {
  listRuntimeSlots(projectId: string): Promise<RuntimeSlotView[]> {
    return invokeSlotCommand(
      "list_runtime_slots",
      { projectId },
      (payload) => {
        if (!Array.isArray(payload) || payload.length > MAX_LIST) {
          throw invalidResponse();
        }
        return payload.map(parseRuntimeSlotView);
      },
    );
  }

  createSlot(input: CreateSlotInput): Promise<RuntimeSlotView> {
    const args: Record<string, unknown> = {
      projectId: input.projectId,
      displayName: input.displayName,
      authKind: input.authKind,
    };
    if (input.role) args.role = input.role;
    if (input.roleCustomName) args.roleCustomName = input.roleCustomName;
    if (input.baseUrl !== undefined) args.baseUrl = input.baseUrl;
    if (input.modelId !== undefined) args.modelId = input.modelId;
    if (input.apiKey !== undefined) args.apiKey = input.apiKey;
    if (input.executablePath !== undefined) {
      args.executablePath = input.executablePath;
    }
    if (input.launchArgs !== undefined) args.launchArgs = input.launchArgs;
    if (input.statusArgs !== undefined) args.statusArgs = input.statusArgs;
    if (input.providerId !== undefined) args.providerId = input.providerId;
    if (input.oauthSupplement !== undefined) {
      // Public metadata only — never tokens/codes.
      args.oauthSupplement = { ...input.oauthSupplement };
    }
    return invokeSlotCommand("create_slot", args, parseRuntimeSlotView);
  }

  updateSlot(input: UpdateSlotInput): Promise<RuntimeSlotView> {
    const args: Record<string, unknown> = {
      projectId: input.projectId,
      slotId: input.slotId,
      displayName: input.displayName,
    };
    if (input.role !== undefined) args.role = input.role;
    if (input.roleCustomName !== undefined) {
      args.roleCustomName = input.roleCustomName;
    }
    if (input.baseUrl !== undefined) args.baseUrl = input.baseUrl;
    if (input.modelId !== undefined) args.modelId = input.modelId;
    // Empty / whitespace key = keep secret (omit from wire).
    if (input.apiKey !== undefined && input.apiKey.trim().length > 0) {
      args.apiKey = input.apiKey.trim();
    }
    if (input.executablePath !== undefined) {
      args.executablePath = input.executablePath;
    }
    if (input.launchArgs !== undefined) args.launchArgs = input.launchArgs;
    if (input.statusArgs !== undefined) args.statusArgs = input.statusArgs;
    if (input.keepAuth !== undefined) args.keepAuth = input.keepAuth;
    return invokeSlotCommand("update_slot", args, parseRuntimeSlotView);
  }

  createGrokLoginSlot(
    input: CreateGrokLoginSlotInput,
  ): Promise<RuntimeSlotView> {
    // Host lib.rs still registers the legacy name; domain path is create_slot.
    return invokeSlotCommand(
      "create_grok_login_slot",
      {
        projectId: input.projectId,
        displayName: input.displayName,
      },
      parseRuntimeSlotView,
    );
  }

  createGrokApiKeySlot(
    input: CreateGrokApiKeySlotInput,
  ): Promise<RuntimeSlotView> {
    return invokeSlotCommand(
      "create_grok_api_key_slot",
      buildCreateGrokApiKeySlotArgs(input),
      parseRuntimeSlotView,
    );
  }

  updateGrokApiKeySlot(
    input: UpdateGrokApiKeySlotInput,
  ): Promise<RuntimeSlotView> {
    // Host foundation report has no update command yet. Keep the renderer
    // contract ready (omit empty apiKey) and fail closed with a fixed code.
    const args = buildUpdateGrokApiKeySlotArgs(input);
    return invokeSlotCommand(
      "update_grok_api_key_slot",
      args,
      parseRuntimeSlotView,
    ).catch((cause: unknown) => {
      const normalized = normalizeRuntimeSlotsError(cause);
      // Unknown command / IPC miss → treat as host gap, not a secret leak path.
      if (
        normalized.code === "runtime_slots_command_failed"
        || /not found|unknown command|COMMAND_NOT_FOUND/i.test(
          String((cause as { message?: string } | null)?.message ?? ""),
        )
      ) {
        throw new RuntimeSlotsGatewayError(
          "runtime_slot_update_unavailable",
          FIXED_UPDATE_UNAVAILABLE,
        );
      }
      throw normalized;
    });
  }

  createExternalCliSlot(
    input: CreateExternalCliSlotInput,
  ): Promise<RuntimeSlotView> {
    return invokeSlotCommand(
      "create_external_cli_slot",
      {
        projectId: input.projectId,
        displayName: input.displayName,
        executablePath: input.executablePath,
        launchArgs: input.launchArgs,
        statusArgs: input.statusArgs,
      },
      parseRuntimeSlotView,
    );
  }

  getExternalCliStatus(
    projectId: string,
    slotId: string,
  ): Promise<ExternalCliStatusView> {
    return invokeSlotCommand(
      "get_external_cli_status",
      { projectId, slotId },
      parseExternalCliStatusView,
    );
  }

  startRuntimeSlot(
    projectId: string,
    slotId: string,
  ): Promise<RuntimeSlotStartView> {
    return invokeSlotCommand(
      "start_runtime_slot",
      { projectId, slotId },
      parseRuntimeSlotStartView,
    );
  }

  getRuntimeSlotModelCatalog(
    projectId: string,
    slotId: string,
  ): Promise<RuntimeSlotModelCatalogView> {
    return invokeSlotCommand(
      "get_runtime_slot_model_catalog",
      { projectId, slotId },
      parseRuntimeSlotModelCatalogView,
    );
  }

  setRuntimeSlotModel(
    projectId: string,
    slotId: string,
    modelId: string,
  ): Promise<RuntimeSlotModelSwitchView> {
    return invokeSlotCommand(
      "set_runtime_slot_model",
      { projectId, slotId, modelId },
      parseRuntimeSlotModelSwitchView,
    );
  }

  detectLocalClis(): Promise<LocalCliDetectionView[]> {
    return invokeSlotCommand(
      "detect_local_clis",
      {},
      (payload) => {
        if (!Array.isArray(payload) || payload.length > 32) {
          throw invalidResponse();
        }
        return payload.map(parseLocalCliDetectionView);
      },
    );
  }

  probeModelCatalog(
    baseUrl: string,
    apiKey: string,
  ): Promise<ProbeModelCatalogView> {
    // apiKey is input-only; parseProbeModelCatalogView rejects secret keys.
    return invokeSlotCommand(
      "probe_model_catalog",
      { baseUrl, apiKey },
      parseProbeModelCatalogView,
    );
  }

  listProviderLoginCatalog(): Promise<ProviderLoginCatalogEntry[]> {
    return invokeSlotCommand(
      "list_provider_login_catalog",
      {},
      (payload) => {
        if (!Array.isArray(payload) || payload.length > 64) {
          throw invalidResponse();
        }
        return payload.map(parseProviderLoginCatalogEntry);
      },
    );
  }
}

/** In-memory mock for unit tests and non-desktop shells. */
export class MockRuntimeSlotsGateway implements RuntimeSlotsGateway {
  private slots: RuntimeSlotView[] = [];
  /** Last create/update payloads for secret-handling assertions (tests only). */
  readonly lastCreateApiKeyArgs: Array<ReturnType<
    typeof buildCreateGrokApiKeySlotArgs
  >> = [];
  readonly lastUpdateApiKeyArgs: Array<ReturnType<
    typeof buildUpdateGrokApiKeySlotArgs
  >> = [];
  /** Last unified create_slot inputs (tests: UI must call createSlot only). */
  readonly lastCreateSlotArgs: CreateSlotInput[] = [];
  /** Last unified update_slot inputs. */
  readonly lastUpdateSlotArgs: UpdateSlotInput[] = [];
  readonly lastSetModelArgs: Array<{
    projectId: string;
    slotId: string;
    modelId: string;
  }> = [];
  /** Opaque secrets kept only in mock memory — never exposed on views. */
  private readonly secrets = new Map<string, string>();
  private probeMode: "success" | "fail" = "success";
  private probeFailCode = "runtime_slot_input_invalid";
  private externalStatusMode: "success" | "fail" = "success";
  private externalStdout = "Logged in using ChatGPT";
  private catalogMode:
    | "available"
    | "unsupported_endpoint"
    | "unsupported_external"
    | "fail" = "available";
  private catalogModels: string[] = ["grok-test", "grok-beta", "custom-listed"];
  private catalogFailCode = "runtime_slot_model_catalog_unavailable";
  /** When true, setRuntimeSlotModel reports runtimeRestarted. */
  private modelSwitchRestarted = false;
  /** Draft probe modes (probeModelCatalog). */
  private draftProbeMode:
    | "available"
    | "unsupported_endpoint"
    | "fail" = "available";
  private draftProbeModels: string[] = ["draft-model-a", "draft-model-b"];
  private draftProbeFailCode = "runtime_slot_model_catalog_unavailable";
  /** detectLocalClis mock rows (empty until seedDetectResults). */
  private detectResults: LocalCliDetectionView[] = [];
  private detectFailCode: string | null = null;
  /** Last probeModelCatalog apiKey lengths only — never store the key text. */
  readonly lastProbeModelCatalogCalls: Array<{
    baseUrl: string;
    apiKeyLength: number;
  }> = [];
  /** Mock provider login catalog (includes api_key_only for filter tests). */
  private providerCatalog: ProviderLoginCatalogEntry[] =
    defaultMockProviderLoginCatalog();
  private providerCatalogFailCode: string | null = null;
  /**
   * OAuth create failure injection. When set, createSlot(oauth) throws and
   * never appends a slot (no optimistic success).
   */
  private oauthLoginFailCode: string | null = null;
  /** Non-secret account label returned on successful mock OAuth create. */
  private oauthSuccessAccountLabel: string | null = "user@example.test";
  private oauthSuccessExpiresAt: string =
    "2026-09-01T00:00:00.000Z";

  constructor(initial: RuntimeSlotView[] = []) {
    this.slots = initial.map((slot) => ({
      ...slot,
      accountLabel: slot.accountLabel ?? null,
      accessExpiresAt: slot.accessExpiresAt ?? null,
    }));
  }

  seedProviderLoginCatalog(rows: ProviderLoginCatalogEntry[]): void {
    this.providerCatalog = rows.map((row) => ({
      ...row,
      knownScopes: row.knownScopes ? [...row.knownScopes] : null,
    }));
    this.providerCatalogFailCode = null;
  }

  setProviderCatalogFail(code: string): void {
    this.providerCatalogFailCode = code;
  }

  /**
   * Inject OAuth login failure. Failure never creates a slot.
   * Codes: timeout / denied / state_mismatch families.
   */
  setOauthLoginFail(code: string | null): void {
    this.oauthLoginFailCode = code;
  }

  setOauthLoginSuccessMeta(options: {
    accountLabel?: string | null;
    accessExpiresAt?: string;
  }): void {
    if (options.accountLabel !== undefined) {
      this.oauthSuccessAccountLabel = options.accountLabel;
    }
    if (options.accessExpiresAt !== undefined) {
      this.oauthSuccessExpiresAt = options.accessExpiresAt;
    }
  }

  /** Test helper: place a secret under an opaque ref without exposing it on views. */
  seedSecret(secretRef: string, apiKey: string): void {
    this.secrets.set(secretRef, apiKey);
  }

  setProbeMode(mode: "success" | "fail", code = "runtime_slot_input_invalid"): void {
    this.probeMode = mode;
    this.probeFailCode = code;
  }

  setExternalStatusMode(
    mode: "success" | "fail",
    stdout = "Logged in using ChatGPT",
  ): void {
    this.externalStatusMode = mode;
    this.externalStdout = stdout;
  }

  setCatalogMode(
    mode:
      | "available"
      | "unsupported_endpoint"
      | "unsupported_external"
      | "fail",
    options?: { models?: string[]; failCode?: string },
  ): void {
    this.catalogMode = mode;
    if (options?.models) {
      this.catalogModels = [...options.models];
    }
    if (options?.failCode) {
      this.catalogFailCode = options.failCode;
    }
  }

  setModelSwitchRestarted(restarted: boolean): void {
    this.modelSwitchRestarted = restarted;
  }

  seedDetectResults(rows: LocalCliDetectionView[]): void {
    this.detectResults = rows.map((row) => ({ ...row }));
    this.detectFailCode = null;
  }

  setDetectFail(code: string): void {
    this.detectFailCode = code;
  }

  setDraftProbeMode(
    mode: "available" | "unsupported_endpoint" | "fail",
    options?: { models?: string[]; failCode?: string },
  ): void {
    this.draftProbeMode = mode;
    if (options?.models) {
      this.draftProbeModels = [...options.models];
    }
    if (options?.failCode) {
      this.draftProbeFailCode = options.failCode;
    }
  }

  getSlotsSnapshot(): RuntimeSlotView[] {
    return this.slots.map((slot) => ({ ...slot }));
  }

  async listRuntimeSlots(projectId: string): Promise<RuntimeSlotView[]> {
    return this.slots
      .filter((slot) => slot.projectId === projectId)
      .map((slot) => ({ ...slot }));
  }

  async createSlot(input: CreateSlotInput): Promise<RuntimeSlotView> {
    this.lastCreateSlotArgs.push({ ...input });
    const role = input.role ?? "custom";
    const roleCustomName =
      role === "custom"
        ? (input.roleCustomName?.trim() || DEFAULT_CUSTOM_ROLE_NAME)
        : null;
    if (role === "controller") {
      if (input.authKind === "local_cli") {
        throw new RuntimeSlotsGatewayError(
          "runtime_slot_controller_requires_governed_auth",
          mapRuntimeSlotErrorMessage(
            "runtime_slot_controller_requires_governed_auth",
          ),
        );
      }
      if (
        this.slots.some(
          (slot) =>
            slot.projectId === input.projectId && slot.role === "controller",
        )
      ) {
        throw new RuntimeSlotsGatewayError(
          "runtime_slot_controller_already_exists",
          mapRuntimeSlotErrorMessage(
            "runtime_slot_controller_already_exists",
          ),
        );
      }
    }
    const now = new Date().toISOString();
    if (input.authKind === "grok_login") {
      const slot: RuntimeSlotView = {
        slotId: cryptoRandomId(),
        projectId: input.projectId,
        displayName: input.displayName,
        kind: "governed_grok",
        governance: "full",
        authMode: "login",
        baseUrl: null,
        modelId: input.modelId ?? null,
        secretRef: null,
        secretConfigured: false,
        executablePath: null,
        launchArgs: null,
        statusArgs: null,
        role,
        roleCustomName,
        createdAt: now,
        updatedAt: now,
        accountLabel: null,
        accessExpiresAt: null,
      };
      this.slots.push(slot);
      return { ...slot };
    }
    if (input.authKind === "api_key") {
      const args = buildCreateGrokApiKeySlotArgs({
        projectId: input.projectId,
        displayName: input.displayName,
        baseUrl: input.baseUrl ?? "",
        modelId: input.modelId ?? "",
        apiKey: input.apiKey ?? "",
      });
      this.lastCreateApiKeyArgs.push({ ...args });
      if (!args.apiKey.trim()) {
        throw new RuntimeSlotsGatewayError(
          "runtime_slot_input_invalid",
          mapRuntimeSlotErrorMessage("runtime_slot_input_invalid"),
        );
      }
      const secretRef = cryptoRandomId();
      this.secrets.set(secretRef, args.apiKey);
      const slot: RuntimeSlotView = {
        slotId: cryptoRandomId(),
        projectId: args.projectId,
        displayName: args.displayName,
        kind: "governed_grok",
        governance: "full",
        authMode: "api_key",
        baseUrl: args.baseUrl,
        modelId: args.modelId,
        secretRef,
        secretConfigured: true,
        executablePath: null,
        launchArgs: null,
        statusArgs: null,
        role,
        roleCustomName,
        createdAt: now,
        updatedAt: now,
        accountLabel: null,
        accessExpiresAt: null,
      };
      this.slots.push(slot);
      return { ...slot };
    }
    if (input.authKind === "oauth") {
      // Fail closed first — never append a slot on injected failure.
      if (this.oauthLoginFailCode) {
        const code = this.oauthLoginFailCode;
        throw new RuntimeSlotsGatewayError(
          code,
          mapRuntimeSlotErrorMessage(code),
        );
      }
      if (!input.providerId?.trim()) {
        throw new RuntimeSlotsGatewayError(
          "runtime_slot_input_invalid",
          mapRuntimeSlotErrorMessage("runtime_slot_input_invalid"),
        );
      }
      if (!input.baseUrl?.trim() || !input.modelId?.trim()) {
        throw new RuntimeSlotsGatewayError(
          "runtime_slot_input_invalid",
          mapRuntimeSlotErrorMessage("runtime_slot_input_invalid"),
        );
      }
      // Tokens stay only in mock secret map under opaque refs — never on view.
      const accessRef = cryptoRandomId();
      const refreshRef = cryptoRandomId();
      this.secrets.set(accessRef, "mock-access-never-on-view");
      this.secrets.set(refreshRef, "mock-refresh-never-on-view");
      const accountFromUser = input.oauthSupplement?.accountLabel?.trim();
      const slot: RuntimeSlotView = {
        slotId: cryptoRandomId(),
        projectId: input.projectId,
        displayName: input.displayName,
        kind: "governed_grok",
        governance: "full",
        authMode: "oauth",
        baseUrl: input.baseUrl.trim(),
        modelId: input.modelId.trim(),
        // Host never surfaces OAuth DPAPI refs on the slot DTO.
        secretRef: null,
        secretConfigured: true,
        executablePath: null,
        launchArgs: null,
        statusArgs: null,
        role,
        roleCustomName,
        createdAt: now,
        updatedAt: now,
        accountLabel: accountFromUser || this.oauthSuccessAccountLabel,
        accessExpiresAt: this.oauthSuccessExpiresAt,
      };
      this.slots.push(slot);
      return { ...slot };
    }
    if (input.authKind !== "local_cli") {
      throw new RuntimeSlotsGatewayError(
        "runtime_slot_input_invalid",
        mapRuntimeSlotErrorMessage("runtime_slot_input_invalid"),
      );
    }
    const slot: RuntimeSlotView = {
      slotId: cryptoRandomId(),
      projectId: input.projectId,
      displayName: input.displayName,
      kind: "external_cli",
      governance: "none",
      authMode: null,
      baseUrl: null,
      modelId: null,
      secretRef: null,
      secretConfigured: false,
      executablePath: input.executablePath ?? null,
      launchArgs: input.launchArgs ? [...input.launchArgs] : null,
      statusArgs: input.statusArgs ? [...input.statusArgs] : null,
      role,
      roleCustomName,
      createdAt: now,
      updatedAt: now,
      accountLabel: null,
      accessExpiresAt: null,
    };
    this.slots.push(slot);
    return { ...slot };
  }

  async updateSlot(input: UpdateSlotInput): Promise<RuntimeSlotView> {
    this.lastUpdateSlotArgs.push({ ...input });
    const index = this.slots.findIndex(
      (slot) =>
        slot.slotId === input.slotId && slot.projectId === input.projectId,
    );
    if (index < 0) {
      throw new RuntimeSlotsGatewayError(
        "runtime_slot_not_found",
        mapRuntimeSlotErrorMessage("runtime_slot_not_found"),
      );
    }
    const current = this.slots[index]!;
    const nextRole = input.role ?? current.role;
    if (nextRole === "controller") {
      if (current.kind === "external_cli" || current.governance !== "full") {
        throw new RuntimeSlotsGatewayError(
          "runtime_slot_controller_requires_governed_auth",
          mapRuntimeSlotErrorMessage(
            "runtime_slot_controller_requires_governed_auth",
          ),
        );
      }
      if (
        this.slots.some(
          (slot) =>
            slot.projectId === input.projectId
            && slot.role === "controller"
            && slot.slotId !== input.slotId,
        )
      ) {
        throw new RuntimeSlotsGatewayError(
          "runtime_slot_controller_already_exists",
          mapRuntimeSlotErrorMessage(
            "runtime_slot_controller_already_exists",
          ),
        );
      }
    }
    const roleCustomName =
      nextRole === "custom"
        ? (input.roleCustomName?.trim()
          || current.roleCustomName
          || DEFAULT_CUSTOM_ROLE_NAME)
        : null;
    current.displayName = input.displayName.trim() || current.displayName;
    current.role = nextRole;
    current.roleCustomName = roleCustomName;
    if (!input.keepAuth) {
      if (input.baseUrl !== undefined && current.authMode === "api_key") {
        current.baseUrl = input.baseUrl;
      }
      if (input.modelId !== undefined && current.kind === "governed_grok") {
        current.modelId = input.modelId;
      }
      if (
        input.apiKey !== undefined
        && input.apiKey.trim().length > 0
        && current.authMode === "api_key"
      ) {
        const ref = current.secretRef ?? cryptoRandomId();
        this.secrets.set(ref, input.apiKey.trim());
        current.secretRef = ref;
        current.secretConfigured = true;
      }
      if (current.kind === "external_cli") {
        if (input.executablePath !== undefined) {
          current.executablePath = input.executablePath;
        }
        if (input.launchArgs !== undefined) {
          current.launchArgs = [...input.launchArgs];
        }
        if (input.statusArgs !== undefined) {
          current.statusArgs = [...input.statusArgs];
        }
      }
    }
    current.updatedAt = new Date().toISOString();
    this.slots[index] = current;
    return { ...current };
  }

  async createGrokLoginSlot(
    input: CreateGrokLoginSlotInput,
  ): Promise<RuntimeSlotView> {
    return this.createSlot({
      projectId: input.projectId,
      displayName: input.displayName,
      authKind: "grok_login",
    });
  }

  async createGrokApiKeySlot(
    input: CreateGrokApiKeySlotInput,
  ): Promise<RuntimeSlotView> {
    return this.createSlot({
      projectId: input.projectId,
      displayName: input.displayName,
      authKind: "api_key",
      baseUrl: input.baseUrl,
      modelId: input.modelId,
      apiKey: input.apiKey,
    });
  }

  async updateGrokApiKeySlot(
    input: UpdateGrokApiKeySlotInput,
  ): Promise<RuntimeSlotView> {
    const args = buildUpdateGrokApiKeySlotArgs(input);
    this.lastUpdateApiKeyArgs.push({ ...args });
    const index = this.slots.findIndex(
      (slot) =>
        slot.slotId === args.slotId && slot.projectId === args.projectId,
    );
    if (index < 0) {
      throw new RuntimeSlotsGatewayError(
        "runtime_slot_not_found",
        mapRuntimeSlotErrorMessage("runtime_slot_not_found"),
      );
    }
    const current = this.slots[index]!;
    if (current.kind !== "governed_grok" || current.authMode !== "api_key") {
      throw new RuntimeSlotsGatewayError(
        "runtime_slot_not_governed_grok",
        mapRuntimeSlotErrorMessage("runtime_slot_not_governed_grok"),
      );
    }
    const previousSecret = current.secretRef
      ? this.secrets.get(current.secretRef)
      : undefined;
    if (args.apiKey !== undefined) {
      const ref = current.secretRef ?? cryptoRandomId();
      this.secrets.set(ref, args.apiKey);
      current.secretRef = ref;
      current.secretConfigured = true;
    } else if (!previousSecret) {
      // Empty key omitted: keep prior secret; if none existed, still keep status.
      current.secretConfigured = Boolean(current.secretRef);
    }
    current.displayName = args.displayName;
    current.baseUrl = args.baseUrl;
    current.modelId = args.modelId;
    current.updatedAt = new Date().toISOString();
    this.slots[index] = current;
    return { ...current };
  }

  /** Test helper: read mock-held secret without putting it on the view. */
  getMockSecret(secretRef: string): string | undefined {
    return this.secrets.get(secretRef);
  }

  async createExternalCliSlot(
    input: CreateExternalCliSlotInput,
  ): Promise<RuntimeSlotView> {
    return this.createSlot({
      projectId: input.projectId,
      displayName: input.displayName,
      authKind: "local_cli",
      executablePath: input.executablePath,
      launchArgs: input.launchArgs,
      statusArgs: input.statusArgs,
    });
  }

  async getExternalCliStatus(
    projectId: string,
    slotId: string,
  ): Promise<ExternalCliStatusView> {
    const slot = this.slots.find(
      (row) => row.slotId === slotId && row.projectId === projectId,
    );
    if (!slot || slot.kind !== "external_cli") {
      throw new RuntimeSlotsGatewayError(
        "runtime_slot_not_found",
        mapRuntimeSlotErrorMessage("runtime_slot_not_found"),
      );
    }
    if (this.externalStatusMode === "fail") {
      return {
        slotId,
        projectId,
        governance: "none",
        exitCode: 1,
        stdout: "",
        stderr: "not logged in",
        timedOut: false,
        truncated: false,
      };
    }
    return {
      slotId,
      projectId,
      governance: "none",
      exitCode: 0,
      stdout: this.externalStdout,
      stderr: "",
      timedOut: false,
      truncated: false,
    };
  }

  async startRuntimeSlot(
    projectId: string,
    slotId: string,
  ): Promise<RuntimeSlotStartView> {
    const slot = this.slots.find(
      (row) => row.slotId === slotId && row.projectId === projectId,
    );
    if (!slot) {
      throw new RuntimeSlotsGatewayError(
        "runtime_slot_not_found",
        mapRuntimeSlotErrorMessage("runtime_slot_not_found"),
      );
    }
    if (slot.kind === "external_cli") {
      throw new RuntimeSlotsGatewayError(
        "external_cli_launch_unsupported",
        mapRuntimeSlotErrorMessage("external_cli_launch_unsupported"),
      );
    }
    if (this.probeMode === "fail") {
      throw new RuntimeSlotsGatewayError(
        this.probeFailCode,
        mapGrokProbeFailureMessage(this.probeFailCode),
      );
    }
    return {
      slotId,
      projectId,
      kind: "governed_grok",
      governance: "full",
      status: "active",
    };
  }

  async getRuntimeSlotModelCatalog(
    projectId: string,
    slotId: string,
  ): Promise<RuntimeSlotModelCatalogView> {
    const slot = this.slots.find(
      (row) => row.slotId === slotId && row.projectId === projectId,
    );
    if (!slot) {
      throw new RuntimeSlotsGatewayError(
        "runtime_slot_not_found",
        mapRuntimeSlotErrorMessage("runtime_slot_not_found"),
      );
    }
    if (slot.kind === "external_cli" || this.catalogMode === "unsupported_external") {
      return {
        slotId,
        projectId,
        status: "unsupported",
        source: "external_cli",
        models: [],
        reasonCode: "external_cli_not_supported",
      };
    }
    if (this.catalogMode === "fail") {
      throw new RuntimeSlotsGatewayError(
        this.catalogFailCode,
        mapRuntimeSlotErrorMessage(this.catalogFailCode),
      );
    }
    if (this.catalogMode === "unsupported_endpoint") {
      return {
        slotId,
        projectId,
        status: "unsupported",
        source: "openai_models",
        models: [],
        reasonCode: "endpoint_not_supported",
      };
    }
    return {
      slotId,
      projectId,
      status: "available",
      source: slot.authMode === "login" ? "grok_cli" : "openai_models",
      models: [...this.catalogModels],
      reasonCode: null,
    };
  }

  async setRuntimeSlotModel(
    projectId: string,
    slotId: string,
    modelId: string,
  ): Promise<RuntimeSlotModelSwitchView> {
    this.lastSetModelArgs.push({ projectId, slotId, modelId });
    const trimmed = modelId.trim();
    if (!trimmed) {
      throw new RuntimeSlotsGatewayError(
        "runtime_slot_input_invalid",
        mapRuntimeSlotErrorMessage("runtime_slot_input_invalid"),
      );
    }
    const index = this.slots.findIndex(
      (row) => row.slotId === slotId && row.projectId === projectId,
    );
    if (index < 0) {
      throw new RuntimeSlotsGatewayError(
        "runtime_slot_not_found",
        mapRuntimeSlotErrorMessage("runtime_slot_not_found"),
      );
    }
    const current = this.slots[index]!;
    if (current.kind !== "governed_grok") {
      throw new RuntimeSlotsGatewayError(
        "runtime_slot_not_governed_grok",
        mapRuntimeSlotErrorMessage("runtime_slot_not_governed_grok"),
      );
    }
    current.modelId = trimmed;
    current.updatedAt = new Date().toISOString();
    this.slots[index] = current;
    const restarted = this.modelSwitchRestarted;
    return {
      slotId,
      projectId,
      modelId: trimmed,
      runtimeRestarted: restarted,
      sessionRestore: restarted ? "unavailable" : "not_applicable",
    };
  }

  async detectLocalClis(): Promise<LocalCliDetectionView[]> {
    if (this.detectFailCode) {
      throw new RuntimeSlotsGatewayError(
        this.detectFailCode,
        mapRuntimeSlotErrorMessage(this.detectFailCode),
      );
    }
    return this.detectResults.map((row) => ({ ...row }));
  }

  async probeModelCatalog(
    baseUrl: string,
    apiKey: string,
  ): Promise<ProbeModelCatalogView> {
    // Record length only — never retain key material in mock state.
    this.lastProbeModelCatalogCalls.push({
      baseUrl,
      apiKeyLength: apiKey.length,
    });
    if (!baseUrl.trim() || !apiKey.trim()) {
      throw new RuntimeSlotsGatewayError(
        "runtime_slot_input_invalid",
        mapRuntimeSlotErrorMessage("runtime_slot_input_invalid"),
      );
    }
    if (this.draftProbeMode === "fail") {
      throw new RuntimeSlotsGatewayError(
        this.draftProbeFailCode,
        mapRuntimeSlotErrorMessage(this.draftProbeFailCode),
      );
    }
    if (this.draftProbeMode === "unsupported_endpoint") {
      return {
        status: "unsupported",
        source: "openai_models",
        models: [],
        reasonCode: "endpoint_not_supported",
      };
    }
    return {
      status: "available",
      source: "openai_models",
      models: [...this.draftProbeModels],
      reasonCode: null,
    };
  }

  async listProviderLoginCatalog(): Promise<ProviderLoginCatalogEntry[]> {
    if (this.providerCatalogFailCode) {
      throw new RuntimeSlotsGatewayError(
        this.providerCatalogFailCode,
        mapRuntimeSlotErrorMessage(this.providerCatalogFailCode),
      );
    }
    return this.providerCatalog.map((row) => ({
      ...row,
      knownScopes: row.knownScopes ? [...row.knownScopes] : null,
    }));
  }
}

function cryptoRandomId(): string {
  if (typeof crypto !== "undefined" && "randomUUID" in crypto) {
    return crypto.randomUUID();
  }
  return `slot-${Math.random().toString(16).slice(2)}-${Date.now()}`;
}
