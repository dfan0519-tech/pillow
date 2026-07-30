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
  resource: ResourceReferenceView;
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
};

export type RunView = {
  runId: string;
  conversationId: string;
  status: RunStatus;
  terminalKind: RuntimeEventKind | null;
};

export type ConversationTimelineView = {
  conversationId: string;
  messages: TimelineMessageDto[];
  events: RuntimeEventDto[];
  /** Idempotent keys: `${runId}:${sequence}`. */
  seenEventKeys: Set<string>;
  assistantText: string;
  reasoningText: string;
  plan: PlanActivity | null;
  tools: ToolActivity[];
  subagents: SubagentActivity[];
  activitiesByRunId: Record<string, RunActivityView>;
  activeRunId: string | null;
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
