import type { ProjectDto } from "../../lib/projectGateway";

type ConversationSidebarProps = { project: ProjectDto };

export function ConversationSidebar({ project }: ConversationSidebarProps) {
  return (
    <nav aria-label="项目对话" className="conversation-sidebar">
      <div className="sidebar-heading"><span>对话</span><button aria-label="新建对话" type="button">+</button></div>
      <p className="conversation-sidebar__project">{project.name}</p>
      <p className="empty-copy">尚无对话。开始一次工作请求后，它会保存在这个项目中。</p>
      <button className="archive-link" type="button">已归档</button>
    </nav>
  );
}
