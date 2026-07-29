/**
 * Close open plan / tool / sub-agent activity when a run becomes terminal
 * or is restored as interrupted.
 */

import type {
  ConversationTimelineView,
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
  plan: ConversationTimelineView["plan"],
  tools: ToolActivity[],
  subagents: SubagentActivity[],
  mode: TerminalActivityMode,
): {
  plan: ConversationTimelineView["plan"];
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
  const activeRun = view.activeRunId ? runs[view.activeRunId] : null;
  if (!activeRun) return view;

  let mode: TerminalActivityMode | null = null;
  if (activeRun.status === "completed") mode = "completed";
  else if (activeRun.status === "failed") mode = "failed";
  else if (
    activeRun.status === "cancelled"
    || activeRun.status === "interrupted"
  ) {
    mode = "cancelled";
  }

  if (!mode) return view;

  const closed = closeOpenActivities(view.plan, view.tools, view.subagents, mode);
  return {
    ...view,
    plan: closed.plan,
    tools: closed.tools,
    subagents: closed.subagents,
  };
}
