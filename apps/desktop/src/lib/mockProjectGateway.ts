import type { CreateProjectInput, ProjectDto, ProjectGateway } from "./projectGateway";

/** Browser and renderer-test adapter; it is never used by the Tauri entrypoint. */
export class MockProjectGateway implements ProjectGateway {
  private projects: ProjectDto[];

  constructor(seed: ProjectDto[] = []) {
    this.projects = [...seed];
  }

  async listProjects(): Promise<ProjectDto[]> {
    return [...this.projects];
  }

  async createProject(input: CreateProjectInput): Promise<ProjectDto> {
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
    if (!project) throw new Error("Project not found");
    return project;
  }
}
