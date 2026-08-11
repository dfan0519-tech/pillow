/**
 * Runtime-slots gateway — closed-key host DTO validation + secret-safe invoke.
 * apiKey is create/update input only; responses never carry key material.
 * Renderer never logs caught host values (may carry secrets).
 */

import { invoke } from "@tauri-apps/api/core";
import {
  type CreateExternalCliSlotInput,
  type CreateGrokApiKeySlotInput,
  type CreateGrokLoginSlotInput,
  type ExternalCliStatusView,
  type GrokSlotAuthMode,
  MODEL_CATALOG_UNSUPPORTED_ENDPOINT,
  MODEL_CATALOG_UNSUPPORTED_EXTERNAL_CLI,
  MODEL_SWITCH_RESTARTED_MESSAGE,
  MODEL_SWITCH_SAVED_INACTIVE_MESSAGE,
  type RuntimeSlotGovernance,
  type RuntimeSlotKind,
  type RuntimeSlotModelCatalogReasonCode,
  type RuntimeSlotModelCatalogSource,
  type RuntimeSlotModelCatalogStatus,
  type RuntimeSlotModelCatalogView,
  type RuntimeSlotModelSessionRestore,
  type RuntimeSlotModelSwitchView,
  type RuntimeSlotStartView,
  type RuntimeSlotView,
  type RuntimeSlotsGateway,
  RuntimeSlotsGatewayError,
  type UpdateGrokApiKeySlotInput,
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
  "createdAt",
  "updatedAt",
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

const KINDS: readonly RuntimeSlotKind[] = ["governed_grok", "external_cli"];
const GOVERNANCE: readonly RuntimeSlotGovernance[] = ["full", "none"];
const AUTH_MODES: readonly GrokSlotAuthMode[] = ["login", "api_key"];
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

function invalidResponse(): RuntimeSlotsGatewayError {
  return new RuntimeSlotsGatewayError(
    "invalid_runtime_slot_response",
    INVALID_RESPONSE,
  );
}

/**
 * Reject response payloads that smuggle secret-shaped keys beyond the
 * host-approved status fields (`secretRef`, `secretConfigured`).
 * `apiKey` / token / password etc. are never valid on slot DTOs.
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
    const lower = key.toLowerCase();
    if (
      lower === "apikey"
      || lower === "api_key"
      || lower === "password"
      || lower === "token"
      || lower === "bearer"
      || lower === "authorization"
      || lower === "credential"
      || lower === "privatekey"
      || lower === "private_key"
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
    || !isFiniteTimestamp(value.createdAt)
    || !isFiniteTimestamp(value.updatedAt)
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
    createdAt: value.createdAt,
    updatedAt: value.updatedAt,
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

  createGrokLoginSlot(
    input: CreateGrokLoginSlotInput,
  ): Promise<RuntimeSlotView> {
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
    // apiKey travels once in the invoke args; response is validated secret-free.
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

  constructor(initial: RuntimeSlotView[] = []) {
    this.slots = initial.map((slot) => ({ ...slot }));
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

  getSlotsSnapshot(): RuntimeSlotView[] {
    return this.slots.map((slot) => ({ ...slot }));
  }

  async listRuntimeSlots(projectId: string): Promise<RuntimeSlotView[]> {
    return this.slots
      .filter((slot) => slot.projectId === projectId)
      .map((slot) => ({ ...slot }));
  }

  async createGrokLoginSlot(
    input: CreateGrokLoginSlotInput,
  ): Promise<RuntimeSlotView> {
    const now = new Date().toISOString();
    const slot: RuntimeSlotView = {
      slotId: cryptoRandomId(),
      projectId: input.projectId,
      displayName: input.displayName,
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
      createdAt: now,
      updatedAt: now,
    };
    this.slots.push(slot);
    return { ...slot };
  }

  async createGrokApiKeySlot(
    input: CreateGrokApiKeySlotInput,
  ): Promise<RuntimeSlotView> {
    const args = buildCreateGrokApiKeySlotArgs(input);
    this.lastCreateApiKeyArgs.push({ ...args });
    if (!args.apiKey.trim()) {
      throw new RuntimeSlotsGatewayError(
        "runtime_slot_input_invalid",
        mapRuntimeSlotErrorMessage("runtime_slot_input_invalid"),
      );
    }
    const secretRef = cryptoRandomId();
    this.secrets.set(secretRef, args.apiKey);
    const now = new Date().toISOString();
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
      createdAt: now,
      updatedAt: now,
    };
    this.slots.push(slot);
    // Response view must not carry the key.
    return { ...slot };
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
    const now = new Date().toISOString();
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
      executablePath: input.executablePath,
      launchArgs: [...input.launchArgs],
      statusArgs: [...input.statusArgs],
      createdAt: now,
      updatedAt: now,
    };
    this.slots.push(slot);
    return { ...slot };
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
}

function cryptoRandomId(): string {
  if (typeof crypto !== "undefined" && "randomUUID" in crypto) {
    return crypto.randomUUID();
  }
  return `slot-${Math.random().toString(16).slice(2)}-${Date.now()}`;
}
