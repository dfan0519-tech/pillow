/**
 * Renderer-side types for the collapsible project resource context panel (C2-C1).
 * Portable relative paths only — no roots, handles, or native paths.
 */

import type {
  RecentResource,
  ResourceEntry,
  ResourceGateway,
  ResourceSearchResult,
} from "../../lib/resourceGateway";

export type ResourcePaneProps = {
  projectId: string;
  gateway: ResourceGateway;
  onOpenResource: (relativePath: string) => void;
};

export type BrowseStatus = "idle" | "loading" | "ready" | "error";

export type SearchStatus = "idle" | "loading" | "ready" | "error";

/** Serializable panel snapshot used by pure helpers and tests. */
export type ResourcePaneSnapshot = {
  open: boolean;
  projectId: string;
  browseStatus: BrowseStatus;
  searchStatus: SearchStatus;
  searchQuery: string;
  recents: RecentResource[];
  rootEntries: ResourceEntry[];
  searchResults: ResourceSearchResult[];
  /** Opaque, path-free message for inline display. */
  errorMessage: string | null;
};

export type ResourceTreeNodeKind = ResourceEntry["kind"];

export type ResourceRowModel = {
  path: string;
  kind: ResourceTreeNodeKind;
  /** Short label for dense rows (leaf name). */
  label: string;
};
