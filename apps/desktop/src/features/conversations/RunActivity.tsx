/**
 * Concise, expandable activity projected for one durable run.
 */

import { useEffect, useRef } from "react";
import type { RunActivityView } from "./conversationReducer";

type RunActivityProps = {
  activity: RunActivityView;
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

const STEP_STATUS_LABEL: Record<string, string> = {
  pending: "待办",
  active: "进行中",
  completed: "完成",
  failed: "失败",
  cancelled: "已取消",
};

const ACTIVITY_STATUS_LABEL: Record<string, string> = {
  running: "运行中",
  completed: "已完成",
  failed: "失败",
  cancelled: "已取消",
};

export function RunActivity({ activity }: RunActivityProps) {
  const isActive =
    activity.status === "pending"
    || activity.status === "running"
    || activity.status === "cancelling";
  const statusLabel = STATUS_LABEL[activity.status] ?? activity.status;
  const summaryParts = activitySummaryParts(activity);
  const detailsRef = useRef<HTMLDetailsElement>(null);
  const initializedRef = useRef(false);
  const wasActiveRef = useRef(isActive);

  useEffect(() => {
    if (!detailsRef.current) return;
    if (!initializedRef.current) {
      detailsRef.current.open = isActive;
      initializedRef.current = true;
    } else if (wasActiveRef.current && !isActive) {
      detailsRef.current.open = false;
    }
    wasActiveRef.current = isActive;
  }, [isActive]);

  return (
    <section
      aria-label="运行活动"
      className="run-activity"
      data-run-id={activity.runId}
      data-testid="run-activity"
    >
      <details ref={detailsRef}>
        <summary>
          <span
            aria-hidden="true"
            className={
              isActive
                ? "run-activity__dot run-activity__dot--live"
                : "run-activity__dot"
            }
          />
          <span
            aria-live="polite"
            aria-atomic="true"
            className="run-activity__status"
            data-testid="run-status"
          >
            {statusLabel}
          </span>
          {summaryParts.length > 0 ? (
            <span className="run-activity__summary">
              {summaryParts.map((part) => (
                <span key={part}>{part}</span>
              ))}
            </span>
          ) : null}
          <time
            className="run-activity__time"
            dateTime={activity.updatedAt}
            title={formatFullTimestamp(activity.updatedAt)}
          >
            {formatActivityTime(activity.updatedAt)}
          </time>
        </summary>

        <div className="run-activity__content">
          {activity.reasoningText ? (
            <div
              className="run-activity__block"
              data-testid="run-activity-reasoning"
            >
              <h3>推理摘要</h3>
              <p>{activity.reasoningText}</p>
            </div>
          ) : null}

          {activity.plan ? (
            <div className="run-activity__block" data-testid="run-activity-plan">
              <h3>计划</h3>
              <ol>
                {activity.plan.steps.map((step) => (
                  <li
                    data-status={step.status}
                    data-testid="run-activity-plan-step"
                    key={step.id}
                  >
                    <span className="run-activity__item-title">{step.title}</span>
                    <span className="run-activity__item-meta">
                      {STEP_STATUS_LABEL[step.status] ?? step.status}
                    </span>
                  </li>
                ))}
              </ol>
            </div>
          ) : null}

          {activity.tools.length > 0 ? (
            <div className="run-activity__block" data-testid="run-activity-tool">
              <h3>工具</h3>
              <ul className="run-activity__list">
                {activity.tools.map((tool) => (
                  <li key={tool.toolCallId}>
                    <span className="run-activity__item-title">{tool.title}</span>
                    <span className="run-activity__item-meta">
                      {ACTIVITY_STATUS_LABEL[tool.status] ?? tool.status}
                      <time
                        dateTime={tool.updatedAt}
                        title={formatFullTimestamp(tool.updatedAt)}
                      >
                        {formatActivityTime(tool.updatedAt)}
                      </time>
                    </span>
                    {tool.output ? (
                      <pre className="run-activity__item-detail">{tool.output}</pre>
                    ) : null}
                  </li>
                ))}
              </ul>
            </div>
          ) : null}

          {activity.subagents.length > 0 ? (
            <div
              className="run-activity__block"
              data-testid="run-activity-subagent"
            >
              <h3>子代理</h3>
              <ul className="run-activity__list">
                {activity.subagents.map((agent) => (
                  <li key={agent.subagentId}>
                    <span className="run-activity__item-title">{agent.title}</span>
                    <span className="run-activity__item-meta">
                      {ACTIVITY_STATUS_LABEL[agent.status] ?? agent.status}
                      <time
                        dateTime={agent.updatedAt}
                        title={formatFullTimestamp(agent.updatedAt)}
                      >
                        {formatActivityTime(agent.updatedAt)}
                      </time>
                    </span>
                    {agent.progress ? (
                      <p className="run-activity__item-detail">
                        {agent.progress}
                      </p>
                    ) : null}
                  </li>
                ))}
              </ul>
            </div>
          ) : null}

          {activity.artifacts.length > 0 ? (
            <div
              className="run-activity__block"
              data-testid="run-activity-artifact"
            >
              <h3>产物</h3>
              <ul className="run-activity__list">
                {activity.artifacts.map((artifact) => (
                  <li key={artifact.artifactId}>
                    <span className="run-activity__item-title">
                      {artifact.path}
                    </span>
                    <span className="run-activity__item-meta">
                      {artifact.kind}
                    </span>
                  </li>
                ))}
              </ul>
            </div>
          ) : null}
        </div>
      </details>
    </section>
  );
}

function activitySummaryParts(activity: RunActivityView): string[] {
  const parts: string[] = [];
  if (activity.plan) parts.push(`${activity.plan.steps.length} 步`);
  if (activity.tools.length > 0) parts.push(`${activity.tools.length} 工具`);
  if (activity.subagents.length > 0) {
    parts.push(`${activity.subagents.length} 子代理`);
  }
  if (activity.artifacts.length > 0) {
    parts.push(`${activity.artifacts.length} 产物`);
  }
  return parts;
}

function formatActivityTime(value: string): string {
  const date = new Date(value);
  if (Number.isNaN(date.getTime())) return "";
  return new Intl.DateTimeFormat("zh-CN", {
    hour: "2-digit",
    minute: "2-digit",
    hour12: false,
  }).format(date);
}

function formatFullTimestamp(value: string): string {
  const date = new Date(value);
  if (Number.isNaN(date.getTime())) return value;
  return new Intl.DateTimeFormat("zh-CN", {
    dateStyle: "medium",
    timeStyle: "medium",
    hour12: false,
  }).format(date);
}
