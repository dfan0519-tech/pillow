export type ProjectDto = {
  id: string;
  name: string;
  rootPath: string;
  trust: "untrusted" | "trusted";
  createdAt: string;
  updatedAt: string;
};

export type CreateProjectInput = Pick<ProjectDto, "name" | "rootPath">;

const HOST_COMMAND_ERROR_CODES = [
  "invalid_project_name",
  "invalid_project_root",
  "project_root_exists",
  "project_store_unavailable",
  "project_not_found",
  "project_state_unavailable",
] as const;

export type HostCommandErrorCode = typeof HOST_COMMAND_ERROR_CODES[number];
export type ProjectGatewayErrorCode =
  | HostCommandErrorCode
  | "invalid_project_response"
  | "project_command_failed";

export type CommandError = {
  code: ProjectGatewayErrorCode;
  message: string;
};

export class ProjectGatewayError extends Error {
  readonly code: ProjectGatewayErrorCode;

  constructor(code: ProjectGatewayErrorCode, message: string) {
    super(message);
    this.name = "ProjectGatewayError";
    this.code = code;
  }
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === "object" && value !== null && !Array.isArray(value);
}

function isHostCommandErrorCode(value: unknown): value is HostCommandErrorCode {
  return typeof value === "string"
    && HOST_COMMAND_ERROR_CODES.some((code) => code === value);
}

export function normalizeProjectGatewayError(error: unknown): ProjectGatewayError {
  if (error instanceof ProjectGatewayError) return error;
  if (
    isRecord(error)
    && isHostCommandErrorCode(error.code)
    && typeof error.message === "string"
    && error.message.trim().length > 0
  ) {
    return new ProjectGatewayError(error.code, error.message);
  }
  return new ProjectGatewayError(
    "project_command_failed",
    "Project command failed.",
  );
}

export interface ProjectGateway {
  listProjects(): Promise<ProjectDto[]>;
  createProject(input: CreateProjectInput): Promise<ProjectDto>;
  selectProject(projectId: string): Promise<ProjectDto>;
  /** Native OS folder picker. Returns null when cancelled. */
  pickDirectory(): Promise<string | null>;
  /** Select existing registered project for path, or register it. */
  openOrRegisterProject(rootPath: string): Promise<ProjectDto>;
}
