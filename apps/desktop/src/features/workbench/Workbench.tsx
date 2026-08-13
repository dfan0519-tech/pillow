import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import type {
  ConversationTimelineView,
  ResourceReferenceView,
  RunView,
} from "../conversations/conversationReducer";
import { isRunActive } from "../conversations/conversationReducer";
import {
  ConversationTimeline,
  type OpenProcessOutputRequest,
} from "../conversations/ConversationTimeline";
import {
  selectVisibleApproval,
  type ApprovalCandidate,
  type VisibleApproval,
} from "../conversations/approvalArbitration";
import { WriteApprovalBar } from "../conversations/WriteApprovalBar";
import type { ProjectDto } from "../../lib/projectGateway";
import type { ResourceGateway } from "../../lib/resourceGateway";
import {
  isPendingWriteProposal,
  type WriteApprovalGateway,
  type WriteDecision,
  type WriteProposalView,
} from "../../lib/writeApprovalGateway";
import type {
  ExecDecision,
  ExecGateway,
  ExecProposalView,
} from "../../lib/execGateway";
import type { RoleDispatchGateway } from "../../lib/roleDispatchGateway";
import {
  MockPermissionRulesGateway,
  TauriPermissionRulesGateway,
  readRuleSubject,
  type PermissionRulesGateway,
} from "../../lib/permissionRulesGateway";
import type {
  CommandDecision,
  CommandProposalView,
} from "../execution/executionTypes";
import { CommandApprovalBar } from "../execution/CommandApprovalBar";
import { RoleDispatchPanel } from "../conversations/RoleDispatchPanel";
import { isPortableRelativePath } from "../resources/resourcePaneState";
import { IconPanelRight } from "../../components/icons";
import type { PermissionModeLabel } from "../settings/settingsTypes";
import type { RuntimeSlotsGateway } from "../settings/runtimeSlotsTypes";
import { Composer } from "./Composer";
import {
  ResourcePane,
  type ContextBodyMode,
  type GitChangesPaneModel,
  type TerminalOutputTarget,
  type WriteDiffTarget,
} from "./ResourcePane";

/**
 * Approval re-list retry budget.
 *
 * A pending proposal is the LAST event of a turn: the agent blocks waiting for
 * the human, so no further lifecycle event arrives to bump the refresh token.
 * That makes a single transient list failure permanent — the bar clears and
 * nothing ever re-lists, so the run looks hung with no error anywhere. Retry
 * the list, and if it still fails say so instead of presenting a clean empty
 * stage. Delays are short because the human is already waiting on this bar.
 */
const APPROVAL_LIST_ATTEMPTS = 3;
const APPROVAL_LIST_RETRY_DELAYS_MS = [250, 1_000];
/** Used if the delay table is ever shorter than the attempt budget. */
const APPROVAL_LIST_FALLBACK_DELAY_MS = 1_000;

const delay = (ms: number) =>
  new Promise<void>((resolve) => {
    setTimeout(resolve, ms);
  });

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
  /** Optional conversation-owned process gateway (Wave C4-D3B2). */
  execGateway?: ExecGateway;
  /**
   * Bumped by App on accepted process lifecycle events for the selected
   * conversation. Triggers process re-list without polling.
   */
  execProposalRefreshToken?: number;
  /**
   * Optional permission-rules gateway for “always allow/deny this pattern”
   * from approval bars. Defaults to Tauri when the IPC bridge is present;
   * otherwise an in-memory mock (unit tests can inject a fixture).
   */
  permissionRulesGateway?: PermissionRulesGateway;
  /**
   * Host-owned Git Changes presentation model (C4 D3B4). Pass-through only —
   * Workbench never calls the Git gateway or owns discovery generation.
   */
  gitChanges?: GitChangesPaneModel | null;
  /**
   * True when App has a Git gateway. Enables the compact 资源/变更 mode switch;
   * does not select Changes by default.
   */
  gitChangesCapable?: boolean;
  /**
   * Notifies App with the active project id when the contextual pane is open on
   * Changes mode (null when inactive). Discovery runs only for that project.
   */
  onGitChangesActive?: (projectId: string | null) => void;
  /** Installation-wide Pillow approval mode, shown whenever it is non-default. */
  permissionMode?: PermissionModeLabel;
  /** One-click safety escape hatch back to default confirmation behavior. */
  onResetPermissionMode?: () => void;
  /**
   * Role-dispatch gateway (controller → other role). Propose only; approval
   * still uses `execGateway` + CommandApprovalBar — never a second bar.
   */
  roleDispatchGateway?: RoleDispatchGateway;
  /** Project slots for dispatch target picker (controller excluded as target). */
  runtimeSlotsGateway?: RuntimeSlotsGateway;
};

function toCommandProposal(proposal: ExecProposalView): CommandProposalView {
  return {
    proposalId: proposal.proposalId,
    conversationId: proposal.conversationId,
    origin: proposal.origin,
    state: proposal.state,
    programLabel: proposal.programLabel,
    argv: proposal.argv,
    portableCwd: proposal.portableCwd,
    riskClass: proposal.riskClass,
    summary: proposal.summary,
    expiresAt: proposal.expiresAt,
  };
}

function isPendingProcessProposal(
  proposal: Pick<ExecProposalView, "state">,
): boolean {
  return proposal.state === "pending_approval";
}

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
  execGateway,
  execProposalRefreshToken = 0,
  permissionRulesGateway: permissionRulesGatewayProp,
  gitChanges = null,
  gitChangesCapable = false,
  onGitChangesActive,
  permissionMode = "default",
  onResetPermissionMode,
  roleDispatchGateway,
  runtimeSlotsGateway,
}: WorkbenchProps) {
  const projectId = project?.id ?? null;
  const permissionRulesGateway = useMemo(() => {
    if (permissionRulesGatewayProp) return permissionRulesGatewayProp;
    if (
      typeof window !== "undefined"
      && ("__TAURI_INTERNALS__" in window || "__TAURI__" in window)
    ) {
      return new TauriPermissionRulesGateway();
    }
    return new MockPermissionRulesGateway();
  }, [permissionRulesGatewayProp]);
  const [resourcePaneScope, setResourcePaneScope] =
    useState<{ projectId: string | null } | null>(null);
  /** Default resources — Changes is never an implicit body. */
  const [contextBodyMode, setContextBodyMode] =
    useState<ContextBodyMode>("resources");
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
  const [terminalOutputTarget, setTerminalOutputTarget] =
    useState<TerminalOutputTarget | null>(null);
  /** Display-safe pending write proposal — never carries raw grant. */
  const [pendingWriteProposal, setPendingWriteProposal] =
    useState<WriteProposalView | null>(null);
  /** Full process proposal for the selected conversation (host decisionAvailable). */
  const [pendingProcessProposal, setPendingProcessProposal] =
    useState<ExecProposalView | null>(null);
  /** Exactly one visible bar domain after dual-domain arbitration. */
  const [visibleApproval, setVisibleApproval] = useState<VisibleApproval | null>(
    null,
  );
  const [decisionBusy, setDecisionBusy] = useState(false);
  /**
   * The approval list could not be read after every retry. Presented, never
   * silent: an empty stage here would be indistinguishable from "nothing is
   * pending", and a proposal may be durably waiting on the host.
   */
  const [approvalsUnavailable, setApprovalsUnavailable] = useState(false);
  /** Manual retry in flight — the budget spans ~1.25s of silence otherwise. */
  const [approvalsRetrying, setApprovalsRetrying] = useState(false);

  /**
   * Latest listed proposals keyed for bar rendering after arbitration.
   * Not a process cache outside the workbench — cleared on scope change.
   * Write grants stay host-private (EA-03); only decisionAvailable crosses.
   */
  const writeByIdRef = useRef<Map<string, WriteProposalView>>(new Map());
  const processByIdRef = useRef<Map<string, ExecProposalView>>(new Map());
  /**
   * Project + conversation generation guard for list/diff/decision/output async.
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
  const activeTerminalOutput =
    terminalOutputTarget
    && terminalOutputTarget.projectId === projectId
    && terminalOutputTarget.conversationId === conversationId
      ? terminalOutputTarget
      : null;
  const hasTimeline = Boolean(
    timeline
    && (
      timeline.messages.length > 0
      || Object.keys(timeline.activitiesByRunId).length > 0
    ),
  );
  const canCancel = isRunActive(run);

  /** Synchronously clear pane scope and presentation targets on close.
   * Closing never cancels a process. */
  const closeResourcePane = () => {
    setResourcePaneScope(null);
    setResourceSelection(null);
    setPreviewPathRequest(null);
    setWriteDiffTarget(null);
    setTerminalOutputTarget(null);
    setContextBodyMode("resources");
  };

  /**
   * Clear grant maps and bar presentation. Terminal is cleared only on
   * project/conversation identity change so same-scope process refresh tokens
   * can re-pull into an open pane.
   */
  const clearApprovalScope = useCallback(
    (options?: { clearTerminal?: boolean }) => {
      writeByIdRef.current.clear();
      processByIdRef.current.clear();
      setPendingWriteProposal(null);
      setPendingProcessProposal(null);
      setVisibleApproval(null);
      setDecisionBusy(false);
      setApprovalsUnavailable(false);
      setWriteDiffTarget(null);
      if (options?.clearTerminal !== false) {
        setTerminalOutputTarget(null);
      }
    },
    [],
  );

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

  const applyListedApprovals = useCallback(
    (
      project: string,
      conversation: string,
      writeList: WriteProposalView[],
      processList: ExecProposalView[],
    ) => {
      // Authoritative same-scope lists: atomically replace proposal maps.
      writeByIdRef.current.clear();
      processByIdRef.current.clear();

      const candidates: ApprovalCandidate[] = [];

      for (const item of writeList) {
        if (item.conversationId !== conversation) continue;
        if (!isPendingWriteProposal(item)) continue;
        writeByIdRef.current.set(item.proposalId, item);
        // Host-private grant: only the non-authority availability bit is used.
        // Pending without decisionAvailable must not win arbitration over process.
        candidates.push({
          domain: "write",
          projectId: project,
          requestId: item.proposalId,
          conversationId: conversation,
          expiresAt: item.expiresAt,
          decisionAvailable: item.decisionAvailable,
          summary: item.summary,
          risk: item.mode,
        });
      }

      for (const item of processList) {
        if (item.conversationId !== conversation) continue;
        if (!isPendingProcessProposal(item)) continue;
        processByIdRef.current.set(item.proposalId, item);
        candidates.push({
          domain: "process",
          projectId: project,
          requestId: item.proposalId,
          conversationId: conversation,
          expiresAt: item.expiresAt,
          decisionAvailable: item.decisionAvailable,
          summary: item.summary,
          risk: item.riskClass,
        });
      }

      const selected = selectVisibleApproval(
        { projectId: project, conversationId: conversation },
        candidates,
      );
      setVisibleApproval(selected);

      if (!selected) {
        setPendingWriteProposal(null);
        setPendingProcessProposal(null);
        setWriteDiffTarget(null);
        return;
      }

      if (selected.domain === "write") {
        setPendingWriteProposal(
          writeByIdRef.current.get(selected.requestId) ?? null,
        );
        setPendingProcessProposal(null);
        setWriteDiffTarget((current) => {
          if (
            current
            && (
              current.proposalId !== selected.requestId
              || current.conversationId !== conversation
              || current.projectId !== project
            )
          ) {
            return null;
          }
          return current;
        });
        return;
      }

      const process = processByIdRef.current.get(selected.requestId) ?? null;
      setPendingProcessProposal(process);
      setPendingWriteProposal(null);
      setWriteDiffTarget(null);
    },
    [],
  );

  const relistApprovals = useCallback(
    async (
      generation: number,
      project: string | null,
      conversation: string,
    ) => {
      if (!project) return;
      if (!writeApprovalGateway && !execGateway) return;

      for (let attempt = 0; attempt < APPROVAL_LIST_ATTEMPTS; attempt += 1) {
        try {
          const [writeList, processList] = await Promise.all([
            writeApprovalGateway
              ? writeApprovalGateway.listWriteProposals(conversation)
              : Promise.resolve([] as WriteProposalView[]),
            execGateway
              ? execGateway.listExecProposals(conversation)
              : Promise.resolve([] as ExecProposalView[]),
          ]);
          if (!isScopeCurrent(generation, project, conversation)) return;
          applyListedApprovals(project, conversation, writeList, processList);
          setApprovalsUnavailable(false);
          return;
        } catch {
          if (!isScopeCurrent(generation, project, conversation)) return;
          // Exhaustion is decided by the attempt counter, never by "is there a
          // delay left": keying it off the delay table means a shorter table
          // silently falls out of the loop reporting nothing, which is the
          // exact defect this retry exists to remove.
          if (attempt < APPROVAL_LIST_ATTEMPTS - 1) {
            await delay(
              APPROVAL_LIST_RETRY_DELAYS_MS[attempt]
              ?? APPROVAL_LIST_FALLBACK_DELAY_MS,
            );
            // The scope may have moved while we waited.
            if (!isScopeCurrent(generation, project, conversation)) return;
            continue;
          }
          // Never log the caught value — host errors may carry secrets.
          console.error("Failed to list governed approvals.");
          writeByIdRef.current.clear();
          processByIdRef.current.clear();
          setPendingWriteProposal(null);
          setPendingProcessProposal(null);
          setVisibleApproval(null);
          setWriteDiffTarget(null);
          // A proposal may still be pending on the host. Say so — do not let
          // this read as an idle stage.
          setApprovalsUnavailable(true);
        }
      }
    },
    [writeApprovalGateway, execGateway, isScopeCurrent, applyListedApprovals],
  );

  // Conversation / project / refresh-token entry: re-list and park foreign state.
  // No polling — both write and process tokens share one generation-guarded list.
  // Same-scope token bumps preserve an open terminal target for re-pull.
  useEffect(() => {
    const previous = scopeRef.current;
    const identityChanged =
      previous.projectId !== projectId
      || previous.conversationId !== conversationId;

    scopeRef.current = {
      projectId,
      conversationId,
      generation: previous.generation + 1,
    };
    const generation = scopeRef.current.generation;
    // Identity change (or empty scope): drop terminal so foreign targets cannot
    // paint or resurface. Token-only refresh keeps the open terminal target.
    clearApprovalScope({
      clearTerminal: identityChanged || !projectId || !conversationId,
    });
    if (!projectId || !conversationId) {
      return;
    }
    if (!writeApprovalGateway && !execGateway) {
      return;
    }
    void relistApprovals(generation, projectId, conversationId);
  }, [
    projectId,
    conversationId,
    writeApprovalGateway,
    writeProposalRefreshToken,
    execGateway,
    execProposalRefreshToken,
    clearApprovalScope,
    relistApprovals,
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
    if (terminalOutputTarget && terminalOutputTarget.projectId !== projectId) {
      setTerminalOutputTarget(null);
    }
  }, [
    projectId,
    resourcePaneScope?.projectId,
    resourceSelection?.projectId,
    previewPathRequest?.projectId,
    writeDiffTarget?.projectId,
    terminalOutputTarget?.projectId,
  ]);

  // Project identity change: always return to resources mode.
  useEffect(() => {
    setContextBodyMode("resources");
  }, [projectId]);

  // Drive App Git discovery only while Changes mode is explicitly open for
  // this project id (null otherwise — never leave a prior project's request).
  useEffect(() => {
    const requestProjectId =
      resourcesOpen
      && contextBodyMode === "changes"
      && projectId
      && gitChangesCapable
        ? projectId
        : null;
    onGitChangesActive?.(requestProjectId);
  }, [
    resourcesOpen,
    contextBodyMode,
    projectId,
    gitChangesCapable,
    onGitChangesActive,
  ]);

  const handleWriteDecide = useCallback(
    async (decision: WriteDecision) => {
      if (
        !writeApprovalGateway
        || !pendingWriteProposal
        || !projectId
        || !conversationId
        || !pendingWriteProposal.decisionAvailable
      ) {
        return;
      }
      const proposalId = pendingWriteProposal.proposalId;

      const generation = scopeRef.current.generation;
      setDecisionBusy(true);
      try {
        // Host looks up the live grant; renderer sends decision only.
        await writeApprovalGateway.decideWriteProposal({
          conversationId,
          proposalId,
          decision,
        });
      } catch {
        if (!isScopeCurrent(generation, projectId, conversationId)) return;
        // Never log the caught value — may contain secret text.
        console.error("Failed to decide write proposal.");
      } finally {
        if (isScopeCurrent(generation, projectId, conversationId)) {
          setDecisionBusy(false);
          // Reconcile from durable/native list — never optimistic-only terminal.
          await relistApprovals(generation, projectId, conversationId);
        }
      }
    },
    [
      writeApprovalGateway,
      pendingWriteProposal,
      projectId,
      conversationId,
      isScopeCurrent,
      relistApprovals,
    ],
  );

  const handleProcessDecide = useCallback(
    async (decision: CommandDecision) => {
      if (
        !execGateway
        || !pendingProcessProposal
        || !projectId
        || !conversationId
      ) {
        return;
      }
      const proposalId = pendingProcessProposal.proposalId;
      const generation = scopeRef.current.generation;
      setDecisionBusy(true);
      try {
        // Process decisions carry no grant — host retains decision authority.
        const execDecision: ExecDecision = decision;
        await execGateway.decideExecProposal({
          conversationId,
          proposalId,
          decision: execDecision,
        });
      } catch {
        if (!isScopeCurrent(generation, projectId, conversationId)) return;
        console.error("Failed to decide process proposal.");
      } finally {
        if (isScopeCurrent(generation, projectId, conversationId)) {
          setDecisionBusy(false);
          // Native authoritative re-list after every decision.
          await relistApprovals(generation, projectId, conversationId);
        }
      }
    },
    [
      execGateway,
      pendingProcessProposal,
      projectId,
      conversationId,
      isScopeCurrent,
      relistApprovals,
    ],
  );

  const handleReviewDiff = useCallback(async () => {
    if (!pendingWriteProposal || !projectId || !conversationId) return;
    setTerminalOutputTarget(null);
    setWriteDiffTarget({
      projectId,
      conversationId,
      proposalId: pendingWriteProposal.proposalId,
      scopeGeneration: scopeRef.current.generation,
    });
    setResourcePaneScope({ projectId });
    setResourceSelection(null);
    setPreviewPathRequest(null);
  }, [pendingWriteProposal, projectId, conversationId]);

  const handleProcessReview = useCallback(async () => {
    if (!pendingProcessProposal || !projectId || !conversationId) return;
    setWriteDiffTarget(null);
    setTerminalOutputTarget({
      projectId,
      conversationId,
      proposalId: pendingProcessProposal.proposalId,
      programLabel: pendingProcessProposal.programLabel,
      argv: pendingProcessProposal.argv,
      scopeGeneration: scopeRef.current.generation,
    });
    setResourcePaneScope({ projectId });
    setResourceSelection(null);
    setPreviewPathRequest(null);
  }, [pendingProcessProposal, projectId, conversationId]);

  /**
   * Timeline command-activity “完整输出”: open the already-accepted Terminal
   * output target in ResourcePane. Pull-on-demand via ExecGateway only.
   */
  const handleOpenProcessOutput = useCallback(
    (request: OpenProcessOutputRequest) => {
      if (!projectId || !conversationId) return;
      setWriteDiffTarget(null);
      setTerminalOutputTarget({
        projectId,
        conversationId,
        proposalId: request.proposalId,
        programLabel: request.programLabel,
        argv: request.argv,
        scopeGeneration: scopeRef.current.generation,
      });
      setResourcePaneScope({ projectId });
      setResourceSelection(null);
      setPreviewPathRequest(null);
    },
    [projectId, conversationId],
  );

  const handleExpiryDeadline = useCallback(() => {
    const scope = scopeRef.current;
    if (!scope.projectId || !scope.conversationId) return;
    // At the displayed deadline: invalidate governed surfaces; re-list so native
    // remains the only terminal-state authority.
    setWriteDiffTarget(null);
    void relistApprovals(
      scope.generation,
      scope.projectId,
      scope.conversationId,
    );
  }, [relistApprovals]);

  const handleRetryApprovals = useCallback(async () => {
    const scope = scopeRef.current;
    if (!scope.projectId || !scope.conversationId) return;
    setApprovalsRetrying(true);
    try {
      await relistApprovals(
        scope.generation,
        scope.projectId,
        scope.conversationId,
      );
    } finally {
      setApprovalsRetrying(false);
    }
  }, [relistApprovals]);

  /**
   * After role-dispatch propose: re-list process proposals so the **existing**
   * CommandApprovalBar appears. Never invent a second approve surface.
   */
  const handleRoleDispatchProposed = useCallback(() => {
    const scope = scopeRef.current;
    if (!scope.projectId || !scope.conversationId) return;
    void relistApprovals(
      scope.generation,
      scope.projectId,
      scope.conversationId,
    );
  }, [relistApprovals]);

  const showWriteBar = Boolean(
    visibleApproval
    && visibleApproval.domain === "write"
    && pendingWriteProposal
    && conversationId
    && pendingWriteProposal.conversationId === conversationId
    && visibleApproval.requestId === pendingWriteProposal.proposalId,
  );
  const showProcessBar = Boolean(
    visibleApproval
    && visibleApproval.domain === "process"
    && pendingProcessProposal
    && conversationId
    && pendingProcessProposal.conversationId === conversationId
    && visibleApproval.requestId === pendingProcessProposal.proposalId,
  );

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
            {permissionMode !== "default" ? (
              <div
                className={`permission-mode-indicator permission-mode-indicator--${permissionMode}`}
                data-testid="permission-mode-indicator"
                role="status"
                title="deny 规则仍然优先；自动批准仍会记录提案、执行与收据。"
              >
                <span>
                  权限：{permissionMode === "bypass" ? "全开" : "接受改动"}
                </span>
                <button
                  data-testid="permission-mode-reset"
                  onClick={onResetPermissionMode}
                  type="button"
                >
                  恢复默认
                </button>
              </div>
            ) : null}
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
                setTerminalOutputTarget(null);
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
              onOpenProcessOutput={handleOpenProcessOutput}
              onOpenResource={(resource) => {
                if (!project) return;
                setWriteDiffTarget(null);
                setTerminalOutputTarget(null);
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
          {approvalsUnavailable ? (
            <div
              className="approvals-unavailable"
              data-testid="approvals-unavailable"
              role="alert"
            >
              <div className="approvals-unavailable__text">
                <strong>无法读取待批准项</strong>
                <span>
                  如果代理正在等待你批准,这里暂时看不到。请重试;重试无效时重新打开会话。
                </span>
              </div>
              <button
                className="approvals-unavailable__retry"
                disabled={approvalsRetrying}
                onClick={() => {
                  void handleRetryApprovals();
                }}
                type="button"
              >
                {approvalsRetrying ? "重试中…" : "重试"}
              </button>
            </div>
          ) : null}
          {showWriteBar && pendingWriteProposal ? (
            <WriteApprovalBar
              decisionAvailable={pendingWriteProposal.decisionAvailable}
              disabled={decisionBusy}
              onDecide={handleWriteDecide}
              onExpiryDeadline={handleExpiryDeadline}
              onReviewDiff={handleReviewDiff}
              permissionRulesGateway={permissionRulesGateway}
              projectId={projectId}
              proposal={pendingWriteProposal}
              ruleSubject={
                readRuleSubject(pendingWriteProposal)
                ?? pendingWriteProposal.path
              }
              writeApprovalGateway={writeApprovalGateway}
            />
          ) : null}
          {showProcessBar && pendingProcessProposal ? (
            <CommandApprovalBar
              decisionAvailable={pendingProcessProposal.decisionAvailable}
              disabled={decisionBusy}
              onDecide={handleProcessDecide}
              onExpiryDeadline={handleExpiryDeadline}
              onReview={handleProcessReview}
              permissionRulesGateway={permissionRulesGateway}
              projectId={projectId}
              proposal={toCommandProposal(pendingProcessProposal)}
              ruleSegments={pendingProcessProposal.ruleSegments}
              ruleSubject={readRuleSubject(pendingProcessProposal)}
            />
          ) : null}
          {roleDispatchGateway && project && conversationId ? (
            <RoleDispatchPanel
              conversationId={conversationId}
              disabled={busy || sending || decisionBusy}
              onProposed={handleRoleDispatchProposed}
              projectId={projectId}
              refreshToken={execProposalRefreshToken}
              roleDispatchGateway={roleDispatchGateway}
              runtimeSlotsGateway={runtimeSlotsGateway}
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
        execGateway={execGateway}
        terminalOutputTarget={activeTerminalOutput}
        execProposalRefreshToken={execProposalRefreshToken}
        gitChanges={gitChanges}
        gitChangesCapable={gitChangesCapable}
        bodyMode={contextBodyMode}
        onBodyModeChange={setContextBodyMode}
      />
    </main>
  );
}
