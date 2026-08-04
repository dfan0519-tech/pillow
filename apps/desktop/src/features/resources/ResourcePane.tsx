/**
 * Collapsible project resource context panel (Wave C2-C1).
 * Contextual only — never a permanent dashboard. Conversation stays visual centre.
 * No filesystem access; portable relative paths via ResourceGateway only.
 *
 * Wave C2-D adds optional controlled open mode for workbench embedding while
 * preserving the standalone closed-by-default uncontrolled contract.
 */

import { useCallback, useEffect, useId, useRef, useState } from "react";
import type { ResourceGateway } from "../../lib/resourceGateway";
import type { RecentResource, ResourceEntry, ResourceSearchResult } from "../../lib/resourceGateway";
import { ResourceTree } from "./ResourceTree";
import {
  filterPortableEntries,
  filterPortableRecents,
  filterPortableSearchResults,
  isPortableRelativePath,
  opaqueResourceErrorMessage,
  RESOURCE_SEARCH_DEBOUNCE_MS,
  sortRecentResources,
  sortResourceEntries,
} from "./resourcePaneState";
import type { BrowseStatus, SearchStatus } from "./resourceTypes";
import "../../styles/resource-pane.css";

export type ResourcePaneProps = {
  projectId: string;
  gateway: ResourceGateway;
  onOpenResource: (relativePath: string) => void;
  /** When defined, open state is controlled by the parent. */
  open?: boolean;
  onOpenChange?: (open: boolean) => void;
  /** Defaults true. Workbench host uses false so the stage header is the only disclosure. */
  showToggle?: boolean;
};

export function ResourcePane({
  projectId,
  gateway,
  onOpenResource,
  open: openProp,
  onOpenChange,
  showToggle = true,
}: ResourcePaneProps) {
  const panelId = useId();
  const regionId = `${panelId}-region`;

  const isControlled = openProp !== undefined;
  const [internalOpen, setInternalOpen] = useState(false);
  const open = isControlled ? openProp : internalOpen;

  const setOpen = useCallback(
    (next: boolean) => {
      if (!isControlled) {
        setInternalOpen(next);
      }
      onOpenChange?.(next);
    },
    [isControlled, onOpenChange],
  );

  const [browseStatus, setBrowseStatus] = useState<BrowseStatus>("idle");
  const [searchStatus, setSearchStatus] = useState<SearchStatus>("idle");
  const [searchInput, setSearchInput] = useState("");
  const [debouncedQuery, setDebouncedQuery] = useState("");
  const [recents, setRecents] = useState<RecentResource[]>([]);
  const [rootEntries, setRootEntries] = useState<ResourceEntry[]>([]);
  const [searchResults, setSearchResults] = useState<ResourceSearchResult[]>([]);
  const [errorMessage, setErrorMessage] = useState<string | null>(null);

  const browseGen = useRef(0);
  const searchGen = useRef(0);

  const openResource = useCallback(
    (relativePath: string) => {
      if (!isPortableRelativePath(relativePath)) return;
      onOpenResource(relativePath);
    },
    [onOpenResource],
  );

  // Project switch: clear in-flight results and local state before any new request.
  useEffect(() => {
    browseGen.current += 1;
    searchGen.current += 1;
    setBrowseStatus(open ? "loading" : "idle");
    setSearchStatus("idle");
    setSearchInput("");
    setDebouncedQuery("");
    setRecents([]);
    setRootEntries([]);
    setSearchResults([]);
    setErrorMessage(null);
  }, [projectId]); // eslint-disable-line react-hooks/exhaustive-deps -- intentional: projectId only

  // Explicit search debounce (cancellable via cleanup + generation).
  useEffect(() => {
    if (!open) return;
    const handle = window.setTimeout(() => {
      setDebouncedQuery(searchInput.trim());
    }, RESOURCE_SEARCH_DEBOUNCE_MS);
    return () => {
      window.clearTimeout(handle);
    };
  }, [searchInput, open]);

  // Browse load: recents then root tree when open.
  useEffect(() => {
    if (!open) return;

    const gen = ++browseGen.current;
    setBrowseStatus("loading");
    setErrorMessage(null);

    let cancelled = false;

    void (async () => {
      try {
        const [recentList, entryList] = await Promise.all([
          gateway.listRecentProjectResources({ projectId }),
          gateway.listProjectResources({ projectId }),
        ]);
        if (cancelled || gen !== browseGen.current) return;
        setRecents(sortRecentResources(filterPortableRecents(recentList)));
        setRootEntries(sortResourceEntries(filterPortableEntries(entryList)));
        setBrowseStatus("ready");
        setErrorMessage(null);
      } catch (error) {
        if (cancelled || gen !== browseGen.current) return;
        setRecents([]);
        setRootEntries([]);
        setBrowseStatus("error");
        setErrorMessage(opaqueResourceErrorMessage(error));
      }
    })();

    return () => {
      cancelled = true;
    };
  }, [open, projectId, gateway]);

  // Search load when debounced query is non-empty.
  useEffect(() => {
    if (!open) return;

    if (debouncedQuery.length === 0) {
      searchGen.current += 1;
      setSearchResults([]);
      setSearchStatus("idle");
      return;
    }

    const gen = ++searchGen.current;
    setSearchStatus("loading");

    let cancelled = false;

    void (async () => {
      try {
        const hits = await gateway.searchProjectResources({
          projectId,
          query: debouncedQuery,
        });
        if (cancelled || gen !== searchGen.current) return;
        setSearchResults(filterPortableSearchResults(hits));
        setSearchStatus("ready");
      } catch (error) {
        if (cancelled || gen !== searchGen.current) return;
        setSearchResults([]);
        setSearchStatus("error");
        setErrorMessage(opaqueResourceErrorMessage(error));
      }
    })();

    return () => {
      cancelled = true;
    };
  }, [debouncedQuery, open, projectId, gateway]);

  const showSearch = debouncedQuery.length > 0;
  const toggleLabel = open ? "关闭资源面板" : "打开资源面板";

  return (
    <section
      className="ctx-resource-pane"
      data-open={open ? "true" : "false"}
      data-testid="ctx-resource-pane"
    >
      {showToggle ? (
        <button
          aria-controls={regionId}
          aria-expanded={open}
          aria-label={toggleLabel}
          className="ctx-resource-pane__toggle"
          onClick={() => setOpen(!open)}
          type="button"
        >
          资源
        </button>
      ) : null}

      {open ? (
        <div
          aria-label="项目资源"
          className="ctx-resource-pane__body"
          id={regionId}
          role="region"
        >
          <div className="ctx-resource-pane__search">
            <label className="ctx-resource-pane__search-label" htmlFor={`${panelId}-search`}>
              搜索项目资源
            </label>
            <input
              aria-label="搜索项目资源"
              autoComplete="off"
              className="ctx-resource-pane__search-input"
              id={`${panelId}-search`}
              onChange={(event) => setSearchInput(event.target.value)}
              placeholder="搜索…"
              spellCheck={false}
              type="search"
              value={searchInput}
            />
          </div>

          {browseStatus === "loading" && !showSearch ? (
            <p aria-live="polite" className="ctx-resource-pane__status">
              加载中…
            </p>
          ) : null}

          {browseStatus === "error" && errorMessage && !showSearch ? (
            <p className="ctx-resource-pane__status ctx-resource-pane__status--error" role="alert">
              {errorMessage}
            </p>
          ) : null}

          {searchStatus === "error" && errorMessage && showSearch ? (
            <p className="ctx-resource-pane__status ctx-resource-pane__status--error" role="alert">
              {errorMessage}
            </p>
          ) : null}

          {showSearch ? (
            <section
              aria-label="搜索结果"
              className="ctx-resource-pane__section"
            >
              {searchStatus === "loading" ? (
                <p aria-live="polite" className="ctx-resource-pane__status">
                  搜索中…
                </p>
              ) : null}
              {searchStatus === "ready" && searchResults.length === 0 ? (
                <p className="ctx-resource-pane__status">无匹配结果</p>
              ) : null}
              {searchResults.length > 0 ? (
                <ul className="ctx-resource-pane__list" role="list">
                  {searchResults.map((hit) => (
                    <li key={`${hit.path}:${hit.line}:${hit.column}`}>
                      <button
                        aria-label={`${hit.path} 第 ${hit.line} 行`}
                        className="ctx-resource-pane__row"
                        onClick={() => openResource(hit.path)}
                        type="button"
                      >
                        <span className="ctx-resource-pane__row-path">{hit.path}</span>
                        <span className="ctx-resource-pane__row-meta">
                          :{hit.line}
                        </span>
                        <span className="ctx-resource-pane__row-preview">{hit.preview}</span>
                      </button>
                    </li>
                  ))}
                </ul>
              ) : null}
            </section>
          ) : (
            <>
              {browseStatus === "ready" || browseStatus === "loading" ? (
                <section
                  aria-label="最近打开"
                  className="ctx-resource-pane__section"
                >
                  <h3 className="ctx-resource-pane__heading">最近</h3>
                  {browseStatus === "ready" && recents.length === 0 ? (
                    <p className="ctx-resource-pane__status">暂无最近打开</p>
                  ) : null}
                  {recents.length > 0 ? (
                    <ul className="ctx-resource-pane__list" role="list">
                      {recents.map((item) => (
                        <li key={item.path}>
                          <button
                            aria-label={item.path}
                            className="ctx-resource-pane__row"
                            onClick={() => openResource(item.path)}
                            type="button"
                          >
                            {item.path}
                          </button>
                        </li>
                      ))}
                    </ul>
                  ) : null}
                </section>
              ) : null}

              {browseStatus === "ready" || browseStatus === "loading" ? (
                <section
                  aria-label="项目文件"
                  className="ctx-resource-pane__section"
                >
                  <h3 className="ctx-resource-pane__heading">文件</h3>
                  {browseStatus === "ready" && rootEntries.length === 0 ? (
                    <p className="ctx-resource-pane__status">暂无文件</p>
                  ) : null}
                  {rootEntries.length > 0 ? (
                    <ResourceTree
                      key={projectId}
                      entries={rootEntries}
                      gateway={gateway}
                      onOpenResource={openResource}
                      projectId={projectId}
                    />
                  ) : null}
                </section>
              ) : null}
            </>
          )}
        </div>
      ) : null}
    </section>
  );
}
