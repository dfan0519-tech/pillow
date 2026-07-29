import { invoke } from "@tauri-apps/api/core";
import {
  ProjectGatewayError,
  normalizeProjectGatewayError,
  type CreateProjectInput,
  type ProjectDto,
  type ProjectGateway,
} from "./projectGateway";

const INVALID_RESPONSE_MESSAGE = "Keeper IDE received an invalid project response.";

function invalidProjectResponse(): ProjectGatewayError {
  return new ProjectGatewayError("invalid_project_response", INVALID_RESPONSE_MESSAGE);
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === "object" && value !== null && !Array.isArray(value);
}

function isNonEmptyString(value: unknown): value is string {
  return typeof value === "string" && value.trim().length > 0;
}

function parseProjectDto(value: unknown): ProjectDto {
  if (
    !isRecord(value)
    || !isNonEmptyString(value.id)
    || !isNonEmptyString(value.name)
    || !isNonEmptyString(value.rootPath)
    || (value.trust !== "untrusted" && value.trust !== "trusted")
    || !isNonEmptyString(value.createdAt)
    || !isNonEmptyString(value.updatedAt)
  ) {
    throw invalidProjectResponse();
  }

  return {
    id: value.id,
    name: value.name,
    rootPath: value.rootPath,
    trust: value.trust,
    createdAt: value.createdAt,
    updatedAt: value.updatedAt,
  };
}

async function invokeProject(
  command: string,
  args?: Record<string, unknown>,
): Promise<ProjectDto> {
  try {
    const payload = args
      ? await invoke<unknown>(command, args)
      : await invoke<unknown>(command);
    return parseProjectDto(payload);
  } catch (error) {
    throw normalizeProjectGatewayError(error);
  }
}

/** Tauri-only boundary. The renderer does not access SQLite directly. */
export class TauriProjectGateway implements ProjectGateway {
  async listProjects(): Promise<ProjectDto[]> {
    try {
      const payload = await invoke<unknown>("list_projects");
      if (!Array.isArray(payload)) throw invalidProjectResponse();
      return payload.map(parseProjectDto);
    } catch (error) {
      throw normalizeProjectGatewayError(error);
    }
  }

  createProject(input: CreateProjectInput): Promise<ProjectDto> {
    return invokeProject("create_project", {
      name: input.name,
      rootPath: input.rootPath,
    });
  }

  selectProject(projectId: string): Promise<ProjectDto> {
    return invokeProject("select_project", { projectId });
  }
}
