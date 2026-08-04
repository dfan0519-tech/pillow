/**
 * Wave C2-D — resource browser + bounded preview mounted in workbench right pane.
 * Project-generation discard and portable-path-only surface.
 */
import {
  act,
  fireEvent,
  render,
  screen,
  waitFor,
  within,
} from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { App } from "../src/App";
import { Workbench } from "../src/features/workbench/Workbench";
import {
  ProjectGatewayError,
  type ProjectDto,
  type ProjectGateway,
} from "../src/lib/projectGateway";
import {
  ResourceGatewayError,
  type RecentResource,
  type ResourceEntry,
  type ResourceGateway,
  type ResourcePreview,
  type ResourceSearchResult,
} from "../src/lib/resourceGateway";
import type {
  ConversationTimelineView,
  ResourceReferenceView,
  RunActivityView,
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

function fixtureProject(overrides: Partial<ProjectDto> = {}): ProjectDto {
  return {
    id: projectAId,
    name: "Project A",
    rootPath: "F:\\secret\\projects\\alpha",
    trust: "trusted",
    createdAt: "2026-07-31T00:00:00.000Z",
    updatedAt: "2026-07-31T00:00:00.000Z",
    ...overrides,
  };
}

function fixtureProjectGateway(
  projects: ProjectDto[] = [fixtureProject()],
  overrides: Partial<ProjectGateway> = {},
): ProjectGateway {
  const store = [...projects];
  return {
    listProjects: async () => [...store],
    createProject: async (input) => {
      const project = fixtureProject({
        id: crypto.randomUUID(),
        name: input.name,
        rootPath: input.rootPath,
        trust: "untrusted",
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
      const leaf = rootPath.split(/[\\/]/u).pop() ?? rootPath;
      const project = fixtureProject({
        id: crypto.randomUUID(),
        name: leaf,
        rootPath,
        trust: "untrusted",
      });
      store.push(project);
      return project;
    },
    ...overrides,
  };
}

const rootEntriesA: ResourceEntry[] = [
  { path: "src", kind: "directory" },
  { path: "README.md", kind: "file", sizeBytes: 40 },
];

const rootEntriesB: ResourceEntry[] = [
  { path: "lib", kind: "directory" },
  { path: "package.json", kind: "file", sizeBytes: 80 },
];

const recentA: RecentResource[] = [
  { path: "src/main.rs", openedAt: "2026-07-31T00:00:00Z" },
];

const recentB: RecentResource[] = [
  { path: "lib/mod.rs", openedAt: "2026-07-31T01:00:00Z" },
];

const searchHitsA: ResourceSearchResult[] = [
  {
    path: "src/main.rs",
    line: 1,
    column: 4,
    preview: "fn main() {}",
  },
];

const previewA: ResourcePreview = {
  path: "src/main.rs",
  kind: "text",
  mediaType: "text/plain; charset=utf-8",
  content: "fn main() { /* project A */ }",
  bytesReturned: 32,
  truncated: false,
};

const previewReadmeA: ResourcePreview = {
  path: "README.md",
  kind: "markdown",
  mediaType: "text/markdown; charset=utf-8",
  content: "# Project A",
  bytesReturned: 12,
  truncated: false,
};

const previewB: ResourcePreview = {
  path: "package.json",
  kind: "text",
  mediaType: "text/plain; charset=utf-8",
  content: '{ "name": "project-b" }',
  bytesReturned: 24,
  truncated: false,
};

function createMockResourceGateway(
  overrides: Partial<ResourceGateway> = {},
): ResourceGateway & {
  listProjectResources: ReturnType<typeof vi.fn>;
  listRecentProjectResources: ReturnType<typeof vi.fn>;
  searchProjectResources: ReturnType<typeof vi.fn>;
  readProjectResource: ReturnType<typeof vi.fn>;
  previewProjectResource: ReturnType<typeof vi.fn>;
} {
  return {
    listProjectResources: vi.fn(async ({ projectId }) => {
      if (projectId === projectAId) return rootEntriesA;
      if (projectId === projectBId) return rootEntriesB;
      return [];
    }),
    listRecentProjectResources: vi.fn(async ({ projectId }) => {
      if (projectId === projectAId) return recentA;
      if (projectId === projectBId) return recentB;
      return [];
    }),
    searchProjectResources: vi.fn(async ({ projectId }) => {
      if (projectId === projectAId) return searchHitsA;
      return [];
    }),
    readProjectResource: vi.fn(async () => {
      throw new Error("read is out of scope for workbench integration");
    }),
    previewProjectResource: vi.fn(async ({ projectId, relativePath }) => {
      if (projectId === projectAId && relativePath === "src/main.rs") {
        return previewA;
      }
      if (projectId === projectAId && relativePath === "README.md") {
        return previewReadmeA;
      }
      if (projectId === projectBId && relativePath === "package.json") {
        return previewB;
      }
      if (projectId === projectBId && relativePath === "lib/mod.rs") {
        return {
          path: "lib/mod.rs",
          kind: "text" as const,
          mediaType: "text/plain; charset=utf-8",
          content: "mod lib;",
          bytesReturned: 8,
          truncated: false,
        };
      }
      throw new ResourceGatewayError(
        "resource_not_found",
        "Resource not found.",
      );
    }),
    ...overrides,
  } as ResourceGateway & {
    listProjectResources: ReturnType<typeof vi.fn>;
    listRecentProjectResources: ReturnType<typeof vi.fn>;
    searchProjectResources: ReturnType<typeof vi.fn>;
    readProjectResource: ReturnType<typeof vi.fn>;
    previewProjectResource: ReturnType<typeof vi.fn>;
  };
}

function totalGatewayCalls(gateway: {
  listProjectResources: { mock: { calls: unknown[] } };
  listRecentProjectResources: { mock: { calls: unknown[] } };
  searchProjectResources: { mock: { calls: unknown[] } };
  readProjectResource: { mock: { calls: unknown[] } };
  previewProjectResource: { mock: { calls: unknown[] } };
}): number {
  return (
    gateway.listProjectResources.mock.calls.length
    + gateway.listRecentProjectResources.mock.calls.length
    + gateway.searchProjectResources.mock.calls.length
    + gateway.readProjectResource.mock.calls.length
    + gateway.previewProjectResource.mock.calls.length
  );
}

function assertNoLeakMarkers(root: HTMLElement | Document = document.body) {
  const text = root.textContent ?? "";
  expect(text).not.toMatch(/F:\\secret/);
  expect(text).not.toMatch(/[A-Za-z]:\\/);
  expect(text).not.toMatch(/raw bytes|second-dashboard|dashboard-card/i);
  expect(root.querySelectorAll("[data-stat], .stat-card, .dashboard-card").length).toBe(
    0,
  );
}

describe("resource workbench integration (C2-D)", () => {
  beforeEach(() => {
    vi.useRealTimers();
  });

  afterEach(() => {
    vi.useRealTimers();
  });

  it("keeps the resource panel closed by default and issues no resource requests", async () => {
    const resourceGateway = createMockResourceGateway();
    const project = fixtureProject();
    render(
      <App
        gateway={fixtureProjectGateway([project])}
        resourceGateway={resourceGateway}
        defaultSelectedProjectId={project.id}
      />,
    );

    await screen.findByTestId("active-project-name");
    expect(screen.getByTestId("resource-toggle")).toHaveAttribute(
      "aria-pressed",
      "false",
    );
    expect(screen.queryByTestId("resource-pane")).toBeNull();
    expect(totalGatewayCalls(resourceGateway)).toBe(0);
  });

  it("disables the outer toggle with no active project and never calls the gateway", async () => {
    const resourceGateway = createMockResourceGateway();
    render(
      <App
        gateway={fixtureProjectGateway([])}
        resourceGateway={resourceGateway}
      />,
    );

    await screen.findByTestId("empty-workbench");
    const toggle = screen.getByTestId("resource-toggle");
    expect(toggle).toBeDisabled();
    expect(totalGatewayCalls(resourceGateway)).toBe(0);
  });

  it("opening with project A loads list/recents only for project A", async () => {
    const user = userEvent.setup();
    const resourceGateway = createMockResourceGateway();
    const project = fixtureProject();
    render(
      <App
        gateway={fixtureProjectGateway([project])}
        resourceGateway={resourceGateway}
        defaultSelectedProjectId={project.id}
      />,
    );

    await screen.findByTestId("active-project-name");
    await user.click(screen.getByTestId("resource-toggle"));

    const pane = await screen.findByTestId("resource-pane");
    await waitFor(() => {
      expect(resourceGateway.listProjectResources).toHaveBeenCalledWith({
        projectId: projectAId,
      });
      expect(resourceGateway.listRecentProjectResources).toHaveBeenCalledWith({
        projectId: projectAId,
      });
    });

    for (const call of resourceGateway.listProjectResources.mock.calls) {
      expect(call[0]).toEqual({ projectId: projectAId });
    }
    for (const call of resourceGateway.listRecentProjectResources.mock.calls) {
      expect(call[0]).toEqual({ projectId: projectAId });
    }
    expect(resourceGateway.previewProjectResource).not.toHaveBeenCalled();
    // Outer stage-header icon remains the only open/close disclosure.
    // Close chrome is allowed; the inner project-browser "资源" toggle is not.
    expect(within(pane).queryByRole("button", { name: /^资源$/ })).toBeNull();
    expect(within(pane).queryByTestId("ctx-resource-pane")?.querySelector(
      ".ctx-resource-pane__toggle",
    )).toBeNull();
    expect(within(pane).getByRole("heading", { level: 2, name: "资源" })).toBeInTheDocument();
    expect(within(pane).getByRole("button", { name: "关闭资源面板" })).toBeInTheDocument();
    assertNoLeakMarkers(pane);
  });

  it("selecting tree, recent, and search paths previews under project A", async () => {
    const user = userEvent.setup();
    const resourceGateway = createMockResourceGateway({
      listProjectResources: vi.fn(
        async ({ projectId, relativePath }): Promise<ResourceEntry[]> => {
          if (relativePath === "src") {
            return [
              { path: "src/main.rs", kind: "file", sizeBytes: 32 },
            ];
          }
          if (projectId === projectAId) return rootEntriesA;
          return [];
        },
      ),
    });
    const project = fixtureProject();
    render(
      <App
        gateway={fixtureProjectGateway([project])}
        resourceGateway={resourceGateway}
        defaultSelectedProjectId={project.id}
      />,
    );

    await screen.findByTestId("active-project-name");
    await user.click(screen.getByTestId("resource-toggle"));
    const pane = await screen.findByTestId("resource-pane");

    await screen.findByRole("button", { name: /src\/main\.rs/ });
    await user.click(screen.getByRole("button", { name: /src\/main\.rs/ }));

    await waitFor(() => {
      expect(resourceGateway.previewProjectResource).toHaveBeenCalledWith({
        projectId: projectAId,
        relativePath: "src/main.rs",
      });
    });
    expect(await screen.findByTestId("resource-preview")).toBeInTheDocument();
    expect(within(pane).getByText("fn main() { /* project A */ }")).toBeInTheDocument();

    await user.click(within(pane).getByRole("button", { name: /README\.md/ }));
    await waitFor(() => {
      expect(resourceGateway.previewProjectResource).toHaveBeenCalledWith({
        projectId: projectAId,
        relativePath: "README.md",
      });
    });
    expect(within(pane).getByText("Project A")).toBeInTheDocument();

    const search = within(pane).getByLabelText("搜索项目资源");
    await user.clear(search);
    await user.type(search, "main");
    await waitFor(
      () => {
        expect(resourceGateway.searchProjectResources).toHaveBeenCalledWith({
          projectId: projectAId,
          query: "main",
        });
      },
      { timeout: 2000 },
    );
    const searchHit = await within(pane).findByRole("button", {
      name: /src\/main\.rs/,
    });
    await user.click(searchHit);
    await waitFor(() => {
      const last =
        resourceGateway.previewProjectResource.mock.calls[
          resourceGateway.previewProjectResource.mock.calls.length - 1
        ];
      expect(last?.[0]).toEqual({
        projectId: projectAId,
        relativePath: "src/main.rs",
      });
    });
    assertNoLeakMarkers(pane);
  });

  it("clears a pending project-A preview before project B can render it", async () => {
    const user = userEvent.setup();
    let resolvePreviewA: ((value: ResourcePreview) => void) | undefined;
    const deferredPreviewA = new Promise<ResourcePreview>((resolve) => {
      resolvePreviewA = resolve;
    });

    const resourceGateway = createMockResourceGateway({
      previewProjectResource: vi.fn(async ({ projectId, relativePath }) => {
        if (projectId === projectAId && relativePath === "src/main.rs") {
          return deferredPreviewA;
        }
        if (projectId === projectBId && relativePath === "package.json") {
          return previewB;
        }
        throw new ResourceGatewayError(
          "resource_not_found",
          "Resource not found.",
        );
      }),
    });

    const projectA = fixtureProject({ id: projectAId, name: "Project A" });
    const projectB = fixtureProject({
      id: projectBId,
      name: "Project B",
      rootPath: "F:\\secret\\projects\\beta",
    });

    render(
      <App
        gateway={fixtureProjectGateway([projectA, projectB])}
        resourceGateway={resourceGateway}
        defaultSelectedProjectId={projectA.id}
      />,
    );

    await screen.findByTestId("active-project-name");
    await user.click(screen.getByTestId("resource-toggle"));
    await screen.findByRole("button", { name: /src\/main\.rs/ });
    await user.click(screen.getByRole("button", { name: /src\/main\.rs/ }));

    await waitFor(() => {
      expect(resourceGateway.previewProjectResource).toHaveBeenCalledWith({
        projectId: projectAId,
        relativePath: "src/main.rs",
      });
    });

    // Switch project while A preview is pending.
    await user.click(screen.getByRole("button", { name: "Project B" }));
    await waitFor(() => {
      expect(screen.getByTestId("active-project-name")).toHaveTextContent("Project B");
    });

    // Stale A content must never appear under B (panel may close on switch).
    expect(screen.queryByText("fn main() { /* project A */ }")).toBeNull();
    expect(screen.queryByText("STALE-A-SHOULD-NEVER-RENDER")).toBeNull();

    await act(async () => {
      resolvePreviewA?.({
        ...previewA,
        content: "STALE-A-SHOULD-NEVER-RENDER",
      });
      await deferredPreviewA;
    });

    expect(screen.queryByText("STALE-A-SHOULD-NEVER-RENDER")).toBeNull();
    expect(screen.queryByText("fn main() { /* project A */ }")).toBeNull();
  });

  it("late project-A success and failure cannot appear under project B", async () => {
    const user = userEvent.setup();
    let resolveOk: ((value: ResourcePreview) => void) | undefined;
    let rejectFail: ((reason?: unknown) => void) | undefined;
    const deferredOk = new Promise<ResourcePreview>((resolve) => {
      resolveOk = resolve;
    });
    const deferredFail = new Promise<ResourcePreview>((_resolve, reject) => {
      rejectFail = reject;
    });
    let previewCall = 0;

    const resourceGateway = createMockResourceGateway({
      previewProjectResource: vi.fn(async ({ projectId, relativePath }) => {
        previewCall += 1;
        if (previewCall === 1 && projectId === projectAId) {
          return deferredOk;
        }
        if (previewCall === 2 && projectId === projectAId) {
          return deferredFail;
        }
        if (projectId === projectBId && relativePath === "package.json") {
          return previewB;
        }
        throw new ResourceGatewayError(
          "resource_not_found",
          "Resource not found.",
        );
      }),
    });

    const projectA = fixtureProject({ id: projectAId, name: "Project A" });
    const projectB = fixtureProject({
      id: projectBId,
      name: "Project B",
      rootPath: "F:\\secret\\projects\\beta",
    });

    render(
      <App
        gateway={fixtureProjectGateway([projectA, projectB])}
        resourceGateway={resourceGateway}
        defaultSelectedProjectId={projectA.id}
      />,
    );

    await screen.findByTestId("active-project-name");
    await user.click(screen.getByTestId("resource-toggle"));
    await screen.findByRole("button", { name: /src\/main\.rs/ });
    await user.click(screen.getByRole("button", { name: /src\/main\.rs/ }));
    await user.click(screen.getByRole("button", { name: /README\.md/ }));

    await user.click(screen.getByRole("button", { name: "Project B" }));
    await waitFor(() => {
      expect(screen.getByTestId("active-project-name")).toHaveTextContent("Project B");
    });
    await user.click(screen.getByTestId("resource-toggle"));

    await screen.findByRole("button", { name: /package\.json/ });
    await user.click(screen.getByRole("button", { name: /package\.json/ }));
    await waitFor(() => {
      expect(screen.getByText('{ "name": "project-b" }')).toBeInTheDocument();
    });

    await act(async () => {
      resolveOk?.({
        ...previewA,
        content: "LATE-SUCCESS-A",
      });
      await deferredOk.catch(() => undefined);
    });
    await act(async () => {
      rejectFail?.(
        new ResourceGatewayError(
          "resource_unavailable",
          "Failed under F:\\secret\\projects\\alpha",
        ),
      );
      await deferredFail.catch(() => undefined);
    });

    expect(screen.queryByText("LATE-SUCCESS-A")).toBeNull();
    expect(document.body.textContent).not.toMatch(/F:\\secret/);
    expect(screen.getByText('{ "name": "project-b" }')).toBeInTheDocument();
  });

  it("opening B and selecting a B path calls only project B for that selection", async () => {
    const user = userEvent.setup();
    const resourceGateway = createMockResourceGateway();
    const projectA = fixtureProject({ id: projectAId, name: "Project A" });
    const projectB = fixtureProject({
      id: projectBId,
      name: "Project B",
      rootPath: "F:\\secret\\projects\\beta",
    });

    render(
      <App
        gateway={fixtureProjectGateway([projectA, projectB])}
        resourceGateway={resourceGateway}
        defaultSelectedProjectId={projectA.id}
      />,
    );

    await screen.findByTestId("active-project-name");
    await user.click(screen.getByRole("button", { name: "Project B" }));
    await waitFor(() => {
      expect(screen.getByTestId("active-project-name")).toHaveTextContent("Project B");
    });
    await user.click(screen.getByTestId("resource-toggle"));

    await waitFor(() => {
      expect(resourceGateway.listProjectResources).toHaveBeenCalledWith({
        projectId: projectBId,
      });
    });

    await screen.findByRole("button", { name: /package\.json/ });
    await user.click(screen.getByRole("button", { name: /package\.json/ }));

    await waitFor(() => {
      expect(resourceGateway.previewProjectResource).toHaveBeenCalledWith({
        projectId: projectBId,
        relativePath: "package.json",
      });
    });

    for (const call of resourceGateway.previewProjectResource.mock.calls) {
      expect(call[0].projectId).toBe(projectBId);
    }
    expect(screen.getByText('{ "name": "project-b" }')).toBeInTheDocument();
    assertNoLeakMarkers();
  });

  it("last-request-wins selection: delayed A success cannot remount resource scope under A", async () => {
    const user = userEvent.setup();
    const resourceGateway = createMockResourceGateway();
    const projectA = fixtureProject({ id: projectAId, name: "Project A" });
    const projectB = fixtureProject({
      id: projectBId,
      name: "Project B",
      rootPath: "F:\\secret\\projects\\beta",
    });

    let resolveA: ((value: ProjectDto) => void) | undefined;
    let resolveB: ((value: ProjectDto) => void) | undefined;
    const deferredA = new Promise<ProjectDto>((resolve) => {
      resolveA = resolve;
    });
    const deferredB = new Promise<ProjectDto>((resolve) => {
      resolveB = resolve;
    });

    const gateway = fixtureProjectGateway([projectA, projectB], {
      selectProject: vi.fn(async (projectId: string) => {
        if (projectId === projectAId) return deferredA;
        if (projectId === projectBId) return deferredB;
        throw new ProjectGatewayError("project_not_found", "Project not found.");
      }),
    });

    render(
      <App gateway={gateway} resourceGateway={resourceGateway} />,
    );
    await screen.findByTestId("empty-workbench");

    await act(async () => {
      fireEvent.click(screen.getByRole("button", { name: "Project A" }));
      fireEvent.click(screen.getByRole("button", { name: "Project B" }));
    });

    await act(async () => {
      resolveB?.(projectB);
      await deferredB;
    });

    await waitFor(() => {
      expect(screen.getByTestId("active-project-name")).toHaveTextContent("Project B");
    });

    await act(async () => {
      resolveA?.(projectA);
      await deferredA;
    });

    await act(async () => {
      await Promise.resolve();
      await Promise.resolve();
    });

    expect(screen.getByTestId("active-project-name")).toHaveTextContent("Project B");
    expect(screen.getByTestId("active-project-name")).not.toHaveTextContent("Project A");

    await user.click(screen.getByTestId("resource-toggle"));
    const pane = await screen.findByTestId("resource-pane");

    await waitFor(() => {
      expect(resourceGateway.listProjectResources).toHaveBeenCalledWith({
        projectId: projectBId,
      });
      expect(resourceGateway.listRecentProjectResources).toHaveBeenCalledWith({
        projectId: projectBId,
      });
    });

    for (const call of resourceGateway.listProjectResources.mock.calls) {
      expect(call[0].projectId).toBe(projectBId);
    }
    for (const call of resourceGateway.listRecentProjectResources.mock.calls) {
      expect(call[0].projectId).toBe(projectBId);
    }

    await screen.findByRole("button", { name: /package\.json/ });
    await user.click(screen.getByRole("button", { name: /package\.json/ }));
    await waitFor(() => {
      expect(resourceGateway.previewProjectResource).toHaveBeenCalledWith({
        projectId: projectBId,
        relativePath: "package.json",
      });
    });
    for (const call of resourceGateway.previewProjectResource.mock.calls) {
      expect(call[0].projectId).toBe(projectBId);
    }
    expect(within(pane).getByText('{ "name": "project-b" }')).toBeInTheDocument();
    expect(screen.queryByText("fn main() { /* project A */ }")).toBeNull();
    assertNoLeakMarkers(pane);
  });

  it("last-request-wins selection: delayed A failure cannot error banner or poison B resources", async () => {
    const user = userEvent.setup();
    const resourceGateway = createMockResourceGateway();
    const projectA = fixtureProject({ id: projectAId, name: "Project A" });
    const projectB = fixtureProject({
      id: projectBId,
      name: "Project B",
      rootPath: "F:\\secret\\projects\\beta",
    });

    let rejectA: ((reason?: unknown) => void) | undefined;
    let resolveB: ((value: ProjectDto) => void) | undefined;
    const deferredA = new Promise<ProjectDto>((_resolve, reject) => {
      rejectA = reject;
    });
    const deferredB = new Promise<ProjectDto>((resolve) => {
      resolveB = resolve;
    });

    const gateway = fixtureProjectGateway([projectA, projectB], {
      selectProject: vi.fn(async (projectId: string) => {
        if (projectId === projectAId) return deferredA;
        if (projectId === projectBId) return deferredB;
        throw new ProjectGatewayError("project_not_found", "Project not found.");
      }),
    });

    render(
      <App gateway={gateway} resourceGateway={resourceGateway} />,
    );
    await screen.findByTestId("empty-workbench");

    await act(async () => {
      fireEvent.click(screen.getByRole("button", { name: "Project A" }));
      fireEvent.click(screen.getByRole("button", { name: "Project B" }));
    });

    await act(async () => {
      resolveB?.(projectB);
      await deferredB;
    });

    await waitFor(() => {
      expect(screen.getByTestId("active-project-name")).toHaveTextContent("Project B");
    });

    await act(async () => {
      rejectA?.(
        new ProjectGatewayError(
          "project_store_unavailable",
          "STALE-A-SELECT-FAILED",
        ),
      );
      await deferredA.catch(() => undefined);
    });

    await act(async () => {
      await Promise.resolve();
      await Promise.resolve();
    });

    expect(screen.getByTestId("active-project-name")).toHaveTextContent("Project B");
    expect(screen.queryByText("STALE-A-SELECT-FAILED")).not.toBeInTheDocument();
    expect(screen.queryByRole("alert")).not.toBeInTheDocument();

    await user.click(screen.getByTestId("resource-toggle"));
    await waitFor(() => {
      expect(resourceGateway.listProjectResources).toHaveBeenCalledWith({
        projectId: projectBId,
      });
    });
    for (const call of resourceGateway.listProjectResources.mock.calls) {
      expect(call[0].projectId).toBe(projectBId);
    }
    expect(await screen.findByRole("button", { name: /package\.json/ })).toBeInTheDocument();
    assertNoLeakMarkers();
  });

  it("last-request-wins open: delayed open success cannot remount resource scope under opened project", async () => {
    const user = userEvent.setup();
    const resourceGateway = createMockResourceGateway();
    const projectB = fixtureProject({
      id: projectBId,
      name: "Project B",
      rootPath: "F:\\secret\\projects\\beta",
    });
    const opened = fixtureProject({
      id: "opened-stale",
      name: "stale-opened",
      rootPath: "F:\\secret\\projects\\stale-opened",
      trust: "untrusted",
    });

    let resolveOpenRegister: ((value: ProjectDto) => void) | undefined;
    const deferredOpenRegister = new Promise<ProjectDto>((resolve) => {
      resolveOpenRegister = resolve;
    });
    let resolveSelectB: ((value: ProjectDto) => void) | undefined;
    const deferredSelectB = new Promise<ProjectDto>((resolve) => {
      resolveSelectB = resolve;
    });

    const selectProject = vi.fn(async (projectId: string) => {
      if (projectId === projectBId) return deferredSelectB;
      if (projectId === opened.id) return opened;
      throw new ProjectGatewayError("project_not_found", "Project not found.");
    });

    const gateway = fixtureProjectGateway([projectB], {
      pickDirectory: vi.fn().mockResolvedValue(opened.rootPath),
      openOrRegisterProject: vi.fn(async () => deferredOpenRegister),
      selectProject,
    });

    const view = render(
      <App gateway={gateway} resourceGateway={resourceGateway} />,
    );
    await screen.findByTestId("empty-workbench");

    // Start open first so registration is already in flight.
    await act(async () => {
      fireEvent.click(screen.getByTestId("nav-open-project"));
    });

    await waitFor(() => {
      expect(gateway.openOrRegisterProject).toHaveBeenCalledWith(opened.rootPath);
    });

    // Concurrent B while register is pending. Busy disables row clicks, but the
    // public default-select path (screenshot harness) can still race activation.
    await act(async () => {
      view.rerender(
        <App
          gateway={gateway}
          resourceGateway={resourceGateway}
          defaultSelectedProjectId={projectBId}
        />,
      );
    });

    await waitFor(() => {
      expect(selectProject).toHaveBeenCalledWith(projectBId);
    });

    await act(async () => {
      resolveSelectB?.(projectB);
      await deferredSelectB;
    });

    await waitFor(() => {
      expect(screen.getByTestId("active-project-name")).toHaveTextContent("Project B");
    });

    const selectCallsAfterB = selectProject.mock.calls.length;

    await act(async () => {
      resolveOpenRegister?.(opened);
      await deferredOpenRegister;
    });

    await act(async () => {
      await Promise.resolve();
      await Promise.resolve();
    });

    expect(screen.getByTestId("active-project-name")).toHaveTextContent("Project B");
    expect(screen.getByTestId("active-project-name")).not.toHaveTextContent("stale-opened");
    expect(selectProject).not.toHaveBeenCalledWith(opened.id);
    expect(selectProject.mock.calls.length).toBe(selectCallsAfterB);
    expect(screen.queryByRole("alert")).not.toBeInTheDocument();

    await user.click(screen.getByTestId("resource-toggle"));
    const pane = await screen.findByTestId("resource-pane");

    await waitFor(() => {
      expect(resourceGateway.listProjectResources).toHaveBeenCalledWith({
        projectId: projectBId,
      });
      expect(resourceGateway.listRecentProjectResources).toHaveBeenCalledWith({
        projectId: projectBId,
      });
    });

    for (const call of resourceGateway.listProjectResources.mock.calls) {
      expect(call[0].projectId).toBe(projectBId);
    }
    for (const call of resourceGateway.listRecentProjectResources.mock.calls) {
      expect(call[0].projectId).toBe(projectBId);
    }

    await screen.findByRole("button", { name: /package\.json/ });
    await user.click(screen.getByRole("button", { name: /package\.json/ }));
    await waitFor(() => {
      expect(resourceGateway.previewProjectResource).toHaveBeenCalledWith({
        projectId: projectBId,
        relativePath: "package.json",
      });
    });
    for (const call of resourceGateway.previewProjectResource.mock.calls) {
      expect(call[0].projectId).toBe(projectBId);
    }
    expect(within(pane).getByText('{ "name": "project-b" }')).toBeInTheDocument();
    expect(screen.queryByText("fn main() { /* project A */ }")).toBeNull();
    assertNoLeakMarkers(pane);
  });

  it("last-request-wins open: delayed open failure cannot error banner or poison B resources", async () => {
    const user = userEvent.setup();
    const resourceGateway = createMockResourceGateway();
    const projectB = fixtureProject({
      id: projectBId,
      name: "Project B",
      rootPath: "F:\\secret\\projects\\beta",
    });
    const openedPath = "F:\\secret\\projects\\stale-opened";

    let rejectOpenRegister: ((reason?: unknown) => void) | undefined;
    const deferredOpenRegister = new Promise<ProjectDto>((_resolve, reject) => {
      rejectOpenRegister = reject;
    });
    let resolveSelectB: ((value: ProjectDto) => void) | undefined;
    const deferredSelectB = new Promise<ProjectDto>((resolve) => {
      resolveSelectB = resolve;
    });

    const selectProject = vi.fn(async (projectId: string) => {
      if (projectId === projectBId) return deferredSelectB;
      throw new ProjectGatewayError("project_not_found", "Project not found.");
    });

    const gateway = fixtureProjectGateway([projectB], {
      pickDirectory: vi.fn().mockResolvedValue(openedPath),
      openOrRegisterProject: vi.fn(async () => deferredOpenRegister),
      selectProject,
    });

    const view = render(
      <App gateway={gateway} resourceGateway={resourceGateway} />,
    );
    await screen.findByTestId("empty-workbench");

    await act(async () => {
      fireEvent.click(screen.getByTestId("nav-open-project"));
    });

    await waitFor(() => {
      expect(gateway.openOrRegisterProject).toHaveBeenCalledWith(openedPath);
    });

    await act(async () => {
      view.rerender(
        <App
          gateway={gateway}
          resourceGateway={resourceGateway}
          defaultSelectedProjectId={projectBId}
        />,
      );
    });

    await waitFor(() => {
      expect(selectProject).toHaveBeenCalledWith(projectBId);
    });

    await act(async () => {
      resolveSelectB?.(projectB);
      await deferredSelectB;
    });

    await waitFor(() => {
      expect(screen.getByTestId("active-project-name")).toHaveTextContent("Project B");
    });

    await act(async () => {
      rejectOpenRegister?.(
        new ProjectGatewayError(
          "project_store_unavailable",
          "STALE-OPEN-REGISTER-FAILED",
        ),
      );
      await deferredOpenRegister.catch(() => undefined);
    });

    await act(async () => {
      await Promise.resolve();
      await Promise.resolve();
    });

    expect(screen.getByTestId("active-project-name")).toHaveTextContent("Project B");
    expect(screen.queryByText("STALE-OPEN-REGISTER-FAILED")).not.toBeInTheDocument();
    expect(screen.queryByRole("alert")).not.toBeInTheDocument();

    await user.click(screen.getByTestId("resource-toggle"));
    await waitFor(() => {
      expect(resourceGateway.listProjectResources).toHaveBeenCalledWith({
        projectId: projectBId,
      });
    });
    for (const call of resourceGateway.listProjectResources.mock.calls) {
      expect(call[0].projectId).toBe(projectBId);
    }
    expect(await screen.findByRole("button", { name: /package\.json/ })).toBeInTheDocument();
    assertNoLeakMarkers();
  });

  it("stale picker: delayed non-empty pickDirectory must not register or poison B resources", async () => {
    const user = userEvent.setup();
    const resourceGateway = createMockResourceGateway();
    const projectB = fixtureProject({
      id: projectBId,
      name: "Project B",
      rootPath: "F:\\secret\\projects\\beta",
    });
    const stalePath = "F:\\secret\\projects\\stale-opened";

    let resolvePick: ((value: string | null) => void) | undefined;
    const deferredPick = new Promise<string | null>((resolve) => {
      resolvePick = resolve;
    });
    let resolveSelectB: ((value: ProjectDto) => void) | undefined;
    const deferredSelectB = new Promise<ProjectDto>((resolve) => {
      resolveSelectB = resolve;
    });

    const selectProject = vi.fn(async (projectId: string) => {
      if (projectId === projectBId) return deferredSelectB;
      throw new ProjectGatewayError("project_not_found", "Project not found.");
    });
    const openOrRegisterProject = vi.fn(async (rootPath: string) =>
      fixtureProject({
        id: "opened-stale",
        name: "stale-opened",
        rootPath,
        trust: "untrusted",
      }),
    );

    const gateway = fixtureProjectGateway([projectB], {
      pickDirectory: vi.fn(() => deferredPick),
      openOrRegisterProject,
      selectProject,
    });

    render(
      <App gateway={gateway} resourceGateway={resourceGateway} />,
    );
    await screen.findByTestId("empty-workbench");

    await act(async () => {
      fireEvent.click(screen.getByTestId("nav-open-project"));
      fireEvent.click(screen.getByRole("button", { name: "Project B" }));
    });

    await waitFor(() => {
      expect(gateway.pickDirectory).toHaveBeenCalledOnce();
      expect(selectProject).toHaveBeenCalledWith(projectBId);
    });
    expect(openOrRegisterProject).not.toHaveBeenCalled();

    await act(async () => {
      resolveSelectB?.(projectB);
      await deferredSelectB;
    });

    await waitFor(() => {
      expect(screen.getByTestId("active-project-name")).toHaveTextContent("Project B");
    });

    await act(async () => {
      resolvePick?.(stalePath);
      await deferredPick;
    });

    await act(async () => {
      await Promise.resolve();
      await Promise.resolve();
    });

    expect(openOrRegisterProject).not.toHaveBeenCalled();
    expect(screen.getByTestId("active-project-name")).toHaveTextContent("Project B");
    expect(screen.getByTestId("active-project-name")).not.toHaveTextContent("stale-opened");
    expect(screen.queryByRole("alert")).not.toBeInTheDocument();
    expect(screen.getByTestId("nav-open-project")).not.toBeDisabled();
    expect(screen.getByTestId("resource-toggle")).not.toBeDisabled();

    await user.click(screen.getByTestId("resource-toggle"));
    const pane = await screen.findByTestId("resource-pane");

    await waitFor(() => {
      expect(resourceGateway.listProjectResources).toHaveBeenCalledWith({
        projectId: projectBId,
      });
      expect(resourceGateway.listRecentProjectResources).toHaveBeenCalledWith({
        projectId: projectBId,
      });
    });

    for (const call of resourceGateway.listProjectResources.mock.calls) {
      expect(call[0].projectId).toBe(projectBId);
    }
    for (const call of resourceGateway.listRecentProjectResources.mock.calls) {
      expect(call[0].projectId).toBe(projectBId);
    }

    await screen.findByRole("button", { name: /package\.json/ });
    await user.click(screen.getByRole("button", { name: /package\.json/ }));
    await waitFor(() => {
      expect(resourceGateway.previewProjectResource).toHaveBeenCalledWith({
        projectId: projectBId,
        relativePath: "package.json",
      });
    });
    for (const call of resourceGateway.previewProjectResource.mock.calls) {
      expect(call[0].projectId).toBe(projectBId);
    }
    expect(within(pane).getByText('{ "name": "project-b" }')).toBeInTheDocument();
    expect(screen.queryByText("fn main() { /* project A */ }")).toBeNull();
    assertNoLeakMarkers(pane);
  });

  it("closing and reopening does not resurrect a stale preview", async () => {
    const user = userEvent.setup();
    const resourceGateway = createMockResourceGateway();
    const project = fixtureProject();
    render(
      <App
        gateway={fixtureProjectGateway([project])}
        resourceGateway={resourceGateway}
        defaultSelectedProjectId={project.id}
      />,
    );

    await screen.findByTestId("active-project-name");
    await user.click(screen.getByTestId("resource-toggle"));
    await screen.findByRole("button", { name: /src\/main\.rs/ });
    await user.click(screen.getByRole("button", { name: /src\/main\.rs/ }));
    expect(await screen.findByTestId("resource-preview")).toBeInTheDocument();
    expect(screen.getByText("fn main() { /* project A */ }")).toBeInTheDocument();

    await user.click(screen.getByTestId("resource-toggle"));
    expect(screen.queryByTestId("resource-pane")).toBeNull();
    expect(screen.queryByTestId("resource-preview")).toBeNull();

    await user.click(screen.getByTestId("resource-toggle"));
    const pane = await screen.findByTestId("resource-pane");
    expect(within(pane).queryByTestId("resource-preview")).toBeNull();
    expect(within(pane).queryByText("fn main() { /* project A */ }")).toBeNull();
  });

  it("conversation portable path: close then reopen does not async-resurrect preview", async () => {
    const user = userEvent.setup();
    const conversationPreview: ResourcePreview = {
      path: "reports/result.md",
      kind: "markdown",
      mediaType: "text/markdown; charset=utf-8",
      content: "CONVERSATION-PREVIEW-BODY-MUST-NOT-RESURRECT",
      bytesReturned: 48,
      truncated: false,
    };
    const resourceGateway = createMockResourceGateway({
      previewProjectResource: vi.fn(async ({ projectId, relativePath }) => {
        // Yield so reopen can race a pending request if the host keeps it.
        await Promise.resolve();
        if (projectId === projectAId && relativePath === "reports/result.md") {
          return conversationPreview;
        }
        throw new ResourceGatewayError(
          "resource_not_found",
          "Resource not found.",
        );
      }),
    });

    const activity: RunActivityView = {
      runId: "run-conversation-preview",
      status: "completed",
      startedAt: "2026-07-30T00:00:00.000Z",
      updatedAt: "2026-07-30T00:00:01.000Z",
      assistantText: "",
      reasoningText: "",
      plan: null,
      tools: [],
      subagents: [],
      artifacts: [
        {
          artifactId: "artifact-report",
          path: "reports/result.md",
          kind: "report",
          createdAt: "2026-07-30T00:00:01.000Z",
          resource: {
            resourceId: "resource-report",
            kind: "artifact",
            label: "Result report",
            path: "reports/result.md",
            contentPreview: "fallback body only without gateway",
            truncated: true,
          } satisfies ResourceReferenceView,
        },
      ],
    };
    const timeline: ConversationTimelineView = {
      conversationId: "c-conversation-preview",
      messages: [],
      events: [],
      seenEventKeys: new Set(),
      assistantText: "",
      reasoningText: "",
      plan: null,
      tools: [],
      subagents: [],
      activitiesByRunId: { [activity.runId]: activity },
      activeRunId: activity.runId,
    };

    render(
      <Workbench
        onCancelRun={() => undefined}
        onOpenProject={() => undefined}
        onSendTurn={() => undefined}
        project={fixtureProject()}
        resourceGateway={resourceGateway}
        run={null}
        timeline={timeline}
      />,
    );

    const disclosure = screen.getByTestId("agent-activity-disclosure");
    const details = within(disclosure).getByTestId(
      "agent-activity-details",
    ) as HTMLDetailsElement;
    details.open = true;
    const artifactToggle = within(disclosure).getByTestId(
      "agent-activity-artifacts-toggle",
    );
    const artifactsDetails = artifactToggle.closest(
      "details",
    ) as HTMLDetailsElement;
    artifactsDetails.open = true;

    // Path 1 — stage-header toggle close
    await user.click(
      screen.getByRole("button", { name: "打开 Result report" }),
    );
    expect(await screen.findByTestId("resource-preview")).toBeInTheDocument();
    expect(
      screen.getByText("CONVERSATION-PREVIEW-BODY-MUST-NOT-RESURRECT"),
    ).toBeInTheDocument();
    await waitFor(() => {
      expect(resourceGateway.previewProjectResource).toHaveBeenCalledWith({
        projectId: projectAId,
        relativePath: "reports/result.md",
      });
    });
    const previewCallsAfterFirstOpen =
      resourceGateway.previewProjectResource.mock.calls.length;

    await user.click(screen.getByTestId("resource-toggle"));
    expect(screen.queryByTestId("resource-pane")).toBeNull();
    expect(screen.queryByTestId("resource-preview")).toBeNull();

    // Reopen without selecting any resource — stale previewPathRequest must not re-fire.
    await user.click(screen.getByTestId("resource-toggle"));
    const paneAfterToggle = await screen.findByTestId("resource-pane");

    await act(async () => {
      await Promise.resolve();
      await Promise.resolve();
      await new Promise((resolve) => setTimeout(resolve, 50));
    });

    expect(resourceGateway.previewProjectResource).toHaveBeenCalledTimes(
      previewCallsAfterFirstOpen,
    );
    expect(
      within(paneAfterToggle).queryByTestId("resource-preview"),
    ).toBeNull();
    expect(
      within(paneAfterToggle).queryByText(
        "CONVERSATION-PREVIEW-BODY-MUST-NOT-RESURRECT",
      ),
    ).toBeNull();
    expect(screen.queryByText("CONVERSATION-PREVIEW-BODY-MUST-NOT-RESURRECT")).toBeNull();

    // Path 2 — contextual pane close button
    await user.click(screen.getByTestId("resource-toggle"));
    expect(screen.queryByTestId("resource-pane")).toBeNull();

    details.open = true;
    artifactsDetails.open = true;
    await user.click(
      screen.getByRole("button", { name: "打开 Result report" }),
    );
    expect(await screen.findByTestId("resource-preview")).toBeInTheDocument();
    expect(
      screen.getByText("CONVERSATION-PREVIEW-BODY-MUST-NOT-RESURRECT"),
    ).toBeInTheDocument();
    await waitFor(() => {
      expect(
        resourceGateway.previewProjectResource.mock.calls.length,
      ).toBeGreaterThan(previewCallsAfterFirstOpen);
    });
    const previewCallsAfterSecondOpen =
      resourceGateway.previewProjectResource.mock.calls.length;

    const openPane = screen.getByTestId("resource-pane");
    await user.click(
      within(openPane).getByRole("button", { name: "关闭资源面板" }),
    );
    expect(screen.queryByTestId("resource-pane")).toBeNull();
    expect(screen.queryByTestId("resource-preview")).toBeNull();

    await user.click(screen.getByTestId("resource-toggle"));
    const paneAfterPaneClose = await screen.findByTestId("resource-pane");

    await act(async () => {
      await Promise.resolve();
      await Promise.resolve();
      await new Promise((resolve) => setTimeout(resolve, 50));
    });

    expect(resourceGateway.previewProjectResource).toHaveBeenCalledTimes(
      previewCallsAfterSecondOpen,
    );
    expect(
      within(paneAfterPaneClose).queryByTestId("resource-preview"),
    ).toBeNull();
    expect(
      within(paneAfterPaneClose).queryByText(
        "CONVERSATION-PREVIEW-BODY-MUST-NOT-RESURRECT",
      ),
    ).toBeNull();
    expect(screen.queryByText("CONVERSATION-PREVIEW-BODY-MUST-NOT-RESURRECT")).toBeNull();
  });

  it("omitting resourceGateway preserves conversation-reference ResourcePane fallback", async () => {
    const activity: RunActivityView = {
      runId: "run-resource-fallback",
      status: "completed",
      startedAt: "2026-07-30T00:00:00.000Z",
      updatedAt: "2026-07-30T00:00:01.000Z",
      assistantText: "",
      reasoningText: "",
      plan: null,
      tools: [],
      subagents: [],
      artifacts: [
        {
          artifactId: "artifact-report",
          path: "reports/result.md",
          kind: "report",
          createdAt: "2026-07-30T00:00:01.000Z",
          resource: {
            resourceId: "resource-report",
            kind: "artifact",
            label: "Result report",
            path: "reports/result.md",
            contentPreview: "Report preview body",
            truncated: true,
          } satisfies ResourceReferenceView,
        },
      ],
    };
    const timeline: ConversationTimelineView = {
      conversationId: "c-resource-fallback",
      messages: [],
      events: [],
      seenEventKeys: new Set(),
      assistantText: "",
      reasoningText: "",
      plan: null,
      tools: [],
      subagents: [],
      activitiesByRunId: { [activity.runId]: activity },
      activeRunId: activity.runId,
    };

    render(
      <Workbench
        onCancelRun={() => undefined}
        onOpenProject={() => undefined}
        onSendTurn={() => undefined}
        project={fixtureProject()}
        run={null}
        timeline={timeline}
      />,
    );

    const disclosure = screen.getByTestId("agent-activity-disclosure");
    const details = within(disclosure).getByTestId(
      "agent-activity-details",
    ) as HTMLDetailsElement;
    details.open = true;
    const artifactToggle = within(disclosure).getByTestId(
      "agent-activity-artifacts-toggle",
    );
    const artifactsDetails = artifactToggle.closest(
      "details",
    ) as HTMLDetailsElement;
    artifactsDetails.open = true;
    const user = userEvent.setup();
    await user.click(
      screen.getByRole("button", { name: "打开 Result report" }),
    );

    const pane = screen.getByTestId("resource-pane");
    expect(within(pane).getByRole("heading", { level: 2, name: "资源" })).toBeVisible();
    expect(within(pane).getByText("reports/result.md")).toBeVisible();
    expect(within(pane).getByText("Report preview body")).toBeVisible();
    expect(within(pane).getByText("内容预览已截断")).toBeVisible();
    expect(within(pane).queryByLabelText("搜索项目资源")).toBeNull();
    expect(within(pane).queryByTestId("resource-preview")).toBeNull();
  });

  it("does not show machine roots, raw bytes, or second-dashboard chrome", async () => {
    const user = userEvent.setup();
    const resourceGateway = createMockResourceGateway();
    const project = fixtureProject();
    render(
      <App
        gateway={fixtureProjectGateway([project])}
        resourceGateway={resourceGateway}
        defaultSelectedProjectId={project.id}
      />,
    );

    await screen.findByTestId("active-project-name");
    await user.click(screen.getByTestId("resource-toggle"));
    const pane = await screen.findByTestId("resource-pane");
    await screen.findByRole("button", { name: /README\.md/ });
    await user.click(screen.getByRole("button", { name: /README\.md/ }));
    await screen.findByTestId("resource-preview");

    assertNoLeakMarkers(pane);
    expect(document.body.textContent).not.toMatch(/F:\\secret\\projects\\alpha/);
    // Conversation remains dominant grid track when panel is open.
    const workbench = screen.getByTestId("workbench");
    expect(workbench.className).toMatch(/has-resources/);
  });
});
