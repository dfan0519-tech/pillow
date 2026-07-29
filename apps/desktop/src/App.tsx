import { useEffect, useRef, useState } from "react";
import { WindowControls } from "./components/WindowControls";
import { AppSidebar } from "./features/projects/AppSidebar";
import { NewProjectDialog } from "./features/projects/NewProjectDialog";
import { Workbench } from "./features/workbench/Workbench";
import {
  normalizeProjectGatewayError,
  type CommandError,
  type ProjectDto,
  type ProjectGateway,
} from "./lib/projectGateway";

type AppProps = {
  gateway: ProjectGateway;
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
  onProjectLauncherReady,
  defaultSelectedProjectId,
}: AppProps) {
  const [projects, setProjects] = useState<ProjectDto[]>([]);
  const [activeProject, setActiveProject] = useState<ProjectDto | null>(null);
  const [loading, setLoading] = useState(true);
  const [shellReady, setShellReady] = useState(false);
  const [busy, setBusy] = useState(false);
  const [newProjectOpen, setNewProjectOpen] = useState(false);
  const [error, setError] = useState<CommandError | null>(null);
  const readinessSent = useRef(false);
  const defaultSelectSent = useRef(false);

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

  function upsertProject(project: ProjectDto) {
    setProjects((current) => {
      const existing = current.some((item) => item.id === project.id);
      return existing
        ? current.map((item) => (item.id === project.id ? project : item))
        : [...current, project];
    });
  }

  async function activateProject(project: ProjectDto) {
    const selected = await gateway.selectProject(project.id);
    upsertProject(selected);
    setActiveProject(selected);
    setError(null);
  }

  async function selectProject(projectId: string) {
    setBusy(true);
    try {
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

  async function confirmNewProject(name: string): Promise<boolean> {
    setBusy(true);
    try {
      const parentPath = await gateway.pickDirectory();
      if (!parentPath) return false;
      const project = await gateway.createProjectWithFolder({
        name,
        parentPath,
      });
      await activateProject(project);
      setNewProjectOpen(false);
      return true;
    } catch (cause) {
      setError(normalizeProjectGatewayError(cause));
      return false;
    } finally {
      setBusy(false);
    }
  }

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
          onNewProject={() => setNewProjectOpen(true)}
          onOpenProject={() => void openProject()}
          onSelect={(id) => void selectProject(id)}
          projects={projects}
        />
        <Workbench
          busy={busy}
          onNewProject={() => setNewProjectOpen(true)}
          onOpenProject={() => void openProject()}
          project={activeProject}
        />
      </div>
      <NewProjectDialog
        busy={busy}
        onCancel={() => setNewProjectOpen(false)}
        onConfirm={confirmNewProject}
        open={newProjectOpen}
      />
    </div>
  );
}
