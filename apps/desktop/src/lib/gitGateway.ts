/** C4 Git DTOs. Every response is bound to a portable repository id + generation. */

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

export type GitMutationOperation = "stage" | "unstage" | "commit" | "push";

export const GIT_PUBLIC_ERRORS = [
  "git_project_not_found",
  "git_path_rejected",
  "git_repository_not_found",
  "git_unavailable",
  "git_generation_stale",
  "git_generation_mismatch",
  "git_message_rejected",
  "git_branch_rejected",
  "git_remote_rejected",
  "git_push_confirmation_required",
  "git_mutation_failed",
] as const;

export type GitPublicError = (typeof GIT_PUBLIC_ERRORS)[number];

export type GitRepositoryView = {
  repositoryId: string;
  available: boolean;
};

export type GitDiscoveryView = {
  generation: number;
  repositories: GitRepositoryView[];
};

export type GitStatusEntryView = {
  path: string;
  kind: GitStatusKind;
  originalPath?: string;
};

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
  /** Present only for `kind: "text"`. Binary/unsupported never invent unified text. */
  unified?: string;
  truncated: boolean;
  lineCount?: number;
  byteCount?: number;
  originalPath?: string;
};

export type GitGeneration = {
  repositoryId: string;
  generation: number;
};

/** Closed push confirmation: target and marker travel together. */
export type GitPushConfirmation = {
  confirmed: boolean;
  remoteName: string;
  branchName: string;
};

export type GitMutationView = {
  repositoryId: string;
  generation: number;
  operation: GitMutationOperation;
  remoteName?: string;
  branchName?: string;
};

/**
 * Renderer wire contract for the Git slice (read + structured mutations).
 * Invokes use only projectId / repositoryId / generation / portable paths /
 * validated message / closed push confirmation — never roots, executables,
 * argv, or native paths.
 */
export interface GitGateway {
  discoverGitRepositories(projectId: string): Promise<GitDiscoveryView>;
  getGitStatus(input: {
    projectId: string;
    expected: GitGeneration;
  }): Promise<GitStatusView>;
  openGitDiff(input: {
    projectId: string;
    expected: GitGeneration;
    path: string;
  }): Promise<GitDiffView>;
  stageGitPaths(input: {
    projectId: string;
    expected: GitGeneration;
    paths: string[];
  }): Promise<GitMutationView>;
  unstageGitPaths(input: {
    projectId: string;
    expected: GitGeneration;
    paths: string[];
  }): Promise<GitMutationView>;
  commitGitChanges(input: {
    projectId: string;
    expected: GitGeneration;
    message: string;
  }): Promise<GitMutationView>;
  pushGitBranch(input: {
    projectId: string;
    expected: GitGeneration;
    confirmation: GitPushConfirmation;
  }): Promise<GitMutationView>;
}

export class GitGatewayError extends Error {
  constructor(
    readonly code: GitPublicError,
    message: string,
  ) {
    super(message);
    this.name = "GitGatewayError";
  }
}

/** Fixed path-free copy for public codes. Host messages are never echoed. */
export const GIT_ERROR_MESSAGES: Record<GitPublicError, string> = {
  git_project_not_found: "Git project was not found.",
  git_path_rejected: "Git path was rejected.",
  git_repository_not_found: "Git repository was not found.",
  git_unavailable: "Git read capability is unavailable.",
  git_generation_stale: "Git result is stale; refresh changes.",
  git_generation_mismatch: "Git repository identity changed; refresh changes.",
  git_message_rejected:
    "Commit message was rejected. Use a non-empty message without leading dashes or control characters.",
  git_branch_rejected:
    "Branch name was rejected. Choose a valid branch and try again.",
  git_remote_rejected:
    "Remote was rejected. Check the remote name and that the target is an allowed HTTPS or local bare repository.",
  git_push_confirmation_required:
    "Push requires explicit confirmation of the remote and branch.",
  git_mutation_failed:
    "Git operation failed. For push: check credentials, remote access, and that the update is a fast-forward; then refresh and retry.",
};

export function isGitPublicError(value: unknown): value is GitPublicError {
  return (
    typeof value === "string"
    && (GIT_PUBLIC_ERRORS as readonly string[]).includes(value)
  );
}

/**
 * Normalize any rejection into a path-free public error.
 * Known codes (including generation stale/mismatch and mutation codes) are
 * preserved; everything else becomes `git_unavailable`.
 */
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
