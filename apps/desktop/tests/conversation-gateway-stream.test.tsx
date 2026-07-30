/**
 * Gateway contract + background-stream workbench paths (Slice B1).
 */
import { screen, waitFor } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";
import {
  MockConversationGateway,
  MOCK_RUNTIME_BADGE,
} from "../src/lib/mockConversationGateway";
import type { RuntimeEventDto } from "../src/lib/conversationGateway";
import { isTerminalKind } from "../src/lib/conversationGateway";
import {
  drainMock,
  selectProjectAfterRender,
} from "./conversation-workbench-helpers";
import "../src/styles/tokens.css";
import "../src/styles/workbench.css";

vi.mock("@tauri-apps/api/window", () => ({
  getCurrentWindow: () => ({
    minimize: vi.fn().mockResolvedValue(undefined),
    toggleMaximize: vi.fn().mockResolvedValue(undefined),
    close: vi.fn().mockResolvedValue(undefined),
    isMaximized: vi.fn().mockResolvedValue(false),
    onResized: vi.fn().mockResolvedValue(vi.fn()),
  }),
}));

afterEach(() => {
  vi.useRealTimers();
});

describe("conversation gateway contract", () => {
  it("isolates conversation lists by project", async () => {
    const gateway = new MockConversationGateway({ stepDelayMs: 0 });
    const a = await gateway.createConversation("project-a", "Alpha chat");
    const b = await gateway.createConversation("project-b", "Beta chat");

    const listA = await gateway.listConversations("project-a");
    const listB = await gateway.listConversations("project-b");

    expect(listA.map((c) => c.id)).toEqual([a.id]);
    expect(listB.map((c) => c.id)).toEqual([b.id]);
    expect(listA[0]?.projectId).toBe("project-a");
    expect(listB[0]?.projectId).toBe("project-b");
  });

  it("streams plan, tool, sub-agent activity and completes with one terminal", async () => {
    const gateway = new MockConversationGateway({ stepDelayMs: 0 });
    const conversation = await gateway.createConversation("project-a");
    const seen: RuntimeEventDto[] = [];
    const unsubscribe = gateway.subscribe((event) => {
      seen.push(event);
    });

    const { runId } = await gateway.sendTurn(conversation.id, "请分析仓库结构");
    await gateway.drain();

    const kinds = seen.map((event) => event.kind);
    expect(kinds[0]).toBe("run_started");
    expect(kinds).toContain("plan_updated");
    expect(kinds).toContain("tool_call_started");
    expect(kinds).toContain("tool_call_completed");
    expect(kinds).toContain("subagent_started");
    expect(kinds).toContain("subagent_completed");
    expect(kinds).toContain("assistant_delta");
    expect(kinds.at(-1)).toBe("turn_completed");
    expect(seen.every((event) => event.runId === runId)).toBe(true);
    expect(seen.filter((event) => isTerminalKind(event.kind))).toHaveLength(1);

    // Canonical snake_case payload keys
    const plan = seen.find((event) => event.kind === "plan_updated");
    expect(plan?.kind === "plan_updated" && plan.payload.steps[0]?.id).toBe("s1");
    const toolOut = seen.find((event) => event.kind === "tool_call_output");
    expect(
      toolOut?.kind === "tool_call_output" && "chunk" in toolOut.payload,
    ).toBe(true);

    const timeline = await gateway.loadTimeline(conversation.id);
    expect(timeline.runs.some((run) => run.runId === runId)).toBe(true);
    expect(timeline.messages.some((m) => m.role === "user")).toBe(true);
    expect(timeline.messages.some((m) => m.role === "assistant")).toBe(true);
    unsubscribe();
  });

  it("cancels an active run with exactly one terminal state", async () => {
    const gateway = new MockConversationGateway({ stepDelayMs: 30 });
    const conversation = await gateway.createConversation("project-a");
    const seen: RuntimeEventDto[] = [];
    gateway.subscribe((event) => {
      seen.push(event);
    });

    const { runId } = await gateway.sendTurn(conversation.id, "长任务");
    await gateway.waitForKind("assistant_delta");
    await gateway.cancelRun(runId);
    await gateway.drain();

    const terminals = seen.filter((event) => isTerminalKind(event.kind));
    expect(terminals).toHaveLength(1);
    expect(terminals[0]?.kind).toBe("run_cancelled");
    expect(seen.some((event) => event.kind === "turn_completed")).toBe(false);
  });

  it("keeps a background stream alive when another conversation is selected", async () => {
    const gateway = new MockConversationGateway({ stepDelayMs: 20 });
    const first = await gateway.createConversation("project-a", "First");
    const second = await gateway.createConversation("project-a", "Second");
    const byConversation = new Map<string, RuntimeEventDto[]>();
    gateway.subscribe((event) => {
      const list = byConversation.get(event.conversationId) ?? [];
      list.push(event);
      byConversation.set(event.conversationId, list);
    });

    await gateway.sendTurn(first.id, "后台流");
    await gateway.waitForKind("assistant_delta");
    await gateway.createConversation("project-a", "Third");
    await gateway.drain();

    const firstEvents = byConversation.get(first.id) ?? [];
    expect(firstEvents.some((event) => event.kind === "assistant_delta")).toBe(true);
    expect(firstEvents.some((event) => event.kind === "turn_completed")).toBe(true);
    expect(byConversation.get(second.id) ?? []).toHaveLength(0);
  });
});

describe("conversation workbench stream UI", () => {
  it("sends a turn, streams structured activity, and completes", async () => {
    const conversationGateway = new MockConversationGateway({ stepDelayMs: 0 });
    const user = await selectProjectAfterRender(conversationGateway);

    await user.click(screen.getByRole("button", { name: "新建对话" }));
    const textarea = await screen.findByRole("textbox", { name: "工作请求" });
    await user.type(textarea, "请分析仓库");
    await user.click(screen.getByTestId("composer-send"));

    await drainMock(conversationGateway);

    expect(await screen.findByTestId("conversation-timeline")).toBeVisible();
    expect(screen.getByTestId("timeline-user-message")).toHaveTextContent("请分析仓库");
    expect(screen.getByTestId("timeline-assistant-message")).toBeVisible();
    const activity = screen.getByTestId("run-activity");
    expect(activity).toBeVisible();
    expect(activity.querySelector("details")).not.toHaveAttribute("open");
    await user.click(activity.querySelector("summary")!);
    expect(screen.getByTestId("run-activity-plan")).toBeVisible();
    expect(screen.getAllByTestId("run-activity-plan-step").length).toBeGreaterThan(0);
    expect(screen.getByTestId("run-activity-tool")).toHaveTextContent(/crates|apps/);
    expect(screen.getByTestId("run-activity-subagent")).toHaveTextContent(/explore|indexed/i);
    await waitFor(() => {
      expect(screen.getByTestId("run-status")).toHaveTextContent(/已完成|completed/i);
    });
    expect(screen.getByTestId("mock-runtime-badge")).toHaveTextContent(MOCK_RUNTIME_BADGE);
    expect(screen.queryByText(/Grok|provider|model|routeCode/i)).not.toBeInTheDocument();
  });

  it("continues a background stream when the user selects another conversation", async () => {
    const conversationGateway = new MockConversationGateway({ stepDelayMs: 25 });
    const user = await selectProjectAfterRender(conversationGateway);

    await user.click(screen.getByRole("button", { name: "新建对话" }));
    await user.type(
      await screen.findByRole("textbox", { name: "工作请求" }),
      "后台任务",
    );
    await user.click(screen.getByTestId("composer-send"));

    await waitFor(() => {
      expect(screen.getByTestId("timeline-user-message")).toBeVisible();
    });

    await user.click(screen.getByRole("button", { name: "新建对话" }));
    const secondTitle = await screen.findAllByRole("button", { name: /新对话|对话/ });
    expect(secondTitle.length).toBeGreaterThanOrEqual(1);

    await drainMock(conversationGateway);

    const rows = screen.getAllByTestId("conversation-row");
    expect(rows.length).toBeGreaterThanOrEqual(2);
    await user.click(rows[0]!);

    await waitFor(() => {
      expect(screen.getByTestId("timeline-assistant-message")).toBeVisible();
      expect(screen.getByTestId("run-status")).toHaveTextContent(/已完成|completed/i);
    });
  });

  it("cancels the active run from the workbench with a single terminal state", async () => {
    const conversationGateway = new MockConversationGateway({ stepDelayMs: 40 });
    const user = await selectProjectAfterRender(conversationGateway);

    await user.click(screen.getByRole("button", { name: "新建对话" }));
    await user.type(
      await screen.findByRole("textbox", { name: "工作请求" }),
      "可取消任务",
    );
    await user.click(screen.getByTestId("composer-send"));

    await waitFor(() => {
      expect(screen.getByTestId("composer-cancel")).toBeEnabled();
    });
    await user.click(screen.getByTestId("composer-cancel"));
    await drainMock(conversationGateway);

    await waitFor(() => {
      expect(screen.getByTestId("run-status")).toHaveTextContent(/已取消|cancelled/i);
    });
    expect(screen.queryByTestId("run-status")).not.toHaveTextContent(/已完成/);
  });
});
