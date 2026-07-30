import { useEffect, useState } from "react";
import type {
  ConversationTimelineView,
  ResourceReferenceView,
  RunView,
} from "../conversations/conversationReducer";
import { isRunActive } from "../conversations/conversationReducer";
import { ConversationTimeline } from "../conversations/ConversationTimeline";
import type { ProjectDto } from "../../lib/projectGateway";
import { IconKeeperMark, IconPanelRight } from "../../components/icons";
import { Composer } from "./Composer";
import { ResourcePane } from "./ResourcePane";

type WorkbenchProps = {
  project: ProjectDto | null;
  onOpenProject: () => void;
  onSendTurn: (content: string) => void | Promise<void>;
  onCancelRun: () => void;
  busy?: boolean;
  sending?: boolean;
  sendError?: string | null;
  refreshWarning?: string | null;
  timeline: ConversationTimelineView | null;
  run: RunView | null;
  showMockBadge?: boolean;
  mockBadgeLabel?: string;
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
  onSendTurn,
  onCancelRun,
  busy = false,
  sending = false,
  sendError = null,
  refreshWarning = null,
  timeline,
  run,
  showMockBadge = false,
  mockBadgeLabel = "模拟运行时",
}: WorkbenchProps) {
  const projectId = project?.id ?? null;
  const [resourcePaneScope, setResourcePaneScope] =
    useState<{ projectId: string | null } | null>(null);
  const [resourceSelection, setResourceSelection] =
    useState<{
      projectId: string;
      resource: ResourceReferenceView;
    } | null>(null);
  const resourcesOpen = resourcePaneScope?.projectId === projectId;
  const selectedResource =
    resourceSelection?.projectId === projectId
      ? resourceSelection.resource
      : null;
  const hasTimeline = Boolean(
    timeline
    && (
      timeline.messages.length > 0
      || Object.keys(timeline.activitiesByRunId).length > 0
    ),
  );
  const canCancel = isRunActive(run);

  useEffect(() => {
    if (resourceSelection && resourceSelection.projectId !== projectId) {
      setResourceSelection(null);
    }
    if (resourcePaneScope && resourcePaneScope.projectId !== projectId) {
      setResourcePaneScope(null);
    }
  }, [projectId, resourcePaneScope?.projectId, resourceSelection?.projectId]);

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
              onClick={() =>
                setResourcePaneScope((scope) =>
                  scope?.projectId === projectId ? null : { projectId },
                )}
              type="button"
            >
              <IconPanelRight size={16} />
            </button>
          </div>
        </header>

        <div className="stage-body">
          {hasTimeline && timeline ? (
            <ConversationTimeline
              mockBadgeLabel={mockBadgeLabel}
              onOpenResource={(resource) => {
                if (!project) return;
                setResourceSelection({ projectId: project.id, resource });
                setResourcePaneScope({ projectId: project.id });
              }}
              run={run}
              showMockBadge={showMockBadge}
              timeline={timeline}
            />
          ) : project ? (
            <div className="stage-empty" data-testid="selected-workbench">
              {showMockBadge ? (
                <div className="mock-runtime-badge" data-testid="mock-runtime-badge">
                  {mockBadgeLabel}
                </div>
              ) : null}
              <IconKeeperMark size={40} title="Keeper" />
              <h2>Keeper</h2>
              <p>从一个清晰的任务开始。计划与变更会在这里展开。</p>
            </div>
          ) : (
            <div className="stage-empty" data-testid="empty-workbench">
              <IconKeeperMark size={40} title="Keeper" />
              <h2>Pillow</h2>
              <p>打开现有项目文件夹以开始本地工作。</p>
            </div>
          )}
        </div>

        <Composer
          busy={busy}
          canCancel={canCancel}
          disabled={!project}
          onCancelRun={onCancelRun}
          onOpenProject={onOpenProject}
          onSend={onSendTurn}
          projectName={project?.name ?? null}
          refreshWarning={refreshWarning}
          sendError={sendError}
          sending={sending}
        />
      </section>
      <ResourcePane
        hasProject={Boolean(project)}
        onClose={() => setResourcePaneScope(null)}
        open={resourcesOpen}
        resource={selectedResource}
      />
    </main>
  );
}
