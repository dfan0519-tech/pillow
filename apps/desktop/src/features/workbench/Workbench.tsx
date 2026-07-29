import { useState } from "react";
import type { ProjectDto } from "../../lib/projectGateway";
import { ConversationSidebar } from "../sessions/ConversationSidebar";
import { Composer } from "./Composer";
import { ResourcePane } from "./ResourcePane";

type WorkbenchProps = {
  project: ProjectDto | null;
  onOpenProject: () => void;
  onNewProject: () => void;
  busy?: boolean;
};

/**
 * Conversation workbench adapted from Grok App main stage layout under MIT.
 * Upstream: src/App.tsx main stage, lobe-chat ConversationThread rhythm,
 * and composer welcome empty state at
 * cbbbdf17bb8db1588b6707f3f42f6b00397e27f9.
 */
export function Workbench({
  project,
  onOpenProject,
  onNewProject,
  busy = false,
}: WorkbenchProps) {
  const [resourcesOpen, setResourcesOpen] = useState(false);

  return (
    <main
      className={
        resourcesOpen ? "workbench has-resources" : "workbench"
      }
    >
      <ConversationSidebar project={project} />
      <section className="conversation-surface" aria-label="对话工作台">
        <header className="workbench-header">
          <div className="workbench-header__project">
            <p className="eyebrow">{project ? "当前项目" : "工作台"}</p>
            {project ? (
              <h1 data-testid="active-project-name" title={project.name}>
                {project.name}
              </h1>
            ) : (
              <h1 data-testid="empty-workbench-title">Keeper IDE</h1>
            )}
          </div>
          <div className="workbench-actions">
            {project ? (
              <span>{project.trust === "trusted" ? "已信任" : "需确认信任"}</span>
            ) : null}
            <button onClick={() => setResourcesOpen(true)} type="button">
              资源
            </button>
          </div>
        </header>

        {project ? (
          <div className="conversation-empty">
            <p className="eyebrow">新对话</p>
            <h2>从一个清晰的任务开始。</h2>
            <p>计划、工具活动和变更证据会在这里按时间展开。</p>
          </div>
        ) : (
          <div className="conversation-empty empty-workbench" data-testid="empty-workbench">
            <p className="eyebrow">本地工作区</p>
            <h2>打开项目，继续工作。</h2>
            <p>项目承载对话、文件、终端与后续的审批记录。</p>
            <div className="empty-workbench__actions">
              <button
                disabled={busy}
                onClick={onOpenProject}
                type="button"
              >
                打开项目
              </button>
              <button
                className="btn-secondary"
                disabled={busy}
                onClick={onNewProject}
                type="button"
              >
                新建项目
              </button>
            </div>
          </div>
        )}

        <Composer disabled={!project} />
      </section>
      <ResourcePane
        hasProject={Boolean(project)}
        onClose={() => setResourcesOpen(false)}
        open={resourcesOpen}
      />
    </main>
  );
}
