import "@testing-library/jest-dom/vitest";
import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it } from "vitest";
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
    render(<App gateway={fixtureGateway()} />);

    expect(screen.queryByText(/routeCode|lease|slotId/)).not.toBeInTheDocument();
  });
});
