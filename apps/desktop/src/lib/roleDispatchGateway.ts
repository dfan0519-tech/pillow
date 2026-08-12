/**
 * Conversation role-dispatch — renderer-facing DTOs and error vocabulary.
 * Propose only reserves a process proposal; approval/execute stay on ExecGateway.
 * No network. Secret material never appears on these views.
 */

import type { ExecProposalView } from "./execGateway";

/** Host-frozen governance at propose time: full = governed, none = LocalCli. */
export type RoleDispatchGovernance = "full" | "none";

/**
 * Closed host DTO after `propose_role_dispatch`.
 * Nested `proposal` is the same shape as list/decide process proposals.
 */
export type RoleDispatchProposalView = {
  assignmentId: string;
  initiatorSlotId: string;
  targetSlotId: string;
  targetRole: string;
  targetRoleCustomName: string | null;
  governance: RoleDispatchGovernance;
  taskText: string;
  program: string;
  argv: string[];
  portableCwd: string | null;
  callId: string;
  runId: string;
  proposal: ExecProposalView;
};

/**
 * In-memory dispatch audit row from `list_role_dispatches`.
 * Identifiers are internal keys only — UI must not paint raw ids as success.
 */
export type RoleDispatchRecordView = {
  assignmentId: string;
  initiatorSlotId: string;
  targetSlotId: string;
  governance: RoleDispatchGovernance;
  proposalId: string | null;
  callId: string;
  claimedGovernedExecution: boolean | null;
  exitCode: number | null;
  durationMs: number | null;
};

export type ProposeRoleDispatchInput = {
  conversationId: string;
  initiatorSlotId: string;
  targetSlotId?: string;
  targetRole?: string;
  taskText: string;
};

/**
 * Host `role_dispatch_*` codes the renderer translates to distinct copy.
 * Do not collapse these into a single "派单失败" string.
 */
export const ROLE_DISPATCH_PUBLIC_ERRORS = [
  "role_dispatch_self_dispatch",
  "role_dispatch_target_missing",
  "role_dispatch_role_mismatch",
  "role_dispatch_shell_join_forbidden",
  "role_dispatch_task_empty",
  "role_dispatch_task_in_program_or_cwd",
  "role_dispatch_program_empty",
  "role_dispatch_local_cli_executable_empty",
  "role_dispatch_empty_argv_element",
  "role_dispatch_unsupported_auth",
  "role_dispatch_process_unavailable",
  "role_dispatch_ledger_unavailable",
  "role_dispatch_unavailable",
] as const;

export type RoleDispatchPublicError =
  (typeof ROLE_DISPATCH_PUBLIC_ERRORS)[number];

export class RoleDispatchGatewayError extends Error {
  constructor(
    readonly code: RoleDispatchPublicError | string,
    message: string,
  ) {
    super(message);
    this.name = "RoleDispatchGatewayError";
  }
}

/**
 * Distinct human copy per host code — next user action differs by code.
 * Never accept host `message` text (may carry paths/secrets).
 */
export const ROLE_DISPATCH_ERROR_MESSAGES: Record<
  RoleDispatchPublicError,
  string
> = {
  role_dispatch_self_dispatch:
    "不能把任务派给总控自己。请选择其他角色或槽位作为目标。",
  role_dispatch_target_missing:
    "找不到目标槽位。请在设置中确认目标角色已创建，或重新选择目标。",
  role_dispatch_role_mismatch:
    "所选槽位的角色与请求的角色不一致。请改选匹配的槽位，或只按角色派单。",
  role_dispatch_shell_join_forbidden:
    "派单不能使用拼接后的 shell 命令字符串。请通过结构化目标槽位派单。",
  role_dispatch_task_empty: "任务描述不能为空。请填写要派给目标角色的工作内容。",
  role_dispatch_task_in_program_or_cwd:
    "任务文本不能写入程序名或工作目录。请只在任务描述中填写内容。",
  role_dispatch_program_empty:
    "目标槽位没有可执行的程序。请在设置中补全目标认证与启动配置。",
  role_dispatch_local_cli_executable_empty:
    "本机 CLI 目标的可执行文件路径为空。请在设置中填写可执行文件路径。",
  role_dispatch_empty_argv_element:
    "目标启动参数含有空项。请在设置中修正该槽位的启动参数。",
  role_dispatch_unsupported_auth:
    "目标槽位的认证方式尚不支持派单（例如 OAuth）。请改用受支持的认证，或换一个目标。",
  role_dispatch_process_unavailable:
    "派单提案无法进入进程治理链（程序目录或路径暂不受支持）。请换用目录内可用的目标，或稍后重试。",
  role_dispatch_ledger_unavailable:
    "派单台账暂时不可用。请稍后重试；若持续失败请重新打开会话。",
  role_dispatch_unavailable: "派单服务暂时不可用。请稍后重试。",
};

export function isRoleDispatchPublicError(
  value: unknown,
): value is RoleDispatchPublicError {
  return (
    typeof value === "string"
    && (ROLE_DISPATCH_PUBLIC_ERRORS as readonly string[]).includes(value)
  );
}

/**
 * Map host / gateway errors to fixed renderer copy.
 * Unknown codes and non-objects collapse to `role_dispatch_unavailable`
 * without echoing host message text.
 */
export function normalizeRoleDispatchError(
  error: unknown,
): RoleDispatchGatewayError {
  if (error instanceof RoleDispatchGatewayError) {
    const code = isRoleDispatchPublicError(error.code)
      ? error.code
      : "role_dispatch_unavailable";
    return new RoleDispatchGatewayError(
      code,
      ROLE_DISPATCH_ERROR_MESSAGES[code],
    );
  }
  if (isRecord(error) && isRoleDispatchPublicError(error.code)) {
    return new RoleDispatchGatewayError(
      error.code,
      ROLE_DISPATCH_ERROR_MESSAGES[error.code],
    );
  }
  return new RoleDispatchGatewayError(
    "role_dispatch_unavailable",
    ROLE_DISPATCH_ERROR_MESSAGES.role_dispatch_unavailable,
  );
}

export function roleDispatchErrorMessage(code: string): string {
  if (isRoleDispatchPublicError(code)) {
    return ROLE_DISPATCH_ERROR_MESSAGES[code];
  }
  return ROLE_DISPATCH_ERROR_MESSAGES.role_dispatch_unavailable;
}

/** UI badge when target governance is full. */
export const ROLE_DISPATCH_GOVERNED_LABEL = "受 Pillow 治理";

/**
 * Exact ungoverned label — must stay plain text so LocalCli is never painted
 * as governed execution after a successful process terminal.
 */
export const ROLE_DISPATCH_UNGOVERNED_LABEL = "目标不受治理";

export const ROLE_DISPATCH_UNGOVERNED_HINT =
  "本机 CLI 目标仅登记执行结果，读写与命令不经过 Pillow 完整治理。";

/** Pending proposal after propose — never claim executed. */
export const ROLE_DISPATCH_PENDING_APPROVAL_NOTICE =
  "派单已提交为进程提案，等待命令批准。未批准前不会执行。";

export interface RoleDispatchGateway {
  proposeRoleDispatch(
    input: ProposeRoleDispatchInput,
  ): Promise<RoleDispatchProposalView>;
  listRoleDispatches(
    conversationId: string,
  ): Promise<RoleDispatchRecordView[]>;
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === "object" && value !== null && !Array.isArray(value);
}
