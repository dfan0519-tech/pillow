/**
 * Renderer-side conversation UI types (projection of gateway DTOs).
 */

import type {
  ConversationDto,
  RuntimeEventDto,
  RuntimeEventKind,
  RunStatus,
  TimelineMessageDto,
} from "../../lib/conversationGateway";

export type PlanStepView = {
  id: string;
  title: string;
  status: string;
};

export type PlanActivity = {
  steps: PlanStepView[];
};

export type ToolActivity = {
  toolCallId: string;
  name: string;
  title: string;
  status: "running" | "completed" | "failed" | "cancelled";
  output?: string;
};

export type SubagentActivity = {
  subagentId: string;
  title: string;
  status: "running" | "completed" | "failed" | "cancelled";
  progress?: string;
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
