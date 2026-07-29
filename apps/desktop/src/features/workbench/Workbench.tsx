import { useState } from "react";
import type { ProjectDto } from "../../lib/projectGateway";
import { IconKeeperMark, IconPanelRight } from "../../components/icons";
import { Composer } from "./Composer";
import { ResourcePane } from "./ResourcePane";

type WorkbenchProps = {
  project: ProjectDto | null;
  onOpenProject: () => void;
  onNewProject: () => void;
  busy?: boolean;
};

/**
 * Main stage adapted from Grok App welcome / conversation surface.
 * Upstream: src/App.tsx main stage + welcome empty rhythm at
 * cbbbdf17bb8db1588b6707f3f42f6b00397e27f9.
 * Quiet center stage — no marketing dual CTAs or admin header bands.
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
      className={resourcesOpen ? "workbench has-resources" : "workbench"}
      data-testid="workbench"
    >
      <section className="conversation-surface" aria-label="对话工作台">
        <header className="stage-top">
          <div className="stage-top__title" data-tauri-drag-region>
            {project ? (
              <h1 data-testid="active-project-name" title={project.name}>
                {project.name}
              </h1>
            ) : (
              <h1 data-testid="empty-workbench-title">新对话</h1>
            )}
          </div>
          <div className="stage-top__actions">
            <button
              aria-label={resourcesOpen ? "关闭资源面板" : "打开资源面板"}
              aria-pressed={resourcesOpen}
              className={
                resourcesOpen
                  ? "chrome-btn chrome-btn--on"
                  : "chrome-btn"
              }
              data-testid="resource-toggle"
              onClick={() => setResourcesOpen((value) => !value)}
              type="button"
            >
              <IconPanelRight size={16} />
            </button>
          </div>
        </header>

        <div className="stage-body">
          {project ? (
            <div className="stage-empty" data-testid="selected-workbench">
              <IconKeeperMark size={40} title="Keeper" />
              <h2>Keeper</h2>
              <p>从一个清晰的任务开始。计划与变更会在这里展开。</p>
            </div>
          ) : (
            <div className="stage-empty" data-testid="empty-workbench">
              <IconKeeperMark size={40} title="Keeper" />
              <h2>Keeper IDE</h2>
              <p>打开或新建项目以开始本地工作。项目命令在左侧边栏与输入区。</p>
            </div>
          )}
        </div>

        <Composer
          busy={busy}
          disabled={!project}
          onNewProject={onNewProject}
          onOpenProject={onOpenProject}
          projectName={project?.name ?? null}
        />
      </section>
      <ResourcePane
        hasProject={Boolean(project)}
        onClose={() => setResourcesOpen(false)}
        open={resourcesOpen}
      />
    </main>
  );
}
