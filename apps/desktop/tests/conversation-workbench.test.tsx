/**
 * Slice B1 conversation workbench tests + Wave B1 UI fix-pass regressions.
 */
import { act, render, screen, waitFor, within } from "@testing-library/react";
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
import type {
  ConversationGateway,
  RuntimeEventDto,
} from "../src/lib/conversationGateway";
import { isTerminalKind } from "../src/lib/conversationGateway";
import {
  ProjectGatewayError,
  type ProjectDto,
  type ProjectGateway,
} from "../src/lib/projectGateway";
import { Composer } from "../src/features/workbench/Composer";
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

function fixtureProject(overrides: Partial<ProjectDto> = {}): ProjectDto {
  return {
    id: "project-a",
    name: "Project A",
    rootPath: "F:\\work\\project-a",
    trust: "trusted",
    createdAt: "2026-07-29T00:00:00.000Z",
    updatedAt: "2026-07-29T00:00:00.000Z",
    ...overrides,
  };
}

function fixtureProjectGateway(
  projects: ProjectDto[] = [fixtureProject()],
  options: {
    openProject?: ProjectDto;
    createProject?: ProjectDto;
  } = {},
): ProjectGateway {
  const store = [...projects];
  return {
    listProjects: async () => [...store],
    createProject: async (input) => {
      const project = fixtureProject({
        id: crypto.randomUUID(),
        name: input.name,
        rootPath: input.rootPath,
      });
      store.push(project);
      return project;
    },
    selectProject: async (projectId) => {
      const project = store.find((item) => item.id === projectId);
      if (!project) {
        throw new ProjectGatewayError("project_not_found", "Project not found.");
      }
      return project;
    },
    pickDirectory: async () => "F:\\work\\picked",
    openOrRegisterProject: async () => {
      const project =
        options.openProject
        ?? fixtureProject({
          id: "opened-project",
          name: "Opened Project",
          rootPath: "F:\\work\\opened",
        });
      if (!store.some((item) => item.id === project.id)) store.push(project);
      return project;
    },
    createProjectWithFolder: async (input) => {
      const project =
        options.createProject
        ?? fixtureProject({
          id: "created-project",
          name: input.name,
          rootPath: `F:\\work\\${input.name}`,
        });
      if (!store.some((item) => item.id === project.id)) store.push(project);
      return project;
    },
  };
}

function renderApp(
  conversationGateway: ConversationGateway,
  projects: ProjectDto[] = [fixtureProject()],
  projectGateway?: ProjectGateway,
) {
  return render(
    <App
      conversationGateway={conversationGateway}
      gateway={projectGateway ?? fixtureProjectGateway(projects)}
    />,
  );
}

async function selectProject(name = "Project A") {
  const user = userEvent.setup();
  await user.click(await screen.findByRole("button", { name }));
  return user;
}

async function drainMock(gateway: MockConversationGateway) {
  await act(async () => {
    await gateway.drain();
  });
}

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

  it("clears conversation scope when creating a project folder", async () => {
    const projectA = fixtureProject({ id: "project-a", name: "Project A" });
    const created = fixtureProject({
      id: "created-project",
      name: "Brand New",
      rootPath: "F:\\work\\Brand New",
    });
    const conversationGateway = new MockConversationGateway({ stepDelayMs: 0 });
    const chat = await conversationGateway.createConversation("project-a", "Old chat");
    conversationGateway.seedTimeline(chat.id, {
      messages: [
        {
          id: "seed-u1",
          role: "user",
          content: "old",
          createdAt: "2026-07-30T00:00:00.000Z",
        },
      ],
    });

    const user = userEvent.setup();
    render(
      <App
        conversationGateway={conversationGateway}
        gateway={fixtureProjectGateway([projectA], { createProject: created })}
      />,
    );

    await user.click(await screen.findByRole("button", { name: "Project A" }));
    await user.click(await screen.findByRole("button", { name: "Old chat" }));
    expect(await screen.findByTestId("conversation-timeline")).toBeVisible();

    await user.click(screen.getByTestId("nav-new-project"));
    await user.type(await screen.findByLabelText("项目名称"), "Brand New");
    await user.click(screen.getByRole("button", { name: "选择位置并创建" }));

    await waitFor(() => {
      expect(screen.getByTestId("active-project-name")).toHaveTextContent("Brand New");
    });
    expect(screen.queryByTestId("conversation-timeline")).not.toBeInTheDocument();
    expect(screen.queryByText("Old chat")).not.toBeInTheDocument();
  });

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
    expect(screen.getByTestId("run-activity")).toBeVisible();
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

  it("shows 模拟运行时 before the first send when mock gateway is active", async () => {
    const conversationGateway = new MockConversationGateway({ stepDelayMs: 0 });
    await selectProjectAfterRender(conversationGateway);

    expect(await screen.findByTestId("mock-runtime-badge")).toHaveTextContent(
      MOCK_RUNTIME_BADGE,
    );
    expect(screen.queryByTestId("conversation-timeline")).not.toBeInTheDocument();
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

async function selectProjectAfterRender(gateway: ConversationGateway) {
  renderApp(gateway);
  return selectProject("Project A");
}
