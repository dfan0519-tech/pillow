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

  it("does not append duplicate tool output within a retained run", () => {
    let state = createInitialConversationState();
    const events = [
      baseEvent({
        eventId: "r1-start",
        runId: "run-a",
        sequence: 1,
        kind: "run_started",
        payload: {},
      }),
      baseEvent({
        eventId: "r1-tool",
        runId: "run-a",
        sequence: 2,
        kind: "tool_call_started",
        payload: { tool_call_id: "tool-a", name: "fs.read", summary: "Read notes" },
        createdAt: "2026-07-30T00:00:02.000Z",
      }),
      baseEvent({
        eventId: "r1-output",
        runId: "run-a",
        sequence: 3,
        kind: "tool_call_output",
        payload: { tool_call_id: "tool-a", chunk: "bounded result" },
        createdAt: "2026-07-30T00:00:03.000Z",
      }),
    ];

    for (const event of [...events, events[2]!]) {
      state = dispatch(state, { type: "runtime_event", event });
    }

    const timeline = state.timelines.c1 as unknown as {
      activitiesByRunId?: Record<string, { tools: Array<{ output?: string }> }>;
    };
    expect(timeline.activitiesByRunId?.["run-a"]?.tools[0]?.output).toBe(
      "bounded result",
    );
  });

  it("bounds retained output per tool and across the owning run", () => {
    let state = createInitialConversationState();
    let sequence = 1;
    state = dispatch(state, {
      type: "runtime_event",
      event: baseEvent({
        runId: "run-bounded",
        sequence: sequence++,
        kind: "run_started",
        payload: {},
      }),
    });

    for (let index = 0; index < 4; index += 1) {
      const toolCallId = `tool-${index}`;
      state = dispatch(state, {
        type: "runtime_event",
        event: baseEvent({
          runId: "run-bounded",
          sequence: sequence++,
          kind: "tool_call_started",
          payload: {
            tool_call_id: toolCallId,
            name: "fs.read",
            summary: `Read ${index}`,
          },
        }),
      });
      state = dispatch(state, {
        type: "runtime_event",
        event: baseEvent({
          runId: "run-bounded",
          sequence: sequence++,
          kind: "tool_call_output",
          payload: { tool_call_id: toolCallId, chunk: "x".repeat(20_000) },
        }),
      });
    }

    const activity = state.timelines.c1?.activitiesByRunId["run-bounded"];
    const retainedChars =
      activity?.tools.reduce((total, tool) => total + (tool.output?.length ?? 0), 0)
      ?? 0;
    const retainedEventChars =
      state.timelines.c1?.events.reduce(
        (total, event) =>
          total
          + (event.kind === "tool_call_output" ? event.payload.chunk.length : 0),
        0,
      )
      ?? 0;

    expect(activity?.tools[0]?.output).toHaveLength(16_384);
    expect(activity?.tools[0]).toMatchObject({
      outputTruncated: true,
      omittedOutputChars: 3_616,
    });
    expect(retainedChars).toBeLessThanOrEqual(49_152);
    expect(retainedEventChars).toBeLessThanOrEqual(49_152);
    expect(activity?.tools[3]).toMatchObject({
      outputTruncated: true,
      omittedOutputChars: 20_000,
    });

    const snapshotEvents =
      state.timelines.c1?.events.filter(
        (event) => event.kind !== "tool_call_output",
      )
      ?? [];
    state = dispatch(state, {
      type: "timeline_loaded",
      timeline: {
        conversationId: "c1",
        messages: [],
        events: snapshotEvents,
        runs: [
          {
            runId: "run-bounded",
            conversationId: "c1",
            status: "running",
          },
        ],
      },
    });

    const reloaded =
      state.timelines.c1?.activitiesByRunId["run-bounded"]?.tools
      ?? [];
    expect(
      reloaded.reduce(
        (total, tool) => total + (tool.output?.length ?? 0),
        0,
      ),
    ).toBe(49_152);
    expect(reloaded[0]).toMatchObject({
      outputTruncated: true,
      omittedOutputChars: 3_616,
    });
  });

  it("bounds retained assistant and reasoning output across the owning run", () => {
    let state = createInitialConversationState();
    const events = [
      baseEvent({
        runId: "run-text-bounded",
        sequence: 1,
        kind: "run_started",
        payload: {},
      }),
      baseEvent({
        runId: "run-text-bounded",
        sequence: 2,
        kind: "assistant_delta",
        payload: { text: "a".repeat(50_000) },
      }),
      baseEvent({
        runId: "run-text-bounded",
        sequence: 3,
        kind: "reasoning_summary_delta",
        payload: { text: "r".repeat(30_000) },
      }),
    ];
    for (const event of events) {
      state = dispatch(state, { type: "runtime_event", event });
    }

    const activity =
      state.timelines.c1?.activitiesByRunId["run-text-bounded"];
    const retainedTextChars =
      (activity?.assistantText.length ?? 0)
      + (activity?.reasoningText.length ?? 0);
    const retainedEventChars =
      state.timelines.c1?.events.reduce((total, event) => {
        if (
          event.kind === "assistant_delta"
          || event.kind === "reasoning_summary_delta"
        ) {
          return total + event.payload.text.length;
        }
        return total;
      }, 0)
      ?? 0;

    expect(retainedTextChars).toBe(65_536);
    expect(retainedEventChars).toBe(65_536);
    expect(activity).toMatchObject({
      assistantTextTruncated: false,
      omittedAssistantChars: 0,
      reasoningTextTruncated: true,
      omittedReasoningChars: 14_464,
    });

    const snapshot = state.timelines.c1;
    state = dispatch(state, {
      type: "timeline_loaded",
      timeline: {
        conversationId: "c1",
        messages: [],
        events: snapshot?.events ?? [],
        runs: [
          {
            runId: "run-text-bounded",
            conversationId: "c1",
            status: "running",
          },
        ],
      },
    });
    expect(
      state.timelines.c1?.activitiesByRunId["run-text-bounded"],
    ).toMatchObject({
      assistantTextTruncated: false,
      omittedAssistantChars: 0,
      reasoningTextTruncated: true,
      omittedReasoningChars: 14_464,
    });
  });

  it("bounds readable receipt previews before retaining renderer events", () => {
    let state = createInitialConversationState();
    const events = [
      baseEvent({
        runId: "run-preview",
        sequence: 1,
        kind: "tool_call_started",
        payload: {
          tool_call_id: "tool-preview",
          name: "fs.read",
          summary: "Read preview",
        },
      }),
      baseEvent({
        runId: "run-preview",
        sequence: 2,
        kind: "tool_call_completed",
        payload: {
          tool_call_id: "tool-preview",
          status: "completed",
          receipt: {
            receipt_id: "receipt-preview",
            label: "Preview receipt",
            resource: {
              resource_id: "resource-preview",
              kind: "file",
              label: "large.txt",
              path: "large.txt",
              content_preview: "z".repeat(20_000),
            },
          },
        },
      }),
    ];
    for (const event of events) {
      state = dispatch(state, { type: "runtime_event", event });
    }

    const resource =
      state.timelines.c1?.activitiesByRunId["run-preview"]?.tools[0]
        ?.receipt?.resource;
    const retainedCompletion = state.timelines.c1?.events.find(
      (event) => event.kind === "tool_call_completed",
    );
    const retainedPreview =
      retainedCompletion?.kind === "tool_call_completed"
        ? retainedCompletion.payload.receipt?.resource?.content_preview
        : undefined;

    expect(resource?.contentPreview).toHaveLength(16_384);
    expect(resource?.truncated).toBe(true);
    expect(retainedPreview).toHaveLength(16_384);
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
    expect(state.timelines.c1?.activitiesByRunId["r-pending"]?.status).toBe(
      "pending",
    );
    expect(state.timelines.c1?.activitiesByRunId["r-cancelling"]?.status).toBe(
      "cancelling",
    );
    expect(state.timelines.c1?.activitiesByRunId["r-interrupted"]?.status).toBe(
      "interrupted",
    );
  });

  it("keeps eventless runs while merging subscribed activity over a reload", () => {
    let state = createInitialConversationState();
    state = dispatch(state, {
      type: "runtime_event",
      event: baseEvent({
        runId: "run-live",
        sequence: 2,
        kind: "tool_call_started",
        payload: {
          tool_call_id: "read-live",
          name: "fs.read",
          summary: "Read live file",
        },
      }),
    });
    state = dispatch(state, {
      type: "runtime_event",
      event: baseEvent({
        runId: "run-live",
        sequence: 3,
        kind: "tool_call_output",
        payload: {
          tool_call_id: "read-live",
          chunk: "live result",
        },
      }),
    });

    state = dispatch(state, {
      type: "timeline_loaded",
      timeline: {
        conversationId: "c1",
        messages: [],
        events: [
          baseEvent({
            runId: "run-live",
            sequence: 1,
            kind: "run_started",
            payload: {},
          }),
        ],
        runs: [
          { runId: "run-live", conversationId: "c1", status: "running" },
          {
            runId: "run-eventless",
            conversationId: "c1",
            status: "interrupted",
          },
        ],
      },
    });

    const activities = state.timelines.c1?.activitiesByRunId;
    expect(Object.keys(activities ?? {})).toEqual([
      "run-live",
      "run-eventless",
    ]);
    expect(activities?.["run-live"]?.tools[0]?.output).toBe("live result");
    expect(activities?.["run-eventless"]?.status).toBe("interrupted");
    expect(state.timelines.c1?.events).toHaveLength(3);
  });

  it("rebuilds structured activity for every run in a multi-turn snapshot", () => {
    const firstEvents = materializeCanonicalEvents(
      "c1",
      "run-first",
      "2026-07-30T01:00:00.000Z",
    );
    const secondEvents = materializeCanonicalEvents(
      "c1",
      "run-second",
      "2026-07-30T02:00:00.000Z",
    );
    const state = dispatch(createInitialConversationState(), {
      type: "timeline_loaded",
      timeline: {
        conversationId: "c1",
        messages: [],
        events: [...firstEvents, ...secondEvents],
        runs: [
          {
            runId: "run-first",
            conversationId: "c1",
            status: "completed",
          },
          {
            runId: "run-second",
            conversationId: "c1",
            status: "completed",
          },
        ],
      },
    });

    const timeline = state.timelines.c1;
    expect(Object.keys(timeline?.activitiesByRunId ?? {})).toEqual([
      "run-first",
      "run-second",
    ]);
    expect(
      timeline?.activitiesByRunId["run-first"]?.artifacts[0]?.artifactId,
    ).toBe("art-summary");
    expect(
      timeline?.activitiesByRunId["run-second"]?.tools[0]?.output,
    ).toContain("crates/");
  });
});

describe("terminal activity normalization", () => {
  function seedRunningChildren(state: ConversationUiState): ConversationUiState {
    let next = state;
    next = dispatch(next, {
      type: "runtime_event",
      event: baseEvent({
        runId: "run-term",
        sequence: 1,
        kind: "run_started",
        payload: { prompt_preview: "x" },
      }),
    });
    next = dispatch(next, {
      type: "runtime_event",
      event: baseEvent({
        runId: "run-term",
        sequence: 2,
        kind: "plan_updated",
        payload: {
          steps: [
            { id: "s1", title: "A", status: "completed" },
            { id: "s2", title: "B", status: "active" },
            { id: "s3", title: "C", status: "pending" },
          ],
        },
        createdAt: "t2",
      }),
    });
    next = dispatch(next, {
      type: "runtime_event",
      event: baseEvent({
        runId: "run-term",
        sequence: 3,
        kind: "tool_call_started",
        payload: { tool_call_id: "tc1", name: "list", summary: "list" },
        createdAt: "t3",
      }),
    });
    next = dispatch(next, {
      type: "runtime_event",
      event: baseEvent({
        runId: "run-term",
        sequence: 4,
        kind: "subagent_started",
        payload: { subagent_id: "sa1", role: "explore" },
        createdAt: "t4",
      }),
    });
    return next;
  }

  it("closes open children on run_cancelled", () => {
    let state = createInitialConversationState();
    state = dispatch(state, {
      type: "timeline_loaded",
      timeline: { conversationId: "c1", messages: [], events: [], runs: [] },
    });
    state = seedRunningChildren(state);
    state = dispatch(state, {
      type: "runtime_event",
      event: baseEvent({
        runId: "run-term",
        sequence: 5,
        kind: "run_cancelled",
        payload: { reason: "user_cancel" },
        createdAt: "t5",
      }),
    });

    const timeline = state.timelines.c1;
    expect(state.runs["run-term"]?.status).toBe("cancelled");
    expect(timeline?.plan?.steps.map((s) => s.status)).toEqual([
      "completed",
      "cancelled",
      "cancelled",
    ]);
    expect(timeline?.tools[0]?.status).toBe("cancelled");
    expect(timeline?.subagents[0]?.status).toBe("cancelled");
    expect(timeline?.tools.some((t) => t.status === "running")).toBe(false);
    expect(timeline?.subagents.some((s) => s.status === "running")).toBe(false);
  });

  it("closes open children on turn_failed", () => {
    let state = createInitialConversationState();
    state = dispatch(state, {
      type: "timeline_loaded",
      timeline: { conversationId: "c1", messages: [], events: [], runs: [] },
    });
    state = seedRunningChildren(state);
    state = dispatch(state, {
      type: "runtime_event",
      event: baseEvent({
        runId: "run-term",
        sequence: 5,
        kind: "turn_failed",
        payload: { code: "mock_error", message: "boom" },
        createdAt: "t5",
      }),
    });

    const timeline = state.timelines.c1;
    expect(state.runs["run-term"]?.status).toBe("failed");
    expect(timeline?.plan?.steps[1]?.status).toBe("failed");
    expect(timeline?.tools[0]?.status).toBe("failed");
    expect(timeline?.subagents[0]?.status).toBe("failed");
  });

  it("normalizes restored interrupted runs without leaving running children", () => {
    const timeline: TimelineDto = {
      conversationId: "c1",
      messages: [],
      events: [
        baseEvent({
          runId: "r-int",
          sequence: 1,
          kind: "run_started",
          payload: {},
        }),
        baseEvent({
          runId: "r-int",
          sequence: 2,
          kind: "tool_call_started",
          payload: { tool_call_id: "tc", name: "read", summary: "read" },
          createdAt: "t2",
        }),
        baseEvent({
          runId: "r-int",
          sequence: 3,
          kind: "plan_updated",
          payload: {
            steps: [{ id: "s1", title: "Work", status: "active" }],
          },
          createdAt: "t3",
        }),
      ],
      runs: [
        { runId: "r-int", conversationId: "c1", status: "interrupted" },
      ],
    };

    const state = dispatch(createInitialConversationState(), {
      type: "timeline_loaded",
      timeline,
    });

    expect(state.runs["r-int"]?.status).toBe("interrupted");
    expect(state.timelines.c1?.activitiesByRunId["r-int"]?.status).toBe(
      "interrupted",
    );
    expect(
      state.timelines.c1?.activitiesByRunId["r-int"]?.tools[0]?.status,
    ).toBe("cancelled");
    expect(state.timelines.c1?.tools[0]?.status).toBe("cancelled");
    expect(state.timelines.c1?.plan?.steps[0]?.status).toBe("cancelled");
    expect(state.timelines.c1?.tools.some((t) => t.status === "running")).toBe(
      false,
    );
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

  it("retains the first run's structured activity when a second run starts", () => {
    let state = createInitialConversationState();
    const firstRun = materializeCanonicalEvents("c1", "run-1");
    for (const event of firstRun) {
      state = dispatch(state, { type: "runtime_event", event });
    }

    state = dispatch(state, {
      type: "runtime_event",
      event: baseEvent({
        eventId: "run-2-start",
        runId: "run-2",
        sequence: 1,
        kind: "run_started",
        payload: { prompt_preview: "second turn" },
        createdAt: "2026-07-30T12:01:00.000Z",
      }),
    });
    state = dispatch(state, {
      type: "runtime_event",
      event: baseEvent({
        eventId: "run-2-plan",
        runId: "run-2",
        sequence: 2,
        kind: "plan_updated",
        payload: {
          steps: [{ id: "second", title: "Handle follow-up", status: "active" }],
        },
        createdAt: "2026-07-30T12:01:01.000Z",
      }),
    });

    const timeline = state.timelines.c1 as unknown as {
      activitiesByRunId?: Record<
        string,
        {
          plan: { steps: Array<{ title: string }> } | null;
          tools: Array<{ toolCallId: string }>;
          subagents: Array<{ subagentId: string }>;
          artifacts: Array<{ artifactId: string }>;
        }
      >;
    };
    expect(Object.keys(timeline.activitiesByRunId ?? {})).toEqual([
      "run-1",
      "run-2",
    ]);
    expect(timeline.activitiesByRunId?.["run-1"]?.plan?.steps[0]?.title).toBe(
      "Scan files",
    );
    expect(timeline.activitiesByRunId?.["run-1"]?.tools[0]?.toolCallId).toBe(
      "tc-list",
    );
    expect(
      timeline.activitiesByRunId?.["run-1"]?.subagents[0]?.subagentId,
    ).toBe("sa-explore");
    expect(timeline.activitiesByRunId?.["run-1"]?.artifacts[0]?.artifactId).toBe(
      "art-summary",
    );
    expect(timeline.activitiesByRunId?.["run-2"]?.plan?.steps[0]?.title).toBe(
      "Handle follow-up",
    );
  });
});
