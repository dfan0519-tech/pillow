/**
 * Concise project resource tree rows (C2-C1).
 * Expands directories via ResourceGateway; opens files through onOpenResource only.
 */

import { useCallback, useState } from "react";
import { IconChevronDown, IconChevronRight, IconFolder } from "../../components/icons";
import type { ResourceEntry, ResourceGateway } from "../../lib/resourceGateway";
import {
  filterPortableEntries,
  isPortableRelativePath,
  opaqueResourceErrorMessage,
  resourceLeafName,
  sortResourceEntries,
  toResourceRow,
} from "./resourcePaneState";

export type ResourceTreeProps = {
  projectId: string;
  gateway: ResourceGateway;
  entries: ResourceEntry[];
  onOpenResource: (relativePath: string) => void;
  /** Indentation depth for nested children. */
  depth?: number;
};

type DirState =
  | { status: "collapsed" }
  | { status: "loading" }
  | { status: "ready"; children: ResourceEntry[] }
  | { status: "error"; message: string };

/**
 * Renders a deterministic list of resource rows.
 * Directory expand state is local and remounts with the parent key on project switch.
 */
export function ResourceTree({
  projectId,
  gateway,
  entries,
  onOpenResource,
  depth = 0,
}: ResourceTreeProps) {
  const [dirState, setDirState] = useState<Record<string, DirState>>({});

  const openIfPortable = useCallback(
    (path: string) => {
      if (!isPortableRelativePath(path)) return;
      onOpenResource(path);
    },
    [onOpenResource],
  );

  const toggleDirectory = useCallback(
    async (path: string) => {
      const current = dirState[path] ?? { status: "collapsed" as const };
      if (current.status === "ready" || current.status === "loading") {
        setDirState((prev) => ({ ...prev, [path]: { status: "collapsed" } }));
        return;
      }
      if (current.status === "error") {
        // Retry on re-expand.
      }
      setDirState((prev) => ({ ...prev, [path]: { status: "loading" } }));
      try {
        const children = sortResourceEntries(
          filterPortableEntries(
            await gateway.listProjectResources({
              projectId,
              relativePath: path,
            }),
          ),
        );
        setDirState((prev) => ({
          ...prev,
          [path]: { status: "ready", children },
        }));
      } catch (error) {
        setDirState((prev) => ({
          ...prev,
          [path]: {
            status: "error",
            message: opaqueResourceErrorMessage(error),
          },
        }));
      }
    },
    [dirState, gateway, projectId],
  );

  const ordered = sortResourceEntries(filterPortableEntries(entries));

  if (ordered.length === 0) {
    return null;
  }

  return (
    <ul className="ctx-resource-tree" role="list">
      {ordered.map((entry) => {
        const row = toResourceRow(entry);
        const isDir = entry.kind === "directory";
        const state = dirState[entry.path] ?? { status: "collapsed" as const };
        const expanded =
          state.status === "loading"
          || state.status === "ready"
          || state.status === "error";
        const fileLabel = entry.path.includes("/") ? entry.path : row.label;

        return (
          <li
            className="ctx-resource-tree__item"
            key={entry.path}
            style={{ paddingInlineStart: depth * 12 }}
          >
            {isDir ? (
              <>
                <button
                  aria-expanded={expanded}
                  aria-label={row.label}
                  className="ctx-resource-tree__row ctx-resource-tree__row--dir"
                  onClick={() => {
                    void toggleDirectory(entry.path);
                  }}
                  type="button"
                >
                  <span aria-hidden className="ctx-resource-tree__chevron">
                    {expanded ? (
                      <IconChevronDown size={14} />
                    ) : (
                      <IconChevronRight size={14} />
                    )}
                  </span>
                  <IconFolder size={14} />
                  <span className="ctx-resource-tree__label">{row.label}</span>
                </button>
                {state.status === "loading" ? (
                  <p
                    aria-live="polite"
                    className="ctx-resource-tree__status"
                    style={{ paddingInlineStart: (depth + 1) * 12 }}
                  >
                    加载中…
                  </p>
                ) : null}
                {state.status === "error" ? (
                  <p
                    className="ctx-resource-tree__status ctx-resource-tree__status--error"
                    role="alert"
                    style={{ paddingInlineStart: (depth + 1) * 12 }}
                  >
                    {state.message}
                  </p>
                ) : null}
                {state.status === "ready" && state.children.length > 0 ? (
                  <ResourceTree
                    depth={depth + 1}
                    entries={state.children}
                    gateway={gateway}
                    onOpenResource={onOpenResource}
                    projectId={projectId}
                  />
                ) : null}
                {state.status === "ready" && state.children.length === 0 ? (
                  <p
                    className="ctx-resource-tree__status"
                    style={{ paddingInlineStart: (depth + 1) * 12 }}
                  >
                    空目录
                  </p>
                ) : null}
              </>
            ) : (
              <button
                aria-label={fileLabel}
                className="ctx-resource-tree__row"
                onClick={() => openIfPortable(entry.path)}
                type="button"
              >
                <span aria-hidden className="ctx-resource-tree__chevron-spacer" />
                <span className="ctx-resource-tree__label">
                  {resourceLeafName(entry.path)}
                </span>
              </button>
            )}
          </li>
        );
      })}
    </ul>
  );
}
