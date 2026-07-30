/**
 * Concise, expandable activity projected for one durable run.
 */

import { useEffect, useId, useRef } from "react";
import { IconPanelRight } from "../../components/icons";
import type {
  ResourceReferenceView,
  RunActivityView,
} from "./conversationReducer";
import type { ToolInputSummaryDto } from "../../lib/conversationGateway";

type RunActivityProps = {
  activity: RunActivityView;
  onOpenResource?: (resource: ResourceReferenceView) => void;
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

export function RunActivity({
  activity,
  onOpenResource,
}: RunActivityProps) {
  const isActive =
    activity.status === "pending"
    || activity.status === "running"
    || activity.status === "cancelling";
  const statusLabel = STATUS_LABEL[activity.status] ?? activity.status;
  const summaryParts = activitySummaryParts(activity);
  const detailsRef = useRef<HTMLDetailsElement>(null);
  const initializedRef = useRef(false);
  const wasActiveRef = useRef(isActive);
  const headingId = useId();

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
      aria-labelledby={headingId}
      className="run-activity"
      data-run-id={activity.runId}
      data-testid="run-activity"
    >
      <h2 className="sr-only" id={headingId}>运行活动</h2>
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
          {activity.updatedAt ? (
            <time
              className="run-activity__time"
              dateTime={activity.updatedAt}
              title={formatFullTimestamp(activity.updatedAt)}
            >
              {formatActivityTime(activity.updatedAt)}
            </time>
          ) : (
            <span aria-hidden="true" />
          )}
        </summary>

        <div className="run-activity__content">
          {activity.assistantTextTruncated ? (
            <p
              className="run-activity__truncation"
              data-testid="run-activity-assistant-truncated"
            >
              回复已截断，省略 {activity.omittedAssistantChars ?? 0} 个字符
            </p>
          ) : null}

          {activity.reasoningText ? (
            <div
              className="run-activity__block"
              data-testid="run-activity-reasoning"
            >
              <h3>推理摘要</h3>
              <p>{activity.reasoningText}</p>
              {activity.reasoningTextTruncated ? (
                <p
                  className="run-activity__truncation"
                  data-testid="run-activity-reasoning-truncated"
                >
                  推理已截断，省略 {activity.omittedReasoningChars ?? 0} 个字符
                </p>
              ) : null}
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
                    {tool.inputSummary ? (
                      <p className="run-activity__item-detail">
                        输入 · {formatInputSummary(tool.inputSummary)}
                      </p>
                    ) : null}
                    {tool.output ? (
                      <pre
                        className="run-activity__item-detail"
                        tabIndex={0}
                      >
                        {tool.output}
                      </pre>
                    ) : null}
                    {tool.outputTruncated ? (
                      <p
                        className="run-activity__truncation"
                        data-testid="run-activity-output-truncated"
                      >
                        输出已截断，省略 {tool.omittedOutputChars ?? 0} 个字符
                      </p>
                    ) : null}
                    {(tool.resources ?? []).map((resource) => (
                      <ResourceButton
                        key={resource.resourceId}
                        onOpenResource={onOpenResource}
                        resource={resource}
                      />
                    ))}
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
                    {artifact.resource ? (
                      <ResourceButton
                        onOpenResource={onOpenResource}
                        resource={artifact.resource}
                      />
                    ) : null}
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

function ResourceButton({
  resource,
  onOpenResource,
}: {
  resource: ResourceReferenceView;
  onOpenResource?: (resource: ResourceReferenceView) => void;
}) {
  if (!onOpenResource) return null;
  return (
    <button
      aria-label={`打开 ${resource.label}`}
      className="run-activity__resource"
      onClick={() => onOpenResource(resource)}
      type="button"
    >
      <IconPanelRight size={13} />
      <span>{resource.label}</span>
    </button>
  );
}

function formatInputSummary(summary: ToolInputSummaryDto): string {
  if (summary.kind === "fs_list") {
    return `${summary.path} · 最多 ${summary.max_results} 项`;
  }
  if (summary.kind === "fs_read") {
    const end = summary.offset_bytes + summary.max_bytes;
    return `${summary.path} · ${summary.offset_bytes}–${end} 字节`;
  }
  const caseLabel = summary.case_sensitive ? "区分大小写" : "忽略大小写";
  return `${summary.path} · “${summary.query_preview}” · ${caseLabel} · 最多 ${summary.max_results} 项`;
}

function activitySummaryParts(activity: RunActivityView): string[] {
  const parts: string[] = [];
  if (activity.assistantTextTruncated) parts.push("回复已截断");
  if (activity.reasoningTextTruncated) parts.push("推理已截断");
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
