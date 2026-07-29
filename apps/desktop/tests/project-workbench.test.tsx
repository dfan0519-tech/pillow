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

function fixtureGateway(overrides: Partial<ProjectDto> = {}): ProjectGateway {
  const project: ProjectDto = {
    id: "keeper-ide",
    name: "Keeper IDE",
    rootPath: "<workspace>/VaultSystem\\keeper-ide",
    trust: "trusted",
    createdAt: "2026-07-29T00:00:00.000Z",
    updatedAt: "2026-07-29T00:00:00.000Z",
    ...overrides,
  };

  return {
    listProjects: async () => [project],
    createProject: async () => project,
    selectProject: async () => project,
  };
}

describe("project workbench", () => {
  it("keeps the selected project visible above its conversations", async () => {
    const user = userEvent.setup();
    render(<App gateway={fixtureGateway()} />);

    await user.click(await screen.findByRole("button", { name: "Keeper IDE" }));

    expect(screen.getByTestId("active-project-name")).toHaveTextContent("Keeper IDE");
    expect(screen.getByRole("navigation", { name: "项目对话" })).toBeVisible();
  });

  it("does not render provider internals in the empty workbench", async () => {
    const user = userEvent.setup();
    render(<App gateway={fixtureGateway()} />);

    await user.click(await screen.findByRole("button", { name: "Keeper IDE" }));
    await screen.findByRole("region", { name: "对话工作台" });

    expect(screen.queryByText(/routeCode|lease|slotId/)).not.toBeInTheDocument();
  });

  it("keeps project details open and exposes a safe command error", async () => {
    const user = userEvent.setup();
    const gateway = fixtureGateway();
    gateway.listProjects = async () => [];
    gateway.createProject = vi.fn().mockRejectedValue(new ProjectGatewayError(
      "project_store_unavailable",
      "Project storage is temporarily unavailable.",
    ));
    render(<App gateway={gateway} />);

    await user.type(await screen.findByLabelText("项目名称"), "Broken project");
    await user.type(screen.getByLabelText("项目目录"), "Z:\\missing");
    await user.click(screen.getByRole("button", { name: "创建" }));

    const alert = await screen.findByRole("alert");
    expect(alert).toHaveTextContent("Project storage is temporarily unavailable.");
    expect(alert).toHaveTextContent("project_store_unavailable");
    expect(screen.getByLabelText("项目名称")).toHaveValue("Broken project");
    expect(screen.getByLabelText("项目目录")).toHaveValue("Z:\\missing");
  });

  it("selects a newly created project through the host before activating it", async () => {
    const user = userEvent.setup();
    const created: ProjectDto = {
      id: "new-project",
      name: "New project",
      rootPath: "<workspace>/new-project",
      trust: "untrusted",
      createdAt: "2026-07-29T01:00:00.000Z",
      updatedAt: "2026-07-29T01:00:00.000Z",
    };
    const gateway = fixtureGateway();
    gateway.listProjects = async () => [];
    gateway.createProject = vi.fn().mockResolvedValue(created);
    gateway.selectProject = vi.fn().mockResolvedValue(created);
    render(<App gateway={gateway} />);

    await user.type(await screen.findByLabelText("项目名称"), created.name);
    await user.type(screen.getByLabelText("项目目录"), created.rootPath);
    await user.click(screen.getByRole("button", { name: "创建" }));

    await waitFor(() => {
      expect(gateway.createProject).toHaveBeenCalledWith({
        name: created.name,
        rootPath: created.rootPath,
      });
      expect(gateway.selectProject).toHaveBeenCalledWith(created.id);
    });
    expect(vi.mocked(gateway.createProject).mock.invocationCallOrder[0])
      .toBeLessThan(vi.mocked(gateway.selectProject).mock.invocationCallOrder[0]);
    expect(await screen.findByTestId("active-project-name")).toHaveTextContent(created.name);
  });

  it("keeps a long project name constrained at the 1024px minimum width", async () => {
    const user = userEvent.setup();
    const longName = "Keeper IDE project with a deliberately very long name ".repeat(8);
    Object.defineProperty(window, "innerWidth", { configurable: true, value: 1024 });
    window.dispatchEvent(new Event("resize"));
    render(<App gateway={fixtureGateway({ name: longName })} />);

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
