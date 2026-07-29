/**
 * Reducer / projection regression tests for Wave B1 UI fix pass.
 */
import { describe, expect, it } from "vitest";
import { materializeCanonicalEvents } from "../src/lib/canonicalTurnFixture";
import type { RuntimeEventDto, TimelineDto } from "../src/lib/conversationGateway";
import {
  conversationReducer,
  createInitialConversationState,
  type ConversationAction,
  type ConversationUiState,
} from "../src/features/conversations/conversationReducer";

function dispatch(
  state: ConversationUiState,
  action: ConversationAction,
): ConversationUiState {
  return conversationReducer(state, action);
}

function baseEvent(
  overrides: Partial<RuntimeEventDto> & Pick<RuntimeEventDto, "kind" | "payload">,
): RuntimeEventDto {
  return {
    eventId: overrides.eventId ?? "e",
    runId: overrides.runId ?? "r1",
    conversationId: overrides.conversationId ?? "c1",
    sequence: overrides.sequence ?? 1,
    createdAt: overrides.createdAt ?? "2026-07-30T00:00:01.000Z",
    kind: overrides.kind,
    payload: overrides.payload,
  } as RuntimeEventDto;
}

describe("event dedupe by (runId, sequence)", () => {
  it("allows two runs that both start at sequence 1", () => {
    let state = createInitialConversationState();
    state = dispatch(state, {
      type: "timeline_loaded",
      timeline: { conversationId: "c1", messages: [], events: [], runs: [] },
    });

    state = dispatch(state, {
      type: "runtime_event",
      event: baseEvent({
        eventId: "e-r1-1",
        runId: "run-a",
        sequence: 1,
        kind: "run_started",
        payload: { prompt_preview: "first" },
      }),
    });
    state = dispatch(state, {
      type: "runtime_event",
      event: baseEvent({
        eventId: "e-r1-2",
        runId: "run-a",
        sequence: 2,
        kind: "assistant_delta",
        payload: { text: "Answer A" },
        createdAt: "2026-07-30T00:00:02.000Z",
      }),
    });
    state = dispatch(state, {
      type: "runtime_event",
      event: baseEvent({
        eventId: "e-r1-3",
        runId: "run-a",
        sequence: 3,
        kind: "turn_completed",
        payload: {},
        createdAt: "2026-07-30T00:00:03.000Z",
      }),
    });

    // Second turn reuses sequence 1 under a new runId — must not be dropped.
    state = dispatch(state, {
      type: "runtime_event",
      event: baseEvent({
        eventId: "e-r2-1",
        runId: "run-b",
        sequence: 1,
        kind: "run_started",
        payload: { prompt_preview: "second" },
        createdAt: "2026-07-30T00:00:04.000Z",
      }),
    });
    state = dispatch(state, {
      type: "runtime_event",
      event: baseEvent({
        eventId: "e-r2-2",
        runId: "run-b",
        sequence: 2,
        kind: "assistant_delta",
        payload: { text: "Answer B" },
        createdAt: "2026-07-30T00:00:05.000Z",
      }),
    });

    // Duplicate (run-b, sequence 2) ignored.
    state = dispatch(state, {
      type: "runtime_event",
      event: baseEvent({
        eventId: "e-r2-2-dup",
        runId: "run-b",
        sequence: 2,
        kind: "assistant_delta",
        payload: { text: "DUPLICATE" },
        createdAt: "2026-07-30T00:00:06.000Z",
      }),
    });

    const timeline = state.timelines.c1;
    expect(timeline?.events).toHaveLength(5);
    expect(timeline?.assistantText).toBe("Answer B");
    expect(timeline?.assistantText).not.toContain("DUPLICATE");
    expect(timeline?.assistantText).not.toContain("Answer A");
    const assistants = timeline?.messages.filter((m) => m.role === "assistant") ?? [];
    expect(assistants).toHaveLength(2);
    expect(assistants[0]?.content).toBe("Answer A");
    expect(assistants[1]?.content).toBe("Answer B");
  });
});

describe("canonical Task 2 fixture projection", () => {
  it("projects structured plan steps, tool chunks, and sub-agent events", () => {
    let state = createInitialConversationState();
    state = dispatch(state, {
      type: "timeline_loaded",
      timeline: { conversationId: "c1", messages: [], events: [], runs: [] },
    });

    const events = materializeCanonicalEvents("c1", "run-canonical");
    for (const event of events) {
      state = dispatch(state, { type: "runtime_event", event });
    }

    const timeline = state.timelines.c1;
    expect(timeline?.plan?.steps).toEqual([
      { id: "s1", title: "Scan files", status: "completed" },
      { id: "s2", title: "Write summary", status: "completed" },
    ]);
    expect(timeline?.tools[0]?.toolCallId).toBe("tc-list");
    expect(timeline?.tools[0]?.output).toContain("crates/");
    expect(timeline?.tools[0]?.status).toBe("completed");
    expect(timeline?.subagents[0]?.subagentId).toBe("sa-explore");
    expect(timeline?.subagents[0]?.title).toBe("explore");
    expect(timeline?.subagents[0]?.progress).toContain("indexed 3");
    expect(timeline?.assistantText).toBe(
      "Workspace contains apps, crates, and docs. Ready for deeper work.",
    );
    expect(timeline?.reasoningText).toBe("Inspecting project layout.");
    expect(state.runs["run-canonical"]?.status).toBe("completed");
  });
});

describe("timeline load merge and run restoration", () => {
  it("merges in-flight subscribed deltas instead of overwriting them", () => {
    let state = createInitialConversationState();
    state = dispatch(state, {
      type: "timeline_loaded",
      timeline: { conversationId: "c1", messages: [], events: [], runs: [] },
    });

    // Live delta arrives while load is in flight.
    state = dispatch(state, {
      type: "runtime_event",
      event: baseEvent({
        eventId: "live-2",
        runId: "run-x",
        sequence: 2,
        kind: "assistant_delta",
        payload: { text: "from-live" },
        createdAt: "t2",
      }),
    });

    const snapshot: TimelineDto = {
      conversationId: "c1",
      messages: [
        {
          id: "u1",
          role: "user",
          content: "hello",
          createdAt: "t0",
        },
      ],
      events: [
        baseEvent({
          eventId: "snap-1",
          runId: "run-x",
          sequence: 1,
          kind: "run_started",
          payload: {},
          createdAt: "t1",
        }),
      ],
      runs: [
        { runId: "run-x", conversationId: "c1", status: "running" },
      ],
    };

    state = dispatch(state, { type: "timeline_loaded", timeline: snapshot });

    const timeline = state.timelines.c1;
    expect(timeline?.messages.some((m) => m.role === "user")).toBe(true);
    expect(timeline?.assistantText).toContain("from-live");
    expect(timeline?.events.map((e) => e.sequence).sort()).toEqual([1, 2]);
  });

  it("restores pending, cancelling, and interrupted runs from TimelineDto", () => {
    const timeline: TimelineDto = {
      conversationId: "c1",
      messages: [],
      events: [],
      runs: [
        { runId: "r-pending", conversationId: "c1", status: "pending" },
        { runId: "r-cancelling", conversationId: "c1", status: "cancelling" },
        { runId: "r-interrupted", conversationId: "c1", status: "interrupted" },
      ],
    };

    const state = dispatch(createInitialConversationState(), {
      type: "timeline_loaded",
      timeline,
    });

    expect(state.runs["r-pending"]?.status).toBe("pending");
    expect(state.runs["r-cancelling"]?.status).toBe("cancelling");
    expect(state.runs["r-interrupted"]?.status).toBe("interrupted");
  });
});

describe("multi-turn chronological messages", () => {
  it("does not carry previous assistant text into a new run", () => {
    let state = createInitialConversationState();
    state = dispatch(state, {
      type: "user_message_appended",
      conversationId: "c1",
      message: {
        id: "u1",
        role: "user",
        content: "first",
        createdAt: "t1",
      },
    });
    state = dispatch(state, {
      type: "runtime_event",
      event: baseEvent({
        runId: "run-1",
        sequence: 1,
        kind: "run_started",
        payload: {},
        createdAt: "t2",
      }),
    });
    state = dispatch(state, {
      type: "runtime_event",
      event: baseEvent({
        runId: "run-1",
        sequence: 2,
        kind: "assistant_delta",
        payload: { text: "First answer" },
        createdAt: "t3",
      }),
    });
    state = dispatch(state, {
      type: "user_message_appended",
      conversationId: "c1",
      message: {
        id: "u2",
        role: "user",
        content: "second",
        createdAt: "t4",
      },
    });
    state = dispatch(state, {
      type: "runtime_event",
      event: baseEvent({
        runId: "run-2",
        sequence: 1,
        kind: "run_started",
        payload: {},
        createdAt: "t5",
      }),
    });
    state = dispatch(state, {
      type: "runtime_event",
      event: baseEvent({
        runId: "run-2",
        sequence: 2,
        kind: "assistant_delta",
        payload: { text: "Second answer" },
        createdAt: "t6",
      }),
    });

    const messages = state.timelines.c1?.messages ?? [];
    expect(messages.map((m) => `${m.role}:${m.content}`)).toEqual([
      "user:first",
      "assistant:First answer",
      "user:second",
      "assistant:Second answer",
    ]);
    expect(state.timelines.c1?.assistantText).toBe("Second answer");
  });
});
