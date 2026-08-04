/** C4 read-only Git DTOs. Every response is bound to a portable repository id + generation. */

export type GitStatusKind =
  | "modified"
  | "added"
  | "deleted"
  | "untracked"
  | "renamed"
  | "copied"
  | "typechange"
  | "conflict"
  | "submodule"
  | "ignored"
  | "unknown";
export type GitDiffKind = "text" | "binary" | "unsupported";

export const GIT_PUBLIC_ERRORS = [
  "git_project_not_found",
  "git_path_rejected",
  "git_repository_not_found",
  "git_unavailable",
  "git_generation_stale",
  "git_generation_mismatch",
] as const;
export type GitPublicError = (typeof GIT_PUBLIC_ERRORS)[number];

export type GitRepositoryView = { repositoryId: string; available: boolean };
export type GitDiscoveryView = { generation: number; repositories: GitRepositoryView[] };
export type GitStatusEntryView = { path: string; kind: GitStatusKind; originalPath?: string };
export type GitStatusView = {
  repositoryId: string;
  generation: number;
  branch: string | null;
  detached: boolean;
  headShort: string;
  dirtyCount: number;
  files: GitStatusEntryView[];
  truncated: boolean;
};
export type GitDiffView = {
  repositoryId: string;
  generation: number;
  path: string;
  kind: GitDiffKind;
  unified?: string;
  truncated: boolean;
  lineCount?: number;
  byteCount?: number;
  originalPath?: string;
};
export type GitGeneration = { repositoryId: string; generation: number };

export interface GitGateway {
  discoverGitRepositories(projectId: string): Promise<GitDiscoveryView>;
  getGitStatus(input: { projectId: string; expected: GitGeneration }): Promise<GitStatusView>;
  openGitDiff(input: {
    projectId: string;
    expected: GitGeneration;
    path: string;
  }): Promise<GitDiffView>;
}

export class GitGatewayError extends Error {
  constructor(readonly code: GitPublicError, message: string) {
    super(message);
    this.name = "GitGatewayError";
  }
}

const GIT_ERROR_MESSAGES: Record<GitPublicError, string> = {
  git_project_not_found: "Git project was not found.",
  git_path_rejected: "Git path was rejected.",
  git_repository_not_found: "Git repository was not found.",
  git_unavailable: "Git read capability is unavailable.",
  git_generation_stale: "Git result is stale; refresh changes.",
  git_generation_mismatch: "Git repository identity changed; refresh changes.",
};

export function isGitPublicError(value: unknown): value is GitPublicError {
  return typeof value === "string" && (GIT_PUBLIC_ERRORS as readonly string[]).includes(value);
}

export function normalizeGitError(error: unknown): GitGatewayError {
  const code =
    error instanceof GitGatewayError
      ? error.code
      : isRecord(error) && isGitPublicError(error.code)
        ? error.code
        : "git_unavailable";
  return new GitGatewayError(code, GIT_ERROR_MESSAGES[code]);
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === "object" && value !== null && !Array.isArray(value);
}
