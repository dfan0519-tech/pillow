/**
 * Conversation timeline surface with chronological turns and bounded rendering.
 * Layout rhythm adapted from Grok App ConversationThread (MIT reference).
 */

import {
  useLayoutEffect,
  useMemo,
  useRef,
  useState,
} from "react";
import { AgentActivityDisclosure } from "./AgentActivityDisclosure";
import {
  agentActivityFromRun,
  resolveResourceFromActivity,
} from "./agentActivityFromTimeline";
import {
  projectDispatchTimeline,
  type DispatchTimelineProjection,
} from "./agentActivityProjection";
import type {
  ConversationDispatchView,
  ConversationTimelineView,
} from "./conversationTypes";
import type {
  ResourceReferenceView,
  RunActivityView,
  RunView,
} from "./conversationReducer";
import { isRunActive } from "./conversationReducer";
import type { TimelineMessageDto } from "../../lib/conversationGateway";
import {
  buildProcessActivityModels,
  formatProcessArgvDisplay,
  type ProcessActivityModel,
} from "../execution/processActivityModel";
import { isShellProgram } from "../execution/executionTypes";
import {
  autoRuleDecisionLabel,
  formatRuleSummary,
} from "../permissions/permissionPattern";
import "../../styles/execution.css";
import "../../styles/permission-rules.css";
import "../../styles/dispatch-timeline.css";

export type OpenProcessOutputRequest = {
  /** Internal only — never painted in the timeline. */
  proposalId: string;
  programLabel: string;
  argv: string[];
};

type ConversationTimelineProps = {
  timeline: ConversationTimelineView;
  run: RunView | null;
  /**
   * Live dispatch overlay from the parent (propose + list + slot labels).
   * When omitted, `timeline.dispatches` is used. Never fetched here.
   */
  dispatches?: ConversationDispatchView[];
  onOpenResource?: (resource: ResourceReferenceView) => void;
  /**
   * Opens the existing Terminal output pane for a conversation-owned process.
   * Receives internal proposal identity plus safe program/argv only.
   */
  onOpenProcessOutput?: (request: OpenProcessOutputRequest) => void;
};

/** Histories above this count use explicit recent-page rendering. */
export const TIMELINE_WINDOW_THRESHOLD = 80;
export const TIMELINE_PAGE_SIZE = 40;
const STICK_BOTTOM_PX = 48;
const VIEWPORT_HEIGHT = 480;

type TimelineItem =
  | { type: "message"; message: TimelineMessageDto; sourceOrder: number }
  | { type: "activity"; activity: RunActivityView; sourceOrder: number }
  | { type: "dispatch"; dispatch: ConversationDispatchView; sourceOrder: number };

export function ConversationTimeline({
  timeline,
  run,
  dispatches,
  onOpenResource,
  onOpenProcessOutput,
}: ConversationTimelineProps) {
  const items = useMemo(
    () => buildChronologicalItems(timeline, dispatches),
    [timeline, dispatches],
  );
  const streaming = isRunActive(run);
  const usePagination = items.length > TIMELINE_WINDOW_THRESHOLD;

  return (
    <div
      className="conversation-timeline"
      data-testid="conversation-timeline"
      data-paginated={usePagination ? "true" : "false"}
      data-windowed="false"
    >
      <PaginatedItemList
        items={items}
        key={timeline.conversationId}
        onOpenProcessOutput={onOpenProcessOutput}
        onOpenResource={onOpenResource}
        run={run}
        streaming={streaming}
        timeline={timeline}
      />
    </div>
  );
}

function resolveDispatches(
  timeline: ConversationTimelineView,
  dispatches?: ConversationDispatchView[],
): ConversationDispatchView[] {
  if (dispatches !== undefined) return dispatches;
  return timeline.dispatches ?? [];
}

function buildChronologicalItems(
  timeline: ConversationTimelineView,
  extraDispatches?: ConversationDispatchView[],
): TimelineItem[] {
  const messages: TimelineItem[] = timeline.messages
    .filter((message) => message.role !== "system")
    .map((message, sourceOrder) => ({ type: "message", message, sourceOrder }));
  const activities: TimelineItem[] = Object.values(timeline.activitiesByRunId)
    .map((activity, index) => ({
      type: "activity",
      activity,
      sourceOrder: timeline.messages.length + index,
    }));
  const dispatchOffset = timeline.messages.length + activities.length;
  const dispatches: TimelineItem[] = resolveDispatches(
    timeline,
    extraDispatches,
  ).map((dispatch, index) => ({
    type: "dispatch",
    dispatch,
    sourceOrder: dispatchOffset + index,
  }));

  return [...messages, ...activities, ...dispatches].sort((left, right) => {
    const leftTime = itemTimestamp(left);
    const rightTime = itemTimestamp(right);
    if (leftTime && rightTime) {
      const byTime = leftTime.localeCompare(rightTime);
      if (byTime !== 0) return byTime;
      const byPriority = itemPriority(left) - itemPriority(right);
      if (byPriority !== 0) return byPriority;
    } else if (leftTime || rightTime) {
      return leftTime ? -1 : 1;
    }
    return left.sourceOrder - right.sourceOrder;
  });
}

function itemKey(item: TimelineItem, index: number): string {
  if (item.type === "activity") return `activity-${item.activity.runId}`;
  if (item.type === "dispatch") {
    const identity = item.dispatch.assignmentId.trim();
    return identity ? `dispatch-${identity}` : `dispatch-${index}`;
  }
  return item.message.id;
}

function itemTimestamp(item: TimelineItem): string | null {
  if (item.type === "activity") return item.activity.startedAt;
  if (item.type === "dispatch") return null;
  return item.message.createdAt;
}

function itemPriority(item: TimelineItem): number {
  if (item.type === "activity") return 1;
  if (item.type === "dispatch") return 1;
  return item.message.role === "user" ? 0 : 2;
}

function TimelineItemView({
  item,
  conversationId,
  onOpenProcessOutput,
  onOpenResource,
  run,
  streaming,
  timeline,
}: {
  item: TimelineItem;
  conversationId: string;
  onOpenProcessOutput?: (request: OpenProcessOutputRequest) => void;
  onOpenResource?: (resource: ResourceReferenceView) => void;
  run: RunView | null;
  streaming: boolean;
  timeline: ConversationTimelineView;
}) {
  if (item.type === "dispatch") {
    const processModels = buildProcessActivityModels(
      timeline.events,
      conversationId,
    );
    const proposalId = item.dispatch.proposalId;
    const process = proposalId
      ? processModels.find((model) => model.proposalId === proposalId)
      : undefined;
    const model = projectDispatchTimeline(
      item.dispatch,
      process
        ? { state: process.state, exitCode: process.exitCode }
        : null,
    );
    return <DispatchTimelineRow model={model} />;
  }

  if (item.type === "activity") {
    const record = agentActivityFromRun(
      conversationId,
      item.activity,
      timeline.events,
    );
    const processModels = buildProcessActivityModels(
      timeline.events,
      conversationId,
      item.activity.runId,
    );
    return (
      <>
        <AgentActivityDisclosure
          activity={record}
          conversationId={conversationId}
          onOpenArtifact={
            onOpenResource
              ? (relativePath, sourceToken) => {
                  const resource = resolveResourceFromActivity(
                    item.activity,
                    relativePath,
                    sourceToken,
                  );
                  if (resource) {
                    onOpenResource(resource);
                    return;
                  }
                  // Succeeded write outcomes open via portable path only.
                  if (!sourceToken) {
                    onOpenResource({
                      resourceId: `write:${relativePath}`,
                      kind: "file",
                      label: relativePath.split("/").pop() ?? relativePath,
                      path: relativePath,
                      truncated: false,
                    });
                  }
                }
              : undefined
          }
        />
        {processModels.map((model) => (
          <ProcessCommandActivityRow
            key={model.proposalId}
            model={model}
            onOpenOutput={
              model.canOpenOutput
                ? () => {
                    onOpenProcessOutput?.({
                      proposalId: model.proposalId,
                      programLabel: model.programLabel,
                      argv: model.argv,
                    });
                  }
                : undefined
            }
          />
        ))}
      </>
    );
  }

  const { message } = item;
  const isAssistant = message.role === "assistant";
  const isStreamingAssistant =
    isAssistant && streaming && run && message.id === `assistant-${run.runId}`;

  return (
    <article
      aria-live={isStreamingAssistant ? "polite" : undefined}
      aria-relevant={isStreamingAssistant ? "additions text" : undefined}
      className={
        isAssistant
          ? "timeline-msg timeline-msg--assistant"
          : "timeline-msg timeline-msg--user"
      }
      data-testid={
        isAssistant ? "timeline-assistant-message" : "timeline-user-message"
      }
      data-message-id={message.id}
    >
      <header>{isAssistant ? "助手" : "你"}</header>
      <p>{message.content}</p>
    </article>
  );
}

type VisibleRange = {
  start: number;
  end: number;
};

/**
 * Narrative dispatch row. Not a decision surface — no approve/reject.
 */
function DispatchTimelineRow({
  model,
}: {
  model: DispatchTimelineProjection;
}) {
  const hasTask = Boolean(model.taskText);
  return (
    <article
      className="dispatch-timeline-row"
      data-status={model.state ?? "unknown"}
      data-testid="dispatch-timeline-row"
    >
      <div className="dispatch-timeline-row__card">
        <div className="dispatch-timeline-row__header">
          <span
            aria-hidden="true"
            className={
              model.isLive
                ? "dispatch-timeline-row__dot dispatch-timeline-row__dot--live"
                : model.isFailureLike
                  ? "dispatch-timeline-row__dot dispatch-timeline-row__dot--risk"
                  : "dispatch-timeline-row__dot"
            }
          />
          <span
            className="dispatch-timeline-row__kind"
            data-testid="dispatch-timeline-kind"
          >
            派单
          </span>
          {model.initiatorLabel ? (
            <span
              className="dispatch-timeline-row__party"
              data-testid="dispatch-timeline-initiator"
            >
              从 {model.initiatorLabel}
            </span>
          ) : null}
          {model.targetRoleLabel ? (
            <span
              className="dispatch-timeline-row__party"
              data-testid="dispatch-timeline-target"
            >
              派给 {model.targetRoleLabel}
            </span>
          ) : null}
          {model.stateLabel ? (
            <span
              className="dispatch-timeline-row__status"
              data-testid="dispatch-timeline-status"
            >
              {model.stateLabel}
            </span>
          ) : null}
          {model.governanceLabel ? (
            <span className="dispatch-timeline-row__governance">
              {model.governanceLabel}
            </span>
          ) : null}
        </div>
        {hasTask ? (
          <details
            className="dispatch-timeline-row__task"
            data-testid="dispatch-timeline-task-details"
          >
            <summary
              className="dispatch-timeline-row__task-summary"
              data-testid="dispatch-timeline-task-summary"
            >
              任务 {model.taskPreview}
            </summary>
            <p
              className="dispatch-timeline-row__task-body"
              data-testid="dispatch-timeline-task"
            >
              {model.taskText}
            </p>
          </details>
        ) : null}
        {model.unavailableNotes.map((note) => (
          <p
            className="dispatch-timeline-row__note"
            data-testid="dispatch-timeline-unavailable"
            key={note}
            role="note"
          >
            {note}
          </p>
        ))}
      </div>
      <div
        aria-hidden="true"
        data-testid="dispatch-timeline-no-approval"
        hidden
      />
    </article>
  );
}

/**
 * Compact progressive-disclosure row for one process proposal.
 * Summary: program + lifecycle status. Details hold argv/meta/完整输出.
 * Reuses existing command-activity CSS; proposal id never enters the DOM.
 */
function ProcessCommandActivityRow({
  model,
  onOpenOutput,
}: {
  model: ProcessActivityModel;
  onOpenOutput?: () => void;
}) {
  const isFailureLike =
    model.state === "failed"
    || model.state === "rejected"
    || model.state === "expired"
    || model.state === "cancelled"
    || model.state === "interrupted";
  const isLive =
    model.state === "pending_approval"
    || model.state === "approved"
    || model.state === "running";
  const isShell = isShellProgram(model.programLabel);
  const argvText = formatProcessArgvDisplay(model.programLabel, model.argv);
  const hasDetails =
    model.argv.length > 0
    || model.decisionLabel
    || Boolean(model.autoApprovedByRule)
    || model.exitCode !== undefined
    || model.durationMs !== undefined
    || model.capturedBytes !== undefined
    || model.truncated
    || model.encodingLossy
    || Boolean(onOpenOutput);
  const autoRule = model.autoApprovedByRule;
  const autoRuleLabel = autoRule ? autoRuleDecisionLabel(autoRule) : null;
  const autoRuleSummary = autoRule ? formatRuleSummary(autoRule) : null;

  return (
    <article
      className="command-activity-row"
      data-command-kind={isShell ? "shell" : "structured"}
      data-origin="agent"
      data-status={model.state}
      data-testid="command-activity-row"
    >
      <details
        className={
          model.conciseSuccess
            ? "command-activity-row__details command-activity-row__details--concise"
            : "command-activity-row__details"
        }
        data-testid="command-activity-details"
      >
        <summary
          className="command-activity-row__summary"
          data-testid="command-activity-summary"
        >
          <span
            aria-hidden="true"
            className={
              isLive
                ? "command-activity-row__dot command-activity-row__dot--live"
                : isFailureLike
                  ? "command-activity-row__dot command-activity-row__dot--risk"
                  : "command-activity-row__dot"
            }
          />
          <span
            className="command-activity-row__origin"
            data-testid="command-activity-origin"
          >
            代理
          </span>
          <span
            className="command-activity-row__status"
            data-testid="command-activity-status"
          >
            {model.statusLabel}
          </span>
          {isShell ? (
            <span
              className="command-activity-row__kind"
              data-testid="command-activity-kind"
            >
              Shell 命令
            </span>
          ) : (
            <code
              className="command-activity-row__program"
              data-testid="command-activity-program"
            >
              {model.programLabel}
            </code>
          )}
          {autoRuleLabel ? (
            <span
              className={
                autoRule?.action === "deny"
                  ? "permission-auto-rule permission-auto-rule--deny"
                  : "permission-auto-rule"
              }
              data-testid="command-activity-auto-rule"
            >
              <span
                className="permission-auto-rule__label"
                data-testid="command-activity-auto-rule-label"
              >
                {autoRuleLabel}
              </span>
              {autoRuleSummary ? (
                <code
                  className="permission-auto-rule__summary"
                  data-testid="command-activity-auto-rule-summary"
                >
                  {autoRuleSummary}
                </code>
              ) : null}
            </span>
          ) : null}
        </summary>

        {hasDetails ? (
          <div className="command-activity-row__body">
            {model.argv.length > 0 ? (
              <code
                className="command-activity-row__argv"
                data-full-command={argvText}
                data-testid="command-activity-argv"
                title={argvText}
              >
                {argvText}
              </code>
            ) : null}
            {model.decisionLabel ? (
              <span
                className="command-activity-row__meta"
                data-testid="command-activity-decision"
              >
                决定：{model.decisionLabel}
              </span>
            ) : null}
            {autoRuleLabel && autoRuleSummary ? (
              <span
                className="command-activity-row__meta"
                data-testid="command-activity-auto-rule-detail"
              >
                规则：{autoRuleSummary}
              </span>
            ) : null}
            {model.exitCode !== undefined && model.exitCode !== null ? (
              <span
                className="command-activity-row__meta"
                data-testid="command-activity-exit"
              >
                退出 {model.exitCode}
              </span>
            ) : null}
            {model.durationMs !== undefined && model.durationMs !== null ? (
              <span
                className="command-activity-row__meta"
                data-testid="command-activity-duration"
              >
                {model.durationMs} ms
              </span>
            ) : null}
            {model.capturedBytes !== undefined && model.capturedBytes !== null ? (
              <span
                className="command-activity-row__meta"
                data-testid="command-activity-bytes"
              >
                已捕获 {model.capturedBytes} 字节
              </span>
            ) : null}
            {model.truncated ? (
              <p
                className="command-activity-row__notice"
                data-testid="command-activity-truncated"
                role="status"
              >
                输出已截断
              </p>
            ) : null}
            {model.encodingLossy ? (
              <p
                className="command-activity-row__notice"
                data-testid="command-activity-lossy"
                role="status"
              >
                编码有损
              </p>
            ) : null}
            {onOpenOutput ? (
              <button
                className="command-activity-row__open"
                data-testid="command-activity-open-output"
                onClick={onOpenOutput}
                type="button"
              >
                完整输出
              </button>
            ) : null}
          </div>
        ) : null}
      </details>
    </article>
  );
}

function PaginatedItemList({
  items,
  onOpenProcessOutput,
  onOpenResource,
  timeline,
  run,
  streaming,
}: {
  items: TimelineItem[];
  onOpenProcessOutput?: (request: OpenProcessOutputRequest) => void;
  onOpenResource?: (resource: ResourceReferenceView) => void;
  timeline: ConversationTimelineView;
  run: RunView | null;
  streaming: boolean;
}) {
  const scrollerRef = useRef<HTMLDivElement>(null);
  const [followBottom, setFollowBottom] = useState(true);
  const [frozenRange, setFrozenRange] = useState<VisibleRange>(() =>
    latestRange(items.length),
  );
  const prependHeightRef = useRef<number | null>(null);

  const visibleRange = followBottom
    ? latestRange(items.length)
    : clampRange(frozenRange, items.length);
  const slice = items.slice(visibleRange.start, visibleRange.end);
  const hasOlder = visibleRange.start > 0;
  const hasNewer = visibleRange.end < items.length;

  useLayoutEffect(() => {
    const el = scrollerRef.current;
    if (!el) return;
    if (prependHeightRef.current !== null) {
      el.scrollTop += el.scrollHeight - prependHeightRef.current;
      prependHeightRef.current = null;
      return;
    }
    if (followBottom) {
      el.scrollTop = Math.max(0, el.scrollHeight - el.clientHeight);
    }
  }, [
    items.length,
    streaming,
    timeline.assistantText,
    followBottom,
    visibleRange.start,
    visibleRange.end,
  ]);

  const loadOlder = () => {
    const el = scrollerRef.current;
    prependHeightRef.current = el?.scrollHeight ?? null;
    setFollowBottom(false);
    setFrozenRange({
      start: Math.max(0, visibleRange.start - TIMELINE_PAGE_SIZE),
      end: visibleRange.end,
    });
  };

  return (
    <div
      className="conversation-timeline__messages conversation-timeline__messages--paginated"
      data-follow-bottom={followBottom ? "true" : "false"}
      data-testid="timeline-page"
      onScroll={(event) => {
        const el = event.currentTarget;
        const distanceFromBottom =
          el.scrollHeight - el.scrollTop - el.clientHeight;
        const nextFollowBottom = distanceFromBottom <= STICK_BOTTOM_PX;
        if (!nextFollowBottom && followBottom) {
          setFrozenRange(visibleRange);
        }
        setFollowBottom(nextFollowBottom);
      }}
      ref={scrollerRef}
      style={{ maxHeight: VIEWPORT_HEIGHT, overflowY: "auto" }}
    >
      {hasOlder ? (
        <button
          className="conversation-timeline__load-older"
          onClick={loadOlder}
          type="button"
        >
          加载更早消息
        </button>
      ) : null}
      {slice.map((item, offset) => (
        <TimelineItemView
          conversationId={timeline.conversationId}
          item={item}
          key={itemKey(item, visibleRange.start + offset)}
          onOpenProcessOutput={onOpenProcessOutput}
          onOpenResource={onOpenResource}
          run={run}
          streaming={streaming}
          timeline={timeline}
        />
      ))}
      {hasNewer ? (
        <button
          className="conversation-timeline__load-older"
          onClick={() => {
            setFrozenRange(latestRange(items.length));
            setFollowBottom(true);
          }}
          type="button"
        >
          跳到最新消息
        </button>
      ) : null}
    </div>
  );
}

function latestRange(length: number): VisibleRange {
  if (length <= TIMELINE_WINDOW_THRESHOLD) {
    return { start: 0, end: length };
  }
  return {
    start: Math.max(0, length - TIMELINE_PAGE_SIZE),
    end: length,
  };
}

function clampRange(range: VisibleRange, length: number): VisibleRange {
  const end = Math.min(range.end, length);
  return {
    start: Math.min(range.start, end),
    end,
  };
}
