/**
 * Dispatch as a first-class conversation-timeline event.
 * Asserts what the user can see — not only projection return values.
 * Written against the pre-change timeline (no dispatch/assignment render).
 */
import { render, screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it } from "vitest";
import { ConversationTimeline } from "../src/features/conversations/ConversationTimeline";
import {
  dispatchViewFromProposal,
  dispatchViewFromRecord,
} from "../src/features/conversations/agentActivityProjection";
import type {
  ConversationDispatchView,
  ConversationTimelineView,
  RunActivityView,
} from "../src/features/conversations/conversationTypes";
import type { RuntimeEventDto } from "../src/lib/conversationGateway";
import type { RoleDispatchProposalView } from "../src/lib/roleDispatchGateway";
import "../src/styles/tokens.css";
import "../src/styles/workbench.css";
import "../src/styles/dispatch-timeline.css";

const conversationId = "conv-dispatch-owner-aaaa";
const RAW_ASSIGNMENT_ID = "assign-raw-never-show-zzzz";
const RAW_INITIATOR_SLOT = "slot-controller-raw-never-show";
const RAW_TARGET_SLOT = "slot-librarian-raw-never-show";
const RAW_PROPOSAL_ID = "proposal-raw-never-show-yyyy";
const RAW_CALL_ID = "call-raw-never-show-xxxx";
const TASK_TEXT = "整理知识库索引并标出过期条目";

type DispatchFixture = ConversationDispatchView;

function emptyTimeline(
  overrides: Partial<ConversationTimelineView> = {},
): ConversationTimelineView {
  return {
    conversationId,
    messages: [
      {
        id: "m-user-1",
        role: "user",
        content: "请把索引整理任务派给信息管理员",
        createdAt: "2026-08-13T10:00:00.000Z",
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
    ...overrides,
  };
}

function proposedDispatch(
  overrides: Partial<DispatchFixture> = {},
): DispatchFixture {
  return {
    assignmentId: RAW_ASSIGNMENT_ID,
    initiatorSlotId: RAW_INITIATOR_SLOT,
    targetSlotId: RAW_TARGET_SLOT,
    targetRole: "librarian",
    targetRoleCustomName: null,
    taskText: TASK_TEXT,
    governance: "full",
    proposalId: RAW_PROPOSAL_ID,
    callId: RAW_CALL_ID,
    claimedGovernedExecution: null,
    exitCode: null,
    durationMs: null,
    initiatorLabel: "总控",
    proposalState: "pending_approval",
    ...overrides,
  };
}

function timelineWithDispatches(
  dispatches: DispatchFixture[],
  overrides: Partial<ConversationTimelineView> = {},
): ConversationTimelineView {
  return {
    ...emptyTimeline(overrides),
    dispatches,
  };
}

function collectAttributeValues(root: ParentNode): string[] {
  const values: string[] = [];
  const walk = (node: Element) => {
    for (const attr of Array.from(node.attributes)) {
      values.push(attr.value);
    }
    for (const child of Array.from(node.children)) {
      walk(child);
    }
  };
  if (root instanceof Element) walk(root);
  else {
    for (const child of Array.from(root.children ?? [])) {
      walk(child as Element);
    }
  }
  return values;
}

function assertNoInternalIds(root: HTMLElement = document.body) {
  const text = root.textContent ?? "";
  const attrs = collectAttributeValues(root).join("\n");
  const html = root.innerHTML;
  for (const secret of [
    RAW_ASSIGNMENT_ID,
    RAW_INITIATOR_SLOT,
    RAW_TARGET_SLOT,
    RAW_PROPOSAL_ID,
    RAW_CALL_ID,
  ]) {
    expect(text).not.toContain(secret);
    expect(attrs).not.toContain(secret);
    expect(html).not.toContain(secret);
  }
}

function processEvent(
  kind: RuntimeEventDto["kind"],
  sequence: number,
  payload: Record<string, unknown>,
): RuntimeEventDto {
  return {
    eventId: `evt-${kind}-${sequence}`,
    runId: "run-dispatch-process",
    conversationId,
    sequence,
    createdAt: `2026-08-13T10:00:${String(sequence).padStart(2, "0")}.000Z`,
    kind,
    payload,
  } as RuntimeEventDto;
}

describe("ConversationTimeline dispatch entries", () => {
  it("renders a visible dispatch entry with target role and task summary after propose", () => {
    render(
      <ConversationTimeline
        run={null}
        timeline={timelineWithDispatches([proposedDispatch()])}
      />,
    );

    const row = screen.getByTestId("dispatch-timeline-row");
    expect(row).toBeVisible();
    expect(row).toHaveTextContent("信息管理员");
    expect(row).toHaveTextContent(TASK_TEXT);
    expect(screen.getByTestId("dispatch-timeline-target")).toHaveTextContent(
      "信息管理员",
    );
    expect(screen.getByTestId("dispatch-timeline-task")).toHaveTextContent(
      TASK_TEXT,
    );
  });

  it("shows initiator, target, collapsible task, and a real pending-approval step", async () => {
    const user = userEvent.setup();
    render(
      <ConversationTimeline
        run={null}
        timeline={timelineWithDispatches([proposedDispatch()])}
      />,
    );

    const row = screen.getByTestId("dispatch-timeline-row");
    expect(screen.getByTestId("dispatch-timeline-kind")).toHaveTextContent(
      "派单",
    );
    expect(screen.getByTestId("dispatch-timeline-initiator")).toHaveTextContent(
      "总控",
    );
    expect(screen.getByTestId("dispatch-timeline-target")).toHaveTextContent(
      "信息管理员",
    );
    expect(screen.getByTestId("dispatch-timeline-status")).toHaveTextContent(
      "待批准",
    );
    expect(screen.getByTestId("dispatch-timeline-status")).not.toHaveTextContent(
      "执行中",
    );

    const details = within(row).getByTestId("dispatch-timeline-task-details");
    expect(details).not.toHaveAttribute("open");
    await user.click(within(row).getByTestId("dispatch-timeline-task-summary"));
    expect(details).toHaveAttribute("open");
    expect(screen.getByTestId("dispatch-timeline-task")).toHaveTextContent(
      TASK_TEXT,
    );
  });

  it("does not invent target role or task from a sparse list record", () => {
    render(
      <ConversationTimeline
        run={null}
        timeline={timelineWithDispatches([
          proposedDispatch({
            targetRole: null,
            targetRoleCustomName: null,
            taskText: null,
            initiatorLabel: null,
            proposalState: null,
            proposalId: RAW_PROPOSAL_ID,
          }),
        ])}
      />,
    );

    const row = screen.getByTestId("dispatch-timeline-row");
    expect(row).toBeVisible();
    expect(row).not.toHaveTextContent("信息管理员");
    expect(row).not.toHaveTextContent(TASK_TEXT);
    expect(screen.queryByTestId("dispatch-timeline-target")).not.toBeInTheDocument();
    expect(screen.queryByTestId("dispatch-timeline-task")).not.toBeInTheDocument();
    expect(screen.queryByTestId("dispatch-timeline-initiator")).not.toBeInTheDocument();
    expect(screen.queryByTestId("dispatch-timeline-status")).not.toBeInTheDocument();

    const notes = screen.getAllByTestId("dispatch-timeline-unavailable");
    const joined = notes.map((node) => node.textContent ?? "").join("\n");
    expect(joined).toMatch(/目标角色/);
    expect(joined).toMatch(/任务描述/);
    expect(joined).toMatch(/当前步骤|状态/);
    expect(joined).not.toMatch(/同时工作|并行运行/);
  });

  it("joins a real process lifecycle instead of fabricating 执行中", () => {
    const events = [
      processEvent("process_proposal_requested", 1, {
        proposal_id: RAW_PROPOSAL_ID,
        program: "node",
        argv: ["--version"],
        portable_cwd: ".",
        summary: "Role dispatch",
        risk_class: "standard",
        expires_at: "2026-08-13T10:15:00.000Z",
      }),
      processEvent("process_proposal_decided", 2, {
        proposal_id: RAW_PROPOSAL_ID,
        program: "node",
        decision: "approve_once",
        state: "approved",
      }),
      processEvent("process_started", 3, {
        proposal_id: RAW_PROPOSAL_ID,
        program: "node",
        argv: ["--version"],
        portable_cwd: ".",
      }),
    ];

    render(
      <ConversationTimeline
        run={null}
        timeline={timelineWithDispatches(
          [proposedDispatch({ proposalState: "pending_approval" })],
          { events },
        )}
      />,
    );

    expect(screen.getByTestId("dispatch-timeline-status")).toHaveTextContent(
      "执行中",
    );
    expect(screen.getByTestId("dispatch-timeline-status")).not.toHaveTextContent(
      "待批准",
    );
  });

  it("shows 完成 from a real process terminal, not from animation", () => {
    const events = [
      processEvent("process_proposal_requested", 1, {
        proposal_id: RAW_PROPOSAL_ID,
        program: "node",
        argv: ["--version"],
        portable_cwd: ".",
        summary: "Role dispatch",
        risk_class: "standard",
        expires_at: "2026-08-13T10:15:00.000Z",
      }),
      processEvent("process_terminal_outcome", 2, {
        proposal_id: RAW_PROPOSAL_ID,
        program: "node",
        state: "succeeded",
        exit_code: 0,
        duration_ms: 40,
      }),
    ];

    render(
      <ConversationTimeline
        run={null}
        timeline={timelineWithDispatches([proposedDispatch()], { events })}
      />,
    );

    expect(screen.getByTestId("dispatch-timeline-status")).toHaveTextContent(
      "完成",
    );
  });

  it("shows 已拒绝 from a real process decision", () => {
    const events = [
      processEvent("process_proposal_requested", 1, {
        proposal_id: RAW_PROPOSAL_ID,
        program: "node",
        argv: ["--version"],
        portable_cwd: ".",
        summary: "Role dispatch",
        risk_class: "standard",
        expires_at: "2026-08-13T10:15:00.000Z",
      }),
      processEvent("process_proposal_decided", 2, {
        proposal_id: RAW_PROPOSAL_ID,
        program: "node",
        decision: "reject",
        state: "rejected",
      }),
    ];

    render(
      <ConversationTimeline
        run={null}
        timeline={timelineWithDispatches([proposedDispatch()], { events })}
      />,
    );

    expect(screen.getByTestId("dispatch-timeline-status")).toHaveTextContent(
      "已拒绝",
    );
  });

  it("is narrative only — no second approval controls on the dispatch row", () => {
    render(
      <ConversationTimeline
        run={null}
        timeline={timelineWithDispatches([proposedDispatch()])}
      />,
    );

    const row = screen.getByTestId("dispatch-timeline-row");
    expect(
      within(row).queryByRole("button", { name: /批准|拒绝/ }),
    ).not.toBeInTheDocument();
    expect(within(row).queryByTestId("command-approval-bar")).not.toBeInTheDocument();
    expect(
      screen.getByTestId("dispatch-timeline-no-approval"),
    ).toBeInTheDocument();
    expect(row.textContent ?? "").not.toMatch(/同时工作|并行运行/);
  });

  it("never paints assignment, slot, proposal, or call ids", () => {
    render(
      <ConversationTimeline
        run={null}
        timeline={timelineWithDispatches([proposedDispatch()])}
      />,
    );

    assertNoInternalIds(screen.getByTestId("conversation-timeline"));
  });

  it("paints a custom role name when that is what the host gave", () => {
    render(
      <ConversationTimeline
        run={null}
        timeline={timelineWithDispatches([
          proposedDispatch({
            targetRole: "custom",
            targetRoleCustomName: "写码",
            initiatorLabel: "总控",
          }),
        ])}
      />,
    );

    expect(screen.getByTestId("dispatch-timeline-target")).toHaveTextContent(
      "写码",
    );
    expect(screen.getByTestId("dispatch-timeline-target")).not.toHaveTextContent(
      "信息管理员",
    );
  });

  it("does not drop existing messages when a dispatch row is present", () => {
    const activity: RunActivityView = {
      runId: "run-keep-existing",
      status: "completed",
      startedAt: "2026-08-13T10:00:01.000Z",
      updatedAt: "2026-08-13T10:00:02.000Z",
      assistantText: "",
      reasoningText: "",
      plan: null,
      tools: [],
      subagents: [],
      artifacts: [],
    };
    render(
      <ConversationTimeline
        run={null}
        timeline={timelineWithDispatches([proposedDispatch()], {
          activitiesByRunId: { [activity.runId]: activity },
        })}
      />,
    );

    expect(screen.getByTestId("timeline-user-message")).toHaveTextContent(
      "请把索引整理任务派给信息管理员",
    );
    expect(screen.getByTestId("dispatch-timeline-row")).toBeVisible();
  });

  it("lifts a propose DTO and paints target role plus task on the timeline", () => {
    const proposal: RoleDispatchProposalView = {
      assignmentId: RAW_ASSIGNMENT_ID,
      initiatorSlotId: RAW_INITIATOR_SLOT,
      targetSlotId: RAW_TARGET_SLOT,
      targetRole: "librarian",
      targetRoleCustomName: null,
      governance: "full",
      taskText: TASK_TEXT,
      program: "node",
      argv: ["--version"],
      portableCwd: ".",
      callId: RAW_CALL_ID,
      runId: "run-raw-never-show",
      proposal: {
        proposalId: RAW_PROPOSAL_ID,
        conversationId,
        origin: "agent",
        state: "pending_approval",
        programLabel: "node",
        argv: ["--version"],
        portableCwd: ".",
        riskClass: "standard",
        summary: "Role dispatch",
        expiresAt: "2026-08-13T10:15:00.000Z",
        decisionAvailable: true,
        ruleSubject: "node --version",
      },
    };

    render(
      <ConversationTimeline
        dispatches={[
          dispatchViewFromProposal(proposal, { initiatorLabel: "总控" }),
        ]}
        run={null}
        timeline={emptyTimeline()}
      />,
    );

    const row = screen.getByTestId("dispatch-timeline-row");
    expect(row).toHaveTextContent("信息管理员");
    expect(row).toHaveTextContent(TASK_TEXT);
    expect(row).toHaveTextContent("总控");
    expect(row).toHaveTextContent("待批准");
    expect(row.textContent ?? "").not.toContain("run-raw-never-show");
  });

  it("list-record DTO alone cannot invent a target role or task", () => {
    render(
      <ConversationTimeline
        dispatches={[
          dispatchViewFromRecord({
            assignmentId: RAW_ASSIGNMENT_ID,
            initiatorSlotId: RAW_INITIATOR_SLOT,
            targetSlotId: RAW_TARGET_SLOT,
            governance: "full",
            proposalId: RAW_PROPOSAL_ID,
            callId: RAW_CALL_ID,
            claimedGovernedExecution: null,
            exitCode: null,
            durationMs: null,
          }),
        ]}
        run={null}
        timeline={emptyTimeline()}
      />,
    );

    const row = screen.getByTestId("dispatch-timeline-row");
    expect(row).not.toHaveTextContent("信息管理员");
    expect(row).not.toHaveTextContent(TASK_TEXT);
    expect(screen.queryByTestId("dispatch-timeline-target")).not.toBeInTheDocument();
    expect(screen.queryByTestId("dispatch-timeline-task")).not.toBeInTheDocument();
  });
});
