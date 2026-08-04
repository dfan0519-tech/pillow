/**
 * Wave B1 UI Fix Pass 2/3 regressions:
 * project-scope create race, stale-send isolation, accepted-send vs refresh,
 * windowing follow.
 */
import { act, render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it, vi } from "vitest";
import { App } from "../src/App";
import {
  ConversationTimeline,
  TIMELINE_WINDOW_THRESHOLD,
} from "../src/features/conversations/ConversationTimeline";
import type { ConversationTimelineView } from "../src/features/conversations/conversationReducer";
import { MockConversationGateway } from "../src/lib/mockConversationGateway";
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

function fixtureProjectGateway(projects: ProjectDto[]): ProjectGateway {
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
    openOrRegisterProject: async () => store[0]!,
  };
}

function makeLongTimeline(count: number): ConversationTimelineView {
  const messages = Array.from({ length: count }, (_, index) => ({
    id: `m-${index}`,
    role: (index % 2 === 0 ? "user" : "assistant") as "user" | "assistant",
    content: `Message ${index}`,
    createdAt: `2026-07-30T00:${String(index % 60).padStart(2, "0")}:00.000Z`,
  }));
  return {
    conversationId: "c-scale",
    messages,
    events: [],
    seenEventKeys: new Set(),
    assistantText: "",
    reasoningText: "",
    plan: null,
    tools: [],
    subagents: [],
    activitiesByRunId: {},
    activeRunId: null,
  };
}

describe("first-send createConversation project scope race", () => {
  it("does not select a deferred create result after the active project changes", async () => {
    const projectA = fixtureProject({ id: "project-a", name: "Project A" });
    const projectB = fixtureProject({
      id: "project-b",
      name: "Project B",
      rootPath: "F:\\work\\project-b",
    });
    const conversationGateway = new MockConversationGateway({ stepDelayMs: 0 });
    const releaseCreate = conversationGateway.holdNextCreate();

    const user = userEvent.setup();
    render(
      <App
        conversationGateway={conversationGateway}
        gateway={fixtureProjectGateway([projectA, projectB])}
      />,
    );

    await user.click(await screen.findByRole("button", { name: "Project A" }));
    const textarea = await screen.findByRole("textbox", { name: "工作请求" });
    await user.type(textarea, "race prompt");
    await user.click(screen.getByTestId("composer-send"));

    // Create is still pending — switch project before it resolves.
    await user.click(screen.getByRole("button", { name: "Project B" }));
    await waitFor(() => {
      expect(screen.getByTestId("active-project-name")).toHaveTextContent(
        "Project B",
      );
    });

    await act(async () => {
      releaseCreate();
    });

    // Stale create must not drive project B selection / timeline / cancel.
    await waitFor(() => {
      expect(screen.queryByTestId("conversation-timeline")).not.toBeInTheDocument();
      expect(screen.queryByTestId("composer-cancel")).not.toBeInTheDocument();
    });
    expect(screen.getByTestId("selected-workbench")).toBeVisible();
    expect(screen.queryByTestId("conversation-row")).not.toBeInTheDocument();

    // Conversation belongs to project A when revisiting.
    await user.click(screen.getByRole("button", { name: "Project A" }));
    await waitFor(() => {
      const rows = screen.queryAllByTestId("conversation-row");
      expect(rows.length).toBeGreaterThanOrEqual(1);
    });
    // Must not auto-select the raced conversation as the active timeline.
    expect(screen.queryByTestId("conversation-timeline")).not.toBeInTheDocument();
  });

  it("isolates deferred create completion: no B draft restore, send error, timeline, or cancel", async () => {
    const projectA = fixtureProject({ id: "project-a", name: "Project A" });
    const projectB = fixtureProject({
      id: "project-b",
      name: "Project B",
      rootPath: "F:\\work\\project-b",
    });
    const conversationGateway = new MockConversationGateway({ stepDelayMs: 0 });
    const sendTurnSpy = vi.spyOn(conversationGateway, "sendTurn");
    const releaseCreate = conversationGateway.holdNextCreate();

    const user = userEvent.setup();
    render(
      <App
        conversationGateway={conversationGateway}
        gateway={fixtureProjectGateway([projectA, projectB])}
      />,
    );

    await user.click(await screen.findByRole("button", { name: "Project A" }));
    const textarea = await screen.findByRole("textbox", { name: "工作请求" });
    await user.type(textarea, "stale draft prompt");
    await user.click(screen.getByTestId("composer-send"));
    // Composer clears draft optimistically before create resolves.
    expect(textarea).toHaveValue("");

    await user.click(screen.getByRole("button", { name: "Project B" }));
    await waitFor(() => {
      expect(screen.getByTestId("active-project-name")).toHaveTextContent(
        "Project B",
      );
    });

    // Project B starts empty; resolve Project A's deferred create while on B.
    const bTextarea = screen.getByRole("textbox", { name: "工作请求" });
    expect(bTextarea).toHaveValue("");

    await act(async () => {
      releaseCreate();
    });

    // Allow any residual promise settlement to surface into state/composer.
    await act(async () => {
      await Promise.resolve();
      await Promise.resolve();
    });

    await waitFor(() => {
      expect(screen.getByTestId("active-project-name")).toHaveTextContent(
        "Project B",
      );
    });

    // Isolation: B must stay empty of A's turn UI, error, and restored draft.
    expect(screen.queryByTestId("conversation-timeline")).not.toBeInTheDocument();
    expect(screen.queryByTestId("composer-cancel")).not.toBeInTheDocument();
    expect(screen.queryByTestId("composer-send-error")).not.toBeInTheDocument();
    expect(bTextarea).toHaveValue("");
    // Must not silently execute the stale turn in either project.
    expect(sendTurnSpy).not.toHaveBeenCalled();
    expect(screen.queryByTestId("conversation-row")).not.toBeInTheDocument();
  });
});

describe("accepted send vs timeline refresh", () => {
  it("does not surface loadTimeline failure as send failure or restore draft", async () => {
    const conversationGateway = new MockConversationGateway({ stepDelayMs: 0 });
    conversationGateway.failNextLoadTimeline("timeline boom");

    const user = userEvent.setup();
    render(
      <App
        conversationGateway={conversationGateway}
        gateway={fixtureProjectGateway([fixtureProject()])}
      />,
    );
    await user.click(await screen.findByRole("button", { name: "Project A" }));

    const textarea = await screen.findByRole("textbox", { name: "工作请求" });
    await user.type(textarea, "accepted once");
    await user.click(screen.getByTestId("composer-send"));

    await waitFor(() => {
      expect(screen.getByTestId("composer-refresh-warning")).toBeVisible();
    });
    expect(screen.queryByTestId("composer-send-error")).not.toBeInTheDocument();
    // Draft stays cleared — retry would be a deliberate new send, not auto-restore.
    expect(textarea).toHaveValue("");

    await act(async () => {
      await conversationGateway.drain();
    });

    // Stream still applies; one accepted run only.
    await waitFor(() => {
      expect(screen.getByTestId("agent-activity-summary")).toHaveTextContent(/已完成/);
    });
  });

  it("retry after accepted send creates a new run rather than duplicating via draft restore", async () => {
    const conversationGateway = new MockConversationGateway({ stepDelayMs: 0 });
    conversationGateway.failNextLoadTimeline("refresh failed");
    const runIds: string[] = [];
    conversationGateway.subscribe((event) => {
      if (event.kind === "run_started" && !runIds.includes(event.runId)) {
        runIds.push(event.runId);
      }
    });

    const user = userEvent.setup();
    render(
      <App
        conversationGateway={conversationGateway}
        gateway={fixtureProjectGateway([fixtureProject()])}
      />,
    );
    await user.click(await screen.findByRole("button", { name: "Project A" }));

    const textarea = await screen.findByRole("textbox", { name: "工作请求" });
    await user.type(textarea, "same text");
    await user.click(screen.getByTestId("composer-send"));

    await waitFor(() => {
      expect(screen.getByTestId("composer-refresh-warning")).toBeVisible();
    });
    expect(textarea).toHaveValue("");
    await act(async () => {
      await conversationGateway.drain();
    });
    expect(runIds).toHaveLength(1);

    // User explicitly re-types and sends — second run, not a silent double-accept.
    await user.type(textarea, "same text");
    await user.click(screen.getByTestId("composer-send"));
    await act(async () => {
      await conversationGateway.drain();
    });
    await waitFor(() => {
      expect(runIds).toHaveLength(2);
    });
    expect(runIds[0]).not.toBe(runIds[1]);
  });
});

describe("paginated timeline newest page and follow", () => {
  it("initial page shows newest messages, not the oldest page", () => {
    const count = TIMELINE_WINDOW_THRESHOLD + 40;
    const timeline = makeLongTimeline(count);

    render(<ConversationTimeline run={null} timeline={timeline} />);

    const page = screen.getByTestId("timeline-page");
    expect(page).toBeInTheDocument();

    const mounted = within(page).getAllByTestId(
      /timeline-(user|assistant)-message/,
    );
    const texts = mounted.map((node) => node.textContent ?? "");
    expect(texts.some((t) => t.includes(`Message ${count - 1}`))).toBe(true);
    expect(texts.some((t) => t.includes("Message 0"))).toBe(false);
  });

  it("follows new content on the recent page", async () => {
    const baseCount = TIMELINE_WINDOW_THRESHOLD + 10;
    const timeline = makeLongTimeline(baseCount);
    const { rerender } = render(
      <ConversationTimeline run={null} timeline={timeline} />,
    );

    expect(
      screen.getByText(`Message ${baseCount - 1}`, { exact: false }),
    ).toBeInTheDocument();

    const grown = makeLongTimeline(baseCount + 8);
    rerender(<ConversationTimeline run={null} timeline={grown} />);

    await waitFor(() => {
      expect(
        screen.getByText(`Message ${baseCount + 7}`, { exact: false }),
      ).toBeInTheDocument();
    });
  });

  it("loads older pages explicitly without virtual spacer content", async () => {
    const count = TIMELINE_WINDOW_THRESHOLD + 40;
    const timeline = makeLongTimeline(count);
    render(<ConversationTimeline run={null} timeline={timeline} />);

    expect(screen.queryByTestId("timeline-pad-top")).not.toBeInTheDocument();
    expect(screen.queryByTestId("timeline-pad-bottom")).not.toBeInTheDocument();
    await userEvent.setup().click(
      screen.getByRole("button", { name: "加载更早消息" }),
    );

    // One explicit page adds exactly 40 older records.
    const page = screen.getByTestId("timeline-page");
    const mounted = within(page).getAllByTestId(
      /timeline-(user|assistant)-message/,
    );
    expect(mounted).toHaveLength(80);
    expect(
      mounted.some((node) => node.textContent?.includes("Message 0")),
    ).toBe(false);
  });
});

describe("cancelled run activity UI", () => {
  it("shows cancelled status without running child labels", async () => {
    const conversationGateway = new MockConversationGateway({ stepDelayMs: 40 });
    const user = userEvent.setup();
    render(
      <App
        conversationGateway={conversationGateway}
        gateway={fixtureProjectGateway([fixtureProject()])}
      />,
    );
    await user.click(await screen.findByRole("button", { name: "Project A" }));
    await user.type(
      await screen.findByRole("textbox", { name: "工作请求" }),
      "cancel children",
    );
    await user.click(screen.getByTestId("composer-send"));

    await waitFor(() => {
      expect(screen.getByTestId("composer-cancel")).toBeEnabled();
    });
    await user.click(screen.getByTestId("composer-cancel"));
    await act(async () => {
      await conversationGateway.drain();
    });

    await waitFor(() => {
      expect(screen.getByTestId("agent-activity-summary")).toHaveTextContent(/已取消/);
    });
    const activity = screen.getByTestId("agent-activity-disclosure");
    expect(activity).not.toHaveTextContent(/^running$/i);
    expect(within(activity).queryByText("running")).not.toBeInTheDocument();
    expect(within(activity).queryByText("进行中")).not.toBeInTheDocument();
  });
});
