import { useCallback, useEffect, useRef, useState } from "react";
import { IconClose } from "../../components/icons";
import type { ResourceReferenceView } from "../conversations/conversationReducer";
import { ResourcePane as ProjectResourceBrowser } from "../resources/ResourcePane";
import { ResourcePreview } from "../resources/ResourcePreview";
import { isPortableRelativePath } from "../resources/resourcePaneState";
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
import { WriteDiffPreview } from "./WriteDiffPreview";

export type WriteDiffTarget = {
  projectId: string;
  conversationId: string;
  proposalId: string;
  scopeGeneration: number;
};

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

type ResourcePaneProps = {
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
}: ResourcePaneProps) {
  const [preview, setPreview] = useState<ResourcePreviewDto | null>(null);
  const [previewError, setPreviewError] = useState<string | null>(null);
  const [previewLoading, setPreviewLoading] = useState(false);
  const [selectedPath, setSelectedPath] = useState<string | null>(null);
  const [writeDiff, setWriteDiff] = useState<WriteDiffView | null>(null);
  const [writeDiffError, setWriteDiffError] = useState<string | null>(null);
  const [writeDiffLoading, setWriteDiffLoading] = useState(false);

  const generationRef = useRef(0);
  const diffGenerationRef = useRef(0);
  const activeProjectIdRef = useRef<string | null>(projectId);
  activeProjectIdRef.current = projectId;
  const activeDiffTargetRef = useRef<WriteDiffTarget | null>(writeDiffTarget);
  activeDiffTargetRef.current = writeDiffTarget;

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

  const invalidatePreviewScope = useCallback(() => {
    generationRef.current += 1;
    clearPreviewState();
  }, [clearPreviewState]);

  const invalidateDiffScope = useCallback(() => {
    diffGenerationRef.current += 1;
    clearWriteDiffState();
  }, [clearWriteDiffState]);

  // Project change or unmount: discard every in-flight preview/diff before render.
  useEffect(() => {
    invalidatePreviewScope();
    invalidateDiffScope();
    return () => {
      generationRef.current += 1;
      diffGenerationRef.current += 1;
    };
  }, [projectId, invalidatePreviewScope, invalidateDiffScope]);

  // Closing the panel must not leave a stale preview/diff to resurrect on reopen.
  useEffect(() => {
    if (!open) {
      invalidatePreviewScope();
      invalidateDiffScope();
    }
  }, [open, invalidatePreviewScope, invalidateDiffScope]);

  // Conversation / proposal target change: drop prior diff results.
  useEffect(() => {
    invalidateDiffScope();
  }, [
    writeDiffTarget?.conversationId,
    writeDiffTarget?.proposalId,
    writeDiffTarget?.scopeGeneration,
    invalidateDiffScope,
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

  if (!open) return null;

  const hasGatewayIntegration = Boolean(resourceGateway && projectId);
  const showWriteDiff = Boolean(writeDiffTarget && writeDiffTarget.projectId === projectId);

  return (
    <aside
      aria-label="上下文资源"
      className="resource-pane"
      data-testid="resource-pane"
    >
      <div className="resource-pane__heading">
        <h2>{showWriteDiff ? "写入差异" : "资源"}</h2>
        <button
          aria-label="关闭资源面板"
          className="chrome-btn"
          onClick={onClose}
          type="button"
        >
          <IconClose size={16} />
        </button>
      </div>

      <div className="resource-pane__scroll">
        {showWriteDiff ? (
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
