/**
 * C4 D3B2 — Workbench process governance mount.
 * Dual-domain single bar, process decide without grant, terminal pane,
 * generation guards, durable-only notice, close-does-not-cancel.
 */
import {
  act,
  fireEvent,
  render,
  screen,
  waitFor,
  within,
} from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, describe, expect, it, vi } from "vitest";
import { Workbench } from "../src/features/workbench/Workbench";
import type {
  ExecGateway,
  ExecOutputView,
  ExecProposalView,
} from "../src/lib/execGateway";
import type { ProjectDto } from "../src/lib/projectGateway";
import type {
  WriteApprovalGateway,
  WriteDiffView,
  WriteProposalView,
} from "../src/lib/writeApprovalGateway";
import type {
  ConversationTimelineView,
} from "../src/features/conversations/conversationReducer";
import "../src/styles/tokens.css";
import "../src/styles/workbench.css";
import "../src/styles/execution.css";

const projectAId = "00000000-0000-0000-0000-0000000000aa";
const projectBId = "00000000-0000-0000-0000-0000000000bb";
const convAId = "00000000-0000-0000-0000-0000000000c1";
const convBId = "00000000-0000-0000-0000-0000000000c2";
/** Canonical 43-char base64url grant — must never appear in DOM/console. */
const grantSecret = "abcdefghijklmnopqrstuvwxyz0123456789ABCDEFG";
const writeProposalId = "write-prop-aaaaaaaa";
const processProposalId = "proc-prop-bbbbbbbb";
const processProposalAltId = "proc-prop-cccccccc";
const RAW_RUN_ID = "run-must-never-paint";
const NATIVE_PATH = "F:\\secret\\projects\\alpha\\crates\\tool";

function futureExpiry(minutes = 10): string {
  return new Date(Date.now() + minutes * 60_000).toISOString();
}

function fixtureProject(overrides: Partial<ProjectDto> = {}): ProjectDto {
  return {
    id: projectAId,
    name: "Project A",
    rootPath: "F:\\secret\\projects\\alpha",
    trust: "trusted",
    createdAt: "2026-08-01T00:00:00.000Z",
    updatedAt: "2026-08-01T00:00:00.000Z",
    ...overrides,
  };
}

function emptyTimeline(conversationId: string): ConversationTimelineView {
  return {
    conversationId,
    messages: [
      {
        id: "m1",
        role: "user",
        content: "请运行测试",
        createdAt: "2026-08-01T12:00:00.000Z",
      },
    ],
    events: [],
    seenEventKeys: new Set(),
    assistantText: "",
    reasoningText: "",
    plan: null,
    tools: [],
    subagents: [],
    activitiesByRunId: {
      "run-1": {
        runId: "run-1",
        status: "running",
        startedAt: "2026-08-01T12:00:00.000Z",
        updatedAt: "2026-08-01T12:00:00.000Z",
        assistantText: "",
        reasoningText: "",
        plan: null,
        tools: [],
        subagents: [],
        artifacts: [],
      },
    },
    activeRunId: "run-1",
  };
}

function pendingWrite(
  overrides: Partial<WriteProposalView> = {},
): WriteProposalView {
  return {
    proposalId: writeProposalId,
    conversationId: convAId,
    path: "src/example.ts",
    mode: "create",
    state: "pending_approval",
    summary: "Create example",
    expiresAt: futureExpiry(20),
    decisionAvailable: true,
    ruleSubject: "src/example.ts",
    ...overrides,
  };
}

function pendingProcess(
  overrides: Partial<ExecProposalView> = {},
): ExecProposalView {
  return {
    proposalId: processProposalId,
    conversationId: convAId,
    origin: "agent",
    state: "pending_approval",
    programLabel: "cargo",
    argv: ["test", "-p", "pillow-desktop"],
    portableCwd: "apps/desktop",
    riskClass: "standard",
    summary: "Run focused desktop tests",
    expiresAt: futureExpiry(10),
    decisionAvailable: true,
    ruleSubject: "cargo test -p pillow-desktop",
    ...overrides,
  };
}

function fixtureOutput(
  overrides: Partial<ExecOutputView> = {},
): ExecOutputView {
  return {
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
    ...overrides,
  };
}

function fixtureDiff(overrides: Partial<WriteDiffView> = {}): WriteDiffView {
  return {
    proposalId: writeProposalId,
    path: "src/example.ts",
    mode: "create",
    unified: "--- /dev/null\n+++ b/src/example.ts\n+export const x = 1;\n",
    truncated: true,
    lineCount: 4000,
    byteCount: 262144,
    ...overrides,
  };
}

function createWriteGateway(
  overrides: Partial<WriteApprovalGateway> & {
    listImpl?: () => Promise<WriteProposalView[]>;
  } = {},
): WriteApprovalGateway & {
  listCalls: string[];
  decideCalls: Array<Record<string, unknown>>;
  setListResult: (next: WriteProposalView[]) => void;
} {
  const listCalls: string[] = [];
  const decideCalls: Array<Record<string, unknown>> = [];
  let listResult: WriteProposalView[] = [];

  return {
    listCalls,
    decideCalls,
    setListResult: (next) => {
      listResult = next;
    },
    listWriteProposals: async (conversationId) => {
      listCalls.push(conversationId);
      if (overrides.listImpl) return overrides.listImpl();
      return listResult.filter((item) => item.conversationId === conversationId);
    },
    decideWriteProposal: async (input) => {
      decideCalls.push({ ...input });
      listResult = [
        {
          ...pendingWrite({
            proposalId: input.proposalId,
            conversationId: input.conversationId,
            state: input.decision === "approve_once" ? "succeeded" : "rejected",
            decisionAvailable: false,
          }),
        },
      ];
      return listResult[0]!;
    },
    openWriteProposalDiff: async (input) => fixtureDiff({
      proposalId: input.proposalId,
    }),
    ...overrides,
  };
}

function createExecGateway(
  overrides: Partial<ExecGateway> & {
    listImpl?: () => Promise<ExecProposalView[]>;
    outputImpl?: () => Promise<ExecOutputView>;
  } = {},
): ExecGateway & {
  listCalls: string[];
  decideCalls: Array<Record<string, unknown>>;
  outputCalls: Array<Record<string, unknown>>;
  setListResult: (next: ExecProposalView[]) => void;
  setOutputResult: (next: ExecOutputView) => void;
} {
  const listCalls: string[] = [];
  const decideCalls: Array<Record<string, unknown>> = [];
  const outputCalls: Array<Record<string, unknown>> = [];
  let listResult: ExecProposalView[] = [pendingProcess()];
  let outputResult: ExecOutputView = fixtureOutput();

  return {
    listCalls,
    decideCalls,
    outputCalls,
    setListResult: (next) => {
      listResult = next;
    },
    setOutputResult: (next) => {
      outputResult = next;
    },
    listExecProposals: async (conversationId) => {
      listCalls.push(conversationId);
      if (overrides.listImpl) return overrides.listImpl();
      return listResult.filter((item) => item.conversationId === conversationId);
    },
    decideExecProposal: async (input) => {
      decideCalls.push({ ...input });
      const next: ExecProposalView = {
        ...pendingProcess({
          proposalId: input.proposalId,
          conversationId: input.conversationId,
          state: input.decision === "approve_once" ? "approved" : "rejected",
          decisionAvailable: false,
        }),
      };
      listResult = [next];
      return next;
    },
    openExecOutput: async (input) => {
      outputCalls.push({ ...input });
      if (overrides.outputImpl) return overrides.outputImpl();
      return { ...outputResult, proposalId: input.proposalId };
    },
    ...overrides,
  };
}

type WorkbenchTestProps = {
  writeApprovalGateway?: WriteApprovalGateway;
  execGateway?: ExecGateway;
  execProposalRefreshToken?: number;
  writeProposalRefreshToken?: number;
  conversationId?: string | null;
  project?: ProjectDto | null;
  onCancelRun?: () => void;
};

function renderWorkbench(props: WorkbenchTestProps = {}) {
  return render(
    <Workbench
      conversationId={convAId}
      onCancelRun={vi.fn()}
      onOpenProject={vi.fn()}
      onSendTurn={vi.fn()}
      project={fixtureProject()}
      run={null}
      timeline={emptyTimeline(convAId)}
      {...props}
    />,
  );
}

function expectNoSecrets(container: HTMLElement = document.body) {
  const text = container.textContent ?? "";
  const html = container.innerHTML;
  expect(text).not.toContain(grantSecret);
  expect(html).not.toContain(grantSecret);
  expect(text).not.toContain(processProposalId);
  expect(html).not.toContain(processProposalId);
  expect(text).not.toContain(writeProposalId);
  expect(html).not.toContain(writeProposalId);
  expect(text).not.toContain(RAW_RUN_ID);
  expect(text).not.toContain(NATIVE_PATH);
  expect(html).not.toMatch(/approvalGrant|data-grant|data-proposal-id/i);
}

afterEach(() => {
  vi.useRealTimers();
});

describe("C4 workbench process integration — single approval bar", () => {
  it("shows write bar only when only a write candidate is pending", async () => {
    const writeGateway = createWriteGateway();
    writeGateway.setListResult([pendingWrite()]);
    const execGateway = createExecGateway();
    execGateway.setListResult([]);

    renderWorkbench({
      writeApprovalGateway: writeGateway,
      execGateway,
    });

    await waitFor(() => {
      expect(screen.getByTestId("write-approval-bar")).toBeInTheDocument();
    });
    expect(screen.queryByTestId("command-approval-bar")).toBeNull();
    expect(screen.getAllByTestId("write-approval-bar")).toHaveLength(1);
    expectNoSecrets();
  });

  it("shows process bar only when only a process candidate is pending", async () => {
    const writeGateway = createWriteGateway();
    writeGateway.setListResult([]);
    const execGateway = createExecGateway();
    execGateway.setListResult([pendingProcess()]);

    renderWorkbench({
      writeApprovalGateway: writeGateway,
      execGateway,
    });

    await waitFor(() => {
      expect(screen.getByTestId("command-approval-bar")).toBeInTheDocument();
    });
    expect(screen.queryByTestId("write-approval-bar")).toBeNull();
    expect(screen.getByTestId("command-approval-program")).toHaveTextContent(
      "cargo",
    );
    expect(screen.getByTestId("command-approval-argv")).toHaveTextContent(
      '"test" "-p" "pillow-desktop"',
    );
    expectNoSecrets();
  });

  it("passes host shell rule segments through to the mounted approval bar", async () => {
    const writeGateway = createWriteGateway();
    writeGateway.setListResult([]);
    const execGateway = createExecGateway();
    execGateway.setListResult([
      pendingProcess({
        programLabel: "shell",
        argv: ["cargo test && cargo build"],
        riskClass: "high",
        ruleSubject: "",
        ruleSegments: ["cargo test", "cargo build"],
      }),
    ]);

    renderWorkbench({
      writeApprovalGateway: writeGateway,
      execGateway,
    });

    await waitFor(() => {
      expect(screen.getByTestId("command-approval-bar")).toBeInTheDocument();
    });
    expect(screen.getByTestId("command-approval-shell-rule-note")).toHaveTextContent(
      "分别保存 2 条允许规则",
    );
    expect(screen.getByTestId("command-approval-always-allow")).toHaveTextContent(
      "逐段设置总是允许（2 条规则）",
    );
    expectNoSecrets();
  });

  it("arbitrates simultaneous candidates to the oldest expiry", async () => {
    const writeGateway = createWriteGateway();
    writeGateway.setListResult([
      pendingWrite({ expiresAt: futureExpiry(30) }),
    ]);
    const execGateway = createExecGateway();
    execGateway.setListResult([
      pendingProcess({ expiresAt: futureExpiry(5) }),
    ]);

    renderWorkbench({
      writeApprovalGateway: writeGateway,
      execGateway,
    });

    await waitFor(() => {
      expect(screen.getByTestId("command-approval-bar")).toBeInTheDocument();
    });
    expect(screen.queryByTestId("write-approval-bar")).toBeNull();
    expect(
      document.querySelectorAll(
        "[data-testid='write-approval-bar'], [data-testid='command-approval-bar']",
      ),
    ).toHaveLength(1);
    expectNoSecrets();
  });

  it("prefers write on exact expiry and request-id ties", async () => {
    const sharedExpiry = futureExpiry(15);
    const sharedId = "same-request-id-zzzz";
    const writeGateway = createWriteGateway();
    writeGateway.setListResult([
      pendingWrite({
        proposalId: sharedId,
        expiresAt: sharedExpiry,
      }),
    ]);
    const execGateway = createExecGateway();
    execGateway.setListResult([
      pendingProcess({
        proposalId: sharedId,
        expiresAt: sharedExpiry,
      }),
    ]);

    renderWorkbench({
      writeApprovalGateway: writeGateway,
      execGateway,
    });

    await waitFor(() => {
      expect(screen.getByTestId("write-approval-bar")).toBeInTheDocument();
    });
    expect(screen.queryByTestId("command-approval-bar")).toBeNull();
    expect(
      document.querySelectorAll(
        "[data-testid='write-approval-bar'], [data-testid='command-approval-bar']",
      ),
    ).toHaveLength(1);
  });

  it("ignores unavailable and expired process candidates", async () => {
    const writeGateway = createWriteGateway();
    writeGateway.setListResult([]);
    const execGateway = createExecGateway();
    execGateway.setListResult([
      pendingProcess({
        proposalId: processProposalAltId,
        decisionAvailable: false,
        expiresAt: futureExpiry(1),
      }),
      pendingProcess({
        proposalId: processProposalId,
        expiresAt: "not-a-timestamp",
        decisionAvailable: true,
      }),
    ]);

    renderWorkbench({
      writeApprovalGateway: writeGateway,
      execGateway,
    });

    await waitFor(() => {
      expect(execGateway.listCalls.length).toBeGreaterThan(0);
    });
    expect(screen.queryByTestId("command-approval-bar")).toBeNull();
    expect(screen.queryByTestId("write-approval-bar")).toBeNull();
  });

  it("never renders two approval bars at once", async () => {
    const writeGateway = createWriteGateway();
    writeGateway.setListResult([
      pendingWrite({ expiresAt: futureExpiry(8) }),
    ]);
    const execGateway = createExecGateway();
    execGateway.setListResult([
      pendingProcess({ expiresAt: futureExpiry(9) }),
      pendingProcess({
        proposalId: processProposalAltId,
        expiresAt: futureExpiry(12),
      }),
    ]);

    renderWorkbench({
      writeApprovalGateway: writeGateway,
      execGateway,
    });

    await waitFor(() => {
      expect(screen.getByTestId("write-approval-bar")).toBeInTheDocument();
    });
    const bars = document.querySelectorAll(
      "[data-testid='write-approval-bar'], [data-testid='command-approval-bar']",
    );
    expect(bars).toHaveLength(1);
  });

  it("does not let an older decision-unavailable write hide a newer actionable process", async () => {
    // Write expires sooner so arbitration would pick it first when available.
    // Without decisionAvailable it cannot hide a later, host-available process.
    const writeGateway = createWriteGateway();
    writeGateway.setListResult([
      pendingWrite({
        decisionAvailable: false,
        expiresAt: futureExpiry(5),
        summary: "Unavailable older write",
      }),
    ]);
    const execGateway = createExecGateway();
    execGateway.setListResult([
      pendingProcess({
        decisionAvailable: true,
        expiresAt: futureExpiry(30),
        summary: "Actionable process",
      }),
    ]);

    renderWorkbench({
      writeApprovalGateway: writeGateway,
      execGateway,
    });

    await waitFor(() => {
      expect(screen.getByTestId("command-approval-bar")).toBeInTheDocument();
    });
    expect(screen.queryByTestId("write-approval-bar")).toBeNull();
    expect(screen.getByTestId("command-approval-approve")).not.toBeDisabled();
    expect(screen.getByTestId("command-approval-summary")).toHaveTextContent(
      "Actionable process",
    );
    expectNoSecrets();
  });
});

describe("C4 workbench process integration — decide and re-list", () => {
  it("approve-once sends no grant and re-lists authoritatively", async () => {
    const user = userEvent.setup();
    const execGateway = createExecGateway();
    execGateway.setListResult([pendingProcess()]);

    renderWorkbench({ execGateway });

    await waitFor(() => {
      expect(screen.getByTestId("command-approval-approve")).toBeInTheDocument();
    });
    const listBefore = execGateway.listCalls.length;
    await user.click(screen.getByTestId("command-approval-approve"));

    await waitFor(() => {
      expect(execGateway.decideCalls.length).toBe(1);
    });
    expect(execGateway.decideCalls[0]).toEqual({
      conversationId: convAId,
      proposalId: processProposalId,
      decision: "approve_once",
    });
    expect(execGateway.decideCalls[0]).not.toHaveProperty("approvalGrant");
    expect(execGateway.decideCalls[0]).not.toHaveProperty("grant");
    await waitFor(() => {
      expect(execGateway.listCalls.length).toBeGreaterThan(listBefore);
    });
    await waitFor(() => {
      expect(screen.queryByTestId("command-approval-bar")).toBeNull();
    });
    expectNoSecrets();
  });

  it("reject sends no grant and re-lists authoritatively", async () => {
    const user = userEvent.setup();
    const execGateway = createExecGateway();
    execGateway.setListResult([pendingProcess()]);

    renderWorkbench({ execGateway });

    await waitFor(() => {
      expect(screen.getByTestId("command-approval-reject")).toBeInTheDocument();
    });
    const listBefore = execGateway.listCalls.length;
    await user.click(screen.getByTestId("command-approval-reject"));

    await waitFor(() => {
      expect(execGateway.decideCalls).toEqual([
        {
          conversationId: convAId,
          proposalId: processProposalId,
          decision: "reject",
        },
      ]);
    });
    await waitFor(() => {
      expect(execGateway.listCalls.length).toBeGreaterThan(listBefore);
    });
    await waitFor(() => {
      expect(screen.queryByTestId("command-approval-bar")).toBeNull();
    });
  });
});

describe("C4 workbench process integration — generation guards", () => {
  it("suppresses stale process list results after conversation switch", async () => {
    let resolveList: ((value: ExecProposalView[]) => void) | undefined;
    const execGateway = createExecGateway({
      listImpl: () =>
        new Promise<ExecProposalView[]>((resolve) => {
          resolveList = resolve;
        }),
    });

    const { rerender } = renderWorkbench({ execGateway });

    await waitFor(() => {
      expect(execGateway.listCalls.length).toBeGreaterThan(0);
    });

    rerender(
      <Workbench
        conversationId={convBId}
        onCancelRun={vi.fn()}
        onOpenProject={vi.fn()}
        onSendTurn={vi.fn()}
        project={fixtureProject()}
        run={null}
        timeline={emptyTimeline(convBId)}
        execGateway={execGateway}
      />,
    );

    await act(async () => {
      resolveList?.([
        pendingProcess({
          conversationId: convAId,
          summary: "STALE-FOREIGN-PROCESS",
        }),
      ]);
    });

    await waitFor(() => {
      expect(screen.queryByText("STALE-FOREIGN-PROCESS")).toBeNull();
    });
    expect(screen.queryByTestId("command-approval-bar")).toBeNull();
  });

  it("suppresses stale process list results after project switch", async () => {
    const resolvers: Array<(value: ExecProposalView[]) => void> = [];
    const execGateway = createExecGateway({
      listImpl: () =>
        new Promise<ExecProposalView[]>((resolve) => {
          resolvers.push(resolve);
        }),
    });

    const { rerender } = renderWorkbench({ execGateway });

    await waitFor(() => {
      expect(resolvers.length).toBe(1);
    });

    rerender(
      <Workbench
        conversationId={convAId}
        onCancelRun={vi.fn()}
        onOpenProject={vi.fn()}
        onSendTurn={vi.fn()}
        project={fixtureProject({ id: projectBId, name: "Project B" })}
        run={null}
        timeline={emptyTimeline(convAId)}
        execGateway={execGateway}
      />,
    );

    await waitFor(() => {
      expect(resolvers.length).toBe(2);
    });

    // Resolve the pre-switch list only — generation guard must discard it.
    await act(async () => {
      resolvers[0]!([
        pendingProcess({ summary: "STALE-PROJECT-PROCESS" }),
      ]);
    });

    await waitFor(() => {
      expect(screen.queryByText("STALE-PROJECT-PROCESS")).toBeNull();
    });
    expect(screen.queryByTestId("command-approval-bar")).toBeNull();

    // Current project list settles empty — still no foreign bar.
    await act(async () => {
      resolvers[1]!([]);
    });
    expect(screen.queryByTestId("command-approval-bar")).toBeNull();
  });

  it("suppresses stale terminal output after conversation switch", async () => {
    let resolveOutput: ((value: ExecOutputView) => void) | undefined;
    const execGateway = createExecGateway({
      outputImpl: () =>
        new Promise<ExecOutputView>((resolve) => {
          resolveOutput = resolve;
        }),
    });
    execGateway.setListResult([pendingProcess()]);
    const user = userEvent.setup();

    const { rerender } = renderWorkbench({ execGateway });

    await waitFor(() => {
      expect(screen.getByTestId("command-approval-review")).toBeInTheDocument();
    });
    await user.click(screen.getByTestId("command-approval-review"));

    await waitFor(() => {
      expect(execGateway.outputCalls.length).toBe(1);
    });

    rerender(
      <Workbench
        conversationId={convBId}
        onCancelRun={vi.fn()}
        onOpenProject={vi.fn()}
        onSendTurn={vi.fn()}
        project={fixtureProject()}
        run={null}
        timeline={emptyTimeline(convBId)}
        execGateway={execGateway}
      />,
    );

    await act(async () => {
      resolveOutput?.(
        fixtureOutput({
          chunks: [
            {
              stream: "stdout",
              sequence: 0,
              text: "STALE-OUTPUT-FOREIGN\n",
            },
          ],
        }),
      );
    });

    await waitFor(() => {
      expect(screen.queryByText("STALE-OUTPUT-FOREIGN")).toBeNull();
    });
    expect(screen.queryByTestId("terminal-output-view")).toBeNull();
  });

  it("suppresses stale terminal output after project switch", async () => {
    let resolveOutput: ((value: ExecOutputView) => void) | undefined;
    const execGateway = createExecGateway({
      outputImpl: () =>
        new Promise<ExecOutputView>((resolve) => {
          resolveOutput = resolve;
        }),
    });
    execGateway.setListResult([pendingProcess()]);
    const user = userEvent.setup();

    const { rerender } = renderWorkbench({ execGateway });

    await waitFor(() => {
      expect(screen.getByTestId("command-approval-review")).toBeInTheDocument();
    });
    await user.click(screen.getByTestId("command-approval-review"));

    await waitFor(() => {
      expect(execGateway.outputCalls.length).toBe(1);
    });

    rerender(
      <Workbench
        conversationId={convAId}
        onCancelRun={vi.fn()}
        onOpenProject={vi.fn()}
        onSendTurn={vi.fn()}
        project={fixtureProject({ id: projectBId, name: "Project B" })}
        run={null}
        timeline={emptyTimeline(convAId)}
        execGateway={execGateway}
      />,
    );

    await act(async () => {
      resolveOutput?.(
        fixtureOutput({
          chunks: [
            {
              stream: "stdout",
              sequence: 0,
              text: "STALE-OUTPUT-PROJECT\n",
            },
          ],
        }),
      );
    });

    await waitFor(() => {
      expect(screen.queryByText("STALE-OUTPUT-PROJECT")).toBeNull();
    });
    expect(screen.queryByTestId("terminal-output-view")).toBeNull();
    // Foreign target must not resurface on the new project.
    expect(screen.queryByTestId("terminal-output-pane")).toBeNull();
  });
});

describe("C4 workbench process integration — terminal pane", () => {
  it("opens Terminal output mode and propagates liveRetained", async () => {
    const user = userEvent.setup();
    const execGateway = createExecGateway();
    execGateway.setListResult([pendingProcess()]);
    execGateway.setOutputResult(
      fixtureOutput({ liveRetained: true, capturedBytes: 42 }),
    );

    renderWorkbench({ execGateway });

    await waitFor(() => {
      expect(screen.getByTestId("command-approval-review")).toBeInTheDocument();
    });
    await user.click(screen.getByTestId("command-approval-review"));

    await waitFor(() => {
      expect(screen.getByTestId("terminal-output-view")).toBeInTheDocument();
    });
    expect(screen.getByTestId("terminal-output-pane")).toBeInTheDocument();
    expect(screen.getByRole("heading", { name: "终端输出", level: 2 }))
      .toBeInTheDocument();
    expect(screen.getByTestId("terminal-output-body")).toHaveTextContent(
      "running 1 test",
    );
    expect(screen.getByTestId("terminal-output-view")).toHaveAttribute(
      "data-live-retained",
      "true",
    );
    expect(screen.queryByTestId("terminal-output-durable-only")).toBeNull();
    expect(execGateway.outputCalls).toEqual([
      { conversationId: convAId, proposalId: processProposalId },
    ]);
    expectNoSecrets();
  });

  it("shows durable-only notice when liveRetained is false", async () => {
    const user = userEvent.setup();
    const execGateway = createExecGateway();
    execGateway.setListResult([pendingProcess()]);
    execGateway.setOutputResult(
      fixtureOutput({
        liveRetained: false,
        chunks: [],
        capturedBytes: 128,
        state: "interrupted",
      }),
    );

    renderWorkbench({ execGateway });

    await waitFor(() => {
      expect(screen.getByTestId("command-approval-review")).toBeInTheDocument();
    });
    await user.click(screen.getByTestId("command-approval-review"));

    await waitFor(() => {
      expect(screen.getByTestId("terminal-output-durable-only")).toBeInTheDocument();
    });
    expect(screen.getByTestId("terminal-output-durable-only")).toHaveTextContent(
      "实时输出未保留",
    );
    expect(screen.getByTestId("terminal-output-body")).toHaveTextContent("");
    // Metadata may remain; transcript must not be invented from digests.
    expect(screen.getByTestId("terminal-output-bytes")).toHaveTextContent("128");
    expectNoSecrets();
  });

  it("closing the pane does not cancel a process", async () => {
    const user = userEvent.setup();
    const execGateway = createExecGateway();
    const cancelSpy = vi.fn();
    execGateway.setListResult([pendingProcess()]);

    renderWorkbench({
      execGateway,
      onCancelRun: cancelSpy,
    });

    await waitFor(() => {
      expect(screen.getByTestId("command-approval-review")).toBeInTheDocument();
    });
    await user.click(screen.getByTestId("command-approval-review"));

    await waitFor(() => {
      expect(screen.getByTestId("terminal-output-pane")).toBeInTheDocument();
    });

    await user.click(screen.getByTestId("resource-pane-close"));

    await waitFor(() => {
      expect(screen.queryByTestId("resource-pane")).toBeNull();
    });
    // No decide(reject) as a side effect of close; no cancel run.
    expect(execGateway.decideCalls).toEqual([]);
    expect(cancelSpy).not.toHaveBeenCalled();
    // Process bar remains — process is still live for decision.
    expect(screen.getByTestId("command-approval-bar")).toBeInTheDocument();
  });

  it("keeps terminal open and re-pulls when only execProposalRefreshToken changes", async () => {
    const user = userEvent.setup();
    const execGateway = createExecGateway();
    execGateway.setListResult([pendingProcess()]);
    execGateway.setOutputResult(
      fixtureOutput({
        chunks: [
          { stream: "stdout", sequence: 0, text: "initial-host-chunk\n" },
        ],
      }),
    );

    const { rerender } = renderWorkbench({
      execGateway,
      execProposalRefreshToken: 0,
    });

    await waitFor(() => {
      expect(screen.getByTestId("command-approval-review")).toBeInTheDocument();
    });
    await user.click(screen.getByTestId("command-approval-review"));

    await waitFor(() => {
      expect(screen.getByTestId("terminal-output-body")).toHaveTextContent(
        "initial-host-chunk",
      );
    });
    expect(execGateway.outputCalls.length).toBe(1);
    expect(screen.getByTestId("terminal-output-pane")).toBeInTheDocument();

    execGateway.setOutputResult(
      fixtureOutput({
        chunks: [
          { stream: "stdout", sequence: 0, text: "refreshed-host-chunk\n" },
          { stream: "stdout", sequence: 1, text: "ok\n" },
        ],
      }),
    );

    rerender(
      <Workbench
        conversationId={convAId}
        onCancelRun={vi.fn()}
        onOpenProject={vi.fn()}
        onSendTurn={vi.fn()}
        project={fixtureProject()}
        run={null}
        timeline={emptyTimeline(convAId)}
        execGateway={execGateway}
        execProposalRefreshToken={1}
      />,
    );

    await waitFor(() => {
      expect(execGateway.outputCalls.length).toBe(2);
    });
    // Same-scope process lifecycle refresh must not close the open pane.
    expect(screen.getByTestId("terminal-output-pane")).toBeInTheDocument();
    expect(screen.getByTestId("terminal-output-view")).toBeInTheDocument();
    await waitFor(() => {
      expect(screen.getByTestId("terminal-output-body")).toHaveTextContent(
        "refreshed-host-chunk",
      );
    });
    expect(screen.getByTestId("terminal-output-body")).toHaveTextContent("ok");
    expect(execGateway.outputCalls[1]).toEqual({
      conversationId: convAId,
      proposalId: processProposalId,
    });
    expectNoSecrets();
  });

  it("Escape rejects only when the process approval bar owns focus", async () => {
    const user = userEvent.setup();
    const execGateway = createExecGateway();
    execGateway.setListResult([pendingProcess()]);

    renderWorkbench({ execGateway });

    await waitFor(() => {
      expect(screen.getByTestId("command-approval-approve")).toBeInTheDocument();
    });

    // Bar focuses Approve on mount — Escape should reject.
    expect(screen.getByTestId("command-approval-approve")).toHaveFocus();
    await user.keyboard("{Escape}");

    await waitFor(() => {
      expect(execGateway.decideCalls).toEqual([
        {
          conversationId: convAId,
          proposalId: processProposalId,
          decision: "reject",
        },
      ]);
    });
  });

  it("Escape outside the process bar does not reject", async () => {
    const execGateway = createExecGateway();
    execGateway.setListResult([pendingProcess()]);

    renderWorkbench({ execGateway });

    await waitFor(() => {
      expect(screen.getByTestId("command-approval-bar")).toBeInTheDocument();
    });

    // Move focus out of the bar (composer textarea).
    const composer = screen.getByRole("textbox");
    composer.focus();
    expect(composer).toHaveFocus();

    fireEvent.keyDown(composer, { key: "Escape", code: "Escape" });

    await act(async () => {
      await new Promise((resolve) => setTimeout(resolve, 30));
    });
    expect(execGateway.decideCalls).toEqual([]);
    expect(screen.getByTestId("command-approval-bar")).toBeInTheDocument();
  });
});

describe("C4 workbench process integration — write path regression", () => {
  it("preserves write bar, grant decide, and governed diff review", async () => {
    const user = userEvent.setup();
    const writeGateway = createWriteGateway();
    writeGateway.setListResult([pendingWrite()]);
    const execGateway = createExecGateway();
    execGateway.setListResult([]);

    renderWorkbench({
      writeApprovalGateway: writeGateway,
      execGateway,
    });

    await waitFor(() => {
      expect(screen.getByTestId("write-approval-bar")).toBeInTheDocument();
    });
    expect(screen.getByTestId("write-approval-path")).toHaveTextContent(
      "src/example.ts",
    );

    await user.click(screen.getByTestId("write-approval-review-diff"));
    await waitFor(() => {
      expect(screen.getByTestId("write-diff-preview")).toBeInTheDocument();
    });

    await user.click(screen.getByTestId("write-approval-approve"));
    await waitFor(() => {
      expect(writeGateway.decideCalls[0]).toEqual({
        conversationId: convAId,
        proposalId: writeProposalId,
        decision: "approve_once",
      });
      expect(writeGateway.decideCalls[0]).not.toHaveProperty("approvalGrant");
    });
    await waitFor(() => {
      expect(screen.queryByTestId("write-approval-bar")).toBeNull();
    });
    expectNoSecrets();
  });

  it("does not poll listExecProposals on a timer", async () => {
    const execGateway = createExecGateway();
    execGateway.setListResult([pendingProcess()]);

    renderWorkbench({ execGateway });

    await waitFor(() => {
      expect(screen.getByTestId("command-approval-bar")).toBeInTheDocument();
    });
    const afterMount = execGateway.listCalls.length;
    await act(async () => {
      await new Promise((resolve) => setTimeout(resolve, 120));
    });
    expect(execGateway.listCalls.length).toBe(afterMount);
  });

  it("re-lists when execProposalRefreshToken bumps", async () => {
    const execGateway = createExecGateway();
    execGateway.setListResult([pendingProcess()]);

    const { rerender } = renderWorkbench({
      execGateway,
      execProposalRefreshToken: 0,
    });

    await waitFor(() => {
      expect(execGateway.listCalls.length).toBeGreaterThan(0);
    });
    const afterMount = execGateway.listCalls.length;

    rerender(
      <Workbench
        conversationId={convAId}
        onCancelRun={vi.fn()}
        onOpenProject={vi.fn()}
        onSendTurn={vi.fn()}
        project={fixtureProject()}
        run={null}
        timeline={emptyTimeline(convAId)}
        execGateway={execGateway}
        execProposalRefreshToken={1}
      />,
    );

    await waitFor(() => {
      expect(execGateway.listCalls.length).toBeGreaterThan(afterMount);
    });
  });
});
