import { useEffect, useRef, useState } from "react";
import { WindowControls } from "./components/WindowControls";
import { ProjectLauncher } from "./features/projects/ProjectLauncher";
import { ProjectSidebar } from "./features/projects/ProjectSidebar";
import { Workbench } from "./features/workbench/Workbench";
import {
  normalizeProjectGatewayError,
  type CommandError,
  type CreateProjectInput,
  type ProjectDto,
  type ProjectGateway,
} from "./lib/projectGateway";

type AppProps = {
  gateway: ProjectGateway;
  onProjectLauncherReady?: () => Promise<void>;
};

/**
 * Shell regions and adjustable navigation adapt Grok App's App.tsx layout under MIT.
 * Source: src/App.tsx at 1c3f9c8aa3a42e869f11afb3d075f1b52843aaa9.
 */
export function App({ gateway, onProjectLauncherReady }: AppProps) {
  const [projects, setProjects] = useState<ProjectDto[]>([]);
  const [activeProject, setActiveProject] = useState<ProjectDto | null>(null);
  const [launcherOpen, setLauncherOpen] = useState(true);
  const [loading, setLoading] = useState(true);
  const [launcherLoaded, setLauncherLoaded] = useState(false);
  const [error, setError] = useState<CommandError | null>(null);
  const readinessSent = useRef(false);

  useEffect(() => {
    let active = true;
    void gateway.listProjects().then(
      (result) => {
        if (!active) return;
        setProjects(result);
        setLauncherLoaded(true);
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
    if (!launcherLoaded || readinessSent.current || !onProjectLauncherReady) return;
    readinessSent.current = true;
    void onProjectLauncherReady().catch((cause: unknown) => {
      console.error("Keeper IDE readiness command failed.", cause);
    });
  }, [launcherLoaded, onProjectLauncherReady]);

  async function selectProject(projectId: string) {
    try {
      const project = await gateway.selectProject(projectId);
      setActiveProject(project);
      setLauncherOpen(false);
      setError(null);
    } catch (cause) {
      setError(normalizeProjectGatewayError(cause));
    }
  }

  async function createProject(input: CreateProjectInput) {
    try {
      const createdProject = await gateway.createProject(input);
      const selectedProject = await gateway.selectProject(createdProject.id);
      setProjects((current) => {
        const existing = current.some((project) => project.id === selectedProject.id);
        return existing
          ? current.map((project) => (
              project.id === selectedProject.id ? selectedProject : project
            ))
          : [...current, selectedProject];
      });
      setActiveProject(selectedProject);
      setLauncherOpen(false);
      setError(null);
      return true;
    } catch (cause) {
      setError(normalizeProjectGatewayError(cause));
      return false;
    }
  }

  if (loading) return <main className="app-loading">正在加载项目…</main>;

  return (
    <div className="app-shell">
      <header className="titlebar" data-tauri-drag-region>
        <button
          aria-label="打开项目启动器"
          className="brand"
          onClick={() => setLauncherOpen(true)}
          type="button"
        >
          Keeper IDE
        </button>
        <WindowControls />
      </header>
      {error ? (
        <p className="app-error" role="alert">
          <span>{error.message}</span>
          <code>{error.code}</code>
        </p>
      ) : null}
      {launcherOpen || !activeProject ? (
        <ProjectLauncher
          onCreate={createProject}
          onSelect={selectProject}
          projects={projects}
        />
      ) : (
        <div className="project-shell">
          <ProjectSidebar
            activeProjectId={activeProject.id}
            onCreate={() => setLauncherOpen(true)}
            onSelect={(id) => void selectProject(id)}
            projects={projects}
          />
          <Workbench project={activeProject} />
        </div>
      )}
    </div>
  );
}
