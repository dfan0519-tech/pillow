/**
 * A pending proposal is the LAST event of a turn — the agent blocks waiting for
 * the human, so nothing further arrives to bump the refresh token. Before this
 * suite existed, one transient list failure cleared the bar for good and the
 * stage looked idle while a proposal sat durably pending on the host.
 *
 * Both directions are pinned here: the list must recover on its own when the
 * failure is transient, and when it genuinely cannot be read the UI must SAY so
 * rather than present a clean empty stage. A test that only asserted the
 * banner renders would be false green — the no-false-alarm cases below are the
 * other half.
 */
import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it, vi } from "vitest";
import { Workbench } from "../src/features/workbench/Workbench";
import type { ProjectDto } from "../src/lib/projectGateway";
import type {
  WriteApprovalGateway,
  WriteDiffView,
  WriteProposalView,
} from "../src/lib/writeApprovalGateway";
import type { ConversationTimelineView } from "../src/features/conversations/conversationReducer";
import "../src/styles/tokens.css";
import "../src/styles/workbench.css";
import "../src/styles/write-approval.css";

vi.mock("@tauri-apps/api/window", () => ({
  getCurrentWindow: () => ({
    minimize: vi.fn().mockResolvedValue(undefined),
    toggleMaximize: vi.fn().mockResolvedValue(undefined),
    close: vi.fn().mockResolvedValue(undefined),
    isMaximized: vi.fn().mockResolvedValue(false),
    onResized: vi.fn().mockResolvedValue(vi.fn()),
  }),
}));

const projectId = "00000000-0000-0000-0000-0000000000aa";
const conversationId = "00000000-0000-0000-0000-0000000000c1";
const proposalId = "prop-relist-aaaaaaaa";
/** Canonical 43-char base64url grant — must never reach DOM or console. */
const grantSecret = "abcdefghijklmnopqrstuvwxyz0123456789ABCDEFG";

/** Retry budget is 250ms + 1000ms; give the assertions room above that. */
const RETRY_WAIT_MS = 5_000;

function fixtureProject(): ProjectDto {
  return {
    id: projectId,
    name: "Project A",
    rootPath: "F:\\projects\\alpha",
    trust: "trusted",
    createdAt: "2026-08-01T00:00:00.000Z",
    updatedAt: "2026-08-01T00:00:00.000Z",
  };
}

function pendingProposal(): WriteProposalView {
  return {
    proposalId,
    conversationId,
    path: "src/example.ts",
    mode: "create",
    state: "pending_approval",
    summary: "Create example",
    expiresAt: new Date(Date.now() + 10 * 60_000).toISOString(),
    approvalGrant: grantSecret,
    ruleSubject: "src/example.ts",
  };
}

function emptyTimeline(): ConversationTimelineView {
  return {
    conversationId,
    messages: [
      {
        id: "m1",
        role: "user",
        content: "请创建文件",
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
    activitiesByRunId: {},
    activeRunId: null,
  };
}

/**
 * Fails the first `failures` list calls, then serves `result`.
 * Counting calls is what proves the retry happened — asserting only the final
 * DOM state cannot tell a retry apart from a slow first call.
 */
function flakyGateway(
  failures: number,
  result: WriteProposalView[] = [pendingProposal()],
): WriteApprovalGateway & { listCalls: number } {
  const gateway = {
    listCalls: 0,
    listWriteProposals: async () => {
      gateway.listCalls += 1;
      if (gateway.listCalls <= failures) {
        throw new Error("host list unavailable");
      }
      return result;
    },
    decideWriteProposal: async () => pendingProposal(),
    openWriteProposalDiff: async () =>
      ({
        proposalId,
        path: "src/example.ts",
        mode: "create",
        unified: "",
        truncated: false,
        lineCount: 0,
        byteCount: 0,
      }) satisfies WriteDiffView,
  };
  return gateway;
}

function renderWorkbench(writeApprovalGateway: WriteApprovalGateway) {
  return render(
    <Workbench
      conversationId={conversationId}
      onCancelRun={vi.fn()}
      onOpenProject={vi.fn()}
      onSendTurn={vi.fn()}
      project={fixtureProject()}
      run={null}
      timeline={emptyTimeline()}
      writeApprovalGateway={writeApprovalGateway}
    />,
  );
}

describe("approval re-list resilience", () => {
  it("recovers a transient list failure without ever alarming the user", async () => {
    const gateway = flakyGateway(1);
    renderWorkbench(gateway);

    await waitFor(
      () => {
        expect(screen.getByTestId("write-approval-bar")).toBeInTheDocument();
      },
      { timeout: RETRY_WAIT_MS },
    );
    // The retry is the reason the bar is here: one call would have thrown.
    expect(gateway.listCalls).toBe(2);
    expect(screen.queryByTestId("approvals-unavailable")).toBeNull();
  });

  it("survives failures right up to the retry budget", async () => {
    const gateway = flakyGateway(2);
    renderWorkbench(gateway);

    await waitFor(
      () => {
        expect(screen.getByTestId("write-approval-bar")).toBeInTheDocument();
      },
      { timeout: RETRY_WAIT_MS },
    );
    expect(gateway.listCalls).toBe(3);
    expect(screen.queryByTestId("approvals-unavailable")).toBeNull();
  });

  it("says the list is unreadable instead of showing an idle stage", async () => {
    const gateway = flakyGateway(Number.POSITIVE_INFINITY);
    renderWorkbench(gateway);

    await waitFor(
      () => {
        expect(screen.getByTestId("approvals-unavailable")).toBeInTheDocument();
      },
      { timeout: RETRY_WAIT_MS },
    );
    // Exhausted, not retrying forever.
    expect(gateway.listCalls).toBe(3);
    // Honest in both directions: no fabricated bar to click.
    expect(screen.queryByTestId("write-approval-bar")).toBeNull();
    expect(document.body.textContent).not.toContain(grantSecret);
    expect(document.body.innerHTML).not.toContain(grantSecret);
  });

  it("recovers the pending bar when the user retries", async () => {
    const user = userEvent.setup();
    let failing = true;
    const gateway: WriteApprovalGateway & { listCalls: number } = {
      listCalls: 0,
      listWriteProposals: async () => {
        gateway.listCalls += 1;
        if (failing) throw new Error("host list unavailable");
        return [pendingProposal()];
      },
      decideWriteProposal: async () => pendingProposal(),
      openWriteProposalDiff: async () =>
        ({
          proposalId,
          path: "src/example.ts",
          mode: "create",
          unified: "",
          truncated: false,
          lineCount: 0,
          byteCount: 0,
        }) satisfies WriteDiffView,
    };
    renderWorkbench(gateway);

    await waitFor(
      () => {
        expect(screen.getByTestId("approvals-unavailable")).toBeInTheDocument();
      },
      { timeout: RETRY_WAIT_MS },
    );

    failing = false;
    await user.click(screen.getByRole("button", { name: "重试" }));

    await waitFor(
      () => {
        expect(screen.getByTestId("write-approval-bar")).toBeInTheDocument();
      },
      { timeout: RETRY_WAIT_MS },
    );
    // The alarm clears once the list is readable again.
    expect(screen.queryByTestId("approvals-unavailable")).toBeNull();
  });

  it("blocks the retry button while a retry is in flight", async () => {
    const user = userEvent.setup();
    const gateway = flakyGateway(Number.POSITIVE_INFINITY);
    renderWorkbench(gateway);

    await waitFor(
      () => {
        expect(screen.getByTestId("approvals-unavailable")).toBeInTheDocument();
      },
      { timeout: RETRY_WAIT_MS },
    );
    expect(gateway.listCalls).toBe(3);

    const retry = screen.getByRole("button", { name: "重试" });
    await user.click(retry);
    // Mid-retry the control must be inert; otherwise repeated clicks stack
    // overlapping retry loops and multiply the calls against a failing host.
    await waitFor(() => {
      expect(screen.getByRole("button", { name: "重试中…" })).toBeDisabled();
    });

    await waitFor(
      () => {
        expect(screen.getByRole("button", { name: "重试" })).toBeEnabled();
      },
      { timeout: RETRY_WAIT_MS },
    );
    // One manual retry spends exactly one more budget, not more.
    expect(gateway.listCalls).toBe(6);
  });

  it("does not alarm when the list is readable and simply empty", async () => {
    const gateway = flakyGateway(0, []);
    renderWorkbench(gateway);

    await waitFor(() => {
      expect(gateway.listCalls).toBe(1);
    });
    expect(screen.queryByTestId("approvals-unavailable")).toBeNull();
    expect(screen.queryByTestId("write-approval-bar")).toBeNull();
  });
});
