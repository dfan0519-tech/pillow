import { invoke } from "@tauri-apps/api/core";
import { beforeEach, describe, expect, it, vi } from "vitest";
import type { ProjectDto, ProjectGateway } from "../src/lib/projectGateway";
import {
  TauriDesktopLifecycle,
  runNativeAcceptanceIfRequested,
} from "../src/lib/desktopLifecycle";

vi.mock("@tauri-apps/api/core", () => ({
  invoke: vi.fn(),
}));

const project: ProjectDto = {
  id: "00000000-0000-0000-0000-000000000001",
  name: "Keeper IDE Native Acceptance",
  rootPath: "C:\\Temp\\keeper-ide-native\\project",
  trust: "untrusted",
  createdAt: "2026-07-29T00:00:00Z",
  updatedAt: "2026-07-29T00:00:00Z",
};

function gateway(): ProjectGateway {
  return {
    listProjects: vi.fn().mockResolvedValue([project]),
    createProject: vi.fn().mockResolvedValue(project),
    selectProject: vi.fn().mockResolvedValue(project),
    pickDirectory: vi.fn().mockResolvedValue(null),
    openOrRegisterProject: vi.fn().mockResolvedValue(project),
    createProjectWithFolder: vi.fn().mockResolvedValue(project),
  };
}

describe("desktop lifecycle bridge", () => {
  beforeEach(() => {
    vi.mocked(invoke).mockReset();
  });

  it("marks the launcher ready through a real Tauri command", async () => {
    vi.mocked(invoke).mockResolvedValue(undefined);

    await new TauriDesktopLifecycle().markProjectLauncherReady();

    expect(invoke).toHaveBeenCalledWith("mark_project_launcher_ready");
  });

  it("creates and selects the acceptance project before completing phase one", async () => {
    vi.mocked(invoke)
      .mockResolvedValueOnce({
        phase: "create",
        projectName: project.name,
        projectRoot: project.rootPath,
      })
      .mockResolvedValueOnce(undefined);
    const projectGateway = gateway();

    await runNativeAcceptanceIfRequested(
      projectGateway,
      new TauriDesktopLifecycle(),
    );

    expect(projectGateway.createProject).toHaveBeenCalledWith({
      name: project.name,
      rootPath: project.rootPath,
    });
    expect(projectGateway.selectProject).toHaveBeenCalledWith(project.id);
    expect(invoke).toHaveBeenNthCalledWith(2, "complete_native_acceptance", {
      result: {
        phase: "create",
        projectId: project.id,
        restoredProjectCount: 1,
        status: "passed",
      },
    });
  });

  it("lists and reselects the persisted project after process restart", async () => {
    vi.mocked(invoke)
      .mockResolvedValueOnce({
        phase: "restore",
        projectName: project.name,
        projectRoot: project.rootPath,
      })
      .mockResolvedValueOnce(undefined);
    const projectGateway = gateway();

    await runNativeAcceptanceIfRequested(
      projectGateway,
      new TauriDesktopLifecycle(),
    );

    expect(projectGateway.listProjects).toHaveBeenCalledOnce();
    expect(projectGateway.selectProject).toHaveBeenCalledWith(project.id);
    expect(invoke).toHaveBeenNthCalledWith(2, "complete_native_acceptance", {
      result: {
        phase: "restore",
        projectId: project.id,
        restoredProjectCount: 1,
        status: "passed",
      },
    });
  });

  it("rejects malformed native acceptance scenarios", async () => {
    vi.mocked(invoke).mockResolvedValue({ phase: "restore", projectName: "" });

    await expect(
      new TauriDesktopLifecycle().getNativeAcceptanceScenario(),
    ).rejects.toThrow("invalid native acceptance scenario");
  });
});
