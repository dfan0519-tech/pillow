/**
 * Renderer-side types for in-conversation agent activity disclosure (C2-C2).
 * Conversation-owned records only — no global background-task table, no raw session IDs in UI.
 */

export type AgentActivityStatus =
  | "pending"
  | "running"
  | "cancelling"
  | "completed"
  | "failed"
  | "cancelled"
  | "interrupted";

export type AgentActivityStepStatus =
  | "pending"
  | "active"
  | "completed"
  | "failed"
  | "cancelled";

export type AgentItemStatus =
  | "running"
  | "completed"
  | "failed"
  | "cancelled";

export type AgentActivityStepInput = {
  /** Internal key only; never rendered. */
  id?: string;
  title: string;
  status: AgentActivityStepStatus | string;
};

export type AgentToolActivityInput = {
  /** Internal key only; never rendered. */
  toolCallId?: string;
  name?: string;
  title: string;
  status: AgentItemStatus | string;
  output?: string;
  /** May contain host paths; projection must opaque them. */
  error?: string;
  /** When true, projection shows a human-readable truncation notice. */
  outputTruncated?: boolean;
  omittedOutputChars?: number;
};

export type AgentSubagentActivityInput = {
  /** Internal key only; never rendered. */
  subagentId?: string;
  role?: string;
  title: string;
  status: AgentItemStatus | string;
  progress?: string;
};

export type AgentArtifactActivityInput = {
  /** Internal key only; never rendered. */
  artifactId?: string;
  /** Absolute or relative; only portable relative paths are retained. */
  path: string;
  kind?: string;
  label?: string;
  /**
   * Non-DOM internal source token for exact open resolution.
   * Passed only through the in-memory click callback — never text/attrs.
   */
  sourceToken?: string;
};

export type AgentTestResultInput = {
  name: string;
  status: "passed" | "failed" | "skipped" | string;
  summary?: string;
};

/** Compact run-level truncation/omission notice (text only; ids never rendered). */
export type AgentActivityNoticeInput = {
  /** Internal key only; never rendered or placed in DOM attributes. */
  id?: string;
  text: string;
};

/**
 * Terminal write proposal outcome for the owning timeline.
 * proposalId is internal only; path is portable and used only for success Open file.
 */
export type AgentWriteOutcomeInput = {
  /** Internal key only; never rendered or placed in DOM attributes. */
  proposalId?: string;
  /** Portable project-relative path; open only when state is succeeded. */
  path: string;
  mode?: "create" | "modify" | string;
  state: string;
  errorCode?: string;
};

/**
 * One run/role activity record supplied by the owning conversation.
 * Diagnostic fields (sessionId, runId) must never be projected into UI text.
 * Process lifecycle is owned by processActivityModel command rows, not notices.
 */
export type AgentActivityRecord = {
  conversationId: string;
  role?: string;
  status: AgentActivityStatus | string;
  currentStep?: string;
  steps?: AgentActivityStepInput[];
  tools?: AgentToolActivityInput[];
  subagents?: AgentSubagentActivityInput[];
  artifacts?: AgentArtifactActivityInput[];
  testResults?: AgentTestResultInput[];
  /** Terminal write outcomes derived from durable runtime events. */
  writeOutcomes?: AgentWriteOutcomeInput[];
  /** Run-level truncation / omission summaries (human-readable). */
  notices?: AgentActivityNoticeInput[];
  /** May include machine paths; projection renders opaque copy only. */
  errorMessage?: string;
  /** Must never appear in rendered output. */
  sessionId?: string;
  /** Must never appear in rendered output. */
  runId?: string;
  /**
   * Internal, non-DOM record identity for React reconciliation.
   * Prefer this over array index when present. Never text/attrs/titles/test IDs.
   */
  recordIdentity?: string;
  /**
   * Internal-only list of forbidden display IDs (receipt, embedded resource,
   * etc.) that may not appear on the projected record fields. Projection merges
   * this into its sanitizer set and must never render or expose the carrier.
   */
  forbiddenDisplayIds?: readonly string[];
};

export type AgentActivityDisclosureProps = {
  conversationId: string;
  /**
   * Run / event / tool / sub-agent activity owned by a conversation.
   * Only records matching `conversationId` are rendered.
   */
  activity:
    | AgentActivityRecord
    | readonly AgentActivityRecord[]
    | null
    | undefined;
  /**
   * Portable project-relative path, plus optional non-DOM source token for
   * exact resource identity. One-argument consumers remain valid.
   */
  onOpenArtifact?: (relativePath: string, sourceToken?: string) => void;
};

export type AgentActivityStepView = {
  /** Stable internal key for React reconciliation only — never DOM text/attrs. */
  key: string;
  title: string;
  status: string;
  statusLabel: string;
};

export type AgentToolStepView = {
  /** Stable internal key for React reconciliation only — never DOM text/attrs. */
  key: string;
  title: string;
  status: string;
  statusLabel: string;
  output?: string;
  /** Human-readable per-tool output truncation notice. */
  truncationNotice?: string;
  hasNestedDetail: boolean;
};

export type AgentSubagentStepView = {
  /** Stable internal key for React reconciliation only — never DOM text/attrs. */
  key: string;
  title: string;
  status: string;
  statusLabel: string;
  progress?: string;
  hasNestedDetail: boolean;
};

export type AgentArtifactView = {
  /** Stable internal key for React reconciliation only — never DOM text/attrs. */
  key: string;
  label: string;
  relativePath: string;
  /**
   * Non-DOM source token for exact open resolution.
   * Never written into text, attributes, titles, or test IDs.
   */
  sourceToken?: string;
};

export type AgentTestResultView = {
  /** Stable internal key for React reconciliation only — never DOM text/attrs. */
  key: string;
  name: string;
  status: string;
  statusLabel: string;
  summary?: string;
};

export type AgentActivityNoticeView = {
  /** Stable internal key for React reconciliation only — never DOM text/attrs. */
  key: string;
  text: string;
};

export type AgentWriteOutcomeView = {
  /** Stable internal key for React reconciliation only — never DOM text/attrs. */
  key: string;
  /** Fixed path-free message (success copy is path-free; open is a separate CTA). */
  message: string;
  state: string;
  /** True only for succeeded outcomes with a portable relative path. */
  canOpenFile: boolean;
  /** Portable path for Open file; never a proposal/session/run id. */
  relativePath?: string;
};

/** Pure projection consumed by the disclosure renderer. */
export type AgentActivityProjection = {
  conversationId: string;
  /** Stable internal key for React reconciliation only — never DOM text/attrs. */
  recordKey: string;
  summaryLine: string;
  status: string;
  statusLabel: string;
  isTerminal: boolean;
  isFailureLike: boolean;
  roleLabel: string | null;
  currentStep: string | null;
  steps: AgentActivityStepView[];
  tools: AgentToolStepView[];
  subagents: AgentSubagentStepView[];
  artifacts: AgentArtifactView[];
  testResults: AgentTestResultView[];
  writeOutcomes: AgentWriteOutcomeView[];
  notices: AgentActivityNoticeView[];
  errorMessage: string | null;
  /** Successful completed history stays a single compact disclosure. */
  conciseSuccess: boolean;
};
