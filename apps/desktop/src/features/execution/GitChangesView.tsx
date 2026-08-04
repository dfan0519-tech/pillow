/**
 * Read-only Git Changes body for the contextual resource pane (C4.0 G0).
 * Host-built status/diff DTOs only — no spawn, no native paths.
 */

import type {
  GitDiffView,
  GitStatusEntry,
  GitStatusKind,
  GitStatusView,
} from "./executionTypes";
import "../../styles/execution.css";

export type GitChangesViewProps = {
  status: GitStatusView;
  diff?: GitDiffView | null;
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

function EntryRow({
  entry,
  index,
  onSelectPath,
}: {
  entry: GitStatusEntry;
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
      {entry.isBinary ? (
        <span className="git-changes-view__binary-badge">二进制</span>
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
  status,
  diff,
  onSelectPath,
  onRefresh,
}: GitChangesViewProps) {
  return (
    <section
      aria-label="工作区变更"
      className="git-changes-view"
      data-stale={status.stale ? "true" : "false"}
      data-testid="git-changes-view"
    >
      <header className="git-changes-view__header">
        <h3 className="git-changes-view__title">变更</h3>
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
        {status.unavailableReason ? (
          <p
            className="git-changes-view__notice"
            data-testid="git-changes-unavailable"
            role="status"
          >
            {status.unavailableReason}
          </p>
        ) : null}
        {status.stale ? (
          <p
            className="git-changes-view__notice"
            data-testid="git-changes-stale"
            role="status"
          >
            状态已过期，请刷新
          </p>
        ) : null}
        {status.truncated ? (
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
          {diff.stale ? (
            <p
              className="git-changes-view__notice"
              data-testid="git-changes-diff-stale"
              role="status"
            >
              差异已过期，请刷新
            </p>
          ) : null}
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
