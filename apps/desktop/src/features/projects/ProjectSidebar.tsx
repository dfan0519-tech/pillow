import type { ProjectDto } from "../../lib/projectGateway";

type ProjectSidebarProps = {
  activeProjectId: string | null;
  projects: ProjectDto[];
  busy: boolean;
  onOpenProject: () => void;
  onNewProject: () => void;
  onSelect: (projectId: string) => void;
};

/**
 * Left project rail adapted from Grok App sidebar nav patterns under MIT.
 * Upstream: src/styles/app.css (.sidebar, .sidebar-nav, .nav-new) and
 * src/App.tsx project tree at cbbbdf17bb8db1588b6707f3f42f6b00397e27f9.
 */
export function ProjectSidebar({
  activeProjectId,
  projects,
  busy,
  onOpenProject,
  onNewProject,
  onSelect,
}: ProjectSidebarProps) {
  return (
    <nav aria-label="项目" className="project-sidebar">
      <div className="sidebar-chrome" data-tauri-drag-region>
        <span className="sidebar-brand">项目</span>
      </div>
      <div className="sidebar-nav">
        <button
          className="nav-new"
          disabled={busy}
          onClick={onOpenProject}
          type="button"
        >
          打开项目
        </button>
        <button
          className="nav-item"
          disabled={busy}
          onClick={onNewProject}
          type="button"
        >
          新建项目
        </button>
      </div>
      <div className="sidebar-section-label">最近项目</div>
      <div className="sidebar-list" role="list">
        {projects.length === 0 ? (
          <p className="sidebar-empty">还没有项目。打开文件夹或新建一个开始工作。</p>
        ) : (
          projects.map((project) => (
            <button
              aria-label={project.name}
              aria-pressed={project.id === activeProjectId}
              className={project.id === activeProjectId ? "is-active" : ""}
              key={project.id}
              onClick={() => onSelect(project.id)}
              title={project.rootPath}
              type="button"
            >
              <span>{project.name}</span>
              <small>{project.trust === "trusted" ? "已信任" : "需确认"}</small>
            </button>
          ))
        )}
      </div>
    </nav>
  );
}
