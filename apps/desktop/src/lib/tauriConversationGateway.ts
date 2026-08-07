/**
 * Tauri conversation gateway — typed commands + live runtime event stream.
 * No provider, credential, or quota state crosses this boundary.
 */

import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import type {
  ConversationDto,
  ConversationGateway,
  ResourceReferenceDto,
  RuntimeEventDto,
  RuntimeEventKind,
  TimelineDto,
  TimelineMessageDto,
  ToolInputSummaryDto,
  ToolReceiptReferenceDto,
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
  "write_proposal_requested",
  "write_proposal_resolved",
  "process_proposal_requested",
  "process_proposal_decided",
  "process_started",
  "process_terminal_outcome",
  "turn_completed",
  "turn_failed",
  "run_cancelled",
  "heartbeat",
];

const WRITE_MODES = ["create", "modify"] as const;
const WRITE_STATES = [
  "pending_approval",
  "approved",
  "applying",
  "succeeded",
  "rejected",
  "expired",
  "cancelled",
  "interrupted",
  "failed",
] as const;

/** Closed C3 public error codes on write_proposal_resolved events. */
const WRITE_PUBLIC_ERROR_CODES = [
  "approval_required",
  "approval_rejected",
  "approval_expired",
  "base_changed",
  "write_conflict",
  "path_rejected",
  "write_failed",
  "cancelled",
  "interrupted",
] as const;

const PROCESS_RISK_CLASSES = ["standard", "high"] as const;
const PROCESS_DECISIONS = ["approve_once", "reject"] as const;
const PROCESS_STATES = [
  "pending_approval",
  "approved",
  "running",
  "succeeded",
  "failed",
  "rejected",
  "expired",
  "cancelled",
  "interrupted",
] as const;

const MAX_ID_CHARS = 256;
const MAX_LABEL_CHARS = 256;
const MAX_PATH_CHARS = 1_024;
const MAX_QUERY_CHARS = 256;
const MAX_EVENT_TEXT_CHARS = 65_536;
const MAX_SUMMARY_CHARS = 1_024;
const MAX_RESOURCE_PREVIEW_CHARS = 16_384;
const MAX_PLAN_STEPS = 512;
const MAX_TOOL_BYTES = 16 * 1024 * 1024;
const MAX_TOOL_RESULTS = 10_000;
const MAX_PROCESS_ARGV = 64;
const MAX_PROCESS_ARGV_BYTES = 32_768;
const MAX_PROCESS_CAPTURED_BYTES = 256 * 1024;

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === "object" && value !== null && !Array.isArray(value);
}

function isNonEmptyString(value: unknown): value is string {
  return typeof value === "string" && value.trim().length > 0;
}

function isBoundedNonEmptyString(
  value: unknown,
  maxChars = MAX_ID_CHARS,
): value is string {
  return isNonEmptyString(value) && value.length <= maxChars;
}

function isRunStatus(value: unknown): value is RunStatus {
  return typeof value === "string" && (RUN_STATUSES as string[]).includes(value);
}

function isEventKind(value: unknown): value is RuntimeEventKind {
  return typeof value === "string" && (EVENT_KINDS as string[]).includes(value);
}

function hasOnlyKeys(
  value: Record<string, unknown>,
  keys: readonly string[],
): boolean {
  const allowed = new Set(keys);
  return Object.keys(value).every((key) => allowed.has(key));
}

function isSafeInteger(
  value: unknown,
  minimum = 0,
  maximum = Number.MAX_SAFE_INTEGER,
): value is number {
  return (
    typeof value === "number"
    && Number.isSafeInteger(value)
    && value >= minimum
    && value <= maximum
  );
}

function optionalString(value: unknown): value is string | undefined {
  return value === undefined || typeof value === "string";
}

function optionalBoolean(value: unknown): value is boolean | undefined {
  return value === undefined || typeof value === "boolean";
}

function bounded(value: string, maxChars: number): string {
  return value.slice(0, maxChars);
}

function boundedSingleLine(value: string, maxChars: number): string {
  return value.replace(/[\u0000-\u001f\u007f]/g, " ").slice(0, maxChars);
}

function invalidResponse(message = "Pillow received an invalid conversation response."): Error {
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
    || !isBoundedNonEmptyString(value.runId)
    || !isBoundedNonEmptyString(value.conversationId)
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
    || !isBoundedNonEmptyString(value.eventId)
    || !isBoundedNonEmptyString(value.runId)
    || !isBoundedNonEmptyString(value.conversationId)
    || !isSafeInteger(value.sequence, 1)
    || !isEventKind(value.kind)
    || !isRecord(value.payload)
    || !isBoundedNonEmptyString(value.createdAt, 128)
    || !hasOnlyKeys(value, [
      "eventId",
      "runId",
      "conversationId",
      "sequence",
      "kind",
      "payload",
      "createdAt",
    ])
  ) {
    throw invalidResponse("Invalid runtime event.");
  }
  const base = {
    eventId: value.eventId,
    runId: value.runId,
    conversationId: value.conversationId,
    sequence: value.sequence,
    createdAt: bounded(value.createdAt, 128),
  };
  const payload = value.payload;

  switch (value.kind) {
    case "run_started":
      if (
        !hasOnlyKeys(payload, ["prompt_preview"])
        || !optionalString(payload.prompt_preview)
      ) {
        throw invalidResponse("Invalid runtime event.");
      }
      return {
        ...base,
        kind: value.kind,
        payload: {
          prompt_preview: payload.prompt_preview === undefined
            ? undefined
            : bounded(payload.prompt_preview, MAX_SUMMARY_CHARS),
        },
      };
    case "assistant_delta":
    case "reasoning_summary_delta":
      if (
        !hasOnlyKeys(payload, ["text", "omitted_chars"])
        || typeof payload.text !== "string"
        || (
          payload.omitted_chars !== undefined
          && !isSafeInteger(payload.omitted_chars)
        )
      ) {
        throw invalidResponse("Invalid runtime event.");
      }
      return {
        ...base,
        kind: value.kind,
        payload: boundDeltaPayload(payload.text, payload.omitted_chars),
      } as RuntimeEventDto;
    case "plan_updated":
      return {
        ...base,
        kind: value.kind,
        payload: { steps: parsePlanSteps(payload) },
      };
    case "tool_call_started":
      return {
        ...base,
        kind: value.kind,
        payload: parseToolCallStarted(payload),
      };
    case "tool_call_output":
      if (
        !hasOnlyKeys(payload, ["tool_call_id", "chunk", "omitted_chars"])
        || !isBoundedNonEmptyString(payload.tool_call_id)
        || typeof payload.chunk !== "string"
        || (
          payload.omitted_chars !== undefined
          && !isSafeInteger(payload.omitted_chars)
        )
      ) {
        throw invalidResponse("Invalid runtime event.");
      }
      return {
        ...base,
        kind: value.kind,
        payload: {
          tool_call_id: payload.tool_call_id,
          chunk: bounded(payload.chunk, MAX_EVENT_TEXT_CHARS),
          omitted_chars:
            (payload.omitted_chars ?? 0)
            + Math.max(0, payload.chunk.length - MAX_EVENT_TEXT_CHARS),
        },
      };
    case "tool_call_completed":
      return {
        ...base,
        kind: value.kind,
        payload: parseToolCallCompleted(payload),
      };
    case "subagent_started":
      if (
        !hasOnlyKeys(payload, ["subagent_id", "role"])
        || !isBoundedNonEmptyString(payload.subagent_id)
        || !isNonEmptyString(payload.role)
      ) {
        throw invalidResponse("Invalid runtime event.");
      }
      return {
        ...base,
        kind: value.kind,
        payload: {
          subagent_id: payload.subagent_id,
          role: boundedSingleLine(payload.role, MAX_LABEL_CHARS),
        },
      };
    case "subagent_progress":
      if (
        !hasOnlyKeys(payload, ["subagent_id", "summary"])
        || !isBoundedNonEmptyString(payload.subagent_id)
        || typeof payload.summary !== "string"
      ) {
        throw invalidResponse("Invalid runtime event.");
      }
      return {
        ...base,
        kind: value.kind,
        payload: {
          subagent_id: payload.subagent_id,
          summary: bounded(payload.summary, MAX_SUMMARY_CHARS),
        },
      };
    case "subagent_completed":
      if (
        !hasOnlyKeys(payload, ["subagent_id", "status"])
        || !isBoundedNonEmptyString(payload.subagent_id)
        || !isActivityStatus(payload.status)
      ) {
        throw invalidResponse("Invalid runtime event.");
      }
      return {
        ...base,
        kind: value.kind,
        payload: {
          subagent_id: payload.subagent_id,
          status: payload.status,
        },
      };
    case "artifact_produced":
      return {
        ...base,
        kind: value.kind,
        payload: parseArtifactProduced(payload),
      };
    case "write_proposal_requested":
      return {
        ...base,
        kind: value.kind,
        payload: parseWriteProposalRequested(payload),
      };
    case "write_proposal_resolved":
      return {
        ...base,
        kind: value.kind,
        payload: parseWriteProposalResolved(payload),
      };
    case "process_proposal_requested":
      return {
        ...base,
        kind: value.kind,
        payload: parseProcessProposalRequested(payload),
      };
    case "process_proposal_decided":
      return {
        ...base,
        kind: value.kind,
        payload: parseProcessProposalDecided(payload),
      };
    case "process_started":
      return {
        ...base,
        kind: value.kind,
        payload: parseProcessStarted(payload),
      };
    case "process_terminal_outcome":
      return {
        ...base,
        kind: value.kind,
        payload: parseProcessTerminalOutcome(payload),
      };
    case "turn_completed":
      if (
        !hasOnlyKeys(payload, ["summary"])
        || !optionalString(payload.summary)
      ) {
        throw invalidResponse("Invalid runtime event.");
      }
      return {
        ...base,
        kind: value.kind,
        payload: {
          summary: payload.summary === undefined
            ? undefined
            : bounded(payload.summary, MAX_SUMMARY_CHARS),
        },
      };
    case "turn_failed":
      if (
        !hasOnlyKeys(payload, ["code", "message"])
        || !isNonEmptyString(payload.code)
        || typeof payload.message !== "string"
      ) {
        throw invalidResponse("Invalid runtime event.");
      }
      return {
        ...base,
        kind: value.kind,
        payload: {
          code: boundedSingleLine(payload.code, MAX_LABEL_CHARS),
          message: bounded(payload.message, MAX_SUMMARY_CHARS),
        },
      };
    case "run_cancelled":
      if (
        !hasOnlyKeys(payload, ["reason"])
        || !optionalString(payload.reason)
      ) {
        throw invalidResponse("Invalid runtime event.");
      }
      return {
        ...base,
        kind: value.kind,
        payload: {
          reason: payload.reason === undefined
            ? undefined
            : bounded(payload.reason, MAX_SUMMARY_CHARS),
        },
      };
    case "heartbeat":
      if (
        !hasOnlyKeys(payload, ["tick"])
        || !isSafeInteger(payload.tick)
      ) {
        throw invalidResponse("Invalid runtime event.");
      }
      return {
        ...base,
        kind: value.kind,
        payload: { tick: payload.tick },
      };
  }
}

function tryParseRuntimeEventDto(value: unknown): RuntimeEventDto | null {
  try {
    return parseRuntimeEventDto(value);
  } catch (cause) {
    console.error("Pillow ignored invalid runtime event payload.", cause);
    return null;
  }
}

function boundDeltaPayload(text: string, omittedChars: unknown) {
  const retained = bounded(text, MAX_EVENT_TEXT_CHARS);
  return {
    text: retained,
    omitted_chars:
      (typeof omittedChars === "number" ? omittedChars : 0)
      + text.length
      - retained.length,
  };
}

function parsePlanSteps(
  payload: Record<string, unknown>,
): Extract<RuntimeEventDto, { kind: "plan_updated" }>["payload"]["steps"] {
  if (
    !hasOnlyKeys(payload, ["steps"])
    || !Array.isArray(payload.steps)
    || payload.steps.length > MAX_PLAN_STEPS
  ) {
    throw invalidResponse("Invalid runtime event.");
  }
  return payload.steps.map((step) => {
    if (
      !isRecord(step)
      || !hasOnlyKeys(step, ["id", "title", "status"])
      || !isBoundedNonEmptyString(step.id)
      || !isNonEmptyString(step.title)
      || !isNonEmptyString(step.status)
    ) {
      throw invalidResponse("Invalid runtime event.");
    }
    return {
      id: step.id,
      title: boundedSingleLine(step.title, MAX_LABEL_CHARS),
      status: boundedSingleLine(step.status, 64),
    };
  });
}

function parseToolCallStarted(
  payload: Record<string, unknown>,
): Extract<RuntimeEventDto, { kind: "tool_call_started" }>["payload"] {
  if (
    !hasOnlyKeys(payload, [
      "tool_call_id",
      "name",
      "summary",
      "input_summary",
    ])
    || !isBoundedNonEmptyString(payload.tool_call_id)
    || !isNonEmptyString(payload.name)
    || !optionalString(payload.summary)
  ) {
    throw invalidResponse("Invalid runtime event.");
  }
  return {
    tool_call_id: payload.tool_call_id,
    name: boundedSingleLine(payload.name, MAX_LABEL_CHARS),
    summary: payload.summary === undefined
      ? undefined
      : boundedSingleLine(payload.summary, MAX_LABEL_CHARS),
    input_summary: payload.input_summary === undefined
      ? undefined
      : parseToolInputSummary(payload.input_summary),
  };
}

function parseToolInputSummary(value: unknown): ToolInputSummaryDto {
  if (!isRecord(value) || !isNonEmptyString(value.kind)) {
    throw invalidResponse("Invalid runtime event.");
  }
  if (value.kind === "fs_list") {
    if (
      !hasOnlyKeys(value, ["kind", "path", "max_results"])
      || !isNonEmptyString(value.path)
      || !isSafeInteger(value.max_results, 0, MAX_TOOL_RESULTS)
    ) {
      throw invalidResponse("Invalid runtime event.");
    }
    return {
      kind: value.kind,
      path: boundedSingleLine(value.path, MAX_PATH_CHARS),
      max_results: value.max_results,
    };
  }
  if (value.kind === "fs_read") {
    if (
      !hasOnlyKeys(value, ["kind", "path", "offset_bytes", "max_bytes"])
      || !isNonEmptyString(value.path)
      || !isSafeInteger(value.offset_bytes, 0, MAX_TOOL_BYTES)
      || !isSafeInteger(value.max_bytes, 0, MAX_TOOL_BYTES)
    ) {
      throw invalidResponse("Invalid runtime event.");
    }
    return {
      kind: value.kind,
      path: boundedSingleLine(value.path, MAX_PATH_CHARS),
      offset_bytes: value.offset_bytes,
      max_bytes: value.max_bytes,
    };
  }
  if (value.kind === "fs_search") {
    if (
      !hasOnlyKeys(value, [
        "kind",
        "path",
        "query_preview",
        "case_sensitive",
        "max_results",
      ])
      || !isNonEmptyString(value.path)
      || typeof value.query_preview !== "string"
      || typeof value.case_sensitive !== "boolean"
      || !isSafeInteger(value.max_results, 0, MAX_TOOL_RESULTS)
    ) {
      throw invalidResponse("Invalid runtime event.");
    }
    return {
      kind: value.kind,
      path: boundedSingleLine(value.path, MAX_PATH_CHARS),
      query_preview: boundedSingleLine(value.query_preview, MAX_QUERY_CHARS),
      case_sensitive: value.case_sensitive,
      max_results: value.max_results,
    };
  }
  throw invalidResponse("Invalid runtime event.");
}

function parseToolCallCompleted(
  payload: Record<string, unknown>,
): Extract<RuntimeEventDto, { kind: "tool_call_completed" }>["payload"] {
  if (
    !hasOnlyKeys(payload, ["tool_call_id", "status", "receipt"])
    || !isBoundedNonEmptyString(payload.tool_call_id)
    || !isActivityStatus(payload.status)
  ) {
    throw invalidResponse("Invalid runtime event.");
  }
  return {
    tool_call_id: payload.tool_call_id,
    status: payload.status,
    receipt: payload.receipt === undefined
      ? undefined
      : parseReceipt(payload.receipt),
  };
}

function isActivityStatus(
  value: unknown,
): value is "completed" | "failed" | "cancelled" {
  return value === "completed" || value === "failed" || value === "cancelled";
}

function parseReceipt(value: unknown): ToolReceiptReferenceDto {
  if (
    !isRecord(value)
    || !hasOnlyKeys(value, ["receipt_id", "label", "resource"])
    || !isBoundedNonEmptyString(value.receipt_id)
    || !isNonEmptyString(value.label)
  ) {
    throw invalidResponse("Invalid runtime event.");
  }
  return {
    receipt_id: value.receipt_id,
    label: boundedSingleLine(value.label, MAX_LABEL_CHARS),
    resource: value.resource === undefined
      ? undefined
      : parseResource(value.resource),
  };
}

function parseResource(value: unknown): ResourceReferenceDto {
  if (
    !isRecord(value)
    || !hasOnlyKeys(value, [
      "resource_id",
      "kind",
      "label",
      "path",
      "content_preview",
      "truncated",
    ])
    || !isBoundedNonEmptyString(value.resource_id)
    || !isResourceKind(value.kind)
    || !isNonEmptyString(value.label)
    || !optionalString(value.path)
    || !optionalString(value.content_preview)
    || (value.truncated !== undefined && typeof value.truncated !== "boolean")
  ) {
    throw invalidResponse("Invalid runtime event.");
  }
  const preview = value.content_preview === undefined
    ? undefined
    : bounded(value.content_preview, MAX_RESOURCE_PREVIEW_CHARS);
  return {
    resource_id: value.resource_id,
    kind: value.kind,
    label: boundedSingleLine(value.label, MAX_LABEL_CHARS),
    path: value.path === undefined
      ? undefined
      : boundedSingleLine(value.path, MAX_PATH_CHARS),
    content_preview: preview,
    truncated:
      value.truncated
      || (
        value.content_preview !== undefined
        && value.content_preview.length > (preview?.length ?? 0)
      ),
  };
}

function isResourceKind(
  value: unknown,
): value is ResourceReferenceDto["kind"] {
  return (
    value === "file"
    || value === "directory"
    || value === "search_results"
    || value === "artifact"
    || value === "receipt"
  );
}

function parseArtifactProduced(
  payload: Record<string, unknown>,
): Extract<RuntimeEventDto, { kind: "artifact_produced" }>["payload"] {
  if (
    !hasOnlyKeys(payload, ["artifact_id", "path", "kind", "resource"])
    || !isBoundedNonEmptyString(payload.artifact_id)
    || !isNonEmptyString(payload.path)
    || !isNonEmptyString(payload.kind)
  ) {
    throw invalidResponse("Invalid runtime event.");
  }
  return {
    artifact_id: payload.artifact_id,
    path: boundedSingleLine(payload.path, MAX_PATH_CHARS),
    kind: boundedSingleLine(payload.kind, MAX_LABEL_CHARS),
    resource: payload.resource === undefined
      ? undefined
      : parseResource(payload.resource),
  };
}

function isWriteMode(
  value: unknown,
): value is (typeof WRITE_MODES)[number] {
  return typeof value === "string"
    && (WRITE_MODES as readonly string[]).includes(value);
}

function isWriteState(
  value: unknown,
): value is (typeof WRITE_STATES)[number] {
  return typeof value === "string"
    && (WRITE_STATES as readonly string[]).includes(value);
}

function isWritePublicErrorCode(
  value: unknown,
): value is (typeof WRITE_PUBLIC_ERROR_CODES)[number] {
  return typeof value === "string"
    && (WRITE_PUBLIC_ERROR_CODES as readonly string[]).includes(value);
}

/**
 * Portable relative path grammar for write events — never absolute/native.
 * Matches the write-approval Tauri boundary (slash-separated components).
 */
function isPortableRelativePath(value: unknown): value is string {
  if (typeof value !== "string" || value.length === 0 || value.length > MAX_PATH_CHARS) {
    return false;
  }
  if (
    value.startsWith("/")
    || value.startsWith("\\")
    || value.includes("\\")
    || value.includes(":")
    || value.includes("..")
  ) {
    return false;
  }
  return value.split("/").every((component) => {
    if (component.length === 0 || component === "." || component === "..") {
      return false;
    }
    if (component.length > 255) return false;
    return !/[<>:"|?*\u0000-\u001f]/.test(component);
  });
}

function isFiniteTimestamp(value: unknown): value is string {
  if (typeof value !== "string" || value.length === 0 || value.length > 128) {
    return false;
  }
  return Number.isFinite(Date.parse(value));
}

function parseWriteProposalRequested(
  payload: Record<string, unknown>,
): Extract<RuntimeEventDto, { kind: "write_proposal_requested" }>["payload"] {
  if (
    !hasOnlyKeys(payload, [
      "proposal_id",
      "path",
      "mode",
      "summary",
      "expires_at",
    ])
    || !isBoundedNonEmptyString(payload.proposal_id)
    || !isPortableRelativePath(payload.path)
    || !isWriteMode(payload.mode)
    || typeof payload.summary !== "string"
    || payload.summary.length > MAX_SUMMARY_CHARS
    || !isFiniteTimestamp(payload.expires_at)
  ) {
    throw invalidResponse("Invalid runtime event.");
  }
  return {
    proposal_id: payload.proposal_id,
    path: payload.path,
    mode: payload.mode,
    summary: payload.summary,
    expires_at: payload.expires_at,
  };
}

function parseWriteProposalResolved(
  payload: Record<string, unknown>,
): Extract<RuntimeEventDto, { kind: "write_proposal_resolved" }>["payload"] {
  if (
    !hasOnlyKeys(payload, [
      "proposal_id",
      "path",
      "mode",
      "state",
      "error_code",
    ])
    || !isBoundedNonEmptyString(payload.proposal_id)
    || !isPortableRelativePath(payload.path)
    || !isWriteMode(payload.mode)
    || !isWriteState(payload.state)
    || (
      payload.error_code !== undefined
      && !isWritePublicErrorCode(payload.error_code)
    )
  ) {
    throw invalidResponse("Invalid runtime event.");
  }
  return {
    proposal_id: payload.proposal_id,
    path: payload.path,
    mode: payload.mode,
    state: payload.state,
    error_code: payload.error_code === undefined
      ? undefined
      : payload.error_code,
  };
}

function isProcessRiskClass(
  value: unknown,
): value is (typeof PROCESS_RISK_CLASSES)[number] {
  return typeof value === "string"
    && (PROCESS_RISK_CLASSES as readonly string[]).includes(value);
}

function isProcessDecision(
  value: unknown,
): value is (typeof PROCESS_DECISIONS)[number] {
  return typeof value === "string"
    && (PROCESS_DECISIONS as readonly string[]).includes(value);
}

function isProcessState(
  value: unknown,
): value is (typeof PROCESS_STATES)[number] {
  return typeof value === "string"
    && (PROCESS_STATES as readonly string[]).includes(value);
}

function isProcessPortableCwd(value: unknown): value is string {
  return value === "." || isPortableRelativePath(value);
}

function processArgvIsBounded(value: unknown): value is string[] {
  return Array.isArray(value)
    && value.length <= MAX_PROCESS_ARGV
    && value.every((argument) => typeof argument === "string")
    && value.reduce((total, argument) => total + new TextEncoder().encode(argument).length, 0)
      <= MAX_PROCESS_ARGV_BYTES;
}

function parseProcessProposalRequested(
  payload: Record<string, unknown>,
): Extract<RuntimeEventDto, { kind: "process_proposal_requested" }>["payload"] {
  if (
    !hasOnlyKeys(payload, [
      "proposal_id",
      "program",
      "argv",
      "portable_cwd",
      "summary",
      "risk_class",
      "expires_at",
    ])
    || !isBoundedNonEmptyString(payload.proposal_id)
    || !isBoundedNonEmptyString(payload.program, 32)
    || !processArgvIsBounded(payload.argv)
    || (payload.portable_cwd !== undefined && !isProcessPortableCwd(payload.portable_cwd))
    || typeof payload.summary !== "string"
    || payload.summary.length > MAX_SUMMARY_CHARS
    || !isProcessRiskClass(payload.risk_class)
    || !isFiniteTimestamp(payload.expires_at)
  ) {
    throw invalidResponse("Invalid runtime event.");
  }
  return {
    proposal_id: payload.proposal_id,
    program: payload.program,
    argv: [...payload.argv],
    portable_cwd: payload.portable_cwd,
    summary: payload.summary,
    risk_class: payload.risk_class,
    expires_at: payload.expires_at,
  };
}

function parseProcessProposalDecided(
  payload: Record<string, unknown>,
): Extract<RuntimeEventDto, { kind: "process_proposal_decided" }>["payload"] {
  if (
    !hasOnlyKeys(payload, ["proposal_id", "program", "decision", "state"])
    || !isBoundedNonEmptyString(payload.proposal_id)
    || !isBoundedNonEmptyString(payload.program, 32)
    || !isProcessDecision(payload.decision)
    || !isProcessState(payload.state)
  ) {
    throw invalidResponse("Invalid runtime event.");
  }
  return {
    proposal_id: payload.proposal_id,
    program: payload.program,
    decision: payload.decision,
    state: payload.state,
  };
}

function parseProcessStarted(
  payload: Record<string, unknown>,
): Extract<RuntimeEventDto, { kind: "process_started" }>["payload"] {
  if (
    !hasOnlyKeys(payload, ["proposal_id", "program", "argv", "portable_cwd"])
    || !isBoundedNonEmptyString(payload.proposal_id)
    || !isBoundedNonEmptyString(payload.program, 32)
    || !processArgvIsBounded(payload.argv)
    || (payload.portable_cwd !== undefined && !isProcessPortableCwd(payload.portable_cwd))
  ) {
    throw invalidResponse("Invalid runtime event.");
  }
  return {
    proposal_id: payload.proposal_id,
    program: payload.program,
    argv: [...payload.argv],
    portable_cwd: payload.portable_cwd,
  };
}

function parseProcessTerminalOutcome(
  payload: Record<string, unknown>,
): Extract<RuntimeEventDto, { kind: "process_terminal_outcome" }>["payload"] {
  if (
    !hasOnlyKeys(payload, [
      "proposal_id",
      "program",
      "state",
      "exit_code",
      "duration_ms",
      "truncated",
      "encoding_lossy",
      "captured_bytes",
      "cancelled",
      "timed_out",
    ])
    || !isBoundedNonEmptyString(payload.proposal_id)
    || !isBoundedNonEmptyString(payload.program, 32)
    || !isProcessState(payload.state)
    || (payload.exit_code !== undefined
      && !isSafeInteger(payload.exit_code, -2_147_483_648, 2_147_483_647))
    || (payload.duration_ms !== undefined && !isSafeInteger(payload.duration_ms))
    || (payload.captured_bytes !== undefined
      && !isSafeInteger(payload.captured_bytes, 0, MAX_PROCESS_CAPTURED_BYTES))
    || !optionalBoolean(payload.truncated)
    || !optionalBoolean(payload.encoding_lossy)
    || !optionalBoolean(payload.cancelled)
    || !optionalBoolean(payload.timed_out)
  ) {
    throw invalidResponse("Invalid runtime event.");
  }
  return {
    proposal_id: payload.proposal_id,
    program: payload.program,
    state: payload.state,
    exit_code: payload.exit_code,
    duration_ms: payload.duration_ms,
    truncated: payload.truncated,
    encoding_lossy: payload.encoding_lossy,
    captured_bytes: payload.captured_bytes,
    cancelled: payload.cancelled,
    timed_out: payload.timed_out,
  };
}

function parseTimelineDto(value: unknown): TimelineDto {
  if (
    !isRecord(value)
    || !isBoundedNonEmptyString(value.conversationId)
    || !Array.isArray(value.messages)
    || !Array.isArray(value.events)
    || !Array.isArray(value.runs)
  ) {
    throw invalidResponse("Invalid timeline.");
  }
  return {
    conversationId: value.conversationId,
    messages: value.messages.map(parseMessageDto),
    events: value.events.flatMap((event) => {
      const parsed = tryParseRuntimeEventDto(event);
      return parsed ? [parsed] : [];
    }),
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
      const parsed = tryParseRuntimeEventDto(event.payload);
      if (parsed) listener(parsed);
    }).then((fn) => {
      if (cancelled) {
        fn();
        return;
      }
      unlisten = fn;
    }).catch((cause: unknown) => {
      console.error("Pillow failed to subscribe to runtime events.", cause);
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
