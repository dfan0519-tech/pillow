/**
 * Typed project-scoped resource gateway — renderer boundary for Wave C2-B.
 * Portable relative paths and bounded content only; no project roots or handles.
 */

export type ResourceEntryKind = "file" | "directory" | "symlink" | "other";

export type ResourceContentKind = "utf8" | "binary";

export type ResourceEntry = {
  path: string;
  kind: ResourceEntryKind;
  sizeBytes?: number;
};

export type ResourceFile = {
  path: string;
  contentKind: ResourceContentKind;
  /** Present only for valid UTF-8 reads within the bridge bound. */
  content?: string;
  bytesReturned: number;
  truncated: boolean;
};

export type ResourceSearchResult = {
  path: string;
  line: number;
  column: number;
  preview: string;
};

export type RecentResource = {
  path: string;
  openedAt: string;
};

export type ResourcePreviewKind =
  | "text"
  | "markdown"
  | "image"
  | "binary";

export type ResourcePreview = {
  path: string;
  kind: ResourcePreviewKind;
  mediaType: string;
  content?: string;
  dataUrl?: string;
  bytesReturned: number;
  truncated: boolean;
};

export type ListProjectResourcesInput = {
  projectId: string;
  relativePath?: string;
};

export type ReadProjectResourceInput = {
  projectId: string;
  relativePath: string;
};

export type SearchProjectResourcesInput = {
  projectId: string;
  query: string;
  relativePath?: string;
};

export type ListRecentProjectResourcesInput = {
  projectId: string;
};

export type PreviewProjectResourceInput = {
  projectId: string;
  relativePath: string;
};

const HOST_COMMAND_ERROR_CODES = [
  "project_not_found",
  "resource_path_rejected",
  "resource_search_query_rejected",
  "resource_not_found",
  "resource_not_a_directory",
  "resource_not_a_file",
  "resource_limit_exceeded",
  "resource_unavailable",
] as const;

export type HostResourceErrorCode = typeof HOST_COMMAND_ERROR_CODES[number];
export type ResourceGatewayErrorCode =
  | HostResourceErrorCode
  | "invalid_resource_response"
  | "resource_command_failed";

export class ResourceGatewayError extends Error {
  readonly code: ResourceGatewayErrorCode;

  constructor(code: ResourceGatewayErrorCode, message: string) {
    super(message);
    this.name = "ResourceGatewayError";
    this.code = code;
  }
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === "object" && value !== null && !Array.isArray(value);
}

function isHostResourceErrorCode(value: unknown): value is HostResourceErrorCode {
  return typeof value === "string"
    && HOST_COMMAND_ERROR_CODES.some((code) => code === value);
}

/** Opaque command errors only — never surface machine paths from host payloads. */
export function normalizeResourceGatewayError(error: unknown): ResourceGatewayError {
  if (error instanceof ResourceGatewayError) return error;
  if (
    isRecord(error)
    && isHostResourceErrorCode(error.code)
    && typeof error.message === "string"
    && error.message.trim().length > 0
    && !looksLikeMachinePath(error.message)
  ) {
    return new ResourceGatewayError(error.code, error.message);
  }
  return new ResourceGatewayError(
    "resource_command_failed",
    "Resource command failed.",
  );
}

function looksLikeMachinePath(message: string): boolean {
  return /[\\/]/.test(message)
    || /[A-Za-z]:\\/.test(message)
    || message.includes("://");
}

export interface ResourceGateway {
  listProjectResources(input: ListProjectResourcesInput): Promise<ResourceEntry[]>;
  readProjectResource(input: ReadProjectResourceInput): Promise<ResourceFile>;
  searchProjectResources(
    input: SearchProjectResourcesInput,
  ): Promise<ResourceSearchResult[]>;
  listRecentProjectResources(
    input: ListRecentProjectResourcesInput,
  ): Promise<RecentResource[]>;
  previewProjectResource(
    input: PreviewProjectResourceInput,
  ): Promise<ResourcePreview>;
}
