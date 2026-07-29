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
};

export type ReasoningSummaryDeltaPayload = {
  text: string;
};

export type PlanUpdatedPayload = {
  steps: PlanStepDto[];
};

export type ToolCallStartedPayload = {
  tool_call_id: string;
  name: string;
  summary?: string;
};

export type ToolCallOutputPayload = {
  tool_call_id: string;
  chunk: string;
};

export type ToolCallCompletedPayload = {
  tool_call_id: string;
  status: "completed" | "failed" | "cancelled";
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
