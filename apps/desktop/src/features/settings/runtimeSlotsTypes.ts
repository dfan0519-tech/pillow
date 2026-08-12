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

/**
 * Data-driven auth method catalog for the create form.
 * Adding OAuth later is one more row here — not a new UI branch tree.
 */
export type SlotAuthMethodOption = {
  id: SlotAuthKind;
  label: string;
  description: string;
  /** Controller requires governed auth; false for local CLI. */
  allowsController: boolean;
};

/** Parallel, equal auth options — none is "primary". */
export const SLOT_AUTH_METHODS: readonly SlotAuthMethodOption[] = [
  {
    id: "api_key",
    label: "API key",
    description: "端点、模型与密钥；受 Pillow 完整治理。",
    allowsController: true,
  },
  {
    id: "grok_login",
    label: "已登录的 Grok",
    description: "使用本机默认 Grok 登录 profile；受 Pillow 完整治理。",
    allowsController: true,
  },
  {
    id: "local_cli",
    label: "本机 CLI",
    description: "可执行文件与参数；不受治理，仅供登记与账号状态查询。",
    allowsController: false,
  },
];

/** Role options for create / reassign (custom needs a free-text name). */
export type SlotRoleOption = {
  id: RuntimeSlotRole;
  label: string;
  description: string;
};

export const SLOT_ROLE_OPTIONS: readonly SlotRoleOption[] = [
  {
    id: "controller",
    label: "总控",
    description: "可在 Pillow 中对话的主控槽位；每个项目最多一个，且必须受治理认证。",
  },
  {
    id: "librarian",
    label: "信息管理员",
    description: "建议设置一位，负责知识与资料侧协助（不强制）。",
  },
  {
    id: "custom",
    label: "自定义",
    description: "自行命名角色，例如写码、审阅等。",
  },
];

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

/**
 * Unified update input (`update_slot`).
 * Role reassignment uses keepAuth + current displayName; empty apiKey omits rotate.
 */
export type UpdateSlotInput = {
  projectId: string;
  slotId: string;
  displayName: string;
  role?: RuntimeSlotRole;
  roleCustomName?: string;
  baseUrl?: string;
  modelId?: string;
  /** Omit or empty to keep stored secret when keepAuth / host omit rules apply. */
  apiKey?: string;
  executablePath?: string;
  launchArgs?: string[];
  statusArgs?: string[];
  keepAuth?: boolean;
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
  /** Unified update — role reassignment and auth-branch fields. */
  updateSlot(input: UpdateSlotInput): Promise<RuntimeSlotView>;
  /** @deprecated UI must use createSlot; kept for host legacy registration. */
  createGrokLoginSlot(
    input: CreateGrokLoginSlotInput,
  ): Promise<RuntimeSlotView>;
  /** @deprecated UI must use createSlot; kept for host legacy registration. */
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
  /** @deprecated UI must use createSlot; kept for host legacy registration. */
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
  /**
   * Scan local agent CLIs (install + auth three-state). Host-only; no secrets.
   */
  detectLocalClis(): Promise<LocalCliDetectionView[]>;
  /**
   * Draft-state model catalog via host (baseUrl + apiKey in, never out).
   * `unsupported` is a normal success payload — not a thrown error.
   * Failures throw `RuntimeSlotsGatewayError` with catalog error codes.
   */
  probeModelCatalog(
    baseUrl: string,
    apiKey: string,
  ): Promise<ProbeModelCatalogView>;
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

/**
 * Guidance when the project has zero slots.
 * Next step: create the first slot (preferably as 总控 with governed auth).
 */
export const GUIDANCE_NO_SLOTS_HEADING = "还没有槽位";

export const GUIDANCE_NO_SLOTS_BODY =
  "请先新建一个槽位。没有总控时无法在 Pillow 里对话；建议将第一个受治理槽位（API key 或已登录的 Grok）设为总控。也可稍后指定信息管理员（不强制）。";

/**
 * Guidance when slots exist but none is controller.
 * Next step: reassign an existing governed slot’s role, or create one as 总控.
 */
export const GUIDANCE_NO_CONTROLLER_HEADING = "尚未指定总控";

export const GUIDANCE_NO_CONTROLLER_BODY =
  "当前已有槽位，但没有总控。没有总控就无法在 Pillow 里对话。请将某个受治理槽位（API key 或已登录的 Grok）的角色改为总控，或新建一个总控槽位。建议再设一位信息管理员（不强制）。";

/** Soft suggestion when controller exists but no librarian. */
export const GUIDANCE_LIBRARIAN_SUGGESTION =
  "建议设置一位信息管理员，便于知识与资料侧协助（不强制）。";

/** Why controller is disabled when local CLI auth is selected. */
export const CONTROLLER_REQUIRES_GOVERNED_AUTH_HINT =
  "总控必须使用受治理认证（API key 或已登录的 Grok）。本机 CLI 不受治理，不能担任总控。";

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

// ---------------------------------------------------------------------------
// Local CLI detect (detect_local_clis) + draft model probe (probe_model_catalog)
// ---------------------------------------------------------------------------

/** Known agent CLI kinds returned by host detection. */
export type LocalCliKind =
  | "grok"
  | "claude"
  | "codex"
  | "opencode"
  | "gemini";

/**
 * Three-state auth probe. Missing evidence is `unknown`, never
 * `not_authenticated` (absence is not negative evidence).
 */
export type LocalCliAuthStatus =
  | "authenticated"
  | "not_authenticated"
  | "unknown";

/** Closed host DTO for one CLI detection row. */
export type LocalCliDetectionView = {
  kind: LocalCliKind;
  found: boolean;
  executablePath: string | null;
  version: string | null;
  authStatus: LocalCliAuthStatus;
  /** Non-sensitive label when authenticated; otherwise null. */
  accountLabel: string | null;
  /** Present for unknown (reason code from host); never secret. */
  authReason: string | null;
};

/**
 * Draft-state model catalog (`probe_model_catalog`) — no slot/project ids.
 * Same status vocabulary as slot catalog; key never appears on this view.
 */
export type ProbeModelCatalogView = {
  status: RuntimeSlotModelCatalogStatus;
  source: RuntimeSlotModelCatalogSource;
  models: string[];
  reasonCode: RuntimeSlotModelCatalogReasonCode | null;
};

/** UI labels for auth three-state (unknown must not read as 未登录). */
export const LOCAL_CLI_AUTH_AUTHENTICATED_LABEL = "已登录";
export const LOCAL_CLI_AUTH_NOT_AUTHENTICATED_LABEL = "未登录";
/** Missing evidence — never the same string as not_authenticated. */
export const LOCAL_CLI_AUTH_UNKNOWN_LABEL = "未确定";

export const LOCAL_CLI_DETECT_HINT =
  "扫描本机已安装的 agent CLI，并读取登录账号状态，便于确认或更换账号。仍可手填路径。";

export const LOCAL_CLI_DETECT_BUSY = "正在扫描本机 CLI…";
export const LOCAL_CLI_DETECT_NONE_FOUND = "未发现已安装的已知 CLI，请手填可执行文件路径。";

/** Suggested status argv when selecting a detected CLI (space-separated). */
export const LOCAL_CLI_DEFAULT_STATUS_ARGS: Readonly<
  Record<LocalCliKind, string>
> = {
  grok: "models",
  claude: "auth status --text",
  codex: "login status",
  opencode: "auth list",
  gemini: "",
};
