import { useCallback, useEffect, useRef, useState } from "react";

type WindowAction = "minimize" | "toggleMaximize" | "close";

export type DesktopWindowApi = {
  minimize(): Promise<void>;
  toggleMaximize(): Promise<void>;
  close(): Promise<void>;
  isMaximized(): Promise<boolean>;
  onResized(handler: () => void): Promise<() => void>;
};

type WindowControlsProps = {
  visible?: boolean;
  getWindow?: () => Promise<DesktopWindowApi>;
};

async function getCurrentDesktopWindow(): Promise<DesktopWindowApi> {
  const { getCurrentWindow } = await import("@tauri-apps/api/window");
  return getCurrentWindow();
}

function reportWindowFailure(context: string, error: unknown) {
  console.error(`[Keeper IDE] ${context}`, error);
}

/**
 * Adapted from Grok App's custom desktop chrome under its MIT license.
 * Source: src/components/WindowControls.tsx at 1c3f9c8aa3a42e869f11afb3d075f1b52843aaa9.
 */
export function WindowControls({
  visible = true,
  getWindow = getCurrentDesktopWindow,
}: WindowControlsProps) {
  const [maximized, setMaximized] = useState(false);
  const windowRef = useRef<DesktopWindowApi | null>(null);

  const resolveWindow = useCallback(async () => {
    if (windowRef.current) return windowRef.current;
    const currentWindow = await getWindow();
    windowRef.current = currentWindow;
    return currentWindow;
  }, [getWindow]);

  const refreshMaximized = useCallback(async (currentWindow?: DesktopWindowApi) => {
    try {
      const target = currentWindow ?? await resolveWindow();
      setMaximized(await target.isMaximized());
    } catch (error) {
      reportWindowFailure("Window state refresh failed", error);
    }
  }, [resolveWindow]);

  const runWindowAction = useCallback(async (action: WindowAction) => {
    try {
      const currentWindow = await resolveWindow();
      await currentWindow[action]();
      if (action === "toggleMaximize") {
        await refreshMaximized(currentWindow);
      }
    } catch (error) {
      reportWindowFailure(`Window action failed: ${action}`, error);
    }
  }, [refreshMaximized, resolveWindow]);

  useEffect(() => {
    if (!visible) return undefined;
    let disposed = false;
    let unlisten: (() => void) | undefined;

    void (async () => {
      try {
        const currentWindow = await resolveWindow();
        if (disposed) return;
        await refreshMaximized(currentWindow);
        const stopListening = await currentWindow.onResized(() => {
          void refreshMaximized(currentWindow);
        });
        if (disposed) {
          stopListening();
        } else {
          unlisten = stopListening;
        }
      } catch (error) {
        reportWindowFailure("Window resize subscription failed", error);
      }
    })();

    return () => {
      disposed = true;
      unlisten?.();
    };
  }, [refreshMaximized, resolveWindow, visible]);

  if (!visible) return null;

  return (
    <div className="window-controls" aria-label="窗口控制">
      <button
        aria-label="最小化"
        onClick={() => void runWindowAction("minimize")}
        type="button"
      >
        -
      </button>
      <button
        aria-label={maximized ? "还原" : "最大化"}
        onClick={() => void runWindowAction("toggleMaximize")}
        type="button"
      >
        {maximized ? "❐" : "□"}
      </button>
      <button
        aria-label="关闭"
        className="window-controls__close"
        onClick={() => void runWindowAction("close")}
        type="button"
      >
        ×
      </button>
    </div>
  );
}
