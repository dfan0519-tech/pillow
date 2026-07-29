export type ProjectDto = {
  id: string;
  name: string;
  rootPath: string;
  trust: "untrusted" | "trusted";
  createdAt: string;
  updatedAt: string;
};

export type CreateProjectInput = Pick<ProjectDto, "name" | "rootPath">;

export interface ProjectGateway {
  listProjects(): Promise<ProjectDto[]>;
  createProject(input: CreateProjectInput): Promise<ProjectDto>;
  selectProject(projectId: string): Promise<ProjectDto>;
}
