/**
 * Renderer-side conversation UI types (projection of gateway DTOs).
 */

import type {
  ConversationDto,
  RuntimeEventDto,
  RuntimeEventKind,
  RunStatus,
  TimelineMessageDto,
  ToolInputSummaryDto,
} from "../../lib/conversationGateway";

export type PlanStepView = {
  id: string;
  title: string;
  status: string;
};

export type PlanActivity = {
  steps: PlanStepView[];
};

export type ResourceReferenceView = {
  resourceId: string;
  kind: "file" | "directory" | "search_results" | "artifact" | "receipt";
  label: string;
  path?: string;
  contentPreview?: string;
  truncated: boolean;
};

export type ReceiptReferenceView = {
  receiptId: string;
  label: string;
  resource?: ResourceReferenceView;
};

export type ToolActivity = {
  toolCallId: string;
  name: string;
  title: string;
  status: "running" | "completed" | "failed" | "cancelled";
  startedAt: string;
  updatedAt: string;
  inputSummary?: ToolInputSummaryDto;
  output?: string;
  outputTruncated?: boolean;
  omittedOutputChars?: number;
  receipt?: ReceiptReferenceView;
  resources?: ResourceReferenceView[];
};

export type SubagentActivity = {
  subagentId: string;
  title: string;
  status: "running" | "completed" | "failed" | "cancelled";
  startedAt: string;
  updatedAt: string;
  progress?: string;
};

export type ArtifactActivity = {
  artifactId: string;
  path: string;
  kind: string;
  createdAt: string;
  resource?: ResourceReferenceView;
};

export type RunActivityView = {
  runId: string;
  status: RunStatus;
  startedAt: string | null;
  updatedAt: string | null;
  promptPreview?: string;
  assistantText: string;
  assistantTextTruncated?: boolean;
  omittedAssistantChars?: number;
  reasoningText: string;
  reasoningTextTruncated?: boolean;
  omittedReasoningChars?: number;
  plan: PlanActivity | null;
  tools: ToolActivity[];
  subagents: SubagentActivity[];
  artifacts: ArtifactActivity[];
  omittedPlanStepCount?: number;
  omittedToolCount?: number;
  omittedSubagentCount?: number;
  omittedArtifactCount?: number;
  omittedResourceCount?: number;
  omittedResourcePreviewChars?: number;
};

export type RunView = {
  runId: string;
  conversationId: string;
  status: RunStatus;
  terminalKind: RuntimeEventKind | null;
};

/**
 * Known dispatch lifecycle tokens the timeline may paint.
 * Only emitted when a real propose state, process event, or exit code exists.
 */
export type DispatchTimelineState =
  | "proposed"
  | "pending_approval"
  | "approved"
  | "running"
  | "completed"
  | "failed"
  | "rejected"
  | "expired"
  | "cancelled"
  | "interrupted";

/**
 * Renderer-side dispatch row. Not a host DTO.
 * List records are sparse: omit target/task/labels when the host did not
 * supply them. Identifiers are internal keys and must never be painted.
 */
export type ConversationDispatchView = {
  assignmentId: string;
  initiatorSlotId: string;
  targetSlotId: string;
  /** Present on propose; absent on `list_role_dispatches`. */
  targetRole?: string | null;
  targetRoleCustomName?: string | null;
  /** Present on propose; absent on `list_role_dispatches`. */
  taskText?: string | null;
  governance: "full" | "none";
  proposalId: string | null;
  callId: string;
  claimedGovernedExecution: boolean | null;
  exitCode: number | null;
  durationMs: number | null;
  /** Parent-resolved slot catalog label. Never invented here. */
  initiatorLabel?: string | null;
  targetLabel?: string | null;
  /** `proposal.state` from propose. Not inferred from list rows. */
  proposalState?: string | null;
};

export type ConversationTimelineView = {
  conversationId: string;
  messages: TimelineMessageDto[];
  events: RuntimeEventDto[];
  /** Idempotent keys: `${runId}:${sequence}`. */
  seenEventKeys: Set<string>;
  /** Monotonic per-run high-water marks preserve dedupe after event pruning. */
  lastSequenceByRunId?: Record<string, number>;
  /** Per-run floor covered by pruned retained events. */
  prunedSequenceByRunId?: Record<string, number>;
  omittedEventCount?: number;
  assistantText: string;
  reasoningText: string;
  plan: PlanActivity | null;
  tools: ToolActivity[];
  subagents: SubagentActivity[];
  activitiesByRunId: Record<string, RunActivityView>;
  activeRunId: string | null;
  /**
   * First-class dispatch rows. Optional so existing reducers stay valid.
   * Parent overlays propose fields + slot labels; this module does not fetch.
   */
  dispatches?: ConversationDispatchView[];
};

export type ConversationUiState = {
  conversationsByProject: Record<string, ConversationDto[]>;
  selectedConversationId: string | null;
  timelines: Record<string, ConversationTimelineView>;
  runs: Record<string, RunView>;
};

export type ConversationAction =
  | {
      type: "conversations_loaded";
      projectId: string;
      conversations: ConversationDto[];
    }
  | {
      type: "conversation_created";
      conversation: ConversationDto;
      /** When false, register under project but do not select (stale scope). */
      select?: boolean;
    }
  | { type: "conversation_selected"; conversationId: string | null }
  | {
      type: "timeline_loaded";
      timeline: import("../../lib/conversationGateway").TimelineDto;
    }
  | { type: "user_message_appended"; conversationId: string; message: TimelineMessageDto }
  | { type: "user_message_removed"; conversationId: string; messageId: string }
  | { type: "runtime_event"; event: RuntimeEventDto }
  | { type: "project_cleared" };
