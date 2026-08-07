/**
 * Read-only Git Changes body for the contextual resource pane (C4 D3B4).
 * Wire DTOs come only from gitGateway — UI loading/error/stale are separate props.
 * Host-built status/diff only — no spawn, no native paths, no project state.
 */

import type {
  GitDiffView,
  GitRepositoryView,
  GitStatusEntryView,
  GitStatusKind,
  GitStatusView,
} from "../../lib/gitGateway";
import "../../styles/execution.css";

export type GitChangesViewProps = {
  repositories?: GitRepositoryView[];
  selectedRepositoryId?: string | null;
  status: GitStatusView | null;
  diff?: GitDiffView | null;
  loading?: boolean;
  error?: string | null;
  /** UI-only stale guidance; never invent stale on wire status/diff records. */
  stale?: boolean;
  onSelectRepository?: (repositoryId: string) => void;
  onSelectPath?: (portablePath: string) => void;
  onRefresh?: () => void;
};

function kindLabel(kind: GitStatusKind): string {
  switch (kind) {
    case "modified":
      return "修改";
    case "added":
      return "新增";
    case "deleted":
      return "删除";
    case "untracked":
      return "未跟踪";
    case "renamed":
      return "重命名";
    case "copied":
      return "复制";
    case "typechange":
      return "类型变更";
    case "conflict":
      return "冲突";
    case "submodule":
      return "子模块";
    case "ignored":
      return "已忽略";
    case "unknown":
      return "未知";
  }
}

function branchDisplay(status: GitStatusView): string {
  if (status.detached || !status.branch) {
    return "分离 HEAD";
  }
  return status.branch;
}

function repositoryLabel(repositoryId: string): string {
  return repositoryId === "." ? "项目根仓库" : repositoryId;
}

function EntryRow({
  entry,
  index,
  onSelectPath,
}: {
  entry: GitStatusEntryView;
  index: number;
  onSelectPath?: (portablePath: string) => void;
}) {
  const interactive = Boolean(onSelectPath);
  const content = (
    <>
      <span className="git-changes-view__kind" data-testid="git-changes-kind">
        {kindLabel(entry.kind)}
      </span>
      <code className="git-changes-view__path">{entry.path}</code>
      {entry.originalPath ? (
        <span className="git-changes-view__rename">
          ← <code>{entry.originalPath}</code>
        </span>
      ) : null}
    </>
  );

  if (interactive) {
    return (
      <li>
        <button
          className="git-changes-view__entry git-changes-view__entry--button"
          data-testid={`git-changes-entry-${index}`}
          onClick={() => onSelectPath?.(entry.path)}
          type="button"
        >
          {content}
        </button>
      </li>
    );
  }

  return (
    <li
      className="git-changes-view__entry"
      data-testid={`git-changes-entry-${index}`}
    >
      {content}
    </li>
  );
}

export function GitChangesView({
  repositories = [],
  selectedRepositoryId = null,
  status,
  diff = null,
  loading = false,
  error = null,
  stale = false,
  onSelectRepository,
  onSelectPath,
  onRefresh,
}: GitChangesViewProps) {
  const showRepoSelector = repositories.length > 1;

  return (
    <section
      aria-label="工作区变更"
      className="git-changes-view"
      data-stale={stale ? "true" : "false"}
      data-testid="git-changes-view"
    >
      <header className="git-changes-view__header">
        {/*
          No visible "变更" title here: the host pane already exposes mode via the
          selected tab (or pane h2 when the switcher is absent). This section keeps
          aria-label="工作区变更" so screen readers still name the body region.
        */}
        {status ? (
          <p className="git-changes-view__identity">
            <span data-testid="git-changes-branch">{branchDisplay(status)}</span>
            <span aria-hidden="true" className="git-changes-view__sep">
              ·
            </span>
            <code data-testid="git-changes-head">{status.headShort}</code>
            <span aria-hidden="true" className="git-changes-view__sep">
              ·
            </span>
            <span data-testid="git-changes-dirty-count">
              {status.dirtyCount} 项变更
            </span>
          </p>
        ) : null}
        {error ? (
          <p
            className="git-changes-view__notice git-changes-view__notice--error"
            data-testid="git-changes-error"
            role="alert"
          >
            {error}
          </p>
        ) : null}
        {stale ? (
          <p
            className="git-changes-view__notice"
            data-testid="git-changes-stale"
            role="status"
          >
            状态已过期，请刷新
          </p>
        ) : null}
        {status?.truncated ? (
          <p
            className="git-changes-view__notice"
            data-testid="git-changes-truncated"
            role="status"
          >
            状态列表已截断
          </p>
        ) : null}
        {onRefresh ? (
          <button
            className="git-changes-view__refresh"
            data-testid="git-changes-refresh"
            onClick={onRefresh}
            type="button"
          >
            刷新
          </button>
        ) : null}
      </header>

      {showRepoSelector ? (
        <div
          className="git-changes-view__repos"
          data-testid="git-changes-repo-selector"
          role="group"
          aria-label="仓库选择"
        >
          {repositories.map((repo) => {
            const selected = repo.repositoryId === selectedRepositoryId;
            return (
              <button
                key={repo.repositoryId}
                aria-pressed={selected}
                className={
                  selected
                    ? "git-changes-view__repo git-changes-view__repo--selected"
                    : "git-changes-view__repo"
                }
                data-available={repo.available ? "true" : "false"}
                data-testid={`git-changes-repo-${repo.repositoryId === "." ? "root" : repo.repositoryId}`}
                disabled={!repo.available}
                onClick={() => {
                  if (repo.available) {
                    onSelectRepository?.(repo.repositoryId);
                  }
                }}
                type="button"
                title={
                  repo.available
                    ? repositoryLabel(repo.repositoryId)
                    : `${repositoryLabel(repo.repositoryId)}（不可用）`
                }
              >
                <code>{repositoryLabel(repo.repositoryId)}</code>
                {!repo.available ? (
                  <span className="git-changes-view__repo-unavailable">
                    不可用
                  </span>
                ) : null}
              </button>
            );
          })}
        </div>
      ) : null}

      {loading ? (
        <p
          aria-live="polite"
          className="git-changes-view__status"
          data-testid="git-changes-loading"
        >
          加载变更…
        </p>
      ) : null}

      {!loading && !error && !status ? (
        <p className="git-changes-view__status" data-testid="git-changes-empty">
          无工作区变更数据
        </p>
      ) : null}

      {status ? (
        <>
          {status.files.length === 0 && !loading ? (
            <p
              className="git-changes-view__status"
              data-testid="git-changes-clean"
            >
              工作区干净
            </p>
          ) : null}
          <ul
            className="git-changes-view__list"
            data-testid="git-changes-file-list"
          >
            {status.files.map((entry, index) => (
              <EntryRow
                entry={entry}
                index={index}
                key={`${entry.kind}:${entry.path}:${entry.originalPath ?? ""}`}
                onSelectPath={onSelectPath}
              />
            ))}
          </ul>
        </>
      ) : null}

      {diff ? (
        <div className="git-changes-view__diff" data-testid="git-changes-diff">
          <p className="git-changes-view__diff-path">
            <code data-testid="git-changes-diff-path">{diff.path}</code>
            {diff.originalPath ? (
              <span className="git-changes-view__rename">
                ← <code>{diff.originalPath}</code>
              </span>
            ) : null}
          </p>
          {diff.kind === "binary" ? (
            <p
              className="git-changes-view__notice"
              data-testid="git-changes-diff-binary"
              role="status"
            >
              二进制文件
            </p>
          ) : null}
          {diff.kind === "unsupported" ? (
            <p
              className="git-changes-view__notice"
              data-testid="git-changes-diff-unsupported"
              role="status"
            >
              不支持的差异类型
            </p>
          ) : null}
          {diff.truncated ? (
            <p
              className="git-changes-view__notice"
              data-testid="git-changes-diff-truncated"
              role="status"
            >
              差异已截断
            </p>
          ) : null}
          {diff.kind === "text" && diff.unified !== undefined ? (
            <pre
              className="git-changes-view__diff-body"
              data-testid="git-changes-diff-unified"
              tabIndex={0}
            >
              {diff.unified}
            </pre>
          ) : null}
        </div>
      ) : null}
    </section>
  );
}
