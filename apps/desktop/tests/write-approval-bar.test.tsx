/**
 * Wave C3-E / EA-03 — single conversation approval bar focus, Tab, Escape, expiry.
 * decisionAvailable is a non-authority bit; raw grants never enter the bar.
 */
import { act, render, screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, describe, expect, it, vi } from "vitest";
import { WriteApprovalBar } from "../src/features/conversations/WriteApprovalBar";
import type {
  WriteApprovalGateway,
  WriteProposalView,
} from "../src/lib/writeApprovalGateway";
import "../src/styles/write-approval.css";

/** Forbidden secret string — must never reach DOM (privacy assertion). */
const grantSecret = "abcdefghijklmnopqrstuvwxyz0123456789ABCDEFG";
const proposalId = "prop-raw-id-must-not-show";

function pendingProposal(
  overrides: Partial<WriteProposalView> = {},
): WriteProposalView {
  return {
    proposalId,
    conversationId: "conv-a",
    path: "src/example.ts",
    mode: "create",
    state: "pending_approval",
    summary: "Create example module",
    expiresAt: new Date(Date.now() + 60_000).toISOString(),
    decisionAvailable: true,
    ruleSubject: "src/example.ts",
    ...overrides,
  };
}

function renderBar(
  props: Partial<{
    proposal: WriteProposalView;
    decisionAvailable: boolean;
    disabled: boolean;
    onDecide: ReturnType<typeof vi.fn>;
    onReviewDiff: ReturnType<typeof vi.fn>;
    onExpiryDeadline: ReturnType<typeof vi.fn>;
    writeApprovalGateway: WriteApprovalGateway;
  }> = {},
) {
  const onDecide = props.onDecide ?? vi.fn().mockResolvedValue(undefined);
  const onReviewDiff = props.onReviewDiff ?? vi.fn();
  const onExpiryDeadline = props.onExpiryDeadline ?? vi.fn();
  const proposal = props.proposal ?? pendingProposal();
  const view = render(
    <WriteApprovalBar
      decisionAvailable={
        props.decisionAvailable === undefined ? true : props.decisionAvailable
      }
      disabled={props.disabled}
      onDecide={onDecide}
      onExpiryDeadline={onExpiryDeadline}
      onReviewDiff={onReviewDiff}
      proposal={proposal}
      writeApprovalGateway={props.writeApprovalGateway}
    />,
  );
  return { ...view, onDecide, onReviewDiff, onExpiryDeadline, proposal };
}

afterEach(() => {
  vi.useRealTimers();
});

describe("WriteApprovalBar", () => {
  it("shows portable path, mode, expiry and action labels without ids or grant", () => {
    const { container } = renderBar();
    const bar = screen.getByTestId("write-approval-bar");
    expect(within(bar).getByTestId("write-approval-path")).toHaveTextContent(
      "src/example.ts",
    );
    expect(within(bar).getByTestId("write-approval-mode")).toHaveTextContent("创建");
    expect(screen.getByTestId("write-approval-approve")).toHaveTextContent("批准一次");
    expect(screen.getByTestId("write-approval-reject")).toHaveTextContent("拒绝");
    expect(screen.getByTestId("write-approval-review-diff")).toHaveTextContent(
      "查看差异",
    );

    const text = container.textContent ?? "";
    const html = container.innerHTML;
    expect(text).not.toContain(grantSecret);
    expect(text).not.toContain(proposalId);
    expect(html).not.toContain(grantSecret);
    expect(html).not.toContain(proposalId);
    expect(html).not.toMatch(/approvalGrant|data-grant|data-proposal/i);
  });

  it("focuses Approve once initially and Tab cycles only inside actions", async () => {
    const user = userEvent.setup();
    renderBar();
    const approve = screen.getByTestId("write-approval-approve");
    const reject = screen.getByTestId("write-approval-reject");
    const review = screen.getByTestId("write-approval-review-diff");
    expect(approve).toHaveFocus();

    await user.tab();
    expect(reject).toHaveFocus();
    await user.tab();
    expect(review).toHaveFocus();
    await user.tab();
    expect(approve).toHaveFocus();
    await user.tab({ shift: true });
    expect(review).toHaveFocus();
  });

  it("Escape rejects only while the bar owns focus", async () => {
    const user = userEvent.setup();
    const onDecide = vi.fn().mockResolvedValue(undefined);
    renderBar({ onDecide });
    const approve = screen.getByTestId("write-approval-approve");
    expect(approve).toHaveFocus();

    await user.keyboard("{Escape}");
    expect(onDecide).toHaveBeenCalledWith("reject");
  });

  it("Escape does nothing when focus is outside the bar", async () => {
    const user = userEvent.setup();
    const onDecide = vi.fn().mockResolvedValue(undefined);
    render(
      <div>
        <button data-testid="outside" type="button">
          outside
        </button>
        <WriteApprovalBar
          decisionAvailable
          onDecide={onDecide}
          onReviewDiff={vi.fn()}
          proposal={pendingProposal()}
        />
      </div>,
    );
    screen.getByTestId("outside").focus();
    await user.keyboard("{Escape}");
    expect(onDecide).not.toHaveBeenCalled();
  });

  it("disables actions after submit and does not double-fire", async () => {
    const user = userEvent.setup();
    let resolveDecide: (() => void) | undefined;
    const onDecide = vi.fn(
      () =>
        new Promise<void>((resolve) => {
          resolveDecide = resolve;
        }),
    );
    renderBar({ onDecide });
    const approve = screen.getByTestId("write-approval-approve");
    await user.click(approve);
    expect(onDecide).toHaveBeenCalledTimes(1);
    expect(approve).toBeDisabled();
    expect(screen.getByTestId("write-approval-reject")).toBeDisabled();
    await user.click(screen.getByTestId("write-approval-reject"));
    expect(onDecide).toHaveBeenCalledTimes(1);
    resolveDecide?.();
  });

  it("locks Approve/Reject when decisionAvailable is false; Review stays available", async () => {
    const user = userEvent.setup();
    const onDecide = vi.fn().mockResolvedValue(undefined);
    const onReviewDiff = vi.fn().mockResolvedValue(undefined);
    renderBar({
      decisionAvailable: false,
      onDecide,
      onReviewDiff,
    });
    expect(screen.getByTestId("write-approval-approve")).toBeDisabled();
    expect(screen.getByTestId("write-approval-reject")).toBeDisabled();
    expect(screen.getByTestId("write-approval-review-diff")).not.toBeDisabled();
    await user.click(screen.getByTestId("write-approval-approve"));
    expect(onDecide).not.toHaveBeenCalled();
    await user.click(screen.getByTestId("write-approval-review-diff"));
    expect(onReviewDiff).toHaveBeenCalledTimes(1);
  });

  it("locks Approve when the host-marked diff is truncated (EA-04 reproduction)", async () => {
    const user = userEvent.setup();
    const onDecide = vi.fn().mockResolvedValue(undefined);
    renderBar({
      onDecide,
      proposal: pendingProposal({
        diffTruncated: true,
        contentDigest:
          "sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
        contentReviewComplete: false,
      }),
    });
    const approve = screen.getByTestId("write-approval-approve");
    expect(approve).toBeDisabled();
    expect(screen.getByTestId("write-approval-reject")).not.toBeDisabled();
    expect(screen.getByTestId("write-approval-truncated-locked")).toHaveTextContent(
      "完整",
    );
    expect(screen.getByTestId("write-approval-content-digest")).toHaveTextContent(
      "sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
    );
    await user.click(approve);
    expect(onDecide).not.toHaveBeenCalled();
  });

  it("restores Approve after every proposed-content page is retrieved (EA-04)", async () => {
    const user = userEvent.setup();
    const onDecide = vi.fn().mockResolvedValue(undefined);
    const digest =
      "sha256:bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb";
    const pages = [
      {
        proposalId,
        path: "src/example.ts",
        mode: "create" as const,
        contentDigest: digest,
        pageIndex: 0,
        pageCount: 2,
        text: "page-zero\n",
        lineCount: 1,
        byteCount: 10,
        reviewComplete: false,
      },
      {
        proposalId,
        path: "src/example.ts",
        mode: "create" as const,
        contentDigest: digest,
        pageIndex: 1,
        pageCount: 2,
        text: "HIDDEN_TAIL\n",
        lineCount: 1,
        byteCount: 12,
        reviewComplete: true,
      },
    ];
    const writeApprovalGateway = {
      listWriteProposals: vi.fn(),
      decideWriteProposal: vi.fn(),
      openWriteProposalDiff: vi.fn(),
      openWriteProposalContentPage: vi
        .fn()
        .mockResolvedValueOnce(pages[0])
        .mockResolvedValueOnce(pages[1]),
    };
    render(
      <WriteApprovalBar
        decisionAvailable
        onDecide={onDecide}
        onReviewDiff={vi.fn()}
        proposal={pendingProposal({
          diffTruncated: true,
          contentDigest: digest,
          contentReviewComplete: false,
        })}
        writeApprovalGateway={writeApprovalGateway}
      />,
    );
    expect(screen.getByTestId("write-approval-approve")).toBeDisabled();
    await user.click(screen.getByTestId("write-approval-review-content"));
    expect(await screen.findByTestId("write-approval-content-page")).toHaveTextContent(
      "page-zero",
    );
    expect(screen.getByTestId("write-approval-approve")).toBeDisabled();
    await user.click(screen.getByTestId("write-approval-review-content"));
    expect(await screen.findByTestId("write-approval-content-page")).toHaveTextContent(
      "HIDDEN_TAIL",
    );
    expect(screen.getByTestId("write-approval-approve")).not.toBeDisabled();
    await user.click(screen.getByTestId("write-approval-approve"));
    expect(onDecide).toHaveBeenCalledWith("approve_once");
  });

  it("disables all actions after expiry deadline", async () => {
    vi.useFakeTimers({ shouldAdvanceTime: true });
    const onExpiryDeadline = vi.fn();
    const expiresAt = new Date(Date.now() + 2_000).toISOString();
    renderBar({
      proposal: pendingProposal({ expiresAt }),
      onExpiryDeadline,
    });
    expect(screen.getByTestId("write-approval-approve")).not.toBeDisabled();
    await act(async () => {
      vi.advanceTimersByTime(2_500);
    });
    expect(screen.getByTestId("write-approval-approve")).toBeDisabled();
    expect(screen.getByTestId("write-approval-reject")).toBeDisabled();
    expect(screen.getByTestId("write-approval-review-diff")).toBeDisabled();
    expect(onExpiryDeadline).toHaveBeenCalled();
  });
});
