/**
 * Typed write-approval gateway — renderer boundary for Wave C3-E.
 * CamelCase DTOs mirror accepted C3-D native serialization exactly.
 * Never transports roots, sealed content, fingerprints, or base snapshots.
 * EA-04 review-binding `contentDigest` is the hash of sealed proposed content.
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
  /**
   * Non-authority availability bit from the host (EA-03).
   * Raw approval grants stay host-private — never cross this boundary.
   */
  decisionAvailable: boolean;
  /** Host-normalized permission-rule match subject (portable path). */
  ruleSubject: string;
  /** Present only when a permission rule, not a human, decided this. */
  autoApprovedByRule?: AutoApprovedByRule;
  errorCode?: WritePublicErrorCode;
  /**
   * Host-built unified diff exceeded its display cap (EA-04).
   * Approve stays locked until full proposed content is reviewed.
   */
  diffTruncated?: boolean;
  /**
   * SHA-256 of sealed proposed content (`sha256:` + lowercase hex).
   * Binds the human decision to the object the executor will write.
   */
  contentDigest?: string;
  /** True once every proposed-content page has been retrieved on the host. */
  contentReviewComplete?: boolean;
};

export type WriteDiffView = {
  proposalId: string;
  path: string;
  mode: WriteMode;
  unified: string;
  truncated: boolean;
  lineCount: number;
  byteCount: number;
  /** SHA-256 of sealed proposed content; never the bounded-diff text. */
  contentDigest?: string;
};

export type WriteContentPageView = {
  proposalId: string;
  path: string;
  mode: WriteMode;
  contentDigest: string;
  pageIndex: number;
  pageCount: number;
  text: string;
  lineCount: number;
  byteCount: number;
  reviewComplete: boolean;
};

export type OpenWriteProposalContentPageInput = {
  conversationId: string;
  proposalId: string;
  pageIndex: number;
};

export type DecideWriteProposalInput = {
  conversationId: string;
  proposalId: string;
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
  "write_truncated_unreviewed",
  "write_invalid_content_page",
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
  write_truncated_unreviewed:
    "Truncated write content cannot be approved until the full proposed content is reviewed.",
  write_invalid_content_page: "Write content page is not available.",
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
  /**
   * Optional: page the sealed proposed content for informed approval (EA-04).
   * Host marks review complete only after every page is retrieved.
   */
  openWriteProposalContentPage?(
    input: OpenWriteProposalContentPageInput,
  ): Promise<WriteContentPageView>;
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
