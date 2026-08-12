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
 * Host label for governed shell proposals (`SHELL_PROGRAM_LABEL` on the host).
 * Shape: `programLabel === "shell"`, `argv === [fullCommandString]`.
 */
export const SHELL_PROGRAM_LABEL = "shell";

/** True when the proposal/activity is a governed shell command (not structured argv). */
export function isShellProgram(programLabel: string): boolean {
  return programLabel === SHELL_PROGRAM_LABEL;
}

/**
 * EA-06 audit-listed Unicode bidi *formatting* controls (category Format).
 * Not matched by typical control-character checks; they reorder visual layout.
 * RTL script letters (Arabic/Hebrew) are intentionally excluded.
 */
const BIDI_FORMATTING_RE = /[\u202A\u202B\u202D\u202E\u2066\u2067\u2068\u2069]/g;

/**
 * Human-audit escape for bidi formatting controls → visible `\uXXXX`.
 *
 * - Returns a **new** string; does not mutate the caller's sealed argv.
 * - Escaped segments are ASCII (`\` `u` hex) and cannot re-decode to Format
 *   characters when painted as DOM text content (not `innerHTML`).
 * - Machine-readable / receipt argv must stay on the original `string[]`.
 */
export function escapeBidiFormattingForDisplay(text: string): string {
  return text.replace(BIDI_FORMATTING_RE, (ch) => {
    const hex = ch.codePointAt(0)!.toString(16).toUpperCase().padStart(4, "0");
    return `\\u${hex}`;
  });
}

/**
 * Display encoding for one sealed argv token (JSON string + bidi escape).
 * Display only — never parse, shell-join, or execute the result.
 */
export function formatArgvTokenDisplay(arg: string): string {
  return escapeBidiFormattingForDisplay(JSON.stringify(arg));
}

/**
 * Format sealed argv as a display-only literal string.
 *
 * Encoding: each argument is `JSON.stringify`'d, then bidi formatting controls
 * are escaped to `\uXXXX`, then tokens are joined with a single space. This
 * preserves empty strings, quotes, backslashes, whitespace and safe Unicode,
 * and keeps sealed argument boundaries unambiguous for approval and review.
 * Display only — never parse, shell-join, or execute the result.
 *
 * For shell proposals (`programLabel === "shell"`), use
 * {@link formatCommandArgvDisplay} instead so the full command string is not
 * JSON-escaped (bidi formatting still escapes).
 */
export function formatArgvDisplay(argv: readonly string[]): string {
  return argv
    .map((arg) => escapeBidiFormattingForDisplay(JSON.stringify(arg)))
    .join(" ");
}

/**
 * Display text for the command body (argv area) on approval / activity surfaces.
 *
 * - **shell**: host seals the full command as a single argv element. Return that
 *   string character-identical except for EA-06 bidi formatting escapes — no
 *   JSON escape, no whitespace normalize, no rejoin.
 * - **structured**: existing {@link formatArgvDisplay} (JSON-stringify each arg
 *   + bidi escape).
 *
 * Display only — never parse, shell-join, or execute the result. Does not
 * mutate the caller's `argv` array (receipt / machine-readable dual-rep).
 */
export function formatCommandArgvDisplay(
  programLabel: string,
  argv: readonly string[],
): string {
  if (isShellProgram(programLabel)) {
    // Host contract: argv is exactly one element — the full command string.
    // Do not join, trim, or JSON-encode; empty argv yields empty display.
    // Bidi formatting controls still escape for human-audit safety.
    return argv.length === 0
      ? ""
      : escapeBidiFormattingForDisplay(argv[0]!);
  }
  return formatArgvDisplay(argv);
}

/** Visual length above which approval UIs offer expand (DOM still holds full text). */
export const COMMAND_DISPLAY_EXPAND_THRESHOLD = 96;
