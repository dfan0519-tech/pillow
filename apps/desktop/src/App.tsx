import {
  useCallback,
  useEffect,
  useLayoutEffect,
  useMemo,
  useReducer,
  useRef,
  useState,
} from "react";
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
import { FirstRunOnboarding } from "./features/onboarding/FirstRunOnboarding";
import { shouldShowFirstRunOnboarding } from "./features/onboarding/shouldShowFirstRunOnboarding";
import { AppSidebar } from "./features/projects/AppSidebar";
import {
  GROK_CLI_LOGIN_GUIDANCE,
  GROK_CLI_LOGIN_TITLE,
  isAuthenticationFailedEvent,
  userFacingSendError,
} from "./features/settings/loginGuidance";
import {
  MockSettingsGateway,
  TauriSettingsGateway,
} from "./features/settings/settingsGateway";
import { SettingsPanel } from "./features/settings/SettingsPanel";
import {
  applyDocumentTheme,
  parseUiThemeLabel,
  type PermissionModeLabel,
  type RuntimeStartupView,
  type SettingsGateway,
} from "./features/settings/settingsTypes";
import type { GitChangesPaneModel } from "./features/workbench/ResourcePane";
import { Workbench } from "./features/workbench/Workbench";
import type { ConversationGateway } from "./lib/conversationGateway";
import type { ExecGateway } from "./lib/execGateway";
import {
  GIT_ERROR_MESSAGES,
  normalizeGitError,
  type GitDiffView,
  type GitGateway,
  type GitRepositoryView,
  type GitStatusView,
} from "./lib/gitGateway";
import { MockConversationGateway } from "./lib/mockConversationGateway";
import {
  normalizeProjectGatewayError,
  type CommandError,
  type ProjectDto,
  type ProjectGateway,
} from "./lib/projectGateway";
import type { ResourceGateway } from "./lib/resourceGateway";
import type { WriteApprovalGateway } from "./lib/writeApprovalGateway";

type AppProps = {
  gateway: ProjectGateway;
  /** Defaults to the deterministic mock gateway (Task 3). Task 4 swaps Tauri. */
  conversationGateway?: ConversationGateway;
  /** Optional project-scoped resource gateway (Wave C2-D workbench mount). */
  resourceGateway?: ResourceGateway;
  /** Optional conversation-owned write approval gateway (Wave C3-E). */
  writeApprovalGateway?: WriteApprovalGateway;
  /** Optional conversation-owned process gateway (Wave C4-D3B2). */
  execGateway?: ExecGateway;
  /**
   * Optional read-only Git gateway (C4 D3B4). When omitted, App never calls Git
   * and passes `gitChanges={null}` so existing resource/diff/terminal modes stay.
   */
  gitGateway?: GitGateway;
  /** S0 settings gateway. Defaults to in-memory mock; desktop main injects Tauri. */
  settingsGateway?: SettingsGateway;
  /** Kept for desktop smoke readiness; fires after the shell project list loads. */
  onProjectLauncherReady?: () => Promise<void>;
  /** Optional project id to select after the initial list loads (screenshot harness). */
  defaultSelectedProjectId?: string;
};

type GitPresentationState = {
  repositories: GitRepositoryView[];
  selectedRepositoryId: string | null;
  /** Native discovery generation for status/diff expects; null before discover. */
  discoveryGeneration: number | null;
  status: GitStatusView | null;
  diff: GitDiffView | null;
  loading: boolean;
  error: string | null;
  stale: boolean;
  /** True while a structured mutation is in flight (blocks repeat submit). */
  mutating: boolean;
};

const EMPTY_GIT_STATE: GitPresentationState = {
  repositories: [],
  selectedRepositoryId: null,
  discoveryGeneration: null,
  status: null,
  diff: null,
  loading: false,
  error: null,
  stale: false,
  mutating: false,
};

/** Prefer project-root `.` when available; otherwise the first available id. */
function pickFirstAvailableRepository(
  repositories: GitRepositoryView[],
): string | null {
  const root = repositories.find(
    (repo) => repo.repositoryId === "." && repo.available,
  );
  if (root) return root.repositoryId;
  const first = repositories.find((repo) => repo.available);
  return first?.repositoryId ?? null;
}

function isStaleGitCode(code: string): boolean {
  return code === "git_generation_stale" || code === "git_generation_mismatch";
}

/**
 * Shell regions adapt Grok App's App.tsx layout under MIT.
 * Upstream: src/App.tsx at cbbbdf17bb8db1588b6707f3f42f6b00397e27f9.
 * Always launches into the full workbench — never a full-screen project launcher.
 */
export function App({
  gateway,
  conversationGateway: conversationGatewayProp,
  resourceGateway,
  writeApprovalGateway,
  execGateway,
  gitGateway,
  settingsGateway: settingsGatewayProp,
  onProjectLauncherReady,
  defaultSelectedProjectId,
}: AppProps) {
  const conversationGateway = useMemo(
    () => conversationGatewayProp ?? new MockConversationGateway({ stepDelayMs: 35 }),
    [conversationGatewayProp],
  );
  const settingsGateway = useMemo(() => {
    if (settingsGatewayProp) return settingsGatewayProp;
    // Desktop shell: use Tauri when the IPC bridge is present (no main.tsx
    // dependency for S0b mount). Tests / story renderers stay on the mock.
    if (
      typeof window !== "undefined"
      && ("__TAURI_INTERNALS__" in window || "__TAURI__" in window)
    ) {
      return new TauriSettingsGateway();
    }
    return new MockSettingsGateway();
  }, [settingsGatewayProp]);
  const isMockRuntime = conversationGateway instanceof MockConversationGateway;

  const [projects, setProjects] = useState<ProjectDto[]>([]);
  const [activeProject, setActiveProject] = useState<ProjectDto | null>(null);
  const [loading, setLoading] = useState(true);
  const [shellReady, setShellReady] = useState(false);
  const [settingsOpen, setSettingsOpen] = useState(false);
  const [permissionMode, setPermissionMode] =
    useState<PermissionModeLabel>("default");
  /** Runtime config degrade (bad Grok path etc.) — shell up, conversation off. */
  const [runtimeStartup, setRuntimeStartup] = useState<RuntimeStartupView | null>(
    null,
  );
  /**
   * Conversation-path login guidance (adapter health class authentication_failed).
   * Independent of degraded startup; never reads auth files — only failure phrases.
   */
  const [loginGuidanceVisible, setLoginGuidanceVisible] = useState(false);
  /**
   * First-run dismiss flag from settings (`ui.onboarding.dismissed`).
   * null = not yet resolved (do not flash guidance).
   */
  const [onboardingDismissed, setOnboardingDismissed] = useState<boolean | null>(
    null,
  );
  const [busy, setBusy] = useState(false);
  const [sending, setSending] = useState(false);
  const [sendError, setSendError] = useState<string | null>(null);
  const [refreshWarning, setRefreshWarning] = useState<string | null>(null);
  const [error, setError] = useState<CommandError | null>(null);
  /**
   * Bumped only on accepted write_proposal_* events for the selected
   * conversation — drives Workbench re-list without polling.
   */
  const [writeProposalRefreshToken, setWriteProposalRefreshToken] = useState(0);
  /**
   * Bumped only on accepted process lifecycle events for the selected
   * conversation under the active project — drives Workbench re-list without
   * polling or a process state cache.
   */
  const [execProposalRefreshToken, setExecProposalRefreshToken] = useState(0);
  /** App-owned Git Changes presentation for the active project (C4 D3B4). */
  const [gitState, setGitState] = useState<GitPresentationState>(EMPTY_GIT_STATE);
  /**
   * Project id for which Workbench has the pane open on explicit Changes mode.
   * Must match `activeProject.id` before discovery/status run. Project selection
   * alone never activates Git.
   */
  const [gitChangesRequestProjectId, setGitChangesRequestProjectId] = useState<
    string | null
  >(null);
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
  /**
   * Per-project monotonic counter for listConversations freshness.
   * Allocated when a list request starts; carried on `conversations_loaded`
   * so the reducer can drop strictly older out-of-order results.
   */
  const conversationListSeqCounterRef = useRef<Record<string, number>>({});
  /**
   * Independent activation generation: last-request-wins for select/open.
   * Only the current generation may commit project, error, or busy from async select.
   */
  const projectActivationGenRef = useRef(0);
  const activeProjectIdRef = useRef<string | null>(null);
  activeProjectIdRef.current = activeProject?.id ?? null;
  /**
   * Controller-owned Git request/scope generation. Bumps on project switch,
   * clear, refresh, repository selection, and path selection so late async
   * results cannot mutate visible state.
   */
  const gitScopeRef = useRef({
    projectId: null as string | null,
    generation: 0,
  });
  const gitStateRef = useRef(gitState);
  gitStateRef.current = gitState;

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

  /**
   * Theme must land before first meaningful paint (no dark→light flash).
   * Resolve via the settings gateway (env → global → default), apply
   * `data-theme` + `data-theme-ready` in layout effect. CSS hides the document
   * until `data-theme-ready` is set (see tokens.css).
   */
  useLayoutEffect(() => {
    let cancelled = false;
    void settingsGateway.getThemeSettings(null).then(
      (view) => {
        if (cancelled) return;
        applyDocumentTheme(parseUiThemeLabel(view.theme.value));
      },
      () => {
        if (cancelled) return;
        // Fail closed to product default so the shell still becomes visible.
        applyDocumentTheme("dark");
      },
    );
    return () => {
      cancelled = true;
    };
  }, [settingsGateway]);

  // Global permission mode is a persistent safety status, so resolve it even
  // while Settings is closed. Read failures fail closed in the renderer too.
  useEffect(() => {
    let cancelled = false;
    void settingsGateway.getPermissionMode().then(
      (view) => {
        if (!cancelled) setPermissionMode(view.mode);
      },
      () => {
        if (!cancelled) setPermissionMode("default");
      },
    );
    return () => {
      cancelled = true;
    };
  }, [settingsGateway]);

  // Degraded runtime (bad path / release stored Fake): surface banner + keep
  // Settings reachable. Conversation stays unavailable until restart after fix.
  useEffect(() => {
    if (!settingsGateway.getRuntimeStartup) {
      setRuntimeStartup({
        conversationAvailable: true,
        selection: null,
        degradedCode: null,
        degradedMessage: null,
        degradedGuidance: null,
      });
      return;
    }
    let cancelled = false;
    void settingsGateway.getRuntimeStartup().then(
      (status) => {
        if (cancelled) return;
        setRuntimeStartup(status);
      },
      () => {
        if (cancelled) return;
        // Missing status command: assume available (unit mocks without the method).
        setRuntimeStartup({
          conversationAvailable: true,
          selection: null,
          degradedCode: null,
          degradedMessage: null,
          degradedGuidance: null,
        });
      },
    );
    return () => {
      cancelled = true;
    };
  }, [settingsGateway]);

  // First-run dismiss flag — settings layer only (no second store / no env).
  useEffect(() => {
    let cancelled = false;
    void settingsGateway.getOnboardingSettings().then(
      (view) => {
        if (cancelled) return;
        setOnboardingDismissed(view.dismissed);
      },
      () => {
        if (cancelled) return;
        // Fail closed to "not dismissed" only when read fails? Prefer hide to
        // avoid nagging when storage is broken; still allow shell use.
        setOnboardingDismissed(true);
      },
    );
    return () => {
      cancelled = true;
    };
  }, [settingsGateway]);

  useEffect(() => {
    if (!shellReady || readinessSent.current || !onProjectLauncherReady) return;
    readinessSent.current = true;
    void onProjectLauncherReady().catch((cause: unknown) => {
      console.error("Pillow readiness command failed.", cause);
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
      // Event-driven re-list only; never interval polling.
      // Only the conversation owned by the currently active project may bump.
      const selectedConversationId =
        conversationStateRef.current.selectedConversationId;
      const ownedByActiveProject = conversationBelongsToProject(
        conversationStateRef.current,
        event.conversationId,
        activeProjectIdRef.current,
      );
      const targetsSelectedConversation =
        event.conversationId === selectedConversationId && ownedByActiveProject;

      if (
        (
          event.kind === "write_proposal_requested"
          || event.kind === "write_proposal_resolved"
        )
        && targetsSelectedConversation
      ) {
        setWriteProposalRefreshToken((token) => token + 1);
      }

      if (
        (
          event.kind === "process_proposal_requested"
          || event.kind === "process_proposal_decided"
          || event.kind === "process_started"
          || event.kind === "process_terminal_outcome"
        )
        && targetsSelectedConversation
      ) {
        setExecProposalRefreshToken((token) => token + 1);
      }

      // Auth-class turn failure: surface fixed CLI login guidance (not raw codes).
      // Phrase/code match only — never open auth files or show account identity.
      if (event.kind === "turn_failed" && targetsSelectedConversation) {
        if (isAuthenticationFailedEvent(event.payload)) {
          setSendError(GROK_CLI_LOGIN_GUIDANCE);
          setLoginGuidanceVisible(true);
        }
      }
      if (event.kind === "turn_completed" && targetsSelectedConversation) {
        setLoginGuidanceVisible(false);
      }
    });
  }, [conversationGateway]);

  useEffect(() => {
    if (!activeProject) {
      dispatchConversation({ type: "project_cleared" });
      return;
    }
    let active = true;
    const projectId = activeProject.id;
    const listSeq = nextConversationListSeq(projectId);
    void conversationGateway.listConversations(projectId).then(
      (conversations) => {
        if (!active) return;
        dispatchConversation({
          type: "conversations_loaded",
          projectId,
          conversations,
          listSeq,
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

  const isGitScopeCurrent = useCallback(
    (projectId: string, generation: number): boolean => {
      return (
        gitScopeRef.current.generation === generation
        && gitScopeRef.current.projectId === projectId
        && activeProjectIdRef.current === projectId
      );
    },
    [],
  );

  const loadGitStatus = useCallback(
    async (
      projectId: string,
      scopeGeneration: number,
      repositoryId: string,
      discoveryGeneration: number,
    ) => {
      if (!gitGateway) return;
      try {
        const status = await gitGateway.getGitStatus({
          projectId,
          expected: {
            repositoryId,
            generation: discoveryGeneration,
          },
        });
        if (!isGitScopeCurrent(projectId, scopeGeneration)) return;
        setGitState((current) => ({
          ...current,
          selectedRepositoryId: repositoryId,
          discoveryGeneration,
          status,
          diff: null,
          loading: false,
          error: null,
          stale: false,
        }));
      } catch (cause) {
        if (!isGitScopeCurrent(projectId, scopeGeneration)) return;
        const normalized = normalizeGitError(cause);
        setGitState((current) => ({
          ...current,
          selectedRepositoryId: repositoryId,
          discoveryGeneration,
          status: null,
          diff: null,
          loading: false,
          error: normalized.message,
          stale: isStaleGitCode(normalized.code),
        }));
      }
    },
    [gitGateway, isGitScopeCurrent],
  );

  const runGitDiscovery = useCallback(
    async (projectId: string, scopeGeneration: number) => {
      if (!gitGateway) return;
      try {
        const discovery = await gitGateway.discoverGitRepositories(projectId);
        if (!isGitScopeCurrent(projectId, scopeGeneration)) return;

        const repositories = discovery.repositories;
        const selectedId = pickFirstAvailableRepository(repositories);

        if (!selectedId) {
          setGitState({
            repositories,
            selectedRepositoryId: null,
            discoveryGeneration: discovery.generation,
            status: null,
            diff: null,
            loading: false,
            error: GIT_ERROR_MESSAGES.git_repository_not_found,
            stale: false,
            mutating: false,
          });
          return;
        }

        setGitState({
          repositories,
          selectedRepositoryId: selectedId,
          discoveryGeneration: discovery.generation,
          status: null,
          diff: null,
          loading: true,
          error: null,
          stale: false,
          mutating: false,
        });

        await loadGitStatus(
          projectId,
          scopeGeneration,
          selectedId,
          discovery.generation,
        );
      } catch (cause) {
        if (!isGitScopeCurrent(projectId, scopeGeneration)) return;
        const normalized = normalizeGitError(cause);
        setGitState({
          repositories: [],
          selectedRepositoryId: null,
          discoveryGeneration: null,
          status: null,
          diff: null,
          loading: false,
          error: normalized.message,
          stale: isStaleGitCode(normalized.code),
          mutating: false,
        });
      }
    },
    [gitGateway, isGitScopeCurrent, loadGitStatus],
  );

  // Project identity / gateway / explicit Changes activation: clear
  // synchronously, discover only when Changes mode is active for the same
  // project id. A stale request for a prior project never starts discovery.
  useEffect(() => {
    const projectId = activeProject?.id ?? null;
    gitScopeRef.current = {
      projectId,
      generation: gitScopeRef.current.generation + 1,
    };
    const generation = gitScopeRef.current.generation;
    const changesActiveForProject =
      projectId != null
      && gitChangesRequestProjectId === projectId;

    if (!projectId || !gitGateway || !changesActiveForProject) {
      setGitState(EMPTY_GIT_STATE);
      return;
    }

    setGitState({
      ...EMPTY_GIT_STATE,
      loading: true,
    });
    void runGitDiscovery(projectId, generation);
  }, [
    activeProject?.id,
    gitGateway,
    gitChangesRequestProjectId,
    runGitDiscovery,
  ]);

  const handleGitSelectRepository = useCallback(
    (repositoryId: string) => {
      const projectId = activeProjectIdRef.current;
      const current = gitStateRef.current;
      if (!projectId || !gitGateway) return;
      if (current.discoveryGeneration === null) return;

      const allowed = current.repositories.find(
        (repo) => repo.repositoryId === repositoryId && repo.available,
      );
      if (!allowed) return;
      if (repositoryId === current.selectedRepositoryId && current.status) {
        return;
      }

      gitScopeRef.current = {
        projectId,
        generation: gitScopeRef.current.generation + 1,
      };
      const generation = gitScopeRef.current.generation;
      const discoveryGeneration = current.discoveryGeneration;

      setGitState((prev) => ({
        ...prev,
        selectedRepositoryId: repositoryId,
        status: null,
        diff: null,
        loading: true,
        error: null,
        stale: false,
      }));

      void loadGitStatus(
        projectId,
        generation,
        repositoryId,
        discoveryGeneration,
      );
    },
    [gitGateway, loadGitStatus],
  );

  const handleGitSelectPath = useCallback(
    (portablePath: string) => {
      const projectId = activeProjectIdRef.current;
      const current = gitStateRef.current;
      if (!projectId || !gitGateway) return;
      if (!current.selectedRepositoryId || current.discoveryGeneration === null) {
        return;
      }
      if (!current.status?.files.some((file) => file.path === portablePath)) {
        return;
      }

      gitScopeRef.current = {
        projectId,
        generation: gitScopeRef.current.generation + 1,
      };
      const generation = gitScopeRef.current.generation;
      const repositoryId = current.selectedRepositoryId;
      const discoveryGeneration = current.discoveryGeneration;

      setGitState((prev) => ({
        ...prev,
        diff: null,
        error: null,
        stale: false,
      }));

      void (async () => {
        try {
          const diff = await gitGateway.openGitDiff({
            projectId,
            expected: {
              repositoryId,
              generation: discoveryGeneration,
            },
            path: portablePath,
          });
          if (!isGitScopeCurrent(projectId, generation)) return;
          setGitState((prev) => ({
            ...prev,
            diff,
            error: null,
            stale: false,
          }));
        } catch (cause) {
          if (!isGitScopeCurrent(projectId, generation)) return;
          const normalized = normalizeGitError(cause);
          setGitState((prev) => ({
            ...prev,
            diff: null,
            error: normalized.message,
            stale: isStaleGitCode(normalized.code),
          }));
        }
      })();
    },
    [gitGateway, isGitScopeCurrent],
  );

  const handleGitRefresh = useCallback(() => {
    const projectId = activeProjectIdRef.current;
    if (!projectId || !gitGateway) return;
    if (gitStateRef.current.mutating) return;

    gitScopeRef.current = {
      projectId,
      generation: gitScopeRef.current.generation + 1,
    };
    const generation = gitScopeRef.current.generation;

    setGitState({
      repositories: [],
      selectedRepositoryId: null,
      discoveryGeneration: null,
      status: null,
      diff: null,
      loading: true,
      error: null,
      stale: false,
      mutating: false,
    });
    void runGitDiscovery(projectId, generation);
  }, [gitGateway, runGitDiscovery]);

  const runGitMutation = useCallback(
    async (
      projectId: string,
      scopeGeneration: number,
      repositoryId: string,
      discoveryGeneration: number,
      operation: () => Promise<unknown>,
    ) => {
      if (!gitGateway) return;
      if (gitStateRef.current.mutating) return;

      setGitState((prev) => ({
        ...prev,
        mutating: true,
        error: null,
        stale: false,
      }));

      try {
        await operation();
        if (!isGitScopeCurrent(projectId, scopeGeneration)) return;
        await loadGitStatus(
          projectId,
          scopeGeneration,
          repositoryId,
          discoveryGeneration,
        );
        if (!isGitScopeCurrent(projectId, scopeGeneration)) return;
        setGitState((prev) => ({
          ...prev,
          mutating: false,
        }));
      } catch (cause) {
        if (!isGitScopeCurrent(projectId, scopeGeneration)) return;
        const normalized = normalizeGitError(cause);
        setGitState((prev) => ({
          ...prev,
          mutating: false,
          error: normalized.message,
          stale: isStaleGitCode(normalized.code),
        }));
      }
    },
    [gitGateway, isGitScopeCurrent, loadGitStatus],
  );

  const handleGitStagePath = useCallback(
    (portablePath: string) => {
      const projectId = activeProjectIdRef.current;
      const current = gitStateRef.current;
      if (!projectId || !gitGateway) return;
      if (!current.selectedRepositoryId || current.discoveryGeneration === null) {
        return;
      }
      if (current.mutating) return;
      if (!current.status?.files.some((file) => file.path === portablePath)) {
        return;
      }

      const repositoryId = current.selectedRepositoryId;
      const discoveryGeneration = current.discoveryGeneration;
      const scopeGeneration = gitScopeRef.current.generation;

      void runGitMutation(
        projectId,
        scopeGeneration,
        repositoryId,
        discoveryGeneration,
        () =>
          gitGateway.stageGitPaths({
            projectId,
            expected: {
              repositoryId,
              generation: discoveryGeneration,
            },
            paths: [portablePath],
          }),
      );
    },
    [gitGateway, runGitMutation],
  );

  const handleGitUnstagePath = useCallback(
    (portablePath: string) => {
      const projectId = activeProjectIdRef.current;
      const current = gitStateRef.current;
      if (!projectId || !gitGateway) return;
      if (!current.selectedRepositoryId || current.discoveryGeneration === null) {
        return;
      }
      if (current.mutating) return;
      if (!current.status?.files.some((file) => file.path === portablePath)) {
        return;
      }

      const repositoryId = current.selectedRepositoryId;
      const discoveryGeneration = current.discoveryGeneration;
      const scopeGeneration = gitScopeRef.current.generation;

      void runGitMutation(
        projectId,
        scopeGeneration,
        repositoryId,
        discoveryGeneration,
        () =>
          gitGateway.unstageGitPaths({
            projectId,
            expected: {
              repositoryId,
              generation: discoveryGeneration,
            },
            paths: [portablePath],
          }),
      );
    },
    [gitGateway, runGitMutation],
  );

  const handleGitCommit = useCallback(
    (message: string) => {
      const projectId = activeProjectIdRef.current;
      const current = gitStateRef.current;
      if (!projectId || !gitGateway) return;
      if (!current.selectedRepositoryId || current.discoveryGeneration === null) {
        return;
      }
      if (current.mutating) return;
      if (message.trim().length === 0) return;

      const repositoryId = current.selectedRepositoryId;
      const discoveryGeneration = current.discoveryGeneration;
      const scopeGeneration = gitScopeRef.current.generation;

      void runGitMutation(
        projectId,
        scopeGeneration,
        repositoryId,
        discoveryGeneration,
        () =>
          gitGateway.commitGitChanges({
            projectId,
            expected: {
              repositoryId,
              generation: discoveryGeneration,
            },
            message,
          }),
      );
    },
    [gitGateway, runGitMutation],
  );

  const handleGitPush = useCallback(
    (payload: { remoteName: string; branchName: string }) => {
      const projectId = activeProjectIdRef.current;
      const current = gitStateRef.current;
      if (!projectId || !gitGateway) return;
      if (!current.selectedRepositoryId || current.discoveryGeneration === null) {
        return;
      }
      if (current.mutating) return;

      const repositoryId = current.selectedRepositoryId;
      const discoveryGeneration = current.discoveryGeneration;
      const scopeGeneration = gitScopeRef.current.generation;

      void runGitMutation(
        projectId,
        scopeGeneration,
        repositoryId,
        discoveryGeneration,
        () =>
          gitGateway.pushGitBranch({
            projectId,
            expected: {
              repositoryId,
              generation: discoveryGeneration,
            },
            confirmation: {
              confirmed: true,
              remoteName: payload.remoteName,
              branchName: payload.branchName,
            },
          }),
      );
    },
    [gitGateway, runGitMutation],
  );

  /**
   * Render-time ownership guard: the project-change effect updates
   * `gitScopeRef` and clears `gitState` only after React commits the render
   * that introduced a new `activeProject`. Until that effect establishes the
   * new scope (and loading state), pass `null` so the prior project's
   * repositories/status/diff cannot paint for one frame under the new project.
   * Model is also null while Changes mode is inactive so resource body is
   * never displaced by a non-empty Git presentation.
   */
  const gitChanges: GitChangesPaneModel | null = useMemo(() => {
    if (!activeProject || !gitGateway) return null;
    if (gitChangesRequestProjectId !== activeProject.id) return null;
    if (gitScopeRef.current.projectId !== activeProject.id) return null;
    return {
      repositories: gitState.repositories,
      selectedRepositoryId: gitState.selectedRepositoryId,
      status: gitState.status,
      diff: gitState.diff,
      loading: gitState.loading,
      error: gitState.error,
      stale: gitState.stale,
      mutating: gitState.mutating,
      onSelectRepository: handleGitSelectRepository,
      onSelectPath: handleGitSelectPath,
      onRefresh: handleGitRefresh,
      onStagePath: handleGitStagePath,
      onUnstagePath: handleGitUnstagePath,
      onCommit: handleGitCommit,
      onPush: handleGitPush,
    };
  }, [
    activeProject,
    gitGateway,
    gitChangesRequestProjectId,
    gitState.repositories,
    gitState.selectedRepositoryId,
    gitState.status,
    gitState.diff,
    gitState.loading,
    gitState.error,
    gitState.stale,
    gitState.mutating,
    handleGitSelectRepository,
    handleGitSelectPath,
    handleGitRefresh,
    handleGitStagePath,
    handleGitUnstagePath,
    handleGitCommit,
    handleGitPush,
  ]);

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
    setLoginGuidanceVisible(false);
    setRefreshWarning(null);
  }

  function isScopeCurrent(projectId: string, generation: number): boolean {
    return (
      projectScopeGenRef.current === generation
      && activeProjectIdRef.current === projectId
    );
  }

  /** Allocate the next monotonic list-load sequence for a project. */
  function nextConversationListSeq(projectId: string): number {
    const next = (conversationListSeqCounterRef.current[projectId] ?? 0) + 1;
    conversationListSeqCounterRef.current[projectId] = next;
    return next;
  }

  /**
   * Final activation for an already-resolved project DTO.
   * When `generation` is supplied (open/register path), reuse that capture —
   * do not allocate a newer generation after the async gateway work.
   */
  async function activateProject(project: ProjectDto, generation?: number) {
    const gen = generation ?? ++projectActivationGenRef.current;
    if (gen !== projectActivationGenRef.current) return;
    clearConversationScope();
    try {
      const selected = await gateway.selectProject(project.id);
      if (gen !== projectActivationGenRef.current) return;
      upsertProject(selected);
      setActiveProject(selected);
      setError(null);
    } catch (cause) {
      // Stale failure: swallow so openProject cannot paint a superseded error.
      if (gen !== projectActivationGenRef.current) return;
      throw cause;
    }
  }

  async function selectProject(projectId: string) {
    const generation = ++projectActivationGenRef.current;
    setBusy(true);
    try {
      clearConversationScope();
      const project = await gateway.selectProject(projectId);
      if (generation !== projectActivationGenRef.current) return;
      upsertProject(project);
      setActiveProject(project);
      setError(null);
    } catch (cause) {
      if (generation !== projectActivationGenRef.current) return;
      setError(normalizeProjectGatewayError(cause));
    } finally {
      if (generation === projectActivationGenRef.current) {
        setBusy(false);
      }
    }
  }

  async function openProject() {
    // Capture before pickDirectory / openOrRegister — those are not authority.
    const generation = ++projectActivationGenRef.current;
    setBusy(true);
    try {
      const path = await gateway.pickDirectory();
      // Cancel: preserve active project. Stale or current, no further work.
      if (!path) return;
      // Stale after picker: do not register — registration is a durable mutation.
      // Keep the separate post-open/register check below for in-flight register races.
      if (generation !== projectActivationGenRef.current) return;
      const project = await gateway.openOrRegisterProject(path);
      if (generation !== projectActivationGenRef.current) return;
      await activateProject(project, generation);
      // Completing the primary first-run action: durable dismiss via settings.
      if (onboardingDismissed === false) {
        void dismissFirstRunOnboarding();
      }
    } catch (cause) {
      if (generation !== projectActivationGenRef.current) return;
      setError(normalizeProjectGatewayError(cause));
    } finally {
      if (generation === projectActivationGenRef.current) {
        setBusy(false);
      }
    }
  }

  /**
   * Persist first-run dismiss through the settings gateway
   * (`ui.onboarding.dismissed`). Optimistic UI hide; settings is authority.
   */
  async function dismissFirstRunOnboarding() {
    setOnboardingDismissed(true);
    try {
      const view = await settingsGateway.setOnboardingDismissed(true);
      setOnboardingDismissed(view.dismissed);
    } catch (cause) {
      console.error("Failed to persist onboarding dismiss.", cause);
      // Keep hidden for this session; next launch may re-show if write failed.
    }
  }

  async function resetPermissionMode() {
    try {
      const view = await settingsGateway.setPermissionMode("default");
      setPermissionMode(view.mode);
    } catch (cause) {
      console.error("Failed to restore the default permission mode.", cause);
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
    // Reject foreign selection at the command boundary — only conversations
    // that belong to the currently active project may become selected.
    if (
      !conversationBelongsToProject(
        conversationStateRef.current,
        conversationId,
        activeProjectIdRef.current,
      )
    ) {
      return;
    }
    const projectIdAtSelect = activeProjectIdRef.current;
    dispatchConversation({ type: "conversation_selected", conversationId });
    try {
      // Authority snapshot; in-flight events continue via subscribe and merge
      // by (runId, sequence) so background streams are not lost.
      const timeline = await conversationGateway.loadTimeline(conversationId);
      // Drop late timeline commits if the user left the project mid-load.
      if (activeProjectIdRef.current !== projectIdAtSelect) {
        return;
      }
      if (
        !conversationBelongsToProject(
          conversationStateRef.current,
          conversationId,
          activeProjectIdRef.current,
        )
      ) {
        return;
      }
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
    setLoginGuidanceVisible(false);
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
      // Refresh both durable projections. The gateway may rename a persisted
      // "新对话" from the first turn, so the rail and stage title must follow.
      // Async auth failures arrive later via turn_failed (subscription above).
      await conversationGateway.sendTurn(conversationId, content);

      // Stamp list freshness at request start (same contract as project effect).
      const listSeq = nextConversationListSeq(projectIdAtStart);
      const [timelineResult, conversationsResult] = await Promise.allSettled([
        conversationGateway.loadTimeline(conversationId),
        conversationGateway.listConversations(projectIdAtStart),
      ]);
      if (isScopeCurrent(projectIdAtStart, generationAtStart)) {
        if (timelineResult.status === "fulfilled") {
          const timeline = timelineResult.value;
          dispatchConversation({ type: "timeline_loaded", timeline });
        } else {
          console.error(
            "Failed to refresh timeline after accepted send.",
            timelineResult.reason,
          );
          setRefreshWarning("消息已发送，但时间线刷新失败。流式更新仍会继续。");
        }

        if (conversationsResult.status === "fulfilled") {
          dispatchConversation({
            type: "conversations_loaded",
            projectId: projectIdAtStart,
            conversations: conversationsResult.value,
            listSeq,
          });
        } else {
          console.error(
            "Failed to refresh conversation titles after accepted send.",
            conversationsResult.reason,
          );
        }
      }
    } catch (cause) {
      // Project switched while this send was in flight: never write send-error
      // into the new project or reject so Composer restores the old draft there.
      if (!isScopeCurrent(projectIdAtStart, generationAtStart)) {
        return;
      }
      const raw =
        cause instanceof Error ? cause.message : "发送失败，请重试。";
      const message = userFacingSendError(raw);
      setSendError(message);
      if (message === GROK_CLI_LOGIN_GUIDANCE) {
        setLoginGuidanceVisible(true);
      }
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
  /**
   * Render-time ownership proof: Workbench may only receive a conversation,
   * timeline, run, or write surface when the selected conversation belongs to
   * the currently active project. Raw global selectedConversationId alone is
   * not authority — a stale/late A selection under B must not paint.
   */
  const selectionOwnedByActiveProject = conversationBelongsToProject(
    conversationState,
    conversationState.selectedConversationId,
    activeProject?.id ?? null,
  );
  const ownedConversationId = selectionOwnedByActiveProject
    ? conversationState.selectedConversationId
    : null;
  const activeConversation = ownedConversationId
    ? projectConversations.find(
      (conversation) => conversation.id === ownedConversationId,
    ) ?? null
    : null;
  const activeTimeline = selectionOwnedByActiveProject
    ? selectActiveTimeline(conversationState)
    : null;
  const activeRun = selectionOwnedByActiveProject
    ? selectActiveRun(conversationState)
    : null;
  if (loading) {
    return <main className="app-loading">正在加载工作台…</main>;
  }

  const showFirstRunOnboarding = shouldShowFirstRunOnboarding({
    dismissed: onboardingDismissed,
    projectsCount: projects.length,
    runtimeStartup,
    shellReady,
  });

  return (
    <div className="app-shell">
      <WindowControls />
      <button
        aria-label="打开设置"
        className="settings-open-btn"
        data-testid="nav-open-settings"
        onClick={() => setSettingsOpen(true)}
        type="button"
      >
        设置
      </button>
      {runtimeStartup
      && !runtimeStartup.conversationAvailable
      && runtimeStartup.degradedCode ? (
        <div
          className="runtime-degraded-banner"
          data-testid="runtime-degraded-banner"
          role="alert"
        >
          <div className="runtime-degraded-banner__body">
            <strong>对话运行时不可用</strong>
            <p>
              {runtimeStartup.degradedGuidance
                ?? "请打开设置检查 Grok 路径与运行时适配器，修正后重启 Pillow。"}
            </p>
            {runtimeStartup.degradedMessage ? (
              <p className="runtime-degraded-banner__detail">
                {runtimeStartup.degradedMessage}
              </p>
            ) : null}
          </div>
          <button
            data-testid="runtime-degraded-open-settings"
            onClick={() => setSettingsOpen(true)}
            type="button"
          >
            打开设置
          </button>
        </div>
      ) : null}
      {/* Never-configured stranger guidance — not an error, not degraded. */}
      {showFirstRunOnboarding ? (
        <FirstRunOnboarding
          busy={busy}
          onDismiss={() => void dismissFirstRunOnboarding()}
          onOpenProject={() => void openProject()}
          onOpenSettings={() => setSettingsOpen(true)}
        />
      ) : null}
      {/* Login guidance only when conversation runtime is up (not degraded).
          Degraded path points at Settings/paths; login path points at grok login. */}
      {loginGuidanceVisible
      && !(
        runtimeStartup
        && !runtimeStartup.conversationAvailable
        && runtimeStartup.degradedCode
      ) ? (
        <div
          className="runtime-login-guidance-banner"
          data-testid="runtime-login-guidance-banner"
          role="status"
        >
          <div className="runtime-login-guidance-banner__body">
            <strong>{GROK_CLI_LOGIN_TITLE}</strong>
            <p>{GROK_CLI_LOGIN_GUIDANCE}</p>
          </div>
          <button
            data-testid="runtime-login-guidance-open-settings"
            onClick={() => setSettingsOpen(true)}
            type="button"
          >
            打开设置
          </button>
        </div>
      ) : null}
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
          selectedConversationId={ownedConversationId}
        />
        <Workbench
          busy={busy}
          onCancelRun={() => void cancelActiveRun()}
          onOpenProject={() => void openProject()}
          onSendTurn={sendTurn}
          project={activeProject}
          conversationId={ownedConversationId}
          conversationTitle={activeConversation?.title ?? null}
          refreshWarning={refreshWarning}
          run={activeRun}
          sendError={sendError}
          sending={sending}
          isMockRuntime={isMockRuntime}
          timeline={activeTimeline}
          resourceGateway={resourceGateway}
          writeApprovalGateway={writeApprovalGateway}
          writeProposalRefreshToken={writeProposalRefreshToken}
          execGateway={execGateway}
          execProposalRefreshToken={execProposalRefreshToken}
          gitChanges={gitChanges}
          gitChangesCapable={Boolean(gitGateway)}
          onGitChangesActive={setGitChangesRequestProjectId}
          permissionMode={permissionMode}
          onResetPermissionMode={() => void resetPermissionMode()}
        />
      </div>
      {settingsOpen ? (
        <SettingsPanel
          gateway={settingsGateway}
          onClose={() => setSettingsOpen(false)}
          onThemeChanged={applyDocumentTheme}
          onPermissionModeChanged={setPermissionMode}
          projectId={activeProject?.id ?? null}
        />
      ) : null}
    </div>
  );
}
