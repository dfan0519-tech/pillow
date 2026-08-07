/**
 * Pure conversation UI reducer. Projects normalized runtime events into
 * per-conversation timelines without depending on provider payloads.
 */

import type { ConversationDto, TimelineDto } from "../../lib/conversationGateway";
import {
  applyRuntimeEvent,
  emptyTimeline,
  mergeTimelineWithInFlight,
} from "./applyRuntimeEvent";
import type {
  ConversationAction as BaseConversationAction,
  ConversationTimelineView,
  ConversationUiState as BaseConversationUiState,
  RunView,
} from "./conversationTypes";

/**
 * Extended UI state: per-project monotonic list-load freshness.
 * Defined here so callers of this reducer get the full contract without
 * requiring a parallel change to the base type module for this race fix.
 */
export type ConversationUiState = BaseConversationUiState & {
  /**
   * Highest `listSeq` already applied for each project via
   * `conversations_loaded`. Strictly older results are discarded.
   */
  conversationListSeqByProject: Record<string, number>;
};

/** `conversations_loaded` carries a per-project list freshness sequence. */
export type ConversationAction =
  | {
      type: "conversations_loaded";
      projectId: string;
      conversations: ConversationDto[];
      /** Monotonic per-project sequence assigned when listConversations starts. */
      listSeq: number;
    }
  | Exclude<BaseConversationAction, { type: "conversations_loaded" }>;

export type {
  ArtifactActivity,
  ConversationTimelineView,
  PlanActivity,
  PlanStepView,
  ReceiptReferenceView,
  ResourceReferenceView,
  RunActivityView,
  RunView,
  SubagentActivity,
  ToolActivity,
} from "./conversationTypes";

export function createInitialConversationState(): ConversationUiState {
  return {
    conversationsByProject: {},
    selectedConversationId: null,
    timelines: {},
    runs: {},
    conversationListSeqByProject: {},
  };
}

/**
 * Whether an incoming list load should replace the applied projection.
 * Equal or older sequences are stale (out-of-order) and must be dropped.
 */
export function shouldApplyConversationListSeq(
  appliedSeq: number | undefined,
  incomingSeq: number,
): boolean {
  return incomingSeq > (appliedSeq ?? 0);
}

export function conversationReducer(
  state: ConversationUiState,
  action: ConversationAction,
): ConversationUiState {
  switch (action.type) {
    case "conversations_loaded": {
      const applied = state.conversationListSeqByProject[action.projectId];
      if (!shouldApplyConversationListSeq(applied, action.listSeq)) {
        return state;
      }
      return {
        ...state,
        conversationsByProject: {
          ...state.conversationsByProject,
          [action.projectId]: action.conversations,
        },
        conversationListSeqByProject: {
          ...state.conversationListSeqByProject,
          [action.projectId]: action.listSeq,
        },
      };
    }
    case "conversation_created": {
      const { conversation } = action;
      const select = action.select !== false;
      const existing = state.conversationsByProject[conversation.projectId] ?? [];
      const without = existing.filter((item) => item.id !== conversation.id);
      return {
        ...state,
        conversationsByProject: {
          ...state.conversationsByProject,
          [conversation.projectId]: [...without, conversation],
        },
        selectedConversationId: select
          ? conversation.id
          : state.selectedConversationId,
      };
    }
    case "conversation_selected":
      return {
        ...state,
        selectedConversationId: action.conversationId,
      };
    case "timeline_loaded":
      // mergeTimelineWithInFlight spreads `...state`, so list-seq map is kept.
      return mergeTimelineWithInFlight(state, action.timeline) as ConversationUiState;
    case "user_message_appended": {
      const timeline =
        state.timelines[action.conversationId]
        ?? emptyTimeline(action.conversationId);
      if (timeline.messages.some((message) => message.id === action.message.id)) {
        return state;
      }
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
    case "user_message_removed": {
      const timeline = state.timelines[action.conversationId];
      if (!timeline) return state;
      return {
        ...state,
        timelines: {
          ...state.timelines,
          [action.conversationId]: {
            ...timeline,
            messages: timeline.messages.filter(
              (message) => message.id !== action.messageId,
            ),
          },
        },
      };
    }
    case "runtime_event":
      // applyRuntimeEvent spreads `...state`, so list-seq map is kept.
      return applyRuntimeEvent(state, action.event) as ConversationUiState;
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
): import("../../lib/conversationGateway").ConversationDto[] {
  if (!projectId) return [];
  return state.conversationsByProject[projectId] ?? [];
}

export function selectActiveTimeline(
  state: ConversationUiState,
): ConversationTimelineView | null {
  if (!state.selectedConversationId) return null;
  return state.timelines[state.selectedConversationId] ?? null;
}

export function selectActiveRun(state: ConversationUiState): RunView | null {
  const timeline = selectActiveTimeline(state);
  if (!timeline?.activeRunId) return null;
  return state.runs[timeline.activeRunId] ?? null;
}

export function isRunActive(run: RunView | null | undefined): boolean {
  if (!run) return false;
  return (
    run.status === "pending"
    || run.status === "running"
    || run.status === "cancelling"
  );
}

/** True when the conversation belongs to the given project in UI state. */
export function conversationBelongsToProject(
  state: ConversationUiState,
  conversationId: string | null,
  projectId: string | null,
): boolean {
  if (!conversationId || !projectId) return false;
  const list = state.conversationsByProject[projectId] ?? [];
  return list.some((item) => item.id === conversationId);
}

export function emptyTimelineView(conversationId: string): ConversationTimelineView {
  return emptyTimeline(conversationId);
}

export type { TimelineDto };
