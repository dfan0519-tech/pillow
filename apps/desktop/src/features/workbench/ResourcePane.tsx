import { IconClose } from "../../components/icons";
import type { ResourceReferenceView } from "../conversations/conversationReducer";

type ResourcePaneProps = {
  open: boolean;
  onClose: () => void;
  hasProject: boolean;
  resource: ResourceReferenceView | null;
};

/**
 * Contextual resource pane chrome adapted from Grok App ResourceViewer.
 * Upstream: src/components/ResourceViewer.tsx + .aside rules in app.css at
 * cbbbdf17bb8db1588b6707f3f42f6b00397e27f9.
 * Displays renderer-projected references only; it does not execute reads.
 */
export function ResourcePane({
  open,
  onClose,
  hasProject,
  resource,
}: ResourcePaneProps) {
  if (!open) return null;

  return (
    <aside aria-label="上下文资源" className="resource-pane" data-testid="resource-pane">
      <div className="resource-pane__heading">
        <h2>资源</h2>
        <button
          aria-label="关闭资源面板"
          className="chrome-btn"
          onClick={onClose}
          type="button"
        >
          <IconClose size={16} />
        </button>
      </div>
      {resource ? (
        <div
          aria-live="polite"
          className="resource-pane__content"
          data-resource-id={resource.resourceId}
        >
          <h3>{resource.label}</h3>
          {resource.path && resource.path !== resource.label ? (
            <code>{resource.path}</code>
          ) : null}
          <span className="resource-pane__kind">
            {resourceKindLabel(resource.kind)}
          </span>
          {resource.contentPreview ? (
            <pre tabIndex={0}>{resource.contentPreview}</pre>
          ) : (
            <p>此引用没有可读内容预览。</p>
          )}
          {resource.truncated ? (
            <p className="resource-pane__truncation">内容预览已截断</p>
          ) : null}
        </div>
      ) : hasProject ? (
        <p>
          在对话中打开文件、差异、终端或产物后，相关内容会显示在这里。
        </p>
      ) : (
        <p>打开项目后，上下文资源会显示在这里。</p>
      )}
    </aside>
  );
}

function resourceKindLabel(kind: ResourceReferenceView["kind"]): string {
  switch (kind) {
    case "file":
      return "文件";
    case "directory":
      return "目录";
    case "search_results":
      return "搜索结果";
    case "artifact":
      return "产物";
    case "receipt":
      return "执行凭据";
  }
}
