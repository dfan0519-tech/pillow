/**
 * Pure helpers for the collapsible resource context panel (C2-C1).
 * No filesystem access; portable relative paths only.
 */

import {
  normalizeResourceGatewayError,
  type RecentResource,
  type ResourceEntry,
  type ResourceSearchResult,
} from "../../lib/resourceGateway";
import type {
  ResourcePaneSnapshot,
  ResourceRowModel,
} from "./resourceTypes";

/** Debounce window for explicit search input (ms). */
export const RESOURCE_SEARCH_DEBOUNCE_MS = 250;

const FALLBACK_ERROR_MESSAGE = "无法加载资源。";

export function createClosedResourcePaneSnapshot(
  projectId: string,
): ResourcePaneSnapshot {
  return {
    open: false,
    projectId,
    browseStatus: "idle",
    searchStatus: "idle",
    searchQuery: "",
    recents: [],
    rootEntries: [],
    searchResults: [],
    errorMessage: null,
  };
}

/**
 * Reset all in-flight results when the active project changes.
 * Keeps disclosure open/closed; never carries over foreign project rows.
 */
export function resetSnapshotForProject(
  previous: ResourcePaneSnapshot,
  projectId: string,
): ResourcePaneSnapshot {
  return {
    ...createClosedResourcePaneSnapshot(projectId),
    open: previous.open,
  };
}

/** True when the path is a portable project-relative POSIX-style path. */
export function isPortableRelativePath(path: string): boolean {
  if (typeof path !== "string") return false;
  const trimmed = path.trim();
  if (trimmed.length === 0) return false;
  if (trimmed.includes("\\")) return false;
  if (trimmed.startsWith("/")) return false;
  if (/^[A-Za-z]:/.test(trimmed)) return false;
  if (trimmed.includes("://")) return false;
  if (trimmed.split("/").some((part) => part === ".." || part === "")) {
    return false;
  }
  return true;
}

export function resourceLeafName(path: string): string {
  const normalized = path.replace(/\\/g, "/");
  const parts = normalized.split("/").filter(Boolean);
  return parts[parts.length - 1] ?? path;
}

export function toResourceRow(entry: Pick<ResourceEntry, "path" | "kind">): ResourceRowModel {
  return {
    path: entry.path,
    kind: entry.kind,
    label: resourceLeafName(entry.path),
  };
}

/**
 * Deterministic ordering: directories first, then files/others, then path.
 * Locale-independent path order for stable trees across hosts.
 */
export function sortResourceEntries(entries: ResourceEntry[]): ResourceEntry[] {
  return [...entries].sort((a, b) => {
    const rank = (kind: ResourceEntry["kind"]) =>
      kind === "directory" ? 0 : kind === "file" ? 1 : 2;
    const byKind = rank(a.kind) - rank(b.kind);
    if (byKind !== 0) return byKind;
    return a.path < b.path ? -1 : a.path > b.path ? 1 : 0;
  });
}

export function sortRecentResources(recents: RecentResource[]): RecentResource[] {
  return [...recents].sort((a, b) => {
    if (a.openedAt !== b.openedAt) {
      return a.openedAt < b.openedAt ? 1 : -1;
    }
    return a.path < b.path ? -1 : a.path > b.path ? 1 : 0;
  });
}

function looksLikeMachinePath(message: string): boolean {
  return /[\\/]/.test(message)
    || /[A-Za-z]:\\/.test(message)
    || message.includes("://")
    || /[A-Za-z]:\//.test(message);
}

/**
 * Convert gateway failures into opaque inline copy.
 * Never surface machine paths or host filesystem detail.
 */
export function opaqueResourceErrorMessage(error: unknown): string {
  const normalized = normalizeResourceGatewayError(error);
  const message = normalized.message.trim();
  if (message.length === 0 || looksLikeMachinePath(message)) {
    return FALLBACK_ERROR_MESSAGE;
  }
  return message;
}

export function filterPortableSearchResults(
  results: ResourceSearchResult[],
): ResourceSearchResult[] {
  return results.filter((hit) => isPortableRelativePath(hit.path));
}

export function filterPortableEntries(entries: ResourceEntry[]): ResourceEntry[] {
  return entries.filter((entry) => isPortableRelativePath(entry.path));
}

export function filterPortableRecents(recents: RecentResource[]): RecentResource[] {
  return recents.filter((item) => isPortableRelativePath(item.path));
}
