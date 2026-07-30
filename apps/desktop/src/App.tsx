import { useEffect, useMemo, useReducer, useRef, useState } from "react";
import { WindowControls } from "./components/WindowControls";
import {
  conversationBelongsToProject,
  conversationReducer,
  createInitialConversationState,
  isRunActive,
  selectActiveRun,
  selectActiveTimeline,
  selectConversationsForProject,
} from "./features/conversations/conversationReducer";
import { AppSidebar } from "./features/projects/AppSidebar";
import { Workbench } from "./features/workbench/Workbench";
import type { ConversationGateway } from "./lib/conversationGateway";
import { MockConversationGateway, MOCK_RUNTIME_BADGE } from "./lib/mockConversationGateway";
import {
  normalizeProjectGatewayError,
  type CommandError,
  type ProjectDto,
  type ProjectGateway,
} from "./lib/projectGateway";

type AppProps = {
  gateway: ProjectGateway;
  /** Defaults to the deterministic mock gateway (Task 3). Task 4 swaps Tauri. */
  conversationGateway?: ConversationGateway;
  /** Kept for desktop smoke readiness; fires after the shell project list loads. */
  onProjectLauncherReady?: () => Promise<void>;
  /** Optional project id to select after the initial list loads (screenshot harness). */
  defaultSelectedProjectId?: string;
};

/**
 * Shell regions adapt Grok App's App.tsx layout under MIT.
 * Upstream: src/App.tsx at cbbbdf17bb8db1588b6707f3f42f6b00397e27f9.
 * Always launches into the full workbench — never a full-screen project launcher.
 */
export function App({
  gateway,
  conversationGateway: conversationGatewayProp,
  onProjectLauncherReady,
  defaultSelectedProjectId,
}: AppProps) {
  const conversationGateway = useMemo(
    () => conversationGatewayProp ?? new MockConversationGateway({ stepDelayMs: 35 }),
    [conversationGatewayProp],
  );
  const isMockRuntime = conversationGateway instanceof MockConversationGateway;

  const [projects, setProjects] = useState<ProjectDto[]>([]);
  const [activeProject, setActiveProject] = useState<ProjectDto | null>(null);
  const [loading, setLoading] = useState(true);
  const [shellReady, setShellReady] = useState(false);
  const [busy, setBusy] = useState(false);
  const [sending, setSending] = useState(false);
  const [sendError, setSendError] = useState<string | null>(null);
  const [refreshWarning, setRefreshWarning] = useState<string | null>(null);
  const [error, setError] = useState<CommandError | null>(null);
  const [conversationState, dispatchConversation] = useReducer(
    conversationReducer,
    undefined,
    createInitialConversationState,
  );
  const readinessSent = useRef(false);
  const defaultSelectSent = useRef(false);
  const conversationStateRef = useRef(conversationState);
  conversationStateRef.current = conversationState;
  /** Bumps on every project-scope clear so in-flight create/send cannot select stale results. */
  const projectScopeGenRef = useRef(0);
  const activeProjectIdRef = useRef<string | null>(null);
  activeProjectIdRef.current = activeProject?.id ?? null;

  useEffect(() => {
    let active = true;
    void gateway.listProjects().then(
      (result) => {
        if (!active) return;
        setProjects(result);
        setShellReady(true);
        setLoading(false);
      },
      (cause: unknown) => {
        if (!active) return;
        setError(normalizeProjectGatewayError(cause));
        setLoading(false);
      },
    );
    return () => {
      active = false;
    };
  }, [gateway]);

  useEffect(() => {
    if (!shellReady || readinessSent.current || !onProjectLauncherReady) return;
    readinessSent.current = true;
    void onProjectLauncherReady().catch((cause: unknown) => {
      console.error("Keeper IDE readiness command failed.", cause);
    });
  }, [shellReady, onProjectLauncherReady]);

  useEffect(() => {
    if (
      !shellReady
      || !defaultSelectedProjectId
      || defaultSelectSent.current
    ) {
      return;
    }
    defaultSelectSent.current = true;
    void selectProject(defaultSelectedProjectId);
  }, [shellReady, defaultSelectedProjectId]);

  // Global event subscription — streams continue regardless of selection.
  useEffect(() => {
    return conversationGateway.subscribe((event) => {
      dispatchConversation({ type: "runtime_event", event });
    });
  }, [conversationGateway]);

  useEffect(() => {
    if (!activeProject) {
      dispatchConversation({ type: "project_cleared" });
      return;
    }
    let active = true;
    void conversationGateway.listConversations(activeProject.id).then(
      (conversations) => {
        if (!active) return;
        dispatchConversation({
          type: "conversations_loaded",
          projectId: activeProject.id,
          conversations,
        });
      },
      (cause: unknown) => {
        if (!active) return;
        console.error("Failed to list conversations.", cause);
      },
    );
    return () => {
      active = false;
    };
  }, [activeProject, conversationGateway]);

  function upsertProject(project: ProjectDto) {
    setProjects((current) => {
      const existing = current.some((item) => item.id === project.id);
      return existing
        ? current.map((item) => (item.id === project.id ? project : item))
        : [...current, project];
    });
  }

  /**
   * Every activation path clears conversation selection before the new project
   * becomes active so sendTurn cannot target a foreign conversation.
   */
  function clearConversationScope() {
    projectScopeGenRef.current += 1;
    dispatchConversation({ type: "conversation_selected", conversationId: null });
    setSendError(null);
    setRefreshWarning(null);
  }

  function isScopeCurrent(projectId: string, generation: number): boolean {
    return (
      projectScopeGenRef.current === generation
      && activeProjectIdRef.current === projectId
    );
  }

  async function activateProject(project: ProjectDto) {
    clearConversationScope();
    const selected = await gateway.selectProject(project.id);
    upsertProject(selected);
    setActiveProject(selected);
    setError(null);
  }

  async function selectProject(projectId: string) {
    setBusy(true);
    try {
      clearConversationScope();
      const project = await gateway.selectProject(projectId);
      upsertProject(project);
      setActiveProject(project);
      setError(null);
    } catch (cause) {
      setError(normalizeProjectGatewayError(cause));
    } finally {
      setBusy(false);
    }
  }

  async function openProject() {
    setBusy(true);
    try {
      const path = await gateway.pickDirectory();
      if (!path) return;
      const project = await gateway.openOrRegisterProject(path);
      await activateProject(project);
    } catch (cause) {
      setError(normalizeProjectGatewayError(cause));
    } finally {
      setBusy(false);
    }
  }

  async function createConversation() {
    if (!activeProject) return;
    const projectId = activeProject.id;
    const generation = projectScopeGenRef.current;
    setBusy(true);
    try {
      const conversation = await conversationGateway.createConversation(projectId);
      const stillCurrent = isScopeCurrent(projectId, generation);
      dispatchConversation({
        type: "conversation_created",
        conversation,
        select: stillCurrent,
      });
      if (!stillCurrent) return;
      dispatchConversation({
        type: "timeline_loaded",
        timeline: {
          conversationId: conversation.id,
          messages: [],
          events: [],
          runs: [],
        },
      });
    } catch (cause) {
      console.error("Failed to create conversation.", cause);
    } finally {
      setBusy(false);
    }
  }

  async function selectConversation(conversationId: string) {
    dispatchConversation({ type: "conversation_selected", conversationId });
    try {
      // Authority snapshot; in-flight events continue via subscribe and merge
      // by (runId, sequence) so background streams are not lost.
      const timeline = await conversationGateway.loadTimeline(conversationId);
      dispatchConversation({ type: "timeline_loaded", timeline });
    } catch (cause) {
      console.error("Failed to load timeline.", cause);
    }
  }

  async function sendTurn(content: string): Promise<void> {
    if (!activeProject) {
      throw new Error("No active project.");
    }
    const projectIdAtStart = activeProject.id;
    const generationAtStart = projectScopeGenRef.current;

    setSending(true);
    setSendError(null);
    setRefreshWarning(null);

    const state = conversationStateRef.current;
    let conversationId = state.selectedConversationId;
    let conversationProjectId: string | null = null;

    if (conversationId) {
      for (const [projectId, list] of Object.entries(state.conversationsByProject)) {
        if (list.some((item) => item.id === conversationId)) {
          conversationProjectId = projectId;
          break;
        }
      }
    }

    // Never send against a conversation owned by another project.
    if (conversationId && conversationProjectId !== projectIdAtStart) {
      conversationId = null;
      dispatchConversation({ type: "conversation_selected", conversationId: null });
    }

    try {
      if (!conversationId) {
        const conversation = await conversationGateway.createConversation(
          projectIdAtStart,
          content.slice(0, 32),
        );
        const stillCurrent = isScopeCurrent(projectIdAtStart, generationAtStart);
        dispatchConversation({
          type: "conversation_created",
          conversation,
          select: stillCurrent,
        });
        // Stale scope: conversation is registered under the owning project but
        // must not select, send, surface an error on the new project, or restore
        // the old draft via Composer rejection.
        if (!stillCurrent) {
          return;
        }
        conversationId = conversation.id;
      } else if (
        !conversationBelongsToProject(
          conversationStateRef.current,
          conversationId,
          projectIdAtStart,
        )
      ) {
        throw new Error("Conversation is not in the active project.");
      }

      if (!isScopeCurrent(projectIdAtStart, generationAtStart)) {
        return;
      }

      // Accepted send: run id means the turn is durable on the gateway.
      // Timeline refresh failures must not look like send failures.
      await conversationGateway.sendTurn(conversationId, content);

      try {
        const timeline = await conversationGateway.loadTimeline(conversationId);
        if (isScopeCurrent(projectIdAtStart, generationAtStart)) {
          dispatchConversation({ type: "timeline_loaded", timeline });
        }
      } catch (cause) {
        console.error("Failed to refresh timeline after accepted send.", cause);
        if (isScopeCurrent(projectIdAtStart, generationAtStart)) {
          setRefreshWarning("消息已发送，但时间线刷新失败。流式更新仍会继续。");
        }
      }
    } catch (cause) {
      // Project switched while this send was in flight: never write send-error
      // into the new project or reject so Composer restores the old draft there.
      if (!isScopeCurrent(projectIdAtStart, generationAtStart)) {
        return;
      }
      const message =
        cause instanceof Error ? cause.message : "发送失败，请重试。";
      setSendError(message);
      console.error("Failed to send turn.", cause);
      throw cause instanceof Error ? cause : new Error(message);
    } finally {
      setSending(false);
    }
  }

  async function cancelActiveRun() {
    const run = selectActiveRun(conversationState);
    if (!run || !isRunActive(run)) return;
    try {
      await conversationGateway.cancelRun(run.runId);
    } catch (cause) {
      console.error("Failed to cancel run.", cause);
    }
  }

  const projectConversations = selectConversationsForProject(
    conversationState,
    activeProject?.id ?? null,
  );
  const activeTimeline = selectActiveTimeline(conversationState);
  const activeRun = selectActiveRun(conversationState);
  // Mock badge is visible whenever the mock gateway is active and a project is selected,
  // including before the first send — it must not imply real Grok connectivity.
  const showMockBadge = Boolean(isMockRuntime && activeProject);

  if (loading) {
    return <main className="app-loading">正在加载工作台…</main>;
  }

  return (
    <div className="app-shell">
      <header className="titlebar" data-tauri-drag-region>
        <div className="brand" data-tauri-drag-region>
          Keeper IDE
        </div>
        <WindowControls />
      </header>
      {error ? (
        <p className="app-error" role="alert">
          <span>{error.message}</span>
          <code>{error.code}</code>
        </p>
      ) : null}
      <div className="project-shell">
        <AppSidebar
          activeProjectId={activeProject?.id ?? null}
          busy={busy}
          conversations={projectConversations}
          onNewConversation={() => void createConversation()}
          onOpenProject={() => void openProject()}
          onSelect={(id) => void selectProject(id)}
          onSelectConversation={(id) => void selectConversation(id)}
          projects={projects}
          selectedConversationId={conversationState.selectedConversationId}
        />
        <Workbench
          busy={busy}
          mockBadgeLabel={MOCK_RUNTIME_BADGE}
          onCancelRun={() => void cancelActiveRun()}
          onOpenProject={() => void openProject()}
          onSendTurn={sendTurn}
          project={activeProject}
          refreshWarning={refreshWarning}
          run={activeRun}
          sendError={sendError}
          sending={sending}
          showMockBadge={showMockBadge}
          timeline={activeTimeline}
        />
      </div>
    </div>
  );
}
