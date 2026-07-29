import { invoke } from "@tauri-apps/api/core";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { TauriProjectGateway } from "../src/lib/tauriProjectGateway";

vi.mock("@tauri-apps/api/core", () => ({
  invoke: vi.fn(),
}));

const project = {
  id: "00000000-0000-0000-0000-000000000001",
  name: "Keeper IDE",
  rootPath: "<workspace>/VaultSystem\\keeper-ide",
  trust: "untrusted" as const,
  createdAt: "2026-07-29T00:00:00Z",
  updatedAt: "2026-07-29T00:00:00Z",
};

describe("TauriProjectGateway", () => {
  beforeEach(() => {
    vi.mocked(invoke).mockReset();
  });

  it("sends the flat Task 3 create-project command payload", async () => {
    vi.mocked(invoke).mockResolvedValue(project);
    const gateway = new TauriProjectGateway();

    await gateway.createProject({
      name: project.name,
      rootPath: project.rootPath,
    });

    expect(invoke).toHaveBeenCalledWith("create_project", {
      name: project.name,
      rootPath: project.rootPath,
    });
  });

  it("uses the typed Task 3 list and selection commands", async () => {
    vi.mocked(invoke)
      .mockResolvedValueOnce([project])
      .mockResolvedValueOnce(project);
    const gateway = new TauriProjectGateway();

    await expect(gateway.listProjects()).resolves.toEqual([project]);
    await expect(gateway.selectProject(project.id)).resolves.toEqual(project);

    expect(invoke).toHaveBeenNthCalledWith(1, "list_projects");
    expect(invoke).toHaveBeenNthCalledWith(2, "select_project", {
      projectId: project.id,
    });
  });
});
