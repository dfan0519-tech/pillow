import { useState } from "react";
import type { ProjectDto } from "../../lib/projectGateway";
import { ConversationSidebar } from "../sessions/ConversationSidebar";
import { ResourcePane } from "./ResourcePane";

type WorkbenchProps = { project: ProjectDto };

/** Conversation rhythm adapted from Grok App's thread spacing, without provider state. */
export function Workbench({ project }: WorkbenchProps) {
  const [resourcesOpen, setResourcesOpen] = useState(false);

  return (
    <main className={resourcesOpen ? "workbench has-resources" : "workbench"}>
      <ConversationSidebar project={project} />
      <section className="conversation-surface" aria-label="对话工作台">
        <header className="workbench-header">
          <div><p className="eyebrow">当前项目</p><h1 data-testid="active-project-name">{project.name}</h1></div>
          <div className="workbench-actions"><span>{project.trust === "trusted" ? "已信任" : "需确认信任"}</span><button onClick={() => setResourcesOpen(true)} type="button">资源</button></div>
        </header>
        <div className="conversation-empty">
          <p className="eyebrow">新对话</p>
          <h2>从一个清晰的任务开始。</h2>
          <p>计划、工具活动和变更证据会在这里按时间展开。</p>
        </div>
        <form className="composer" onSubmit={(event) => event.preventDefault()}>
          <label className="sr-only" htmlFor="work-request">工作请求</label>
          <textarea id="work-request" placeholder="描述要完成的工作" rows={3} />
          <div><span>运行时将在创建对话后选择</span><button disabled type="submit">发送</button></div>
        </form>
      </section>
      {resourcesOpen ? <ResourcePane onClose={() => setResourcesOpen(false)} /> : null}
    </main>
  );
}
