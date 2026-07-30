import { invoke } from "@tauri-apps/api/core";
import type { ProjectGateway } from "./projectGateway";

type NativeAcceptancePhase = "create" | "restore";

type NativeAcceptanceScenario = {
  phase: NativeAcceptancePhase;
  projectName: string;
  projectRoot: string;
};

type NativeAcceptanceResult = {
  phase: NativeAcceptancePhase;
  status: "passed" | "failed";
  projectId: string;
  restoredProjectCount: number;
  errorMessage?: string;
};

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === "object" && value !== null && !Array.isArray(value);
}

function parseScenario(value: unknown): NativeAcceptanceScenario | null {
  if (value === null) return null;
  if (
    !isRecord(value)
    || (value.phase !== "create" && value.phase !== "restore")
    || typeof value.projectName !== "string"
    || value.projectName.trim().length === 0
    || typeof value.projectRoot !== "string"
    || value.projectRoot.trim().length === 0
  ) {
    throw new Error("Pillow received an invalid native acceptance scenario.");
  }
  return {
    phase: value.phase,
    projectName: value.projectName,
    projectRoot: value.projectRoot,
  };
}

export class TauriDesktopLifecycle {
  async markProjectLauncherReady(): Promise<void> {
    await invoke("mark_project_launcher_ready");
  }

  async getNativeAcceptanceScenario(): Promise<NativeAcceptanceScenario | null> {
    return parseScenario(await invoke<unknown>("get_native_acceptance_scenario"));
  }

  async completeNativeAcceptance(result: NativeAcceptanceResult): Promise<void> {
    await invoke("complete_native_acceptance", { result });
  }
}

export async function runNativeAcceptanceIfRequested(
  gateway: ProjectGateway,
  lifecycle: TauriDesktopLifecycle,
): Promise<void> {
  const scenario = await lifecycle.getNativeAcceptanceScenario();
  if (!scenario) return;

  try {
    if (scenario.phase === "create") {
      const created = await gateway.createProject({
        name: scenario.projectName,
        rootPath: scenario.projectRoot,
      });
      const selected = await gateway.selectProject(created.id);
      await lifecycle.completeNativeAcceptance({
        phase: "create",
        status: "passed",
        projectId: selected.id,
        restoredProjectCount: 1,
      });
      return;
    }

    const projects = await gateway.listProjects();
    const matches = projects.filter(
      (project) =>
        project.name === scenario.projectName
        && project.rootPath === scenario.projectRoot,
    );
    if (matches.length !== 1) {
      throw new Error(`Expected one restored project, received ${matches.length}.`);
    }
    const selected = await gateway.selectProject(matches[0].id);
    await lifecycle.completeNativeAcceptance({
      phase: "restore",
      status: "passed",
      projectId: selected.id,
      restoredProjectCount: projects.length,
    });
  } catch (error) {
    await lifecycle.completeNativeAcceptance({
      phase: scenario.phase,
      status: "failed",
      projectId: "",
      restoredProjectCount: 0,
      errorMessage: error instanceof Error ? error.message : "Unknown acceptance failure.",
    });
    throw error;
  }
}
