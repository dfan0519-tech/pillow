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
export type GrokSlotAuthMode = "login" | "api_key" | "oauth";

/** Slot role within a project. Controller is at most one and must be governed. */
export type RuntimeSlotRole = "controller" | "librarian" | "custom";

/**
 * Closed renderer view of one runtime slot.
 * `secretRef` is an opaque host reference (not key material); UI must not
 * present it as a credential. Prefer `secretConfigured` for status copy.
 * There is never an `apiKey` field on this type.
 * OAuth tokens never appear here — only non-sensitive accountLabel + expiry.
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
  /**
   * Non-sensitive OAuth account label (email / display name). Null for
   * non-OAuth slots or when the provider does not yield a label.
   * Never token material.
   */
  accountLabel: string | null;
  /**
   * Access-token expiry (RFC3339) for OAuth slots. Null otherwise.
   * Access/refresh token strings are never on this DTO.
   */
  accessExpiresAt: string | null;
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
export type SlotAuthKind = "api_key" | "grok_login" | "local_cli" | "oauth";

/**
 * Data-driven auth method catalog for the create form.
 * OAuth is one more row here — not a new UI branch tree.
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
    id: "oauth",
    label: "OAuth 登录",
    description:
      "通过浏览器完成 OIDC/OAuth 授权；受 Pillow 完整治理，可担任总控。",
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

/**
 * Public OAuth metadata the user may supply to complete an incomplete
 * provider profile. Never carries tokens, codes, or client secrets.
 */
export type OauthLoginSupplementInput = {
  clientId?: string;
  issuer?: string;
  authorizeEndpoint?: string;
  deviceAuthorizationEndpoint?: string;
  tokenEndpoint?: string;
  scopes?: string[];
  /** When the catalog marks account id as user-supplied. */
  accountLabel?: string;
};

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
  /** OAuth: catalog provider id (e.g. `openai`, `xai`, `custom-oidc`). */
  providerId?: string;
  /** OAuth: user-supplied public fields for Unknown / UserSupplied catalog rows. */
  oauthSupplement?: OauthLoginSupplementInput;
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
  | "runtime_slot_controller_requires_governed_auth"
  | "runtime_slot_oauth_login_timeout"
  | "runtime_slot_oauth_login_denied"
  | "runtime_slot_oauth_login_state_mismatch"
  | "runtime_slot_oauth_loopback_timeout"
  | "runtime_slot_oauth_loopback_denied"
  | "runtime_slot_oauth_loopback_state_mismatch"
  | "runtime_slot_oauth_device_expired"
  | "runtime_slot_oauth_device_denied"
  | "runtime_slot_oauth_http_timeout"
  | "runtime_slot_provider_login_catalog_unavailable";

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
  /**
   * Public provider login catalog (no tokens, no client secrets).
   * Host command: `list_provider_login_catalog`.
   */
  listProviderLoginCatalog(): Promise<ProviderLoginCatalogEntry[]>;
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
  "请先新建一个槽位。没有总控时无法在 Pillow 里对话；建议将第一个受治理槽位（API key、已登录的 Grok 或 OAuth）设为总控。也可稍后指定信息管理员（不强制）。";

/**
 * Guidance when slots exist but none is controller.
 * Next step: reassign an existing governed slot’s role, or create one as 总控.
 */
export const GUIDANCE_NO_CONTROLLER_HEADING = "尚未指定总控";

export const GUIDANCE_NO_CONTROLLER_BODY =
  "当前已有槽位，但没有总控。没有总控就无法在 Pillow 里对话。请将某个受治理槽位（API key、已登录的 Grok 或 OAuth）的角色改为总控，或新建一个总控槽位。建议再设一位信息管理员（不强制）。";

/** Soft suggestion when controller exists but no librarian. */
export const GUIDANCE_LIBRARIAN_SUGGESTION =
  "建议设置一位信息管理员，便于知识与资料侧协助（不强制）。";

/** Why controller is disabled when local CLI auth is selected. */
export const CONTROLLER_REQUIRES_GOVERNED_AUTH_HINT =
  "总控必须使用受治理认证（API key、已登录的 Grok 或 OAuth）。本机 CLI 不受治理，不能担任总控。";

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

// ---------------------------------------------------------------------------
// Provider login catalog (list_provider_login_catalog) + OAuth create UX
// ---------------------------------------------------------------------------

/** Host wire: login mechanism for one provider profile. */
export type ProviderLoginMethodWire = "pkce" | "device_code" | "api_key_only";

/**
 * Host wire: whether a catalog field is fixed, must be user-filled, unknown,
 * or unused for this login method.
 */
export type ProfileFieldWire =
  | "known"
  | "user_supplied"
  | "unknown"
  | "not_applicable";

/** How a successful login associates a non-secret account label. */
export type AccountIdentifierWire =
  | "token_claims"
  | "user_supplied"
  | "unknown"
  | "not_applicable";

/**
 * Closed host DTO for one provider login catalog row.
 * Public OIDC/OAuth metadata only — never tokens, codes, or client secrets.
 * `knownClientId` is only set when the host has a PublicDocumented id;
 * Unknown client ids must be null so the UI cannot auto-fill a guess.
 */
export type ProviderLoginCatalogEntry = {
  providerId: string;
  displayName: string;
  loginMethod: ProviderLoginMethodWire;
  issuer: ProfileFieldWire;
  authorizeEndpoint: ProfileFieldWire;
  deviceAuthorizationEndpoint: ProfileFieldWire;
  tokenEndpoint: ProfileFieldWire;
  scopes: ProfileFieldWire;
  clientId: ProfileFieldWire;
  knownIssuer: string | null;
  knownAuthorizeEndpoint: string | null;
  knownDeviceAuthorizationEndpoint: string | null;
  knownTokenEndpoint: string | null;
  knownScopes: string[] | null;
  knownClientId: string | null;
  accountIdentifier: AccountIdentifierWire;
};

/** User-fillable OAuth profile fields (public metadata only). */
export type OauthUserFieldId =
  | "clientId"
  | "issuer"
  | "authorizeEndpoint"
  | "deviceAuthorizationEndpoint"
  | "tokenEndpoint"
  | "scopes"
  | "accountLabel";

export type OauthUserFieldRequirement = {
  fieldId: OauthUserFieldId;
  /** Short field label for the form. */
  label: string;
  /**
   * Why the user must type this value (especially for Unknown — never guess).
   */
  reason: string;
};

/** In-progress copy while host opens the browser / waits for callback. */
export const OAUTH_LOGIN_IN_PROGRESS =
  "正在登录…请在浏览器中完成授权，完成后将自动返回。切勿关闭此窗口。";

/** Distinct failure copy — never imply success. */
export const OAUTH_LOGIN_TIMEOUT_MESSAGE =
  "登录超时：在限定时间内未完成浏览器授权。请重试。";

export const OAUTH_LOGIN_DENIED_MESSAGE =
  "登录被拒绝：您在授权页面取消了授权，或提供方拒绝了本次请求。";

export const OAUTH_LOGIN_STATE_MISMATCH_MESSAGE =
  "登录失败：授权回调校验未通过（state 不匹配），可能存在干扰。请重新登录。";

export const OAUTH_LOGIN_GENERIC_FAILURE =
  "OAuth 登录失败。请检查提供方配置后重试。";

export const OAUTH_CATALOG_LOAD_BUSY = "正在加载可登录的提供方…";

export const OAUTH_CATALOG_EMPTY =
  "当前没有可用的 OAuth 提供方（仅 API key 的提供方不会出现在此列表）。";

export const OAUTH_FIELD_CLIENT_ID_UNKNOWN_REASON =
  "该提供方的 client_id 未取得可复用的第三方出处证明，Pillow 不会自动填写或猜测。请填写您自己注册的应用 client_id。";

export const OAUTH_FIELD_CLIENT_ID_USER_SUPPLIED_REASON =
  "请填写您在该提供方注册的应用 client_id（Pillow 不会预填）。";

export const OAUTH_FIELD_ISSUER_REASON =
  "目录将该字段标为需用户提供或未知；请填写您的 OIDC issuer URL，勿使用猜测值。";

export const OAUTH_FIELD_AUTHORIZE_REASON =
  "目录将该字段标为需用户提供或未知；请填写授权端点 URL。";

export const OAUTH_FIELD_DEVICE_REASON =
  "目录将该字段标为需用户提供或未知；请填写设备授权端点 URL。";

export const OAUTH_FIELD_TOKEN_REASON =
  "目录将该字段标为需用户提供或未知；请填写 token 端点 URL。";

export const OAUTH_FIELD_SCOPES_REASON =
  "目录将该字段标为需用户提供或未知；请填写空格分隔的 scopes。";

export const OAUTH_FIELD_ACCOUNT_LABEL_REASON =
  "该提供方不会从 token 自动提取账号标识；请填写便于识别的非敏感账号标签（如邮箱）。";

/** List-row badge for OAuth auth mode. */
export const OAUTH_AUTH_BADGE = "OAuth";

/** Soft note under OAuth form. */
export const OAUTH_LOGIN_FORM_HINT =
  "登录由宿主打开浏览器并等待回调；界面不发起对外网络请求，也不接收 token 明文。";
