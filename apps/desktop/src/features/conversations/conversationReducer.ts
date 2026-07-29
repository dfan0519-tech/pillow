/**
 * Pure conversation UI reducer. Projects normalized runtime events into
 * per-conversation timelines without depending on provider payloads.
 */

import type {
  ConversationDto,
  RuntimeEventDto,
  RuntimeEventKind,
  RunStatus,
  TimelineDto,
  TimelineMessageDto,
} from "../../lib/conversationGateway";

export type PlanActivity = {
  title: string;
  steps: string[];
};

export type ToolActivity = {
  toolCallId: string;
  name: string;
  title: string;
  status: "running" | "completed" | "failed";
  output?: string;
};

export type SubagentActivity = {
  subagentId: string;
  title: string;
  status: "running" | "completed" | "failed";
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
  seenSequences: Set<number>;
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
  | { type: "conversation_created"; conversation: ConversationDto }
  | { type: "conversation_selected"; conversationId: string | null }
  | { type: "timeline_loaded"; timeline: TimelineDto }
  | { type: "user_message_appended"; conversationId: string; message: TimelineMessageDto }
  | { type: "runtime_event"; event: RuntimeEventDto }
  | { type: "project_cleared" };

export function createInitialConversationState(): ConversationUiState {
  return {
    conversationsByProject: {},
    selectedConversationId: null,
    timelines: {},
    runs: {},
  };
}

export function conversationReducer(
  state: ConversationUiState,
  action: ConversationAction,
): ConversationUiState {
  switch (action.type) {
    case "conversations_loaded":
      return {
        ...state,
        conversationsByProject: {
          ...state.conversationsByProject,
          [action.projectId]: action.conversations,
        },
      };
    case "conversation_created": {
      const { conversation } = action;
      const existing = state.conversationsByProject[conversation.projectId] ?? [];
      const without = existing.filter((item) => item.id !== conversation.id);
      return {
        ...state,
        conversationsByProject: {
          ...state.conversationsByProject,
          [conversation.projectId]: [...without, conversation],
        },
        selectedConversationId: conversation.id,
      };
    }
    case "conversation_selected":
      return {
        ...state,
        selectedConversationId: action.conversationId,
      };
    case "timeline_loaded":
      return {
        ...state,
        timelines: {
          ...state.timelines,
          [action.timeline.conversationId]: timelineFromDto(action.timeline),
        },
      };
    case "user_message_appended": {
      const timeline =
        state.timelines[action.conversationId]
        ?? emptyTimeline(action.conversationId);
      return {
        ...state,
        timelines: {
          ...state.timelines,
          [action.conversationId]: {
            ...timeline,
            messages: [...timeline.messages, action.message],
          },
        },
      };
    }
    case "runtime_event":
      return applyRuntimeEvent(state, action.event);
    case "project_cleared":
      return {
        ...state,
        selectedConversationId: null,
      };
    default:
      return state;
  }
}

export function selectConversationsForProject(
  state: ConversationUiState,
  projectId: string | null,
): ConversationDto[] {
  if (!projectId) return [];
  return state.conversationsByProject[projectId] ?? [];
}

export function selectActiveTimeline(
  state: ConversationUiState,
): ConversationTimelineView | null {
  if (!state.selectedConversationId) return null;
  return state.timelines[state.selectedConversationId] ?? null;
}

export function selectActiveRun(
  state: ConversationUiState,
): RunView | null {
  const timeline = selectActiveTimeline(state);
  if (!timeline?.activeRunId) return null;
  return state.runs[timeline.activeRunId] ?? null;
}

export function isRunActive(run: RunView | null | undefined): boolean {
  if (!run) return false;
  return run.status === "pending" || run.status === "running" || run.status === "cancelling";
}

function applyRuntimeEvent(
  state: ConversationUiState,
  event: RuntimeEventDto,
): ConversationUiState {
  const timeline =
    state.timelines[event.conversationId]
    ?? emptyTimeline(event.conversationId);

  if (timeline.seenSequences.has(event.sequence)) {
    return state;
  }

  const seenSequences = new Set(timeline.seenSequences);
  seenSequences.add(event.sequence);
  const events = [...timeline.events, event];

  let messages = timeline.messages;
  let assistantText = timeline.assistantText;
  let reasoningText = timeline.reasoningText;
  let plan = timeline.plan;
  let tools = timeline.tools;
  let subagents = timeline.subagents;
  let activeRunId = timeline.activeRunId ?? event.runId;

  if (event.kind === "run_started") {
    activeRunId = event.runId;
  }

  if (event.kind === "assistant_delta") {
    const text = String(event.payload.text ?? "");
    assistantText += text;
    messages = upsertAssistantMessage(messages, assistantText, event.createdAt);
  }

  if (event.kind === "reasoning_summary_delta") {
    reasoningText += String(event.payload.text ?? "");
  }

  if (event.kind === "plan_updated") {
    const steps = Array.isArray(event.payload.steps)
      ? event.payload.steps.map((step) => String(step))
      : plan?.steps ?? [];
    plan = {
      title: String(event.payload.title ?? plan?.title ?? "计划"),
      steps,
    };
  }

  if (event.kind === "tool_call_started") {
    const toolCallId = String(event.payload.toolCallId ?? event.eventId);
    tools = upsertTool(tools, {
      toolCallId,
      name: String(event.payload.name ?? "tool"),
      title: String(event.payload.title ?? event.payload.name ?? "工具"),
      status: "running",
    });
  }

  if (event.kind === "tool_call_output") {
    const toolCallId = String(event.payload.toolCallId ?? "");
    tools = tools.map((tool) =>
      tool.toolCallId === toolCallId
        ? { ...tool, output: String(event.payload.output ?? "") }
        : tool,
    );
  }

  if (event.kind === "tool_call_completed") {
    const toolCallId = String(event.payload.toolCallId ?? "");
    tools = tools.map((tool) =>
      tool.toolCallId === toolCallId
        ? {
            ...tool,
            status:
              event.payload.status === "failed" ? "failed" : "completed",
          }
        : tool,
    );
  }

  if (event.kind === "subagent_started") {
    const subagentId = String(event.payload.subagentId ?? event.eventId);
    subagents = upsertSubagent(subagents, {
      subagentId,
      title: String(event.payload.title ?? "子代理"),
      status: "running",
    });
  }

  if (event.kind === "subagent_progress") {
    const subagentId = String(event.payload.subagentId ?? "");
    subagents = subagents.map((item) =>
      item.subagentId === subagentId
        ? { ...item, progress: String(event.payload.message ?? "") }
        : item,
    );
  }

  if (event.kind === "subagent_completed") {
    const subagentId = String(event.payload.subagentId ?? "");
    subagents = subagents.map((item) =>
      item.subagentId === subagentId
        ? {
            ...item,
            status:
              event.payload.status === "failed" ? "failed" : "completed",
          }
        : item,
    );
  }

  const runStatus = statusFromEvent(event.kind, state.runs[event.runId]?.status);
  const terminalKind = isTerminal(event.kind) ? event.kind : state.runs[event.runId]?.terminalKind ?? null;

  // Conversation title refresh from list metadata when present.
  const conversationsByProject = updateConversationTimestamp(
    state.conversationsByProject,
    event.conversationId,
    event.createdAt,
  );

  return {
    ...state,
    conversationsByProject,
    timelines: {
      ...state.timelines,
      [event.conversationId]: {
        conversationId: event.conversationId,
        messages,
        events,
        seenSequences,
        assistantText,
        reasoningText,
        plan,
        tools,
        subagents,
        activeRunId,
      },
    },
    runs: {
      ...state.runs,
      [event.runId]: {
        runId: event.runId,
        conversationId: event.conversationId,
        status: runStatus,
        terminalKind: terminalKind ?? null,
      },
    },
  };
}

function timelineFromDto(timeline: TimelineDto): ConversationTimelineView {
  // Seed user messages only; assistant text and activity rebuild from events
  // so loadTimeline does not double-apply assistant deltas.
  let view: ConversationTimelineView = {
    ...emptyTimeline(timeline.conversationId),
    messages: timeline.messages
      .filter((message) => message.role === "user")
      .map((message) => ({ ...message })),
  };
  for (const event of timeline.events) {
    const next = applyRuntimeEvent(
      {
        conversationsByProject: {},
        selectedConversationId: timeline.conversationId,
        timelines: { [timeline.conversationId]: view },
        runs: {},
      },
      event,
    );
    view = next.timelines[timeline.conversationId] ?? view;
  }
  return view;
}

function emptyTimeline(conversationId: string): ConversationTimelineView {
  return {
    conversationId,
    messages: [],
    events: [],
    seenSequences: new Set(),
    assistantText: "",
    reasoningText: "",
    plan: null,
    tools: [],
    subagents: [],
    activeRunId: null,
  };
}

function upsertAssistantMessage(
  messages: TimelineMessageDto[],
  content: string,
  createdAt: string,
): TimelineMessageDto[] {
  const last = messages.at(-1);
  if (last?.role === "assistant") {
    return [...messages.slice(0, -1), { ...last, content }];
  }
  return [
    ...messages,
    {
      id: `assistant-${createdAt}`,
      role: "assistant",
      content,
      createdAt,
    },
  ];
}

function upsertTool(tools: ToolActivity[], next: ToolActivity): ToolActivity[] {
  const index = tools.findIndex((tool) => tool.toolCallId === next.toolCallId);
  if (index < 0) return [...tools, next];
  const copy = [...tools];
  copy[index] = { ...copy[index], ...next };
  return copy;
}

function upsertSubagent(
  items: SubagentActivity[],
  next: SubagentActivity,
): SubagentActivity[] {
  const index = items.findIndex((item) => item.subagentId === next.subagentId);
  if (index < 0) return [...items, next];
  const copy = [...items];
  copy[index] = { ...copy[index], ...next };
  return copy;
}

function statusFromEvent(kind: RuntimeEventKind, previous?: RunStatus): RunStatus {
  switch (kind) {
    case "run_started":
    case "assistant_delta":
    case "reasoning_summary_delta":
    case "plan_updated":
    case "tool_call_started":
    case "tool_call_output":
    case "tool_call_completed":
    case "subagent_started":
    case "subagent_progress":
    case "subagent_completed":
    case "artifact_produced":
    case "heartbeat":
      return previous === "cancelling" ? "cancelling" : "running";
    case "turn_completed":
      return "completed";
    case "turn_failed":
      return "failed";
    case "run_cancelled":
      return "cancelled";
    default:
      return previous ?? "pending";
  }
}

function isTerminal(kind: RuntimeEventKind): boolean {
  return kind === "turn_completed" || kind === "turn_failed" || kind === "run_cancelled";
}

function updateConversationTimestamp(
  byProject: Record<string, ConversationDto[]>,
  conversationId: string,
  updatedAt: string,
): Record<string, ConversationDto[]> {
  let changed = false;
  const next: Record<string, ConversationDto[]> = {};
  for (const [projectId, list] of Object.entries(byProject)) {
    next[projectId] = list.map((item) => {
      if (item.id !== conversationId) return item;
      changed = true;
      return { ...item, updatedAt };
    });
  }
  return changed ? next : byProject;
}
