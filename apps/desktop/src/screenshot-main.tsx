import { StrictMode } from "react";
import { createRoot } from "react-dom/client";
import { App } from "./App";
import { MockConversationGateway } from "./lib/mockConversationGateway";
import { MockProjectGateway } from "./lib/mockProjectGateway";
import type { ProjectDto } from "./lib/projectGateway";
import "./styles/tokens.css";
import "./styles/workbench.css";

const sample: ProjectDto = {
  id: "screenshot-project",
  name: "Keeper IDE",
  rootPath: "F:\\work\\keeper-ide",
  trust: "trusted",
  createdAt: "2026-07-29T00:00:00.000Z",
  updatedAt: "2026-07-29T00:00:00.000Z",
};

const params = new URLSearchParams(window.location.search);
const state = params.get("state") === "selected" ? "selected" : "empty";

// Slow mock steps so the capture script can photograph mid-stream / cancel.
// Keep well above Playwright interaction latency so streaming shots stay active.
const conversationGateway = new MockConversationGateway({ stepDelayMs: 90 });
const gateway = new MockProjectGateway(state === "selected" ? [sample] : []);

const root = document.getElementById("root");
if (!root) throw new Error("Missing renderer root");

createRoot(root).render(
  <StrictMode>
    <App
      conversationGateway={conversationGateway}
      defaultSelectedProjectId={state === "selected" ? sample.id : undefined}
      gateway={gateway}
    />
  </StrictMode>,
);

document.documentElement.dataset.screenshotReady = "pending";
void gateway.listProjects().then(() => {
  window.setTimeout(() => {
    document.documentElement.dataset.screenshotReady = "ready";
  }, state === "selected" ? 120 : 40);
});
