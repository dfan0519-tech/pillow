/**
 * Deterministic mock conversation gateway for Slice B1 renderer evidence.
 * No network, credentials, or real agent runtime — test and prototype only.
 */

import type {
  ConversationDto,
  ConversationGateway,
  RuntimeEventDto,
  RuntimeEventKind,
  TimelineDto,
  TimelineMessageDto,
} from "./conversationGateway";

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

type StoredConversation = ConversationDto & {
  messages: TimelineMessageDto[];
  events: RuntimeEventDto[];
};

const ASSISTANT_CHUNKS = [
  "已收到任务。",
  "正在整理项目结构与入口文件。",
  "建议从 `apps/desktop` 与核心 crate 边界开始阅读。",
];

/**
 * Fixed, ordered fixture stream. Kinds are the normalized vocabulary only.
 */
function buildFixtureSteps(): Array<{
  kind: RuntimeEventKind;
  payload: Record<string, unknown>;
}> {
  return [
    { kind: "run_started", payload: {} },
    {
      kind: "reasoning_summary_delta",
      payload: { text: "梳理请求目标与可验证交付物。" },
    },
    {
      kind: "plan_updated",
      payload: {
        title: "执行计划",
        steps: ["定位入口", "检查对话边界", "汇总结果"],
      },
    },
    {
      kind: "tool_call_started",
      payload: { toolCallId: "tool-1", name: "list_workspace", title: "列出工作区" },
    },
    {
      kind: "tool_call_output",
      payload: {
        toolCallId: "tool-1",
        output: "apps/desktop, crates/kernel-session",
      },
    },
    {
      kind: "tool_call_completed",
      payload: { toolCallId: "tool-1", name: "list_workspace", status: "completed" },
    },
    {
      kind: "subagent_started",
      payload: { subagentId: "sub-1", title: "结构扫描" },
    },
    {
      kind: "subagent_progress",
      payload: { subagentId: "sub-1", message: "扫描源码树…" },
    },
    {
      kind: "subagent_completed",
      payload: { subagentId: "sub-1", title: "结构扫描", status: "completed" },
    },
    ...ASSISTANT_CHUNKS.map((text) => ({
      kind: "assistant_delta" as const,
      payload: { text },
    })),
    { kind: "turn_completed", payload: {} },
  ];
}

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
    const now = this.nextTimestamp();
    const conversation: StoredConversation = {
      id: `conv-${crypto.randomUUID()}`,
      projectId,
      title: title?.trim() || "新对话",
      createdAt: now,
      updatedAt: now,
      messages: [],
      events: [],
    };
    this.conversations.set(conversation.id, conversation);
    return toConversationDto(conversation);
  }

  async loadTimeline(conversationId: string): Promise<TimelineDto> {
    const conversation = this.requireConversation(conversationId);
    return {
      conversationId,
      messages: conversation.messages.map((message) => ({ ...message })),
      events: conversation.events.map((event) => ({ ...event })),
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
    this.emitEvent(inflight, "run_cancelled", {});
    this.markTerminal(inflight);
  }

  subscribe(listener: (event: RuntimeEventDto) => void): () => void {
    this.listeners.add(listener);
    return () => {
      this.listeners.delete(listener);
    };
  }

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
    const steps = buildFixtureSteps();
    let delay = 0;
    for (const step of steps) {
      const handle = setTimeout(() => {
        this.step(inflight, step.kind, step.payload);
      }, delay);
      inflight.timers.push(handle);
      delay += Math.max(0, this.stepDelayMs);
    }
  }

  private step(
    inflight: InflightRun,
    kind: RuntimeEventKind,
    payload: Record<string, unknown>,
  ) {
    if (inflight.terminal || inflight.cancelled) return;

    if (kind === "assistant_delta") {
      const text = String(payload.text ?? "");
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

    if (kind === "turn_completed" || kind === "turn_failed" || kind === "run_cancelled") {
      this.markTerminal(inflight);
    }
  }

  private emitEvent(
    inflight: InflightRun,
    kind: RuntimeEventKind,
    payload: Record<string, unknown>,
  ) {
    if (inflight.terminal) return;
    const conversation = this.requireConversation(inflight.conversationId);
    const sequence = (this.seqCounters.get(inflight.runId) ?? 0) + 1;
    this.seqCounters.set(inflight.runId, sequence);
    const event: RuntimeEventDto = {
      eventId: `evt-${inflight.runId}-${sequence}`,
      runId: inflight.runId,
      conversationId: inflight.conversationId,
      sequence,
      kind,
      payload: { ...payload },
      createdAt: this.nextTimestamp(),
    };
    conversation.events.push(event);
    conversation.updatedAt = event.createdAt;
    for (const listener of this.listeners) {
      listener(event);
    }
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
