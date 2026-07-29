import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it, vi } from "vitest";
import { App } from "../src/App";
import type { ProjectGateway } from "../src/lib/projectGateway";

function fixtureGateway(): ProjectGateway {
  const project = {
    id: "keeper-ide",
    name: "Keeper IDE",
    rootPath: "<workspace>/VaultSystem\\keeper-ide",
    trust: "trusted" as const,
    createdAt: "2026-07-29T00:00:00.000Z",
    updatedAt: "2026-07-29T00:00:00.000Z",
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

  it("keeps project details open when creation fails", async () => {
    const user = userEvent.setup();
    const gateway = fixtureGateway();
    gateway.listProjects = async () => [];
    gateway.createProject = vi.fn().mockRejectedValue(new Error("store unavailable"));
    render(<App gateway={gateway} />);

    await user.type(await screen.findByLabelText("项目名称"), "Broken project");
    await user.type(screen.getByLabelText("项目目录"), "Z:\\missing");
    await user.click(screen.getByRole("button", { name: "创建" }));

    expect(await screen.findByRole("alert")).toHaveTextContent("无法创建项目");
    expect(screen.getByLabelText("项目名称")).toHaveValue("Broken project");
    expect(screen.getByLabelText("项目目录")).toHaveValue("Z:\\missing");
  });
});
