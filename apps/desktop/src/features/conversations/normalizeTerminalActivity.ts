/**
 * Close open plan / tool / sub-agent activity when a run becomes terminal
 * or is restored as interrupted.
 */

import type {
  ConversationTimelineView,
  PlanActivity,
  RunActivityView,
  RunView,
  SubagentActivity,
  ToolActivity,
} from "./conversationTypes";

export type TerminalActivityMode = "completed" | "failed" | "cancelled";

/**
 * Close pending/active plan steps and running tools/sub-agents so a terminal
 * (or interrupted) run never renders alongside "running" child activity.
 */
export function closeOpenActivities(
  plan: PlanActivity | null,
  tools: ToolActivity[],
  subagents: SubagentActivity[],
  mode: TerminalActivityMode,
): {
  plan: PlanActivity | null;
  tools: ToolActivity[];
  subagents: SubagentActivity[];
} {
  const stepStatus =
    mode === "completed" ? "completed" : mode === "failed" ? "failed" : "cancelled";
  const activityStatus =
    mode === "completed" ? "completed" : mode === "failed" ? "failed" : "cancelled";

  const nextPlan = plan
    ? {
        steps: plan.steps.map((step) =>
          step.status === "pending" || step.status === "active"
            ? { ...step, status: stepStatus }
            : step,
        ),
      }
    : null;

  return {
    plan: nextPlan,
    tools: tools.map((tool) =>
      tool.status === "running" ? { ...tool, status: activityStatus } : tool,
    ),
    subagents: subagents.map((item) =>
      item.status === "running" ? { ...item, status: activityStatus } : item,
    ),
  };
}

export function normalizeTimelineActivitiesForRuns(
  view: ConversationTimelineView,
  runs: Record<string, RunView>,
): ConversationTimelineView {
  const activitiesByRunId: Record<string, RunActivityView> = {};
  for (const [runId, activity] of Object.entries(view.activitiesByRunId)) {
    const run = runs[runId];
    if (!run) {
      activitiesByRunId[runId] = activity;
      continue;
    }
    const mode = terminalModeForStatus(run.status);
    const normalized = mode
      ? closeOpenActivities(
          activity.plan,
          activity.tools,
          activity.subagents,
          mode,
        )
      : {
          plan: activity.plan,
          tools: activity.tools,
          subagents: activity.subagents,
        };
    activitiesByRunId[runId] = {
      ...activity,
      ...normalized,
      status: run.status,
    };
  }

  const activeActivity = view.activeRunId
    ? activitiesByRunId[view.activeRunId]
    : null;
  return {
    ...view,
    activitiesByRunId,
    assistantText: activeActivity?.assistantText ?? "",
    reasoningText: activeActivity?.reasoningText ?? "",
    plan: activeActivity?.plan ?? null,
    tools: activeActivity?.tools ?? [],
    subagents: activeActivity?.subagents ?? [],
  };
}

function terminalModeForStatus(
  status: RunView["status"],
): TerminalActivityMode | null {
  if (status === "completed") return "completed";
  if (status === "failed") return "failed";
  if (status === "cancelled" || status === "interrupted") return "cancelled";
  return null;
}
