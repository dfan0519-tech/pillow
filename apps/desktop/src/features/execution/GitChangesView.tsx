/**
 * Git Changes body for the contextual resource pane (C4 D3B4 + mutations).
 * Wire DTOs come only from gitGateway — UI loading/error/stale/mutating are
 * separate props. Mutations are callback-driven; this view never invokes the
 * host. Host-built status/diff only — no spawn, no native paths.
 *
 * Push is a multi-step external publish flow: the primary control only opens
 * confirmation; the gateway is called only after the user confirms remote +
 * branch.
 */

import { useEffect, useId, useState } from "react";
import type {
  GitDiffView,
  GitRepositoryView,
  GitStatusEntryView,
  GitStatusKind,
  GitStatusView,
} from "../../lib/gitGateway";
import "../../styles/execution.css";

/** Exact push-confirmation copy (pinned by tests and the execution report). */
export const GIT_PUSH_CONFIRM_TITLE = "推送是对外发布";
export const GIT_PUSH_CONFIRM_BODY =
  "即将把本地分支推送到远端。此操作会发布到远程仓库，难以撤回。请确认远端名称与分支后再继续。";
export const GIT_PUSH_CONFIRM_SUBMIT = "确认推送到远端";
export const GIT_PUSH_OPEN_LABEL = "准备推送…";
export const GIT_PUSH_CANCEL_LABEL = "取消";

export type GitPushConfirmPayload = {
  remoteName: string;
  branchName: string;
};

export type GitChangesViewProps = {
  repositories?: GitRepositoryView[];
  selectedRepositoryId?: string | null;
  status: GitStatusView | null;
  diff?: GitDiffView | null;
  loading?: boolean;
  error?: string | null;
  /** UI-only stale guidance; never invent stale on wire status/diff records. */
  stale?: boolean;
  /** True while a mutation is in flight — disables repeat submit. */
  mutating?: boolean;
  onSelectRepository?: (repositoryId: string) => void;
  onSelectPath?: (portablePath: string) => void;
  onRefresh?: () => void;
  onStagePath?: (portablePath: string) => void;
  onUnstagePath?: (portablePath: string) => void;
  onCommit?: (message: string) => void;
  /**
   * Called only after the in-view push confirmation step. Never from the
   * primary “准备推送…” control alone.
   */
  onPush?: (payload: GitPushConfirmPayload) => void;
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

/**
 * Host status entries do not yet carry staged/worktree split. Heuristic until
 * the host admits index/worktree (or staged) fields — see execution report.
 */
function canStageEntry(kind: GitStatusKind): boolean {
  return kind !== "ignored";
}

function canUnstageEntry(kind: GitStatusKind): boolean {
  return kind !== "untracked" && kind !== "ignored";
}

function EntryRow({
  entry,
  index,
  mutating,
  onSelectPath,
  onStagePath,
  onUnstagePath,
}: {
  entry: GitStatusEntryView;
  index: number;
  mutating: boolean;
  onSelectPath?: (portablePath: string) => void;
  onStagePath?: (portablePath: string) => void;
  onUnstagePath?: (portablePath: string) => void;
}) {
  const showStage = Boolean(onStagePath) && canStageEntry(entry.kind);
  const showUnstage = Boolean(onUnstagePath) && canUnstageEntry(entry.kind);
  const hasActions = showStage || showUnstage;

  return (
    <li
      className="git-changes-view__entry"
      data-testid={`git-changes-entry-${index}`}
    >
      <div className="git-changes-view__entry-main">
        {onSelectPath ? (
          <button
            className="git-changes-view__entry-select"
            data-testid={`git-changes-entry-select-${index}`}
            disabled={mutating}
            onClick={() => onSelectPath(entry.path)}
            type="button"
          >
            <span className="git-changes-view__kind" data-testid="git-changes-kind">
              {kindLabel(entry.kind)}
            </span>
            <code className="git-changes-view__path">{entry.path}</code>
            {entry.originalPath ? (
              <span className="git-changes-view__rename">
                ← <code>{entry.originalPath}</code>
              </span>
            ) : null}
          </button>
        ) : (
          <div className="git-changes-view__entry-select git-changes-view__entry-select--static">
            <span className="git-changes-view__kind" data-testid="git-changes-kind">
              {kindLabel(entry.kind)}
            </span>
            <code className="git-changes-view__path">{entry.path}</code>
            {entry.originalPath ? (
              <span className="git-changes-view__rename">
                ← <code>{entry.originalPath}</code>
              </span>
            ) : null}
          </div>
        )}
      </div>
      {hasActions ? (
        <div
          className="git-changes-view__entry-actions"
          data-testid={`git-changes-entry-actions-${index}`}
        >
          {showStage ? (
            <button
              className="git-changes-view__action"
              data-testid={`git-changes-stage-${index}`}
              disabled={mutating}
              onClick={() => onStagePath?.(entry.path)}
              type="button"
            >
              暂存
            </button>
          ) : null}
          {showUnstage ? (
            <button
              className="git-changes-view__action"
              data-testid={`git-changes-unstage-${index}`}
              disabled={mutating}
              onClick={() => onUnstagePath?.(entry.path)}
              type="button"
            >
              取消暂存
            </button>
          ) : null}
        </div>
      ) : null}
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
  mutating = false,
  onSelectRepository,
  onSelectPath,
  onRefresh,
  onStagePath,
  onUnstagePath,
  onCommit,
  onPush,
}: GitChangesViewProps) {
  const showRepoSelector = repositories.length > 1;
  const [commitMessage, setCommitMessage] = useState("");
  const [pushConfirmOpen, setPushConfirmOpen] = useState(false);
  const [pushRemoteName, setPushRemoteName] = useState("origin");
  const commitMessageId = useId();
  const pushRemoteId = useId();

  const branchName =
    status && !status.detached && status.branch ? status.branch : null;
  const canOpenPush =
    Boolean(onPush) && Boolean(branchName) && !mutating && !loading;
  const trimmedMessage = commitMessage.trim();
  const canCommit =
    Boolean(onCommit) && trimmedMessage.length > 0 && !mutating && !loading;

  // Reset local push draft when leaving confirm or when branch identity changes.
  useEffect(() => {
    if (!pushConfirmOpen) return;
    setPushRemoteName((current) => (current.trim().length > 0 ? current : "origin"));
  }, [pushConfirmOpen]);

  useEffect(() => {
    setPushConfirmOpen(false);
  }, [selectedRepositoryId, status?.branch, status?.detached]);

  return (
    <section
      aria-label="工作区变更"
      className="git-changes-view"
      data-mutating={mutating ? "true" : "false"}
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
        {mutating ? (
          <p
            aria-live="polite"
            className="git-changes-view__status"
            data-testid="git-changes-mutating"
          >
            正在执行 Git 操作…
          </p>
        ) : null}
        {onRefresh ? (
          <button
            className="git-changes-view__refresh"
            data-testid="git-changes-refresh"
            disabled={mutating}
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
                disabled={!repo.available || mutating}
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
                mutating={mutating}
                onSelectPath={onSelectPath}
                onStagePath={onStagePath}
                onUnstagePath={onUnstagePath}
              />
            ))}
          </ul>
        </>
      ) : null}

      {onCommit ? (
        <div
          className="git-changes-view__commit"
          data-testid="git-changes-commit-form"
        >
          <label className="git-changes-view__label" htmlFor={commitMessageId}>
            提交说明
          </label>
          <textarea
            className="git-changes-view__commit-message"
            data-testid="git-changes-commit-message"
            disabled={mutating || loading}
            id={commitMessageId}
            onChange={(event) => setCommitMessage(event.target.value)}
            placeholder="描述本次提交…"
            rows={3}
            value={commitMessage}
          />
          <button
            className="git-changes-view__commit-submit"
            data-testid="git-changes-commit-submit"
            disabled={!canCommit}
            onClick={() => {
              if (!canCommit) return;
              onCommit(trimmedMessage);
              setCommitMessage("");
            }}
            type="button"
          >
            提交
          </button>
        </div>
      ) : null}

      {onPush ? (
        <div
          className="git-changes-view__push"
          data-testid="git-changes-push-panel"
        >
          {!pushConfirmOpen ? (
            <button
              className="git-changes-view__push-open"
              data-testid="git-changes-push-open"
              disabled={!canOpenPush}
              onClick={() => setPushConfirmOpen(true)}
              title={
                branchName
                  ? undefined
                  : "当前为分离 HEAD，无法推送命名分支"
              }
              type="button"
            >
              {GIT_PUSH_OPEN_LABEL}
            </button>
          ) : (
            <div
              className="git-changes-view__push-confirm"
              data-testid="git-changes-push-confirm"
              role="group"
              aria-label={GIT_PUSH_CONFIRM_TITLE}
            >
              <p
                className="git-changes-view__push-title"
                data-testid="git-changes-push-title"
              >
                {GIT_PUSH_CONFIRM_TITLE}
              </p>
              <p
                className="git-changes-view__push-body"
                data-testid="git-changes-push-body"
              >
                {GIT_PUSH_CONFIRM_BODY}
              </p>
              <p className="git-changes-view__push-target">
                <span className="git-changes-view__label">远端</span>
                <input
                  className="git-changes-view__push-remote"
                  data-testid="git-changes-push-remote"
                  disabled={mutating}
                  id={pushRemoteId}
                  onChange={(event) => setPushRemoteName(event.target.value)}
                  spellCheck={false}
                  type="text"
                  value={pushRemoteName}
                />
              </p>
              <p className="git-changes-view__push-target">
                <span className="git-changes-view__label">分支</span>
                <code data-testid="git-changes-push-branch">
                  {branchName ?? "—"}
                </code>
              </p>
              <div className="git-changes-view__push-actions">
                <button
                  className="git-changes-view__action"
                  data-testid="git-changes-push-cancel"
                  disabled={mutating}
                  onClick={() => setPushConfirmOpen(false)}
                  type="button"
                >
                  {GIT_PUSH_CANCEL_LABEL}
                </button>
                <button
                  className="git-changes-view__push-submit"
                  data-testid="git-changes-push-confirm-submit"
                  disabled={
                    mutating
                    || !branchName
                    || pushRemoteName.trim().length === 0
                  }
                  onClick={() => {
                    if (!branchName || mutating) return;
                    const remote = pushRemoteName.trim();
                    if (remote.length === 0) return;
                    onPush({ remoteName: remote, branchName });
                    setPushConfirmOpen(false);
                  }}
                  type="button"
                >
                  {GIT_PUSH_CONFIRM_SUBMIT}
                </button>
              </div>
            </div>
          )}
        </div>
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
