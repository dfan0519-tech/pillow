/**
 * Wave C3-E — workbench integration: one bar, isolation, stale suppression,
 * grant memory, success-only Open file, no polling.
 */
import {
  act,
  fireEvent,
  render,
  screen,
  waitFor,
} from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, describe, expect, it, vi } from "vitest";
import { App } from "../src/App";
import { Workbench } from "../src/features/workbench/Workbench";
import type {
  ConversationGateway,
  RuntimeEventDto,
} from "../src/lib/conversationGateway";
import {
  ProjectGatewayError,
  type ProjectDto,
  type ProjectGateway,
} from "../src/lib/projectGateway";
import type { ResourceGateway } from "../src/lib/resourceGateway";
import {
  WriteApprovalGatewayError,
  type WriteApprovalGateway,
  type WriteDiffView,
  type WriteProposalView,
} from "../src/lib/writeApprovalGateway";
import type {
  ConversationTimelineView,
  RunView,
} from "../src/features/conversations/conversationReducer";
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

const projectAId = "00000000-0000-0000-0000-0000000000aa";
const projectBId = "00000000-0000-0000-0000-0000000000bb";
const convAId = "00000000-0000-0000-0000-0000000000c1";
const convBId = "00000000-0000-0000-0000-0000000000c2";
/** Canonical 43-char base64url grant — must never appear in DOM/console. */
const grantSecret = "abcdefghijklmnopqrstuvwxyz0123456789ABCDEFG";
const proposalOldId = "prop-old-aaaaaaaa";
const proposalNewId = "prop-new-bbbbbbbb";

function futureExpiry(minutes = 10): string {
  return new Date(Date.now() + minutes * 60_000).toISOString();
}

function fixtureProject(overrides: Partial<ProjectDto> = {}): ProjectDto {
  return {
    id: projectAId,
    name: "Project A",
    rootPath: "F:\\secret\\projects\\alpha",
    trust: "trusted",
    createdAt: "2026-08-01T00:00:00.000Z",
    updatedAt: "2026-08-01T00:00:00.000Z",
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
    openOrRegisterProject: async (rootPath) => {
      const existing = store.find((item) => item.rootPath === rootPath);
      if (existing) return existing;
      const project = fixtureProject({
        id: crypto.randomUUID(),
        name: rootPath.split(/[\\/]/u).pop() ?? rootPath,
        rootPath,
      });
      store.push(project);
      return project;
    },
  };
}

function pending(
  overrides: Partial<WriteProposalView> = {},
): WriteProposalView {
  return {
    proposalId: proposalOldId,
    conversationId: convAId,
    path: "src/example.ts",
    mode: "create",
    state: "pending_approval",
    summary: "Create example",
    expiresAt: futureExpiry(),
    approvalGrant: grantSecret,
    ...overrides,
  };
}

function fixtureDiff(overrides: Partial<WriteDiffView> = {}): WriteDiffView {
  return {
    proposalId: proposalOldId,
    path: "src/example.ts",
    mode: "create",
    unified: "--- /dev/null\n+++ b/src/example.ts\n+export const x = 1;\n",
    truncated: true,
    lineCount: 4000,
    byteCount: 262144,
    ...overrides,
  };
}

function createWriteGateway(
  overrides: Partial<WriteApprovalGateway> & {
    listImpl?: () => Promise<WriteProposalView[]>;
  } = {},
): WriteApprovalGateway & {
  listCalls: string[];
  decideCalls: Array<Record<string, unknown>>;
  diffCalls: Array<Record<string, unknown>>;
  setListResult: (next: WriteProposalView[]) => void;
} {
  const listCalls: string[] = [];
  const decideCalls: Array<Record<string, unknown>> = [];
  const diffCalls: Array<Record<string, unknown>> = [];
  let listResult: WriteProposalView[] = [pending()];

  return {
    listCalls,
    decideCalls,
    diffCalls,
    setListResult: (next) => {
      listResult = next;
    },
    listWriteProposals: async (conversationId) => {
      listCalls.push(conversationId);
      if (overrides.listImpl) return overrides.listImpl();
      return listResult.filter((item) => item.conversationId === conversationId);
    },
    decideWriteProposal: async (input) => {
      decideCalls.push({ ...input });
      listResult = [
        {
          ...pending({
            proposalId: input.proposalId,
            conversationId: input.conversationId,
            state: input.decision === "approve_once" ? "succeeded" : "rejected",
            approvalGrant: undefined,
          }),
        },
      ];
      return listResult[0]!;
    },
    openWriteProposalDiff: async (input) => {
      diffCalls.push({ ...input });
      return fixtureDiff({ proposalId: input.proposalId });
    },
    ...overrides,
  };
}

function emptyTimeline(conversationId: string): ConversationTimelineView {
  return {
    conversationId,
    messages: [
      {
        id: "m1",
        role: "user",
        content: "请创建文件",
        createdAt: "2026-08-01T12:00:00.000Z",
      },
    ],
    events: [],
    seenEventKeys: new Set(),
    assistantText: "",
    reasoningText: "",
    plan: null,
    tools: [],
    subagents: [],
    activitiesByRunId: {
      "run-1": {
        runId: "run-1",
        status: "running",
        startedAt: "2026-08-01T12:00:00.000Z",
        updatedAt: "2026-08-01T12:00:00.000Z",
        assistantText: "",
        reasoningText: "",
        plan: null,
        tools: [],
        subagents: [],
        artifacts: [],
      },
    },
    activeRunId: "run-1",
  };
}

afterEach(() => {
  vi.useRealTimers();
});

describe("write approval workbench integration", () => {
  it("shows one bar for the oldest pending proposal of the selected conversation", async () => {
    const writeGateway = createWriteGateway({
      listImpl: async () => [
        pending({
          proposalId: proposalNewId,
          expiresAt: futureExpiry(30),
          path: "src/new.ts",
        }),
        pending({
          proposalId: proposalOldId,
          expiresAt: futureExpiry(10),
          path: "src/old.ts",
        }),
        pending({
          proposalId: "foreign",
          conversationId: convBId,
          path: "src/foreign.ts",
          expiresAt: futureExpiry(5),
        }),
      ],
    });

    render(
      <Workbench
        conversationId={convAId}
        onCancelRun={vi.fn()}
        onOpenProject={vi.fn()}
        onSendTurn={vi.fn()}
        project={fixtureProject()}
        run={null}
        timeline={emptyTimeline(convAId)}
        writeApprovalGateway={writeGateway}
      />,
    );

    await waitFor(() => {
      expect(screen.getByTestId("write-approval-bar")).toBeInTheDocument();
    });
    expect(screen.getAllByTestId("write-approval-bar")).toHaveLength(1);
    expect(screen.getByTestId("write-approval-path")).toHaveTextContent(
      "src/old.ts",
    );
    expect(screen.queryByText("src/foreign.ts")).toBeNull();
    expect(screen.queryByText("src/new.ts")).toBeNull();
    expect(document.body.textContent).not.toContain(grantSecret);
    expect(document.body.innerHTML).not.toContain(grantSecret);
    expect(document.body.textContent).not.toContain(proposalOldId);
  });

  it("opens governed-write diff in the resource pane with truncation metadata", async () => {
    const user = userEvent.setup();
    const writeGateway = createWriteGateway();
    render(
      <Workbench
        conversationId={convAId}
        onCancelRun={vi.fn()}
        onOpenProject={vi.fn()}
        onSendTurn={vi.fn()}
        project={fixtureProject()}
        run={null}
        timeline={emptyTimeline(convAId)}
        writeApprovalGateway={writeGateway}
      />,
    );

    await waitFor(() => {
      expect(screen.getByTestId("write-approval-review-diff")).toBeInTheDocument();
    });
    await user.click(screen.getByTestId("write-approval-review-diff"));

    await waitFor(() => {
      expect(screen.getByTestId("write-diff-preview")).toBeInTheDocument();
    });
    expect(screen.getByTestId("write-diff-truncated")).toBeInTheDocument();
    expect(screen.getByTestId("write-diff-line-count")).toHaveTextContent(
      "4000 行",
    );
    expect(writeGateway.diffCalls).toEqual([
      { conversationId: convAId, proposalId: proposalOldId },
    ]);
    expect(document.body.innerHTML).not.toContain(grantSecret);
  });

  it("submits exact decide payload then clears grant and re-lists", async () => {
    const user = userEvent.setup();
    const writeGateway = createWriteGateway();
    render(
      <Workbench
        conversationId={convAId}
        onCancelRun={vi.fn()}
        onOpenProject={vi.fn()}
        onSendTurn={vi.fn()}
        project={fixtureProject()}
        run={null}
        timeline={emptyTimeline(convAId)}
        writeApprovalGateway={writeGateway}
      />,
    );

    await waitFor(() => {
      expect(screen.getByTestId("write-approval-approve")).toBeInTheDocument();
    });
    const listBefore = writeGateway.listCalls.length;
    await user.click(screen.getByTestId("write-approval-approve"));

    await waitFor(() => {
      expect(writeGateway.decideCalls.length).toBe(1);
    });
    expect(writeGateway.decideCalls[0]).toEqual({
      conversationId: convAId,
      proposalId: proposalOldId,
      approvalGrant: grantSecret,
      decision: "approve_once",
    });
    await waitFor(() => {
      expect(writeGateway.listCalls.length).toBeGreaterThan(listBefore);
    });
    // After succeeded reconcile, no pending bar.
    await waitFor(() => {
      expect(screen.queryByTestId("write-approval-bar")).toBeNull();
    });
  });

  it("suppresses stale list results after conversation switch", async () => {
    let resolveList: ((value: WriteProposalView[]) => void) | undefined;
    const writeGateway = createWriteGateway({
      listImpl: () =>
        new Promise<WriteProposalView[]>((resolve) => {
          resolveList = resolve;
        }),
    });

    const { rerender } = render(
      <Workbench
        conversationId={convAId}
        onCancelRun={vi.fn()}
        onOpenProject={vi.fn()}
        onSendTurn={vi.fn()}
        project={fixtureProject()}
        run={null}
        timeline={emptyTimeline(convAId)}
        writeApprovalGateway={writeGateway}
      />,
    );

    await waitFor(() => {
      expect(writeGateway.listCalls.length).toBeGreaterThan(0);
    });

    rerender(
      <Workbench
        conversationId={convBId}
        onCancelRun={vi.fn()}
        onOpenProject={vi.fn()}
        onSendTurn={vi.fn()}
        project={fixtureProject()}
        run={null}
        timeline={emptyTimeline(convBId)}
        writeApprovalGateway={writeGateway}
      />,
    );

    // Stale list for A must not paint after B is selected.
    await act(async () => {
      resolveList?.([
        pending({
          conversationId: convAId,
          path: "src/stale-a.ts",
        }),
      ]);
    });

    await waitFor(() => {
      expect(screen.queryByText("src/stale-a.ts")).toBeNull();
    });
  });

  it("does not poll listWriteProposals on a timer", async () => {
    const writeGateway = createWriteGateway();
    render(
      <Workbench
        conversationId={convAId}
        onCancelRun={vi.fn()}
        onOpenProject={vi.fn()}
        onSendTurn={vi.fn()}
        project={fixtureProject()}
        run={null}
        timeline={emptyTimeline(convAId)}
        writeApprovalGateway={writeGateway}
      />,
    );
    await waitFor(() => {
      expect(screen.getByTestId("write-approval-bar")).toBeInTheDocument();
    });
    const afterMount = writeGateway.listCalls.length;
    await act(async () => {
      await new Promise((resolve) => setTimeout(resolve, 120));
    });
    // No interval re-list: count stays flat without refresh token or scope change.
    expect(writeGateway.listCalls.length).toBe(afterMount);
  });

  it("re-lists when writeProposalRefreshToken bumps (event-driven, no poll)", async () => {
    const writeGateway = createWriteGateway();
    const { rerender } = render(
      <Workbench
        conversationId={convAId}
        onCancelRun={vi.fn()}
        onOpenProject={vi.fn()}
        onSendTurn={vi.fn()}
        project={fixtureProject()}
        run={null}
        timeline={emptyTimeline(convAId)}
        writeApprovalGateway={writeGateway}
        writeProposalRefreshToken={0}
      />,
    );
    await waitFor(() => {
      expect(writeGateway.listCalls.length).toBeGreaterThan(0);
    });
    const baseLists = writeGateway.listCalls.length;
    rerender(
      <Workbench
        conversationId={convAId}
        onCancelRun={vi.fn()}
        onOpenProject={vi.fn()}
        onSendTurn={vi.fn()}
        project={fixtureProject()}
        run={null}
        timeline={emptyTimeline(convAId)}
        writeApprovalGateway={writeGateway}
        writeProposalRefreshToken={1}
      />,
    );
    await waitFor(() => {
      expect(writeGateway.listCalls.length).toBeGreaterThan(baseLists);
    });
  });

  it("App bumps write refresh on accepted write_proposal_* for selected conversation", async () => {
    const user = userEvent.setup();
    const writeGateway = createWriteGateway();
    const listeners = new Set<(event: RuntimeEventDto) => void>();
    const conversationGateway: ConversationGateway = {
      listConversations: async (projectId) =>
        projectId === projectAId
          ? [
              {
                id: convAId,
                projectId: projectAId,
                title: "对话 A",
                createdAt: "2026-08-01T00:00:00.000Z",
                updatedAt: "2026-08-01T00:00:00.000Z",
              },
            ]
          : [],
      createConversation: async (projectId) => ({
        id: convAId,
        projectId,
        title: "对话 A",
        createdAt: "2026-08-01T00:00:00.000Z",
        updatedAt: "2026-08-01T00:00:00.000Z",
      }),
      loadTimeline: async () => ({
        conversationId: convAId,
        messages: [
          {
            id: "u1",
            role: "user",
            content: "write please",
            createdAt: "2026-08-01T00:00:00.000Z",
          },
        ],
        events: [],
        runs: [
          { runId: "run-1", conversationId: convAId, status: "running" },
        ],
      }),
      sendTurn: async () => ({ runId: "run-1" }),
      cancelRun: async () => undefined,
      subscribe: (listener) => {
        listeners.add(listener);
        return () => {
          listeners.delete(listener);
        };
      },
    };

    render(
      <App
        conversationGateway={conversationGateway}
        gateway={fixtureProjectGateway()}
        writeApprovalGateway={writeGateway}
      />,
    );

    await waitFor(() => {
      expect(screen.getByText("Project A")).toBeInTheDocument();
    });
    await user.click(screen.getByText("Project A"));
    await waitFor(() => {
      expect(screen.getByText("对话 A")).toBeInTheDocument();
    });
    await user.click(screen.getByText("对话 A"));

    await waitFor(() => {
      expect(writeGateway.listCalls.length).toBeGreaterThan(0);
    });
    const afterSelect = writeGateway.listCalls.length;

    act(() => {
      for (const listener of listeners) {
        listener({
          eventId: "ev-write-req",
          runId: "run-1",
          conversationId: convAId,
          sequence: 3,
          kind: "write_proposal_requested",
          createdAt: "2026-08-01T12:01:00.000Z",
          payload: {
            proposal_id: proposalOldId,
            path: "src/example.ts",
            mode: "create",
            summary: "Create example",
            expires_at: "2026-08-01T12:11:00.000Z",
          },
        });
      }
    });

    await waitFor(() => {
      expect(writeGateway.listCalls.length).toBeGreaterThan(afterSelect);
    });
  });

  it("renders success-only Open file and routes through resource gateway", async () => {
    const preview = vi.fn().mockResolvedValue({
      path: "src/example.ts",
      kind: "text",
      mediaType: "text/plain; charset=utf-8",
      content: "export const x = 1;\n",
      bytesReturned: 20,
      truncated: false,
    });
    const resourceGateway: ResourceGateway = {
      listProjectResources: async () => [],
      readProjectResource: async () => {
        throw new Error("not used");
      },
      searchProjectResources: async () => [],
      listRecentProjectResources: async () => [],
      previewProjectResource: preview,
    };

    const timeline: ConversationTimelineView = {
      ...emptyTimeline(convAId),
      events: [
        {
          eventId: "e-write-1",
          runId: "run-1",
          conversationId: convAId,
          sequence: 2,
          kind: "write_proposal_resolved",
          createdAt: "2026-08-01T12:05:00.000Z",
          payload: {
            proposal_id: proposalOldId,
            path: "src/example.ts",
            mode: "create",
            state: "succeeded",
          },
        },
      ],
      activitiesByRunId: {
        "run-1": {
          runId: "run-1",
          status: "completed",
          startedAt: "2026-08-01T12:00:00.000Z",
          updatedAt: "2026-08-01T12:05:00.000Z",
          assistantText: "done",
          reasoningText: "",
          plan: null,
          tools: [],
          subagents: [],
          artifacts: [],
        },
      },
    };

    const user = userEvent.setup();
    render(
      <Workbench
        conversationId={convAId}
        onCancelRun={vi.fn()}
        onOpenProject={vi.fn()}
        onSendTurn={vi.fn()}
        project={fixtureProject()}
        resourceGateway={resourceGateway}
        run={
          {
            runId: "run-1",
            conversationId: convAId,
            status: "completed",
            terminalKind: "turn_completed",
          } satisfies RunView
        }
        timeline={timeline}
        writeApprovalGateway={createWriteGateway({
          listImpl: async () => [],
        })}
      />,
    );

    await waitFor(() => {
      expect(screen.getByTestId("write-outcome-row")).toBeInTheDocument();
    });
    expect(screen.getByTestId("write-outcome-row")).toHaveTextContent(
      "文件写入已完成",
    );
    expect(screen.getByTestId("write-outcome-open-file")).toBeInTheDocument();
    expect(document.body.textContent).not.toContain(proposalOldId);

    await user.click(screen.getByTestId("write-outcome-open-file"));
    await waitFor(() => {
      expect(preview).toHaveBeenCalledWith({
        projectId: projectAId,
        relativePath: "src/example.ts",
      });
    });
  });

  it("does not offer Open file for non-success terminal outcomes", async () => {
    const timeline: ConversationTimelineView = {
      ...emptyTimeline(convAId),
      events: [
        {
          eventId: "e-write-2",
          runId: "run-1",
          conversationId: convAId,
          sequence: 2,
          kind: "write_proposal_resolved",
          createdAt: "2026-08-01T12:05:00.000Z",
          payload: {
            proposal_id: proposalOldId,
            path: "src/example.ts",
            mode: "create",
            state: "rejected",
          },
        },
      ],
      activitiesByRunId: {
        "run-1": {
          runId: "run-1",
          status: "completed",
          startedAt: "2026-08-01T12:00:00.000Z",
          updatedAt: "2026-08-01T12:05:00.000Z",
          assistantText: "",
          reasoningText: "",
          plan: null,
          tools: [],
          subagents: [],
          artifacts: [],
        },
      },
    };

    render(
      <Workbench
        conversationId={convAId}
        onCancelRun={vi.fn()}
        onOpenProject={vi.fn()}
        onSendTurn={vi.fn()}
        project={fixtureProject()}
        run={null}
        timeline={timeline}
        writeApprovalGateway={createWriteGateway({
          listImpl: async () => [],
        })}
      />,
    );

    await waitFor(() => {
      expect(screen.getByTestId("write-outcome-row")).toHaveTextContent(
        "写入请求已拒绝",
      );
    });
    expect(screen.queryByTestId("write-outcome-open-file")).toBeNull();
    expect(document.body.textContent).not.toContain("src/example.ts");
  });

  it("Escape on the bar does not invoke global cancel", async () => {
    const user = userEvent.setup();
    const onCancelRun = vi.fn();
    const writeGateway = createWriteGateway();
    render(
      <Workbench
        conversationId={convAId}
        onCancelRun={onCancelRun}
        onOpenProject={vi.fn()}
        onSendTurn={vi.fn()}
        project={fixtureProject()}
        run={
          {
            runId: "run-1",
            conversationId: convAId,
            status: "running",
            terminalKind: null,
          } satisfies RunView
        }
        timeline={emptyTimeline(convAId)}
        writeApprovalGateway={writeGateway}
      />,
    );

    await waitFor(() => {
      expect(screen.getByTestId("write-approval-approve")).toHaveFocus();
    });
    await user.keyboard("{Escape}");
    await waitFor(() => {
      expect(writeGateway.decideCalls[0]?.decision).toBe("reject");
    });
    expect(onCancelRun).not.toHaveBeenCalled();
  });

  it("clears the bar when switching projects", async () => {
    const writeGateway = createWriteGateway();
    const { rerender } = render(
      <Workbench
        conversationId={convAId}
        onCancelRun={vi.fn()}
        onOpenProject={vi.fn()}
        onSendTurn={vi.fn()}
        project={fixtureProject()}
        run={null}
        timeline={emptyTimeline(convAId)}
        writeApprovalGateway={writeGateway}
      />,
    );
    await waitFor(() => {
      expect(screen.getByTestId("write-approval-bar")).toBeInTheDocument();
    });

    rerender(
      <Workbench
        conversationId={null}
        onCancelRun={vi.fn()}
        onOpenProject={vi.fn()}
        onSendTurn={vi.fn()}
        project={fixtureProject({ id: projectBId, name: "Project B" })}
        run={null}
        timeline={null}
        writeApprovalGateway={writeGateway}
      />,
    );

    await waitFor(() => {
      expect(screen.queryByTestId("write-approval-bar")).toBeNull();
    });
  });

  it("atomically drops a raw grant when the next list omits it or terminalizes", async () => {
    let listPhase: "with-grant" | "no-grant" | "terminal" = "with-grant";
    const writeGateway = createWriteGateway({
      listImpl: async () => {
        if (listPhase === "with-grant") {
          return [pending()];
        }
        if (listPhase === "no-grant") {
          // Same live proposal id, grant revoked by another actor / host.
          return [pending({ approvalGrant: undefined })];
        }
        return [
          pending({
            state: "expired",
            approvalGrant: undefined,
          }),
        ];
      },
    });

    const { rerender } = render(
      <Workbench
        conversationId={convAId}
        onCancelRun={vi.fn()}
        onOpenProject={vi.fn()}
        onSendTurn={vi.fn()}
        project={fixtureProject()}
        run={null}
        timeline={emptyTimeline(convAId)}
        writeApprovalGateway={writeGateway}
        writeProposalRefreshToken={0}
      />,
    );

    await waitFor(() => {
      expect(screen.getByTestId("write-approval-approve")).not.toBeDisabled();
    });

    listPhase = "no-grant";
    rerender(
      <Workbench
        conversationId={convAId}
        onCancelRun={vi.fn()}
        onOpenProject={vi.fn()}
        onSendTurn={vi.fn()}
        project={fixtureProject()}
        run={null}
        timeline={emptyTimeline(convAId)}
        writeApprovalGateway={writeGateway}
        writeProposalRefreshToken={1}
      />,
    );

    await waitFor(() => {
      expect(screen.queryByTestId("write-approval-bar")).toBeNull();
      expect(screen.queryByTestId("write-approval-approve")).toBeNull();
      expect(writeGateway.decideCalls).toHaveLength(0);
      expect(document.body.innerHTML).not.toContain(grantSecret);
    });

    listPhase = "terminal";
    rerender(
      <Workbench
        conversationId={convAId}
        onCancelRun={vi.fn()}
        onOpenProject={vi.fn()}
        onSendTurn={vi.fn()}
        project={fixtureProject()}
        run={null}
        timeline={emptyTimeline(convAId)}
        writeApprovalGateway={writeGateway}
        writeProposalRefreshToken={2}
      />,
    );

    await waitFor(() => {
      expect(screen.queryByTestId("write-approval-bar")).toBeNull();
      expect(screen.queryByTestId("write-approval-approve")).toBeNull();
      expect(writeGateway.decideCalls).toHaveLength(0);
      expect(document.body.innerHTML).not.toContain(grantSecret);
    });
  });

  it("at expiry disables actions, closes governed diff, and re-lists", async () => {
    vi.useFakeTimers({ shouldAdvanceTime: true });
    const user = userEvent.setup({
      advanceTimers: vi.advanceTimersByTime,
    });
    const expiresAt = new Date(Date.now() + 5_000).toISOString();
    const writeGateway = createWriteGateway({
      listImpl: async () => [pending({ expiresAt })],
    });

    render(
      <Workbench
        conversationId={convAId}
        onCancelRun={vi.fn()}
        onOpenProject={vi.fn()}
        onSendTurn={vi.fn()}
        project={fixtureProject()}
        run={null}
        timeline={emptyTimeline(convAId)}
        writeApprovalGateway={writeGateway}
      />,
    );

    await waitFor(() => {
      expect(screen.getByTestId("write-approval-review-diff")).toBeInTheDocument();
    });
    await user.click(screen.getByTestId("write-approval-review-diff"));
    await waitFor(() => {
      expect(screen.getByTestId("write-diff-preview")).toBeInTheDocument();
    });

    const listsBeforeExpiry = writeGateway.listCalls.length;
    await act(async () => {
      await vi.advanceTimersByTimeAsync(5_001);
    });

    await waitFor(() => {
      expect(writeGateway.listCalls.length).toBeGreaterThan(listsBeforeExpiry);
    });
    expect(screen.getByTestId("write-approval-approve")).toBeDisabled();
    expect(screen.getByTestId("write-approval-reject")).toBeDisabled();
    expect(screen.getByTestId("write-approval-review-diff")).toBeDisabled();
    expect(screen.queryByTestId("write-diff-preview")).toBeNull();
  });

  it("never logs caught list/decision objects that carry grant or token text", async () => {
    const user = userEvent.setup();
    const errorSpy = vi.spyOn(console, "error").mockImplementation(() => {});
    const logSpy = vi.spyOn(console, "log").mockImplementation(() => {});
    const warnSpy = vi.spyOn(console, "warn").mockImplementation(() => {});

    const adversarial = {
      code: "write_invalid_grant",
      message: `rejected grant=${grantSecret} token=host-secret-token-no-slash`,
      approvalGrant: grantSecret,
    };

    const writeGateway = createWriteGateway({
      listImpl: async () => [pending()],
      decideWriteProposal: async () => {
        throw adversarial;
      },
    });

    render(
      <Workbench
        conversationId={convAId}
        onCancelRun={vi.fn()}
        onOpenProject={vi.fn()}
        onSendTurn={vi.fn()}
        project={fixtureProject()}
        run={null}
        timeline={emptyTimeline(convAId)}
        writeApprovalGateway={writeGateway}
      />,
    );

    await waitFor(() => {
      expect(screen.getByTestId("write-approval-approve")).toBeInTheDocument();
    });
    await user.click(screen.getByTestId("write-approval-approve"));

    await waitFor(() => {
      expect(errorSpy).toHaveBeenCalled();
    });

    const serialized = JSON.stringify([
      ...errorSpy.mock.calls,
      ...logSpy.mock.calls,
      ...warnSpy.mock.calls,
    ]);
    expect(serialized).not.toContain(grantSecret);
    expect(serialized).not.toContain("host-secret-token-no-slash");
    expect(serialized).not.toContain("approvalGrant");

    errorSpy.mockRestore();
    logSpy.mockRestore();
    warnSpy.mockRestore();
  });

  it("normalizes WriteApprovalGatewayError instances so secret messages never reach DOM or console", async () => {
    const user = userEvent.setup();
    const errorSpy = vi.spyOn(console, "error").mockImplementation(() => {});
    const logSpy = vi.spyOn(console, "log").mockImplementation(() => {});
    const warnSpy = vi.spyOn(console, "warn").mockImplementation(() => {});
    const secretToken = "instance-diff-token-no-slash";
    const secretMessage = `diff failed grant=${grantSecret} token=${secretToken}`;

    const writeGateway = createWriteGateway({
      listImpl: async () => [pending()],
      openWriteProposalDiff: async () => {
        throw new WriteApprovalGatewayError("write_unavailable", secretMessage);
      },
    });

    render(
      <Workbench
        conversationId={convAId}
        onCancelRun={vi.fn()}
        onOpenProject={vi.fn()}
        onSendTurn={vi.fn()}
        project={fixtureProject()}
        run={null}
        timeline={emptyTimeline(convAId)}
        writeApprovalGateway={writeGateway}
      />,
    );

    await waitFor(() => {
      expect(screen.getByTestId("write-approval-review-diff")).toBeInTheDocument();
    });
    await user.click(screen.getByTestId("write-approval-review-diff"));

    await waitFor(() => {
      expect(screen.getByRole("alert")).toHaveTextContent(
        "Write proposal is unavailable.",
      );
    });
    expect(document.body.textContent).not.toContain(grantSecret);
    expect(document.body.textContent).not.toContain(secretToken);
    expect(document.body.innerHTML).not.toContain(grantSecret);
    expect(document.body.innerHTML).not.toContain(secretToken);

    const serialized = JSON.stringify([
      ...errorSpy.mock.calls,
      ...logSpy.mock.calls,
      ...warnSpy.mock.calls,
    ]);
    expect(serialized).not.toContain(grantSecret);
    expect(serialized).not.toContain(secretToken);
    expect(serialized).not.toContain(secretMessage);

    errorSpy.mockRestore();
    logSpy.mockRestore();
    warnSpy.mockRestore();
  });

  it("project switch drops A write surfaces at B render boundary without effect settle", async () => {
    const user = userEvent.setup();
    const rootA = "F:\\secret\\projects\\alpha-isolation";
    const rootB = "F:\\secret\\projects\\beta-isolation";
    const projectA = fixtureProject({
      id: projectAId,
      name: "Project A",
      rootPath: rootA,
    });
    const projectB = fixtureProject({
      id: projectBId,
      name: "Project B",
      rootPath: rootB,
    });

    let resolveSelectB: ((value: ProjectDto) => void) | undefined;
    const deferredSelectB = new Promise<ProjectDto>((resolve) => {
      resolveSelectB = resolve;
    });

    const projectGateway = fixtureProjectGateway([projectA, projectB]);
    projectGateway.selectProject = vi.fn(async (projectId: string) => {
      if (projectId === projectBId) return deferredSelectB;
      const project = projectId === projectAId ? projectA : projectB;
      if (projectId !== projectAId && projectId !== projectBId) {
        throw new ProjectGatewayError("project_not_found", "Project not found.");
      }
      return project;
    });

    let resolveTimelineA: ((value: {
      conversationId: string;
      messages: Array<{
        id: string;
        role: "user" | "assistant" | "system";
        content: string;
        createdAt: string;
      }>;
      events: RuntimeEventDto[];
      runs: Array<{ runId: string; conversationId: string; status: "running" }>;
    }) => void) | undefined;
    let timelineACalls = 0;
    let listConversationsALate: ((value: Array<{
      id: string;
      projectId: string;
      title: string;
      createdAt: string;
      updatedAt: string;
    }>) => void) | undefined;

    const conversationGateway: ConversationGateway = {
      listConversations: async (projectId) => {
        if (projectId === projectAId) {
          return [
            {
              id: convAId,
              projectId: projectAId,
              title: "对话 A 写入",
              createdAt: "2026-08-01T00:00:00.000Z",
              updatedAt: "2026-08-01T00:00:00.000Z",
            },
          ];
        }
        if (projectId === projectBId) {
          return new Promise((resolve) => {
            // Immediate empty list for B; late A-shaped payloads must not rehydrate under B.
            resolve([]);
            queueMicrotask(() => {
              listConversationsALate?.([
                {
                  id: convAId,
                  projectId: projectAId,
                  title: "对话 A 写入",
                  createdAt: "2026-08-01T00:00:00.000Z",
                  updatedAt: "2026-08-01T00:00:00.000Z",
                },
              ]);
            });
          });
        }
        return [];
      },
      createConversation: async (projectId) => ({
        id: projectId === projectAId ? convAId : convBId,
        projectId,
        title: "新对话",
        createdAt: "2026-08-01T00:00:00.000Z",
        updatedAt: "2026-08-01T00:00:00.000Z",
      }),
      loadTimeline: async (conversationId) => {
        if (conversationId === convAId) {
          timelineACalls += 1;
          if (timelineACalls === 1) {
            return {
              conversationId: convAId,
              messages: [
                {
                  id: "u-a",
                  role: "user" as const,
                  content: "create a file",
                  createdAt: "2026-08-01T00:00:00.000Z",
                },
              ],
              events: [],
              runs: [
                {
                  runId: "run-a",
                  conversationId: convAId,
                  status: "running" as const,
                },
              ],
            };
          }
          // Second load (after project switch races) stays pending until late resolve.
          return new Promise((resolve) => {
            resolveTimelineA = resolve;
          });
        }
        return {
          conversationId,
          messages: [],
          events: [],
          runs: [],
        };
      },
      sendTurn: async () => ({ runId: "run-a" }),
      cancelRun: async () => undefined,
      subscribe: () => () => undefined,
    };

    const pendingA = pending({
      proposalId: proposalOldId,
      conversationId: convAId,
      path: "src/alpha-only.ts",
      summary: "Create alpha only",
      approvalGrant: grantSecret,
    });
    const writeGateway = createWriteGateway({
      listImpl: async (/* conversation filtered below */) => [pendingA],
    });
    // Override list to always return A pending for conv A and empty for others.
    writeGateway.listWriteProposals = async (conversationId) => {
      writeGateway.listCalls.push(conversationId);
      if (conversationId === convAId) return [pendingA];
      return [];
    };

    render(
      <App
        conversationGateway={conversationGateway}
        gateway={projectGateway}
        writeApprovalGateway={writeGateway}
      />,
    );

    await waitFor(() => {
      expect(screen.getByRole("button", { name: "Project A" })).toBeInTheDocument();
    });
    await user.click(screen.getByRole("button", { name: "Project A" }));
    await waitFor(() => {
      expect(screen.getByText("对话 A 写入")).toBeInTheDocument();
    });
    await user.click(screen.getByText("对话 A 写入"));

    await waitFor(() => {
      expect(screen.getByTestId("write-approval-bar")).toBeInTheDocument();
    });
    expect(screen.getByTestId("write-approval-path")).toHaveTextContent(
      "src/alpha-only.ts",
    );
    expect(screen.getByTestId("active-project-name")).toHaveTextContent("Project A");
    const listCallsAfterA = writeGateway.listCalls.filter((id) => id === convAId).length;
    expect(listCallsAfterA).toBeGreaterThan(0);

    // Start B activation (deferred). clearConversationScope runs, then busy is set.
    // Conversation rows stay clickable while A is still the painted active project —
    // a re-select during that window can leave selectedConversationId = A under B.
    await act(async () => {
      fireEvent.click(screen.getByRole("button", { name: "Project B" }));
    });
    await act(async () => {
      // Re-select A's conversation while B select is still in flight.
      fireEvent.click(screen.getByText("对话 A 写入"));
    });

    await act(async () => {
      resolveSelectB?.(projectB);
      await deferredSelectB;
    });

    // Render-time ownership proof: B heading is active and A's bar/write path is gone
    // immediately — no waitFor settle, no effect/timeout reliance.
    expect(screen.getByTestId("active-project-name")).toHaveTextContent("Project B");
    expect(screen.getByTestId("active-project-name")).not.toHaveTextContent("Project A");
    expect(screen.queryByTestId("write-approval-bar")).toBeNull();
    expect(screen.queryByText("src/alpha-only.ts")).toBeNull();
    expect(screen.queryByText("Create alpha only")).toBeNull();
    expect(screen.queryByText("对话 A 写入")).toBeNull();

    // Late A timeline / conversation list must not rehydrate A under B.
    await act(async () => {
      resolveTimelineA?.({
        conversationId: convAId,
        messages: [
          {
            id: "u-a-late",
            role: "user",
            content: "late A timeline",
            createdAt: "2026-08-01T00:05:00.000Z",
          },
        ],
        events: [],
        runs: [
          {
            runId: "run-a",
            conversationId: convAId,
            status: "running",
          },
        ],
      });
      listConversationsALate?.([
        {
          id: convAId,
          projectId: projectAId,
          title: "对话 A 写入",
          createdAt: "2026-08-01T00:00:00.000Z",
          updatedAt: "2026-08-01T00:00:00.000Z",
        },
      ]);
      await Promise.resolve();
      await Promise.resolve();
    });

    expect(screen.getByTestId("active-project-name")).toHaveTextContent("Project B");
    expect(screen.queryByTestId("write-approval-bar")).toBeNull();
    expect(screen.queryByText("src/alpha-only.ts")).toBeNull();
    expect(screen.queryByText("late A timeline")).toBeNull();
    expect(screen.queryByText("对话 A 写入")).toBeNull();

    // Switch back to A and select its conversation: re-list restores the same pending.
    await user.click(screen.getByRole("button", { name: "Project A" }));
    await waitFor(() => {
      expect(screen.getByTestId("active-project-name")).toHaveTextContent("Project A");
    });
    await waitFor(() => {
      expect(screen.getByText("对话 A 写入")).toBeInTheDocument();
    });
    const listBeforeRestore = writeGateway.listCalls.filter((id) => id === convAId).length;
    await user.click(screen.getByText("对话 A 写入"));
    await waitFor(() => {
      expect(
        writeGateway.listCalls.filter((id) => id === convAId).length,
      ).toBeGreaterThan(listBeforeRestore);
    });
    await waitFor(() => {
      expect(screen.getByTestId("write-approval-bar")).toBeInTheDocument();
    });
    expect(screen.getByTestId("write-approval-path")).toHaveTextContent(
      "src/alpha-only.ts",
    );

    // Grant + both native roots never appear in body text or HTML attributes.
    expect(document.body.textContent).not.toContain(grantSecret);
    expect(document.body.innerHTML).not.toContain(grantSecret);
    expect(document.body.textContent).not.toContain(rootA);
    expect(document.body.textContent).not.toContain(rootB);
    expect(document.body.innerHTML).not.toContain(rootA);
    expect(document.body.innerHTML).not.toContain(rootB);
    expect(document.body.textContent).not.toContain(proposalOldId);
  });
});
