/**
 * Pure conversation UI reducer. Projects normalized runtime events into
 * per-conversation timelines without depending on provider payloads.
 */

import type { TimelineDto } from "../../lib/conversationGateway";
import {
  applyRuntimeEvent,
  emptyTimeline,
  mergeTimelineWithInFlight,
} from "./applyRuntimeEvent";
import type {
  ConversationAction,
  ConversationTimelineView,
  ConversationUiState,
  RunView,
} from "./conversationTypes";

export type {
  ConversationAction,
  ConversationTimelineView,
  ConversationUiState,
  PlanActivity,
  PlanStepView,
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
      return mergeTimelineWithInFlight(state, action.timeline);
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
