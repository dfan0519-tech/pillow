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
