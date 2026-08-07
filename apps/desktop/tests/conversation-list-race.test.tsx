/**
 * C4 — conversations_loaded out-of-order race (D3B3 cancel-tree flake root cause).
 * Stale list snapshots must not replace a fresher projection for the same project.
 */
import { act, render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it, vi } from "vitest";
import { App } from "../src/App";
import {
  conversationBelongsToProject,
  conversationReducer,
  createInitialConversationState,
  isRunActive,
  selectActiveRun,
  shouldApplyConversationListSeq,
} from "../src/features/conversations/conversationReducer";
import type {
  ConversationDto,
  ConversationGateway,
  RuntimeEventDto,
  TimelineDto,
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

function conv(
  id: string,
  projectId: string,
  title: string,
): ConversationDto {
  return {
    id,
    projectId,
    title,
    createdAt: "2026-08-07T00:00:00.000Z",
    updatedAt: "2026-08-07T00:00:00.000Z",
  };
}

describe("conversation list freshness (reducer)", () => {
  it("discards an older listSeq so a later-arriving stale snapshot cannot overwrite", () => {
    const projectId = "project-a";
    const older = [conv("c-old", projectId, "Older snapshot")];
    const newer = [
      conv("c-old", projectId, "Older snapshot"),
      conv("c-new", projectId, "New conversation"),
    ];

    let state = createInitialConversationState();
    state = conversationReducer(state, {
      type: "conversations_loaded",
      projectId,
      conversations: newer,
      listSeq: 2,
    });
    expect(state.conversationsByProject[projectId]).toEqual(newer);
    expect(state.conversationListSeqByProject[projectId]).toBe(2);

    // Same project, strictly older sequence — must not replace.
    const afterStale = conversationReducer(state, {
      type: "conversations_loaded",
      projectId,
      conversations: older,
      listSeq: 1,
    });
    expect(afterStale).toBe(state);
    expect(afterStale.conversationsByProject[projectId]).toEqual(newer);
    expect(afterStale.conversationListSeqByProject[projectId]).toBe(2);

    // Equal sequence also discarded (not strictly fresher).
    expect(shouldApplyConversationListSeq(2, 2)).toBe(false);
    expect(shouldApplyConversationListSeq(2, 1)).toBe(false);
    expect(shouldApplyConversationListSeq(2, 3)).toBe(true);
  });
});

describe("conversation list race (accepted send vs late project-effect list)", () => {
  it("keeps the newly created conversation selected after a late older list lands", async () => {
    const projectA = fixtureProject({ id: "project-a", name: "Project A" });
    const convId = "conv-created-by-send";
    const created: ConversationDto = conv(convId, projectA.id, "race prompt");

    type ListResolvers = Array<{
      projectId: string;
      resolve: (value: ConversationDto[]) => void;
    }>;
    const pendingLists: ListResolvers = [];
    const listeners = new Set<(event: RuntimeEventDto) => void>();

    const conversationGateway: ConversationGateway = {
      listConversations: async (projectId) =>
        new Promise<ConversationDto[]>((resolve) => {
          pendingLists.push({ projectId, resolve });
        }),
      createConversation: async (projectId, title) => ({
        id: convId,
        projectId,
        title: title?.trim() || "新对话",
        createdAt: "2026-08-07T00:01:00.000Z",
        updatedAt: "2026-08-07T00:01:00.000Z",
      }),
      loadTimeline: async (conversationId): Promise<TimelineDto> => ({
        conversationId,
        messages: [
          {
            id: "msg-u1",
            role: "user",
            content: "race prompt",
            createdAt: "2026-08-07T00:01:00.000Z",
          },
        ],
        events: [],
        runs: [
          {
            runId: "run-race-1",
            conversationId,
            status: "running",
          },
        ],
      }),
      sendTurn: async (conversationId) => {
        const event: RuntimeEventDto = {
          eventId: "evt-run-race-1",
          kind: "run_started",
          runId: "run-race-1",
          conversationId,
          sequence: 1,
          createdAt: "2026-08-07T00:01:01.000Z",
          payload: {},
        };
        queueMicrotask(() => {
          for (const listener of listeners) listener(event);
        });
        return { runId: "run-race-1" };
      },
      cancelRun: async () => undefined,
      subscribe: (listener) => {
        listeners.add(listener);
        return () => {
          listeners.delete(listener);
        };
      },
    };

    const user = userEvent.setup();
    render(
      <App
        conversationGateway={conversationGateway}
        gateway={fixtureProjectGateway([projectA])}
      />,
    );

    await user.click(await screen.findByRole("button", { name: "Project A" }));

    // Project-effect list is pending (seq 1).
    await waitFor(() => {
      expect(pendingLists.length).toBeGreaterThanOrEqual(1);
    });
    expect(pendingLists[0]?.projectId).toBe(projectA.id);

    const textarea = await screen.findByRole("textbox", { name: "工作请求" });
    await user.type(textarea, "race prompt");
    await user.click(screen.getByTestId("composer-send"));

    // Send path list is pending (seq 2).
    await waitFor(() => {
      expect(pendingLists.length).toBeGreaterThanOrEqual(2);
    });

    // Fresher send-path list lands first (includes the new conversation).
    await act(async () => {
      const sendPathList = pendingLists[1]!;
      sendPathList.resolve([created]);
    });

    await waitFor(() => {
      const group = screen.getByRole("group", { name: "项目对话" });
      expect(within(group).getByText("race prompt")).toBeVisible();
    });
    expect(screen.getByTestId("conversation-row")).toHaveAttribute(
      "aria-current",
      "page",
    );

    // Stale project-effect list (empty / pre-create) arrives later.
    await act(async () => {
      pendingLists[0]!.resolve([]);
    });

    await act(async () => {
      await Promise.resolve();
      await Promise.resolve();
    });

    // Must still be listed and selected — not wiped by the older snapshot.
    const group = screen.getByRole("group", { name: "项目对话" });
    expect(within(group).getByText("race prompt")).toBeVisible();
    expect(screen.getByTestId("conversation-row")).toHaveAttribute(
      "aria-current",
      "page",
    );
  });

  it("keeps composer-cancel mounted when a late older list arrives mid-run", async () => {
    const projectA = fixtureProject({ id: "project-a", name: "Project A" });
    const convId = "conv-cancel-race";
    const created: ConversationDto = conv(convId, projectA.id, "long tree");

    type ListResolvers = Array<{
      projectId: string;
      resolve: (value: ConversationDto[]) => void;
    }>;
    const pendingLists: ListResolvers = [];
    const listeners = new Set<(event: RuntimeEventDto) => void>();

    const conversationGateway: ConversationGateway = {
      listConversations: async (projectId) =>
        new Promise<ConversationDto[]>((resolve) => {
          pendingLists.push({ projectId, resolve });
        }),
      createConversation: async (projectId, title) => ({
        id: convId,
        projectId,
        title: title?.trim() || "新对话",
        createdAt: "2026-08-07T00:02:00.000Z",
        updatedAt: "2026-08-07T00:02:00.000Z",
      }),
      loadTimeline: async (conversationId): Promise<TimelineDto> => ({
        conversationId,
        messages: [
          {
            id: "msg-u-long",
            role: "user",
            content: "long tree",
            createdAt: "2026-08-07T00:02:00.000Z",
          },
        ],
        events: [
          {
            eventId: "evt-run-long-1",
            kind: "run_started",
            runId: "run-long-1",
            conversationId,
            sequence: 1,
            createdAt: "2026-08-07T00:02:01.000Z",
            payload: {},
          },
        ],
        runs: [
          {
            runId: "run-long-1",
            conversationId,
            status: "running",
          },
        ],
      }),
      sendTurn: async (conversationId) => {
        const event: RuntimeEventDto = {
          eventId: "evt-run-long-1",
          kind: "run_started",
          runId: "run-long-1",
          conversationId,
          sequence: 1,
          createdAt: "2026-08-07T00:02:01.000Z",
          payload: {},
        };
        queueMicrotask(() => {
          for (const listener of listeners) listener(event);
        });
        return { runId: "run-long-1" };
      },
      cancelRun: async () => undefined,
      subscribe: (listener) => {
        listeners.add(listener);
        return () => {
          listeners.delete(listener);
        };
      },
    };

    const user = userEvent.setup();
    render(
      <App
        conversationGateway={conversationGateway}
        gateway={fixtureProjectGateway([projectA])}
      />,
    );

    await user.click(await screen.findByRole("button", { name: "Project A" }));
    await waitFor(() => expect(pendingLists.length).toBeGreaterThanOrEqual(1));

    const textarea = await screen.findByRole("textbox", { name: "工作请求" });
    await user.type(textarea, "long tree");
    await user.click(screen.getByTestId("composer-send"));

    await waitFor(() => expect(pendingLists.length).toBeGreaterThanOrEqual(2));

    // Apply fresher list + timeline so ownership + active run hold.
    await act(async () => {
      pendingLists[1]!.resolve([created]);
    });

    await waitFor(() => {
      expect(screen.getByTestId("composer-cancel")).toBeVisible();
    });

    // Late older empty list (project-effect) — ownership must not drop.
    await act(async () => {
      pendingLists[0]!.resolve([]);
    });

    await act(async () => {
      await Promise.resolve();
      await Promise.resolve();
    });

    expect(screen.getByTestId("composer-cancel")).toBeVisible();
    expect(screen.getByTestId("conversation-row")).toHaveAttribute(
      "aria-current",
      "page",
    );
  });
});

describe("conversation list project isolation (not weakened by freshness)", () => {
  it("drops a late list result for the previous project after switch", async () => {
    const projectA = fixtureProject({ id: "project-a", name: "Project A" });
    const projectB = fixtureProject({
      id: "project-b",
      name: "Project B",
      rootPath: "F:\\work\\project-b",
    });

    const convA = conv("conv-a-only", projectA.id, "A only");
    const convB = conv("conv-b-only", projectB.id, "B only");

    type ListResolvers = Array<{
      projectId: string;
      resolve: (value: ConversationDto[]) => void;
    }>;
    const pendingLists: ListResolvers = [];

    const conversationGateway: ConversationGateway = {
      listConversations: async (projectId) =>
        new Promise<ConversationDto[]>((resolve) => {
          pendingLists.push({ projectId, resolve });
        }),
      createConversation: async (projectId) => ({
        id: `conv-${projectId}`,
        projectId,
        title: "新对话",
        createdAt: "2026-08-07T00:03:00.000Z",
        updatedAt: "2026-08-07T00:03:00.000Z",
      }),
      loadTimeline: async (conversationId): Promise<TimelineDto> => ({
        conversationId,
        messages: [],
        events: [],
        runs: [],
      }),
      sendTurn: async () => ({ runId: "run-x" }),
      cancelRun: async () => undefined,
      subscribe: () => () => undefined,
    };

    const user = userEvent.setup();
    render(
      <App
        conversationGateway={conversationGateway}
        gateway={fixtureProjectGateway([projectA, projectB])}
      />,
    );

    await user.click(await screen.findByRole("button", { name: "Project A" }));
    await waitFor(() => {
      expect(pendingLists.some((item) => item.projectId === projectA.id)).toBe(
        true,
      );
    });
    const lateA = pendingLists.find((item) => item.projectId === projectA.id)!;

    // Switch before A's list resolves — active flag must drop the late result.
    await user.click(screen.getByRole("button", { name: "Project B" }));
    await waitFor(() => {
      expect(screen.getByTestId("active-project-name")).toHaveTextContent(
        "Project B",
      );
    });
    await waitFor(() => {
      expect(pendingLists.some((item) => item.projectId === projectB.id)).toBe(
        true,
      );
    });
    const listB = pendingLists.filter((item) => item.projectId === projectB.id).at(-1)!;

    await act(async () => {
      listB.resolve([convB]);
    });
    await waitFor(() => {
      const group = screen.getByRole("group", { name: "项目对话" });
      expect(within(group).getByText("B only")).toBeVisible();
    });

    // Late A snapshot must not appear under B and must not poison B's rail.
    await act(async () => {
      lateA.resolve([convA]);
    });
    await act(async () => {
      await Promise.resolve();
      await Promise.resolve();
    });

    const group = screen.getByRole("group", { name: "项目对话" });
    expect(within(group).getByText("B only")).toBeVisible();
    expect(within(group).queryByText("A only")).not.toBeInTheDocument();
    expect(screen.queryByText("A only")).not.toBeInTheDocument();
  });
});

describe("listSeq helper and ownership after stale discard", () => {
  it("preserves selected ownership when stale list is rejected", () => {
    const projectId = "project-a";
    const selected = conv("c-selected", projectId, "Selected");
    let state = createInitialConversationState();
    state = conversationReducer(state, {
      type: "conversations_loaded",
      projectId,
      conversations: [selected],
      listSeq: 5,
    });
    state = conversationReducer(state, {
      type: "conversation_selected",
      conversationId: selected.id,
    });
    state = conversationReducer(state, {
      type: "timeline_loaded",
      timeline: {
        conversationId: selected.id,
        messages: [],
        events: [],
        runs: [
          {
            runId: "run-active",
            conversationId: selected.id,
            status: "running",
          },
        ],
      },
    });

    expect(conversationBelongsToProject(state, selected.id, projectId)).toBe(
      true,
    );
    expect(isRunActive(selectActiveRun(state))).toBe(true);

    state = conversationReducer(state, {
      type: "conversations_loaded",
      projectId,
      conversations: [],
      listSeq: 3,
    });

    expect(conversationBelongsToProject(state, selected.id, projectId)).toBe(
      true,
    );
    expect(isRunActive(selectActiveRun(state))).toBe(true);
    expect(state.conversationListSeqByProject[projectId]).toBe(5);
  });
});
