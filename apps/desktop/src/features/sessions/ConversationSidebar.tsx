import type { ProjectDto } from "../../lib/projectGateway";

type ConversationSidebarProps = {
  project: ProjectDto | null;
};

/**
 * Conversation rail adapted from Grok App session sidebar patterns.
 * Upstream: src/App.tsx session tree + src/styles/app.css at
 * cbbbdf17bb8db1588b6707f3f42f6b00397e27f9.
 */
export function ConversationSidebar({ project }: ConversationSidebarProps) {
  return (
    <nav aria-label="项目对话" className="conversation-sidebar">
      <div className="sidebar-heading">
        <span>对话</span>
        <button aria-label="新建对话" disabled={!project} type="button">
          +
        </button>
      </div>
      {project ? (
        <>
          <p className="conversation-sidebar__project" title={project.name}>
            {project.name}
          </p>
          <p className="empty-copy">
            尚无对话。开始一次工作请求后，它会保存在这个项目中。
          </p>
          <button className="archive-link" type="button">
            已归档
          </button>
        </>
      ) : (
        <p className="empty-copy">选择或打开一个项目后，对话会显示在这里。</p>
      )}
    </nav>
  );
}
