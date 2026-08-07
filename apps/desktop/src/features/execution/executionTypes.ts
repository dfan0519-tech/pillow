/**
 * C4 isolated execution UI DTOs.
 * Host-sanitized views only — no Tauri, native roots, grants or authority.
 */

/** Explicit origin for conversation-owned command summaries. */
export type CommandOrigin = "agent" | "user";

/**
 * Exact command lifecycle union for C4.0 process proposals (Task 4).
 * Renderer projects these labels only; it never infers terminal truth.
 */
export type CommandState =
  | "pending_approval"
  | "approved"
  | "running"
  | "succeeded"
  | "failed"
  | "rejected"
  | "expired"
  | "cancelled"
  | "interrupted";

/** Decision tokens accepted by the approval bar callbacks. */
export type CommandDecision = "approve_once" | "reject";

/** Host-classified risk for display; high marks e.g. node -e / python -c. */
export type CommandRiskClass = "standard" | "high";

/**
 * Pending (or reviewed) process proposal for the single conversation bar.
 * Identifiers exist for parent keying/callbacks and must never be painted.
 */
export type CommandProposalView = {
  proposalId: string;
  conversationId: string;
  origin: CommandOrigin;
  state: CommandState;
  /** Host tool-registry label, never a filesystem path. */
  programLabel: string;
  /** Literal sealed arguments — never re-parsed for execution. */
  argv: string[];
  /** Project-relative portable cwd (`.` allowed). */
  portableCwd: string;
  riskClass: CommandRiskClass;
  summary: string;
  expiresAt: string;
};

/**
 * Compact timeline activity for one conversation-owned command.
 * Successful history collapses to one disclosure; no permanent card stack.
 */
export type CommandActivityView = {
  origin: CommandOrigin;
  state: CommandState;
  programLabel: string;
  argv: string[];
  portableCwd?: string;
  exitCode?: number | null;
  durationMs?: number | null;
  /** Bounded nested tail for progressive disclosure (L1). */
  outputTail?: string[];
  truncated?: boolean;
  encodingLossy?: boolean;
};

/** One host-captured stdout/stderr chunk (already sanitized). */
export type TerminalOutputChunk = {
  stream: "stdout" | "stderr";
  sequence: number;
  text: string;
};

/**
 * Contextual Terminal-output mode model (not an interactive shell).
 * Parent supplies capped chunks; this view never fetches or persists.
 * `liveRetained` is presentation-only host truth: when false, the view must
 * show an explicit durable-only notice and must not invent transcript text.
 */
export type TerminalOutputModel = {
  programLabel: string;
  argv: string[];
  state: CommandState;
  chunks: TerminalOutputChunk[];
  /** Host memory still holds live chunks; false after restart/eviction. */
  liveRetained: boolean;
  truncated: boolean;
  encodingLossy: boolean;
  capturedBytes: number;
  exitCode?: number | null;
  durationMs?: number | null;
};

/**
 * Format sealed argv as a display-only literal string.
 *
 * Encoding: each argument is `JSON.stringify`'d, then joined with a single
 * space. This preserves empty strings, quotes, backslashes, whitespace and
 * Unicode, and keeps sealed argument boundaries unambiguous for approval and
 * review. Display only — never parse, shell-join, or execute the result.
 */
export function formatArgvDisplay(argv: readonly string[]): string {
  return argv.map((arg) => JSON.stringify(arg)).join(" ");
}
