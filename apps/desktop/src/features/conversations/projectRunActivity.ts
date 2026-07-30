/**
 * Pure projection of one runtime event into its owning run activity.
 */

import type {
  ResourceReferenceDto,
  RuntimeEventDto,
  RunStatus,
  ToolInputSummaryDto,
} from "../../lib/conversationGateway";
import type {
  ArtifactActivity,
  PlanStepView,
  ResourceReferenceView,
  RunActivityView,
  SubagentActivity,
  ToolActivity,
} from "./conversationTypes";
import { closeOpenActivities } from "./normalizeTerminalActivity";

export const MAX_TOOL_OUTPUT_CHARS = 16_384;
export const MAX_RUN_TOOL_OUTPUT_CHARS = 49_152;
export const MAX_RUN_TEXT_CHARS = 65_536;
export const MAX_RESOURCE_PREVIEW_CHARS = 16_384;
const MAX_REFERENCE_PATH_CHARS = 1_024;
const MAX_LABEL_CHARS = 256;
const MAX_QUERY_PREVIEW_CHARS = 256;

export function emptyRunActivity(
  runId: string,
  createdAt: string | null,
  status: RunStatus = "pending",
): RunActivityView {
  return {
    runId,
    status,
    startedAt: createdAt,
    updatedAt: createdAt,
    assistantText: "",
    assistantTextTruncated: false,
    omittedAssistantChars: 0,
    reasoningText: "",
    reasoningTextTruncated: false,
    omittedReasoningChars: 0,
    plan: null,
    tools: [],
    subagents: [],
    artifacts: [],
  };
}

export function boundEventForRetention(
  current: RunActivityView,
  event: RuntimeEventDto,
): RuntimeEventDto {
  if (
    event.kind === "assistant_delta"
    || event.kind === "reasoning_summary_delta"
  ) {
    const allowed = runTextAllowance(current, event.payload.text.length);
    const omitted =
      (event.payload.omitted_chars ?? 0)
      + event.payload.text.length
      - allowed;
    return {
      ...event,
      payload: {
        ...event.payload,
        text: event.payload.text.slice(0, allowed),
        ...(omitted > 0 ? { omitted_chars: omitted } : {}),
      },
    } as RuntimeEventDto;
  }
  if (event.kind === "tool_call_started") {
    return {
      ...event,
      payload: {
        ...event.payload,
        name: boundedSingleLine(event.payload.name, MAX_LABEL_CHARS),
        summary: event.payload.summary
          ? boundedSingleLine(event.payload.summary, MAX_LABEL_CHARS)
          : undefined,
        input_summary: event.payload.input_summary
          ? boundInputSummary(event.payload.input_summary)
          : undefined,
      },
    };
  }
  if (event.kind === "tool_call_completed" && event.payload.receipt) {
    return {
      ...event,
      payload: {
        ...event.payload,
        receipt: {
          ...event.payload.receipt,
          receipt_id: event.payload.receipt.receipt_id.slice(
            0,
            MAX_LABEL_CHARS,
          ),
          label: boundedSingleLine(
            event.payload.receipt.label,
            MAX_LABEL_CHARS,
          ),
          resource: event.payload.receipt.resource
            ? boundResource(event.payload.receipt.resource)
            : undefined,
        },
      },
    };
  }
  if (event.kind === "artifact_produced") {
    return {
      ...event,
      payload: {
        ...event.payload,
        path: boundedSingleLine(
          event.payload.path,
          MAX_REFERENCE_PATH_CHARS,
        ),
        kind: boundedSingleLine(event.payload.kind, MAX_LABEL_CHARS),
        resource: event.payload.resource
          ? boundResource(event.payload.resource)
          : undefined,
      },
    };
  }
  if (event.kind !== "tool_call_output") return event;

  const allowed = outputAllowance(
    current,
    event.payload.tool_call_id,
    event.payload.chunk.length,
  );
  const omitted =
    (event.payload.omitted_chars ?? 0)
    + event.payload.chunk.length
    - allowed;
  return {
    ...event,
    payload: {
      ...event.payload,
      chunk: event.payload.chunk.slice(0, allowed),
      ...(omitted > 0 ? { omitted_chars: omitted } : {}),
    },
  };
}

export function projectRunActivity(
  current: RunActivityView,
  event: RuntimeEventDto,
): RunActivityView {
  let activity: RunActivityView = {
    ...current,
    startedAt: current.startedAt ?? event.createdAt,
    updatedAt: event.createdAt,
  };

  if (event.kind === "run_started") {
    activity = {
      ...activity,
      promptPreview: event.payload.prompt_preview,
      status: "running",
    };
  } else if (event.kind === "assistant_delta") {
    const allowed = runTextAllowance(activity, event.payload.text.length);
    const retained = event.payload.text.slice(0, allowed);
    const omitted =
      (event.payload.omitted_chars ?? 0)
      + event.payload.text.length
      - allowed;
    activity = {
      ...activity,
      assistantText: `${activity.assistantText}${retained}`,
      assistantTextTruncated:
        activity.assistantTextTruncated || omitted > 0,
      omittedAssistantChars:
        (activity.omittedAssistantChars ?? 0) + omitted,
    };
  } else if (event.kind === "reasoning_summary_delta") {
    const allowed = runTextAllowance(activity, event.payload.text.length);
    const retained = event.payload.text.slice(0, allowed);
    const omitted =
      (event.payload.omitted_chars ?? 0)
      + event.payload.text.length
      - allowed;
    activity = {
      ...activity,
      reasoningText: `${activity.reasoningText}${retained}`,
      reasoningTextTruncated:
        activity.reasoningTextTruncated || omitted > 0,
      omittedReasoningChars:
        (activity.omittedReasoningChars ?? 0) + omitted,
    };
  } else if (event.kind === "plan_updated") {
    activity = {
      ...activity,
      plan: {
        steps: event.payload.steps.map(
          (step): PlanStepView => ({
            id: step.id,
            title: step.title,
            status: step.status,
          }),
        ),
      },
    };
  } else if (event.kind === "tool_call_started") {
    activity = {
      ...activity,
      tools: upsertTool(activity.tools, {
        toolCallId: event.payload.tool_call_id,
        name: event.payload.name,
        title: event.payload.summary ?? event.payload.name,
        status: "running",
        startedAt: event.createdAt,
        updatedAt: event.createdAt,
        inputSummary: event.payload.input_summary
          ? boundInputSummary(event.payload.input_summary)
          : undefined,
        resources: [],
      }),
    };
  } else if (event.kind === "tool_call_output") {
    const allowed = outputAllowance(
      activity,
      event.payload.tool_call_id,
      event.payload.chunk.length,
    );
    const retained = event.payload.chunk.slice(0, allowed);
    const omitted =
      (event.payload.omitted_chars ?? 0)
      + event.payload.chunk.length
      - allowed;
    activity = {
      ...activity,
      tools: activity.tools.map((tool) =>
        tool.toolCallId === event.payload.tool_call_id
          ? {
              ...tool,
              output: `${tool.output ?? ""}${retained}`,
              outputTruncated: tool.outputTruncated || omitted > 0,
              omittedOutputChars:
                (tool.omittedOutputChars ?? 0) + omitted,
              updatedAt: event.createdAt,
            }
          : tool,
      ),
    };
  } else if (event.kind === "tool_call_completed") {
    activity = {
      ...activity,
      tools: activity.tools.map((tool) =>
        tool.toolCallId === event.payload.tool_call_id
          ? {
              ...tool,
              status: event.payload.status,
              updatedAt: event.createdAt,
              ...projectReceipt(event.payload.receipt),
            }
          : tool,
      ),
    };
  } else if (event.kind === "subagent_started") {
    activity = {
      ...activity,
      subagents: upsertSubagent(activity.subagents, {
        subagentId: event.payload.subagent_id,
        title: event.payload.role,
        status: "running",
        startedAt: event.createdAt,
        updatedAt: event.createdAt,
      }),
    };
  } else if (event.kind === "subagent_progress") {
    activity = {
      ...activity,
      subagents: activity.subagents.map((item) =>
        item.subagentId === event.payload.subagent_id
          ? {
              ...item,
              progress: event.payload.summary,
              updatedAt: event.createdAt,
            }
          : item,
      ),
    };
  } else if (event.kind === "subagent_completed") {
    activity = {
      ...activity,
      subagents: activity.subagents.map((item) =>
        item.subagentId === event.payload.subagent_id
          ? {
              ...item,
              status: event.payload.status,
              updatedAt: event.createdAt,
            }
          : item,
      ),
    };
  } else if (event.kind === "artifact_produced") {
    const resource = event.payload.resource
      ? projectResource(event.payload.resource)
      : {
          resourceId: `artifact:${event.payload.artifact_id}`,
          kind: "artifact" as const,
          label: event.payload.path,
          path: event.payload.path,
          truncated: false,
        };
    activity = {
      ...activity,
      artifacts: upsertArtifact(activity.artifacts, {
        artifactId: event.payload.artifact_id,
        path: event.payload.path,
        kind: event.payload.kind,
        createdAt: event.createdAt,
        resource,
      }),
    };
  }

  if (event.kind === "turn_completed") {
    activity = closeActivity(activity, "completed");
  } else if (event.kind === "turn_failed") {
    activity = closeActivity(activity, "failed");
  } else if (event.kind === "run_cancelled") {
    activity = closeActivity(activity, "cancelled");
  }

  return activity;
}

function outputAllowance(
  activity: RunActivityView,
  toolCallId: string,
  requested: number,
): number {
  const tool = activity.tools.find((item) => item.toolCallId === toolCallId);
  const toolRemaining = Math.max(
    0,
    MAX_TOOL_OUTPUT_CHARS - (tool?.output?.length ?? 0),
  );
  const runRetained = activity.tools.reduce(
    (total, item) => total + (item.output?.length ?? 0),
    0,
  );
  const runRemaining = Math.max(0, MAX_RUN_TOOL_OUTPUT_CHARS - runRetained);
  return Math.min(requested, toolRemaining, runRemaining);
}

function runTextAllowance(
  activity: RunActivityView,
  requested: number,
): number {
  const retained =
    activity.assistantText.length + activity.reasoningText.length;
  return Math.min(
    requested,
    Math.max(0, MAX_RUN_TEXT_CHARS - retained),
  );
}

function projectReceipt(
  receipt:
    | Extract<
        RuntimeEventDto,
        { kind: "tool_call_completed" }
      >["payload"]["receipt"]
    | undefined,
): Pick<ToolActivity, "receipt" | "resources"> | Record<string, never> {
  if (!receipt) return {};
  const resource = receipt.resource
    ? projectResource(receipt.resource)
    : {
        resourceId: `receipt:${receipt.receipt_id}`,
        kind: "receipt" as const,
        label: receipt.label,
        truncated: false,
      };
  return {
    receipt: {
      receiptId: receipt.receipt_id,
      label: receipt.label,
      resource,
    },
    resources: [resource],
  };
}

function projectResource(
  resource: ResourceReferenceDto,
): ResourceReferenceView {
  const bounded = boundResource(resource);
  return {
    resourceId: bounded.resource_id,
    kind: bounded.kind,
    label: bounded.label,
    path: bounded.path,
    contentPreview: bounded.content_preview,
    truncated: bounded.truncated ?? false,
  };
}

function boundInputSummary(summary: ToolInputSummaryDto): ToolInputSummaryDto {
  if (summary.kind === "fs_list") {
    return {
      ...summary,
      path: boundedSingleLine(summary.path, MAX_REFERENCE_PATH_CHARS),
    };
  }
  if (summary.kind === "fs_read") {
    return {
      ...summary,
      path: boundedSingleLine(summary.path, MAX_REFERENCE_PATH_CHARS),
    };
  }
  return {
    ...summary,
    path: boundedSingleLine(summary.path, MAX_REFERENCE_PATH_CHARS),
    query_preview: boundedSingleLine(
      summary.query_preview,
      MAX_QUERY_PREVIEW_CHARS,
    ),
  };
}

function boundResource(resource: ResourceReferenceDto): ResourceReferenceDto {
  const content = resource.content_preview?.slice(
    0,
    MAX_RESOURCE_PREVIEW_CHARS,
  );
  return {
    ...resource,
    resource_id: resource.resource_id.slice(0, MAX_LABEL_CHARS),
    label: boundedSingleLine(resource.label, MAX_LABEL_CHARS),
    path: resource.path
      ? boundedSingleLine(resource.path, MAX_REFERENCE_PATH_CHARS)
      : undefined,
    content_preview: content,
    truncated:
      resource.truncated
      || (resource.content_preview?.length ?? 0) > (content?.length ?? 0),
  };
}

function boundedSingleLine(value: string, maxChars: number): string {
  return value.replace(/[\u0000-\u001f\u007f]/g, " ").slice(0, maxChars);
}

function closeActivity(
  activity: RunActivityView,
  mode: "completed" | "failed" | "cancelled",
): RunActivityView {
  const closed = closeOpenActivities(
    activity.plan,
    activity.tools,
    activity.subagents,
    mode,
  );
  return { ...activity, ...closed };
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

function upsertArtifact(
  artifacts: ArtifactActivity[],
  next: ArtifactActivity,
): ArtifactActivity[] {
  const index = artifacts.findIndex(
    (artifact) => artifact.artifactId === next.artifactId,
  );
  if (index < 0) return [...artifacts, next];
  const copy = [...artifacts];
  copy[index] = { ...copy[index]!, ...next };
  return copy;
}
