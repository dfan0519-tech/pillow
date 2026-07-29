type ResourcePaneProps = { onClose: () => void };

export function ResourcePane({ onClose }: ResourcePaneProps) {
  return (
    <aside aria-label="上下文资源" className="resource-pane">
      <div className="resource-pane__heading"><strong>资源</strong><button aria-label="关闭资源面板" onClick={onClose} type="button">×</button></div>
      <p>在对话中打开文件、差异、终端或产物后，相关内容会显示在这里。</p>
    </aside>
  );
}
