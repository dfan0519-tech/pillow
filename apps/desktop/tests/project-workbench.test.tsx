import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it, vi } from "vitest";
import { App } from "../src/App";
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
    id: "keeper-ide",
    name: "Keeper IDE",
    rootPath: "<workspace>/VaultSystem\\keeper-ide",
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

    await user.click(await screen.findByRole("button", { name: "Keeper IDE" }));

    expect(screen.getByTestId("active-project-name")).toHaveTextContent("Keeper IDE");
    expect(screen.getByTestId("project-conversations")).toBeVisible();
    expect(screen.getByRole("group", { name: "项目对话" })).toBeVisible();
  });

  it("does not render provider internals or Grok branding", async () => {
    const user = userEvent.setup();
    render(<App gateway={fixtureGateway()} />);

    await user.click(await screen.findByLabelText("Keeper IDE"));
    await screen.findByRole("region", { name: "对话工作台" });

    expect(screen.queryByText(/routeCode|lease|slotId|Grok|SuperGrok/i)).not.toBeInTheDocument();
  });

  it("keeps a long project name constrained at the 1024px minimum width", async () => {
    const user = userEvent.setup();
    const longName = "Keeper IDE project with a deliberately very long name ".repeat(8);
    Object.defineProperty(window, "innerWidth", { configurable: true, value: 1024 });
    window.dispatchEvent(new Event("resize"));
    render(<App gateway={fixtureGateway([fixtureProject({ name: longName })])} />);

    await user.click(await screen.findByRole("button", {
      name: /Keeper IDE project with a deliberately very long name/,
    }));

    const heading = await screen.findByTestId("active-project-name");
    const headingStyle = getComputedStyle(heading);
    expect(heading).toHaveAttribute("title", longName);
    expect(headingStyle.overflow).toBe("hidden");
    expect(headingStyle.textOverflow).toBe("ellipsis");
    expect(headingStyle.whiteSpace).toBe("nowrap");
    expect(getComputedStyle(heading.parentElement as HTMLElement).minWidth).toBe("0");
  });
});
