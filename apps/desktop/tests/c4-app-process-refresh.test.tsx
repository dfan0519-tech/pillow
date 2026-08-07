/**
 * C4 D3B2 — App process refresh composition.
 * Event-driven execProposalRefreshToken only; no polling or process cache.
 */
import { act, render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type {
  ConversationGateway,
  RuntimeEventDto,
} from "../src/lib/conversationGateway";
import type { ExecGateway } from "../src/lib/execGateway";
import {
  ProjectGatewayError,
  type ProjectDto,
  type ProjectGateway,
} from "../src/lib/projectGateway";

type CapturedWorkbenchProps = {
  execGateway?: ExecGateway;
  execProposalRefreshToken?: number;
  writeProposalRefreshToken?: number;
};

const workbenchPropSnapshots: CapturedWorkbenchProps[] = [];

vi.mock("../src/features/workbench/Workbench", () => ({
  Workbench: (props: CapturedWorkbenchProps) => {
    workbenchPropSnapshots.push({
      execGateway: props.execGateway,
      execProposalRefreshToken: props.execProposalRefreshToken,
      writeProposalRefreshToken: props.writeProposalRefreshToken,
    });
    return <div data-testid="mock-workbench" />;
  },
}));

// Import after Workbench mock so App receives the capture stub.
const { App } = await import("../src/App");

vi.mock("@tauri-apps/api/window", () => ({
  getCurrentWindow: () => ({
    minimize: vi.fn().mockResolvedValue(undefined),
    toggleMaximize: vi.fn().mockResolvedValue(undefined),
    close: vi.fn().mockResolvedValue(undefined),
    isMaximized: vi.fn().mockResolvedValue(false),
    onResized: vi.fn().mockResolvedValue(vi.fn()),
  }),
}));

const projectAId = "00000000-0000-0000-0000-0000000000aa";
const projectBId = "00000000-0000-0000-0000-0000000000bb";
const convAId = "00000000-0000-0000-0000-0000000000c1";
const convBId = "00000000-0000-0000-0000-0000000000c2";
const convForeignId = "00000000-0000-0000-0000-0000000000cf";

const PROCESS_KINDS = [
  "process_proposal_requested",
  "process_proposal_decided",
  "process_started",
  "process_terminal_outcome",
] as const;

function fixtureProject(overrides: Partial<ProjectDto> = {}): ProjectDto {
  return {
    id: projectAId,
    name: "Project A",
    rootPath: "F:\\work\\project-a",
    trust: "trusted",
    createdAt: "2026-08-04T00:00:00.000Z",
    updatedAt: "2026-08-04T00:00:00.000Z",
    ...overrides,
  };
}

function fixtureProjectGateway(
  projects: ProjectDto[] = [fixtureProject()],
): ProjectGateway {
  const store = [...projects];
  return {
    listProjects: async () => [...store],
    createProject: async (input) => {
      const project = fixtureProject({
        id: crypto.randomUUID(),
        name: input.name,
        rootPath: input.rootPath,
      });
      store.push(project);
      return project;
    },
    selectProject: async (projectId) => {
      const project = store.find((item) => item.id === projectId);
      if (!project) {
        throw new ProjectGatewayError("project_not_found", "Project not found.");
      }
      return project;
    },
    pickDirectory: async () => null,
    openOrRegisterProject: async () => fixtureProject(),
  };
}

function processPayload(
  kind: (typeof PROCESS_KINDS)[number],
): RuntimeEventDto["payload"] {
  switch (kind) {
    case "process_proposal_requested":
      return {
        proposal_id: "process-proposal-1",
        program: "node",
        argv: ["--version"],
        portable_cwd: ".",
        summary: "Run node version",
        risk_class: "standard",
        expires_at: "2026-08-04T12:10:00.000Z",
      };
    case "process_proposal_decided":
      return {
        proposal_id: "process-proposal-1",
        program: "node",
        decision: "approve_once",
        state: "approved",
      };
    case "process_started":
      return {
        proposal_id: "process-proposal-1",
        program: "node",
        argv: ["--version"],
        portable_cwd: ".",
      };
    case "process_terminal_outcome":
      return {
        proposal_id: "process-proposal-1",
        program: "node",
        state: "succeeded",
        exit_code: 0,
        duration_ms: 12,
        truncated: false,
        encoding_lossy: false,
        captured_bytes: 8,
        cancelled: false,
        timed_out: false,
      };
  }
}

function processEvent(
  kind: (typeof PROCESS_KINDS)[number],
  conversationId: string,
  sequence: number,
): RuntimeEventDto {
  return {
    eventId: `ev-${kind}-${sequence}`,
    runId: "run-1",
    conversationId,
    sequence,
    createdAt: `2026-08-04T12:00:${String(sequence).padStart(2, "0")}.000Z`,
    kind,
    payload: processPayload(kind),
  } as RuntimeEventDto;
}

function writeEvent(
  kind: "write_proposal_requested" | "write_proposal_resolved",
  conversationId: string,
  sequence: number,
): RuntimeEventDto {
  if (kind === "write_proposal_requested") {
    return {
      eventId: `ev-write-req-${sequence}`,
      runId: "run-1",
      conversationId,
      sequence,
      createdAt: `2026-08-04T12:00:${String(sequence).padStart(2, "0")}.000Z`,
      kind,
      payload: {
        proposal_id: "write-proposal-1",
        path: "src/example.ts",
        mode: "create",
        summary: "Create example",
        expires_at: "2026-08-04T12:10:00.000Z",
      },
    };
  }
  return {
    eventId: `ev-write-res-${sequence}`,
    runId: "run-1",
    conversationId,
    sequence,
    createdAt: `2026-08-04T12:00:${String(sequence).padStart(2, "0")}.000Z`,
    kind,
    payload: {
      proposal_id: "write-proposal-1",
      path: "src/example.ts",
      mode: "create",
      state: "succeeded",
    },
  };
}

function createConversationGateway(options: {
  projectAConversations?: Array<{ id: string; title: string }>;
  projectBConversations?: Array<{ id: string; title: string }>;
}): {
  gateway: ConversationGateway;
  listeners: Set<(event: RuntimeEventDto) => void>;
  emit: (event: RuntimeEventDto) => void;
} {
  const listeners = new Set<(event: RuntimeEventDto) => void>();
  const projectAConversations = options.projectAConversations ?? [
    { id: convAId, title: "对话 A" },
  ];
  const projectBConversations = options.projectBConversations ?? [
    { id: convBId, title: "对话 B" },
  ];

  const gateway: ConversationGateway = {
    listConversations: async (projectId) => {
      if (projectId === projectAId) {
        return projectAConversations.map((item) => ({
          id: item.id,
          projectId: projectAId,
          title: item.title,
          createdAt: "2026-08-04T00:00:00.000Z",
          updatedAt: "2026-08-04T00:00:00.000Z",
        }));
      }
      if (projectId === projectBId) {
        return projectBConversations.map((item) => ({
          id: item.id,
          projectId: projectBId,
          title: item.title,
          createdAt: "2026-08-04T00:00:00.000Z",
          updatedAt: "2026-08-04T00:00:00.000Z",
        }));
      }
      return [];
    },
    createConversation: async (projectId) => ({
      id: convAId,
      projectId,
      title: "对话 A",
      createdAt: "2026-08-04T00:00:00.000Z",
      updatedAt: "2026-08-04T00:00:00.000Z",
    }),
    loadTimeline: async (conversationId) => ({
      conversationId,
      messages: [
        {
          id: "u1",
          role: "user",
          content: "run please",
          createdAt: "2026-08-04T00:00:00.000Z",
        },
      ],
      events: [],
      runs: [
        {
          runId: "run-1",
          conversationId,
          status: "running",
        },
      ],
    }),
    sendTurn: async () => ({ runId: "run-1" }),
    cancelRun: async () => undefined,
    subscribe: (listener) => {
      listeners.add(listener);
      return () => {
        listeners.delete(listener);
      };
    },
  };

  return {
    gateway,
    listeners,
    emit: (event) => {
      for (const listener of listeners) {
        listener(event);
      }
    },
  };
}

function stubExecGateway(): ExecGateway {
  return {
    listExecProposals: async () => [],
    decideExecProposal: async () => {
      throw new Error("decide not used in App refresh tests");
    },
    openExecOutput: async () => {
      throw new Error("openExecOutput not used in App refresh tests");
    },
  };
}

function latestProps(): CapturedWorkbenchProps {
  const last = workbenchPropSnapshots[workbenchPropSnapshots.length - 1];
  if (!last) {
    throw new Error("Workbench was not rendered.");
  }
  return last;
}

async function selectProjectAndConversation(
  projectName: string,
  conversationTitle: string,
) {
  const user = userEvent.setup();
  await waitFor(() => {
    expect(screen.getByText(projectName)).toBeInTheDocument();
  });
  await user.click(screen.getByText(projectName));
  await waitFor(() => {
    expect(screen.getByText(conversationTitle)).toBeInTheDocument();
  });
  await user.click(screen.getByText(conversationTitle));
  await waitFor(() => {
    expect(screen.getByTestId("mock-workbench")).toBeInTheDocument();
  });
}

describe("C4 D3B2 App process refresh composition", () => {
  beforeEach(() => {
    workbenchPropSnapshots.length = 0;
  });

  afterEach(() => {
    workbenchPropSnapshots.length = 0;
  });

  it("passes execGateway and starts execProposalRefreshToken at 0", async () => {
    const { gateway } = createConversationGateway({});
    const execGateway = stubExecGateway();
    render(
      <App
        conversationGateway={gateway}
        execGateway={execGateway}
        gateway={fixtureProjectGateway()}
      />,
    );
    await selectProjectAndConversation("Project A", "对话 A");
    const props = latestProps();
    expect(props.execGateway).toBe(execGateway);
    expect(props.execProposalRefreshToken).toBe(0);
    expect(props.writeProposalRefreshToken).toBe(0);
  });

  it("bumps execProposalRefreshToken once per accepted process lifecycle kind for the selected conversation", async () => {
    const { gateway, emit } = createConversationGateway({});
    const execGateway = stubExecGateway();
    render(
      <App
        conversationGateway={gateway}
        execGateway={execGateway}
        gateway={fixtureProjectGateway()}
      />,
    );
    await selectProjectAndConversation("Project A", "对话 A");

    let sequence = 1;
    for (const kind of PROCESS_KINDS) {
      const before = latestProps().execProposalRefreshToken ?? 0;
      const writeBefore = latestProps().writeProposalRefreshToken ?? 0;
      act(() => {
        emit(processEvent(kind, convAId, sequence));
      });
      sequence += 1;
      await waitFor(() => {
        expect(latestProps().execProposalRefreshToken).toBe(before + 1);
      });
      expect(latestProps().writeProposalRefreshToken).toBe(writeBefore);
    }

    expect(latestProps().execProposalRefreshToken).toBe(4);
  });

  it("does not bump for a foreign conversation under the same project", async () => {
    const { gateway, emit } = createConversationGateway({
      projectAConversations: [
        { id: convAId, title: "对话 A" },
        { id: convForeignId, title: "对话 Foreign" },
      ],
    });
    render(
      <App
        conversationGateway={gateway}
        execGateway={stubExecGateway()}
        gateway={fixtureProjectGateway()}
      />,
    );
    await selectProjectAndConversation("Project A", "对话 A");
    const before = latestProps().execProposalRefreshToken ?? 0;

    act(() => {
      emit(processEvent("process_proposal_requested", convForeignId, 10));
      emit(processEvent("process_proposal_decided", convForeignId, 11));
      emit(processEvent("process_started", convForeignId, 12));
      emit(processEvent("process_terminal_outcome", convForeignId, 13));
    });

    await act(async () => {
      await Promise.resolve();
    });
    expect(latestProps().execProposalRefreshToken).toBe(before);
  });

  it("does not bump for a foreign project conversation", async () => {
    const projects = [
      fixtureProject(),
      fixtureProject({
        id: projectBId,
        name: "Project B",
        rootPath: "F:\\work\\project-b",
      }),
    ];
    const { gateway, emit } = createConversationGateway({
      projectAConversations: [{ id: convAId, title: "对话 A" }],
      projectBConversations: [{ id: convBId, title: "对话 B" }],
    });
    render(
      <App
        conversationGateway={gateway}
        execGateway={stubExecGateway()}
        gateway={fixtureProjectGateway(projects)}
      />,
    );
    await selectProjectAndConversation("Project A", "对话 A");
    const before = latestProps().execProposalRefreshToken ?? 0;

    act(() => {
      emit(processEvent("process_started", convBId, 20));
      emit(processEvent("process_terminal_outcome", convBId, 21));
    });

    await act(async () => {
      await Promise.resolve();
    });
    expect(latestProps().execProposalRefreshToken).toBe(before);
  });

  it("preserves write refresh and isolates write vs process tokens", async () => {
    const { gateway, emit } = createConversationGateway({});
    render(
      <App
        conversationGateway={gateway}
        execGateway={stubExecGateway()}
        gateway={fixtureProjectGateway()}
      />,
    );
    await selectProjectAndConversation("Project A", "对话 A");

    const execBefore = latestProps().execProposalRefreshToken ?? 0;
    const writeBefore = latestProps().writeProposalRefreshToken ?? 0;

    act(() => {
      emit(writeEvent("write_proposal_requested", convAId, 30));
    });
    await waitFor(() => {
      expect(latestProps().writeProposalRefreshToken).toBe(writeBefore + 1);
    });
    expect(latestProps().execProposalRefreshToken).toBe(execBefore);

    const writeAfterWrite = latestProps().writeProposalRefreshToken ?? 0;
    act(() => {
      emit(writeEvent("write_proposal_resolved", convAId, 31));
    });
    await waitFor(() => {
      expect(latestProps().writeProposalRefreshToken).toBe(writeAfterWrite + 1);
    });
    expect(latestProps().execProposalRefreshToken).toBe(execBefore);

    const execAfterWrites = latestProps().execProposalRefreshToken ?? 0;
    const writeAfterWrites = latestProps().writeProposalRefreshToken ?? 0;
    act(() => {
      emit(processEvent("process_proposal_requested", convAId, 32));
    });
    await waitFor(() => {
      expect(latestProps().execProposalRefreshToken).toBe(execAfterWrites + 1);
    });
    expect(latestProps().writeProposalRefreshToken).toBe(writeAfterWrites);
  });
});
