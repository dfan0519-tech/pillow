/**
 * Wave C2-C1 — collapsible project resource context panel (renderer slice).
 * Isolated from workbench wiring; gateway-mocked only.
 */
import { act, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { ResourcePane } from "../src/features/resources/ResourcePane";
import { RESOURCE_SEARCH_DEBOUNCE_MS } from "../src/features/resources/resourcePaneState";
import {
  ResourcePane as WorkbenchResourcePane,
  type GitChangesPaneModel,
} from "../src/features/workbench/ResourcePane";
import type {
  GitDiffView,
  GitStatusView,
} from "../src/lib/gitGateway";
import {
  ResourceGatewayError,
  type RecentResource,
  type ResourceEntry,
  type ResourceGateway,
  type ResourceSearchResult,
} from "../src/lib/resourceGateway";

const projectA = "00000000-0000-0000-0000-0000000000aa";
const projectB = "00000000-0000-0000-0000-0000000000bb";

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

const searchHits: ResourceSearchResult[] = [
  {
    path: "src/main.rs",
    line: 1,
    column: 4,
    preview: "fn main() {}",
  },
];

function createMockGateway(
  overrides: Partial<ResourceGateway> = {},
): ResourceGateway & {
  listProjectResources: ReturnType<typeof vi.fn>;
  listRecentProjectResources: ReturnType<typeof vi.fn>;
  searchProjectResources: ReturnType<typeof vi.fn>;
  readProjectResource: ReturnType<typeof vi.fn>;
} {
  return {
    listProjectResources: vi.fn(async ({ projectId }) => {
      if (projectId === projectA) return rootEntriesA;
      if (projectId === projectB) return rootEntriesB;
      return [];
    }),
    listRecentProjectResources: vi.fn(async ({ projectId }) => {
      if (projectId === projectA) return recentA;
      if (projectId === projectB) return recentB;
      return [];
    }),
    searchProjectResources: vi.fn(async () => searchHits),
    readProjectResource: vi.fn(async () => {
      throw new Error("read is out of scope for this panel");
    }),
    ...overrides,
  } as ResourceGateway & {
    listProjectResources: ReturnType<typeof vi.fn>;
    listRecentProjectResources: ReturnType<typeof vi.fn>;
    searchProjectResources: ReturnType<typeof vi.fn>;
    readProjectResource: ReturnType<typeof vi.fn>;
  };
}

function renderPane(
  props: Partial<{
    projectId: string;
    gateway: ResourceGateway;
    onOpenResource: (relativePath: string) => void;
    open?: boolean;
    onOpenChange?: (open: boolean) => void;
    showToggle?: boolean;
  }> = {},
) {
  const onOpenResource = props.onOpenResource ?? vi.fn();
  const gateway = props.gateway ?? createMockGateway();
  const projectId = props.projectId ?? projectA;
  const view = render(
    <ResourcePane
      projectId={projectId}
      gateway={gateway}
      onOpenResource={onOpenResource}
      open={props.open}
      onOpenChange={props.onOpenChange}
      showToggle={props.showToggle}
    />,
  );
  return { ...view, onOpenResource, gateway, projectId };
}

describe("ResourcePane (C2-C1)", () => {
  beforeEach(() => {
    vi.useRealTimers();
  });

  afterEach(() => {
    vi.useRealTimers();
  });

  it("is closed by default with a labelled disclosure control", () => {
    renderPane();

    const toggle = screen.getByRole("button", { name: /资源/i });
    expect(toggle).toHaveAttribute("aria-expanded", "false");
    expect(screen.queryByRole("region", { name: "项目资源" })).toBeNull();
    expect(screen.queryByLabelText("搜索项目资源")).toBeNull();
    expect(screen.queryByText("README.md")).toBeNull();
  });

  it("opens to show recents then deterministic root tree; closes again", async () => {
    const user = userEvent.setup();
    const { gateway } = renderPane();

    await user.click(screen.getByRole("button", { name: /资源/i }));

    const region = await screen.findByRole("region", { name: "项目资源" });
    expect(screen.getByRole("button", { name: /资源/i })).toHaveAttribute(
      "aria-expanded",
      "true",
    );

    await waitFor(() => {
      expect(gateway.listRecentProjectResources).toHaveBeenCalledWith({
        projectId: projectA,
      });
      expect(gateway.listProjectResources).toHaveBeenCalledWith({
        projectId: projectA,
      });
    });

    const recents = within(region).getByRole("region", { name: "最近打开" });
    const tree = within(region).getByRole("region", { name: "项目文件" });
    expect(within(recents).getByRole("button", { name: /src\/main\.rs/ })).toBeInTheDocument();
    expect(within(tree).getByRole("button", { name: /README\.md/ })).toBeInTheDocument();
    expect(within(tree).getByRole("button", { name: /src/ })).toBeInTheDocument();

    // Directory before file for deterministic ordering of root rows.
    const treeButtons = within(tree).getAllByRole("button");
    const labels = treeButtons.map((btn) => btn.textContent ?? "");
    const srcIndex = labels.findIndex((label) => label.includes("src"));
    const readmeIndex = labels.findIndex((label) => label.includes("README.md"));
    expect(srcIndex).toBeGreaterThanOrEqual(0);
    expect(readmeIndex).toBeGreaterThan(srcIndex);

    await user.click(screen.getByRole("button", { name: /资源/i }));
    expect(screen.queryByRole("region", { name: "项目资源" })).toBeNull();
  });

  it("isolates project switches and never applies stale in-flight results", async () => {
    const user = userEvent.setup();
    let resolveA: ((value: ResourceEntry[]) => void) | undefined;
    const deferredA = new Promise<ResourceEntry[]>((resolve) => {
      resolveA = resolve;
    });

    const gateway = createMockGateway({
      listProjectResources: vi.fn(async ({ projectId }) => {
        if (projectId === projectA) return deferredA;
        return rootEntriesB;
      }),
      listRecentProjectResources: vi.fn(async ({ projectId }) => {
        if (projectId === projectA) {
          // Keep recents pending with the same deferred project until A resolves.
          await deferredA;
          return recentA;
        }
        return recentB;
      }),
    });

    const { rerender, onOpenResource } = renderPane({ gateway, projectId: projectA });

    await user.click(screen.getByRole("button", { name: /资源/i }));
    await waitFor(() => {
      expect(gateway.listProjectResources).toHaveBeenCalledWith({
        projectId: projectA,
      });
    });

    rerender(
      <ResourcePane
        projectId={projectB}
        gateway={gateway}
        onOpenResource={onOpenResource}
      />,
    );

    await waitFor(() => {
      expect(gateway.listProjectResources).toHaveBeenCalledWith({
        projectId: projectB,
      });
    });

    await screen.findByRole("button", { name: /package\.json/ });
    expect(screen.queryByText("README.md")).toBeNull();
    expect(screen.getByRole("button", { name: /lib\/mod\.rs/ })).toBeInTheDocument();

    // Late A resolution must not clobber B.
    await act(async () => {
      resolveA?.(rootEntriesA);
      await deferredA;
    });

    expect(screen.queryByText("README.md")).toBeNull();
    expect(screen.getByRole("button", { name: /package\.json/ })).toBeInTheDocument();
  });

  it("calls onOpenResource with only portable relative paths from recents, tree, and search", async () => {
    const user = userEvent.setup();
    const onOpenResource = vi.fn();
    const gateway = createMockGateway({
      listProjectResources: vi.fn(
        async ({ projectId, relativePath }): Promise<ResourceEntry[]> => {
          if (relativePath === "src") {
            return [
              { path: "src/main.rs", kind: "file", sizeBytes: 12 },
              { path: "src/lib.rs", kind: "file", sizeBytes: 8 },
            ];
          }
          if (projectId === projectA) return rootEntriesA;
          return [];
        },
      ),
    });

    renderPane({ gateway, onOpenResource });
    await user.click(screen.getByRole("button", { name: /资源/i }));

    await screen.findByRole("button", { name: /src\/main\.rs/ });
    await user.click(screen.getByRole("button", { name: /src\/main\.rs/ }));
    expect(onOpenResource).toHaveBeenLastCalledWith("src/main.rs");

    await user.click(screen.getByRole("button", { name: /^src$/ }));
    await screen.findByRole("button", { name: /src\/lib\.rs/ });
    await user.click(screen.getByRole("button", { name: /src\/lib\.rs/ }));
    expect(onOpenResource).toHaveBeenLastCalledWith("src/lib.rs");

    for (const call of onOpenResource.mock.calls) {
      const path = call[0] as string;
      expect(path).not.toMatch(/^[A-Za-z]:\\/);
      expect(path).not.toMatch(/\\/);
      expect(path.startsWith("/")).toBe(false);
      expect(path.includes("..")).toBe(false);
    }
  });

  it("debounces and cancels search when the query changes or project switches", async () => {
    vi.useFakeTimers();

    let resolveFirst: ((value: ResourceSearchResult[]) => void) | undefined;
    const firstSearch = new Promise<ResourceSearchResult[]>((resolve) => {
      resolveFirst = resolve;
    });
    let searchCalls = 0;

    const gateway = createMockGateway({
      searchProjectResources: vi.fn(async ({ projectId, query }) => {
        searchCalls += 1;
        if (searchCalls === 1) {
          await firstSearch;
          return [{ path: "stale.rs", line: 1, column: 1, preview: "stale" }];
        }
        if (projectId === projectB) {
          return [{ path: "lib/mod.rs", line: 2, column: 1, preview: query }];
        }
        return searchHits;
      }),
    });

    const onOpenResource = vi.fn();
    const { rerender } = render(
      <ResourcePane
        projectId={projectA}
        gateway={gateway}
        onOpenResource={onOpenResource}
      />,
    );

    // fireEvent + act avoids userEvent/waitFor hangs under fake timers.
    fireEvent.click(screen.getByRole("button", { name: /资源/i }));
    await act(async () => {
      await Promise.resolve();
    });
    expect(gateway.listProjectResources).toHaveBeenCalledWith({
      projectId: projectA,
    });

    const search = screen.getByLabelText("搜索项目资源");
    fireEvent.change(search, { target: { value: "mai" } });

    // Still within debounce — no search yet.
    expect(gateway.searchProjectResources).not.toHaveBeenCalled();

    await act(async () => {
      vi.advanceTimersByTime(RESOURCE_SEARCH_DEBOUNCE_MS);
      await Promise.resolve();
    });

    expect(gateway.searchProjectResources).toHaveBeenCalledWith({
      projectId: projectA,
      query: "mai",
    });

    // Change project before first search resolves — stale hit must not appear.
    rerender(
      <ResourcePane
        projectId={projectB}
        gateway={gateway}
        onOpenResource={onOpenResource}
      />,
    );
    await act(async () => {
      await Promise.resolve();
    });
    expect(gateway.listProjectResources).toHaveBeenCalledWith({
      projectId: projectB,
    });

    const searchB = screen.getByLabelText("搜索项目资源");
    fireEvent.change(searchB, { target: { value: "mod" } });

    await act(async () => {
      vi.advanceTimersByTime(RESOURCE_SEARCH_DEBOUNCE_MS);
      await Promise.resolve();
    });

    expect(gateway.searchProjectResources).toHaveBeenCalledWith({
      projectId: projectB,
      query: "mod",
    });

    await act(async () => {
      resolveFirst?.([{ path: "stale.rs", line: 1, column: 1, preview: "stale" }]);
      await firstSearch;
      await Promise.resolve();
    });

    expect(screen.getByRole("button", { name: /lib\/mod\.rs/ })).toBeInTheDocument();
    expect(screen.queryByText("stale.rs")).toBeNull();

    fireEvent.click(screen.getByRole("button", { name: /lib\/mod\.rs/ }));
    expect(onOpenResource).toHaveBeenCalledWith("lib/mod.rs");
  });

  it("keeps keyboard focus labels on toggle, search, and resource rows", async () => {
    const user = userEvent.setup();
    renderPane();

    await user.tab();
    expect(screen.getByRole("button", { name: /资源/i })).toHaveFocus();

    await user.keyboard("{Enter}");
    expect(await screen.findByRole("region", { name: "项目资源" })).toBeInTheDocument();

    await user.tab();
    expect(screen.getByLabelText("搜索项目资源")).toHaveFocus();

    await waitFor(() => {
      expect(screen.getByRole("button", { name: /src\/main\.rs/ })).toBeInTheDocument();
    });

    // Walk focus into recents / tree rows — all labelled buttons.
    await user.tab();
    const focused = document.activeElement as HTMLElement | null;
    expect(focused).not.toBeNull();
    expect(focused?.getAttribute("aria-label") || focused?.textContent).toBeTruthy();
    expect(focused?.tagName).toMatch(/BUTTON|INPUT/);
  });

  it("shows opaque inline errors and never exposes machine paths", async () => {
    const user = userEvent.setup();
    const gateway = createMockGateway({
      listProjectResources: vi.fn(async () => {
        throw new ResourceGatewayError(
          "resource_unavailable",
          "Failed under F:\\secret\\projects\\alpha",
        );
      }),
      listRecentProjectResources: vi.fn(async () => {
        throw new ResourceGatewayError(
          "resource_unavailable",
          "Failed under F:\\secret\\projects\\alpha",
        );
      }),
    });

    renderPane({ gateway });
    await user.click(screen.getByRole("button", { name: /资源/i }));

    const alert = await screen.findByRole("alert");
    expect(alert.textContent).toBeTruthy();
    expect(alert.textContent).not.toMatch(/F:\\/);
    expect(alert.textContent).not.toMatch(/secret/);
    expect(alert.textContent).not.toMatch(/projects\\alpha/);
    expect(document.body.textContent).not.toMatch(/F:\\secret/);
  });

  it("shows empty inline states without dashboard chrome", async () => {
    const user = userEvent.setup();
    const gateway = createMockGateway({
      listProjectResources: vi.fn(async () => []),
      listRecentProjectResources: vi.fn(async () => []),
    });

    renderPane({ gateway });
    await user.click(screen.getByRole("button", { name: /资源/i }));

    const region = await screen.findByRole("region", { name: "项目资源" });
    await waitFor(() => {
      expect(within(region).getByText(/暂无文件|没有文件|空/)).toBeInTheDocument();
    });

    expect(region.querySelectorAll("[data-stat], .stat-card, .dashboard-card").length).toBe(
      0,
    );
    expect(within(region).queryByText(/计数|总数|files total/i)).toBeNull();
  });

  it("supports controlled open mode without an inner disclosure toggle", async () => {
    const onOpenChange = vi.fn();
    const onOpenResource = vi.fn();
    const gateway = createMockGateway();

    const { rerender } = render(
      <ResourcePane
        projectId={projectA}
        gateway={gateway}
        onOpenResource={onOpenResource}
        open={true}
        onOpenChange={onOpenChange}
        showToggle={false}
      />,
    );

    expect(screen.queryByRole("button", { name: /打开资源面板|关闭资源面板/ })).toBeNull();
    expect(screen.queryByRole("button", { name: /^资源$/ })).toBeNull();
    expect(await screen.findByRole("region", { name: "项目资源" })).toBeInTheDocument();

    await waitFor(() => {
      expect(gateway.listProjectResources).toHaveBeenCalledWith({
        projectId: projectA,
      });
    });

    rerender(
      <ResourcePane
        projectId={projectA}
        gateway={gateway}
        onOpenResource={onOpenResource}
        open={false}
        onOpenChange={onOpenChange}
        showToggle={false}
      />,
    );

    expect(screen.queryByRole("region", { name: "项目资源" })).toBeNull();
    expect(onOpenChange).not.toHaveBeenCalled();
  });

  it("keeps uncontrolled closed-by-default when open is omitted", () => {
    renderPane();
    expect(screen.getByRole("button", { name: /资源/i })).toHaveAttribute(
      "aria-expanded",
      "false",
    );
    expect(screen.queryByRole("region", { name: "项目资源" })).toBeNull();
  });
});

/**
 * Workbench contextual ResourcePane — Git Changes mode (C4 D3B4 Round 2A).
 * View-only: host supplies GitChangesPaneModel; pane never calls Git gateway.
 */
const workbenchProjectId = "00000000-0000-0000-0000-0000000000aa";

function workbenchGitStatus(
  overrides: Partial<GitStatusView> = {},
): GitStatusView {
  return {
    repositoryId: ".",
    generation: 1,
    branch: "main",
    detached: false,
    headShort: "abcdef0",
    dirtyCount: 1,
    files: [{ path: "src/app.ts", kind: "modified" }],
    truncated: false,
    ...overrides,
  };
}

function workbenchGitDiff(
  overrides: Partial<GitDiffView> = {},
): GitDiffView {
  return {
    repositoryId: ".",
    generation: 1,
    path: "src/app.ts",
    kind: "text",
    unified: "@@ -1 +1 @@\n-old\n+new\n",
    truncated: false,
    lineCount: 3,
    byteCount: 20,
    ...overrides,
  };
}

function gitChangesModel(
  overrides: Partial<GitChangesPaneModel> = {},
): GitChangesPaneModel {
  return {
    repositories: [{ repositoryId: ".", available: true }],
    selectedRepositoryId: ".",
    status: workbenchGitStatus(),
    diff: null,
    loading: false,
    error: null,
    stale: false,
    onSelectRepository: vi.fn(),
    onSelectPath: vi.fn(),
    onRefresh: vi.fn(),
    ...overrides,
  };
}

function renderWorkbenchPane(
  props: Partial<{
    projectId: string | null;
    hasProject: boolean;
    open: boolean;
    gitChanges: GitChangesPaneModel | null;
    gitChangesCapable: boolean;
    bodyMode: "resources" | "changes";
    onBodyModeChange: (mode: "resources" | "changes") => void;
    resourceGateway: ResourceGateway;
    writeDiffTarget: {
      projectId: string;
      conversationId: string;
      proposalId: string;
      scopeGeneration: number;
    } | null;
    terminalOutputTarget: {
      projectId: string;
      conversationId: string;
      proposalId: string;
      programLabel: string;
      argv: string[];
      scopeGeneration: number;
    } | null;
  }> = {},
) {
  const onClose = vi.fn();
  const onBodyModeChange = props.onBodyModeChange ?? vi.fn();
  const projectId =
    props.projectId === undefined ? workbenchProjectId : props.projectId;
  const gitChangesCapable =
    props.gitChangesCapable
    ?? (props.gitChanges != null || props.bodyMode === "changes");
  const view = render(
    <WorkbenchResourcePane
      open={props.open ?? true}
      onClose={onClose}
      hasProject={props.hasProject ?? true}
      projectId={projectId}
      resource={null}
      resourceGateway={props.resourceGateway}
      gitChanges={props.gitChanges}
      gitChangesCapable={gitChangesCapable}
      bodyMode={props.bodyMode ?? "resources"}
      onBodyModeChange={onBodyModeChange}
      writeDiffTarget={props.writeDiffTarget ?? null}
      terminalOutputTarget={props.terminalOutputTarget ?? null}
    />,
  );
  return { ...view, onClose, onBodyModeChange, projectId };
}

describe("Workbench ResourcePane — Git Changes mode (C4 D3B4)", () => {
  it("defaults to the resource body even when gitChanges model is present", () => {
    const model = gitChangesModel({
      diff: workbenchGitDiff(),
    });
    const gateway = createMockGateway();
    renderWorkbenchPane({
      gitChanges: model,
      gitChangesCapable: true,
      resourceGateway: gateway,
    });

    expect(screen.queryByTestId("git-changes-pane")).toBeNull();
    expect(screen.queryByTestId("git-changes-view")).toBeNull();
    // Mode switcher present: pane h2 is generic; mode name lives on the tab.
    expect(screen.getByRole("heading", { name: "上下文", level: 2 })).toBeInTheDocument();
    expect(screen.getByRole("region", { name: "项目资源" })).toBeInTheDocument();
    expect(screen.getByTestId("context-body-mode-switcher")).toBeInTheDocument();
    expect(screen.getByTestId("context-body-mode-resources")).toHaveAttribute(
      "aria-selected",
      "true",
    );
    expect(screen.getByTestId("context-body-mode-changes")).toHaveAttribute(
      "aria-selected",
      "false",
    );
  });

  it("defaults to resource body with region 项目资源 when the pane opens", async () => {
    const gateway = createMockGateway();
    renderWorkbenchPane({
      gitChanges: null,
      gitChangesCapable: true,
      resourceGateway: gateway,
    });

    expect(
      await screen.findByRole("region", { name: "项目资源" }),
    ).toBeInTheDocument();
    expect(screen.queryByTestId("git-changes-pane")).toBeNull();
    expect(screen.getByRole("heading", { name: "上下文", level: 2 })).toBeInTheDocument();
  });

  it("mounts compact Changes mode only after explicit mode selection", async () => {
    const user = userEvent.setup();
    const onBodyModeChange = vi.fn();
    const model = gitChangesModel({
      diff: workbenchGitDiff(),
    });
    const { rerender, onClose } = renderWorkbenchPane({
      gitChanges: model,
      gitChangesCapable: true,
      bodyMode: "resources",
      onBodyModeChange,
    });

    expect(screen.queryByTestId("git-changes-pane")).toBeNull();

    await user.click(screen.getByTestId("context-body-mode-changes"));
    expect(onBodyModeChange).toHaveBeenCalledWith("changes");

    // Controlled host applies the selected mode.
    rerender(
      <WorkbenchResourcePane
        open
        onClose={onClose}
        hasProject
        projectId={workbenchProjectId}
        resource={null}
        gitChanges={model}
        gitChangesCapable
        bodyMode="changes"
        onBodyModeChange={onBodyModeChange}
      />,
    );

    expect(screen.getByTestId("git-changes-pane")).toBeInTheDocument();
    expect(screen.getByTestId("git-changes-view")).toBeInTheDocument();
    // Compact: generic pane heading; mode name only on the selected tab.
    expect(screen.getByRole("heading", { name: "上下文", level: 2 })).toBeInTheDocument();
    expect(screen.queryByRole("heading", { name: "变更" })).toBeNull();
    expect(screen.getByTestId("git-changes-branch")).toHaveTextContent("main");
    expect(screen.getByTestId("git-changes-dirty-count")).toHaveTextContent(
      "1",
    );
    expect(screen.getByTestId("git-changes-diff-path")).toHaveTextContent(
      "src/app.ts",
    );
    // No permanent dashboard chrome beyond the compact body.
    expect(screen.queryByTestId("terminal-output-pane")).toBeNull();
    expect(screen.queryByTestId("write-diff-pane")).toBeNull();
  });

  it("does not show Changes mode when gitChanges is null and mode stays resources", () => {
    renderWorkbenchPane({
      gitChanges: null,
      gitChangesCapable: false,
      hasProject: true,
    });
    expect(screen.queryByTestId("git-changes-pane")).toBeNull();
    expect(screen.queryByTestId("git-changes-view")).toBeNull();
    expect(screen.queryByTestId("context-body-mode-switcher")).toBeNull();
    // No switcher: single mode-specific pane heading remains.
    expect(screen.getByRole("heading", { name: "资源", level: 2 })).toBeInTheDocument();
  });

  it("shows each mode name once while tabs keep accessible names and testids", async () => {
    const user = userEvent.setup();
    const onBodyModeChange = vi.fn();
    const model = gitChangesModel({
      diff: workbenchGitDiff(),
    });
    const gateway = createMockGateway();
    const { rerender, onClose } = renderWorkbenchPane({
      gitChanges: model,
      gitChangesCapable: true,
      bodyMode: "resources",
      onBodyModeChange,
      resourceGateway: gateway,
    });

    // Resources mode: exact mode label "资源" only once (the tab).
    expect(screen.getAllByText("资源")).toHaveLength(1);
    expect(screen.queryByText("变更", { selector: "h2, h3" })).toBeNull();
    const resourcesTab = screen.getByRole("tab", { name: "资源" });
    const changesTab = screen.getByRole("tab", { name: "变更" });
    expect(resourcesTab).toHaveAttribute("aria-selected", "true");
    expect(changesTab).toHaveAttribute("aria-selected", "false");
    expect(resourcesTab).toHaveAttribute("data-testid", "context-body-mode-resources");
    expect(changesTab).toHaveAttribute("data-testid", "context-body-mode-changes");
    expect(screen.getByTestId("context-body-mode-switcher")).toBeInTheDocument();
    expect(screen.getByTestId("resource-pane")).toBeInTheDocument();
    expect(screen.getByTestId("resource-pane-close")).toBeInTheDocument();

    await user.click(changesTab);
    expect(onBodyModeChange).toHaveBeenCalledWith("changes");

    rerender(
      <WorkbenchResourcePane
        open
        onClose={onClose}
        hasProject
        projectId={workbenchProjectId}
        resource={null}
        resourceGateway={gateway}
        gitChanges={model}
        gitChangesCapable
        bodyMode="changes"
        onBodyModeChange={onBodyModeChange}
      />,
    );

    // Changes mode: exact mode label "变更" only once (the tab); no view h3.
    expect(screen.getAllByText("变更")).toHaveLength(1);
    expect(screen.queryByRole("heading", { name: "变更" })).toBeNull();
    expect(screen.getByRole("tab", { name: "变更" })).toHaveAttribute(
      "aria-selected",
      "true",
    );
    expect(screen.getByRole("tab", { name: "资源" })).toHaveAttribute(
      "aria-selected",
      "false",
    );
    // Body region remains named for AT without a duplicate visible title.
    expect(screen.getByTestId("git-changes-pane")).toBeInTheDocument();
    expect(screen.getByTestId("git-changes-view")).toBeInTheDocument();
    expect(screen.getByTestId("git-changes-branch")).toBeInTheDocument();
    expect(screen.getByTestId("git-changes-head")).toBeInTheDocument();
    expect(screen.getByTestId("git-changes-dirty-count")).toBeInTheDocument();
    expect(screen.getByTestId("git-changes-file-list")).toBeInTheDocument();
    expect(screen.getByTestId("git-changes-entry-0")).toBeInTheDocument();
    expect(screen.getByTestId("git-changes-diff")).toBeInTheDocument();
    expect(screen.getByTestId("git-changes-diff-path")).toBeInTheDocument();
    expect(screen.getByTestId("git-changes-refresh")).toBeInTheDocument();
  });

  it("places Terminal and governed-write Diff ahead of Changes mode", () => {
    const model = gitChangesModel();
    const { rerender, onClose } = renderWorkbenchPane({
      gitChanges: model,
      gitChangesCapable: true,
      bodyMode: "changes",
      terminalOutputTarget: {
        projectId: workbenchProjectId,
        conversationId: "conv-1",
        proposalId: "prop-1",
        programLabel: "cargo",
        argv: ["test"],
        scopeGeneration: 1,
      },
    });

    expect(screen.getByTestId("terminal-output-pane")).toBeInTheDocument();
    expect(screen.queryByTestId("git-changes-pane")).toBeNull();
    expect(screen.queryByTestId("context-body-mode-switcher")).toBeNull();
    expect(screen.getByRole("heading", { name: "终端输出", level: 2 }))
      .toBeInTheDocument();

    rerender(
      <WorkbenchResourcePane
        open
        onClose={onClose}
        hasProject
        projectId={workbenchProjectId}
        resource={null}
        gitChanges={model}
        gitChangesCapable
        bodyMode="changes"
        writeDiffTarget={{
          projectId: workbenchProjectId,
          conversationId: "conv-1",
          proposalId: "prop-write",
          scopeGeneration: 1,
        }}
        terminalOutputTarget={null}
      />,
    );

    expect(screen.getByTestId("write-diff-pane")).toBeInTheDocument();
    expect(screen.queryByTestId("git-changes-pane")).toBeNull();
    expect(screen.queryByTestId("context-body-mode-switcher")).toBeNull();
    expect(screen.getByRole("heading", { name: "写入差异", level: 2 }))
      .toBeInTheDocument();
  });

  it("routes refresh, path and repository clicks only through supplied callbacks", async () => {
    const user = userEvent.setup();
    const onRefresh = vi.fn();
    const onSelectPath = vi.fn();
    const onSelectRepository = vi.fn();
    const model = gitChangesModel({
      repositories: [
        { repositoryId: ".", available: true },
        { repositoryId: "nested/pkg", available: true },
        { repositoryId: "broken/repo", available: false },
      ],
      selectedRepositoryId: ".",
      onRefresh,
      onSelectPath,
      onSelectRepository,
      stale: true,
    });
    const { container } = renderWorkbenchPane({
      gitChanges: model,
      gitChangesCapable: true,
      bodyMode: "changes",
    });

    expect(screen.getByTestId("git-changes-repo-selector")).toBeInTheDocument();
    expect(screen.getByTestId("git-changes-repo-broken/repo")).toBeDisabled();

    await user.click(screen.getByTestId("git-changes-refresh"));
    expect(onRefresh).toHaveBeenCalledTimes(1);

    await user.click(screen.getByTestId("git-changes-entry-0"));
    expect(onSelectPath).toHaveBeenCalledWith("src/app.ts");

    await user.click(screen.getByTestId("git-changes-repo-nested/pkg"));
    expect(onSelectRepository).toHaveBeenCalledWith("nested/pkg");

    await user.click(screen.getByTestId("git-changes-repo-broken/repo"));
    expect(onSelectRepository).not.toHaveBeenCalledWith("broken/repo");

    const text = container.textContent ?? "";
    expect(text).not.toMatch(/F:\\|C:\\|\\\\server/);
    expect(text).not.toMatch(/argv|sessionId|executor/i);
  });

  it("preserves Files/Preview path when gitChanges is absent and gateway is present", async () => {
    const gateway = createMockGateway();
    renderWorkbenchPane({
      gitChanges: null,
      gitChangesCapable: false,
      resourceGateway: gateway,
    });

    expect(screen.queryByTestId("git-changes-pane")).toBeNull();
    expect(await screen.findByRole("region", { name: "项目资源" })).toBeInTheDocument();
    await waitFor(() => {
      expect(gateway.listProjectResources).toHaveBeenCalledWith({
        projectId: workbenchProjectId,
      });
    });
  });

  it("keeps resource body when mode is resources even if git model has status", async () => {
    const gateway = createMockGateway();
    const model = gitChangesModel({
      status: workbenchGitStatus({ branch: "should-not-paint", dirtyCount: 9 }),
    });
    renderWorkbenchPane({
      gitChanges: model,
      gitChangesCapable: true,
      bodyMode: "resources",
      resourceGateway: gateway,
    });

    expect(await screen.findByRole("region", { name: "项目资源" })).toBeInTheDocument();
    expect(screen.queryByTestId("git-changes-pane")).toBeNull();
    expect(screen.queryByText("should-not-paint")).toBeNull();
    expect(screen.queryByText("9")).toBeNull();
  });
});
