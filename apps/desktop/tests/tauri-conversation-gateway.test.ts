import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { beforeEach, describe, expect, it, vi } from "vitest";
import {
  CONVERSATION_RUNTIME_EVENT,
  TauriConversationGateway,
} from "../src/lib/tauriConversationGateway";

vi.mock("@tauri-apps/api/core", () => ({
  invoke: vi.fn(),
}));

vi.mock("@tauri-apps/api/event", () => ({
  listen: vi.fn(),
}));

const conversation = {
  id: "11111111-1111-1111-1111-111111111111",
  projectId: "00000000-0000-0000-0000-000000000001",
  title: "新对话",
  createdAt: "2026-07-30T00:00:00Z",
  updatedAt: "2026-07-30T00:00:00Z",
};

const runEvent = {
  eventId: "22222222-2222-2222-2222-222222222222",
  runId: "33333333-3333-3333-3333-333333333333",
  conversationId: conversation.id,
  sequence: 1,
  kind: "run_started",
  payload: { prompt_preview: "hi" },
  createdAt: "2026-07-30T00:00:01Z",
};

const timeline = {
  conversationId: conversation.id,
  messages: [
    {
      id: "44444444-4444-4444-4444-444444444444",
      role: "user",
      content: "hello",
      createdAt: "2026-07-30T00:00:00Z",
    },
  ],
  events: [runEvent],
  runs: [
    {
      runId: runEvent.runId,
      conversationId: conversation.id,
      status: "running",
    },
  ],
};

const validPayloadByKind = {
  run_started: { prompt_preview: "start" },
  assistant_delta: { text: "answer" },
  reasoning_summary_delta: { text: "reasoning" },
  plan_updated: {
    steps: [{ id: "step-1", title: "Inspect", status: "active" }],
  },
  tool_call_started: {
    tool_call_id: "tool-1",
    name: "fs.search",
    summary: "Search",
    input_summary: {
      kind: "fs_search",
      path: "src",
      query_preview: "needle",
      case_sensitive: false,
      max_results: 20,
    },
  },
  tool_call_output: { tool_call_id: "tool-1", chunk: "result" },
  tool_call_completed: {
    tool_call_id: "tool-1",
    status: "completed",
    receipt: {
      receipt_id: "receipt-1",
      label: "Search receipt",
      resource: {
        resource_id: "resource-1",
        kind: "search_results",
        label: "Search results",
        path: "src",
        content_preview: "src/App.tsx:1",
        truncated: false,
      },
    },
  },
  subagent_started: { subagent_id: "agent-1", role: "explore" },
  subagent_progress: { subagent_id: "agent-1", summary: "working" },
  subagent_completed: { subagent_id: "agent-1", status: "completed" },
  artifact_produced: {
    artifact_id: "artifact-1",
    path: "reports/result.md",
    kind: "report",
    resource: {
      resource_id: "artifact-resource-1",
      kind: "artifact",
      label: "Result",
      path: "reports/result.md",
      content_preview: "preview",
    },
  },
  turn_completed: { summary: "done" },
  turn_failed: { code: "runtime_failed", message: "failed safely" },
  run_cancelled: { reason: "requested" },
  heartbeat: { tick: 1 },
} as const;

function eventFor(
  kind: keyof typeof validPayloadByKind,
  payload: unknown = validPayloadByKind[kind],
) {
  return {
    ...runEvent,
    eventId: `event-${kind}`,
    kind,
    payload,
  };
}

describe("TauriConversationGateway", () => {
  beforeEach(() => {
    vi.mocked(invoke).mockReset();
    vi.mocked(listen).mockReset();
  });

  it("uses typed list/create/load/send/cancel commands", async () => {
    vi.mocked(invoke)
      .mockResolvedValueOnce([conversation])
      .mockResolvedValueOnce(conversation)
      .mockResolvedValueOnce(timeline)
      .mockResolvedValueOnce({ runId: runEvent.runId })
      .mockResolvedValueOnce(undefined);

    const gateway = new TauriConversationGateway();

    await expect(gateway.listConversations(conversation.projectId)).resolves.toEqual([
      conversation,
    ]);
    await expect(
      gateway.createConversation(conversation.projectId, "title"),
    ).resolves.toEqual(conversation);
    await expect(gateway.loadTimeline(conversation.id)).resolves.toEqual(timeline);
    await expect(gateway.sendTurn(conversation.id, "hello")).resolves.toEqual({
      runId: runEvent.runId,
    });
    await expect(gateway.cancelRun(runEvent.runId)).resolves.toBeUndefined();

    expect(invoke).toHaveBeenNthCalledWith(1, "list_conversations", {
      projectId: conversation.projectId,
    });
    expect(invoke).toHaveBeenNthCalledWith(2, "create_conversation", {
      projectId: conversation.projectId,
      title: "title",
    });
    expect(invoke).toHaveBeenNthCalledWith(3, "load_timeline", {
      conversationId: conversation.id,
    });
    expect(invoke).toHaveBeenNthCalledWith(4, "send_turn", {
      conversationId: conversation.id,
      content: "hello",
    });
    expect(invoke).toHaveBeenNthCalledWith(5, "cancel_run", {
      runId: runEvent.runId,
    });
  });

  it("rejects malformed conversation DTOs", async () => {
    vi.mocked(invoke).mockResolvedValue({ ...conversation, projectId: "" });
    await expect(
      new TauriConversationGateway().createConversation(conversation.projectId),
    ).rejects.toThrow(/invalid conversation response/i);
  });

  it("subscribes to the typed runtime event channel and unsubscribes cleanly", async () => {
    const unlisten = vi.fn();
    const handlers: Array<(event: { payload: unknown }) => void> = [];
    vi.mocked(listen).mockImplementation(async (_event, cb) => {
      handlers.push(cb as (event: { payload: unknown }) => void);
      return unlisten;
    });

    const gateway = new TauriConversationGateway();
    const received: unknown[] = [];
    const unsubscribe = gateway.subscribe((event) => {
      received.push(event);
    });

    // Allow async listen registration.
    await vi.waitFor(() => {
      expect(listen).toHaveBeenCalledWith(
        CONVERSATION_RUNTIME_EVENT,
        expect.any(Function),
      );
      expect(handlers.length).toBeGreaterThan(0);
    });

    handlers[0]!({ payload: runEvent });
    expect(received).toEqual([runEvent]);

    unsubscribe();
    expect(unlisten).toHaveBeenCalledTimes(1);

    // Late unsubscribe before listen resolves must not leave a dangling listener.
    const unlistenLate = vi.fn();
    let resolveListen!: (fn: () => void) => void;
    vi.mocked(listen).mockImplementation(
      () =>
        new Promise((resolve) => {
          resolveListen = resolve;
        }),
    );
    const gateway2 = new TauriConversationGateway();
    const unsub2 = gateway2.subscribe(() => undefined);
    unsub2();
    resolveListen(unlistenLate);
    await Promise.resolve();
    await Promise.resolve();
    expect(unlistenLate).toHaveBeenCalledTimes(1);
  });

  it("ignores invalid runtime event payloads without throwing to the host", async () => {
    const handlers: Array<(event: { payload: unknown }) => void> = [];
    vi.mocked(listen).mockImplementation(async (_event, cb) => {
      handlers.push(cb as (event: { payload: unknown }) => void);
      return vi.fn();
    });

    const gateway = new TauriConversationGateway();
    const received: unknown[] = [];
    gateway.subscribe((event) => {
      received.push(event);
    });
    await vi.waitFor(() => expect(handlers.length).toBeGreaterThan(0));

    handlers[0]!({ payload: { kind: "not_a_real_kind" } });
    expect(received).toEqual([]);
  });

  it("validates the discriminated payload for every known runtime event kind", async () => {
    const handlers: Array<(event: { payload: unknown }) => void> = [];
    vi.mocked(listen).mockImplementation(async (_event, cb) => {
      handlers.push(cb as (event: { payload: unknown }) => void);
      return vi.fn();
    });
    const received: unknown[] = [];
    new TauriConversationGateway().subscribe((event) => received.push(event));
    await vi.waitFor(() => expect(handlers.length).toBeGreaterThan(0));

    for (const kind of Object.keys(validPayloadByKind) as Array<
      keyof typeof validPayloadByKind
    >) {
      expect(() => handlers[0]!({ payload: eventFor(kind) })).not.toThrow();
    }
    expect(received.map((event) => (event as { kind: string }).kind)).toEqual(
      Object.keys(validPayloadByKind),
    );
  });

  it("ignores malformed known-kind and nested payloads without invoking listeners", async () => {
    const handlers: Array<(event: { payload: unknown }) => void> = [];
    vi.mocked(listen).mockImplementation(async (_event, cb) => {
      handlers.push(cb as (event: { payload: unknown }) => void);
      return vi.fn();
    });
    const received: unknown[] = [];
    const consoleError = vi.spyOn(console, "error").mockImplementation(() => undefined);
    new TauriConversationGateway().subscribe((event) => received.push(event));
    await vi.waitFor(() => expect(handlers.length).toBeGreaterThan(0));

    const malformed = [
      eventFor("run_started", { prompt_preview: 7 }),
      eventFor("assistant_delta", { text: 7 }),
      eventFor("reasoning_summary_delta", { text: null }),
      eventFor("plan_updated", { steps: [{ id: "s", title: 2, status: "active" }] }),
      eventFor("tool_call_started", {
        tool_call_id: "t",
        name: "fs.list",
        input_summary: { kind: "fs_list", path: "src", max_results: "many" },
      }),
      eventFor("tool_call_output", { tool_call_id: "t", chunk: 4 }),
      eventFor("tool_call_completed", {
        tool_call_id: "t",
        status: "completed",
        receipt: {
          receipt_id: "r",
          label: "receipt",
          resource: { resource_id: "x", kind: "socket", label: "bad" },
        },
      }),
      eventFor("subagent_started", { subagent_id: 1, role: "explore" }),
      eventFor("subagent_progress", { subagent_id: "a", summary: false }),
      eventFor("subagent_completed", { subagent_id: "a", status: "running" }),
      eventFor("artifact_produced", {
        artifact_id: "a",
        path: "result.md",
        kind: "report",
        resource: { resource_id: "r", kind: "artifact", label: 9 },
      }),
      eventFor("turn_completed", { summary: 1 }),
      eventFor("turn_failed", { code: "failed", message: 3 }),
      eventFor("run_cancelled", { reason: [] }),
      eventFor("heartbeat", { tick: Number.NaN }),
      eventFor("tool_call_output", {
        tool_call_id: "t".repeat(257),
        chunk: "bounded identity",
      }),
    ];
    for (const event of malformed) {
      expect(() => handlers[0]!({ payload: event })).not.toThrow();
    }

    expect(received).toEqual([]);
    expect(consoleError).toHaveBeenCalledTimes(malformed.length);
    consoleError.mockRestore();
  });

  it("drops malformed known-kind events during reload before reducer use", async () => {
    vi.mocked(invoke).mockResolvedValue({
      ...timeline,
      events: [eventFor("tool_call_output", { tool_call_id: "t", chunk: 4 })],
    });
    const consoleError = vi.spyOn(console, "error").mockImplementation(() => undefined);

    await expect(
      new TauriConversationGateway().loadTimeline(conversation.id),
    ).resolves.toEqual({ ...timeline, events: [] });
    expect(consoleError).toHaveBeenCalledTimes(1);
    consoleError.mockRestore();
  });

  it("bounds validated nested input and resource fields at the Tauri boundary", async () => {
    vi.mocked(invoke).mockResolvedValue({
      ...timeline,
      events: [
        eventFor("tool_call_started", {
          tool_call_id: "tool-bounded",
          name: "fs.search",
          input_summary: {
            kind: "fs_search",
            path: "p".repeat(2_000),
            query_preview: "q".repeat(2_000),
            case_sensitive: false,
            max_results: 20,
          },
        }),
        eventFor("tool_call_completed", {
          tool_call_id: "tool-bounded",
          status: "completed",
          receipt: {
            receipt_id: "receipt-bounded",
            label: "Receipt",
            resource: {
              resource_id: "resource-bounded",
              kind: "search_results",
              label: "Results",
              path: "p".repeat(2_000),
              content_preview: "x".repeat(20_000),
            },
          },
        }),
      ],
    });

    const loaded = await new TauriConversationGateway().loadTimeline(
      conversation.id,
    );
    const started = loaded.events[0];
    const completed = loaded.events[1];
    expect(
      started?.kind === "tool_call_started"
        ? started.payload.input_summary?.path.length
        : 0,
    ).toBe(1_024);
    expect(
      started?.kind === "tool_call_started"
        && started.payload.input_summary?.kind === "fs_search"
        ? started.payload.input_summary.query_preview.length
        : 0,
    ).toBe(256);
    expect(
      completed?.kind === "tool_call_completed"
        ? completed.payload.receipt?.resource?.path?.length
        : 0,
    ).toBe(1_024);
    expect(
      completed?.kind === "tool_call_completed"
        ? completed.payload.receipt?.resource?.content_preview?.length
        : 0,
    ).toBe(16_384);
    expect(
      completed?.kind === "tool_call_completed"
        ? completed.payload.receipt?.resource?.truncated
        : false,
    ).toBe(true);
  });

  it("does not expose provider or credential command names", async () => {
    vi.mocked(invoke).mockResolvedValue([conversation]);
    await new TauriConversationGateway().listConversations(conversation.projectId);
    const command = vi.mocked(invoke).mock.calls[0]?.[0];
    expect(command).toBe("list_conversations");
    expect(String(command)).not.toMatch(/grok|codex|claude|api[_-]?key|quota|account/i);
  });
});
