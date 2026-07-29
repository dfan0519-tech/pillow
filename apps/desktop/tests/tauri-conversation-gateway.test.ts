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

  it("does not expose provider or credential command names", async () => {
    vi.mocked(invoke).mockResolvedValue([conversation]);
    await new TauriConversationGateway().listConversations(conversation.projectId);
    const command = vi.mocked(invoke).mock.calls[0]?.[0];
    expect(command).toBe("list_conversations");
    expect(String(command)).not.toMatch(/grok|codex|claude|api[_-]?key|quota|account/i);
  });
});
