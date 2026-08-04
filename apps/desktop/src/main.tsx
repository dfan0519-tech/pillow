import { StrictMode } from "react";
import { createRoot } from "react-dom/client";
import { App } from "./App";
import {
  TauriDesktopLifecycle,
  runNativeAcceptanceIfRequested,
} from "./lib/desktopLifecycle";
import { TauriConversationGateway } from "./lib/tauriConversationGateway";
import { TauriProjectGateway } from "./lib/tauriProjectGateway";
import { TauriResourceGateway } from "./lib/tauriResourceGateway";
import { TauriWriteApprovalGateway } from "./lib/tauriWriteApprovalGateway";
import type { ConversationGateway } from "./lib/conversationGateway";
import type { ProjectGateway } from "./lib/projectGateway";
import { invoke } from "@tauri-apps/api/core";
import "./styles/tokens.css";
import "./styles/workbench.css";
import "./styles/write-approval.css";

const root = document.getElementById("root");
if (!root) throw new Error("Missing renderer root");

const projectGateway = new TauriProjectGateway();
const conversationGateway = new TauriConversationGateway();
const resourceGateway = new TauriResourceGateway();
const writeApprovalGateway = new TauriWriteApprovalGateway();
const desktopLifecycle = new TauriDesktopLifecycle();

createRoot(root).render(
  <StrictMode>
    <App
      gateway={projectGateway}
      conversationGateway={conversationGateway}
      resourceGateway={resourceGateway}
      writeApprovalGateway={writeApprovalGateway}
      onProjectLauncherReady={() => desktopLifecycle.markProjectLauncherReady()}
    />
  </StrictMode>,
);

void runNativeAcceptanceIfRequested(projectGateway, desktopLifecycle).catch(
  (cause: unknown) => {
    console.error("Pillow native acceptance failed.", cause);
  },
);

void runConversationAcceptanceIfRequested(
  projectGateway,
  conversationGateway,
).catch((cause: unknown) => {
  console.error("Pillow conversation acceptance failed.", cause);
});

type ConversationAcceptanceScenario = {
  phase: "create_stream" | "restore";
  projectName: string;
  projectRoot: string;
  prompt: string;
};

type ConversationAcceptanceResult = {
  phase: string;
  status: "passed" | "failed";
  projectId: string;
  conversationId: string;
  runId: string;
  runStatus: string;
  eventCount: number;
  errorMessage?: string;
};

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === "object" && value !== null && !Array.isArray(value);
}

function parseConversationScenario(
  value: unknown,
): ConversationAcceptanceScenario | null {
  if (value === null || value === undefined) return null;
  if (
    !isRecord(value)
    || (value.phase !== "create_stream" && value.phase !== "restore")
    || typeof value.projectName !== "string"
    || value.projectName.trim().length === 0
    || typeof value.projectRoot !== "string"
    || value.projectRoot.trim().length === 0
    || typeof value.prompt !== "string"
  ) {
    throw new Error("Pillow received an invalid conversation acceptance scenario.");
  }
  return {
    phase: value.phase,
    projectName: value.projectName,
    projectRoot: value.projectRoot,
    prompt: value.prompt,
  };
}

async function completeConversationAcceptance(
  result: ConversationAcceptanceResult,
): Promise<void> {
  await invoke("complete_conversation_acceptance", { result });
}

/** Request the app's own window-close path so ExitRequested runs service.shutdown. */
async function requestGracefulAppExit(): Promise<void> {
  try {
    const { getCurrentWindow } = await import("@tauri-apps/api/window");
    await getCurrentWindow().close();
  } catch (error) {
    console.error("Pillow graceful exit request failed.", error);
  }
}

async function waitForTerminalRun(
  gateway: ConversationGateway,
  conversationId: string,
  runId: string,
  timeoutMs = 30_000,
): Promise<{ runStatus: string; eventCount: number }> {
  const deadline = Date.now() + timeoutMs;
  while (Date.now() < deadline) {
    const timeline = await gateway.loadTimeline(conversationId);
    const run = timeline.runs.find((item) => item.runId === runId);
    const terminal = run
      && (run.status === "completed"
        || run.status === "failed"
        || run.status === "cancelled"
        || run.status === "interrupted");
    if (terminal && run) {
      return { runStatus: run.status, eventCount: timeline.events.length };
    }
    await new Promise((resolve) => setTimeout(resolve, 100));
  }
  throw new Error(`Timed out waiting for terminal run ${runId}.`);
}

/**
 * Native conversation acceptance for Slice B1 (create stream + restart restore).
 * Driven only when the host injects the conversation acceptance scenario env.
 */
export async function runConversationAcceptanceIfRequested(
  projects: ProjectGateway,
  conversations: ConversationGateway,
): Promise<void> {
  let scenario: ConversationAcceptanceScenario | null = null;
  try {
    scenario = parseConversationScenario(
      await invoke<unknown>("get_conversation_acceptance_scenario"),
    );
  } catch {
    return;
  }
  if (!scenario) return;

  try {
    if (scenario.phase === "create_stream") {
      const project = await projects.createProject({
        name: scenario.projectName,
        rootPath: scenario.projectRoot,
      });
      await projects.selectProject(project.id);
      const conversation = await conversations.createConversation(
        project.id,
        scenario.prompt.slice(0, 32),
      );
      const { runId } = await conversations.sendTurn(
        conversation.id,
        scenario.prompt,
      );
      const terminal = await waitForTerminalRun(
        conversations,
        conversation.id,
        runId,
      );
      if (terminal.runStatus !== "completed") {
        throw new Error(`Expected completed run, got ${terminal.runStatus}.`);
      }
      if (terminal.eventCount < 2) {
        throw new Error(`Expected streamed events, got ${terminal.eventCount}.`);
      }
      await completeConversationAcceptance({
        phase: scenario.phase,
        status: "passed",
        projectId: project.id,
        conversationId: conversation.id,
        runId,
        runStatus: terminal.runStatus,
        eventCount: terminal.eventCount,
      });
      await requestGracefulAppExit();
      return;
    }

    // restore phase: same app-data, process restarted; prove SQLite recovery.
    const listedProjects = await projects.listProjects();
    const matches = listedProjects.filter(
      (project) =>
        project.name === scenario.projectName
        && project.rootPath === scenario.projectRoot,
    );
    if (matches.length !== 1) {
      throw new Error(`Expected one restored project, received ${matches.length}.`);
    }
    const project = matches[0]!;
    await projects.selectProject(project.id);
    const conversationsForProject = await conversations.listConversations(project.id);
    if (conversationsForProject.length < 1) {
      throw new Error("Expected at least one restored conversation.");
    }
    const conversation = conversationsForProject[0]!;
    const timeline = await conversations.loadTimeline(conversation.id);
    if (timeline.messages.length < 1) {
      throw new Error("Expected restored user message.");
    }
    if (timeline.events.length < 1) {
      throw new Error("Expected restored runtime events.");
    }
    if (timeline.runs.length < 1) {
      throw new Error("Expected restored run snapshot.");
    }
    const run = timeline.runs[timeline.runs.length - 1]!;
    // Completed turns stay completed; non-terminal leftover becomes interrupted.
    if (
      run.status !== "completed"
      && run.status !== "interrupted"
      && run.status !== "failed"
      && run.status !== "cancelled"
    ) {
      throw new Error(`Unexpected restored run status: ${run.status}`);
    }
    await completeConversationAcceptance({
      phase: scenario.phase,
      status: "passed",
      projectId: project.id,
      conversationId: conversation.id,
      runId: run.runId,
      runStatus: run.status,
      eventCount: timeline.events.length,
    });
    await requestGracefulAppExit();
  } catch (error) {
    await completeConversationAcceptance({
      phase: scenario.phase,
      status: "failed",
      projectId: "",
      conversationId: "",
      runId: "",
      runStatus: "",
      eventCount: 0,
      errorMessage: error instanceof Error ? error.message : "Unknown acceptance failure.",
    });
    await requestGracefulAppExit();
    throw error;
  }
}
