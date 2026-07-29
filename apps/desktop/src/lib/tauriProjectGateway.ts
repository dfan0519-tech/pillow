import { invoke } from "@tauri-apps/api/core";
import type { CreateProjectInput, ProjectDto, ProjectGateway } from "./projectGateway";

type ProjectWireDto = ProjectDto & {
  root_path?: string;
  created_at?: string;
  updated_at?: string;
};

function toProjectDto(project: ProjectWireDto): ProjectDto {
  return {
    id: project.id,
    name: project.name,
    rootPath: project.rootPath ?? project.root_path ?? "",
    trust: project.trust,
    createdAt: project.createdAt ?? project.created_at ?? "",
    updatedAt: project.updatedAt ?? project.updated_at ?? "",
  };
}

/** Tauri-only boundary. The renderer does not access SQLite directly. */
export class TauriProjectGateway implements ProjectGateway {
  async listProjects(): Promise<ProjectDto[]> {
    const projects = await invoke<ProjectWireDto[]>("list_projects");
    return projects.map(toProjectDto);
  }

  async createProject(input: CreateProjectInput): Promise<ProjectDto> {
    const project = await invoke<ProjectWireDto>("create_project", { input });
    return toProjectDto(project);
  }

  async selectProject(projectId: string): Promise<ProjectDto> {
    const project = await invoke<ProjectWireDto>("select_project", { projectId });
    return toProjectDto(project);
  }
}
