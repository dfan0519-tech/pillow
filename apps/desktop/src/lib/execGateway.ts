/** C4 process DTOs. Renderer-facing values are bounded, portable, and authority-free. */

export type ExecOrigin = "agent" | "user";
export type ExecState =
  | "pending_approval"
  | "approved"
  | "running"
  | "succeeded"
  | "failed"
  | "rejected"
  | "expired"
  | "cancelled"
  | "interrupted";
export type ExecDecision = "approve_once" | "reject";
export type ExecRiskClass = "standard" | "high";
export type ExecOutputStream = "stdout" | "stderr";

/** One canonical opaque error vocabulary for the native and TS process boundary. */
export const EXEC_PUBLIC_ERRORS = [
  "process_ownership_mismatch",
  "process_invalid_decision",
  "process_not_live",
  "process_expired",
  "process_unavailable",
  "process_output_unavailable",
  "process_cleanup_unproven",
  "process_path_rejected",
  "process_conflict",
] as const;
export type ExecPublicError = (typeof EXEC_PUBLIC_ERRORS)[number];

export type ExecProposalView = {
  proposalId: string;
  conversationId: string;
  origin: ExecOrigin;
  state: ExecState;
  programLabel: string;
  argv: string[];
  portableCwd: string;
  riskClass: ExecRiskClass;
  summary: string;
  expiresAt: string;
  /** Native retains any grant; renderer gets only this non-authority bit. */
  decisionAvailable: boolean;
  errorCode?: ExecPublicError;
};

export type ExecOutputChunk = {
  stream: ExecOutputStream;
  sequence: number;
  text: string;
};

export type ExecOutputView = {
  proposalId: string;
  state: ExecState;
  chunks: ExecOutputChunk[];
  truncated: boolean;
  encodingLossy: boolean;
  capturedBytes: number;
  exitCode: number | null;
  durationMs: number | null;
};

export interface ExecGateway {
  listExecProposals(conversationId: string): Promise<ExecProposalView[]>;
  decideExecProposal(input: {
    conversationId: string;
    proposalId: string;
    decision: ExecDecision;
  }): Promise<ExecProposalView>;
  openExecOutput(input: {
    conversationId: string;
    proposalId: string;
  }): Promise<ExecOutputView>;
}

export class ExecGatewayError extends Error {
  constructor(readonly code: ExecPublicError, message: string) {
    super(message);
    this.name = "ExecGatewayError";
  }
}

const EXEC_ERROR_MESSAGES: Record<ExecPublicError, string> = {
  process_ownership_mismatch: "Process proposal ownership mismatch.",
  process_invalid_decision: "Process decision must be approve_once or reject.",
  process_not_live: "Process proposal is not live.",
  process_expired: "Process proposal expired.",
  process_unavailable: "Process proposal is unavailable.",
  process_output_unavailable: "Process output is unavailable.",
  process_cleanup_unproven: "Process cleanup could not be verified.",
  process_path_rejected: "Process path was rejected.",
  process_conflict: "Process proposal conflicted with current state.",
};

export function isExecPublicError(value: unknown): value is ExecPublicError {
  return typeof value === "string" && (EXEC_PUBLIC_ERRORS as readonly string[]).includes(value);
}

export function normalizeExecError(error: unknown): ExecGatewayError {
  const code =
    error instanceof ExecGatewayError
      ? error.code
      : isRecord(error) && isExecPublicError(error.code)
        ? error.code
        : "process_unavailable";
  return new ExecGatewayError(code, EXEC_ERROR_MESSAGES[code]);
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === "object" && value !== null && !Array.isArray(value);
}
