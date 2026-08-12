/**
 * Role-dispatch panel — entry, distinct errors, unapproved honesty, governance.
 */
import {
  render,
  screen,
  waitFor,
  within,
} from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it, vi } from "vitest";
import {
  RoleDispatchPanel,
  governanceBadgeLabel,
  outcomeClaimsGoverned,
  selectControllerSlot,
  selectDispatchTargets,
} from "../src/features/conversations/RoleDispatchPanel";
import {
  ROLE_DISPATCH_ERROR_MESSAGES,
  ROLE_DISPATCH_GOVERNED_LABEL,
  ROLE_DISPATCH_PENDING_APPROVAL_NOTICE,
  ROLE_DISPATCH_UNGOVERNED_HINT,
  ROLE_DISPATCH_UNGOVERNED_LABEL,
  RoleDispatchGatewayError,
  type RoleDispatchGateway,
  type RoleDispatchProposalView,
  type RoleDispatchRecordView,
} from "../src/lib/roleDispatchGateway";
import type {
  RuntimeSlotView,
  RuntimeSlotsGateway,
} from "../src/features/settings/runtimeSlotsTypes";
import "../src/styles/role-dispatch.css";

const conversationId = "00000000-0000-0000-0000-0000000000c1";
const projectId = "00000000-0000-0000-0000-0000000000aa";
const controllerId = "00000000-0000-0000-0000-0000000000s1";
const librarianId = "00000000-0000-0000-0000-0000000000s2";
const localCliId = "00000000-0000-0000-0000-0000000000s3";
const proposalId = "00000000-0000-0000-0000-0000000000p1";

function futureExpiry(): string {
  return new Date(Date.now() + 15 * 60_000).toISOString();
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
    modelId: "grok-test",
    secretRef: null,
    secretConfigured: true,
    executablePath: null,
    launchArgs: null,
    statusArgs: null,
    roleCustomName: overrides.roleCustomName ?? null,
    createdAt: "2026-08-12T00:00:00.000Z",
    updatedAt: "2026-08-12T00:00:00.000Z",
    accountLabel: null,
    accessExpiresAt: null,
    ...overrides,
  };
}

const fixtureSlots: RuntimeSlotView[] = [
  slot({
    slotId: controllerId,
    role: "controller",
    displayName: "总控 Claude",
    governance: "full",
  }),
  slot({
    slotId: librarianId,
    role: "librarian",
    displayName: "信息管理员",
    governance: "full",
  }),
  slot({
    slotId: localCliId,
    role: "custom",
    roleCustomName: "写码",
    displayName: "本机 Codex",
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

function pendingProposal(
  overrides: Partial<RoleDispatchProposalView> = {},
): RoleDispatchProposalView {
  return {
    assignmentId: "assign-1",
    initiatorSlotId: controllerId,
    targetSlotId: librarianId,
    targetRole: "librarian",
    targetRoleCustomName: null,
    governance: "full",
    taskText: "整理知识库索引",
    program: "node",
    argv: ["--version"],
    portableCwd: ".",
    callId: "call-1",
    runId: "run-1",
    proposal: {
      proposalId,
      conversationId,
      origin: "agent",
      state: "pending_approval",
      programLabel: "node",
      argv: ["--version"],
      portableCwd: ".",
      riskClass: "standard",
      summary: "Role dispatch",
      expiresAt: futureExpiry(),
      decisionAvailable: true,
      ruleSubject: "node --version",
    },
    ...overrides,
  };
}

function createSlotsGateway(
  slots: RuntimeSlotView[] = fixtureSlots,
): RuntimeSlotsGateway {
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

function createDispatchGateway(
  overrides: Partial<RoleDispatchGateway> = {},
): RoleDispatchGateway & {
  proposeCalls: Array<Record<string, unknown>>;
  listCalls: string[];
} {
  const proposeCalls: Array<Record<string, unknown>> = [];
  const listCalls: string[] = [];
  let records: RoleDispatchRecordView[] = [];
  return {
    proposeCalls,
    listCalls,
    proposeRoleDispatch: async (input) => {
      proposeCalls.push({ ...input });
      if (overrides.proposeRoleDispatch) {
        return overrides.proposeRoleDispatch(input);
      }
      return pendingProposal({
        targetSlotId: input.targetSlotId ?? librarianId,
        taskText: input.taskText,
      });
    },
    listRoleDispatches: async (id) => {
      listCalls.push(id);
      if (overrides.listRoleDispatches) {
        return overrides.listRoleDispatches(id);
      }
      return records.map((r) => ({ ...r }));
    },
    setRecords(next: RoleDispatchRecordView[]) {
      records = next;
    },
  } as RoleDispatchGateway & {
    proposeCalls: Array<Record<string, unknown>>;
    listCalls: string[];
    setRecords: (next: RoleDispatchRecordView[]) => void;
  };
}

async function openPanel() {
  const user = userEvent.setup();
  await user.click(screen.getByTestId("role-dispatch-toggle"));
  await waitFor(() => {
    expect(screen.getByTestId("role-dispatch-form")).toBeInTheDocument();
  });
  return user;
}

describe("selectDispatchTargets / governance helpers", () => {
  it("excludes controller from targets and keeps ungoverned custom slots", () => {
    const targets = selectDispatchTargets(fixtureSlots);
    expect(targets.map((t) => t.slotId)).toEqual([librarianId, localCliId]);
    expect(selectControllerSlot(fixtureSlots)?.slotId).toBe(controllerId);
    expect(governanceBadgeLabel("full")).toBe(ROLE_DISPATCH_GOVERNED_LABEL);
    expect(governanceBadgeLabel("none")).toBe(ROLE_DISPATCH_UNGOVERNED_LABEL);
    expect(
      outcomeClaimsGoverned({
        governance: "none",
        claimedGovernedExecution: true,
      }),
    ).toBe(false);
    expect(
      outcomeClaimsGoverned({
        governance: "full",
        claimedGovernedExecution: true,
      }),
    ).toBe(true);
  });
});

describe("RoleDispatchPanel", () => {
  it("submits propose and shows pending-approval notice — never 已派出/已执行", async () => {
    const onProposed = vi.fn();
    const dispatch = createDispatchGateway();
    render(
      <RoleDispatchPanel
        conversationId={conversationId}
        onProposed={onProposed}
        projectId={projectId}
        roleDispatchGateway={dispatch}
        runtimeSlotsGateway={createSlotsGateway()}
      />,
    );

    const user = await openPanel();
    const task = screen.getByTestId("role-dispatch-task");
    await user.clear(task);
    await user.type(task, "整理知识库索引");
    await user.click(screen.getByTestId("role-dispatch-submit"));

    await waitFor(() => {
      expect(screen.getByTestId("role-dispatch-pending-notice")).toHaveTextContent(
        ROLE_DISPATCH_PENDING_APPROVAL_NOTICE,
      );
    });

    expect(dispatch.proposeCalls).toHaveLength(1);
    expect(dispatch.proposeCalls[0]).toMatchObject({
      conversationId,
      initiatorSlotId: controllerId,
      targetSlotId: librarianId,
      taskText: "整理知识库索引",
    });
    expect(onProposed).toHaveBeenCalledTimes(1);
    expect(onProposed.mock.calls[0]?.[0]?.proposal.state).toBe("pending_approval");

    const body = document.body.textContent ?? "";
    expect(body).not.toMatch(/已派出|已执行|执行成功|正在运行目标/);
    // No second approve/reject in this panel.
    expect(screen.queryByTestId("command-approval-approve")).not.toBeInTheDocument();
    expect(screen.queryByTestId("command-approval-reject")).not.toBeInTheDocument();
    expect(screen.getByTestId("role-dispatch-no-second-approval")).toBeInTheDocument();
  });

  it("shows ungoverned badge and hint when LocalCli target is selected", async () => {
    render(
      <RoleDispatchPanel
        conversationId={conversationId}
        projectId={projectId}
        roleDispatchGateway={createDispatchGateway()}
        runtimeSlotsGateway={createSlotsGateway()}
      />,
    );
    const user = await openPanel();
    await user.selectOptions(
      screen.getByTestId("role-dispatch-target"),
      localCliId,
    );

    expect(screen.getByTestId("role-dispatch-target-governance")).toHaveTextContent(
      ROLE_DISPATCH_UNGOVERNED_LABEL,
    );
    expect(screen.getByTestId("role-dispatch-ungoverned-hint")).toHaveTextContent(
      ROLE_DISPATCH_UNGOVERNED_HINT,
    );
    // Must not paint governed badge for this selection.
    expect(screen.getByTestId("role-dispatch-target-governance")).not.toHaveTextContent(
      ROLE_DISPATCH_GOVERNED_LABEL,
    );
  });

  it("shows governed badge for librarian without ungoverned hint", async () => {
    render(
      <RoleDispatchPanel
        conversationId={conversationId}
        projectId={projectId}
        roleDispatchGateway={createDispatchGateway()}
        runtimeSlotsGateway={createSlotsGateway()}
      />,
    );
    await openPanel();
    expect(screen.getByTestId("role-dispatch-target-governance")).toHaveTextContent(
      ROLE_DISPATCH_GOVERNED_LABEL,
    );
    expect(screen.queryByTestId("role-dispatch-ungoverned-hint")).not.toBeInTheDocument();
  });

  it("lists ungoverned outcome rows with exit code and ungoverned label", async () => {
    const dispatch = createDispatchGateway({
      listRoleDispatches: async () => [
        {
          assignmentId: "a1",
          initiatorSlotId: controllerId,
          targetSlotId: localCliId,
          governance: "none",
          proposalId,
          callId: "call-1",
          claimedGovernedExecution: false,
          exitCode: 0,
          durationMs: 12,
        },
        {
          assignmentId: "a2",
          initiatorSlotId: controllerId,
          targetSlotId: librarianId,
          governance: "full",
          proposalId: "p2",
          callId: "call-2",
          claimedGovernedExecution: true,
          exitCode: 1,
          durationMs: 99,
        },
      ],
    });

    render(
      <RoleDispatchPanel
        conversationId={conversationId}
        projectId={projectId}
        roleDispatchGateway={dispatch}
        runtimeSlotsGateway={createSlotsGateway()}
      />,
    );
    await openPanel();

    await waitFor(() => {
      expect(screen.getAllByTestId("role-dispatch-result-row")).toHaveLength(2);
    });

    const rows = screen.getAllByTestId("role-dispatch-result-row");
    const ungoverned = rows.find((row) => row.getAttribute("data-governance") === "none");
    const governed = rows.find((row) => row.getAttribute("data-governance") === "full");
    expect(ungoverned).toBeTruthy();
    expect(governed).toBeTruthy();
    expect(
      within(ungoverned!).getByTestId("role-dispatch-result-governance"),
    ).toHaveTextContent(ROLE_DISPATCH_UNGOVERNED_LABEL);
    expect(
      within(ungoverned!).getByTestId("role-dispatch-result-ungoverned-note"),
    ).toHaveTextContent(ROLE_DISPATCH_UNGOVERNED_HINT);
    expect(
      within(ungoverned!).getByTestId("role-dispatch-result-exit"),
    ).toHaveTextContent("退出 0");
    // Governed row must not use the ungoverned label.
    expect(
      within(governed!).getByTestId("role-dispatch-result-governance"),
    ).toHaveTextContent(ROLE_DISPATCH_GOVERNED_LABEL);
    expect(
      within(governed!).queryByTestId("role-dispatch-result-ungoverned-note"),
    ).not.toBeInTheDocument();
  });

  it("surfaces each role_dispatch_* error with distinct copy (not 派单失败)", async () => {
    const codes = [
      "role_dispatch_self_dispatch",
      "role_dispatch_target_missing",
      "role_dispatch_role_mismatch",
      "role_dispatch_shell_join_forbidden",
      "role_dispatch_task_empty",
      "role_dispatch_unsupported_auth",
    ] as const;

    for (const code of codes) {
      const dispatch = createDispatchGateway({
        proposeRoleDispatch: async () => {
          throw new RoleDispatchGatewayError(
            code,
            ROLE_DISPATCH_ERROR_MESSAGES[code],
          );
        },
      });
      const { unmount } = render(
        <RoleDispatchPanel
          conversationId={conversationId}
          projectId={projectId}
          roleDispatchGateway={dispatch}
          runtimeSlotsGateway={createSlotsGateway()}
        />,
      );
      const user = await openPanel();
      await user.type(screen.getByTestId("role-dispatch-task"), "任务");
      await user.click(screen.getByTestId("role-dispatch-submit"));
      await waitFor(() => {
        expect(screen.getByTestId("role-dispatch-error")).toHaveTextContent(
          ROLE_DISPATCH_ERROR_MESSAGES[code],
        );
      });
      expect(screen.getByTestId("role-dispatch-error").textContent).not.toBe(
        "派单失败",
      );
      expect(screen.queryByTestId("role-dispatch-pending-notice")).not.toBeInTheDocument();
      unmount();
    }
  });

  it("does not offer controller as a target option", async () => {
    render(
      <RoleDispatchPanel
        conversationId={conversationId}
        projectId={projectId}
        roleDispatchGateway={createDispatchGateway()}
        runtimeSlotsGateway={createSlotsGateway()}
      />,
    );
    await openPanel();
    const select = screen.getByTestId("role-dispatch-target") as HTMLSelectElement;
    const values = Array.from(select.options).map((o) => o.value);
    expect(values).not.toContain(controllerId);
    expect(values).toContain(librarianId);
    expect(values).toContain(localCliId);
  });

  it("shows guidance when only controller exists (no silent empty submit)", async () => {
    render(
      <RoleDispatchPanel
        conversationId={conversationId}
        projectId={projectId}
        roleDispatchGateway={createDispatchGateway()}
        runtimeSlotsGateway={createSlotsGateway([
          slot({
            slotId: controllerId,
            role: "controller",
            displayName: "总控",
          }),
        ])}
      />,
    );
    await openPanel();
    expect(screen.getByTestId("role-dispatch-no-targets")).toBeInTheDocument();
    expect(screen.getByTestId("role-dispatch-submit")).toBeDisabled();
  });
});
