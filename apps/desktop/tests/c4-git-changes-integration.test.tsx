/**
 * C4 D3B4 Round 2B — App/Workbench Git Changes composition.
 * Project + request generation isolation; no polling; path-free public errors.
 */
import {
  act,
  render,
  screen,
  waitFor,
  within,
} from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { ComponentProps } from "react";
import { App } from "../src/App";
import { Workbench } from "../src/features/workbench/Workbench";
import type { GitChangesPaneModel } from "../src/features/workbench/ResourcePane";
import {
  GIT_ERROR_MESSAGES,
  GitGatewayError,
  type GitDiffView,
  type GitDiscoveryView,
  type GitGateway,
  type GitRepositoryView,
  type GitStatusView,
} from "../src/lib/gitGateway";
import {
  ProjectGatewayError,
  type ProjectDto,
  type ProjectGateway,
} from "../src/lib/projectGateway";
import type { ConversationGateway } from "../src/lib/conversationGateway";
import type {
  ConversationTimelineView,
} from "../src/features/conversations/conversationReducer";
import type {
  ExecGateway,
  ExecOutputView,
  ExecProposalView,
} from "../src/lib/execGateway";
import type {
  WriteApprovalGateway,
  WriteDiffView,
  WriteProposalView,
} from "../src/lib/writeApprovalGateway";
import "../src/styles/tokens.css";
import "../src/styles/workbench.css";
import "../src/styles/execution.css";

vi.mock("@tauri-apps/api/window", () => ({
  getCurrentWindow: () => ({
    minimize: vi.fn().mockResolvedValue(undefined),
    toggleMaximize: vi.fn().mockResolvedValue(undefined),
    close: vi.fn().mockResolvedValue(undefined),
    isMaximized: vi.fn().mockResolvedValue(false),
    onResized: vi.fn().mockResolvedValue(vi.fn()),
  }),
}));

/**
 * Every Workbench render's active project id + `gitChanges` prop.
 * Asserts the App→Workbench render-time ownership contract (DOM alone is
 * insufficient when the resource pane closes on project identity change).
 */
const workbenchGitRenders: Array<{
  projectId: string | null;
  gitChanges: GitChangesPaneModel | null | undefined;
}> = [];

vi.mock("../src/features/workbench/Workbench", async (importOriginal) => {
  const mod =
    await importOriginal<typeof import("../src/features/workbench/Workbench")>();
  function Workbench(props: ComponentProps<typeof mod.Workbench>) {
    workbenchGitRenders.push({
      projectId: props.project?.id ?? null,
      gitChanges: props.gitChanges,
    });
    return <mod.Workbench {...props} />;
  }
  return { Workbench };
});
const projectAId = "00000000-0000-0000-0000-0000000000aa";
const projectBId = "00000000-0000-0000-0000-0000000000bb";
const convAId = "00000000-0000-0000-0000-0000000000c1";

type Deferred<T> = {
  promise: Promise<T>;
  resolve: (value: T) => void;
  reject: (reason?: unknown) => void;
};

function deferred<T>(): Deferred<T> {
  let resolve!: (value: T) => void;
  let reject!: (reason?: unknown) => void;
  const promise = new Promise<T>((res, rej) => {
    resolve = res;
    reject = rej;
  });
  return { promise, resolve, reject };
}

function fixtureProject(overrides: Partial<ProjectDto> = {}): ProjectDto {
  return {
    id: projectAId,
    name: "Project A",
    rootPath: "F:\\secret\\projects\\alpha",
    trust: "trusted",
    createdAt: "2026-08-05T00:00:00.000Z",
    updatedAt: "2026-08-05T00:00:00.000Z",
    ...overrides,
  };
}

function fixtureProjectGateway(
  projects: ProjectDto[] = [
    fixtureProject(),
    fixtureProject({
      id: projectBId,
      name: "Project B",
      rootPath: "F:\\secret\\projects\\beta",
    }),
  ],
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
    openOrRegisterProject: async () => fixtureProject(),
  };
}

function emptyConversationGateway(): ConversationGateway {
  return {
    listConversations: async () => [],
    createConversation: async (projectId) => ({
      id: convAId,
      projectId,
      title: "对话",
      createdAt: "2026-08-05T00:00:00.000Z",
      updatedAt: "2026-08-05T00:00:00.000Z",
    }),
    loadTimeline: async (conversationId) => ({
      conversationId,
      messages: [],
      events: [],
      runs: [],
    }),
    sendTurn: async () => ({ runId: "run-1" }),
    cancelRun: async () => undefined,
    subscribe: () => () => undefined,
  };
}

function discoveryView(
  generation: number,
  repositories: GitRepositoryView[],
): GitDiscoveryView {
  return { generation, repositories };
}

function statusView(
  overrides: Partial<GitStatusView> = {},
): GitStatusView {
  return {
    repositoryId: ".",
    generation: 1,
    branch: "main",
    detached: false,
    headShort: "abc1234",
    dirtyCount: 1,
    files: [{ path: "src/app.ts", kind: "modified" }],
    truncated: false,
    ...overrides,
  };
}

function diffView(overrides: Partial<GitDiffView> = {}): GitDiffView {
  const unified = "@@ -1 +1 @@\n-old\n+new\n";
  return {
    repositoryId: ".",
    generation: 1,
    path: "src/app.ts",
    kind: "text",
    unified,
    truncated: false,
    lineCount: 3,
    byteCount: new TextEncoder().encode(unified).length,
    ...overrides,
  };
}

type MockGitGateway = GitGateway & {
  discoverCalls: string[];
  statusCalls: Array<{
    projectId: string;
    repositoryId: string;
    generation: number;
  }>;
  diffCalls: Array<{
    projectId: string;
    repositoryId: string;
    generation: number;
    path: string;
  }>;
  setDiscoverImpl: (
    impl: (projectId: string) => Promise<GitDiscoveryView>,
  ) => void;
  setStatusImpl: (
    impl: (input: {
      projectId: string;
      expected: { repositoryId: string; generation: number };
    }) => Promise<GitStatusView>,
  ) => void;
  setDiffImpl: (
    impl: (input: {
      projectId: string;
      expected: { repositoryId: string; generation: number };
      path: string;
    }) => Promise<GitDiffView>,
  ) => void;
};

function createMockGitGateway(
  overrides: {
    discoverImpl?: (projectId: string) => Promise<GitDiscoveryView>;
    statusImpl?: (input: {
      projectId: string;
      expected: { repositoryId: string; generation: number };
    }) => Promise<GitStatusView>;
    diffImpl?: (input: {
      projectId: string;
      expected: { repositoryId: string; generation: number };
      path: string;
    }) => Promise<GitDiffView>;
  } = {},
): MockGitGateway {
  const discoverCalls: string[] = [];
  const statusCalls: MockGitGateway["statusCalls"] = [];
  const diffCalls: MockGitGateway["diffCalls"] = [];
  let discoverImpl =
    overrides.discoverImpl
    ?? (async () =>
      discoveryView(1, [{ repositoryId: ".", available: true }]));
  let statusImpl =
    overrides.statusImpl
    ?? (async (input) =>
      statusView({
        repositoryId: input.expected.repositoryId,
        generation: input.expected.generation,
      }));
  let diffImpl =
    overrides.diffImpl
    ?? (async (input) =>
      diffView({
        repositoryId: input.expected.repositoryId,
        generation: input.expected.generation,
        path: input.path,
      }));

  return {
    discoverCalls,
    statusCalls,
    diffCalls,
    setDiscoverImpl: (impl) => {
      discoverImpl = impl;
    },
    setStatusImpl: (impl) => {
      statusImpl = impl;
    },
    setDiffImpl: (impl) => {
      diffImpl = impl;
    },
    discoverGitRepositories: async (projectId) => {
      discoverCalls.push(projectId);
      return discoverImpl(projectId);
    },
    getGitStatus: async (input) => {
      statusCalls.push({
        projectId: input.projectId,
        repositoryId: input.expected.repositoryId,
        generation: input.expected.generation,
      });
      return statusImpl(input);
    },
    openGitDiff: async (input) => {
      diffCalls.push({
        projectId: input.projectId,
        repositoryId: input.expected.repositoryId,
        generation: input.expected.generation,
        path: input.path,
      });
      return diffImpl(input);
    },
  };
}

async function selectProjectByName(name: string) {
  const user = userEvent.setup();
  await waitFor(() => {
    expect(screen.getByText(name)).toBeInTheDocument();
  });
  await user.click(screen.getByText(name));
  await waitFor(() => {
    expect(screen.getByTestId("active-project-name")).toHaveTextContent(name);
  });
}

async function openResourcePane() {
  const user = userEvent.setup();
  const toggle = screen.getByTestId("resource-toggle");
  await user.click(toggle);
  await waitFor(() => {
    expect(screen.getByTestId("resource-pane")).toBeInTheDocument();
  });
}

/** Open the contextual pane and explicitly select Changes mode (never default). */
async function openGitChangesMode() {
  await openResourcePane();
  const user = userEvent.setup();
  const changesTab = screen.getByTestId("context-body-mode-changes");
  await user.click(changesTab);
  await waitFor(() => {
    expect(screen.getByTestId("git-changes-pane")).toBeInTheDocument();
  });
}

function emptyTimeline(conversationId: string): ConversationTimelineView {
  return {
    conversationId,
    messages: [
      {
        id: "m1",
        role: "user",
        content: "hello",
        createdAt: "2026-08-05T12:00:00.000Z",
      },
    ],
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

describe("C4 D3B4 Git workbench composition", () => {
  beforeEach(() => {
    vi.restoreAllMocks();
    workbenchGitRenders.length = 0;
  });

  afterEach(() => {
    vi.restoreAllMocks();
    workbenchGitRenders.length = 0;
  });

  it("makes zero Git gateway calls and passes gitChanges=null without a selected project", async () => {
    const gitGateway = createMockGitGateway();
    render(
      <App
        gateway={fixtureProjectGateway()}
        conversationGateway={emptyConversationGateway()}
        gitGateway={gitGateway}
      />,
    );

    await waitFor(() => {
      expect(screen.getByTestId("empty-workbench")).toBeInTheDocument();
    });

    expect(gitGateway.discoverCalls).toEqual([]);
    expect(gitGateway.statusCalls).toEqual([]);
    expect(gitGateway.diffCalls).toEqual([]);
    expect(screen.queryByTestId("git-changes-pane")).toBeNull();
    expect(screen.queryByTestId("git-changes-view")).toBeNull();
  });

  it("makes zero Git gateway calls when gitGateway is omitted even with a project", async () => {
    const gitGateway = createMockGitGateway();
    render(
      <App
        gateway={fixtureProjectGateway()}
        conversationGateway={emptyConversationGateway()}
      />,
    );
    await selectProjectByName("Project A");
    await openResourcePane();

    expect(gitGateway.discoverCalls).toEqual([]);
    expect(screen.queryByTestId("git-changes-pane")).toBeNull();
    expect(screen.queryByTestId("context-body-mode-switcher")).toBeNull();
    expect(screen.getByRole("heading", { name: "资源", level: 2 })).toBeInTheDocument();
  });

  it("defaults to resource body and does not discover Git until Changes mode is selected", async () => {
    const gitGateway = createMockGitGateway({
      discoverImpl: async () =>
        discoveryView(7, [
          { repositoryId: "vendor/frozen", available: false },
          { repositoryId: ".", available: true },
          { repositoryId: "packages/nested", available: true },
        ]),
      statusImpl: async (input) =>
        statusView({
          repositoryId: input.expected.repositoryId,
          generation: input.expected.generation,
          branch: "feature",
          dirtyCount: 2,
          files: [
            { path: "src/app.ts", kind: "modified" },
            { path: "src/b.ts", kind: "added" },
          ],
        }),
    });

    render(
      <App
        gateway={fixtureProjectGateway()}
        conversationGateway={emptyConversationGateway()}
        gitGateway={gitGateway}
      />,
    );
    await selectProjectByName("Project A");

    // Project selection alone must not trigger discovery.
    expect(gitGateway.discoverCalls).toEqual([]);
    expect(gitGateway.statusCalls).toEqual([]);

    await openResourcePane();
    // Mode switcher present: pane h2 is generic; mode name lives on the tab.
    expect(screen.getByRole("heading", { name: "上下文", level: 2 })).toBeInTheDocument();
    expect(screen.getByRole("tab", { name: "资源" })).toHaveAttribute(
      "aria-selected",
      "true",
    );
    expect(screen.queryByTestId("git-changes-pane")).toBeNull();
    expect(gitGateway.discoverCalls).toEqual([]);

    const user = userEvent.setup();
    await user.click(screen.getByTestId("context-body-mode-changes"));

    await waitFor(() => {
      expect(gitGateway.discoverCalls).toEqual([projectAId]);
      expect(gitGateway.statusCalls).toEqual([
        {
          projectId: projectAId,
          repositoryId: ".",
          generation: 7,
        },
      ]);
    });

    await waitFor(() => {
      expect(screen.getByTestId("git-changes-pane")).toBeInTheDocument();
      expect(screen.getByTestId("git-changes-branch")).toHaveTextContent("feature");
      expect(screen.getByTestId("git-changes-dirty-count")).toHaveTextContent("2");
    });
    // Compact labels: mode name once (tab); no duplicate heading "变更".
    expect(screen.getAllByText("变更")).toHaveLength(1);
    expect(screen.queryByRole("heading", { name: "变更" })).toBeNull();
    expect(screen.getByRole("tab", { name: "变更" })).toHaveAttribute(
      "aria-selected",
      "true",
    );
    expect(gitGateway.diffCalls).toEqual([]);
  });

  it("clears old Git content synchronously on project switch before new promises settle", async () => {
    const discoverB = deferred<GitDiscoveryView>();
    const gitGateway = createMockGitGateway();
    gitGateway.setDiscoverImpl(async (projectId) => {
      if (projectId === projectAId) {
        return discoveryView(1, [{ repositoryId: ".", available: true }]);
      }
      return discoverB.promise;
    });
    gitGateway.setStatusImpl(async (input) =>
      statusView({
        repositoryId: input.expected.repositoryId,
        generation: input.expected.generation,
        branch: input.projectId === projectAId ? "branch-a" : "branch-b",
        files: [{ path: "a-only.ts", kind: "modified" }],
      }),
    );

    render(
      <App
        gateway={fixtureProjectGateway()}
        conversationGateway={emptyConversationGateway()}
        gitGateway={gitGateway}
      />,
    );
    await selectProjectByName("Project A");
    await openGitChangesMode();
    await waitFor(() => {
      expect(gitGateway.statusCalls.length).toBe(1);
    });
    expect(screen.getByTestId("git-changes-branch")).toHaveTextContent("branch-a");
    expect(screen.getByText("a-only.ts")).toBeInTheDocument();

    await selectProjectByName("Project B");
    // Pane closes on project identity change; re-open Changes while B discovery is pending.
    await openGitChangesMode();

    // B discovery is still pending: old A content must not remain painted.
    expect(screen.queryByText("a-only.ts")).toBeNull();
    expect(screen.queryByText("branch-a")).toBeNull();
    expect(screen.getByTestId("git-changes-loading")).toBeInTheDocument();
    expect(gitGateway.discoverCalls).toEqual([projectAId, projectBId]);

    await act(async () => {
      discoverB.resolve(
        discoveryView(3, [{ repositoryId: ".", available: true }]),
      );
      await discoverB.promise;
    });

    await waitFor(() => {
      expect(screen.getByTestId("git-changes-branch")).toHaveTextContent(
        "branch-b",
      );
    });
  });

  it("never supplies Project A Git model to Workbench once Project B is active (render-time owner)", async () => {
    const discoverB = deferred<GitDiscoveryView>();
    const gitGateway = createMockGitGateway();
    gitGateway.setDiscoverImpl(async (projectId) => {
      if (projectId === projectAId) {
        return discoveryView(1, [{ repositoryId: ".", available: true }]);
      }
      return discoverB.promise;
    });
    gitGateway.setStatusImpl(async (input) =>
      statusView({
        repositoryId: input.expected.repositoryId,
        generation: input.expected.generation,
        branch: input.projectId === projectAId ? "branch-a" : "branch-b",
        files: [
          {
            path:
              input.projectId === projectAId ? "a-only.ts" : "b-only.ts",
            kind: "modified",
          },
        ],
      }),
    );

    render(
      <App
        gateway={fixtureProjectGateway()}
        conversationGateway={emptyConversationGateway()}
        gitGateway={gitGateway}
      />,
    );
    await selectProjectByName("Project A");
    await openGitChangesMode();
    await waitFor(() => {
      expect(gitGateway.statusCalls.length).toBe(1);
    });

    // A model must have been supplied with A's status markers at some point.
    expect(
      workbenchGitRenders.some(
        (entry) =>
          entry.projectId === projectAId
          && entry.gitChanges != null
          && entry.gitChanges.status?.branch === "branch-a"
          && entry.gitChanges.status.files.some((f) => f.path === "a-only.ts"),
      ),
    ).toBe(true);

    await selectProjectByName("Project B");
    // Mode resets to resources on project switch; Git model must be null.
    // B discovery still pending — do not resolve yet.
    const whileB = workbenchGitRenders.filter(
      (entry) => entry.projectId === projectBId,
    );
    expect(whileB.length).toBeGreaterThan(0);
    for (const entry of whileB) {
      const model = entry.gitChanges;
      if (model == null) continue;
      expect(model.status?.branch).not.toBe("branch-a");
      expect(
        model.status?.files.some((f) => f.path === "a-only.ts") ?? false,
      ).toBe(false);
      if (model.diff) {
        expect(model.diff.path).not.toBe("a-only.ts");
      }
    }
    // Resource-default: no Git model until Changes is selected for B.
    expect(whileB.some((entry) => entry.gitChanges == null)).toBe(true);

    // Explicitly enter Changes for B; discovery is still deferred.
    await openGitChangesMode();
    expect(
      workbenchGitRenders.some(
        (entry) =>
          entry.projectId === projectBId
          && entry.gitChanges != null
          && (
            entry.gitChanges.loading
            || entry.gitChanges.status == null
          ),
      ),
    ).toBe(true);

    await act(async () => {
      discoverB.resolve(
        discoveryView(3, [{ repositoryId: ".", available: true }]),
      );
      await discoverB.promise;
    });

    await waitFor(() => {
      const latestB = [...workbenchGitRenders]
        .reverse()
        .find((entry) => entry.projectId === projectBId);
      expect(latestB?.gitChanges?.status?.branch).toBe("branch-b");
    });
  });

  it("drops late discovery and status results from a prior project", async () => {
    const discoverA = deferred<GitDiscoveryView>();
    const statusA = deferred<GitStatusView>();
    const gitGateway = createMockGitGateway();
    gitGateway.setDiscoverImpl(async (projectId) => {
      if (projectId === projectAId) return discoverA.promise;
      return discoveryView(9, [{ repositoryId: ".", available: true }]);
    });
    gitGateway.setStatusImpl(async (input) => {
      if (input.projectId === projectAId) return statusA.promise;
      return statusView({
        repositoryId: input.expected.repositoryId,
        generation: input.expected.generation,
        branch: "on-b",
      });
    });

    render(
      <App
        gateway={fixtureProjectGateway()}
        conversationGateway={emptyConversationGateway()}
        gitGateway={gitGateway}
      />,
    );
    await selectProjectByName("Project A");
    await openGitChangesMode();
    await waitFor(() => {
      expect(gitGateway.discoverCalls).toEqual([projectAId]);
    });

    await selectProjectByName("Project B");
    await openGitChangesMode();
    await waitFor(() => {
      expect(screen.getByTestId("git-changes-branch")).toHaveTextContent("on-b");
    });
    const bStatusCalls = gitGateway.statusCalls.length;

    await act(async () => {
      discoverA.resolve(
        discoveryView(1, [{ repositoryId: ".", available: true }]),
      );
      await discoverA.promise.catch(() => undefined);
    });
    // Late A discovery must not trigger extra status under B.
    expect(gitGateway.statusCalls.length).toBe(bStatusCalls);

    await act(async () => {
      statusA.resolve(
        statusView({
          repositoryId: ".",
          generation: 1,
          branch: "stale-a",
          files: [{ path: "secret-a.ts", kind: "modified" }],
        }),
      );
      await statusA.promise.catch(() => undefined);
    });

    expect(screen.queryByText("stale-a")).toBeNull();
    expect(screen.queryByText("secret-a.ts")).toBeNull();
    expect(screen.getByTestId("git-changes-branch")).toHaveTextContent("on-b");
  });

  it("drops late diff results after refresh invalidation and adopts only the new discovery generation", async () => {
    let discoverGeneration = 4;
    const firstDiff = deferred<GitDiffView>();
    let diffCallCount = 0;
    const gitGateway = createMockGitGateway();
    gitGateway.setDiscoverImpl(async () => {
      const generation = discoverGeneration;
      discoverGeneration += 1;
      return discoveryView(generation, [
        { repositoryId: ".", available: true },
      ]);
    });
    gitGateway.setStatusImpl(async (input) =>
      statusView({
        repositoryId: input.expected.repositoryId,
        generation: input.expected.generation,
        files: [{ path: "src/app.ts", kind: "modified" }],
      }),
    );
    gitGateway.setDiffImpl(async (input) => {
      diffCallCount += 1;
      if (diffCallCount === 1) return firstDiff.promise;
      return diffView({
        repositoryId: input.expected.repositoryId,
        generation: input.expected.generation,
        path: input.path,
        unified: "@@ refreshed @@\n",
        lineCount: 1,
        byteCount: 16,
      });
    });

    render(
      <App
        gateway={fixtureProjectGateway()}
        conversationGateway={emptyConversationGateway()}
        gitGateway={gitGateway}
      />,
    );
    await selectProjectByName("Project A");
    await openGitChangesMode();
    await waitFor(() => {
      expect(gitGateway.statusCalls[0]?.generation).toBe(4);
    });

    const user = userEvent.setup();
    await user.click(screen.getByTestId("git-changes-entry-0"));
    await waitFor(() => {
      expect(gitGateway.diffCalls).toHaveLength(1);
      expect(gitGateway.diffCalls[0]).toEqual({
        projectId: projectAId,
        repositoryId: ".",
        generation: 4,
        path: "src/app.ts",
      });
    });

    await user.click(screen.getByTestId("git-changes-refresh"));
    await waitFor(() => {
      expect(gitGateway.discoverCalls).toHaveLength(2);
      expect(gitGateway.statusCalls.at(-1)?.generation).toBe(5);
    });

    // Late first-diff (generation 4) must not paint after refresh.
    await act(async () => {
      firstDiff.resolve(
        diffView({
          repositoryId: ".",
          generation: 4,
          path: "src/app.ts",
          unified: "@@ stale-diff @@\n",
          lineCount: 1,
          byteCount: 18,
        }),
      );
      await firstDiff.promise.catch(() => undefined);
    });
    expect(screen.queryByText("stale-diff")).toBeNull();
    expect(screen.queryByTestId("git-changes-diff")).toBeNull();

    await user.click(screen.getByTestId("git-changes-entry-0"));
    await waitFor(() => {
      expect(gitGateway.diffCalls.at(-1)).toEqual({
        projectId: projectAId,
        repositoryId: ".",
        generation: 5,
        path: "src/app.ts",
      });
      expect(screen.getByTestId("git-changes-diff-path")).toHaveTextContent(
        "src/app.ts",
      );
    });
    expect(screen.getByTestId("git-changes-diff").textContent).toContain(
      "refreshed",
    );
  });

  it("selects nested available repositories and rejects unavailable ids", async () => {
    const gitGateway = createMockGitGateway({
      discoverImpl: async () =>
        discoveryView(2, [
          { repositoryId: ".", available: true },
          { repositoryId: "packages/nested", available: true },
          { repositoryId: "vendor/frozen", available: false },
        ]),
      statusImpl: async (input) =>
        statusView({
          repositoryId: input.expected.repositoryId,
          generation: input.expected.generation,
          branch:
            input.expected.repositoryId === "packages/nested"
              ? "nested-main"
              : "root-main",
          files: [
            {
              path:
                input.expected.repositoryId === "packages/nested"
                  ? "nested.ts"
                  : "root.ts",
              kind: "modified",
            },
          ],
        }),
    });

    render(
      <App
        gateway={fixtureProjectGateway()}
        conversationGateway={emptyConversationGateway()}
        gitGateway={gitGateway}
      />,
    );
    await selectProjectByName("Project A");
    await openGitChangesMode();
    await waitFor(() => {
      expect(gitGateway.statusCalls).toHaveLength(1);
    });

    const user = userEvent.setup();
    expect(screen.getByTestId("git-changes-repo-selector")).toBeInTheDocument();
    expect(screen.getByTestId("git-changes-repo-vendor/frozen")).toBeDisabled();

    await user.click(screen.getByTestId("git-changes-repo-vendor/frozen"));
    expect(gitGateway.statusCalls).toHaveLength(1);

    await user.click(screen.getByTestId("git-changes-repo-packages/nested"));
    await waitFor(() => {
      expect(gitGateway.statusCalls).toHaveLength(2);
      expect(gitGateway.statusCalls[1]).toEqual({
        projectId: projectAId,
        repositoryId: "packages/nested",
        generation: 2,
      });
      expect(screen.getByTestId("git-changes-branch")).toHaveTextContent(
        "nested-main",
      );
      expect(screen.getByText("nested.ts")).toBeInTheDocument();
    });
  });

  it("opens one diff for a valid current-status portable path with the exact tuple", async () => {
    const gitGateway = createMockGitGateway({
      discoverImpl: async () =>
        discoveryView(11, [{ repositoryId: ".", available: true }]),
      statusImpl: async (input) =>
        statusView({
          repositoryId: input.expected.repositoryId,
          generation: input.expected.generation,
          files: [
            { path: "src/app.ts", kind: "modified" },
            { path: "src/other.ts", kind: "added" },
          ],
        }),
    });

    render(
      <App
        gateway={fixtureProjectGateway()}
        conversationGateway={emptyConversationGateway()}
        gitGateway={gitGateway}
      />,
    );
    await selectProjectByName("Project A");
    await openGitChangesMode();
    await waitFor(() => {
      expect(gitGateway.statusCalls).toHaveLength(1);
    });

    const user = userEvent.setup();
    await user.click(screen.getByTestId("git-changes-entry-1"));
    await waitFor(() => {
      expect(gitGateway.diffCalls).toEqual([
        {
          projectId: projectAId,
          repositoryId: ".",
          generation: 11,
          path: "src/other.ts",
        },
      ]);
      expect(screen.getByTestId("git-changes-diff-path")).toHaveTextContent(
        "src/other.ts",
      );
    });

    // Native absolute path must never appear in the surface or gateway path arg.
    const pane = screen.getByTestId("git-changes-pane");
    expect(pane.textContent).not.toMatch(/F:\\|C:\\|\\\\server/);
    expect(gitGateway.diffCalls[0]?.path).toBe("src/other.ts");
  });

  it("surfaces path-free stale/mismatch recovery and remains refreshable", async () => {
    let discoverGeneration = 1;
    let failStatus = true;
    const gitGateway = createMockGitGateway();
    gitGateway.setDiscoverImpl(async () => {
      const generation = discoverGeneration;
      discoverGeneration += 1;
      return discoveryView(generation, [
        { repositoryId: ".", available: true },
      ]);
    });
    gitGateway.setStatusImpl(async (input) => {
      if (failStatus) {
        throw new GitGatewayError(
          "git_generation_stale",
          "host leaked F:\\secret\\repo path must not paint",
        );
      }
      return statusView({
        repositoryId: input.expected.repositoryId,
        generation: input.expected.generation,
        branch: "recovered",
      });
    });

    render(
      <App
        gateway={fixtureProjectGateway()}
        conversationGateway={emptyConversationGateway()}
        gitGateway={gitGateway}
      />,
    );
    await selectProjectByName("Project A");
    await openGitChangesMode();

    await waitFor(() => {
      expect(screen.getByTestId("git-changes-error")).toHaveTextContent(
        GIT_ERROR_MESSAGES.git_generation_stale,
      );
    });
    expect(screen.getByTestId("git-changes-stale")).toBeInTheDocument();
    expect(screen.getByTestId("git-changes-view")).toHaveAttribute(
      "data-stale",
      "true",
    );
    const errorText = screen.getByTestId("git-changes-error").textContent ?? "";
    expect(errorText).not.toMatch(/F:\\|secret|\\\\/);
    expect(errorText).not.toContain("host leaked");

    failStatus = false;
    const user = userEvent.setup();
    await user.click(screen.getByTestId("git-changes-refresh"));
    await waitFor(() => {
      expect(screen.getByTestId("git-changes-branch")).toHaveTextContent(
        "recovered",
      );
    });
    expect(screen.queryByTestId("git-changes-stale")).toBeNull();
    expect(screen.queryByTestId("git-changes-error")).toBeNull();
    expect(gitGateway.statusCalls.at(-1)?.generation).toBe(2);
  });

  it("keeps a path-free error and skips status/diff when no repository is available", async () => {
    const gitGateway = createMockGitGateway({
      discoverImpl: async () =>
        discoveryView(3, [
          { repositoryId: "vendor/a", available: false },
          { repositoryId: "vendor/b", available: false },
        ]),
    });

    render(
      <App
        gateway={fixtureProjectGateway()}
        conversationGateway={emptyConversationGateway()}
        gitGateway={gitGateway}
      />,
    );
    await selectProjectByName("Project A");
    await openGitChangesMode();

    await waitFor(() => {
      expect(screen.getByTestId("git-changes-error")).toBeInTheDocument();
    });
    expect(gitGateway.statusCalls).toEqual([]);
    expect(gitGateway.diffCalls).toEqual([]);
    const errorText = screen.getByTestId("git-changes-error").textContent ?? "";
    expect(errorText).not.toMatch(/F:\\|C:\\|\\\\/);
    expect(errorText.length).toBeGreaterThan(0);
  });

  it("preserves ResourcePane toggle and Terminal/write Diff precedence over Changes", async () => {
    // Pin wall clock so fixed expiries stay live (and setTimeout-safe) without
    // Date.now() arbitration races between write and process candidates.
    vi.useFakeTimers({
      shouldAdvanceTime: true,
      now: new Date("2026-08-05T10:00:00.000Z"),
    });
    try {
      const writeProposalId = "write-prop-1";
      const processProposalId = "proc-prop-1";
      const grantSecret = "abcdefghijklmnopqrstuvwxyz0123456789ABCDEFG";
      // Fixed ISO timestamps — write-only then process-only phases, no competition.
      const writeExpiresAt = "2026-08-05T12:00:00.000Z";
      const processExpiresAt = "2026-08-05T12:30:00.000Z";

      const writeOnlyGateway: WriteApprovalGateway = {
        listWriteProposals: async () =>
          [
            {
              proposalId: writeProposalId,
              conversationId: convAId,
              path: "src/example.ts",
              mode: "create",
              state: "pending_approval",
              summary: "Create example",
              expiresAt: writeExpiresAt,
              approvalGrant: grantSecret,
              ruleSubject: "src/example.ts",
            } satisfies WriteProposalView,
          ],
        decideWriteProposal: async () => {
          throw new Error("not used");
        },
        openWriteProposalDiff: async () =>
          ({
            proposalId: writeProposalId,
            path: "src/example.ts",
            mode: "create",
            unified: "+export const x = 1;\n",
            truncated: false,
            lineCount: 1,
            byteCount: 20,
          }) satisfies WriteDiffView,
      };

      const emptyWriteGateway: WriteApprovalGateway = {
        listWriteProposals: async () => [],
        decideWriteProposal: async () => {
          throw new Error("not used");
        },
        openWriteProposalDiff: async () => {
          throw new Error("not used");
        },
      };

      const emptyExecGateway: ExecGateway = {
        listExecProposals: async () => [],
        decideExecProposal: async () => {
          throw new Error("not used");
        },
        openExecOutput: async () => {
          throw new Error("not used");
        },
      };

      const processOnlyGateway: ExecGateway = {
        listExecProposals: async () =>
          [
            {
              proposalId: processProposalId,
              conversationId: convAId,
              origin: "agent",
              state: "pending_approval",
              programLabel: "cargo",
              argv: ["test"],
              portableCwd: ".",
              riskClass: "standard",
              summary: "Run tests",
              expiresAt: processExpiresAt,
              decisionAvailable: true,
              ruleSubject: "cargo test",
            } satisfies ExecProposalView,
          ],
        decideExecProposal: async () => {
          throw new Error("not used");
        },
        openExecOutput: async () =>
          ({
            proposalId: processProposalId,
            state: "succeeded",
            chunks: [
              { stream: "stdout", sequence: 0, text: "ok\n" },
            ],
            liveRetained: true,
            truncated: false,
            encodingLossy: false,
            capturedBytes: 3,
            exitCode: 0,
            durationMs: 10,
          }) satisfies ExecOutputView,
      };

      // Direct Workbench mount with App-built model shape (callbacks no-ops) plus
      // live targets to prove Terminal / write Diff still outrank Changes.
      const model = {
        repositories: [{ repositoryId: ".", available: true }],
        selectedRepositoryId: ".",
        status: statusView(),
        diff: null,
        loading: false,
        error: null,
        stale: false,
        onSelectRepository: vi.fn(),
        onSelectPath: vi.fn(),
        onRefresh: vi.fn(),
      };

      const project = fixtureProject();
      // Write-only first: accepted arbitration must surface the write bar
      // without competing process candidates.
      const { rerender } = render(
        <Workbench
          project={project}
          conversationId={convAId}
          conversationTitle="对话"
          onOpenProject={() => undefined}
          onSendTurn={async () => undefined}
          onCancelRun={() => undefined}
          timeline={emptyTimeline(convAId)}
          run={null}
          writeApprovalGateway={writeOnlyGateway}
          execGateway={emptyExecGateway}
          gitChanges={model}
          gitChangesCapable
        />,
      );

      const user = userEvent.setup({ advanceTimers: vi.advanceTimersByTime });
      await user.click(screen.getByTestId("resource-toggle"));
      // Default body is resources; explicitly select Changes.
      await waitFor(() => {
        expect(screen.getByTestId("context-body-mode-changes")).toBeInTheDocument();
      });
      await user.click(screen.getByTestId("context-body-mode-changes"));
      await waitFor(() => {
        expect(screen.getByTestId("git-changes-pane")).toBeInTheDocument();
      });
      expect(screen.getByRole("heading", { name: "上下文", level: 2 })).toBeInTheDocument();
      expect(screen.getByRole("tab", { name: "变更" })).toHaveAttribute(
        "aria-selected",
        "true",
      );

      // Open write Diff via review — Diff outranks Changes.
      await waitFor(() => {
        expect(screen.getByTestId("write-approval-bar")).toBeInTheDocument();
      });
      await user.click(screen.getByTestId("write-approval-review-diff"));
      await waitFor(() => {
        expect(screen.getByTestId("write-diff-pane")).toBeInTheDocument();
      });
      expect(screen.queryByTestId("git-changes-pane")).toBeNull();

      // Process-only list so process bar wins; Terminal outranks Diff and Changes.
      rerender(
        <Workbench
          project={project}
          conversationId={convAId}
          conversationTitle="对话"
          onOpenProject={() => undefined}
          onSendTurn={async () => undefined}
          onCancelRun={() => undefined}
          timeline={emptyTimeline(convAId)}
          run={null}
          writeApprovalGateway={emptyWriteGateway}
          execGateway={processOnlyGateway}
          gitChanges={model}
          gitChangesCapable
          execProposalRefreshToken={1}
        />,
      );

      await waitFor(() => {
        expect(screen.getByTestId("command-approval-bar")).toBeInTheDocument();
      });
      await user.click(screen.getByTestId("command-approval-review"));
      await waitFor(() => {
        expect(screen.getByTestId("terminal-output-pane")).toBeInTheDocument();
      });
      expect(screen.queryByTestId("git-changes-pane")).toBeNull();
      expect(screen.queryByTestId("write-diff-pane")).toBeNull();
    } finally {
      vi.useRealTimers();
    }
  });

  it("does not invent native roots, argv, session ids, or host error echoes in the Git pane", async () => {
    const gitGateway = createMockGitGateway({
      discoverImpl: async () => {
        throw new Error("spawn git.exe --git-dir=F:\\secret\\repo failed session=xyz");
      },
    });

    render(
      <App
        gateway={fixtureProjectGateway()}
        conversationGateway={emptyConversationGateway()}
        gitGateway={gitGateway}
      />,
    );
    await selectProjectByName("Project A");
    await openGitChangesMode();

    await waitFor(() => {
      expect(screen.getByTestId("git-changes-error")).toHaveTextContent(
        GIT_ERROR_MESSAGES.git_unavailable,
      );
    });
    const pane = screen.getByTestId("resource-pane");
    const text = pane.textContent ?? "";
    expect(text).not.toMatch(/F:\\|git\.exe|session=|argv|--git-dir/i);
    expect(text).not.toContain("secret");
    expect(within(pane).queryByText(/xyz/)).toBeNull();
  });
});
