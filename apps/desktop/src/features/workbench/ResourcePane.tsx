type ResourcePaneProps = {
  open: boolean;
  onClose: () => void;
  hasProject: boolean;
};

/**
 * Contextual resource pane adapted from Grok App ResourceViewer shell chrome.
 * Upstream: src/components/ResourceViewer.tsx + .aside rules in app.css at
 * cbbbdf17bb8db1588b6707f3f42f6b00397e27f9.
 * File previews and agent artifacts are intentionally stubbed for this prototype.
 */
export function ResourcePane({ open, onClose, hasProject }: ResourcePaneProps) {
  if (!open) return null;

  return (
    <aside aria-label="上下文资源" className="resource-pane">
      <div className="resource-pane__heading">
        <strong>资源</strong>
        <button aria-label="关闭资源面板" onClick={onClose} type="button">
          ×
        </button>
      </div>
      {hasProject ? (
        <p>
          在对话中打开文件、差异、终端或产物后，相关内容会显示在这里。
        </p>
      ) : (
        <p>打开项目后，上下文资源会显示在这里。</p>
      )}
    </aside>
  );
}
