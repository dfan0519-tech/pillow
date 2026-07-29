import { useEffect, useState } from "react";
import { WindowControls } from "./components/WindowControls";
import { ProjectLauncher } from "./features/projects/ProjectLauncher";
import { ProjectSidebar } from "./features/projects/ProjectSidebar";
import { Workbench } from "./features/workbench/Workbench";
import type { CreateProjectInput, ProjectDto, ProjectGateway } from "./lib/projectGateway";

type AppProps = { gateway: ProjectGateway };

/**
 * Shell regions and adjustable navigation adapt Grok App's App.tsx layout under MIT.
 * Source: src/App.tsx at 1c3f9c8aa3a42e869f11afb3d075f1b52843aaa9.
 */
export function App({ gateway }: AppProps) {
  const [projects, setProjects] = useState<ProjectDto[]>([]);
  const [activeProject, setActiveProject] = useState<ProjectDto | null>(null);
  const [launcherOpen, setLauncherOpen] = useState(true);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    let active = true;
    void gateway.listProjects().then(
      (result) => {
        if (!active) return;
        setProjects(result);
        setLoading(false);
      },
      () => {
        if (!active) return;
        setError("无法加载项目。请检查本地桌面服务是否正在运行。");
        setLoading(false);
      },
    );
    return () => { active = false; };
  }, [gateway]);

  async function selectProject(projectId: string) {
    try {
      const project = await gateway.selectProject(projectId);
      setActiveProject(project);
      setLauncherOpen(false);
    } catch {
      setError("无法打开该项目。");
    }
  }

  async function createProject(input: CreateProjectInput) {
    try {
      const project = await gateway.createProject(input);
      setProjects((current) => [...current, project]);
      setActiveProject(project);
      setLauncherOpen(false);
    } catch {
      setError("无法创建项目。请确认目录可用后重试。");
    }
  }

  if (loading) return <main className="app-loading">正在加载项目…</main>;

  return (
    <div className="app-shell">
      <header className="titlebar" data-tauri-drag-region>
        <button aria-label="打开项目启动器" className="brand" onClick={() => setLauncherOpen(true)} type="button">Keeper IDE</button>
        <WindowControls />
      </header>
      {error ? <p className="app-error" role="alert">{error}</p> : null}
      {launcherOpen || !activeProject ? (
        <ProjectLauncher onCreate={createProject} onSelect={selectProject} projects={projects} />
      ) : (
        <div className="project-shell">
          <ProjectSidebar activeProjectId={activeProject.id} onCreate={() => setLauncherOpen(true)} onSelect={(id) => void selectProject(id)} projects={projects} />
          <Workbench project={activeProject} />
        </div>
      )}
    </div>
  );
}
