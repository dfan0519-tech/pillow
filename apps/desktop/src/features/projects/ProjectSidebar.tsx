import type { ProjectDto } from "../../lib/projectGateway";

type ProjectSidebarProps = {
  activeProjectId: string | null;
  projects: ProjectDto[];
  onCreate: () => void;
  onSelect: (projectId: string) => void;
};

/** Fixed row navigation derived from Grok App's bounded sidebar-list pattern. */
export function ProjectSidebar({ activeProjectId, projects, onCreate, onSelect }: ProjectSidebarProps) {
  return (
    <nav aria-label="项目" className="project-sidebar">
      <div className="sidebar-heading"><span>项目</span><button aria-label="新建项目" onClick={onCreate} type="button">+</button></div>
      <div className="sidebar-list" role="list">
        {projects.map((project) => (
          <button
            aria-pressed={project.id === activeProjectId}
            className={project.id === activeProjectId ? "is-active" : ""}
            key={project.id}
            onClick={() => onSelect(project.id)}
            type="button"
          >
            <span>{project.name}</span><small>{project.trust === "trusted" ? "已信任" : "需确认"}</small>
          </button>
        ))}
      </div>
    </nav>
  );
}
