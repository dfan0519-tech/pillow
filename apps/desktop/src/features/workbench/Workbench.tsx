import { useCallback, useEffect, useRef, useState } from "react";
import type {
  ConversationTimelineView,
  ResourceReferenceView,
  RunView,
} from "../conversations/conversationReducer";
import { isRunActive } from "../conversations/conversationReducer";
import { ConversationTimeline } from "../conversations/ConversationTimeline";
import { WriteApprovalBar } from "../conversations/WriteApprovalBar";
import type { ProjectDto } from "../../lib/projectGateway";
import type { ResourceGateway } from "../../lib/resourceGateway";
import {
  selectOldestPendingProposal,
  stripApprovalGrant,
  type WriteApprovalGateway,
  type WriteDecision,
  type WriteProposalView,
} from "../../lib/writeApprovalGateway";
import { isPortableRelativePath } from "../resources/resourcePaneState";
import { IconPanelRight } from "../../components/icons";
import { Composer } from "./Composer";
import { ResourcePane, type WriteDiffTarget } from "./ResourcePane";

type WorkbenchProps = {
  project: ProjectDto | null;
  conversationId?: string | null;
  conversationTitle?: string | null;
  onOpenProject: () => void;
  onSendTurn: (content: string) => void | Promise<void>;
  onCancelRun: () => void;
  busy?: boolean;
  sending?: boolean;
  sendError?: string | null;
  refreshWarning?: string | null;
  timeline: ConversationTimelineView | null;
  run: RunView | null;
  isMockRuntime?: boolean;
  resourceGateway?: ResourceGateway;
  writeApprovalGateway?: WriteApprovalGateway;
  /**
   * Bumped by App when a write_proposal_* runtime event is accepted for the
   * selected conversation. Triggers a re-list without polling.
   */
  writeProposalRefreshToken?: number;
};

/**
 * Main stage adapted from Grok App welcome / conversation surface.
 * Upstream: src/App.tsx main stage + welcome empty rhythm at
 * cbbbdf17bb8db1588b6707f3f42f6b00397e27f9.
 * Quiet center stage — no marketing dual CTAs or admin header bands.
 */
export function Workbench({
  project,
  conversationId = null,
  conversationTitle = null,
  onOpenProject,
  onSendTurn,
  onCancelRun,
  busy = false,
  sending = false,
  sendError = null,
  refreshWarning = null,
  timeline,
  run,
  isMockRuntime = false,
  resourceGateway,
  writeApprovalGateway,
  writeProposalRefreshToken = 0,
}: WorkbenchProps) {
  const projectId = project?.id ?? null;
  const [resourcePaneScope, setResourcePaneScope] =
    useState<{ projectId: string | null } | null>(null);
  const [resourceSelection, setResourceSelection] =
    useState<{
      projectId: string;
      resource: ResourceReferenceView;
    } | null>(null);
  const [previewPathRequest, setPreviewPathRequest] = useState<{
    projectId: string;
    path: string;
    nonce: number;
  } | null>(null);
  const [writeDiffTarget, setWriteDiffTarget] = useState<WriteDiffTarget | null>(
    null,
  );
  /** Display-safe pending proposal — never carries approvalGrant. */
  const [pendingProposal, setPendingProposal] =
    useState<WriteProposalView | null>(null);
  const [decisionBusy, setDecisionBusy] = useState(false);

  /**
   * Opaque grants live only in this ref map. Never React state, DOM, log,
   * storage, or snapshot. Cleared after decide settles and on scope change.
   */
  const grantByProposalRef = useRef<Map<string, string>>(new Map());
  /**
   * Project + conversation generation guard for list/diff/decision async results.
   */
  const scopeRef = useRef({
    projectId: null as string | null,
    conversationId: null as string | null,
    generation: 0,
  });

  const resourcesOpen = resourcePaneScope?.projectId === projectId
    && projectId !== null;
  const selectedResource =
    resourceSelection?.projectId === projectId
      ? resourceSelection.resource
      : null;
  const activePreviewRequest =
    previewPathRequest?.projectId === projectId
      ? { path: previewPathRequest.path, nonce: previewPathRequest.nonce }
      : null;
  const activeWriteDiff =
    writeDiffTarget
    && writeDiffTarget.projectId === projectId
    && writeDiffTarget.conversationId === conversationId
      ? writeDiffTarget
      : null;
  const hasTimeline = Boolean(
    timeline
    && (
      timeline.messages.length > 0
      || Object.keys(timeline.activitiesByRunId).length > 0
    ),
  );
  const canCancel = isRunActive(run);

  /** Synchronously clear pane scope, selection, and host preview request on close. */
  const closeResourcePane = () => {
    setResourcePaneScope(null);
    setResourceSelection(null);
    setPreviewPathRequest(null);
    setWriteDiffTarget(null);
  };

  const clearWriteScope = useCallback(() => {
    grantByProposalRef.current.clear();
    setPendingProposal(null);
    setDecisionBusy(false);
    setWriteDiffTarget(null);
  }, []);

  const isScopeCurrent = useCallback(
    (generation: number, project: string | null, conversation: string | null) => {
      const scope = scopeRef.current;
      return (
        scope.generation === generation
        && scope.projectId === project
        && scope.conversationId === conversation
      );
    },
    [],
  );

  const relistProposals = useCallback(
    async (
      generation: number,
      project: string | null,
      conversation: string,
    ) => {
      if (!writeApprovalGateway || !project) return;
      try {
        const list = await writeApprovalGateway.listWriteProposals(conversation);
        if (!isScopeCurrent(generation, project, conversation)) return;

        // Authoritative same-scope list: atomically replace grant memory.
        // Only the exact currently selected live pending proposal may retain a grant.
        const selected = selectOldestPendingProposal(list, conversation);
        grantByProposalRef.current.clear();

        if (!selected) {
          setPendingProposal(null);
          setWriteDiffTarget(null);
          return;
        }

        if (selected.approvalGrant) {
          grantByProposalRef.current.set(
            selected.proposalId,
            selected.approvalGrant,
          );
        }

        // If the selected proposal changes, drop a diff belonging to the prior one.
        setWriteDiffTarget((current) => {
          if (
            current
            && (
              current.proposalId !== selected.proposalId
              || current.conversationId !== conversation
              || current.projectId !== project
            )
          ) {
            return null;
          }
          return current;
        });

        setPendingProposal(stripApprovalGrant(selected));
      } catch {
        if (!isScopeCurrent(generation, project, conversation)) return;
        // Never log the caught value — host errors may carry grants/secrets.
        console.error("Failed to list write proposals.");
        grantByProposalRef.current.clear();
        setPendingProposal(null);
        setWriteDiffTarget(null);
      }
    },
    [writeApprovalGateway, isScopeCurrent],
  );

  // Conversation / project entry: re-list and park foreign state. No polling.
  useEffect(() => {
    scopeRef.current = {
      projectId,
      conversationId,
      generation: scopeRef.current.generation + 1,
    };
    const generation = scopeRef.current.generation;
    clearWriteScope();
    if (!projectId || !conversationId || !writeApprovalGateway) {
      return;
    }
    void relistProposals(generation, projectId, conversationId);
  }, [
    projectId,
    conversationId,
    writeApprovalGateway,
    writeProposalRefreshToken,
    clearWriteScope,
    relistProposals,
  ]);

  useEffect(() => {
    if (resourceSelection && resourceSelection.projectId !== projectId) {
      setResourceSelection(null);
    }
    if (resourcePaneScope && resourcePaneScope.projectId !== projectId) {
      setResourcePaneScope(null);
    }
    if (previewPathRequest && previewPathRequest.projectId !== projectId) {
      setPreviewPathRequest(null);
    }
    if (writeDiffTarget && writeDiffTarget.projectId !== projectId) {
      setWriteDiffTarget(null);
    }
  }, [
    projectId,
    resourcePaneScope?.projectId,
    resourceSelection?.projectId,
    previewPathRequest?.projectId,
    writeDiffTarget?.projectId,
  ]);

  const handleDecide = useCallback(
    async (decision: WriteDecision) => {
      if (!writeApprovalGateway || !pendingProposal || !projectId || !conversationId) {
        return;
      }
      const proposalId = pendingProposal.proposalId;
      const grant = grantByProposalRef.current.get(proposalId) ?? null;
      if (!grant) return;

      const generation = scopeRef.current.generation;
      setDecisionBusy(true);
      try {
        await writeApprovalGateway.decideWriteProposal({
          conversationId,
          proposalId,
          approvalGrant: grant,
          decision,
        });
      } catch {
        if (!isScopeCurrent(generation, projectId, conversationId)) return;
        // Never log the caught value — may contain grant/token text.
        console.error("Failed to decide write proposal.");
      } finally {
        // Clear the local grant reference after the native call settles.
        grantByProposalRef.current.delete(proposalId);
        if (isScopeCurrent(generation, projectId, conversationId)) {
          setDecisionBusy(false);
          // Reconcile from durable/native list — never optimistic-only terminal.
          await relistProposals(generation, projectId, conversationId);
        }
      }
    },
    [
      writeApprovalGateway,
      pendingProposal,
      projectId,
      conversationId,
      isScopeCurrent,
      relistProposals,
    ],
  );

  const handleReviewDiff = useCallback(async () => {
    if (!pendingProposal || !projectId || !conversationId) return;
    setWriteDiffTarget({
      projectId,
      conversationId,
      proposalId: pendingProposal.proposalId,
      scopeGeneration: scopeRef.current.generation,
    });
    setResourcePaneScope({ projectId });
    setResourceSelection(null);
    setPreviewPathRequest(null);
  }, [pendingProposal, projectId, conversationId]);

  const handleExpiryDeadline = useCallback(() => {
    const scope = scopeRef.current;
    if (!scope.projectId || !scope.conversationId) return;
    // At the displayed deadline: invalidate governed diff; re-list so native
    // remains the only terminal-state authority.
    setWriteDiffTarget(null);
    void relistProposals(
      scope.generation,
      scope.projectId,
      scope.conversationId,
    );
  }, [relistProposals]);

  const grantForBar =
    pendingProposal
      ? grantByProposalRef.current.get(pendingProposal.proposalId) ?? null
      : null;

  return (
    <main
      className={resourcesOpen ? "workbench has-resources" : "workbench"}
      data-testid="workbench"
    >
      <section className="conversation-surface" aria-label="对话工作台">
        <header className="stage-top">
          <div className="stage-top__title" data-tauri-drag-region>
            {project ? (
              <h1
                data-testid="active-project-name"
                title={conversationTitle ? `${project.name} / ${conversationTitle}` : project.name}
              >
                <span>{project.name}</span>
                {conversationTitle ? (
                  <>
                    <span aria-hidden="true" className="stage-top__separator">/</span>
                    <span
                      className="stage-top__conversation"
                      data-testid="active-conversation-title"
                    >
                      {conversationTitle}
                    </span>
                  </>
                ) : null}
              </h1>
            ) : (
              <h1 data-testid="empty-workbench-title">新对话</h1>
            )}
          </div>
          <div className="stage-top__actions">
            <button
              aria-label={resourcesOpen ? "关闭资源面板" : "打开资源面板"}
              aria-pressed={resourcesOpen}
              className={
                resourcesOpen
                  ? "chrome-btn chrome-btn--on"
                  : "chrome-btn"
              }
              data-testid="resource-toggle"
              disabled={!project}
              onClick={() => {
                if (!project) return;
                if (resourcePaneScope?.projectId === project.id) {
                  closeResourcePane();
                  return;
                }
                setWriteDiffTarget(null);
                setResourcePaneScope({ projectId: project.id });
              }}
              type="button"
            >
              <IconPanelRight size={16} />
            </button>
          </div>
        </header>

        <div className="stage-body">
          {hasTimeline && timeline ? (
            <ConversationTimeline
              onOpenResource={(resource) => {
                if (!project) return;
                setWriteDiffTarget(null);
                setResourceSelection({ projectId: project.id, resource });
                setResourcePaneScope({ projectId: project.id });
                const portable =
                  resource.path && isPortableRelativePath(resource.path)
                    ? resource.path
                    : null;
                // Prefer the host resource gateway when present; otherwise the
                // conversation-reference pane keeps label/preview content.
                if (portable && resourceGateway) {
                  setPreviewPathRequest((current) => ({
                    projectId: project.id,
                    path: portable,
                    nonce: (current?.nonce ?? 0) + 1,
                  }));
                }
              }}
              run={run}
              timeline={timeline}
            />
          ) : project ? (
            <div className="stage-empty" data-testid="selected-workbench">
              <h2>开始新对话</h2>
              <p>描述要完成的工作。运行和协作会保留在这段对话中。</p>
            </div>
          ) : (
            <div className="stage-empty" data-testid="empty-workbench">
              <h2>打开项目</h2>
              <p>打开现有项目文件夹以开始本地工作。</p>
            </div>
          )}
        </div>

        <div className="composer-stack">
          {pendingProposal
          && conversationId
          && pendingProposal.conversationId === conversationId ? (
            <WriteApprovalBar
              approvalGrant={grantForBar}
              disabled={decisionBusy}
              onDecide={handleDecide}
              onExpiryDeadline={handleExpiryDeadline}
              onReviewDiff={handleReviewDiff}
              proposal={pendingProposal}
            />
          ) : null}
          <Composer
            busy={busy}
            canCancel={canCancel}
            disabled={!project}
            onCancelRun={onCancelRun}
            onOpenProject={onOpenProject}
            onSend={onSendTurn}
            projectName={project?.name ?? null}
            runtimeLabel={isMockRuntime ? "模拟" : "本地"}
            refreshWarning={refreshWarning}
            sendError={sendError}
            sending={sending}
          />
        </div>
      </section>
      <ResourcePane
        hasProject={Boolean(project)}
        onClose={closeResourcePane}
        open={resourcesOpen}
        projectId={projectId}
        resource={selectedResource}
        resourceGateway={resourceGateway}
        previewPathRequest={activePreviewRequest}
        writeApprovalGateway={writeApprovalGateway}
        writeDiffTarget={activeWriteDiff}
      />
    </main>
  );
}
