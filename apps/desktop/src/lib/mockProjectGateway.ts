import type {
  CreateProjectInput,
  ProjectDto,
  ProjectGateway,
} from "./projectGateway";
import { ProjectGatewayError } from "./projectGateway";

/** Browser and renderer-test adapter; it is never used by the Tauri entrypoint. */
export class MockProjectGateway implements ProjectGateway {
  private projects: ProjectDto[];
  private nextPick: string | null | undefined = undefined;

  constructor(seed: ProjectDto[] = []) {
    this.projects = [...seed];
  }

  /** Test helper: queue the next folder-picker result. */
  setNextPick(path: string | null) {
    this.nextPick = path;
  }

  async listProjects(): Promise<ProjectDto[]> {
    return [...this.projects];
  }

  async createProject(input: CreateProjectInput): Promise<ProjectDto> {
    const existing = this.projects.find(
      (project) => project.rootPath === input.rootPath,
    );
    if (existing) {
      throw new ProjectGatewayError(
        "project_root_exists",
        "A project already exists for this folder.",
      );
    }
    const timestamp = new Date().toISOString();
    const project: ProjectDto = {
      id: crypto.randomUUID(),
      name: input.name,
      rootPath: input.rootPath,
      trust: "untrusted",
      createdAt: timestamp,
      updatedAt: timestamp,
    };
    this.projects = [...this.projects, project];
    return project;
  }

  async selectProject(projectId: string): Promise<ProjectDto> {
    const project = this.projects.find((candidate) => candidate.id === projectId);
    if (!project) {
      throw new ProjectGatewayError("project_not_found", "Project not found.");
    }
    return project;
  }

  async pickDirectory(): Promise<string | null> {
    if (this.nextPick !== undefined) {
      const value = this.nextPick;
      this.nextPick = undefined;
      return value;
    }
    return null;
  }

  async openOrRegisterProject(rootPath: string): Promise<ProjectDto> {
    const existing = this.projects.find((project) => project.rootPath === rootPath);
    if (existing) return existing;
    const leaf = rootPath.replace(/[\\/]+$/u, "").split(/[\\/]/u).pop() ?? rootPath;
    return this.createProject({ name: leaf, rootPath });
  }
}
