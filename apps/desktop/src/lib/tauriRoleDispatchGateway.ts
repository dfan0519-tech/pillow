/**
 * Tauri role-dispatch gateway — closed-key host DTO validation.
 * Nested process proposal reuses normalizeExecProposal (same approval chain).
 * Host message text is never accepted; fixed copy only.
 */

import { invoke } from "@tauri-apps/api/core";
import {
  normalizeExecProposal,
} from "./tauriExecGateway";
import {
  ROLE_DISPATCH_ERROR_MESSAGES,
  RoleDispatchGatewayError,
  isRoleDispatchPublicError,
  normalizeRoleDispatchError,
  type ProposeRoleDispatchInput,
  type RoleDispatchGateway,
  type RoleDispatchGovernance,
  type RoleDispatchProposalView,
  type RoleDispatchPublicError,
  type RoleDispatchRecordView,
} from "./roleDispatchGateway";

const MAX_ID = 256;
const MAX_TASK = 16_384;
const MAX_ARGV = 64;
const MAX_ARGV_ELEM = 4_096;
const MAX_PATH = 1_024;
const MAX_LIST = 256;
const MAX_PROGRAM = 1_024;

/** Exact closed allowlist for host `RoleDispatchProposalDto` (camelCase). */
export const ROLE_DISPATCH_PROPOSAL_KEYS = [
  "assignmentId",
  "initiatorSlotId",
  "targetSlotId",
  "targetRole",
  "targetRoleCustomName",
  "governance",
  "taskText",
  "program",
  "argv",
  "portableCwd",
  "callId",
  "runId",
  "proposal",
] as const;

/** Exact closed allowlist for host `RoleDispatchRecordDto` (camelCase). */
export const ROLE_DISPATCH_RECORD_KEYS = [
  "assignmentId",
  "initiatorSlotId",
  "targetSlotId",
  "governance",
  "proposalId",
  "callId",
  "claimedGovernedExecution",
  "exitCode",
  "durationMs",
] as const;

const GOVERNANCE: readonly RoleDispatchGovernance[] = ["full", "none"];

function invalid(): RoleDispatchGatewayError {
  return new RoleDispatchGatewayError(
    "role_dispatch_unavailable",
    ROLE_DISPATCH_ERROR_MESSAGES.role_dispatch_unavailable,
  );
}

function record(value: unknown): value is Record<string, unknown> {
  return typeof value === "object" && value !== null && !Array.isArray(value);
}

function only(
  value: Record<string, unknown>,
  keys: readonly string[],
): boolean {
  return Object.keys(value).every((key) => keys.includes(key));
}

function bounded(value: unknown, max: number): value is string {
  return typeof value === "string" && value.length > 0 && value.length <= max;
}

function integer(
  value: unknown,
  min = Number.MIN_SAFE_INTEGER,
  max = Number.MAX_SAFE_INTEGER,
): value is number {
  return (
    typeof value === "number"
    && Number.isSafeInteger(value)
    && value >= min
    && value <= max
  );
}

function isGovernance(value: unknown): value is RoleDispatchGovernance {
  return typeof value === "string" && (GOVERNANCE as string[]).includes(value);
}

function portableCwd(value: unknown): value is string {
  if (value === ".") return true;
  if (
    typeof value !== "string"
    || value.length === 0
    || value.length > MAX_PATH
    || value.includes("\\")
    || value.includes(":")
    || value.startsWith("/")
    || value.startsWith("\\")
  ) {
    return false;
  }
  return value.split("/").every(
    (part) =>
      part.length > 0
      && part !== "."
      && part !== ".."
      && part.length <= 255
      && !/[<>:"|?*\u0000-\u001f]/.test(part),
  );
}

/**
 * Accept the exact host propose DTO. Unknown keys fail closed.
 * Nested `proposal` must pass process allowlist (same CommandApprovalBar path).
 */
export function normalizeRoleDispatchProposal(
  value: unknown,
): RoleDispatchProposalView {
  if (
    !record(value)
    || !only(value, ROLE_DISPATCH_PROPOSAL_KEYS)
    || !bounded(value.assignmentId, MAX_ID)
    || !bounded(value.initiatorSlotId, MAX_ID)
    || !bounded(value.targetSlotId, MAX_ID)
    || !bounded(value.targetRole, 64)
    || !(
      value.targetRoleCustomName === null
      || value.targetRoleCustomName === undefined
      || (typeof value.targetRoleCustomName === "string"
        && value.targetRoleCustomName.length <= 256)
    )
    || !isGovernance(value.governance)
    || typeof value.taskText !== "string"
    || value.taskText.length === 0
    || value.taskText.length > MAX_TASK
    || !bounded(value.program, MAX_PROGRAM)
    || !Array.isArray(value.argv)
    || value.argv.length > MAX_ARGV
    || !value.argv.every(
      (arg) => typeof arg === "string" && arg.length <= MAX_ARGV_ELEM,
    )
    || !(
      value.portableCwd === null
      || value.portableCwd === undefined
      || portableCwd(value.portableCwd)
    )
    || !bounded(value.callId, MAX_ID)
    || !bounded(value.runId, MAX_ID)
  ) {
    throw invalid();
  }

  let proposal: RoleDispatchProposalView["proposal"];
  try {
    proposal = normalizeExecProposal(value.proposal);
  } catch {
    throw invalid();
  }

  // Propose must return a live pending proposal — never claim executed here.
  if (proposal.state !== "pending_approval") {
    throw invalid();
  }

  return {
    assignmentId: value.assignmentId,
    initiatorSlotId: value.initiatorSlotId,
    targetSlotId: value.targetSlotId,
    targetRole: value.targetRole,
    targetRoleCustomName:
      typeof value.targetRoleCustomName === "string"
        ? value.targetRoleCustomName
        : null,
    governance: value.governance,
    taskText: value.taskText,
    program: value.program,
    argv: [...value.argv],
    portableCwd:
      typeof value.portableCwd === "string" ? value.portableCwd : null,
    callId: value.callId,
    runId: value.runId,
    proposal,
  };
}

export function normalizeRoleDispatchRecord(
  value: unknown,
): RoleDispatchRecordView {
  if (
    !record(value)
    || !only(value, ROLE_DISPATCH_RECORD_KEYS)
    || !bounded(value.assignmentId, MAX_ID)
    || !bounded(value.initiatorSlotId, MAX_ID)
    || !bounded(value.targetSlotId, MAX_ID)
    || !isGovernance(value.governance)
    || !(
      value.proposalId === null
      || value.proposalId === undefined
      || bounded(value.proposalId, MAX_ID)
    )
    || !bounded(value.callId, MAX_ID)
    || !(
      value.claimedGovernedExecution === null
      || value.claimedGovernedExecution === undefined
      || typeof value.claimedGovernedExecution === "boolean"
    )
    || !(
      value.exitCode === null
      || value.exitCode === undefined
      || integer(value.exitCode, -2147483648, 2147483647)
    )
    || !(
      value.durationMs === null
      || value.durationMs === undefined
      || integer(value.durationMs, 0)
    )
  ) {
    throw invalid();
  }

  // Fail closed: ungoverned rows must never claim governed execution.
  if (
    value.governance === "none"
    && value.claimedGovernedExecution === true
  ) {
    throw invalid();
  }

  return {
    assignmentId: value.assignmentId,
    initiatorSlotId: value.initiatorSlotId,
    targetSlotId: value.targetSlotId,
    governance: value.governance,
    proposalId:
      typeof value.proposalId === "string" ? value.proposalId : null,
    callId: value.callId,
    claimedGovernedExecution:
      typeof value.claimedGovernedExecution === "boolean"
        ? value.claimedGovernedExecution
        : null,
    exitCode: typeof value.exitCode === "number" ? value.exitCode : null,
    durationMs: typeof value.durationMs === "number" ? value.durationMs : null,
  };
}

function mapInvokeError(error: unknown): RoleDispatchGatewayError {
  // Prefer host code when it is a known role_dispatch_* token.
  if (record(error) && typeof error.code === "string") {
    if (isRoleDispatchPublicError(error.code)) {
      return new RoleDispatchGatewayError(
        error.code as RoleDispatchPublicError,
        ROLE_DISPATCH_ERROR_MESSAGES[error.code as RoleDispatchPublicError],
      );
    }
  }
  return normalizeRoleDispatchError(error);
}

export class TauriRoleDispatchGateway implements RoleDispatchGateway {
  async proposeRoleDispatch(
    input: ProposeRoleDispatchInput,
  ): Promise<RoleDispatchProposalView> {
    try {
      const args: Record<string, unknown> = {
        conversationId: input.conversationId,
        initiatorSlotId: input.initiatorSlotId,
        taskText: input.taskText,
      };
      if (input.targetSlotId !== undefined) {
        args.targetSlotId = input.targetSlotId;
      }
      if (input.targetRole !== undefined) {
        args.targetRole = input.targetRole;
      }
      const payload = await invoke<unknown>("propose_role_dispatch", args);
      return normalizeRoleDispatchProposal(payload);
    } catch (error) {
      throw mapInvokeError(error);
    }
  }

  async listRoleDispatches(
    conversationId: string,
  ): Promise<RoleDispatchRecordView[]> {
    try {
      const payload = await invoke<unknown>("list_role_dispatches", {
        conversationId,
      });
      if (!Array.isArray(payload) || payload.length > MAX_LIST) {
        throw invalid();
      }
      return payload.map(normalizeRoleDispatchRecord);
    } catch (error) {
      throw mapInvokeError(error);
    }
  }
}
