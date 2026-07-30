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
  RunView,
} from "./conversationTypes";
import { normalizeTimelineActivitiesForRuns } from "./normalizeTerminalActivity";
import {
  boundEventForRetention,
  emptyRunActivity,
  projectRunActivity,
} from "./projectRunActivity";

export { closeOpenActivities } from "./normalizeTerminalActivity";
export const MAX_RETAINED_EVENTS = 512;

export function emptyTimeline(conversationId: string): ConversationTimelineView {
  return {
    conversationId,
    messages: [],
    events: [],
    seenEventKeys: new Set(),
    lastSequenceByRunId: {},
    prunedSequenceByRunId: {},
    omittedEventCount: 0,
    assistantText: "",
    reasoningText: "",
    plan: null,
    tools: [],
    subagents: [],
    activitiesByRunId: {},
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
  const prunedSequence = timeline.prunedSequenceByRunId?.[event.runId];
  if (
    timeline.seenEventKeys.has(key)
    || (prunedSequence !== undefined && event.sequence <= prunedSequence)
  ) {
    return state;
  }

  let messages = timeline.messages;
  let activeRunId = timeline.activeRunId ?? event.runId;
  const currentActivity =
    timeline.activitiesByRunId[event.runId]
    ?? emptyRunActivity(event.runId, event.createdAt);
  const retainedEvent = boundEventForRetention(currentActivity, event);
  const appendedEvents = [...timeline.events, retainedEvent];
  const prunedEventCount = Math.max(
    0,
    appendedEvents.length - MAX_RETAINED_EVENTS,
  );
  const events = prunedEventCount > 0
    ? appendedEvents.slice(prunedEventCount)
    : appendedEvents;
  const prunedSequenceByRunId = {
    ...(timeline.prunedSequenceByRunId ?? {}),
  };
  for (const pruned of appendedEvents.slice(0, prunedEventCount)) {
    prunedSequenceByRunId[pruned.runId] = Math.max(
      prunedSequenceByRunId[pruned.runId] ?? 0,
      pruned.sequence,
    );
  }
  const seenEventKeys = new Set(
    events.map((item) => eventKey(item.runId, item.sequence)),
  );
  const lastSequenceByRunId = {
    ...(timeline.lastSequenceByRunId ?? {}),
    [event.runId]: Math.max(
      timeline.lastSequenceByRunId?.[event.runId] ?? 0,
      event.sequence,
    ),
  };
  let activity = projectRunActivity(currentActivity, retainedEvent);

  if (event.kind === "run_started") {
    activeRunId = event.runId;
  }

  if (event.kind === "assistant_delta") {
    messages = upsertAssistantMessage(
      messages,
      activity.assistantText,
      event.createdAt,
      event.runId,
    );
  }

  const previousRun = state.runs[event.runId];
  const runStatus = statusFromEvent(event.kind, previousRun?.status);
  activity = { ...activity, status: runStatus };
  const terminalKind = isTerminalKind(event.kind)
    ? event.kind
    : previousRun?.terminalKind ?? null;
  const activitiesByRunId = {
    ...timeline.activitiesByRunId,
    [event.runId]: activity,
  };
  const activeActivity = activitiesByRunId[activeRunId] ?? activity;

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
        lastSequenceByRunId,
        prunedSequenceByRunId,
        omittedEventCount:
          (timeline.omittedEventCount ?? 0) + prunedEventCount,
        assistantText: activeActivity.assistantText,
        reasoningText: activeActivity.reasoningText,
        plan: activeActivity.plan,
        tools: activeActivity.tools,
        subagents: activeActivity.subagents,
        activitiesByRunId,
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
  const seedActivities: ConversationTimelineView["activitiesByRunId"] = {};
  for (const run of timeline.runs) {
    seedRuns[run.runId] = {
      runId: run.runId,
      conversationId: run.conversationId,
      status: run.status,
      terminalKind: terminalKindFromStatus(run.status),
    };
    seedActivities[run.runId] = emptyRunActivity(
      run.runId,
      null,
      run.status,
    );
  }

  let view: ConversationTimelineView = {
    ...emptyTimeline(timeline.conversationId),
    messages: timeline.messages
      .filter((message) => message.role === "user" || message.role === "system")
      .map((message) => ({ ...message })),
    activitiesByRunId: seedActivities,
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

  // Authoritative snapshot statuses win after idempotent event reconstruction.
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
    runs[run.runId] = {
      ...projected,
      status: run.status,
      terminalKind: terminalKindFromStatus(run.status),
    };
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

  // Restored interrupted / terminal runs must close orphan running children.
  view = normalizeTimelineActivitiesForRuns(view, runs);

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
