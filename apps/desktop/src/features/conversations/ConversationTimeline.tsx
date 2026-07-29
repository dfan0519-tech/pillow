/**
 * Conversation timeline surface.
 * Layout rhythm adapted from Grok App ConversationThread (MIT reference).
 * Streaming updates render inside the scroll region; composer stays fixed below.
 */

import { RunActivity } from "./RunActivity";
import type {
  ConversationTimelineView,
  RunView,
} from "./conversationReducer";

type ConversationTimelineProps = {
  timeline: ConversationTimelineView;
  run: RunView | null;
  showMockBadge?: boolean;
  mockBadgeLabel?: string;
};

export function ConversationTimeline({
  timeline,
  run,
  showMockBadge = false,
  mockBadgeLabel = "模拟运行时",
}: ConversationTimelineProps) {
  const userMessages = timeline.messages.filter((message) => message.role === "user");
  const assistantMessages = timeline.messages.filter(
    (message) => message.role === "assistant",
  );

  return (
    <div
      className="conversation-timeline"
      data-testid="conversation-timeline"
    >
      {showMockBadge ? (
        <div className="mock-runtime-badge" data-testid="mock-runtime-badge">
          {mockBadgeLabel}
        </div>
      ) : null}

      <div className="conversation-timeline__messages">
        {userMessages.map((message) => (
          <article
            className="timeline-msg timeline-msg--user"
            data-testid="timeline-user-message"
            key={message.id}
          >
            <header>你</header>
            <p>{message.content}</p>
          </article>
        ))}

        <RunActivity run={run} timeline={timeline} />

        {assistantMessages.map((message) => (
          <article
            className="timeline-msg timeline-msg--assistant"
            data-testid="timeline-assistant-message"
            key={message.id}
          >
            <header>助手</header>
            <p>{message.content}</p>
          </article>
        ))}
      </div>
    </div>
  );
}
