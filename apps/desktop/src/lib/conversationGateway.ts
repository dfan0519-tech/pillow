/**
 * Typed conversation gateway — runtime-neutral renderer boundary for Slice B1.
 * Mirrors the fixed domain vocabulary in the B1 plan; no provider payloads.
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

export type TimelineDto = {
  conversationId: string;
  messages: TimelineMessageDto[];
  events: RuntimeEventDto[];
};

/** Normalized run event delivered to the renderer. */
export type RuntimeEventDto = {
  eventId: string;
  runId: string;
  conversationId: string;
  sequence: number;
  kind: RuntimeEventKind;
  /** JSON-serializable payload; never secrets or provider envelopes. */
  payload: Record<string, unknown>;
  createdAt: string;
};

export interface ConversationGateway {
  listConversations(projectId: string): Promise<ConversationDto[]>;
  createConversation(projectId: string, title?: string): Promise<ConversationDto>;
  loadTimeline(conversationId: string): Promise<TimelineDto>;
  sendTurn(conversationId: string, content: string): Promise<{ runId: string }>;
  cancelRun(runId: string): Promise<void>;
  subscribe(listener: (event: RuntimeEventDto) => void): () => void;
}
