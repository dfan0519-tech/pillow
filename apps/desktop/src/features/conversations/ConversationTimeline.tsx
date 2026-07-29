/**
 * Conversation timeline surface with chronological turns and bounded rendering.
 * Layout rhythm adapted from Grok App ConversationThread (MIT reference).
 */

import {
  useCallback,
  useLayoutEffect,
  useMemo,
  useRef,
  useState,
  type UIEvent,
} from "react";
import { RunActivity } from "./RunActivity";
import type {
  ConversationTimelineView,
  RunView,
} from "./conversationReducer";
import { isRunActive } from "./conversationReducer";
import type { TimelineMessageDto } from "../../lib/conversationGateway";

type ConversationTimelineProps = {
  timeline: ConversationTimelineView;
  run: RunView | null;
  showMockBadge?: boolean;
  mockBadgeLabel?: string;
};

/** Messages above this count use windowed rendering (Stage 1 scale rule). */
export const TIMELINE_WINDOW_THRESHOLD = 80;
/** Visible message capacity for the virtual window (plus small overscan). */
export const TIMELINE_WINDOW_SIZE = 40;
const ESTIMATED_ITEM_HEIGHT = 88;
const OVERSCAN = 6;
const STICK_BOTTOM_PX = 48;
const VIEWPORT_HEIGHT = 480;

type TimelineItem =
  | { type: "message"; message: TimelineMessageDto }
  | { type: "activity" };

export function ConversationTimeline({
  timeline,
  run,
  showMockBadge = false,
  mockBadgeLabel = "模拟运行时",
}: ConversationTimelineProps) {
  const items = useMemo(() => buildChronologicalItems(timeline), [timeline]);
  const streaming = isRunActive(run);
  const useWindow = items.length > TIMELINE_WINDOW_THRESHOLD;

  return (
    <div
      className="conversation-timeline"
      data-testid="conversation-timeline"
      data-windowed={useWindow ? "true" : "false"}
    >
      {showMockBadge ? (
        <div className="mock-runtime-badge" data-testid="mock-runtime-badge">
          {mockBadgeLabel}
        </div>
      ) : null}

      {useWindow ? (
        <WindowedItemList
          items={items}
          run={run}
          streaming={streaming}
          timeline={timeline}
        />
      ) : (
        <div className="conversation-timeline__messages">
          {items.map((item, index) => (
            <TimelineItemView
              item={item}
              key={itemKey(item, index)}
              run={run}
              streaming={streaming}
              timeline={timeline}
            />
          ))}
        </div>
      )}
    </div>
  );
}

function buildChronologicalItems(timeline: ConversationTimelineView): TimelineItem[] {
  const items: TimelineItem[] = [];
  for (const message of timeline.messages) {
    if (message.role === "system") continue;
    items.push({ type: "message", message });
  }
  // Activity block after the latest user message when present; else at end.
  if (
    timeline.plan
    || timeline.tools.length > 0
    || timeline.subagents.length > 0
    || timeline.reasoningText
    || timeline.activeRunId
  ) {
    const lastUserIndex = [...items]
      .map((item, index) => ({ item, index }))
      .reverse()
      .find(({ item }) => item.type === "message" && item.message.role === "user")
      ?.index;
    const insertAt =
      lastUserIndex === undefined ? items.length : lastUserIndex + 1;
    let at = insertAt;
    while (
      at < items.length
      && items[at]?.type === "message"
      && (items[at] as { type: "message"; message: TimelineMessageDto }).message.role
        === "assistant"
    ) {
      break;
    }
    items.splice(at, 0, { type: "activity" });
  }
  return items;
}

function itemKey(item: TimelineItem, index: number): string {
  if (item.type === "activity") return `activity-${index}`;
  return item.message.id;
}

function TimelineItemView({
  item,
  timeline,
  run,
  streaming,
}: {
  item: TimelineItem;
  timeline: ConversationTimelineView;
  run: RunView | null;
  streaming: boolean;
}) {
  if (item.type === "activity") {
    return <RunActivity run={run} timeline={timeline} />;
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

function WindowedItemList({
  items,
  timeline,
  run,
  streaming,
}: {
  items: TimelineItem[];
  timeline: ConversationTimelineView;
  run: RunView | null;
  streaming: boolean;
}) {
  const scrollerRef = useRef<HTMLDivElement>(null);
  // Anchor to newest content by default (initial load + streaming follow).
  // jsdom lacks reliable scroll metrics, so the window is driven by this flag
  // rather than scrollTop alone.
  const [followBottom, setFollowBottom] = useState(true);
  const [scrollTop, setScrollTop] = useState(0);
  const totalHeight = items.length * ESTIMATED_ITEM_HEIGHT;
  const estimatedMaxScroll = Math.max(0, totalHeight - VIEWPORT_HEIGHT);
  const visibleCount =
    Math.ceil(VIEWPORT_HEIGHT / ESTIMATED_ITEM_HEIGHT) + OVERSCAN * 2;
  const windowSize = Math.max(visibleCount, TIMELINE_WINDOW_SIZE);

  const onScroll = useCallback(
    (event: UIEvent<HTMLDivElement>) => {
      const el = event.currentTarget;
      const nextTop = el.scrollTop;
      setScrollTop(nextTop);

      // Prefer live DOM metrics when the scroller actually overflows;
      // fall back to estimated height so tests/jsdom still work.
      const domOverflows = el.scrollHeight > el.clientHeight + 1;
      const distanceFromBottom = domOverflows
        ? el.scrollHeight - el.scrollTop - el.clientHeight
        : estimatedMaxScroll - nextTop;
      setFollowBottom(distanceFromBottom <= STICK_BOTTOM_PX);
    },
    [estimatedMaxScroll],
  );

  useLayoutEffect(() => {
    const el = scrollerRef.current;
    if (!el || !followBottom) return;
    const target = Math.max(0, el.scrollHeight - el.clientHeight);
    if (Math.abs(el.scrollTop - target) > 1) {
      el.scrollTop = target;
    }
    setScrollTop(target > 0 ? target : estimatedMaxScroll);
  }, [
    items.length,
    streaming,
    timeline.assistantText,
    followBottom,
    estimatedMaxScroll,
  ]);

  let startIndex: number;
  let endIndex: number;
  if (followBottom) {
    endIndex = items.length;
    startIndex = Math.max(0, endIndex - windowSize - OVERSCAN);
  } else {
    startIndex = Math.max(
      0,
      Math.floor(scrollTop / ESTIMATED_ITEM_HEIGHT) - OVERSCAN,
    );
    endIndex = Math.min(items.length, startIndex + windowSize);
  }
  const slice = items.slice(startIndex, endIndex);
  const padTop = startIndex * ESTIMATED_ITEM_HEIGHT;
  const padBottom = Math.max(0, totalHeight - endIndex * ESTIMATED_ITEM_HEIGHT);

  return (
    <div
      className="conversation-timeline__messages conversation-timeline__messages--windowed"
      data-follow-bottom={followBottom ? "true" : "false"}
      data-testid="timeline-window"
      onScroll={onScroll}
      ref={scrollerRef}
      style={{ maxHeight: VIEWPORT_HEIGHT, overflowY: "auto" }}
    >
      <div style={{ height: padTop }} aria-hidden="true" data-testid="timeline-pad-top" />
      {slice.map((item, offset) => (
        <TimelineItemView
          item={item}
          key={itemKey(item, startIndex + offset)}
          run={run}
          streaming={streaming}
          timeline={timeline}
        />
      ))}
      <div
        style={{ height: padBottom }}
        aria-hidden="true"
        data-testid="timeline-pad-bottom"
      />
    </div>
  );
}
