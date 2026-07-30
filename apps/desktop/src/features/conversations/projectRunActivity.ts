/**
 * Pure projection of one runtime event into its owning run activity.
 */

import type { RuntimeEventDto } from "../../lib/conversationGateway";
import type {
  ArtifactActivity,
  PlanStepView,
  RunActivityView,
  SubagentActivity,
  ToolActivity,
} from "./conversationTypes";
import { closeOpenActivities } from "./normalizeTerminalActivity";

export function emptyRunActivity(
  runId: string,
  createdAt: string,
): RunActivityView {
  return {
    runId,
    status: "pending",
    startedAt: createdAt,
    updatedAt: createdAt,
    assistantText: "",
    reasoningText: "",
    plan: null,
    tools: [],
    subagents: [],
    artifacts: [],
  };
}

export function projectRunActivity(
  current: RunActivityView,
  event: RuntimeEventDto,
): RunActivityView {
  let activity = { ...current, updatedAt: event.createdAt };

  if (event.kind === "run_started") {
    activity = {
      ...activity,
      promptPreview: event.payload.prompt_preview,
      status: "running",
    };
  } else if (event.kind === "assistant_delta") {
    activity = {
      ...activity,
      assistantText: `${activity.assistantText}${event.payload.text}`,
    };
  } else if (event.kind === "reasoning_summary_delta") {
    activity = {
      ...activity,
      reasoningText: `${activity.reasoningText}${event.payload.text}`,
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
      }),
    };
  } else if (event.kind === "tool_call_output") {
    activity = {
      ...activity,
      tools: activity.tools.map((tool) =>
        tool.toolCallId === event.payload.tool_call_id
          ? {
              ...tool,
              output: `${tool.output ?? ""}${event.payload.chunk}`,
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
    activity = {
      ...activity,
      artifacts: upsertArtifact(activity.artifacts, {
        artifactId: event.payload.artifact_id,
        path: event.payload.path,
        kind: event.payload.kind,
        createdAt: event.createdAt,
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
