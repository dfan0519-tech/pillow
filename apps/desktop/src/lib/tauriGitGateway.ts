/**
 * Tauri Git gateway — exact read-only invokes and strict DTO validation.
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

/** Align with keeper-git-executor discovery/status/diff caps. */
const MAX_PATH = 1_024;
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
 * Portable project-relative path or repository id.
 * Repository id `.` is valid; all other ids and paths are `/`-separated and
 * reject absolute, native, backslash, traversal, empty segments, and control
 * characters.
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
  return value.split("/").every(
    (part) =>
      part.length > 0
      && part !== "."
      && part !== ".."
      && part.length <= 255
      && !/[<>:"|?*\u0000-\u001f]/.test(part),
  );
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
 * porcelain v2 reports with exactly one trailing `/` (`vendor/`). The host
 * carries that form through parsing and project-path mapping, so this layer
 * must accept it too — otherwise one untracked folder makes the whole status
 * response look malformed and it degrades to `git_unavailable`.
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
}
