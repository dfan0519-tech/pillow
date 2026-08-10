/**
 * Typed write-approval gateway — renderer boundary for Wave C3-E.
 * CamelCase DTOs mirror accepted C3-D native serialization exactly.
 * Never transports roots, content, digests, fingerprints, or base snapshots.
 */

import type { AutoApprovedByRule } from "./permissionRulesGateway";

export type WriteDecision = "approve_once" | "reject";

export type WriteMode = "create" | "modify";

export type WriteProposalState =
  | "pending_approval"
  | "approved"
  | "applying"
  | "succeeded"
  | "rejected"
  | "expired"
  | "cancelled"
  | "interrupted"
  | "failed";

/** Closed C3 public error codes on proposal/event DTOs (snake_case wire). */
export type WritePublicErrorCode =
  | "approval_required"
  | "approval_rejected"
  | "approval_expired"
  | "base_changed"
  | "write_conflict"
  | "path_rejected"
  | "write_failed"
  | "cancelled"
  | "interrupted";

export type WriteProposalView = {
  proposalId: string;
  conversationId: string;
  path: string;
  mode: WriteMode;
  state: WriteProposalState;
  summary: string;
  expiresAt: string;
  /** Opaque one-shot grant — component memory only; never DOM/log/storage. */
  approvalGrant?: string;
  /** Host-normalized permission-rule match subject (portable path). */
  ruleSubject: string;
  /** Present only when a permission rule, not a human, decided this. */
  autoApprovedByRule?: AutoApprovedByRule;
  errorCode?: WritePublicErrorCode;
};

export type WriteDiffView = {
  proposalId: string;
  path: string;
  mode: WriteMode;
  unified: string;
  truncated: boolean;
  lineCount: number;
  byteCount: number;
};

export type DecideWriteProposalInput = {
  conversationId: string;
  proposalId: string;
  approvalGrant: string;
  decision: WriteDecision;
};

export type OpenWriteProposalDiffInput = {
  conversationId: string;
  proposalId: string;
};

/** Native command error codes returned by Tauri write commands. */
const HOST_ERROR_CODES = [
  "write_ownership_mismatch",
  "write_invalid_grant",
  "write_not_live",
  "write_decision_denied",
  "write_expired",
  "write_cancelled",
  "write_interrupted",
  "write_unavailable",
  "write_invalid_decision",
] as const;

export type HostWriteApprovalErrorCode = (typeof HOST_ERROR_CODES)[number];

export type WriteApprovalGatewayErrorCode =
  | HostWriteApprovalErrorCode
  | "invalid_write_response";

/** Fixed renderer copy — never surface host message text (may carry secrets). */
const HOST_ERROR_MESSAGES: Record<HostWriteApprovalErrorCode, string> = {
  write_ownership_mismatch: "Write proposal ownership mismatch.",
  write_invalid_grant: "Approval grant was rejected.",
  write_not_live: "Write proposal is not live.",
  write_decision_denied: "Write decision was denied.",
  write_expired: "Write proposal expired.",
  write_cancelled: "Write proposal was cancelled.",
  write_interrupted: "Write proposal was interrupted.",
  write_unavailable: "Write proposal is unavailable.",
  write_invalid_decision: "Decision must be approve_once or reject.",
};

const FIXED_UNAVAILABLE_MESSAGE = HOST_ERROR_MESSAGES.write_unavailable;

/** Fixed renderer copy for parse/contract failures — never host message text. */
export const FIXED_INVALID_WRITE_RESPONSE_MESSAGE =
  "Pillow received an invalid write approval response.";

export const WRITE_PUBLIC_ERROR_CODES: readonly WritePublicErrorCode[] = [
  "approval_required",
  "approval_rejected",
  "approval_expired",
  "base_changed",
  "write_conflict",
  "path_rejected",
  "write_failed",
  "cancelled",
  "interrupted",
] as const;

/** Canonical raw grant: 32 OS-random bytes as base64url without padding. */
export const CANONICAL_APPROVAL_GRANT_LENGTH = 43;

const BASE64URL_GRANT_RE = /^[A-Za-z0-9_-]+$/;

export class WriteApprovalGatewayError extends Error {
  readonly code: WriteApprovalGatewayErrorCode;

  constructor(code: WriteApprovalGatewayErrorCode, message: string) {
    super(message);
    this.name = "WriteApprovalGatewayError";
    this.code = code;
  }
}

export interface WriteApprovalGateway {
  listWriteProposals(conversationId: string): Promise<WriteProposalView[]>;
  decideWriteProposal(input: DecideWriteProposalInput): Promise<WriteProposalView>;
  openWriteProposalDiff(input: OpenWriteProposalDiffInput): Promise<WriteDiffView>;
}

const PENDING_STATE: WriteProposalState = "pending_approval";

/** True when the proposal can still receive a one-shot human decision. */
export function isPendingWriteProposal(
  proposal: Pick<WriteProposalView, "state">,
): boolean {
  return proposal.state === PENDING_STATE;
}

export function isTerminalWriteState(state: WriteProposalState): boolean {
  return (
    state === "succeeded"
    || state === "rejected"
    || state === "expired"
    || state === "cancelled"
    || state === "interrupted"
    || state === "failed"
  );
}

/**
 * Conversation-owned live bar selection: oldest pending by expiresAt,
 * then stable proposalId. Foreign conversation ids are discarded.
 */
export function selectOldestPendingProposal(
  proposals: readonly WriteProposalView[],
  conversationId: string,
): WriteProposalView | null {
  const owned = proposals.filter(
    (item) =>
      item.conversationId === conversationId
      && isPendingWriteProposal(item),
  );
  if (owned.length === 0) return null;
  const sorted = [...owned].sort((left, right) => {
    const byExpiry = left.expiresAt.localeCompare(right.expiresAt);
    if (byExpiry !== 0) return byExpiry;
    return left.proposalId.localeCompare(right.proposalId);
  });
  return sorted[0] ?? null;
}

/** Strip the opaque grant so view models never carry it into render props. */
export function stripApprovalGrant(
  proposal: WriteProposalView,
): WriteProposalView {
  if (proposal.approvalGrant === undefined) return proposal;
  const { approvalGrant: _grant, ...rest } = proposal;
  return rest;
}

/** Fixed path-free terminal copy. Success alone is handled by the Open file CTA. */
export function writeTerminalMessage(
  state: WriteProposalState,
): string {
  switch (state) {
    case "succeeded":
      return "文件写入已完成";
    case "rejected":
      return "写入请求已拒绝";
    case "expired":
      return "写入请求已过期";
    case "cancelled":
      return "写入请求已取消";
    case "interrupted":
      return "写入请求已中断";
    case "failed":
      return "写入未能完成";
    case "approved":
    case "applying":
      return "正在应用写入…";
    case "pending_approval":
      return "等待批准写入";
  }
}

export function isWritePublicErrorCode(
  value: unknown,
): value is WritePublicErrorCode {
  return typeof value === "string"
    && (WRITE_PUBLIC_ERROR_CODES as readonly string[]).includes(value);
}

export function isCanonicalApprovalGrant(value: unknown): value is string {
  return (
    typeof value === "string"
    && value.length === CANONICAL_APPROVAL_GRANT_LENGTH
    && BASE64URL_GRANT_RE.test(value)
  );
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === "object" && value !== null && !Array.isArray(value);
}

function isHostErrorCode(value: unknown): value is HostWriteApprovalErrorCode {
  return typeof value === "string"
    && (HOST_ERROR_CODES as readonly string[]).includes(value);
}

function fixedMessageForGatewayCode(
  code: WriteApprovalGatewayErrorCode,
): string {
  if (code === "invalid_write_response") {
    return FIXED_INVALID_WRITE_RESPONSE_MESSAGE;
  }
  return HOST_ERROR_MESSAGES[code];
}

/**
 * Opaque command errors only — map known codes to fixed renderer copy.
 * Existing WriteApprovalGatewayError instances are rewritten the same way as
 * object-shaped native errors: keep a known code, replace its message, never
 * echo the incoming text (may contain paths, grants, or tokens).
 */
export function normalizeWriteApprovalError(
  error: unknown,
): WriteApprovalGatewayError {
  if (error instanceof WriteApprovalGatewayError) {
    return new WriteApprovalGatewayError(
      error.code,
      fixedMessageForGatewayCode(error.code),
    );
  }
  if (isRecord(error) && isHostErrorCode(error.code)) {
    return new WriteApprovalGatewayError(
      error.code,
      HOST_ERROR_MESSAGES[error.code],
    );
  }
  return new WriteApprovalGatewayError(
    "write_unavailable",
    FIXED_UNAVAILABLE_MESSAGE,
  );
}
