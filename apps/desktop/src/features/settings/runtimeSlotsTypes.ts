/**
 * Project-scoped runtime slots (governed Grok + external CLI registration).
 * Field names match host camelCase DTOs from the slots foundation report.
 * Secret material never appears on response views — only configuration status.
 */

/** Host `kind` values. */
export type RuntimeSlotKind = "governed_grok" | "external_cli";

/**
 * Host `governance` is an explicit field — never infer from kind in product UI.
 * `full` = governed ACP path; `none` = registration/status only, no proposal gate.
 */
export type RuntimeSlotGovernance = "full" | "none";

/** Present only on `governed_grok` slots. */
export type GrokSlotAuthMode = "login" | "api_key";

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
  | "external_cli_status_unavailable"
  | "external_cli_launch_unsupported"
  | "runtime_slot_id_invalid"
  | "runtime_slot_project_mismatch"
  | "runtime_slot_update_unavailable";

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

/** Saved API-key slot shows status only — never ciphertext or plaintext. */
export const SECRET_CONFIGURED_LABEL = "已配置";

export const SECRET_NOT_CONFIGURED_LABEL = "未配置";
