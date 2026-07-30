/**
 * Workbench presentation and error-path regressions (Slice B1).
 */
import { render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, describe, expect, it, vi } from "vitest";
import { App } from "../src/App";
import {
  ConversationTimeline,
  TIMELINE_WINDOW_THRESHOLD,
} from "../src/features/conversations/ConversationTimeline";
import type { ConversationTimelineView } from "../src/features/conversations/conversationReducer";
import {
  MockConversationGateway,
  MOCK_RUNTIME_BADGE,
} from "../src/lib/mockConversationGateway";
import { Composer } from "../src/features/workbench/Composer";
import {
  drainMock,
  fixtureProject,
  fixtureProjectGateway,
  renderApp,
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

describe("conversation workbench UI", () => {
  it("lists only the active project's conversations in the sidebar", async () => {
    const projectA = fixtureProject({ id: "project-a", name: "Project A" });
    const projectB = fixtureProject({
      id: "project-b",
      name: "Project B",
      rootPath: "F:\\work\\project-b",
    });
    const conversationGateway = new MockConversationGateway({ stepDelayMs: 0 });
    await conversationGateway.createConversation("project-a", "A only");
    await conversationGateway.createConversation("project-b", "B only");

    const user = userEvent.setup();
    renderApp(conversationGateway, [projectA, projectB]);

    await user.click(await screen.findByRole("button", { name: "Project A" }));
    const groupA = await screen.findByRole("group", { name: "项目对话" });
    expect(within(groupA).getByText("A only")).toBeVisible();
    expect(within(groupA).queryByText("B only")).not.toBeInTheDocument();

    await user.click(screen.getByRole("button", { name: "Project B" }));
    const groupB = await screen.findByRole("group", { name: "项目对话" });
    expect(within(groupB).getByText("B only")).toBeVisible();
    expect(within(groupB).queryByText("A only")).not.toBeInTheDocument();
  });

  it("clears selected conversation on sidebar project switch", async () => {
    const projectA = fixtureProject({ id: "project-a", name: "Project A" });
    const projectB = fixtureProject({
      id: "project-b",
      name: "Project B",
      rootPath: "F:\\work\\project-b",
    });
    const conversationGateway = new MockConversationGateway({ stepDelayMs: 0 });
    const chat = await conversationGateway.createConversation("project-a", "A chat");
    conversationGateway.seedTimeline(chat.id, {
      messages: [
        {
          id: "seed-u1",
          role: "user",
          content: "seeded",
          createdAt: "2026-07-30T00:00:00.000Z",
        },
      ],
    });

    const user = userEvent.setup();
    renderApp(conversationGateway, [projectA, projectB]);

    await user.click(await screen.findByRole("button", { name: "Project A" }));
    await user.click(await screen.findByRole("button", { name: "A chat" }));
    expect(await screen.findByTestId("conversation-timeline")).toBeVisible();
    expect(screen.getByTestId("conversation-row")).toHaveClass("session-row--active");

    await user.click(screen.getByRole("button", { name: "Project B" }));
    await waitFor(() => {
      expect(screen.queryByTestId("conversation-timeline")).not.toBeInTheDocument();
      expect(screen.queryByText("A chat")).not.toBeInTheDocument();
    });
    expect(screen.getByTestId("selected-workbench")).toBeVisible();
  });

  it("clears conversation scope when opening a registered project", async () => {
    const projectA = fixtureProject({ id: "project-a", name: "Project A" });
    const opened = fixtureProject({
      id: "opened-project",
      name: "Opened Project",
      rootPath: "F:\\work\\opened",
    });
    const conversationGateway = new MockConversationGateway({ stepDelayMs: 0 });
    const chat = await conversationGateway.createConversation("project-a", "Stay behind");
    conversationGateway.seedTimeline(chat.id, {
      messages: [
        {
          id: "seed-u1",
          role: "user",
          content: "stay",
          createdAt: "2026-07-30T00:00:00.000Z",
        },
      ],
    });

    const user = userEvent.setup();
    render(
      <App
        conversationGateway={conversationGateway}
        gateway={fixtureProjectGateway([projectA], { openProject: opened })}
      />,
    );

    await user.click(await screen.findByRole("button", { name: "Project A" }));
    await user.click(await screen.findByRole("button", { name: "Stay behind" }));
    expect(await screen.findByTestId("conversation-timeline")).toBeVisible();

    await user.click(screen.getByTestId("nav-open-project"));
    await waitFor(() => {
      expect(screen.getByTestId("active-project-name")).toHaveTextContent(
        "Opened Project",
      );
    });
    expect(screen.queryByTestId("conversation-timeline")).not.toBeInTheDocument();
    expect(screen.queryByText("Stay behind")).not.toBeInTheDocument();
  });

  it("shows 模拟运行时 before the first send when mock gateway is active", async () => {
    const conversationGateway = new MockConversationGateway({ stepDelayMs: 0 });
    await selectProjectAfterRender(conversationGateway);

    expect(await screen.findByTestId("mock-runtime-badge")).toHaveTextContent(
      MOCK_RUNTIME_BADGE,
    );
    expect(screen.queryByTestId("conversation-timeline")).not.toBeInTheDocument();
  });

  it("keeps composer geometry stable while streaming deltas", async () => {
    const conversationGateway = new MockConversationGateway({ stepDelayMs: 0 });
    const user = await selectProjectAfterRender(conversationGateway);

    await user.click(screen.getByRole("button", { name: "新建对话" }));
    const composer = screen.getByTestId("composer");
    const before = composer.getBoundingClientRect();

    await user.type(
      await screen.findByRole("textbox", { name: "工作请求" }),
      "几何稳定",
    );
    await user.click(screen.getByTestId("composer-send"));
    await drainMock(conversationGateway);

    const after = composer.getBoundingClientRect();
    expect(after.top).toBe(before.top);
    expect(after.height).toBe(before.height);
    expect(after.left).toBe(before.left);
    expect(after.width).toBe(before.width);
  });

  it("recovers draft and surfaces failure without a successful local message", async () => {
    const conversationGateway = new MockConversationGateway({ stepDelayMs: 0 });
    conversationGateway.failNextSend("network down");
    const user = await selectProjectAfterRender(conversationGateway);

    await user.click(screen.getByRole("button", { name: "新建对话" }));
    const textarea = await screen.findByRole("textbox", { name: "工作请求" });
    await user.type(textarea, "这会失败");
    await user.click(screen.getByTestId("composer-send"));

    expect(await screen.findByTestId("composer-send-error")).toHaveTextContent(
      /network down|失败/,
    );
    expect(textarea).toHaveValue("这会失败");
    expect(screen.queryByTestId("timeline-user-message")).not.toBeInTheDocument();
  });

  it("renders multiple turns in chronological order", async () => {
    const conversationGateway = new MockConversationGateway({ stepDelayMs: 0 });
    const user = await selectProjectAfterRender(conversationGateway);

    await user.click(screen.getByRole("button", { name: "新建对话" }));
    await user.type(
      await screen.findByRole("textbox", { name: "工作请求" }),
      "第一轮",
    );
    await user.click(screen.getByTestId("composer-send"));
    await drainMock(conversationGateway);

    await waitFor(() => {
      expect(screen.getByTestId("run-status")).toHaveTextContent(/已完成/);
    });

    await user.type(
      await screen.findByRole("textbox", { name: "工作请求" }),
      "第二轮",
    );
    await user.click(screen.getByTestId("composer-send"));
    await drainMock(conversationGateway);

    await waitFor(() => {
      const users = screen.getAllByTestId("timeline-user-message");
      const assistants = screen.getAllByTestId("timeline-assistant-message");
      expect(users).toHaveLength(2);
      expect(assistants).toHaveLength(2);
      expect(users[0]).toHaveTextContent("第一轮");
      expect(users[1]).toHaveTextContent("第二轮");
    });

    const timeline = screen.getByTestId("conversation-timeline");
    const ordered = within(timeline)
      .getAllByTestId(/timeline-(user|assistant)-message/)
      .map((node) => node.getAttribute("data-testid"));
    expect(ordered).toEqual([
      "timeline-user-message",
      "timeline-assistant-message",
      "timeline-user-message",
      "timeline-assistant-message",
    ]);
  });

  it("preserves Slice A empty and selected empty-conversation surfaces", async () => {
    const conversationGateway = new MockConversationGateway({ stepDelayMs: 0 });
    renderApp(conversationGateway, []);

    expect(await screen.findByTestId("empty-workbench")).toBeVisible();
    expect(screen.queryByTestId("mock-runtime-badge")).not.toBeInTheDocument();
    expect(screen.queryByTestId("conversation-timeline")).not.toBeInTheDocument();

    const withProject = new MockConversationGateway({ stepDelayMs: 0 });
    renderApp(withProject, [fixtureProject({ name: "Keeper IDE", id: "keeper-ide" })]);
    await userEvent.setup().click(await screen.findByRole("button", { name: "Keeper IDE" }));

    expect(await screen.findByTestId("selected-workbench")).toBeVisible();
    expect(screen.getByTestId("project-conversations")).toBeVisible();
    expect(screen.getByText(/尚无对话/)).toBeVisible();
    // Mock badge is intentional on selected project with mock gateway (before first send).
    expect(screen.getByTestId("mock-runtime-badge")).toBeVisible();
  });
});

describe("composer contracts", () => {
  it("ignores Enter while IME composition is active", async () => {
    const onSend = vi.fn().mockResolvedValue(undefined);
    render(<Composer onSend={onSend} projectName="Demo" />);
    const textarea = screen.getByRole("textbox", { name: "工作请求" });
    await userEvent.type(textarea, "你好");

    textarea.dispatchEvent(
      new CompositionEvent("compositionstart", { bubbles: true }),
    );
    textarea.dispatchEvent(
      new KeyboardEvent("keydown", {
        key: "Enter",
        bubbles: true,
        cancelable: true,
        // IME uses keyCode 229 while composing
        // eslint-disable-next-line @typescript-eslint/no-explicit-any
        ...({ keyCode: 229 } as any),
      }),
    );
    expect(onSend).not.toHaveBeenCalled();

    textarea.dispatchEvent(
      new CompositionEvent("compositionend", { bubbles: true }),
    );
  });

  it("restores draft when onSend rejects", async () => {
    const onSend = vi.fn().mockRejectedValue(new Error("boom"));
    const user = userEvent.setup();
    render(<Composer onSend={onSend} projectName="Demo" sendError="boom" />);
    const textarea = screen.getByRole("textbox", { name: "工作请求" });
    await user.type(textarea, "retry me");
    await user.click(screen.getByTestId("composer-send"));
    await waitFor(() => {
      expect(textarea).toHaveValue("retry me");
    });
    expect(screen.getByTestId("composer-send-error")).toHaveTextContent("boom");
  });
});

describe("timeline scale", () => {
  it("window-renders long histories without mounting every message", () => {
    const count = TIMELINE_WINDOW_THRESHOLD + 40;
    const messages = Array.from({ length: count }, (_, index) => ({
      id: `m-${index}`,
      role: (index % 2 === 0 ? "user" : "assistant") as "user" | "assistant",
      content: `Message ${index}`,
      createdAt: `2026-07-30T00:${String(index % 60).padStart(2, "0")}:00.000Z`,
    }));
    const timeline: ConversationTimelineView = {
      conversationId: "c-scale",
      messages,
      events: [],
      seenEventKeys: new Set(),
      assistantText: "",
      reasoningText: "",
      plan: null,
      tools: [],
      subagents: [],
      activeRunId: null,
    };

    render(
      <ConversationTimeline run={null} timeline={timeline} />,
    );

    expect(screen.getByTestId("conversation-timeline")).toHaveAttribute(
      "data-windowed",
      "true",
    );
    const mounted = screen.getAllByTestId(/timeline-(user|assistant)-message/);
    expect(mounted.length).toBeLessThan(count);
    expect(mounted.length).toBeLessThanOrEqual(60);
    expect(screen.getByTestId("timeline-window")).toBeInTheDocument();
  });
});
