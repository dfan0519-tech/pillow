import { useCallback, useEffect, useRef, useState } from "react";
import { IconClose } from "../../components/icons";
import type { ResourceReferenceView } from "../conversations/conversationReducer";
import { ResourcePane as ProjectResourceBrowser } from "../resources/ResourcePane";
import { ResourcePreview } from "../resources/ResourcePreview";
import { isPortableRelativePath } from "../resources/resourcePaneState";
import {
  normalizeExecError,
  type ExecGateway,
} from "../../lib/execGateway";
import {
  normalizeResourceGatewayError,
  type ResourceGateway,
  type ResourcePreview as ResourcePreviewDto,
} from "../../lib/resourceGateway";
import {
  normalizeWriteApprovalError,
  type WriteApprovalGateway,
  type WriteDiffView,
} from "../../lib/writeApprovalGateway";
import type {
  GitDiffView,
  GitRepositoryView,
  GitStatusView,
} from "../../lib/gitGateway";
import type { TerminalOutputModel } from "../execution/executionTypes";
import { GitChangesView } from "../execution/GitChangesView";
import { TerminalOutputView } from "../execution/TerminalOutputView";
import { WriteDiffPreview } from "./WriteDiffPreview";

export type WriteDiffTarget = {
  projectId: string;
  conversationId: string;
  proposalId: string;
  scopeGeneration: number;
};

/** Pull-on-demand process terminal target — never cancels on pane close. */
export type TerminalOutputTarget = {
  projectId: string;
  conversationId: string;
  proposalId: string;
  programLabel: string;
  argv: string[];
  scopeGeneration: number;
};

/**
 * Git Changes body model (read + structured mutations).
 * Wire DTOs from gitGateway only; loading/error/stale/mutating are UI presentation.
 * Field names are a frozen contract — do not rename existing keys.
 */
export type GitChangesPaneModel = {
  repositories: GitRepositoryView[];
  selectedRepositoryId: string | null;
  status: GitStatusView | null;
  diff: GitDiffView | null;
  loading: boolean;
  error: string | null;
  stale: boolean;
  /** True while stage/unstage/commit/push is in flight. */
  mutating: boolean;
  onSelectRepository: (repositoryId: string) => void;
  onSelectPath: (portablePath: string) => void;
  onRefresh: () => void;
  onStagePath: (portablePath: string) => void;
  onUnstagePath: (portablePath: string) => void;
  onCommit: (message: string) => void;
  onPush: (payload: { remoteName: string; branchName: string }) => void;
};

/**
 * Explicit contextual pane body mode (C4 D3B4 Round 4).
 * Default must be `resources` so Changes never displaces the C2 resource browser.
 */
export type ContextBodyMode = "resources" | "changes";

type PreviewScope = {
  projectId: string;
  generation: number;
};

type DiffScope = {
  projectId: string;
  conversationId: string;
  proposalId: string;
  scopeGeneration: number;
  generation: number;
};

type TerminalScope = {
  projectId: string;
  conversationId: string;
  proposalId: string;
  scopeGeneration: number;
  generation: number;
};

export type ResourcePaneProps = {
  open: boolean;
  onClose: () => void;
  hasProject: boolean;
  /** Active project id when a project is selected; null otherwise. */
  projectId: string | null;
  /** Optional typed resource gateway. When absent, conversation-reference fallback only. */
  resourceGateway?: ResourceGateway;
  /** Conversation-projected reference (fallback when gateway omitted). */
  resource: ResourceReferenceView | null;
  /**
   * Portable path requested from conversation / outer host. Triggers the same
   * preview path as browser selection when a gateway is present.
   * `nonce` forces re-request when the same path is opened again.
   */
  previewPathRequest?: { path: string; nonce: number } | null;
  /** Optional write-approval gateway for host-generated governed-write diffs. */
  writeApprovalGateway?: WriteApprovalGateway;
  /**
   * Dedicated governed-write diff target. Not a normal resource read.
   * Becomes inert when project/conversation generation changes.
   */
  writeDiffTarget?: WriteDiffTarget | null;
  /** Optional conversation-owned process gateway for bounded terminal output. */
  execGateway?: ExecGateway;
  /**
   * Dedicated process terminal-output target. Not a resource read and not a
   * cancel signal — closing the pane only clears local presentation state.
   */
  terminalOutputTarget?: TerminalOutputTarget | null;
  /**
   * Bumped by App on accepted process lifecycle events. Re-pulls open output
   * for the active terminal target without polling.
   */
  execProposalRefreshToken?: number;
  /**
   * Host-owned Git Changes presentation model. When present for the active
   * project and body mode is `changes`, mounts compact Changes body
   * (behind Terminal / write Diff). This pane never calls the Git gateway.
   */
  gitChanges?: GitChangesPaneModel | null;
  /**
   * When true, show the compact 资源 / 变更 mode switch (Git gateway available).
   * Does not alone select Changes — default body mode remains resources.
   */
  gitChangesCapable?: boolean;
  /**
   * Workbench-owned body mode. Default `resources` restores C2 resource browser.
   * Terminal / write-diff targets still outrank both modes.
   */
  bodyMode?: ContextBodyMode;
  /** Explicit mode selection; keyboard-reachable controls call this. */
  onBodyModeChange?: (mode: ContextBodyMode) => void;
};

/**
 * Contextual resource pane — integration host for project browser, bounded
 * preview, conversation reference fallback (Wave C2-D), and governed-write
 * diff review (Wave C3-E).
 * Upstream chrome: Grok App ResourceViewer aside shell at
 * cbbbdf17bb8db1588b6707f3f42f6b00397e27f9.
 */
export function ResourcePane({
  open,
  onClose,
  hasProject,
  projectId,
  resourceGateway,
  resource,
  previewPathRequest = null,
  writeApprovalGateway,
  writeDiffTarget = null,
  execGateway,
  terminalOutputTarget = null,
  execProposalRefreshToken = 0,
  gitChanges = null,
  gitChangesCapable = false,
  bodyMode = "resources",
  onBodyModeChange,
}: ResourcePaneProps) {
  const [preview, setPreview] = useState<ResourcePreviewDto | null>(null);
  const [previewError, setPreviewError] = useState<string | null>(null);
  const [previewLoading, setPreviewLoading] = useState(false);
  const [selectedPath, setSelectedPath] = useState<string | null>(null);
  const [writeDiff, setWriteDiff] = useState<WriteDiffView | null>(null);
  const [writeDiffError, setWriteDiffError] = useState<string | null>(null);
  const [writeDiffLoading, setWriteDiffLoading] = useState(false);
  const [terminalOutput, setTerminalOutput] = useState<TerminalOutputModel | null>(
    null,
  );
  const [terminalError, setTerminalError] = useState<string | null>(null);
  const [terminalLoading, setTerminalLoading] = useState(false);

  const generationRef = useRef(0);
  const diffGenerationRef = useRef(0);
  const terminalGenerationRef = useRef(0);
  const activeProjectIdRef = useRef<string | null>(projectId);
  activeProjectIdRef.current = projectId;
  const activeDiffTargetRef = useRef<WriteDiffTarget | null>(writeDiffTarget);
  activeDiffTargetRef.current = writeDiffTarget;
  const activeTerminalTargetRef = useRef<TerminalOutputTarget | null>(
    terminalOutputTarget,
  );
  activeTerminalTargetRef.current = terminalOutputTarget;

  const clearPreviewState = useCallback(() => {
    setPreview(null);
    setPreviewError(null);
    setPreviewLoading(false);
    setSelectedPath(null);
  }, []);

  const clearWriteDiffState = useCallback(() => {
    setWriteDiff(null);
    setWriteDiffError(null);
    setWriteDiffLoading(false);
  }, []);

  const clearTerminalState = useCallback(() => {
    setTerminalOutput(null);
    setTerminalError(null);
    setTerminalLoading(false);
  }, []);

  const invalidatePreviewScope = useCallback(() => {
    generationRef.current += 1;
    clearPreviewState();
  }, [clearPreviewState]);

  const invalidateDiffScope = useCallback(() => {
    diffGenerationRef.current += 1;
    clearWriteDiffState();
  }, [clearWriteDiffState]);

  const invalidateTerminalScope = useCallback(() => {
    terminalGenerationRef.current += 1;
    clearTerminalState();
  }, [clearTerminalState]);

  // Project change or unmount: discard every in-flight preview/diff/terminal.
  useEffect(() => {
    invalidatePreviewScope();
    invalidateDiffScope();
    invalidateTerminalScope();
    return () => {
      generationRef.current += 1;
      diffGenerationRef.current += 1;
      terminalGenerationRef.current += 1;
    };
  }, [projectId, invalidatePreviewScope, invalidateDiffScope, invalidateTerminalScope]);

  // Closing the panel must not leave a stale surface to resurrect on reopen.
  // Closing never cancels a process — it only clears local presentation.
  useEffect(() => {
    if (!open) {
      invalidatePreviewScope();
      invalidateDiffScope();
      invalidateTerminalScope();
    }
  }, [open, invalidatePreviewScope, invalidateDiffScope, invalidateTerminalScope]);

  // Conversation / proposal target change: drop prior diff results.
  useEffect(() => {
    invalidateDiffScope();
  }, [
    writeDiffTarget?.conversationId,
    writeDiffTarget?.proposalId,
    writeDiffTarget?.scopeGeneration,
    invalidateDiffScope,
  ]);

  // Terminal target identity change: drop prior output before re-pull.
  useEffect(() => {
    invalidateTerminalScope();
  }, [
    terminalOutputTarget?.conversationId,
    terminalOutputTarget?.proposalId,
    terminalOutputTarget?.scopeGeneration,
    invalidateTerminalScope,
  ]);

  const requestPreview = useCallback(
    (relativePath: string) => {
      if (!resourceGateway || !projectId) return;
      if (!isPortableRelativePath(relativePath)) return;

      // Selecting a normal resource leaves the governed-write diff target inert.
      invalidateDiffScope();

      generationRef.current += 1;
      const scope: PreviewScope = {
        projectId,
        generation: generationRef.current,
      };

      setSelectedPath(relativePath);
      setPreview(null);
      setPreviewError(null);
      setPreviewLoading(true);

      void (async () => {
        try {
          const result = await resourceGateway.previewProjectResource({
            projectId: scope.projectId,
            relativePath,
          });
          if (
            scope.generation !== generationRef.current
            || activeProjectIdRef.current !== scope.projectId
          ) {
            return;
          }
          setPreview(result);
          setPreviewError(null);
          setPreviewLoading(false);
        } catch (error) {
          if (
            scope.generation !== generationRef.current
            || activeProjectIdRef.current !== scope.projectId
          ) {
            return;
          }
          const normalized = normalizeResourceGatewayError(error);
          setPreview(null);
          setPreviewError(normalized.message);
          setPreviewLoading(false);
        }
      })();
    },
    [resourceGateway, projectId, invalidateDiffScope],
  );

  // Conversation (or host) portable path → same preview function.
  useEffect(() => {
    if (!open || !resourceGateway || !projectId) return;
    if (!previewPathRequest) return;
    if (!isPortableRelativePath(previewPathRequest.path)) return;
    requestPreview(previewPathRequest.path);
    // nonce intentionally included so re-opening the same path re-previews.
  }, [
    open,
    resourceGateway,
    projectId,
    previewPathRequest?.path,
    previewPathRequest?.nonce,
    requestPreview,
  ]);

  // Host-generated governed-write diff — never reinterpret as resource read.
  useEffect(() => {
    if (!open || !writeApprovalGateway || !projectId || !writeDiffTarget) {
      return;
    }
    if (writeDiffTarget.projectId !== projectId) return;

    diffGenerationRef.current += 1;
    const scope: DiffScope = {
      projectId: writeDiffTarget.projectId,
      conversationId: writeDiffTarget.conversationId,
      proposalId: writeDiffTarget.proposalId,
      scopeGeneration: writeDiffTarget.scopeGeneration,
      generation: diffGenerationRef.current,
    };

    setWriteDiff(null);
    setWriteDiffError(null);
    setWriteDiffLoading(true);

    void (async () => {
      try {
        const result = await writeApprovalGateway.openWriteProposalDiff({
          conversationId: scope.conversationId,
          proposalId: scope.proposalId,
        });
        if (
          scope.generation !== diffGenerationRef.current
          || activeProjectIdRef.current !== scope.projectId
          || activeDiffTargetRef.current?.conversationId !== scope.conversationId
          || activeDiffTargetRef.current?.proposalId !== scope.proposalId
          || activeDiffTargetRef.current?.scopeGeneration !== scope.scopeGeneration
        ) {
          return;
        }
        setWriteDiff(result);
        setWriteDiffError(null);
        setWriteDiffLoading(false);
      } catch (error) {
        if (
          scope.generation !== diffGenerationRef.current
          || activeProjectIdRef.current !== scope.projectId
          || activeDiffTargetRef.current?.conversationId !== scope.conversationId
          || activeDiffTargetRef.current?.proposalId !== scope.proposalId
          || activeDiffTargetRef.current?.scopeGeneration !== scope.scopeGeneration
        ) {
          return;
        }
        const normalized = normalizeWriteApprovalError(error);
        setWriteDiff(null);
        setWriteDiffError(normalized.message);
        setWriteDiffLoading(false);
      }
    })();
  }, [
    open,
    writeApprovalGateway,
    projectId,
    writeDiffTarget?.projectId,
    writeDiffTarget?.conversationId,
    writeDiffTarget?.proposalId,
    writeDiffTarget?.scopeGeneration,
  ]);

  // Pull-on-demand process output — event-token refresh, no polling.
  useEffect(() => {
    if (!open || !execGateway || !projectId || !terminalOutputTarget) {
      return;
    }
    if (terminalOutputTarget.projectId !== projectId) return;

    terminalGenerationRef.current += 1;
    const scope: TerminalScope = {
      projectId: terminalOutputTarget.projectId,
      conversationId: terminalOutputTarget.conversationId,
      proposalId: terminalOutputTarget.proposalId,
      scopeGeneration: terminalOutputTarget.scopeGeneration,
      generation: terminalGenerationRef.current,
    };
    const labels = {
      programLabel: terminalOutputTarget.programLabel,
      argv: terminalOutputTarget.argv,
    };

    setTerminalOutput(null);
    setTerminalError(null);
    setTerminalLoading(true);

    void (async () => {
      try {
        const result = await execGateway.openExecOutput({
          conversationId: scope.conversationId,
          proposalId: scope.proposalId,
        });
        if (
          scope.generation !== terminalGenerationRef.current
          || activeProjectIdRef.current !== scope.projectId
          || activeTerminalTargetRef.current?.conversationId !== scope.conversationId
          || activeTerminalTargetRef.current?.proposalId !== scope.proposalId
          || activeTerminalTargetRef.current?.scopeGeneration !== scope.scopeGeneration
        ) {
          return;
        }
        // Map host output + sealed labels; never invent chunks from metadata.
        setTerminalOutput({
          programLabel: labels.programLabel,
          argv: labels.argv,
          state: result.state,
          chunks: result.chunks.map((chunk) => ({
            stream: chunk.stream,
            sequence: chunk.sequence,
            text: chunk.text,
          })),
          liveRetained: result.liveRetained,
          truncated: result.truncated,
          encodingLossy: result.encodingLossy,
          capturedBytes: result.capturedBytes,
          exitCode: result.exitCode,
          durationMs: result.durationMs,
        });
        setTerminalError(null);
        setTerminalLoading(false);
      } catch (error) {
        if (
          scope.generation !== terminalGenerationRef.current
          || activeProjectIdRef.current !== scope.projectId
          || activeTerminalTargetRef.current?.conversationId !== scope.conversationId
          || activeTerminalTargetRef.current?.proposalId !== scope.proposalId
          || activeTerminalTargetRef.current?.scopeGeneration !== scope.scopeGeneration
        ) {
          return;
        }
        const normalized = normalizeExecError(error);
        setTerminalOutput(null);
        setTerminalError(normalized.message);
        setTerminalLoading(false);
      }
    })();
  }, [
    open,
    execGateway,
    projectId,
    terminalOutputTarget?.projectId,
    terminalOutputTarget?.conversationId,
    terminalOutputTarget?.proposalId,
    terminalOutputTarget?.scopeGeneration,
    terminalOutputTarget?.programLabel,
    terminalOutputTarget?.argv,
    execProposalRefreshToken,
  ]);

  if (!open) return null;

  const hasGatewayIntegration = Boolean(resourceGateway && projectId);
  const showTerminalOutput = Boolean(
    terminalOutputTarget && terminalOutputTarget.projectId === projectId,
  );
  const showWriteDiff = Boolean(
    !showTerminalOutput
    && writeDiffTarget
    && writeDiffTarget.projectId === projectId,
  );
  // Changes body only when explicitly selected. Priority:
  // Terminal > governed-write Diff > (explicit Changes | Files/Preview/fallback).
  const showGitChanges = Boolean(
    !showTerminalOutput
    && !showWriteDiff
    && bodyMode === "changes"
    && projectId
    && hasProject,
  );
  const showModeSwitcher = Boolean(
    !showTerminalOutput
    && !showWriteDiff
    && hasProject
    && gitChangesCapable
    && onBodyModeChange,
  );
  // When the mode switcher is present, mode identity lives on the selected tab
  // (WAI-ARIA tabs). The h2 names the pane itself so 资源/变更 are not repeated.
  // Terminal / write-diff still use specific headings (no switcher in those states).
  const heading = showTerminalOutput
    ? "终端输出"
    : showWriteDiff
      ? "写入差异"
      : showModeSwitcher
        ? "上下文"
        : showGitChanges
          ? "变更"
          : "资源";

  return (
    <aside
      aria-label="上下文资源"
      className="resource-pane"
      data-testid="resource-pane"
    >
      <div className="resource-pane__heading">
        <div
          style={{
            display: "flex",
            alignItems: "center",
            gap: 8,
            minWidth: 0,
            flex: "1 1 auto",
          }}
        >
          <h2>{heading}</h2>
          {showModeSwitcher ? (
            <div
              role="tablist"
              aria-label="上下文面板模式"
              data-testid="context-body-mode-switcher"
              style={{
                display: "inline-flex",
                alignItems: "center",
                gap: 4,
                flexShrink: 0,
              }}
            >
              <button
                type="button"
                role="tab"
                aria-selected={bodyMode === "resources"}
                aria-label="资源"
                data-testid="context-body-mode-resources"
                className={
                  bodyMode === "resources"
                    ? "chrome-btn chrome-btn--on"
                    : "chrome-btn"
                }
                onClick={() => onBodyModeChange?.("resources")}
                style={{
                  width: "auto",
                  minWidth: 28,
                  height: 24,
                  padding: "0 8px",
                  fontSize: 12,
                  fontWeight: 600,
                }}
              >
                资源
              </button>
              <button
                type="button"
                role="tab"
                aria-selected={bodyMode === "changes"}
                aria-label="变更"
                data-testid="context-body-mode-changes"
                className={
                  bodyMode === "changes"
                    ? "chrome-btn chrome-btn--on"
                    : "chrome-btn"
                }
                onClick={() => onBodyModeChange?.("changes")}
                style={{
                  width: "auto",
                  minWidth: 28,
                  height: 24,
                  padding: "0 8px",
                  fontSize: 12,
                  fontWeight: 600,
                }}
              >
                变更
              </button>
            </div>
          ) : null}
        </div>
        <button
          aria-label="关闭资源面板"
          className="chrome-btn"
          data-testid="resource-pane-close"
          onClick={onClose}
          type="button"
        >
          <IconClose size={16} />
        </button>
      </div>

      <div className="resource-pane__scroll">
        {showTerminalOutput ? (
          <section
            aria-label="终端输出预览"
            className="resource-pane__preview"
            data-testid="terminal-output-pane"
          >
            {terminalLoading ? (
              <p aria-live="polite" className="resource-pane__status">
                加载输出…
              </p>
            ) : null}
            {terminalError ? (
              <p
                className="resource-pane__status resource-pane__status--error"
                role="alert"
                data-testid="terminal-output-error"
              >
                {terminalError}
              </p>
            ) : null}
            {terminalOutput ? <TerminalOutputView output={terminalOutput} /> : null}
            {!terminalLoading && !terminalError && !terminalOutput ? (
              <p className="resource-pane__status">无输出</p>
            ) : null}
          </section>
        ) : showWriteDiff ? (
          <section
            aria-label="写入差异预览"
            className="resource-pane__preview"
            data-testid="write-diff-pane"
          >
            {writeDiffLoading ? (
              <p aria-live="polite" className="resource-pane__status">
                加载差异…
              </p>
            ) : null}
            {writeDiffError ? (
              <p
                className="resource-pane__status resource-pane__status--error"
                role="alert"
              >
                {writeDiffError}
              </p>
            ) : null}
            {writeDiff ? <WriteDiffPreview diff={writeDiff} /> : null}
            {!writeDiffLoading && !writeDiffError && !writeDiff ? (
              <p className="resource-pane__status">无差异</p>
            ) : null}
          </section>
        ) : showGitChanges && gitChanges ? (
          <section
            aria-label="工作区变更"
            className="resource-pane__preview"
            data-testid="git-changes-pane"
          >
            <GitChangesView
              repositories={gitChanges.repositories}
              selectedRepositoryId={gitChanges.selectedRepositoryId}
              status={gitChanges.status}
              diff={gitChanges.diff}
              loading={gitChanges.loading}
              error={gitChanges.error}
              stale={gitChanges.stale}
              mutating={gitChanges.mutating}
              onSelectRepository={gitChanges.onSelectRepository}
              onSelectPath={gitChanges.onSelectPath}
              onRefresh={gitChanges.onRefresh}
              onStagePath={gitChanges.onStagePath}
              onUnstagePath={gitChanges.onUnstagePath}
              onCommit={gitChanges.onCommit}
              onPush={gitChanges.onPush}
            />
          </section>
        ) : showGitChanges ? (
          <section
            aria-label="工作区变更"
            className="resource-pane__preview"
            data-testid="git-changes-pane"
          >
            <p aria-live="polite" className="resource-pane__status">
              加载变更…
            </p>
          </section>
        ) : hasGatewayIntegration && resourceGateway && projectId ? (
          <>
            <ProjectResourceBrowser
              projectId={projectId}
              gateway={resourceGateway}
              onOpenResource={requestPreview}
              open={true}
              showToggle={false}
            />

            <section
              aria-label="资源预览"
              className="resource-pane__preview"
            >
              {previewLoading ? (
                <p aria-live="polite" className="resource-pane__status">
                  加载预览…
                </p>
              ) : null}
              {previewError ? (
                <p
                  className="resource-pane__status resource-pane__status--error"
                  role="alert"
                >
                  {previewError}
                </p>
              ) : null}
              {preview ? <ResourcePreview preview={preview} /> : null}
              {!previewLoading && !previewError && !preview && selectedPath ? (
                <p className="resource-pane__status">无预览</p>
              ) : null}
              {!previewLoading
              && !previewError
              && !preview
              && !selectedPath ? (
                <p className="resource-pane__status">
                  从上方选择文件以预览内容。
                </p>
              ) : null}
            </section>
          </>
        ) : resource ? (
          <div
            aria-live="polite"
            className="resource-pane__content"
            data-resource-id={resource.resourceId}
          >
            <h3>{resource.label}</h3>
            {resource.path && resource.path !== resource.label ? (
              <code>{resource.path}</code>
            ) : null}
            <span className="resource-pane__kind">
              {resourceKindLabel(resource.kind)}
            </span>
            {resource.contentPreview ? (
              <pre tabIndex={0}>{resource.contentPreview}</pre>
            ) : (
              <p>此引用没有可读内容预览。</p>
            )}
            {resource.truncated ? (
              <p className="resource-pane__truncation">内容预览已截断</p>
            ) : null}
          </div>
        ) : hasProject ? (
          <p>
            在对话中打开文件、目录、搜索结果或产物后，相关内容会显示在这里。
          </p>
        ) : (
          <p>打开项目后，上下文资源会显示在这里。</p>
        )}
      </div>
    </aside>
  );
}

function resourceKindLabel(kind: ResourceReferenceView["kind"]): string {
  switch (kind) {
    case "file":
      return "文件";
    case "directory":
      return "目录";
    case "search_results":
      return "搜索结果";
    case "artifact":
      return "产物";
    case "receipt":
      return "执行凭据";
  }
}
