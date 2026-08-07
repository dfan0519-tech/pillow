/**
 * Pure process activity model for conversation-owned command rows (C4 D3B2).
 * Groups durable RuntimeEventDto process lifecycle events into one ordered
 * model per proposal. No Tauri, fetch, persist, polling, or output cache.
 */

import type {
  ProcessProposalDecidedPayload,
  ProcessProposalRequestedPayload,
  ProcessStartedPayload,
  ProcessTerminalOutcomePayload,
  RuntimeEventDto,
} from "../../lib/conversationGateway";
import type { CommandDecision, CommandState } from "./executionTypes";
import { formatArgvDisplay } from "./executionTypes";

const PROCESS_KINDS = new Set<RuntimeEventDto["kind"]>([
  "process_proposal_requested",
  "process_proposal_decided",
  "process_started",
  "process_terminal_outcome",
]);

/** Redaction token for native absolute / UNC path-looking argv entries. */
const REDACTED_PATH = "[path]";

const STATUS_LABEL: Record<CommandState, string> = {
  pending_approval: "待批准",
  approved: "已批准",
  running: "运行中",
  succeeded: "已成功",
  failed: "已失败",
  rejected: "已拒绝",
  expired: "已过期",
  cancelled: "已取消",
  interrupted: "已中断",
};

const DECISION_LABEL: Record<CommandDecision, string> = {
  approve_once: "批准一次",
  reject: "拒绝",
};

/**
 * One conversation-owned command activity model.
 * `proposalId` is an internal grouping/callback key only — never paint it.
 */
export type ProcessActivityModel = {
  /** Internal only: never text, DOM attributes, titles, test IDs, logs, or errors. */
  proposalId: string;
  programLabel: string;
  /** Safe argv for display and Terminal open (native paths redacted). */
  argv: string[];
  state: CommandState;
  statusLabel: string;
  decision?: CommandDecision;
  decisionLabel?: string;
  exitCode?: number | null;
  durationMs?: number | null;
  capturedBytes?: number | null;
  truncated?: boolean;
  encodingLossy?: boolean;
  /** True when process has started or reached a terminal outcome. */
  canOpenOutput: boolean;
  /** Successful completed history stays collapsed/concise. */
  conciseSuccess: boolean;
  /** Durable first-sequence for stable ordering across proposals. */
  firstSequence: number;
};

type MutableGroup = {
  proposalId: string;
  firstSequence: number;
  programLabel: string;
  argv: string[];
  state: CommandState;
  decision?: CommandDecision;
  exitCode?: number | null;
  durationMs?: number | null;
  capturedBytes?: number | null;
  truncated?: boolean;
  encodingLossy?: boolean;
  started: boolean;
  terminal: boolean;
};

/**
 * Group durable process lifecycle events into one ordered model per proposal.
 * Filters by owning conversation and optional run. Sequence order is durable.
 */
export function buildProcessActivityModels(
  events: readonly RuntimeEventDto[],
  conversationId: string,
  runId?: string,
): ProcessActivityModel[] {
  const owned = [...events]
    .filter((event) => {
      if (event.conversationId !== conversationId) return false;
      if (runId !== undefined && event.runId !== runId) return false;
      return PROCESS_KINDS.has(event.kind);
    })
    .sort((left, right) => left.sequence - right.sequence);

  const groups = new Map<string, MutableGroup>();
  const order: string[] = [];

  for (const event of owned) {
    const proposalId = proposalIdOf(event);
    if (!proposalId) continue;

    let group = groups.get(proposalId);
    if (!group) {
      group = {
        proposalId,
        firstSequence: event.sequence,
        programLabel: "命令",
        argv: [],
        state: "pending_approval",
        started: false,
        terminal: false,
      };
      groups.set(proposalId, group);
      order.push(proposalId);
    }

    applyEvent(group, event);
  }

  return order.map((id) => finalize(groups.get(id)!));
}

/** Display-only argv string; never re-parse for execution. */
export function formatProcessArgvDisplay(argv: readonly string[]): string {
  return formatArgvDisplay(argv);
}

/**
 * Redact native absolute or UNC path-looking argv tokens.
 * Portable relative arguments and ordinary flags remain literal.
 */
export function redactArgvToken(token: string): string {
  if (looksLikeNativeOrAbsolutePath(token)) return REDACTED_PATH;
  return token;
}

export function redactArgv(argv: readonly string[]): string[] {
  return argv.map((token) => redactArgvToken(token));
}

function proposalIdOf(event: RuntimeEventDto): string | null {
  const payload = event.payload as { proposal_id?: unknown };
  if (typeof payload.proposal_id !== "string") return null;
  const trimmed = payload.proposal_id.trim();
  return trimmed.length > 0 ? trimmed : null;
}

function applyEvent(group: MutableGroup, event: RuntimeEventDto): void {
  switch (event.kind) {
    case "process_proposal_requested": {
      const payload = event.payload as ProcessProposalRequestedPayload;
      group.programLabel = safeProgramLabel(payload.program);
      group.argv = redactArgv(payload.argv ?? []);
      // Request without later decision/start stays pending.
      if (!group.started && !group.terminal && !group.decision) {
        group.state = "pending_approval";
      }
      break;
    }
    case "process_proposal_decided": {
      const payload = event.payload as ProcessProposalDecidedPayload;
      group.programLabel = safeProgramLabel(payload.program) || group.programLabel;
      group.decision = payload.decision;
      if (!group.started && !group.terminal) {
        group.state = mapDecisionState(payload);
      }
      break;
    }
    case "process_started": {
      const payload = event.payload as ProcessStartedPayload;
      group.programLabel = safeProgramLabel(payload.program) || group.programLabel;
      if (payload.argv && payload.argv.length > 0) {
        group.argv = redactArgv(payload.argv);
      }
      group.started = true;
      if (!group.terminal) {
        group.state = "running";
      }
      break;
    }
    case "process_terminal_outcome": {
      const payload = event.payload as ProcessTerminalOutcomePayload;
      group.programLabel = safeProgramLabel(payload.program) || group.programLabel;
      group.terminal = true;
      group.started = true;
      group.state = mapTerminalState(payload.state);
      if (payload.exit_code !== undefined) group.exitCode = payload.exit_code;
      if (payload.duration_ms !== undefined) group.durationMs = payload.duration_ms;
      if (payload.captured_bytes !== undefined) {
        group.capturedBytes = payload.captured_bytes;
      }
      if (payload.truncated) group.truncated = true;
      if (payload.encoding_lossy) group.encodingLossy = true;
      break;
    }
    default:
      break;
  }
}

function finalize(group: MutableGroup): ProcessActivityModel {
  const state = group.state;
  const model: ProcessActivityModel = {
    proposalId: group.proposalId,
    programLabel: group.programLabel,
    argv: group.argv,
    state,
    statusLabel: STATUS_LABEL[state],
    canOpenOutput: group.started || group.terminal,
    conciseSuccess: state === "succeeded",
    firstSequence: group.firstSequence,
  };
  if (group.decision) {
    model.decision = group.decision;
    model.decisionLabel = DECISION_LABEL[group.decision];
  }
  if (group.exitCode !== undefined) model.exitCode = group.exitCode;
  if (group.durationMs !== undefined) model.durationMs = group.durationMs;
  if (group.capturedBytes !== undefined) model.capturedBytes = group.capturedBytes;
  if (group.truncated) model.truncated = true;
  if (group.encodingLossy) model.encodingLossy = true;
  return model;
}

function mapDecisionState(
  payload: ProcessProposalDecidedPayload,
): CommandState {
  if (payload.decision === "reject" || payload.state === "rejected") {
    return "rejected";
  }
  if (payload.state === "expired") return "expired";
  if (payload.state === "cancelled") return "cancelled";
  if (payload.state === "interrupted") return "interrupted";
  if (payload.state === "failed") return "failed";
  if (payload.state === "succeeded") return "succeeded";
  if (payload.state === "running") return "running";
  if (payload.state === "approved" || payload.decision === "approve_once") {
    return "approved";
  }
  return "pending_approval";
}

function mapTerminalState(
  state: ProcessTerminalOutcomePayload["state"],
): CommandState {
  switch (state) {
    case "succeeded":
    case "failed":
    case "rejected":
    case "expired":
    case "cancelled":
    case "interrupted":
    case "running":
    case "approved":
    case "pending_approval":
      return state;
    default:
      return "failed";
  }
}

function safeProgramLabel(program: string | undefined): string {
  const trimmed = program?.trim() ?? "";
  if (!trimmed) return "命令";
  if (looksLikeNativeOrAbsolutePath(trimmed)) return "命令";
  // Never surface grant-like or ID-like program tokens.
  if (/^[A-Za-z0-9_-]{20,}$/.test(trimmed) && /[0-9]/.test(trimmed)) {
    return "命令";
  }
  return trimmed;
}

function looksLikeNativeOrAbsolutePath(token: string): boolean {
  const value = token.trim();
  if (!value) return false;
  // Windows drive-absolute.
  if (/^[A-Za-z]:[\\/]/.test(value)) return true;
  // UNC.
  if (value.startsWith("\\\\") || value.startsWith("//")) return true;
  // Any backslash path form.
  if (value.includes("\\")) return true;
  // Unix absolute (flags use "-", relative portable paths do not start with /).
  if (value.startsWith("/")) return true;
  if (value.includes("://")) return true;
  return false;
}
