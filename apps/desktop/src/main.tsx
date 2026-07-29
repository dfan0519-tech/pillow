import { StrictMode } from "react";
import { createRoot } from "react-dom/client";
import { App } from "./App";
import {
  TauriDesktopLifecycle,
  runNativeAcceptanceIfRequested,
} from "./lib/desktopLifecycle";
import { TauriProjectGateway } from "./lib/tauriProjectGateway";
import "./styles/tokens.css";
import "./styles/workbench.css";

const root = document.getElementById("root");
if (!root) throw new Error("Missing renderer root");

const projectGateway = new TauriProjectGateway();
const desktopLifecycle = new TauriDesktopLifecycle();

createRoot(root).render(
  <StrictMode>
    <App
      gateway={projectGateway}
      onProjectLauncherReady={() => desktopLifecycle.markProjectLauncherReady()}
    />
  </StrictMode>,
);

void runNativeAcceptanceIfRequested(projectGateway, desktopLifecycle).catch(
  (cause: unknown) => {
    console.error("Keeper IDE native acceptance failed.", cause);
  },
);
