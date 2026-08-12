/**
 * T2: Role-dispatch propose surfaces on the **existing** CommandApprovalBar.
 * No second approval UI. Unapproved propose does not claim execution.
 */
import {
  render,
  screen,
  waitFor,
  within,
} from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it, vi } from "vitest";
import { Workbench } from "../src/features/workbench/Workbench";
import type {
  ConversationTimelineView,
} from "../src/features/conversations/conversationReducer";
import type {
  ExecGateway,
  ExecProposalView,
} from "../src/lib/execGateway";
import type { ProjectDto } from "../src/lib/projectGateway";
import {
  ROLE_DISPATCH_PENDING_APPROVAL_NOTICE,
  ROLE_DISPATCH_UNGOVERNED_LABEL,
  type RoleDispatchGateway,
  type RoleDispatchProposalView,
  type RoleDispatchRecordView,
} from "../src/lib/roleDispatchGateway";
import type {
  RuntimeSlotView,
  RuntimeSlotsGateway,
} from "../src/features/settings/runtimeSlotsTypes";
import "../src/styles/tokens.css";
import "../src/styles/workbench.css";
import "../src/styles/execution.css";
import "../src/styles/role-dispatch.css";

const projectId = "00000000-0000-0000-0000-0000000000aa";
const conversationId = "00000000-0000-0000-0000-0000000000c1";
const controllerId = "00000000-0000-0000-0000-0000000000s1";
const workerId = "00000000-0000-0000-0000-0000000000s2";
const processProposalId = "proc-dispatch-bbbbbbbb";

function futureExpiry(minutes = 10): string {
  return new Date(Date.now() + minutes * 60_000).toISOString();
}

function fixtureProject(): ProjectDto {
  return {
    id: projectId,
    name: "Project A",
    rootPath: "F:\\secret\\projects\\alpha",
    trust: "trusted",
    createdAt: "2026-08-01T00:00:00.000Z",
    updatedAt: "2026-08-01T00:00:00.000Z",
  };
}

function emptyTimeline(): ConversationTimelineView {
  return {
    conversationId,
    messages: [
      {
        id: "m1",
        role: "user",
        content: "请派单给写码角色",
        createdAt: "2026-08-12T12:00:00.000Z",
      },
    ],
    events: [],
    seenEventKeys: new Set(),
    assistantText: "",
    reasoningText: "",
    plan: null,
    tools: [],
    subagents: [],
    activitiesByRunId: {},
    activeRunId: null,
  };
}

function pendingProcess(
  overrides: Partial<ExecProposalView> = {},
): ExecProposalView {
  return {
    proposalId: processProposalId,
    conversationId,
    origin: "agent",
    state: "pending_approval",
    programLabel: "node",
    argv: ["--version", "--pillow-task", "实现派单 UI"],
    portableCwd: ".",
    riskClass: "standard",
    summary: "Role dispatch to worker",
    expiresAt: futureExpiry(10),
    decisionAvailable: true,
    ruleSubject: "node --version --pillow-task 实现派单 UI",
    ...overrides,
  };
}

function slot(
  overrides: Partial<RuntimeSlotView> & Pick<RuntimeSlotView, "slotId" | "role">,
): RuntimeSlotView {
  return {
    projectId,
    displayName: overrides.displayName ?? "Slot",
    kind: overrides.kind ?? "governed_grok",
    governance: overrides.governance ?? "full",
    authMode: overrides.authMode ?? "login",
    baseUrl: null,
    modelId: "grok",
    secretRef: null,
    secretConfigured: true,
    executablePath: null,
    launchArgs: null,
    statusArgs: null,
    roleCustomName: overrides.roleCustomName ?? null,
    createdAt: "2026-08-12T00:00:00.000Z",
    updatedAt: "2026-08-12T00:00:00.000Z",
    ...overrides,
  };
}

function createSlotsGateway(): RuntimeSlotsGateway {
  const slots = [
    slot({
      slotId: controllerId,
      role: "controller",
      displayName: "总控",
    }),
    slot({
      slotId: workerId,
      role: "custom",
      roleCustomName: "写码",
      displayName: "本机 CLI 写码",
      kind: "external_cli",
      governance: "none",
      authMode: null,
      modelId: null,
      secretConfigured: false,
      executablePath: "node",
      launchArgs: ["--version"],
      statusArgs: [],
    }),
  ];
  return {
    listRuntimeSlots: vi.fn(async () => slots.map((s) => ({ ...s }))),
    createSlot: vi.fn(),
    updateSlot: vi.fn(),
    createGrokLoginSlot: vi.fn(),
    createGrokApiKeySlot: vi.fn(),
    updateGrokApiKeySlot: vi.fn(),
    createExternalCliSlot: vi.fn(),
    getExternalCliStatus: vi.fn(),
    startRuntimeSlot: vi.fn(),
    getRuntimeSlotModelCatalog: vi.fn(),
    setRuntimeSlotModel: vi.fn(),
    detectLocalClis: vi.fn(async () => []),
    probeModelCatalog: vi.fn(),
  } as unknown as RuntimeSlotsGateway;
}

function createExecGateway(initial: ExecProposalView[] = []): ExecGateway & {
  listCalls: number;
  decideCalls: Array<Record<string, unknown>>;
  setList: (next: ExecProposalView[]) => void;
} {
  let list = initial.map((p) => ({ ...p }));
  const decideCalls: Array<Record<string, unknown>> = [];
  const gateway = {
    listCalls: 0,
    decideCalls,
    setList(next: ExecProposalView[]) {
      list = next.map((p) => ({ ...p }));
    },
    async listExecProposals(id: string) {
      gateway.listCalls += 1;
      return list.filter((p) => p.conversationId === id).map((p) => ({ ...p }));
    },
    async decideExecProposal(input: {
      conversationId: string;
      proposalId: string;
      decision: "approve_once" | "reject";
    }) {
      decideCalls.push({ ...input });
      list = list.map((p) =>
        p.proposalId === input.proposalId
          ? {
              ...p,
              state:
                input.decision === "approve_once" ? "running" as const : "rejected" as const,
              decisionAvailable: false,
            }
          : p,
      );
      return (
        list.find((p) => p.proposalId === input.proposalId) ?? pendingProcess()
      );
    },
    async openExecOutput() {
      return {
        proposalId: processProposalId,
        state: "succeeded" as const,
        chunks: [{ stream: "stdout" as const, sequence: 0, text: "v22\n" }],
        liveRetained: true,
        truncated: false,
        encodingLossy: false,
        capturedBytes: 4,
        exitCode: 0,
        durationMs: 5,
      };
    },
  };
  return gateway;
}

function createDispatchGateway(
  onPropose: (proposed: RoleDispatchProposalView) => void,
): RoleDispatchGateway {
  const records: RoleDispatchRecordView[] = [];
  return {
    async proposeRoleDispatch(input) {
      const process = pendingProcess({
        summary: `Dispatch: ${input.taskText}`,
        argv: ["--version", "--pillow-task", input.taskText],
      });
      const proposed: RoleDispatchProposalView = {
        assignmentId: "assign-dispatch-1",
        initiatorSlotId: input.initiatorSlotId,
        targetSlotId: input.targetSlotId ?? workerId,
        targetRole: "custom",
        targetRoleCustomName: "写码",
        governance: "none",
        taskText: input.taskText,
        program: "node",
        argv: process.argv,
        portableCwd: ".",
        callId: "call-dispatch-1",
        runId: "run-dispatch-1",
        proposal: process,
      };
      // Host only returns pending — process is not started.
      onPropose(proposed);
      records.push({
        assignmentId: proposed.assignmentId,
        initiatorSlotId: proposed.initiatorSlotId,
        targetSlotId: proposed.targetSlotId,
        governance: "none",
        proposalId: process.proposalId,
        callId: proposed.callId,
        claimedGovernedExecution: false,
        exitCode: null,
        durationMs: null,
      });
      return proposed;
    },
    async listRoleDispatches() {
      return records.map((r) => ({ ...r }));
    },
  };
}

describe("role-dispatch → existing CommandApprovalBar (T2)", () => {
  it("after propose, the process approval bar appears; panel has no approve buttons", async () => {
    const exec = createExecGateway([]);
    const dispatch = createDispatchGateway((proposed) => {
      // Simulate host list after propose (same process proposal table).
      exec.setList([proposed.proposal]);
    });

    render(
      <Workbench
        conversationId={conversationId}
        conversationTitle="派单"
        execGateway={exec}
        onCancelRun={() => {}}
        onOpenProject={() => {}}
        onSendTurn={async () => {}}
        project={fixtureProject()}
        roleDispatchGateway={dispatch}
        runtimeSlotsGateway={createSlotsGateway()}
        timeline={emptyTimeline()}
        run={null}
      />,
    );

    // Initial list is empty — no approval bar yet.
    await waitFor(() => {
      expect(exec.listCalls).toBeGreaterThan(0);
    });
    expect(screen.queryByTestId("command-approval-bar")).not.toBeInTheDocument();

    const user = userEvent.setup();
    await user.click(screen.getByTestId("role-dispatch-toggle"));
    await waitFor(() => {
      expect(screen.getByTestId("role-dispatch-form")).toBeInTheDocument();
    });

    // Target is ungoverned LocalCli — must label before submit.
    expect(screen.getByTestId("role-dispatch-target-governance")).toHaveTextContent(
      ROLE_DISPATCH_UNGOVERNED_LABEL,
    );

    await user.type(screen.getByTestId("role-dispatch-task"), "实现派单 UI");
    await user.click(screen.getByTestId("role-dispatch-submit"));

    await waitFor(() => {
      expect(screen.getByTestId("role-dispatch-pending-notice")).toHaveTextContent(
        ROLE_DISPATCH_PENDING_APPROVAL_NOTICE,
      );
    });

    // Existing bar only — same test id as C4 process path.
    await waitFor(() => {
      expect(screen.getByTestId("command-approval-bar")).toBeInTheDocument();
    });
    expect(screen.getByTestId("command-approval-approve")).toBeInTheDocument();
    expect(screen.getByTestId("command-approval-reject")).toBeInTheDocument();

    // Role-dispatch panel must not host a parallel approve surface.
    const panel = screen.getByTestId("role-dispatch-panel");
    expect(within(panel).queryByTestId("command-approval-approve")).not.toBeInTheDocument();
    expect(within(panel).queryByRole("button", { name: /批准/ })).not.toBeInTheDocument();
    expect(within(panel).queryByRole("button", { name: /拒绝/ })).not.toBeInTheDocument();

    // Unapproved: no false success claim in the stage.
    const stage = screen.getByTestId("workbench").textContent ?? "";
    expect(stage).not.toMatch(/已派出|已执行成功|派单已完成/);
    // Result row exists without exit code (still pending).
    await waitFor(() => {
      expect(screen.getByTestId("role-dispatch-result-row")).toBeInTheDocument();
    });
    expect(screen.getByTestId("role-dispatch-result-pending")).toBeInTheDocument();
    expect(screen.queryByTestId("role-dispatch-result-exit")).not.toBeInTheDocument();
    expect(screen.getByTestId("role-dispatch-result-governance")).toHaveTextContent(
      ROLE_DISPATCH_UNGOVERNED_LABEL,
    );

    // Approve via the **existing** bar only.
    await user.click(screen.getByTestId("command-approval-approve"));
    await waitFor(() => {
      expect(exec.decideCalls.length).toBe(1);
    });
    expect(exec.decideCalls[0]).toMatchObject({
      conversationId,
      proposalId: processProposalId,
      decision: "approve_once",
    });
  });

  it("does not paint command approval when propose fails (self-dispatch)", async () => {
    const exec = createExecGateway([]);
    const dispatch: RoleDispatchGateway = {
      async proposeRoleDispatch() {
        const { RoleDispatchGatewayError, ROLE_DISPATCH_ERROR_MESSAGES } =
          await import("../src/lib/roleDispatchGateway");
        throw new RoleDispatchGatewayError(
          "role_dispatch_self_dispatch",
          ROLE_DISPATCH_ERROR_MESSAGES.role_dispatch_self_dispatch,
        );
      },
      async listRoleDispatches() {
        return [];
      },
    };

    render(
      <Workbench
        conversationId={conversationId}
        execGateway={exec}
        onCancelRun={() => {}}
        onOpenProject={() => {}}
        onSendTurn={async () => {}}
        project={fixtureProject()}
        roleDispatchGateway={dispatch}
        runtimeSlotsGateway={createSlotsGateway()}
        timeline={emptyTimeline()}
        run={null}
      />,
    );

    const user = userEvent.setup();
    await user.click(screen.getByTestId("role-dispatch-toggle"));
    await waitFor(() => {
      expect(screen.getByTestId("role-dispatch-form")).toBeInTheDocument();
    });
    await user.type(screen.getByTestId("role-dispatch-task"), "自派");
    await user.click(screen.getByTestId("role-dispatch-submit"));

    await waitFor(() => {
      expect(screen.getByTestId("role-dispatch-error")).toHaveTextContent(
        "不能把任务派给总控自己",
      );
    });
    expect(screen.queryByTestId("command-approval-bar")).not.toBeInTheDocument();
    expect(screen.queryByTestId("role-dispatch-pending-notice")).not.toBeInTheDocument();
  });
});
