/**
 * Project-scoped runtime slots — one record shape: auth + role.
 * Field names match host camelCase DTOs. `kind` / `governance` / `authMode`
 * are **derived** wire labels (not independent storage). Secret material never
 * appears on response views — only configuration status.
 */

/** Derived host `kind` values (from auth). */
export type RuntimeSlotKind = "governed_grok" | "external_cli";

/**
 * Derived governance from auth — host still emits the field so UI never infers.
 * `full` = ApiKey | GrokLogin; `none` = LocalCli.
 */
export type RuntimeSlotGovernance = "full" | "none";

/** Present only on governed auth (`governed_grok` wire kind). */
export type GrokSlotAuthMode = "login" | "api_key";

/** Slot role within a project. Controller is at most one and must be governed. */
export type RuntimeSlotRole = "controller" | "librarian" | "custom";

/**
 * Closed renderer view of one runtime slot.
 * `secretRef` is an opaque host reference (not key material); UI must not
 * present it as a credential. Prefer `secretConfigured` for status copy.
 * There is never an `apiKey` field on this type.
 */
export type RuntimeSlotView = {
  slotId: string;
  projectId: string;
  displayName: string;
  kind: RuntimeSlotKind;
  governance: RuntimeSlotGovernance;
  authMode: GrokSlotAuthMode | null;
  baseUrl: string | null;
  modelId: string | null;
  secretRef: string | null;
  secretConfigured: boolean;
  executablePath: string | null;
  launchArgs: string[] | null;
  statusArgs: string[] | null;
  role: RuntimeSlotRole;
  roleCustomName: string | null;
  createdAt: string;
  updatedAt: string;
};

/** External CLI account-status command result (host may return raw stdout). */
export type ExternalCliStatusView = {
  slotId: string;
  projectId: string;
  governance: RuntimeSlotGovernance;
  exitCode: number | null;
  stdout: string;
  stderr: string;
  timedOut: boolean;
  truncated: boolean;
};

/** Successful activation of a governed Grok slot. */
export type RuntimeSlotStartView = {
  slotId: string;
  projectId: string;
  kind: RuntimeSlotKind;
  governance: RuntimeSlotGovernance;
  status: string;
};

/**
 * Model catalog for one runtime slot (`get_runtime_slot_model_catalog`).
 * `status === "unsupported"` is a normal result (not a command error): the
 * endpoint lacks `/models`, or the slot kind cannot list models.
 */
export type RuntimeSlotModelCatalogStatus = "available" | "unsupported";

export type RuntimeSlotModelCatalogSource =
  | "openai_models"
  | "grok_cli"
  | "external_cli";

export type RuntimeSlotModelCatalogReasonCode =
  | "endpoint_not_supported"
  | "external_cli_not_supported";

export type RuntimeSlotModelCatalogView = {
  slotId: string;
  projectId: string;
  status: RuntimeSlotModelCatalogStatus;
  source: RuntimeSlotModelCatalogSource;
  models: string[];
  reasonCode: RuntimeSlotModelCatalogReasonCode | null;
};

/**
 * Result of switching a slot model (`set_runtime_slot_model`).
 * Model is a launch argument: when the slot was active, the host restarts the
 * runtime (`runtimeRestarted: true`). UI must not invent "already applied to
 * the current session" when restarted is false.
 */
export type RuntimeSlotModelSessionRestore =
  | "not_applicable"
  | "unavailable";

export type RuntimeSlotModelSwitchView = {
  slotId: string;
  projectId: string;
  modelId: string;
  runtimeRestarted: boolean;
  sessionRestore: RuntimeSlotModelSessionRestore;
};

/** Auth kind for unified create_slot (host `auth_kind`). */
export type SlotAuthKind = "api_key" | "grok_login" | "local_cli";

/** Unified create input — one command for all auth variants. */
export type CreateSlotInput = {
  projectId: string;
  displayName: string;
  authKind: SlotAuthKind;
  role?: RuntimeSlotRole;
  roleCustomName?: string;
  baseUrl?: string;
  modelId?: string;
  /** One-shot create input only — never returned, never persisted client-side. */
  apiKey?: string;
  executablePath?: string;
  launchArgs?: string[];
  statusArgs?: string[];
};

export type CreateGrokLoginSlotInput = {
  projectId: string;
  displayName: string;
};

export type CreateGrokApiKeySlotInput = {
  projectId: string;
  displayName: string;
  baseUrl: string;
  modelId: string;
  /** One-shot create input only — never returned, never persisted client-side. */
  apiKey: string;
};

/**
 * Update input for an existing API-key slot.
 * Empty/whitespace `apiKey` means "keep stored secret" — gateway omits the key
 * from the host invoke so the host cannot treat blank as clear.
 */
export type UpdateGrokApiKeySlotInput = {
  projectId: string;
  slotId: string;
  displayName: string;
  baseUrl: string;
  modelId: string;
  /** Leave empty to keep the existing secret. */
  apiKey: string;
};

export type CreateExternalCliSlotInput = {
  projectId: string;
  displayName: string;
  executablePath: string;
  launchArgs: string[];
  statusArgs: string[];
};

export type RuntimeSlotsGatewayErrorCode =
  | "invalid_runtime_slot_response"
  | "runtime_slots_command_failed"
  | "runtime_slot_input_invalid"
  | "runtime_slot_not_found"
  | "runtime_slot_not_governed_grok"
  | "runtime_slot_store_unavailable"
  | "runtime_slot_project_unavailable"
  | "runtime_slot_secret_conflict"
  | "runtime_slot_secret_missing"
  | "runtime_slot_secret_unavailable"
  | "runtime_slot_secret_unsupported"
  | "runtime_slot_private_home_contaminated"
  | "runtime_slot_config_unavailable"
  | "runtime_slot_model_catalog_unavailable"
  | "runtime_slot_model_catalog_invalid"
  | "external_cli_status_unavailable"
  | "external_cli_launch_unsupported"
  | "runtime_slot_id_invalid"
  | "runtime_slot_project_mismatch"
  | "runtime_slot_update_unavailable"
  | "runtime_slot_controller_already_exists"
  | "runtime_slot_controller_requires_governed_auth";

export class RuntimeSlotsGatewayError extends Error {
  readonly code: string;

  constructor(code: string, message: string) {
    super(message);
    this.name = "RuntimeSlotsGatewayError";
    this.code = code;
  }
}

export interface RuntimeSlotsGateway {
  listRuntimeSlots(projectId: string): Promise<RuntimeSlotView[]>;
  /** Unified create — preferred entry. */
  createSlot(input: CreateSlotInput): Promise<RuntimeSlotView>;
  createGrokLoginSlot(
    input: CreateGrokLoginSlotInput,
  ): Promise<RuntimeSlotView>;
  createGrokApiKeySlot(
    input: CreateGrokApiKeySlotInput,
  ): Promise<RuntimeSlotView>;
  /**
   * Update an existing API-key slot. Empty `apiKey` must not be sent to the
   * host (keep stored secret). Host command may still be pending — Tauri
   * gateway surfaces a fixed unavailable error in that case.
   */
  updateGrokApiKeySlot(
    input: UpdateGrokApiKeySlotInput,
  ): Promise<RuntimeSlotView>;
  createExternalCliSlot(
    input: CreateExternalCliSlotInput,
  ): Promise<RuntimeSlotView>;
  getExternalCliStatus(
    projectId: string,
    slotId: string,
  ): Promise<ExternalCliStatusView>;
  /** Activate a governed Grok slot (product "probe / make ready" path). */
  startRuntimeSlot(
    projectId: string,
    slotId: string,
  ): Promise<RuntimeSlotStartView>;
  /**
   * List models for a governed Grok slot. `unsupported` is a normal success
   * payload (endpoint or kind cannot list) — not a thrown error.
   */
  getRuntimeSlotModelCatalog(
    projectId: string,
    slotId: string,
  ): Promise<RuntimeSlotModelCatalogView>;
  /**
   * Persist model id and restart the active runtime when this slot is active.
   * Return value carries `runtimeRestarted` for honest UI copy.
   */
  setRuntimeSlotModel(
    projectId: string,
    slotId: string,
    modelId: string,
  ): Promise<RuntimeSlotModelSwitchView>;
}

/**
 * Exact governance warning for external CLI slots.
 * Must remain plain text (not icon-only) so users never confuse registration
 * with a governed agent path.
 */
export const EXTERNAL_CLI_UNGOVERNED_HEADING = "此槽位不受 Pillow 治理";

export const EXTERNAL_CLI_UNGOVERNED_BODY =
  "它的读写和命令不经过提案与审批。外部 CLI 仅供登记与账号状态查询，不能作为受治理代理启动。";

/** Full banner text (heading + body) used in UI and text assertions. */
export const EXTERNAL_CLI_UNGOVERNED_BANNER =
  `${EXTERNAL_CLI_UNGOVERNED_HEADING}。${EXTERNAL_CLI_UNGOVERNED_BODY}`;

export const GOVERNED_GROK_BADGE = "受 Pillow 完整治理";

/** Default custom role applied to migrated / unspecified slots. */
export const DEFAULT_CUSTOM_ROLE_NAME = "未分配";

/** Saved API-key slot shows status only — never ciphertext or plaintext. */
export const SECRET_CONFIGURED_LABEL = "已配置";

export const SECRET_NOT_CONFIGURED_LABEL = "未配置";

/** Honest copy when switch restarts an active runtime (session cannot restore). */
export const MODEL_SWITCH_RESTARTED_MESSAGE =
  "模型已更新，活跃 runtime 已重启。当前会话上下文无法恢复，下一轮将使用新模型。";

/** Honest copy when slot was inactive — do not imply current session switched. */
export const MODEL_SWITCH_SAVED_INACTIVE_MESSAGE =
  "模型已保存。该槽位当前未激活，下次启动时将使用新模型。";

/** Catalog unsupported: endpoint has no listable /models. */
export const MODEL_CATALOG_UNSUPPORTED_ENDPOINT =
  "该端点不提供模型目录（未实现 /models）。请手输模型 id。";

/** Catalog unsupported: external CLI is registration-only. */
export const MODEL_CATALOG_UNSUPPORTED_EXTERNAL_CLI =
  "外部 CLI 槽位不适用模型目录，仅可登记与查询账号状态。";
