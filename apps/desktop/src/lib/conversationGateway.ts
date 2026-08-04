/**
 * Typed conversation gateway — runtime-neutral renderer boundary for Slice B1.
 * Payload shapes mirror Task 2 agent-protocol snake_case fixtures.
 */

export type RunStatus =
  | "pending"
  | "running"
  | "cancelling"
  | "completed"
  | "failed"
  | "cancelled"
  | "interrupted";

export type RuntimeEventKind =
  | "run_started"
  | "assistant_delta"
  | "reasoning_summary_delta"
  | "plan_updated"
  | "tool_call_started"
  | "tool_call_output"
  | "tool_call_completed"
  | "subagent_started"
  | "subagent_progress"
  | "subagent_completed"
  | "artifact_produced"
  | "write_proposal_requested"
  | "write_proposal_resolved"
  | "turn_completed"
  | "turn_failed"
  | "run_cancelled"
  | "heartbeat";

export type ConversationDto = {
  id: string;
  projectId: string;
  title: string;
  createdAt: string;
  updatedAt: string;
};

export type TimelineMessageDto = {
  id: string;
  role: "user" | "assistant" | "system";
  content: string;
  createdAt: string;
};

/** Authoritative run snapshot included with timeline loads. */
export type RunDto = {
  runId: string;
  conversationId: string;
  status: RunStatus;
};

export type PlanStepDto = {
  id: string;
  title: string;
  status: string;
};

export type RunStartedPayload = {
  prompt_preview?: string;
};

export type AssistantDeltaPayload = {
  text: string;
  /** Renderer projection metadata; transport events normally omit this. */
  omitted_chars?: number;
};

export type ReasoningSummaryDeltaPayload = {
  text: string;
  /** Renderer projection metadata; transport events normally omit this. */
  omitted_chars?: number;
};

export type PlanUpdatedPayload = {
  steps: PlanStepDto[];
  /** Renderer projection metadata; transport events normally omit this. */
  omitted_steps?: number;
};

export type ToolInputSummaryDto =
  | {
      kind: "fs_list";
      path: string;
      max_results: number;
    }
  | {
      kind: "fs_read";
      path: string;
      offset_bytes: number;
      max_bytes: number;
    }
  | {
      kind: "fs_search";
      path: string;
      query_preview: string;
      case_sensitive: boolean;
      max_results: number;
    };

export type ResourceReferenceDto = {
  resource_id: string;
  kind: "file" | "directory" | "search_results" | "artifact" | "receipt";
  label: string;
  path?: string;
  content_preview?: string;
  truncated?: boolean;
};

export type ToolReceiptReferenceDto = {
  receipt_id: string;
  label: string;
  resource?: ResourceReferenceDto;
};

export type ToolCallStartedPayload = {
  tool_call_id: string;
  name: string;
  summary?: string;
  input_summary?: ToolInputSummaryDto;
};

export type ToolCallOutputPayload = {
  tool_call_id: string;
  chunk: string;
  /** Renderer projection metadata; transport events normally omit this. */
  omitted_chars?: number;
};

export type ToolCallCompletedPayload = {
  tool_call_id: string;
  status: "completed" | "failed" | "cancelled";
  receipt?: ToolReceiptReferenceDto;
};

export type SubagentStartedPayload = {
  subagent_id: string;
  role: string;
};

export type SubagentProgressPayload = {
  subagent_id: string;
  summary: string;
};

export type SubagentCompletedPayload = {
  subagent_id: string;
  status: "completed" | "failed" | "cancelled";
};

export type ArtifactProducedPayload = {
  artifact_id: string;
  path: string;
  kind: string;
  resource?: ResourceReferenceDto;
};

export type TurnCompletedPayload = {
  summary?: string;
};

export type TurnFailedPayload = {
  code: string;
  message: string;
};

export type RunCancelledPayload = {
  reason?: string;
};

export type HeartbeatPayload = {
  tick: number;
};

/** Summary-only write proposal request — never content, grant, or native path. */
export type WriteProposalRequestedPayload = {
  proposal_id: string;
  path: string;
  mode: "create" | "modify";
  summary: string;
  expires_at: string;
};

/** Summary-only write proposal resolution — never content, grant, or native path. */
export type WriteProposalResolvedPayload = {
  proposal_id: string;
  path: string;
  mode: "create" | "modify";
  state:
    | "pending_approval"
    | "approved"
    | "applying"
    | "succeeded"
    | "rejected"
    | "expired"
    | "cancelled"
    | "interrupted"
    | "failed";
  error_code?: string;
};

type RuntimeEventBase = {
  eventId: string;
  runId: string;
  conversationId: string;
  sequence: number;
  createdAt: string;
};

/** Discriminated runtime event aligned to Task 2 snake_case payloads. */
export type RuntimeEventDto =
  | (RuntimeEventBase & { kind: "run_started"; payload: RunStartedPayload })
  | (RuntimeEventBase & { kind: "assistant_delta"; payload: AssistantDeltaPayload })
  | (RuntimeEventBase & {
      kind: "reasoning_summary_delta";
      payload: ReasoningSummaryDeltaPayload;
    })
  | (RuntimeEventBase & { kind: "plan_updated"; payload: PlanUpdatedPayload })
  | (RuntimeEventBase & { kind: "tool_call_started"; payload: ToolCallStartedPayload })
  | (RuntimeEventBase & { kind: "tool_call_output"; payload: ToolCallOutputPayload })
  | (RuntimeEventBase & {
      kind: "tool_call_completed";
      payload: ToolCallCompletedPayload;
    })
  | (RuntimeEventBase & { kind: "subagent_started"; payload: SubagentStartedPayload })
  | (RuntimeEventBase & { kind: "subagent_progress"; payload: SubagentProgressPayload })
  | (RuntimeEventBase & {
      kind: "subagent_completed";
      payload: SubagentCompletedPayload;
    })
  | (RuntimeEventBase & { kind: "artifact_produced"; payload: ArtifactProducedPayload })
  | (RuntimeEventBase & {
      kind: "write_proposal_requested";
      payload: WriteProposalRequestedPayload;
    })
  | (RuntimeEventBase & {
      kind: "write_proposal_resolved";
      payload: WriteProposalResolvedPayload;
    })
  | (RuntimeEventBase & { kind: "turn_completed"; payload: TurnCompletedPayload })
  | (RuntimeEventBase & { kind: "turn_failed"; payload: TurnFailedPayload })
  | (RuntimeEventBase & { kind: "run_cancelled"; payload: RunCancelledPayload })
  | (RuntimeEventBase & { kind: "heartbeat"; payload: HeartbeatPayload });

export type TimelineDto = {
  conversationId: string;
  messages: TimelineMessageDto[];
  events: RuntimeEventDto[];
  /** Authoritative run statuses (includes pending / cancelling / interrupted). */
  runs: RunDto[];
};

export interface ConversationGateway {
  listConversations(projectId: string): Promise<ConversationDto[]>;
  createConversation(projectId: string, title?: string): Promise<ConversationDto>;
  loadTimeline(conversationId: string): Promise<TimelineDto>;
  sendTurn(conversationId: string, content: string): Promise<{ runId: string }>;
  cancelRun(runId: string): Promise<void>;
  subscribe(listener: (event: RuntimeEventDto) => void): () => void;
}

export function eventKey(runId: string, sequence: number): string {
  return `${runId}:${sequence}`;
}

export function isTerminalKind(kind: RuntimeEventKind): boolean {
  return kind === "turn_completed" || kind === "turn_failed" || kind === "run_cancelled";
}
