/**
 * Slice B1 Task 3 — conversation workbench RED/GREEN tests.
 * Covers project isolation, streaming, activity, cancellation, and background runs.
 */
import { act, render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, describe, expect, it, vi } from "vitest";
import { App } from "../src/App";
import {
  createInitialConversationState,
  conversationReducer,
  type ConversationAction,
  type ConversationUiState,
} from "../src/features/conversations/conversationReducer";
import {
  MockConversationGateway,
  MOCK_RUNTIME_BADGE,
} from "../src/lib/mockConversationGateway";
import type {
  ConversationGateway,
  RuntimeEventDto,
} from "../src/lib/conversationGateway";
import {
  ProjectGatewayError,
  type ProjectDto,
  type ProjectGateway,
} from "../src/lib/projectGateway";
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
    pickDirectory: async () => null,
    openOrRegisterProject: async () => store[0]!,
    createProjectWithFolder: async () => store[0]!,
  };
}

function renderApp(
  conversationGateway: ConversationGateway,
  projects: ProjectDto[] = [fixtureProject()],
) {
  return render(
    <App
      conversationGateway={conversationGateway}
      gateway={fixtureProjectGateway(projects)}
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

    const timeline = await gateway.loadTimeline(conversation.id);
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
    // Selecting another conversation is a UI concern; gateway continues by run id.
    await gateway.createConversation("project-a", "Third");
    await gateway.drain();

    const firstEvents = byConversation.get(first.id) ?? [];
    expect(firstEvents.some((event) => event.kind === "assistant_delta")).toBe(true);
    expect(firstEvents.some((event) => event.kind === "turn_completed")).toBe(true);
    expect(byConversation.get(second.id) ?? []).toHaveLength(0);
  });
});

describe("conversation reducer", () => {
  it("applies deltas without duplicating sequences and tracks run status", () => {
    let state = createInitialConversationState();
    state = dispatch(state, {
      type: "conversations_loaded",
      projectId: "p1",
      conversations: [
        {
          id: "c1",
          projectId: "p1",
          title: "T",
          createdAt: "t0",
          updatedAt: "t0",
        },
      ],
    });
    state = dispatch(state, { type: "conversation_selected", conversationId: "c1" });
    state = dispatch(state, {
      type: "timeline_loaded",
      timeline: { conversationId: "c1", messages: [], events: [] },
    });

    const base = {
      eventId: "e1",
      runId: "r1",
      conversationId: "c1",
      sequence: 1,
      kind: "run_started" as const,
      payload: {},
      createdAt: "t1",
    };
    state = dispatch(state, { type: "runtime_event", event: base });
    state = dispatch(state, {
      type: "runtime_event",
      event: {
        ...base,
        eventId: "e2",
        sequence: 2,
        kind: "assistant_delta",
        payload: { text: "Hello" },
      },
    });
    // Duplicate sequence is ignored.
    state = dispatch(state, {
      type: "runtime_event",
      event: {
        ...base,
        eventId: "e2-dup",
        sequence: 2,
        kind: "assistant_delta",
        payload: { text: "Hello" },
      },
    });
    state = dispatch(state, {
      type: "runtime_event",
      event: {
        ...base,
        eventId: "e3",
        sequence: 3,
        kind: "turn_completed",
        payload: {},
      },
    });

    const timeline = state.timelines.c1;
    expect(timeline?.events).toHaveLength(3);
    expect(timeline?.assistantText).toContain("Hello");
    expect(state.runs.r1?.status).toBe("completed");
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

  it("sends a turn, streams activity into the timeline, and completes", async () => {
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
    expect(screen.getByTestId("run-activity-tool")).toBeVisible();
    expect(screen.getByTestId("run-activity-subagent")).toBeVisible();
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
    // Second conversation row should appear; select the first conversation again later.
    expect(secondTitle.length).toBeGreaterThanOrEqual(1);

    await drainMock(conversationGateway);

    // Open first conversation from the list (first non-empty titled row under project).
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
    expect(screen.queryByTestId("mock-runtime-badge")).not.toBeInTheDocument();
  });
});

function isTerminalKind(kind: RuntimeEventDto["kind"]): boolean {
  return kind === "turn_completed" || kind === "turn_failed" || kind === "run_cancelled";
}

function dispatch(
  state: ConversationUiState,
  action: ConversationAction,
): ConversationUiState {
  return conversationReducer(state, action);
}

async function selectProjectAfterRender(gateway: ConversationGateway) {
  renderApp(gateway);
  return selectProject("Project A");
}
