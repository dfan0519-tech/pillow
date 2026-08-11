/**
 * Tauri Git gateway — exact read + mutation invokes and strict DTO validation.
 * Wire types live only in gitGateway.ts; this module never mounts UI state.
 */

import { invoke } from "@tauri-apps/api/core";
import {
  GIT_ERROR_MESSAGES,
  GitGatewayError,
  normalizeGitError,
  type GitDiffKind,
  type GitDiffView,
  type GitDiscoveryView,
  type GitGateway,
  type GitGeneration,
  type GitMutationOperation,
  type GitMutationView,
  type GitPushConfirmation,
  type GitStatusEntryView,
  type GitStatusKind,
  type GitStatusView,
} from "./gitGateway";

const STATUS_KINDS: readonly GitStatusKind[] = [
  "modified",
  "added",
  "deleted",
  "untracked",
  "renamed",
  "copied",
  "typechange",
  "conflict",
  "submodule",
  "ignored",
  "unknown",
];

const DIFF_KINDS: readonly GitDiffKind[] = ["text", "binary", "unsupported"];

/**
 * Align with keeper-git-executor portable_path SSoT
 * (`PORTABLE_PATH_MAX_LEN` / discovery status entry caps).
 */
const MAX_PATH = 4_096;
const MAX_COMPONENT = 255;
const MAX_REPOSITORIES = 32;
const MAX_FILES = 5_000;
const MAX_DIFF_BYTES = 256 * 1024;
const MAX_DIFF_LINES = 4_000;
const MAX_BRANCH = 256;
const MAX_HEAD_SHORT = 128;

const DISCOVERY_KEYS = ["generation", "repositories"] as const;
const REPOSITORY_KEYS = ["repositoryId", "available"] as const;
const STATUS_KEYS = [
  "repositoryId",
  "generation",
  "branch",
  "detached",
  "headShort",
  "dirtyCount",
  "files",
  "truncated",
] as const;
const STATUS_ENTRY_KEYS = ["path", "kind", "originalPath"] as const;
const DIFF_KEYS = [
  "repositoryId",
  "generation",
  "path",
  "kind",
  "unified",
  "truncated",
  "lineCount",
  "byteCount",
  "originalPath",
] as const;
const MUTATION_KEYS = [
  "repositoryId",
  "generation",
  "operation",
  "remoteName",
  "branchName",
] as const;

const MUTATION_OPERATIONS: readonly GitMutationOperation[] = [
  "stage",
  "unstage",
  "commit",
  "push",
];

const MAX_MESSAGE_BYTES = 4_096;
const MAX_REMOTE_NAME = 64;
const MAX_BRANCH_NAME = 255;
const MAX_PATHS = 5_000;

function invalid(): GitGatewayError {
  return new GitGatewayError(
    "git_unavailable",
    "Pillow received an invalid Git response.",
  );
}

function record(value: unknown): value is Record<string, unknown> {
  return typeof value === "object" && value !== null && !Array.isArray(value);
}

function only(value: Record<string, unknown>, keys: readonly string[]): boolean {
  return Object.keys(value).every((key) => keys.includes(key));
}

function integer(
  value: unknown,
  min = 0,
  max = Number.MAX_SAFE_INTEGER,
): value is number {
  return (
    typeof value === "number"
    && Number.isSafeInteger(value)
    && value >= min
    && value <= max
  );
}

function enumValue<T extends string>(
  value: unknown,
  values: readonly T[],
): value is T {
  return typeof value === "string" && (values as readonly string[]).includes(value);
}

/**
 * Windows reserved device names (stem before first `.`), matching the strict
 * SSoT in `keeper_project::portable_path` (re-exported by git-executor).
 *
 * Includes COM0–COM9 / LPT0–LPT9 and superscript COM¹–COM³ / LPT¹–LPT³
 * (U+00B9/B2/B3). COM0/LPT0 are fail-closed (see portable-path converge report).
 */
function isWindowsDeviceComponent(component: string): boolean {
  const stem = (component.split(".")[0] ?? component)
    .replace(/[ .]+$/g, "")
    .toUpperCase();
  if (
    stem === "CON"
    || stem === "PRN"
    || stem === "AUX"
    || stem === "NUL"
    || stem === "CLOCK$"
    || stem === "CONIN$"
    || stem === "CONOUT$"
  ) {
    return true;
  }
  // ASCII COM0–COM9 / LPT0–LPT9
  if (/^(COM|LPT)[0-9]$/.test(stem)) {
    return true;
  }
  // Windows treats ISO-8859-1 superscripts ¹²³ as device digits.
  return (
    stem === "COM\u00B9"
    || stem === "COM\u00B2"
    || stem === "COM\u00B3"
    || stem === "LPT\u00B9"
    || stem === "LPT\u00B2"
    || stem === "LPT\u00B3"
  );
}

/**
 * Portable project-relative path or repository id.
 *
 * Mirrors `keeper_git_executor::is_portable_path` (file form). Repository id
 * `.` is valid; directory form (`vendor/`) is not. Rejects absolute paths,
 * native separators, traversal, empty segments, trailing dot/space, control
 * characters, and Windows device names.
 */
function portable(value: unknown): value is string {
  if (value === ".") return true;
  if (typeof value !== "string" || value.length === 0 || value.length > MAX_PATH) {
    return false;
  }
  if (
    value.includes("\\")
    || value.includes(":")
    || value.startsWith("/")
    || value.startsWith("\\")
  ) {
    return false;
  }
  return value.split("/").every((part) => {
    if (
      part.length === 0
      || part === "."
      || part === ".."
      || part.length > MAX_COMPONENT
    ) {
      return false;
    }
    if (part.endsWith(".") || part.endsWith(" ")) {
      return false;
    }
    if (/[<>:"|?*\u0000-\u001f]/.test(part)) {
      return false;
    }
    if (isWindowsDeviceComponent(part)) {
      return false;
    }
    return true;
  });
}

/**
 * Contract-test and documentation surface for the file-form portable rule.
 * Diff targets and repository ids use this form only.
 */
export function isPortablePath(value: string): boolean {
  return portable(value);
}

/**
 * Contract-test surface for the status-entry portable rule (allows `vendor/`).
 */
export function isPortableEntryPath(value: string): boolean {
  return portableEntryPath(value);
}

function utf8Bytes(value: string): number {
  return new TextEncoder().encode(value).length;
}

/** Logical line count: empty → 0; trailing newline does not invent an extra line. */
function lineCount(value: string): number {
  if (value.length === 0) return 0;
  return value.endsWith("\n")
    ? value.split("\n").length - 1
    : value.split("\n").length;
}

/**
 * Echoed repository/generation must match the request.
 * Mismatch and stale keep their specific public codes (not git_unavailable).
 */
function assertExpected(
  current: GitGeneration,
  response: { repositoryId: string; generation: number },
): void {
  if (response.repositoryId !== current.repositoryId) {
    throw new GitGatewayError(
      "git_generation_mismatch",
      GIT_ERROR_MESSAGES.git_generation_mismatch,
    );
  }
  if (response.generation !== current.generation) {
    throw new GitGatewayError(
      "git_generation_stale",
      GIT_ERROR_MESSAGES.git_generation_stale,
    );
  }
}

/**
 * Status entry paths may name a collapsed untracked/ignored directory, which
 * porcelain v2 reports with exactly one trailing `/` (`vendor/`). Mirrors
 * `keeper_git_executor::is_portable_entry_path`. The host carries that form
 * through parsing and project-path mapping, so this layer must accept it too —
 * otherwise one untracked folder makes the whole status response look
 * malformed and it degrades to `git_unavailable`.
 *
 * Deliberately NOT used for repository ids or diff paths: a diff target must
 * still be a file. The stem runs through the identical `portable` rules, so
 * absolute, native, backslash, traversal and empty-segment rejection is
 * unchanged.
 */
function portableEntryPath(value: unknown): value is string {
  if (typeof value === "string" && value.endsWith("/")) {
    const stem = value.slice(0, -1);
    return stem.length > 0 && stem !== "." && !stem.endsWith("/") && portable(stem);
  }
  return portable(value);
}

function normalizeStatusEntry(item: unknown): GitStatusEntryView {
  if (
    !record(item)
    || !only(item, STATUS_ENTRY_KEYS)
    || !portableEntryPath(item.path)
    || !enumValue(item.kind, STATUS_KINDS)
  ) {
    throw invalid();
  }
  if (item.originalPath !== undefined && !portableEntryPath(item.originalPath)) {
    throw invalid();
  }
  return {
    path: item.path,
    kind: item.kind,
    ...(item.originalPath === undefined ? {} : { originalPath: item.originalPath }),
  };
}

export function normalizeGitDiscovery(value: unknown): GitDiscoveryView {
  if (
    !record(value)
    || !only(value, DISCOVERY_KEYS)
    || !integer(value.generation)
    || !Array.isArray(value.repositories)
    || value.repositories.length > MAX_REPOSITORIES
  ) {
    throw invalid();
  }

  const repositories = value.repositories.map((item) => {
    if (
      !record(item)
      || !only(item, REPOSITORY_KEYS)
      || !portable(item.repositoryId)
      || typeof item.available !== "boolean"
    ) {
      throw invalid();
    }
    return {
      repositoryId: item.repositoryId,
      available: item.available,
    };
  });

  return {
    generation: value.generation,
    repositories,
  };
}

export function normalizeGitStatus(
  value: unknown,
  current: GitGeneration,
): GitStatusView {
  if (
    !record(value)
    || !only(value, STATUS_KEYS)
    || !portable(value.repositoryId)
    || !integer(value.generation)
    || !(
      value.branch === null
      || (typeof value.branch === "string" && value.branch.length <= MAX_BRANCH)
    )
    || typeof value.detached !== "boolean"
    || typeof value.headShort !== "string"
    || value.headShort.length > MAX_HEAD_SHORT
    || !integer(value.dirtyCount, 0, MAX_FILES)
    || !Array.isArray(value.files)
    || value.files.length > MAX_FILES
    || typeof value.truncated !== "boolean"
  ) {
    throw invalid();
  }

  assertExpected(current, {
    repositoryId: value.repositoryId,
    generation: value.generation,
  });

  const files = value.files.map(normalizeStatusEntry);

  return {
    repositoryId: value.repositoryId,
    generation: value.generation,
    branch: value.branch,
    detached: value.detached,
    headShort: value.headShort,
    dirtyCount: value.dirtyCount,
    files,
    truncated: value.truncated,
  };
}

export function normalizeGitDiff(
  value: unknown,
  current: GitGeneration,
): GitDiffView {
  if (
    !record(value)
    || !only(value, DIFF_KEYS)
    || !portable(value.repositoryId)
    || !integer(value.generation)
    || !portable(value.path)
    || !enumValue(value.kind, DIFF_KINDS)
    || typeof value.truncated !== "boolean"
  ) {
    throw invalid();
  }

  if (value.originalPath !== undefined && !portable(value.originalPath)) {
    throw invalid();
  }

  assertExpected(current, {
    repositoryId: value.repositoryId,
    generation: value.generation,
  });

  const isText = value.kind === "text";
  const hasUnified = Object.prototype.hasOwnProperty.call(value, "unified");
  const hasLineCount = Object.prototype.hasOwnProperty.call(value, "lineCount");
  const hasByteCount = Object.prototype.hasOwnProperty.call(value, "byteCount");

  if (isText) {
    if (
      !hasUnified
      || !hasLineCount
      || !hasByteCount
      || typeof value.unified !== "string"
      || !integer(value.lineCount, 0, MAX_DIFF_LINES)
      || !integer(value.byteCount, 0, MAX_DIFF_BYTES)
    ) {
      throw invalid();
    }
    const unified = value.unified;
    const encoded = utf8Bytes(unified);
    if (
      encoded > MAX_DIFF_BYTES
      || value.byteCount !== encoded
      || value.lineCount !== lineCount(unified)
    ) {
      throw invalid();
    }
    return {
      repositoryId: value.repositoryId,
      generation: value.generation,
      path: value.path,
      kind: "text",
      unified,
      truncated: value.truncated,
      lineCount: value.lineCount,
      byteCount: value.byteCount,
      ...(value.originalPath === undefined ? {} : { originalPath: value.originalPath }),
    };
  }

  // binary / unsupported: never accept invented unified text or counts
  if (hasUnified || hasLineCount || hasByteCount) {
    throw invalid();
  }

  return {
    repositoryId: value.repositoryId,
    generation: value.generation,
    path: value.path,
    kind: value.kind,
    truncated: value.truncated,
    ...(value.originalPath === undefined ? {} : { originalPath: value.originalPath }),
  };
}

/**
 * Normalize a closed mutation receipt. Echoed repository/generation must match
 * the request; optional remote/branch are admitted only when present and bounded.
 */
export function normalizeGitMutation(
  value: unknown,
  current: GitGeneration,
  expectedOperation?: GitMutationOperation,
): GitMutationView {
  if (
    !record(value)
    || !only(value, MUTATION_KEYS)
    || !portable(value.repositoryId)
    || !integer(value.generation)
    || !enumValue(value.operation, MUTATION_OPERATIONS)
  ) {
    throw invalid();
  }

  if (
    value.remoteName !== undefined
    && (typeof value.remoteName !== "string"
      || value.remoteName.length === 0
      || value.remoteName.length > MAX_REMOTE_NAME)
  ) {
    throw invalid();
  }
  if (
    value.branchName !== undefined
    && (typeof value.branchName !== "string"
      || value.branchName.length === 0
      || value.branchName.length > MAX_BRANCH_NAME)
  ) {
    throw invalid();
  }

  assertExpected(current, {
    repositoryId: value.repositoryId,
    generation: value.generation,
  });

  if (expectedOperation !== undefined && value.operation !== expectedOperation) {
    throw invalid();
  }

  return {
    repositoryId: value.repositoryId,
    generation: value.generation,
    operation: value.operation,
    ...(value.remoteName === undefined ? {} : { remoteName: value.remoteName }),
    ...(value.branchName === undefined ? {} : { branchName: value.branchName }),
  };
}

function assertPortablePaths(paths: string[]): void {
  if (
    !Array.isArray(paths)
    || paths.length === 0
    || paths.length > MAX_PATHS
    || paths.some((path) => !portableEntryPath(path))
  ) {
    throw new GitGatewayError(
      "git_path_rejected",
      GIT_ERROR_MESSAGES.git_path_rejected,
    );
  }
}

function assertCommitMessage(message: string): void {
  if (
    typeof message !== "string"
    || message.length === 0
    || utf8Bytes(message) > MAX_MESSAGE_BYTES
    || message.startsWith("-")
    || /[\u0000-\u0008\u000b\u000c\u000e-\u001f]/.test(message)
  ) {
    throw new GitGatewayError(
      "git_message_rejected",
      GIT_ERROR_MESSAGES.git_message_rejected,
    );
  }
}

function assertPushConfirmation(
  confirmation: GitPushConfirmation,
): GitPushConfirmation {
  if (typeof confirmation !== "object" || confirmation === null) {
    throw new GitGatewayError(
      "git_push_confirmation_required",
      GIT_ERROR_MESSAGES.git_push_confirmation_required,
    );
  }
  if (typeof confirmation.confirmed !== "boolean") {
    throw new GitGatewayError(
      "git_push_confirmation_required",
      GIT_ERROR_MESSAGES.git_push_confirmation_required,
    );
  }
  if (
    typeof confirmation.remoteName !== "string"
    || confirmation.remoteName.length === 0
    || confirmation.remoteName.length > MAX_REMOTE_NAME
    || confirmation.remoteName.startsWith("-")
    || /[\u0000-\u001f]/.test(confirmation.remoteName)
  ) {
    throw new GitGatewayError(
      "git_remote_rejected",
      GIT_ERROR_MESSAGES.git_remote_rejected,
    );
  }
  if (
    typeof confirmation.branchName !== "string"
    || confirmation.branchName.length === 0
    || confirmation.branchName.length > MAX_BRANCH_NAME
    || confirmation.branchName.startsWith("-")
    || /[\u0000-\u001f]/.test(confirmation.branchName)
  ) {
    throw new GitGatewayError(
      "git_branch_rejected",
      GIT_ERROR_MESSAGES.git_branch_rejected,
    );
  }
  return {
    confirmed: confirmation.confirmed,
    remoteName: confirmation.remoteName,
    branchName: confirmation.branchName,
  };
}

export class TauriGitGateway implements GitGateway {
  async discoverGitRepositories(projectId: string): Promise<GitDiscoveryView> {
    try {
      const payload = await invoke<unknown>("discover_git_repositories", {
        projectId,
      });
      return normalizeGitDiscovery(payload);
    } catch (error) {
      throw normalizeGitError(error);
    }
  }

  async getGitStatus(input: {
    projectId: string;
    expected: GitGeneration;
  }): Promise<GitStatusView> {
    try {
      const payload = await invoke<unknown>("get_git_status", {
        projectId: input.projectId,
        repositoryId: input.expected.repositoryId,
        generation: input.expected.generation,
      });
      return normalizeGitStatus(payload, input.expected);
    } catch (error) {
      throw normalizeGitError(error);
    }
  }

  async openGitDiff(input: {
    projectId: string;
    expected: GitGeneration;
    path: string;
  }): Promise<GitDiffView> {
    try {
      const payload = await invoke<unknown>("open_git_diff", {
        projectId: input.projectId,
        repositoryId: input.expected.repositoryId,
        generation: input.expected.generation,
        path: input.path,
      });
      return normalizeGitDiff(payload, input.expected);
    } catch (error) {
      throw normalizeGitError(error);
    }
  }

  async stageGitPaths(input: {
    projectId: string;
    expected: GitGeneration;
    paths: string[];
  }): Promise<GitMutationView> {
    assertPortablePaths(input.paths);
    try {
      const payload = await invoke<unknown>("stage_git_paths", {
        projectId: input.projectId,
        repositoryId: input.expected.repositoryId,
        generation: input.expected.generation,
        paths: input.paths,
      });
      return normalizeGitMutation(payload, input.expected, "stage");
    } catch (error) {
      throw normalizeGitError(error);
    }
  }

  async unstageGitPaths(input: {
    projectId: string;
    expected: GitGeneration;
    paths: string[];
  }): Promise<GitMutationView> {
    assertPortablePaths(input.paths);
    try {
      const payload = await invoke<unknown>("unstage_git_paths", {
        projectId: input.projectId,
        repositoryId: input.expected.repositoryId,
        generation: input.expected.generation,
        paths: input.paths,
      });
      return normalizeGitMutation(payload, input.expected, "unstage");
    } catch (error) {
      throw normalizeGitError(error);
    }
  }

  async commitGitChanges(input: {
    projectId: string;
    expected: GitGeneration;
    message: string;
  }): Promise<GitMutationView> {
    assertCommitMessage(input.message);
    try {
      const payload = await invoke<unknown>("commit_git_changes", {
        projectId: input.projectId,
        repositoryId: input.expected.repositoryId,
        generation: input.expected.generation,
        message: input.message,
      });
      return normalizeGitMutation(payload, input.expected, "commit");
    } catch (error) {
      throw normalizeGitError(error);
    }
  }

  async pushGitBranch(input: {
    projectId: string;
    expected: GitGeneration;
    confirmation: GitPushConfirmation;
  }): Promise<GitMutationView> {
    const confirmation = assertPushConfirmation(input.confirmation);
    if (!confirmation.confirmed) {
      throw new GitGatewayError(
        "git_push_confirmation_required",
        GIT_ERROR_MESSAGES.git_push_confirmation_required,
      );
    }
    try {
      const payload = await invoke<unknown>("push_git_branch", {
        projectId: input.projectId,
        repositoryId: input.expected.repositoryId,
        generation: input.expected.generation,
        confirmation: {
          confirmed: true,
          remoteName: confirmation.remoteName,
          branchName: confirmation.branchName,
        },
      });
      return normalizeGitMutation(payload, input.expected, "push");
    } catch (error) {
      throw normalizeGitError(error);
    }
  }
}
