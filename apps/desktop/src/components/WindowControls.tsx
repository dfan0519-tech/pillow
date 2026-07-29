import { useCallback, useEffect, useState } from "react";

type WindowAction = "minimize" | "toggleMaximize" | "close";

type WindowControlsProps = {
  visible?: boolean;
};

async function useWindowAction(action: WindowAction): Promise<boolean | void> {
  try {
    const { getCurrentWindow } = await import("@tauri-apps/api/window");
    const window = getCurrentWindow();
    if (action === "minimize") return window.minimize();
    if (action === "toggleMaximize") return window.toggleMaximize();
    return window.close();
  } catch {
    return undefined;
  }
}

/**
 * Adapted from Grok App's custom desktop chrome under its MIT license.
 * Source: src/components/WindowControls.tsx at 1c3f9c8aa3a42e869f11afb3d075f1b52843aaa9.
 */
export function WindowControls({ visible = true }: WindowControlsProps) {
  const [maximized, setMaximized] = useState(false);
  const refreshMaximized = useCallback(async () => {
    try {
      const { getCurrentWindow } = await import("@tauri-apps/api/window");
      setMaximized(await getCurrentWindow().isMaximized());
    } catch {
      setMaximized(false);
    }
  }, []);

  useEffect(() => {
    if (!visible) return undefined;
    void refreshMaximized();
    return undefined;
  }, [refreshMaximized, visible]);

  if (!visible) return null;

  return (
    <div className="window-controls" aria-label="窗口控制">
      <button aria-label="最小化" onClick={() => void useWindowAction("minimize")} type="button">-</button>
      <button
        aria-label={maximized ? "还原" : "最大化"}
        onClick={async () => {
          await useWindowAction("toggleMaximize");
          await refreshMaximized();
        }}
        type="button"
      >
        {maximized ? "□" : "□"}
      </button>
      <button aria-label="关闭" className="window-controls__close" onClick={() => void useWindowAction("close")} type="button">×</button>
    </div>
  );
}
