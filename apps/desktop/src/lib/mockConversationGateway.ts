/**
 * Deterministic mock conversation gateway for Slice B1 renderer evidence.
 * Emits the Task 2 canonical snake_case fixture. No network or real runtime.
 */

import { CANONICAL_TURN_FIXTURE } from "./canonicalTurnFixture";
import type {
  ConversationDto,
  ConversationGateway,
  RunDto,
  RunStatus,
  RuntimeEventDto,
  RuntimeEventKind,
  TimelineDto,
  TimelineMessageDto,
} from "./conversationGateway";
import { isTerminalKind } from "./conversationGateway";

export const MOCK_RUNTIME_BADGE = "模拟运行时";

type MockOptions = {
  /** Delay between fixture steps. 0 schedules on the next macrotask. */
  stepDelayMs?: number;
};

type InflightRun = {
  runId: string;
  conversationId: string;
  cancelled: boolean;
  terminal: boolean;
  timers: ReturnType<typeof setTimeout>[];
  resolveDrain: Array<() => void>;
};

type StoredRun = RunDto;

type StoredConversation = ConversationDto & {
  messages: TimelineMessageDto[];
  events: RuntimeEventDto[];
  runs: Map<string, StoredRun>;
};

export class MockConversationGateway implements ConversationGateway {
  private readonly stepDelayMs: number;
  private readonly conversations = new Map<string, StoredConversation>();
  private readonly listeners = new Set<(event: RuntimeEventDto) => void>();
  private readonly inflight = new Map<string, InflightRun>();
  private seqCounters = new Map<string, number>();
  private clock = 0;

  constructor(options: MockOptions = {}) {
    this.stepDelayMs = options.stepDelayMs ?? 0;
  }

  async listConversations(projectId: string): Promise<ConversationDto[]> {
    return [...this.conversations.values()]
      .filter((item) => item.projectId === projectId)
      .sort((a, b) => a.createdAt.localeCompare(b.createdAt))
      .map(toConversationDto);
  }

  async createConversation(
    projectId: string,
    title?: string,
  ): Promise<ConversationDto> {
    if (this.createHold) {
      const hold = this.createHold;
      this.createHold = null;
      await hold;
    }
    const now = this.nextTimestamp();
    const conversation: StoredConversation = {
      id: `conv-${crypto.randomUUID()}`,
      projectId,
      title: title?.trim() || "新对话",
      createdAt: now,
      updatedAt: now,
      messages: [],
      events: [],
      runs: new Map(),
    };
    this.conversations.set(conversation.id, conversation);
    return toConversationDto(conversation);
  }

  async loadTimeline(conversationId: string): Promise<TimelineDto> {
    if (this.nextLoadError) {
      const message = this.nextLoadError;
      this.nextLoadError = null;
      throw new Error(message);
    }
    const conversation = this.requireConversation(conversationId);
    return {
      conversationId,
      messages: conversation.messages.map((message) => ({ ...message })),
      events: conversation.events.map((event) => ({ ...event })),
      runs: [...conversation.runs.values()].map((run) => ({ ...run })),
    };
  }

  async sendTurn(
    conversationId: string,
    content: string,
  ): Promise<{ runId: string }> {
    const conversation = this.requireConversation(conversationId);
    const trimmed = content.trim();
    if (!trimmed) {
      throw new Error("Turn content must not be empty.");
    }
    if (this.nextSendError) {
      const message = this.nextSendError;
      this.nextSendError = null;
      throw new Error(message);
    }

    const now = this.nextTimestamp();
    conversation.messages.push({
      id: `msg-${crypto.randomUUID()}`,
      role: "user",
      content: trimmed,
      createdAt: now,
    });
    if (conversation.title === "新对话") {
      conversation.title = trimmed.slice(0, 32);
    }
    conversation.updatedAt = now;

    const runId = `run-${crypto.randomUUID()}`;
    conversation.runs.set(runId, {
      runId,
      conversationId,
      status: "pending",
    });

    const inflight: InflightRun = {
      runId,
      conversationId,
      cancelled: false,
      terminal: false,
      timers: [],
      resolveDrain: [],
    };
    this.inflight.set(runId, inflight);
    this.scheduleFixture(inflight);
    return { runId };
  }

  async cancelRun(runId: string): Promise<void> {
    const inflight = this.inflight.get(runId);
    if (!inflight || inflight.terminal) return;
    inflight.cancelled = true;
    for (const timer of inflight.timers) {
      clearTimeout(timer);
    }
    inflight.timers = [];
    this.setRunStatus(inflight.conversationId, runId, "cancelling");
    this.emitEvent(inflight, "run_cancelled", { reason: "user_cancel" });
    this.markTerminal(inflight);
  }

  subscribe(listener: (event: RuntimeEventDto) => void): () => void {
    this.listeners.add(listener);
    return () => {
      this.listeners.delete(listener);
    };
  }

  /**
   * Test helper: seed a conversation timeline with authoritative runs
   * (pending / cancelling / interrupted restoration cases).
   */
  seedTimeline(
    conversationId: string,
    input: {
      messages?: TimelineMessageDto[];
      events?: RuntimeEventDto[];
      runs?: RunDto[];
    },
  ): void {
    const conversation = this.requireConversation(conversationId);
    if (input.messages) {
      conversation.messages = input.messages.map((message) => ({ ...message }));
    }
    if (input.events) {
      conversation.events = input.events.map((event) => ({ ...event }));
      for (const event of conversation.events) {
        const prev = this.seqCounters.get(event.runId) ?? 0;
        if (event.sequence > prev) {
          this.seqCounters.set(event.runId, event.sequence);
        }
      }
    }
    if (input.runs) {
      for (const run of input.runs) {
        conversation.runs.set(run.runId, { ...run });
      }
    }
  }

  /** Test helper: force a sendTurn failure for the next call. */
  failNextSend(message = "mock send failed"): void {
    this.nextSendError = message;
  }

  /** Test helper: force loadTimeline failure for the next call. */
  failNextLoadTimeline(message = "mock timeline load failed"): void {
    this.nextLoadError = message;
  }

  /**
   * Test helper: hold the next createConversation until the returned release
   * function is called (deferred-promise race tests).
   */
  holdNextCreate(): () => void {
    let release!: () => void;
    this.createHold = new Promise<void>((resolve) => {
      release = resolve;
    });
    return release;
  }

  private nextSendError: string | null = null;
  private nextLoadError: string | null = null;
  private createHold: Promise<void> | null = null;

  /** Test helper: wait until every inflight run has finished or been cancelled. */
  async drain(): Promise<void> {
    const pending = [...this.inflight.values()].filter((run) => !run.terminal);
    if (pending.length === 0) return;
    await Promise.all(
      pending.map(
        (run) =>
          new Promise<void>((resolve) => {
            if (run.terminal) {
              resolve();
              return;
            }
            run.resolveDrain.push(resolve);
          }),
      ),
    );
  }

  /** Test helper: resolve when an event kind is observed (any run). */
  waitForKind(kind: RuntimeEventKind, timeoutMs = 5_000): Promise<RuntimeEventDto> {
    return new Promise((resolve, reject) => {
      const timer = setTimeout(() => {
        unsubscribe();
        reject(new Error(`Timed out waiting for ${kind}`));
      }, timeoutMs);
      const unsubscribe = this.subscribe((event) => {
        if (event.kind !== kind) return;
        clearTimeout(timer);
        unsubscribe();
        resolve(event);
      });
    });
  }

  private scheduleFixture(inflight: InflightRun) {
    let delay = 0;
    for (const step of CANONICAL_TURN_FIXTURE) {
      const handle = setTimeout(() => {
        this.step(inflight, step.kind, step.payload as RuntimeEventDto["payload"]);
      }, delay);
      inflight.timers.push(handle);
      delay += Math.max(0, this.stepDelayMs);
    }
  }

  private step(
    inflight: InflightRun,
    kind: RuntimeEventKind,
    payload: RuntimeEventDto["payload"],
  ) {
    if (inflight.terminal || inflight.cancelled) return;

    if (kind === "assistant_delta") {
      const text =
        "text" in payload && typeof payload.text === "string" ? payload.text : "";
      const conversation = this.conversations.get(inflight.conversationId);
      if (conversation) {
        const last = conversation.messages.at(-1);
        if (last?.role === "assistant") {
          last.content += text;
        } else {
          conversation.messages.push({
            id: `msg-${crypto.randomUUID()}`,
            role: "assistant",
            content: text,
            createdAt: this.nextTimestamp(),
          });
        }
        conversation.updatedAt = this.nextTimestamp();
      }
    }

    this.emitEvent(inflight, kind, payload);

    if (isTerminalKind(kind)) {
      this.markTerminal(inflight);
    }
  }

  private emitEvent(
    inflight: InflightRun,
    kind: RuntimeEventKind,
    payload: RuntimeEventDto["payload"],
  ) {
    if (inflight.terminal) return;
    const conversation = this.requireConversation(inflight.conversationId);
    const sequence = (this.seqCounters.get(inflight.runId) ?? 0) + 1;
    this.seqCounters.set(inflight.runId, sequence);

    if (kind === "run_started") {
      this.setRunStatus(inflight.conversationId, inflight.runId, "running");
    } else if (kind === "turn_completed") {
      this.setRunStatus(inflight.conversationId, inflight.runId, "completed");
    } else if (kind === "turn_failed") {
      this.setRunStatus(inflight.conversationId, inflight.runId, "failed");
    } else if (kind === "run_cancelled") {
      this.setRunStatus(inflight.conversationId, inflight.runId, "cancelled");
    }

    const event = {
      eventId: `evt-${inflight.runId}-${sequence}`,
      runId: inflight.runId,
      conversationId: inflight.conversationId,
      sequence,
      kind,
      payload: { ...payload },
      createdAt: this.nextTimestamp(),
    } as RuntimeEventDto;

    conversation.events.push(event);
    conversation.updatedAt = event.createdAt;
    for (const listener of this.listeners) {
      listener(event);
    }
  }

  private setRunStatus(
    conversationId: string,
    runId: string,
    status: RunStatus,
  ) {
    const conversation = this.conversations.get(conversationId);
    if (!conversation) return;
    const existing = conversation.runs.get(runId);
    conversation.runs.set(runId, {
      runId,
      conversationId,
      status,
      ...(existing ? {} : {}),
    });
  }

  private markTerminal(inflight: InflightRun) {
    if (inflight.terminal) return;
    inflight.terminal = true;
    for (const timer of inflight.timers) {
      clearTimeout(timer);
    }
    inflight.timers = [];
    const waiters = inflight.resolveDrain.splice(0, inflight.resolveDrain.length);
    for (const resolve of waiters) resolve();
  }

  private requireConversation(conversationId: string): StoredConversation {
    const conversation = this.conversations.get(conversationId);
    if (!conversation) {
      throw new Error(`Conversation not found: ${conversationId}`);
    }
    return conversation;
  }

  private nextTimestamp(): string {
    this.clock += 1;
    const base = Date.parse("2026-07-30T00:00:00.000Z");
    return new Date(base + this.clock * 1000).toISOString();
  }
}

function toConversationDto(conversation: StoredConversation): ConversationDto {
  return {
    id: conversation.id,
    projectId: conversation.projectId,
    title: conversation.title,
    createdAt: conversation.createdAt,
    updatedAt: conversation.updatedAt,
  };
}
