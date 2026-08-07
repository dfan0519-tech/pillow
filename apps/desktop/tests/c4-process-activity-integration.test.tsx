/**
 * C4 D3B2 — compact conversation-owned process command activity.
 * Progressive disclosure from durable RuntimeEventDto process events only.
 * No second dashboard, polling, process cache, or alternate output renderer.
 */
import {
  render,
  screen,
  waitFor,
  within,
} from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it, vi } from "vitest";
import { ConversationTimeline } from "../src/features/conversations/ConversationTimeline";
import type {
  ConversationTimelineView,
  RunActivityView,
} from "../src/features/conversations/conversationTypes";
import { buildProcessActivityModels } from "../src/features/execution/processActivityModel";
import { Workbench } from "../src/features/workbench/Workbench";
import type { RuntimeEventDto } from "../src/lib/conversationGateway";
import type {
  ExecGateway,
  ExecOutputView,
} from "../src/lib/execGateway";
import type { ProjectDto } from "../src/lib/projectGateway";
import "../src/styles/tokens.css";
import "../src/styles/workbench.css";
import "../src/styles/agent-activity.css";
import "../src/styles/execution.css";

const conversationId = "conv-process-owner-aaaa";
const foreignConversationId = "conv-process-foreign-bbbb";
const projectId = "00000000-0000-0000-0000-0000000000aa";
const runId = "run-process-raw-never-show";
const foreignRunId = "run-foreign-raw-never-show";
const processProposalId = "process-proposal-raw-never-show";
const processProposalAltId = "process-proposal-alt-never-show";
const NATIVE_PATH = "F:\\secret\\projects\\alpha\\bin\\tool.exe";
const UNC_PATH = "\\\\fileserver\\share\\tool.exe";
const PORTABLE_ARG = "apps/desktop";
const RAW_GRANT = "abcdefghijklmnopqrstuvwxyz0123456789ABCDEFG";

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

function assertNoSecrets(root: HTMLElement = document.body) {
  const text = root.textContent ?? "";
  const attrs = collectAttributeValues(root).join("\n");
  const html = root.innerHTML;
  for (const secret of [
    processProposalId,
    processProposalAltId,
    runId,
    foreignRunId,
    conversationId,
    foreignConversationId,
    NATIVE_PATH,
    UNC_PATH,
    RAW_GRANT,
    "stdout-digest",
    "stderr-digest",
    "F:\\secret",
    "error_code",
    "session-",
  ]) {
    expect(text).not.toContain(secret);
    expect(attrs).not.toContain(secret);
    expect(html).not.toContain(secret);
  }
  expect(html).not.toMatch(/data-proposal|data-run-id|data-event-id|approvalGrant/i);
}

function baseActivity(
  overrides: Partial<RunActivityView> = {},
): RunActivityView {
  return {
    runId,
    status: "running",
    startedAt: "2026-08-04T00:00:00.000Z",
    updatedAt: "2026-08-04T00:00:05.000Z",
    assistantText: "",
    reasoningText: "",
    plan: null,
    tools: [
      {
        toolCallId: "tool-call-raw-id-xyz",
        name: "fs.read",
        title: "读取配置",
        status: "completed",
        startedAt: "2026-08-04T00:00:01.000Z",
        updatedAt: "2026-08-04T00:00:02.000Z",
        output: "ok",
        resources: [],
      },
    ],
    subagents: [],
    artifacts: [],
    ...overrides,
  };
}

function emptyTimeline(
  overrides: Partial<ConversationTimelineView> = {},
): ConversationTimelineView {
  return {
    conversationId,
    messages: [
      {
        id: "m1",
        role: "user",
        content: "请运行测试",
        createdAt: "2026-08-04T00:00:00.000Z",
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

function timelineWithProcess(
  events: RuntimeEventDto[],
  activity: RunActivityView = baseActivity(),
): ConversationTimelineView {
  return emptyTimeline({
    events,
    activitiesByRunId: { [activity.runId]: activity },
    activeRunId: activity.runId,
  });
}

function processEvent(
  kind: RuntimeEventDto["kind"],
  sequence: number,
  payload: Record<string, unknown>,
  overrides: Partial<Pick<RuntimeEventDto, "runId" | "conversationId" | "eventId">> = {},
): RuntimeEventDto {
  return {
    eventId: overrides.eventId ?? `evt-${kind}-${sequence}`,
    runId: overrides.runId ?? runId,
    conversationId: overrides.conversationId ?? conversationId,
    sequence,
    createdAt: `2026-08-04T00:00:${String(sequence).padStart(2, "0")}.000Z`,
    kind,
    payload,
  } as RuntimeEventDto;
}

/** Distinct portable cwd — must never appear in the command activity DOM. */
const PORTABLE_CWD_ONLY = "crates/tool-executor";

function fullLifecycleEvents(
  proposalId = processProposalId,
  startSequence = 1,
): RuntimeEventDto[] {
  return [
    processEvent("process_terminal_outcome", startSequence + 3, {
      proposal_id: proposalId,
      program: "cargo",
      state: "succeeded",
      exit_code: 0,
      duration_ms: 1250,
      captured_bytes: 48,
      truncated: true,
      encoding_lossy: true,
    }),
    processEvent("process_proposal_requested", startSequence, {
      proposal_id: proposalId,
      program: "cargo",
      argv: ["test", "-p", "pillow-desktop", PORTABLE_ARG, NATIVE_PATH],
      portable_cwd: PORTABLE_CWD_ONLY,
      summary: "Run focused desktop tests",
      risk_class: "standard",
      expires_at: "2026-08-04T00:10:00.000Z",
    }),
    processEvent("process_proposal_decided", startSequence + 1, {
      proposal_id: proposalId,
      program: "cargo",
      decision: "approve_once",
      state: "approved",
    }),
    processEvent("process_started", startSequence + 2, {
      proposal_id: proposalId,
      program: "cargo",
      argv: ["test", "-p", "pillow-desktop", PORTABLE_ARG, NATIVE_PATH],
      portable_cwd: PORTABLE_CWD_ONLY,
    }),
  ];
}

function fixtureProject(overrides: Partial<ProjectDto> = {}): ProjectDto {
  return {
    id: projectId,
    name: "Project A",
    rootPath: "F:\\secret\\projects\\alpha",
    trust: "trusted",
    createdAt: "2026-08-01T00:00:00.000Z",
    updatedAt: "2026-08-01T00:00:00.000Z",
    ...overrides,
  };
}

function createExecGateway(
  overrides: Partial<ExecGateway> = {},
): ExecGateway & {
  outputCalls: Array<{ conversationId: string; proposalId: string }>;
  setOutputResult: (next: ExecOutputView) => void;
} {
  const outputCalls: Array<{ conversationId: string; proposalId: string }> = [];
  let outputResult: ExecOutputView = {
    proposalId: processProposalId,
    state: "succeeded",
    chunks: [
      { stream: "stdout", sequence: 0, text: "running 1 test\n" },
      { stream: "stdout", sequence: 1, text: "ok\n" },
    ],
    liveRetained: true,
    truncated: false,
    encodingLossy: false,
    capturedBytes: 20,
    exitCode: 0,
    durationMs: 100,
  };

  return {
    listExecProposals: async () => [],
    decideExecProposal: async () => ({
      proposalId: processProposalId,
      conversationId,
      origin: "agent",
      state: "succeeded",
      programLabel: "cargo",
      argv: ["test"],
      portableCwd: ".",
      riskClass: "standard",
      summary: "done",
      expiresAt: "2026-08-04T00:10:00.000Z",
      decisionAvailable: false,
    }),
    openExecOutput: async (input) => {
      outputCalls.push({ ...input });
      return { ...outputResult, proposalId: input.proposalId };
    },
    outputCalls,
    setOutputResult: (next) => {
      outputResult = next;
    },
    ...overrides,
  };
}

describe("buildProcessActivityModels", () => {
  it("groups proposal/decision/start/terminal into one ordered model per proposal", () => {
    const models = buildProcessActivityModels(
      fullLifecycleEvents(),
      conversationId,
      runId,
    );
    expect(models).toHaveLength(1);
    const model = models[0]!;
    expect(model.programLabel).toBe("cargo");
    expect(model.state).toBe("succeeded");
    expect(model.statusLabel).toBe("已成功");
    expect(model.conciseSuccess).toBe(true);
    expect(model.canOpenOutput).toBe(true);
    expect(model.exitCode).toBe(0);
    expect(model.durationMs).toBe(1250);
    expect(model.capturedBytes).toBe(48);
    expect(model.truncated).toBe(true);
    expect(model.encodingLossy).toBe(true);
    expect(model.decision).toBe("approve_once");
    // Internal proposal id available for callbacks only.
    expect(model.proposalId).toBe(processProposalId);
    // Redacted native path; portable relative and ordinary tokens stay literal.
    expect(model.argv).toContain("test");
    expect(model.argv).toContain("-p");
    expect(model.argv).toContain("pillow-desktop");
    expect(model.argv).toContain(PORTABLE_ARG);
    expect(model.argv).not.toContain(NATIVE_PATH);
    expect(model.argv.some((token) => token.includes("secret"))).toBe(false);
  });

  it("orders multiple proposals by durable first sequence", () => {
    const events = [
      ...fullLifecycleEvents(processProposalAltId, 10),
      ...fullLifecycleEvents(processProposalId, 1),
    ];
    const models = buildProcessActivityModels(events, conversationId, runId);
    expect(models.map((m) => m.proposalId)).toEqual([
      processProposalId,
      processProposalAltId,
    ]);
  });

  it("ignores foreign conversation and foreign run events", () => {
    const events = [
      ...fullLifecycleEvents(),
      processEvent(
        "process_proposal_requested",
        20,
        {
          proposal_id: "foreign-conv-proposal",
          program: "node",
          argv: ["--version"],
          portable_cwd: ".",
          summary: "foreign",
          risk_class: "standard",
          expires_at: "2026-08-04T00:10:00.000Z",
        },
        { conversationId: foreignConversationId },
      ),
      processEvent(
        "process_started",
        21,
        {
          proposal_id: "foreign-run-proposal",
          program: "node",
          argv: ["--version"],
          portable_cwd: ".",
        },
        { runId: foreignRunId },
      ),
    ];
    const models = buildProcessActivityModels(events, conversationId, runId);
    expect(models).toHaveLength(1);
    expect(models[0]!.proposalId).toBe(processProposalId);
  });

  it("redacts UNC path argv tokens", () => {
    const events = [
      processEvent("process_proposal_requested", 1, {
        proposal_id: processProposalId,
        program: "tool",
        argv: ["run", UNC_PATH, "src/main.rs"],
        portable_cwd: ".",
        summary: "run tool",
        risk_class: "standard",
        expires_at: "2026-08-04T00:10:00.000Z",
      }),
    ];
    const models = buildProcessActivityModels(events, conversationId, runId);
    expect(models[0]!.argv).toEqual(["run", "[path]", "src/main.rs"]);
  });

  it("does not expose portable cwd, grants, digests, or raw host fields on the model surface used for display", () => {
    const models = buildProcessActivityModels(
      fullLifecycleEvents(),
      conversationId,
      runId,
    );
    const model = models[0]!;
    // portable_cwd must never become a model field (argv may still hold relative tokens).
    expect("portableCwd" in model).toBe(false);
    expect("cwd" in model).toBe(false);
    expect("grant" in model).toBe(false);
    expect("digest" in model).toBe(false);
    const json = JSON.stringify(model);
    expect(json).not.toContain(RAW_GRANT);
    expect(json).not.toContain("digest");
    expect(json).not.toContain(NATIVE_PATH);
    expect(json).not.toContain("portable_cwd");
  });
});

describe("ConversationTimeline process command activity", () => {
  it("renders one command activity row for four lifecycle events in durable order", async () => {
    const user = userEvent.setup();
    render(
      <ConversationTimeline
        run={null}
        timeline={timelineWithProcess(fullLifecycleEvents())}
      />,
    );

    const rows = screen.getAllByTestId("command-activity-row");
    expect(rows).toHaveLength(1);
    const row = rows[0]!;
    expect(within(row).getByTestId("command-activity-program")).toHaveTextContent(
      "cargo",
    );
    expect(within(row).getByTestId("command-activity-status")).toHaveTextContent(
      "已成功",
    );

    // Successful commands stay collapsed: details body is not visible until open.
    const details = within(row).getByTestId(
      "command-activity-details",
    ) as HTMLDetailsElement;
    expect(details.className).toMatch(/concise/);
    expect(details.open).toBe(false);
    expect(within(row).getByTestId("command-activity-argv")).not.toBeVisible();
    expect(within(row).getByTestId("command-activity-exit")).not.toBeVisible();
    expect(within(row).getByTestId("command-activity-duration")).not.toBeVisible();
    expect(within(row).getByTestId("command-activity-bytes")).not.toBeVisible();
    expect(within(row).getByTestId("command-activity-open-output")).not
      .toBeVisible();

    await user.click(within(row).getByTestId("command-activity-summary"));
    expect(details.open).toBe(true);
    expect(within(row).getByTestId("command-activity-argv")).toBeVisible();
    expect(within(row).getByTestId("command-activity-argv")).toHaveTextContent(
      '"test"',
    );
    expect(within(row).getByTestId("command-activity-argv")).toHaveTextContent(
      PORTABLE_ARG,
    );
    expect(within(row).getByTestId("command-activity-argv").textContent).not
      .toContain(NATIVE_PATH);
    expect(within(row).getByTestId("command-activity-exit")).toHaveTextContent(
      "0",
    );
    expect(within(row).getByTestId("command-activity-duration")).toHaveTextContent(
      "1250",
    );
    expect(within(row).getByTestId("command-activity-bytes")).toHaveTextContent(
      "48",
    );
    expect(within(row).getByTestId("command-activity-decision")).toHaveTextContent(
      /批准/,
    );
    expect(within(row).getByTestId("command-activity-truncated")).toHaveTextContent(
      "输出已截断",
    );
    expect(within(row).getByTestId("command-activity-lossy")).toHaveTextContent(
      "编码有损",
    );
    expect(within(row).getByTestId("command-activity-open-output")).toBeVisible();
    expect(within(row).getByTestId("command-activity-open-output")).toHaveTextContent(
      "完整输出",
    );

    // Old four generic process notices must be gone.
    expect(screen.queryByText("Process approval requested.")).toBeNull();
    expect(screen.queryByText("Process proposal approved.")).toBeNull();
    expect(screen.queryByText("Process started.")).toBeNull();
    expect(screen.queryByText("Process completed.")).toBeNull();

    // Non-process agent activity remains.
    expect(screen.getByTestId("agent-activity-disclosure")).toBeInTheDocument();
    await user.click(screen.getByTestId("agent-activity-summary"));
    expect(screen.getByText("读取配置")).toBeInTheDocument();

    assertNoSecrets();
  });

  it("hides 完整输出 until the process has started", async () => {
    const user = userEvent.setup();
    const events = [
      processEvent("process_proposal_requested", 1, {
        proposal_id: processProposalId,
        program: "node",
        argv: ["--version"],
        portable_cwd: ".",
        summary: "version",
        risk_class: "standard",
        expires_at: "2026-08-04T00:10:00.000Z",
      }),
      processEvent("process_proposal_decided", 2, {
        proposal_id: processProposalId,
        program: "node",
        decision: "approve_once",
        state: "approved",
      }),
    ];
    render(
      <ConversationTimeline
        run={null}
        timeline={timelineWithProcess(events)}
      />,
    );
    const row = screen.getByTestId("command-activity-row");
    expect(within(row).getByTestId("command-activity-status")).toHaveTextContent(
      "已批准",
    );
    await user.click(within(row).getByTestId("command-activity-summary"));
    expect(within(row).queryByTestId("command-activity-open-output")).toBeNull();
    assertNoSecrets();
  });

  it("does not paint foreign conversation or run process activity", () => {
    const events = [
      processEvent(
        "process_proposal_requested",
        1,
        {
          proposal_id: "foreign-conv-proposal",
          program: "evil",
          argv: ["--leak"],
          portable_cwd: ".",
          summary: "foreign",
          risk_class: "high",
          expires_at: "2026-08-04T00:10:00.000Z",
        },
        { conversationId: foreignConversationId },
      ),
      processEvent(
        "process_started",
        2,
        {
          proposal_id: "foreign-run-proposal",
          program: "evil",
          argv: ["--leak"],
          portable_cwd: ".",
        },
        { runId: foreignRunId },
      ),
    ];
    render(
      <ConversationTimeline
        run={null}
        timeline={timelineWithProcess(events)}
      />,
    );
    expect(screen.queryByTestId("command-activity-row")).toBeNull();
    expect(screen.queryByText("evil")).toBeNull();
    assertNoSecrets();
  });

  it("never paints raw IDs, native paths, cwd, grants, digests, or debug fields", async () => {
    const user = userEvent.setup();
    render(
      <ConversationTimeline
        run={null}
        timeline={timelineWithProcess(fullLifecycleEvents())}
      />,
    );
    const row = screen.getByTestId("command-activity-row");
    await user.click(within(row).getByTestId("command-activity-summary"));
    const body = document.body;
    const text = body.textContent ?? "";
    // portable_cwd is distinct from argv tokens and must never appear.
    expect(text).not.toContain(PORTABLE_CWD_ONLY);
    expect(text).not.toContain("portable_cwd");
    expect(text).not.toContain("Run focused desktop tests");
    expect(text).not.toContain(processProposalId);
    expect(text).not.toContain(runId);
    expect(text).not.toContain(NATIVE_PATH);
    // Portable relative argv tokens remain literal after disclosure.
    expect(text).toContain(PORTABLE_ARG);
    assertNoSecrets();
  });
});

describe("Workbench process activity routes 完整输出 to Terminal pane", () => {
  it("opens the existing Terminal output target for the exact internal proposal", async () => {
    const user = userEvent.setup();
    const execGateway = createExecGateway();

    render(
      <Workbench
        busy={false}
        conversationId={conversationId}
        conversationTitle="Process activity"
        execGateway={execGateway}
        onCancelRun={vi.fn()}
        onOpenProject={vi.fn()}
        onSendTurn={vi.fn()}
        project={fixtureProject()}
        run={null}
        timeline={timelineWithProcess(fullLifecycleEvents())}
      />,
    );

    const row = await screen.findByTestId("command-activity-row");
    await user.click(within(row).getByTestId("command-activity-summary"));
    await user.click(within(row).getByTestId("command-activity-open-output"));

    await waitFor(() => {
      expect(screen.getByTestId("terminal-output-view")).toBeInTheDocument();
    });
    expect(screen.getByTestId("terminal-output-pane")).toBeInTheDocument();
    expect(screen.getByRole("heading", { name: "终端输出", level: 2 }))
      .toBeInTheDocument();
    expect(execGateway.outputCalls).toEqual([
      { conversationId, proposalId: processProposalId },
    ]);
    assertNoSecrets();
  });

  it("project/conversation switch does not reopen foreign activity output", async () => {
    const user = userEvent.setup();
    const execGateway = createExecGateway();
    const projectBId = "00000000-0000-0000-0000-0000000000bb";
    const convBId = "00000000-0000-0000-0000-0000000000c2";

    const { rerender } = render(
      <Workbench
        busy={false}
        conversationId={conversationId}
        conversationTitle="A"
        execGateway={execGateway}
        onCancelRun={vi.fn()}
        onOpenProject={vi.fn()}
        onSendTurn={vi.fn()}
        project={fixtureProject()}
        run={null}
        timeline={timelineWithProcess(fullLifecycleEvents())}
      />,
    );

    const row = await screen.findByTestId("command-activity-row");
    await user.click(within(row).getByTestId("command-activity-summary"));
    await user.click(within(row).getByTestId("command-activity-open-output"));
    await waitFor(() => {
      expect(screen.getByTestId("terminal-output-view")).toBeInTheDocument();
    });

    rerender(
      <Workbench
        busy={false}
        conversationId={convBId}
        conversationTitle="B"
        execGateway={execGateway}
        onCancelRun={vi.fn()}
        onOpenProject={vi.fn()}
        onSendTurn={vi.fn()}
        project={fixtureProject({
          id: projectBId,
          name: "Project B",
          rootPath: "F:\\secret\\projects\\beta",
        })}
        run={null}
        timeline={emptyTimeline({
          conversationId: convBId,
          messages: [
            {
              id: "mb",
              role: "user",
              content: "other",
              createdAt: "2026-08-04T01:00:00.000Z",
            },
          ],
        })}
      />,
    );

    await waitFor(() => {
      expect(screen.queryByTestId("terminal-output-view")).toBeNull();
      expect(screen.queryByTestId("command-activity-row")).toBeNull();
    });
    expect(screen.queryByText("cargo")).toBeNull();
  });
});
