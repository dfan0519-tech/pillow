import { useState } from "react";
import type { ConversationDto } from "../../lib/conversationGateway";
import type { ProjectDto } from "../../lib/projectGateway";
import { BrandMark } from "../../components/BrandMark";
import {
  IconChevronDown,
  IconChevronRight,
  IconCircleDot,
  IconFolder,
  IconNewChat,
  IconSearch,
} from "../../components/icons";

type AppSidebarProps = {
  activeProjectId: string | null;
  projects: ProjectDto[];
  conversations: ConversationDto[];
  selectedConversationId: string | null;
  busy: boolean;
  onOpenProject: () => void;
  onSelect: (projectId: string) => void;
  onNewConversation: () => void;
  onSelectConversation: (conversationId: string) => void;
};

/**
 * Continuous left sidebar adapted from Grok App App.tsx sidebar chrome.
 * Upstream: src/App.tsx sidebar + src/styles/app.css (.sidebar, .nav-new,
 * .tree-l1, .project-row, .session-row) at
 * cbbbdf17bb8db1588b6707f3f42f6b00397e27f9.
 * Project + conversation navigation share one rail (no dual admin columns).
 */
export function AppSidebar({
  activeProjectId,
  projects,
  conversations,
  selectedConversationId,
  busy,
  onOpenProject,
  onSelect,
  onNewConversation,
  onSelectConversation,
}: AppSidebarProps) {
  const [projectsOpen, setProjectsOpen] = useState(true);
  const active = projects.find((project) => project.id === activeProjectId) ?? null;

  return (
    <nav aria-label="导航" className="app-sidebar" data-testid="app-sidebar">
      <div className="sidebar-brand-row" data-tauri-drag-region>
        <div className="sidebar-brand-row__left" data-tauri-drag-region>
          <BrandMark size={22} showWordmark />
        </div>
        <button
          aria-label="搜索"
          className="chrome-btn"
          disabled
          title="搜索（即将推出）"
          type="button"
        >
          <IconSearch size={16} />
        </button>
      </div>

      <div className="sidebar-nav">
        <button
          className="nav-new"
          disabled={busy || !active}
          onClick={onNewConversation}
          title={active ? "新建对话" : "打开项目后可新建对话"}
          type="button"
        >
          <span className="nav-item__icon">
            <IconNewChat size={16} />
          </span>
          新建对话
        </button>
        <button
          className="nav-item"
          data-testid="nav-open-project"
          disabled={busy}
          onClick={onOpenProject}
          type="button"
        >
          <span className="nav-item__icon">
            <IconFolder size={16} />
          </span>
          打开项目
        </button>
      </div>

      <div className="sidebar__scroll">
        <div className="tree-l1">
          <button
            aria-expanded={projectsOpen}
            className="tree-l1__head"
            onClick={() => setProjectsOpen((value) => !value)}
            type="button"
          >
            {projectsOpen ? (
              <IconChevronDown size={14} />
            ) : (
              <IconChevronRight size={14} />
            )}
            <span className="tree-l1__label">项目</span>
          </button>
        </div>

        {projectsOpen ? (
          <div className="sidebar-projects" role="list">
            {projects.length === 0 ? (
              <p className="sidebar-empty">还没有项目。打开现有文件夹以开始。</p>
            ) : (
              projects.map((project) => {
                const isActive = project.id === activeProjectId;
                return (
                  <div className="project-block" key={project.id} role="listitem">
                    <button
                      aria-label={project.name}
                      aria-pressed={isActive}
                      className={
                        isActive ? "project-row project-row--active" : "project-row"
                      }
                      disabled={busy}
                      onClick={() => onSelect(project.id)}
                      title={project.name}
                      type="button"
                    >
                      <span className="project-row__icon">
                        <IconFolder size={15} />
                      </span>
                      <span className="project-row__name">{project.name}</span>
                    </button>
                    {isActive ? (
                      <div
                        aria-label="项目对话"
                        className="session-list"
                        data-testid="project-conversations"
                        role="group"
                      >
                        {conversations.length === 0 ? (
                          <p className="session-empty">
                            尚无对话。在下方描述工作后，它会保存在此项目中。
                          </p>
                        ) : (
                          conversations.map((conversation) => {
                            const selected = conversation.id === selectedConversationId;
                            return (
                              <button
                                aria-current={selected ? "page" : undefined}
                                className={
                                  selected
                                    ? "session-row session-row--active"
                                    : "session-row"
                                }
                                data-testid="conversation-row"
                                key={conversation.id}
                                onClick={() => onSelectConversation(conversation.id)}
                                title={conversation.title}
                                type="button"
                              >
                                <span className="session-row__title">
                                  {conversation.title}
                                </span>
                              </button>
                            );
                          })
                        )}
                      </div>
                    ) : null}
                  </div>
                );
              })
            )}
          </div>
        ) : null}
      </div>

      <footer className="sidebar-footer">
        <span className="sidebar-footer__status">
          <IconCircleDot size={14} />
          <span>本地工作台 · 就绪</span>
        </span>
      </footer>
    </nav>
  );
}
