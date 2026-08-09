import { act, fireEvent, render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it, vi } from "vitest";
import { App } from "../src/App";
import type { ConversationGateway } from "../src/lib/conversationGateway";
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
    id: "pillow",
    name: "Pillow",
    rootPath: "C:\\Users\\Alice\\Projects\\pillow",
    trust: "trusted",
    createdAt: "2026-07-29T00:00:00.000Z",
    updatedAt: "2026-07-29T00:00:00.000Z",
    ...overrides,
  };
}

function fixtureGateway(
  projects: ProjectDto[] = [fixtureProject()],
  overrides: Partial<ProjectGateway> = {},
): ProjectGateway {
  const store = [...projects];
  const gateway: ProjectGateway = {
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
  return gateway;
}

describe("project workbench shell", () => {
  it("launches into the full shell without a full-screen launcher", async () => {
    render(<App gateway={fixtureGateway([])} />);

    expect(await screen.findByTestId("empty-workbench")).toBeVisible();
    expect(screen.getByTestId("app-sidebar")).toBeVisible();
    expect(screen.getByRole("navigation", { name: "导航" })).toBeVisible();
    expect(screen.getByRole("region", { name: "对话工作台" })).toBeVisible();
    expect(screen.queryByRole("main", { name: "项目启动器" })).not.toBeInTheDocument();
    expect(screen.queryByLabelText("项目目录")).not.toBeInTheDocument();
    // Single continuous left rail — no second conversation column rail
    expect(document.querySelectorAll(".app-sidebar").length).toBe(1);
    expect(document.querySelector(".conversation-sidebar")).toBeNull();
    expect(document.querySelector(".project-sidebar")).toBeNull();
  });

  it("marks readiness only after the initial project list succeeds", async () => {
    const onProjectLauncherReady = vi.fn().mockResolvedValue(undefined);

    render(
      <App
        gateway={fixtureGateway()}
        onProjectLauncherReady={onProjectLauncherReady}
      />,
    );

    await screen.findByTestId("empty-workbench-title");
    await waitFor(() => expect(onProjectLauncherReady).toHaveBeenCalledOnce());
  });

  it("does not mark readiness when the initial project list fails", async () => {
    const gateway = fixtureGateway();
    gateway.listProjects = vi.fn().mockRejectedValue(new ProjectGatewayError(
      "project_store_unavailable",
      "Project storage is temporarily unavailable.",
    ));
    const onProjectLauncherReady = vi.fn().mockResolvedValue(undefined);

    render(
      <App
        gateway={gateway}
        onProjectLauncherReady={onProjectLauncherReady}
      />,
    );

    await screen.findByRole("alert");
    await waitFor(() => expect(gateway.listProjects).toHaveBeenCalledOnce());
    expect(onProjectLauncherReady).not.toHaveBeenCalled();
  });

  it("opens a folder via picker and registers/selects without path typing", async () => {
    const user = userEvent.setup();
    const opened = fixtureProject({
      id: "opened",
      name: "demo-folder",
      rootPath: "F:\\work\\demo-folder",
      trust: "untrusted",
    });
    const gateway = fixtureGateway([]);
    gateway.pickDirectory = vi.fn().mockResolvedValue(opened.rootPath);
    gateway.openOrRegisterProject = vi.fn().mockResolvedValue(opened);
    gateway.selectProject = vi.fn().mockResolvedValue(opened);

    render(<App gateway={gateway} />);

    await user.click(await screen.findByTestId("nav-open-project"));

    await waitFor(() => {
      expect(gateway.pickDirectory).toHaveBeenCalledOnce();
      expect(gateway.openOrRegisterProject).toHaveBeenCalledWith(opened.rootPath);
      expect(gateway.selectProject).toHaveBeenCalledWith(opened.id);
    });
    expect(await screen.findByTestId("active-project-name")).toHaveTextContent("demo-folder");
    expect(screen.queryByLabelText("项目目录")).not.toBeInTheDocument();
  });

  it("selects an existing registered project when the path already exists", async () => {
    const user = userEvent.setup();
    const existing = fixtureProject();
    const gateway = fixtureGateway([existing]);
    gateway.pickDirectory = vi.fn().mockResolvedValue(existing.rootPath);
    gateway.openOrRegisterProject = vi.fn().mockResolvedValue(existing);
    gateway.selectProject = vi.fn().mockResolvedValue(existing);
    gateway.createProject = vi.fn();

    render(<App gateway={gateway} />);
    await user.click(await screen.findByTestId("nav-open-project"));

    await waitFor(() => {
      expect(gateway.openOrRegisterProject).toHaveBeenCalledWith(existing.rootPath);
      expect(gateway.selectProject).toHaveBeenCalledWith(existing.id);
    });
    expect(gateway.createProject).not.toHaveBeenCalled();
  });

  it("hides unsafe new-folder creation while keeping open-folder available", async () => {
    const gateway = fixtureGateway([]);

    render(<App gateway={gateway} />);

    await screen.findByTestId("empty-workbench");
    expect(screen.queryByTestId("nav-new-project")).not.toBeInTheDocument();
    expect(screen.getByTestId("nav-open-project")).toBeEnabled();
  });

  it("keeps the selected project visible above its nested conversations", async () => {
    const user = userEvent.setup();
    render(<App gateway={fixtureGateway()} />);

    await user.click(await screen.findByRole("button", { name: "Pillow" }));

    expect(screen.getByTestId("active-project-name")).toHaveTextContent("Pillow");
    expect(screen.getByTestId("project-conversations")).toBeVisible();
    expect(screen.getByRole("group", { name: "项目对话" })).toBeVisible();
    expect(screen.getByLabelText("项目：Pillow")).toBeVisible();
    expect(screen.getByLabelText("运行时：模拟")).toBeVisible();
    expect(screen.getByLabelText("权限：按需批准")).toBeVisible();
  });

  it("does not render provider internals or Grok branding", async () => {
    const user = userEvent.setup();
    render(<App gateway={fixtureGateway()} />);

    await user.click(await screen.findByLabelText("Pillow"));
    await screen.findByRole("region", { name: "对话工作台" });

    expect(screen.queryByText(/routeCode|lease|slotId|Grok|SuperGrok/i)).not.toBeInTheDocument();
  });

  it("does not leak native project rootPath into sidebar DOM text or attributes", async () => {
    const sentinelRoot =
      "F:\\tmp\\c3f-native-privacy-sentinel\\project-alpha";
    const project = fixtureProject({
      id: "privacy-sentinel",
      name: "Privacy Sentinel",
      rootPath: sentinelRoot,
    });

    render(<App gateway={fixtureGateway([project])} />);

    const row = await screen.findByRole("button", { name: "Privacy Sentinel" });
    expect(row).toBeVisible();
    expect(row).toHaveTextContent("Privacy Sentinel");
    // Name-only tooltip is allowed; never the native root or parent path.
    expect(row).toHaveAttribute("title", "Privacy Sentinel");
    expect(row.getAttribute("title")).not.toContain("F:\\");
    expect(row.getAttribute("title")).not.toContain(sentinelRoot);

    const bodyHtml = document.body.innerHTML;
    const bodyText = document.body.innerText ?? document.body.textContent ?? "";
    expect(bodyText).not.toContain(sentinelRoot);
    expect(bodyHtml).not.toContain(sentinelRoot);
    expect(bodyHtml).not.toContain("F:\\tmp\\c3f-native-privacy-sentinel");
    expect(bodyHtml).not.toContain("file://");

    const sidebarHtml = screen.getByTestId("app-sidebar").innerHTML;
    expect(sidebarHtml).not.toContain(sentinelRoot);
    expect(sidebarHtml).not.toMatch(/[A-Za-z]:\\/u);
    expect(sidebarHtml).not.toContain("file://");
  });

  it("keeps a long project name constrained at the 1024px minimum width", async () => {
    const user = userEvent.setup();
    const longName = "Pillow project with a deliberately very long name ".repeat(8);
    Object.defineProperty(window, "innerWidth", { configurable: true, value: 1024 });
    window.dispatchEvent(new Event("resize"));
    render(<App gateway={fixtureGateway([fixtureProject({ name: longName })])} />);

    await user.click(await screen.findByRole("button", {
      name: /Pillow project with a deliberately very long name/,
    }));

    const heading = await screen.findByTestId("active-project-name");
    const headingStyle = getComputedStyle(heading);
    expect(heading).toHaveAttribute("title", longName);
    expect(headingStyle.overflow).toBe("hidden");
    expect(headingStyle.textOverflow).toBe("ellipsis");
    expect(headingStyle.whiteSpace).toBe("nowrap");
    expect(getComputedStyle(heading.parentElement as HTMLElement).minWidth).toBe("0");
  });

  it("last-request-wins: delayed select success for A cannot overwrite B", async () => {
    const projectA = fixtureProject({
      id: "project-a",
      name: "Project A",
      rootPath: "F:\\work\\project-a",
    });
    const projectB = fixtureProject({
      id: "project-b",
      name: "Project B",
      rootPath: "F:\\work\\project-b",
    });

    let resolveA: ((value: ProjectDto) => void) | undefined;
    let resolveB: ((value: ProjectDto) => void) | undefined;
    const deferredA = new Promise<ProjectDto>((resolve) => {
      resolveA = resolve;
    });
    const deferredB = new Promise<ProjectDto>((resolve) => {
      resolveB = resolve;
    });

    const gateway = fixtureGateway([projectA, projectB], {
      selectProject: vi.fn(async (projectId: string) => {
        if (projectId === projectA.id) return deferredA;
        if (projectId === projectB.id) return deferredB;
        throw new ProjectGatewayError("project_not_found", "Project not found.");
      }),
    });

    render(<App gateway={gateway} />);
    await screen.findByTestId("empty-workbench");

    // Batch both clicks in one act so both handlers run before busy disables buttons.
    await act(async () => {
      fireEvent.click(screen.getByRole("button", { name: "Project A" }));
      fireEvent.click(screen.getByRole("button", { name: "Project B" }));
    });

    expect(gateway.selectProject).toHaveBeenCalledWith(projectA.id);
    expect(gateway.selectProject).toHaveBeenCalledWith(projectB.id);

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
    expect(screen.queryByText("Project A")).not.toBeNull(); // still listed
    expect(screen.queryByRole("alert")).not.toBeInTheDocument();
    // Active heading must remain B — not overwritten by late A.
    expect(screen.getByTestId("active-project-name")).not.toHaveTextContent("Project A");
  });

  it("last-request-wins: delayed select failure for A cannot error banner over B", async () => {
    const projectA = fixtureProject({
      id: "project-a",
      name: "Project A",
      rootPath: "F:\\work\\project-a",
    });
    const projectB = fixtureProject({
      id: "project-b",
      name: "Project B",
      rootPath: "F:\\work\\project-b",
    });

    let rejectA: ((reason?: unknown) => void) | undefined;
    let resolveB: ((value: ProjectDto) => void) | undefined;
    const deferredA = new Promise<ProjectDto>((_resolve, reject) => {
      rejectA = reject;
    });
    const deferredB = new Promise<ProjectDto>((resolve) => {
      resolveB = resolve;
    });

    const gateway = fixtureGateway([projectA, projectB], {
      selectProject: vi.fn(async (projectId: string) => {
        if (projectId === projectA.id) return deferredA;
        if (projectId === projectB.id) return deferredB;
        throw new ProjectGatewayError("project_not_found", "Project not found.");
      }),
    });

    render(<App gateway={gateway} />);
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
  });

  it("last-request-wins: delayed open success cannot overwrite later B selection", async () => {
    const projectB = fixtureProject({
      id: "project-b",
      name: "Project B",
      rootPath: "F:\\work\\project-b",
    });
    const opened = fixtureProject({
      id: "opened-stale",
      name: "stale-opened",
      rootPath: "F:\\work\\stale-opened",
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
      if (projectId === projectB.id) return deferredSelectB;
      if (projectId === opened.id) return opened;
      throw new ProjectGatewayError("project_not_found", "Project not found.");
    });

    const gateway = fixtureGateway([projectB], {
      pickDirectory: vi.fn().mockResolvedValue(opened.rootPath),
      openOrRegisterProject: vi.fn(async () => deferredOpenRegister),
      selectProject,
    });

    const view = render(<App gateway={gateway} />);
    await screen.findByTestId("empty-workbench");

    // Start open first so registration is already in flight.
    await act(async () => {
      fireEvent.click(screen.getByTestId("nav-open-project"));
    });

    await waitFor(() => {
      expect(gateway.pickDirectory).toHaveBeenCalledOnce();
      expect(gateway.openOrRegisterProject).toHaveBeenCalledWith(opened.rootPath);
    });

    // Concurrent B while register is pending. Busy disables row clicks, but the
    // public default-select path (screenshot harness) can still race activation.
    await act(async () => {
      view.rerender(
        <App gateway={gateway} defaultSelectedProjectId={projectB.id} />,
      );
    });

    await waitFor(() => {
      expect(selectProject).toHaveBeenCalledWith(projectB.id);
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
  });

  it("last-request-wins: delayed open failure cannot error banner over later B selection", async () => {
    const projectB = fixtureProject({
      id: "project-b",
      name: "Project B",
      rootPath: "F:\\work\\project-b",
    });
    const openedPath = "F:\\work\\stale-opened";

    let rejectOpenRegister: ((reason?: unknown) => void) | undefined;
    const deferredOpenRegister = new Promise<ProjectDto>((_resolve, reject) => {
      rejectOpenRegister = reject;
    });
    let resolveSelectB: ((value: ProjectDto) => void) | undefined;
    const deferredSelectB = new Promise<ProjectDto>((resolve) => {
      resolveSelectB = resolve;
    });

    const selectProject = vi.fn(async (projectId: string) => {
      if (projectId === projectB.id) return deferredSelectB;
      throw new ProjectGatewayError("project_not_found", "Project not found.");
    });

    const gateway = fixtureGateway([projectB], {
      pickDirectory: vi.fn().mockResolvedValue(openedPath),
      openOrRegisterProject: vi.fn(async () => deferredOpenRegister),
      selectProject,
    });

    const view = render(<App gateway={gateway} />);
    await screen.findByTestId("empty-workbench");

    await act(async () => {
      fireEvent.click(screen.getByTestId("nav-open-project"));
    });

    await waitFor(() => {
      expect(gateway.openOrRegisterProject).toHaveBeenCalledWith(openedPath);
    });

    await act(async () => {
      view.rerender(
        <App gateway={gateway} defaultSelectedProjectId={projectB.id} />,
      );
    });

    await waitFor(() => {
      expect(selectProject).toHaveBeenCalledWith(projectB.id);
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
  });

  it("stale picker: delayed non-empty pickDirectory must not call openOrRegister after B wins", async () => {
    const projectB = fixtureProject({
      id: "project-b",
      name: "Project B",
      rootPath: "F:\\work\\project-b",
    });
    const stalePath = "F:\\work\\stale-opened";

    let resolvePick: ((value: string | null) => void) | undefined;
    const deferredPick = new Promise<string | null>((resolve) => {
      resolvePick = resolve;
    });
    let resolveSelectB: ((value: ProjectDto) => void) | undefined;
    const deferredSelectB = new Promise<ProjectDto>((resolve) => {
      resolveSelectB = resolve;
    });

    const selectProject = vi.fn(async (projectId: string) => {
      if (projectId === projectB.id) return deferredSelectB;
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

    const gateway = fixtureGateway([projectB], {
      pickDirectory: vi.fn(() => deferredPick),
      openOrRegisterProject,
      selectProject,
    });

    render(<App gateway={gateway} />);
    await screen.findByTestId("empty-workbench");

    // Open keeps the picker pending; B starts in the same batch (busy is not authority).
    await act(async () => {
      fireEvent.click(screen.getByTestId("nav-open-project"));
      fireEvent.click(screen.getByRole("button", { name: "Project B" }));
    });

    await waitFor(() => {
      expect(gateway.pickDirectory).toHaveBeenCalledOnce();
      expect(selectProject).toHaveBeenCalledWith(projectB.id);
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
    // B owns busy; open-project and project list remain interactive.
    expect(screen.getByTestId("nav-open-project")).not.toBeDisabled();
    expect(screen.getByRole("button", { name: "Project B" })).not.toBeDisabled();
  });

  it("render boundary: foreign conversation selection cannot paint under active project B", async () => {
    const rootA = "F:\\secret\\projects\\project-a-owned";
    const rootB = "F:\\secret\\projects\\project-b-owned";
    const projectA = fixtureProject({
      id: "project-a",
      name: "Project A",
      rootPath: rootA,
    });
    const projectB = fixtureProject({
      id: "project-b",
      name: "Project B",
      rootPath: rootB,
    });
    const convAId = "00000000-0000-0000-0000-0000000000a1";

    let resolveSelectB: ((value: ProjectDto) => void) | undefined;
    const deferredSelectB = new Promise<ProjectDto>((resolve) => {
      resolveSelectB = resolve;
    });
    const gateway = fixtureGateway([projectA, projectB], {
      selectProject: vi.fn(async (projectId: string) => {
        if (projectId === projectB.id) return deferredSelectB;
        if (projectId === projectA.id) return projectA;
        throw new ProjectGatewayError("project_not_found", "Project not found.");
      }),
    });

    const conversationGateway: ConversationGateway = {
      listConversations: async (projectId) =>
        projectId === projectA.id
          ? [
              {
                id: convAId,
                projectId: projectA.id,
                title: "A only chat",
                createdAt: "2026-08-01T00:00:00.000Z",
                updatedAt: "2026-08-01T00:00:00.000Z",
              },
            ]
          : [],
      createConversation: async (projectId) => ({
        id: convAId,
        projectId,
        title: "A only chat",
        createdAt: "2026-08-01T00:00:00.000Z",
        updatedAt: "2026-08-01T00:00:00.000Z",
      }),
      loadTimeline: async (conversationId) => ({
        conversationId,
        messages: [
          {
            id: "m-a",
            role: "user",
            content: "seeded A body",
            createdAt: "2026-08-01T00:00:00.000Z",
          },
        ],
        events: [],
        runs: [],
      }),
      sendTurn: async () => ({ runId: "run-a" }),
      cancelRun: async () => undefined,
      subscribe: () => () => undefined,
    };

    render(
      <App conversationGateway={conversationGateway} gateway={gateway} />,
    );
    await screen.findByTestId("empty-workbench");

    await act(async () => {
      fireEvent.click(screen.getByRole("button", { name: "Project A" }));
    });
    await waitFor(() => {
      expect(screen.getByText("A only chat")).toBeInTheDocument();
    });
    await act(async () => {
      fireEvent.click(screen.getByText("A only chat"));
    });
    await waitFor(() => {
      expect(screen.getByTestId("conversation-timeline")).toBeInTheDocument();
    });
    expect(screen.getByText("seeded A body")).toBeInTheDocument();

    // Start B; re-select A while B select is still deferred (conversation rows
    // are not busy-disabled). Ownership gate must drop A under B at paint time.
    await act(async () => {
      fireEvent.click(screen.getByRole("button", { name: "Project B" }));
    });
    await act(async () => {
      fireEvent.click(screen.getByText("A only chat"));
    });
    await act(async () => {
      resolveSelectB?.(projectB);
      await deferredSelectB;
    });

    expect(screen.getByTestId("active-project-name")).toHaveTextContent("Project B");
    expect(screen.getByTestId("active-project-name")).not.toHaveTextContent("Project A");
    expect(screen.queryByTestId("conversation-timeline")).not.toBeInTheDocument();
    expect(screen.queryByText("seeded A body")).not.toBeInTheDocument();
    expect(screen.queryByText("A only chat")).not.toBeInTheDocument();
    expect(screen.getByTestId("selected-workbench")).toBeInTheDocument();
    expect(document.body.textContent).not.toContain(rootA);
    expect(document.body.textContent).not.toContain(rootB);
    expect(document.body.innerHTML).not.toContain(rootA);
    expect(document.body.innerHTML).not.toContain(rootB);
  });
});
