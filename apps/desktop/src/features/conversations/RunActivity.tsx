/**
 * Plan / tool / sub-agent activity inside the owning conversation.
 * Rhythm adapted from Grok App AgentActivity + TurnActivityBlock + PlanStatusBar
 * (read-only reference at <workspace>/VaultSystem/references/grok-app).
 */

import type {
  ConversationTimelineView,
  RunView,
} from "./conversationReducer";
import { isRunActive } from "./conversationReducer";

type RunActivityProps = {
  timeline: ConversationTimelineView;
  run: RunView | null;
};

const STATUS_LABEL: Record<string, string> = {
  pending: "等待中",
  running: "运行中",
  cancelling: "取消中",
  completed: "已完成",
  failed: "失败",
  cancelled: "已取消",
  interrupted: "已中断",
};

export function RunActivity({ timeline, run }: RunActivityProps) {
  const hasActivity =
    timeline.plan
    || timeline.tools.length > 0
    || timeline.subagents.length > 0
    || timeline.reasoningText
    || run;

  if (!hasActivity) return null;

  return (
    <section
      aria-label="运行活动"
      className="run-activity"
      data-testid="run-activity"
    >
      {run ? (
        <div className="run-activity__status" data-testid="run-status">
          <span
            className={
              isRunActive(run)
                ? "run-activity__dot run-activity__dot--live"
                : "run-activity__dot"
            }
          />
          <span>{STATUS_LABEL[run.status] ?? run.status}</span>
        </div>
      ) : null}

      {timeline.reasoningText ? (
        <div className="run-activity__block" data-testid="run-activity-reasoning">
          <h3>推理摘要</h3>
          <p>{timeline.reasoningText}</p>
        </div>
      ) : null}

      {timeline.plan ? (
        <div className="run-activity__block" data-testid="run-activity-plan">
          <h3>{timeline.plan.title}</h3>
          <ol>
            {timeline.plan.steps.map((step) => (
              <li key={step}>{step}</li>
            ))}
          </ol>
        </div>
      ) : null}

      {timeline.tools.length > 0 ? (
        <div className="run-activity__block" data-testid="run-activity-tool">
          <h3>工具</h3>
          <ul className="run-activity__list">
            {timeline.tools.map((tool) => (
              <li key={tool.toolCallId}>
                <span className="run-activity__item-title">{tool.title}</span>
                <span className="run-activity__item-meta">{tool.status}</span>
                {tool.output ? (
                  <p className="run-activity__item-detail">{tool.output}</p>
                ) : null}
              </li>
            ))}
          </ul>
        </div>
      ) : null}

      {timeline.subagents.length > 0 ? (
        <div className="run-activity__block" data-testid="run-activity-subagent">
          <h3>子代理</h3>
          <ul className="run-activity__list">
            {timeline.subagents.map((agent) => (
              <li key={agent.subagentId}>
                <span className="run-activity__item-title">{agent.title}</span>
                <span className="run-activity__item-meta">{agent.status}</span>
                {agent.progress ? (
                  <p className="run-activity__item-detail">{agent.progress}</p>
                ) : null}
              </li>
            ))}
          </ul>
        </div>
      ) : null}
    </section>
  );
}
