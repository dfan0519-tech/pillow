import { invoke } from "@tauri-apps/api/core";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { ProjectGatewayError } from "../src/lib/projectGateway";
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

  it.each([
    "id",
    "name",
    "rootPath",
    "trust",
    "createdAt",
    "updatedAt",
  ] as const)("rejects a project DTO missing %s", async (field) => {
    const malformedProject: Partial<typeof project> = { ...project };
    delete malformedProject[field];
    vi.mocked(invoke).mockResolvedValue(malformedProject);

    const result = new TauriProjectGateway().createProject({
      name: project.name,
      rootPath: project.rootPath,
    });

    await expect(result).rejects.toMatchObject({
      name: "ProjectGatewayError",
      code: "invalid_project_response",
      message: "Keeper IDE received an invalid project response.",
    });
  });

  it("rejects malformed list and enum payloads", async () => {
    vi.mocked(invoke)
      .mockResolvedValueOnce(project)
      .mockResolvedValueOnce([{ ...project, trust: "owner" }]);
    const gateway = new TauriProjectGateway();

    await expect(gateway.listProjects()).rejects.toBeInstanceOf(ProjectGatewayError);
    await expect(gateway.listProjects()).rejects.toMatchObject({
      code: "invalid_project_response",
    });
  });

  it("preserves allow-listed command errors for the UI", async () => {
    vi.mocked(invoke).mockRejectedValue({
      code: "project_not_found",
      message: "Project not found.",
    });

    await expect(new TauriProjectGateway().selectProject(project.id)).rejects.toMatchObject({
      name: "ProjectGatewayError",
      code: "project_not_found",
      message: "Project not found.",
    });
  });

  it.each([
    new Error("database path leaked"),
    { code: "sqlite_error", message: "secret database path" },
    { code: "project_not_found" },
    "unexpected rejection",
  ])("normalizes an unknown command error payload", async (payload) => {
    vi.mocked(invoke).mockRejectedValue(payload);

    await expect(new TauriProjectGateway().listProjects()).rejects.toMatchObject({
      name: "ProjectGatewayError",
      code: "project_command_failed",
      message: "Project command failed.",
    });
  });
});
