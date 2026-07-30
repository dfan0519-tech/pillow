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
import { RunActivity } from "./RunActivity";
import type {
  ConversationTimelineView,
  ResourceReferenceView,
  RunActivityView,
  RunView,
} from "./conversationReducer";
import { isRunActive } from "./conversationReducer";
import type { TimelineMessageDto } from "../../lib/conversationGateway";

type ConversationTimelineProps = {
  timeline: ConversationTimelineView;
  run: RunView | null;
  showMockBadge?: boolean;
  mockBadgeLabel?: string;
  onOpenResource?: (resource: ResourceReferenceView) => void;
};

/** Histories above this count use explicit recent-page rendering. */
export const TIMELINE_WINDOW_THRESHOLD = 80;
export const TIMELINE_PAGE_SIZE = 40;
const STICK_BOTTOM_PX = 48;
const VIEWPORT_HEIGHT = 480;

type TimelineItem =
  | { type: "message"; message: TimelineMessageDto; sourceOrder: number }
  | { type: "activity"; activity: RunActivityView; sourceOrder: number };

export function ConversationTimeline({
  timeline,
  run,
  showMockBadge = false,
  mockBadgeLabel = "模拟运行时",
  onOpenResource,
}: ConversationTimelineProps) {
  const items = useMemo(() => buildChronologicalItems(timeline), [timeline]);
  const streaming = isRunActive(run);
  const usePagination = items.length > TIMELINE_WINDOW_THRESHOLD;

  return (
    <div
      className="conversation-timeline"
      data-testid="conversation-timeline"
      data-paginated={usePagination ? "true" : "false"}
      data-windowed="false"
    >
      {showMockBadge ? (
        <div className="mock-runtime-badge" data-testid="mock-runtime-badge">
          {mockBadgeLabel}
        </div>
      ) : null}

      <PaginatedItemList
        items={items}
        key={timeline.conversationId}
        onOpenResource={onOpenResource}
        run={run}
        streaming={streaming}
        timeline={timeline}
      />
    </div>
  );
}

function buildChronologicalItems(timeline: ConversationTimelineView): TimelineItem[] {
  const messages: TimelineItem[] = timeline.messages
    .filter((message) => message.role !== "system")
    .map((message, sourceOrder) => ({ type: "message", message, sourceOrder }));
  const activities: TimelineItem[] = Object.values(timeline.activitiesByRunId)
    .map((activity, index) => ({
      type: "activity",
      activity,
      sourceOrder: timeline.messages.length + index,
    }));

  return [...messages, ...activities].sort((left, right) => {
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
  return item.message.id;
}

function itemTimestamp(item: TimelineItem): string | null {
  return item.type === "activity"
    ? item.activity.startedAt
    : item.message.createdAt;
}

function itemPriority(item: TimelineItem): number {
  if (item.type === "activity") return 1;
  return item.message.role === "user" ? 0 : 2;
}

function TimelineItemView({
  item,
  onOpenResource,
  run,
  streaming,
}: {
  item: TimelineItem;
  onOpenResource?: (resource: ResourceReferenceView) => void;
  run: RunView | null;
  streaming: boolean;
}) {
  if (item.type === "activity") {
    return (
      <RunActivity
        activity={item.activity}
        onOpenResource={onOpenResource}
      />
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

function PaginatedItemList({
  items,
  onOpenResource,
  timeline,
  run,
  streaming,
}: {
  items: TimelineItem[];
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
          item={item}
          key={itemKey(item, visibleRange.start + offset)}
          onOpenResource={onOpenResource}
          run={run}
          streaming={streaming}
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
