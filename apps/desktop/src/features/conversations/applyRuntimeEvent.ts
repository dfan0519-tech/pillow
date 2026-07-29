/**
 * Pure projection of typed RuntimeEventDto into conversation UI state.
 */

import {
  eventKey,
  isTerminalKind,
  type RuntimeEventDto,
  type RuntimeEventKind,
  type RunStatus,
  type TimelineDto,
  type TimelineMessageDto,
} from "../../lib/conversationGateway";
import type {
  ConversationTimelineView,
  ConversationUiState,
  PlanStepView,
  RunView,
  SubagentActivity,
  ToolActivity,
} from "./conversationTypes";

export function emptyTimeline(conversationId: string): ConversationTimelineView {
  return {
    conversationId,
    messages: [],
    events: [],
    seenEventKeys: new Set(),
    assistantText: "",
    reasoningText: "",
    plan: null,
    tools: [],
    subagents: [],
    activeRunId: null,
  };
}

export function applyRuntimeEvent(
  state: ConversationUiState,
  event: RuntimeEventDto,
): ConversationUiState {
  const timeline =
    state.timelines[event.conversationId]
    ?? emptyTimeline(event.conversationId);

  const key = eventKey(event.runId, event.sequence);
  if (timeline.seenEventKeys.has(key)) {
    return state;
  }

  const seenEventKeys = new Set(timeline.seenEventKeys);
  seenEventKeys.add(key);
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
    // New run: do not carry previous assistant stream or activity into this turn.
    assistantText = "";
    reasoningText = "";
    plan = null;
    tools = [];
    subagents = [];
  }

  if (event.kind === "assistant_delta") {
    assistantText += event.payload.text;
    messages = upsertAssistantMessage(
      messages,
      assistantText,
      event.createdAt,
      event.runId,
    );
  }

  if (event.kind === "reasoning_summary_delta") {
    reasoningText += event.payload.text;
  }

  if (event.kind === "plan_updated") {
    plan = {
      steps: event.payload.steps.map(
        (step): PlanStepView => ({
          id: step.id,
          title: step.title,
          status: step.status,
        }),
      ),
    };
  }

  if (event.kind === "tool_call_started") {
    tools = upsertTool(tools, {
      toolCallId: event.payload.tool_call_id,
      name: event.payload.name,
      title: event.payload.summary ?? event.payload.name,
      status: "running",
    });
  }

  if (event.kind === "tool_call_output") {
    const toolCallId = event.payload.tool_call_id;
    tools = tools.map((tool) =>
      tool.toolCallId === toolCallId
        ? {
            ...tool,
            output: `${tool.output ?? ""}${event.payload.chunk}`,
          }
        : tool,
    );
  }

  if (event.kind === "tool_call_completed") {
    const toolCallId = event.payload.tool_call_id;
    tools = tools.map((tool) =>
      tool.toolCallId === toolCallId
        ? {
            ...tool,
            status:
              event.payload.status === "failed"
                ? "failed"
                : event.payload.status === "cancelled"
                  ? "cancelled"
                  : "completed",
          }
        : tool,
    );
  }

  if (event.kind === "subagent_started") {
    subagents = upsertSubagent(subagents, {
      subagentId: event.payload.subagent_id,
      title: event.payload.role,
      status: "running",
    });
  }

  if (event.kind === "subagent_progress") {
    const subagentId = event.payload.subagent_id;
    subagents = subagents.map((item) =>
      item.subagentId === subagentId
        ? { ...item, progress: event.payload.summary }
        : item,
    );
  }

  if (event.kind === "subagent_completed") {
    const subagentId = event.payload.subagent_id;
    subagents = subagents.map((item) =>
      item.subagentId === subagentId
        ? {
            ...item,
            status:
              event.payload.status === "failed"
                ? "failed"
                : event.payload.status === "cancelled"
                  ? "cancelled"
                  : "completed",
          }
        : item,
    );
  }

  const previousRun = state.runs[event.runId];
  const runStatus = statusFromEvent(event.kind, previousRun?.status);
  const terminalKind = isTerminalKind(event.kind)
    ? event.kind
    : previousRun?.terminalKind ?? null;

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
        seenEventKeys,
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

/**
 * Rebuild timeline view + run map from an authoritative TimelineDto.
 * User messages are taken from the snapshot; assistant/activity rebuild from events.
 */
export function rebuildFromTimeline(timeline: TimelineDto): {
  view: ConversationTimelineView;
  runs: Record<string, RunView>;
} {
  const seedRuns: Record<string, RunView> = {};
  for (const run of timeline.runs) {
    seedRuns[run.runId] = {
      runId: run.runId,
      conversationId: run.conversationId,
      status: run.status,
      terminalKind: terminalKindFromStatus(run.status),
    };
  }

  let view: ConversationTimelineView = {
    ...emptyTimeline(timeline.conversationId),
    messages: timeline.messages
      .filter((message) => message.role === "user" || message.role === "system")
      .map((message) => ({ ...message })),
  };

  let runs = { ...seedRuns };
  let working: ConversationUiState = {
    conversationsByProject: {},
    selectedConversationId: timeline.conversationId,
    timelines: { [timeline.conversationId]: view },
    runs,
  };

  for (const event of timeline.events) {
    working = applyRuntimeEvent(working, event);
  }

  view = working.timelines[timeline.conversationId] ?? view;
  runs = { ...seedRuns, ...working.runs };

  // Authoritative statuses win for runs present in the snapshot
  // (pending / cancelling / interrupted that have no terminal event yet).
  for (const run of timeline.runs) {
    const projected = runs[run.runId];
    if (!projected) {
      runs[run.runId] = {
        runId: run.runId,
        conversationId: run.conversationId,
        status: run.status,
        terminalKind: terminalKindFromStatus(run.status),
      };
      continue;
    }
    if (
      run.status === "pending"
      || run.status === "cancelling"
      || run.status === "interrupted"
    ) {
      runs[run.runId] = {
        ...projected,
        status: run.status,
        terminalKind: terminalKindFromStatus(run.status),
      };
    }
  }

  // Interleave user snapshot messages with event-built assistant turns by time.
  view = {
    ...view,
    messages: [...view.messages].sort((a, b) => {
      const byTime = a.createdAt.localeCompare(b.createdAt);
      if (byTime !== 0) return byTime;
      if (a.role === b.role) return a.id.localeCompare(b.id);
      return a.role === "user" ? -1 : 1;
    }),
  };

  // Point activeRunId at the latest non-terminal run when present.
  const active =
    [...Object.values(runs)]
      .filter((run) => run.conversationId === timeline.conversationId)
      .reverse()
      .find((run) => isActiveStatus(run.status))
    ?? [...Object.values(runs)]
      .filter((run) => run.conversationId === timeline.conversationId)
      .at(-1);
  if (active) {
    view = { ...view, activeRunId: active.runId };
  }

  return { view, runs };
}

/**
 * Merge a loaded snapshot with events that arrived while the async load was in flight.
 * Snapshot is the base; newer subscribed deltas (by runId+sequence) are re-applied.
 */
export function mergeTimelineWithInFlight(
  state: ConversationUiState,
  timeline: TimelineDto,
): ConversationUiState {
  const existing = state.timelines[timeline.conversationId];
  const { view: loadedView, runs: loadedRuns } = rebuildFromTimeline(timeline);

  let next: ConversationUiState = {
    ...state,
    timelines: {
      ...state.timelines,
      [timeline.conversationId]: loadedView,
    },
    runs: {
      ...state.runs,
      ...loadedRuns,
    },
  };

  if (!existing) {
    return next;
  }

  const loadedKeys = loadedView.seenEventKeys;
  for (const event of existing.events) {
    const key = eventKey(event.runId, event.sequence);
    if (!loadedKeys.has(key)) {
      next = applyRuntimeEvent(next, event);
    }
  }

  return next;
}

function upsertAssistantMessage(
  messages: TimelineMessageDto[],
  content: string,
  createdAt: string,
  runId: string,
): TimelineMessageDto[] {
  const assistantId = `assistant-${runId}`;
  const index = messages.findIndex((message) => message.id === assistantId);
  if (index >= 0) {
    const copy = [...messages];
    copy[index] = { ...copy[index]!, content, createdAt };
    return copy;
  }
  return [
    ...messages,
    {
      id: assistantId,
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
  copy[index] = { ...copy[index]!, ...next };
  return copy;
}

function upsertSubagent(
  items: SubagentActivity[],
  next: SubagentActivity,
): SubagentActivity[] {
  const index = items.findIndex((item) => item.subagentId === next.subagentId);
  if (index < 0) return [...items, next];
  const copy = [...items];
  copy[index] = { ...copy[index]!, ...next };
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

function terminalKindFromStatus(status: RunStatus): RuntimeEventKind | null {
  switch (status) {
    case "completed":
      return "turn_completed";
    case "failed":
      return "turn_failed";
    case "cancelled":
      return "run_cancelled";
    default:
      return null;
  }
}

function isActiveStatus(status: RunStatus): boolean {
  return status === "pending" || status === "running" || status === "cancelling";
}

function updateConversationTimestamp(
  byProject: ConversationUiState["conversationsByProject"],
  conversationId: string,
  updatedAt: string,
): ConversationUiState["conversationsByProject"] {
  let changed = false;
  const next: ConversationUiState["conversationsByProject"] = {};
  for (const [projectId, list] of Object.entries(byProject)) {
    next[projectId] = list.map((item) => {
      if (item.id !== conversationId) return item;
      changed = true;
      return { ...item, updatedAt };
    });
  }
  return changed ? next : byProject;
}
