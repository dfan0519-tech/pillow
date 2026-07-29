/**
 * Tauri conversation gateway — typed commands + live runtime event stream.
 * No provider, credential, or quota state crosses this boundary.
 */

import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import type {
  ConversationDto,
  ConversationGateway,
  RuntimeEventDto,
  RuntimeEventKind,
  TimelineDto,
  TimelineMessageDto,
  RunDto,
  RunStatus,
} from "./conversationGateway";

const RUNTIME_EVENT = "conversation://runtime-event";

const RUN_STATUSES: RunStatus[] = [
  "pending",
  "running",
  "cancelling",
  "completed",
  "failed",
  "cancelled",
  "interrupted",
];

const EVENT_KINDS: RuntimeEventKind[] = [
  "run_started",
  "assistant_delta",
  "reasoning_summary_delta",
  "plan_updated",
  "tool_call_started",
  "tool_call_output",
  "tool_call_completed",
  "subagent_started",
  "subagent_progress",
  "subagent_completed",
  "artifact_produced",
  "turn_completed",
  "turn_failed",
  "run_cancelled",
  "heartbeat",
];

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === "object" && value !== null && !Array.isArray(value);
}

function isNonEmptyString(value: unknown): value is string {
  return typeof value === "string" && value.trim().length > 0;
}

function isRunStatus(value: unknown): value is RunStatus {
  return typeof value === "string" && (RUN_STATUSES as string[]).includes(value);
}

function isEventKind(value: unknown): value is RuntimeEventKind {
  return typeof value === "string" && (EVENT_KINDS as string[]).includes(value);
}

function invalidResponse(message = "Keeper IDE received an invalid conversation response."): Error {
  return new Error(message);
}

function parseConversationDto(value: unknown): ConversationDto {
  if (
    !isRecord(value)
    || !isNonEmptyString(value.id)
    || !isNonEmptyString(value.projectId)
    || !isNonEmptyString(value.title)
    || !isNonEmptyString(value.createdAt)
    || !isNonEmptyString(value.updatedAt)
  ) {
    throw invalidResponse();
  }
  return {
    id: value.id,
    projectId: value.projectId,
    title: value.title,
    createdAt: value.createdAt,
    updatedAt: value.updatedAt,
  };
}

function parseMessageDto(value: unknown): TimelineMessageDto {
  if (
    !isRecord(value)
    || !isNonEmptyString(value.id)
    || (value.role !== "user" && value.role !== "assistant" && value.role !== "system")
    || typeof value.content !== "string"
    || !isNonEmptyString(value.createdAt)
  ) {
    throw invalidResponse("Invalid timeline message.");
  }
  return {
    id: value.id,
    role: value.role,
    content: value.content,
    createdAt: value.createdAt,
  };
}

function parseRunDto(value: unknown): RunDto {
  if (
    !isRecord(value)
    || !isNonEmptyString(value.runId)
    || !isNonEmptyString(value.conversationId)
    || !isRunStatus(value.status)
  ) {
    throw invalidResponse("Invalid run snapshot.");
  }
  return {
    runId: value.runId,
    conversationId: value.conversationId,
    status: value.status,
  };
}

function parseRuntimeEventDto(value: unknown): RuntimeEventDto {
  if (
    !isRecord(value)
    || !isNonEmptyString(value.eventId)
    || !isNonEmptyString(value.runId)
    || !isNonEmptyString(value.conversationId)
    || typeof value.sequence !== "number"
    || !Number.isFinite(value.sequence)
    || !isEventKind(value.kind)
    || !isRecord(value.payload)
    || !isNonEmptyString(value.createdAt)
  ) {
    throw invalidResponse("Invalid runtime event.");
  }
  return {
    eventId: value.eventId,
    runId: value.runId,
    conversationId: value.conversationId,
    sequence: value.sequence,
    kind: value.kind,
    payload: value.payload,
    createdAt: value.createdAt,
  } as RuntimeEventDto;
}

function parseTimelineDto(value: unknown): TimelineDto {
  if (
    !isRecord(value)
    || !isNonEmptyString(value.conversationId)
    || !Array.isArray(value.messages)
    || !Array.isArray(value.events)
    || !Array.isArray(value.runs)
  ) {
    throw invalidResponse("Invalid timeline.");
  }
  return {
    conversationId: value.conversationId,
    messages: value.messages.map(parseMessageDto),
    events: value.events.map(parseRuntimeEventDto),
    runs: value.runs.map(parseRunDto),
  };
}

function normalizeCommandError(error: unknown): Error {
  if (error instanceof Error) return error;
  if (
    isRecord(error)
    && typeof error.message === "string"
    && error.message.trim().length > 0
  ) {
    return new Error(error.message);
  }
  return new Error("Conversation command failed.");
}

/** Tauri-only conversation boundary. Renderer never opens SQLite or hosts. */
export class TauriConversationGateway implements ConversationGateway {
  async listConversations(projectId: string): Promise<ConversationDto[]> {
    try {
      const payload = await invoke<unknown>("list_conversations", { projectId });
      if (!Array.isArray(payload)) throw invalidResponse();
      return payload.map(parseConversationDto);
    } catch (error) {
      throw normalizeCommandError(error);
    }
  }

  async createConversation(
    projectId: string,
    title?: string,
  ): Promise<ConversationDto> {
    try {
      const payload = await invoke<unknown>("create_conversation", {
        projectId,
        title: title ?? null,
      });
      return parseConversationDto(payload);
    } catch (error) {
      throw normalizeCommandError(error);
    }
  }

  async loadTimeline(conversationId: string): Promise<TimelineDto> {
    try {
      const payload = await invoke<unknown>("load_timeline", { conversationId });
      return parseTimelineDto(payload);
    } catch (error) {
      throw normalizeCommandError(error);
    }
  }

  async sendTurn(
    conversationId: string,
    content: string,
  ): Promise<{ runId: string }> {
    try {
      const payload = await invoke<unknown>("send_turn", {
        conversationId,
        content,
      });
      if (!isRecord(payload) || !isNonEmptyString(payload.runId)) {
        throw invalidResponse("Invalid send_turn response.");
      }
      return { runId: payload.runId };
    } catch (error) {
      throw normalizeCommandError(error);
    }
  }

  async cancelRun(runId: string): Promise<void> {
    try {
      await invoke("cancel_run", { runId });
    } catch (error) {
      throw normalizeCommandError(error);
    }
  }

  /**
   * Subscribe to host-emitted runtime events. Unsubscribe is always safe,
   * including when called before the async listen registration completes.
   */
  subscribe(listener: (event: RuntimeEventDto) => void): () => void {
    let unlisten: UnlistenFn | null = null;
    let cancelled = false;

    void listen<unknown>(RUNTIME_EVENT, (event) => {
      try {
        listener(parseRuntimeEventDto(event.payload));
      } catch (cause) {
        console.error("Keeper IDE ignored invalid runtime event payload.", cause);
      }
    }).then((fn) => {
      if (cancelled) {
        fn();
        return;
      }
      unlisten = fn;
    }).catch((cause: unknown) => {
      console.error("Keeper IDE failed to subscribe to runtime events.", cause);
    });

    return () => {
      cancelled = true;
      if (unlisten) {
        unlisten();
        unlisten = null;
      }
    };
  }
}

export const CONVERSATION_RUNTIME_EVENT = RUNTIME_EVENT;
