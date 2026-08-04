/**
 * Wave C3-E — single conversation approval bar focus, Tab, Escape, expiry.
 */
import { act, render, screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, describe, expect, it, vi } from "vitest";
import { WriteApprovalBar } from "../src/features/conversations/WriteApprovalBar";
import type { WriteProposalView } from "../src/lib/writeApprovalGateway";
import "../src/styles/write-approval.css";

/** Canonical 43-char base64url grant. */
const grant = "abcdefghijklmnopqrstuvwxyz0123456789ABCDEFG";
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
    ...overrides,
  };
}

function renderBar(
  props: Partial<{
    proposal: WriteProposalView;
    approvalGrant: string | null;
    disabled: boolean;
    onDecide: ReturnType<typeof vi.fn>;
    onReviewDiff: ReturnType<typeof vi.fn>;
    onExpiryDeadline: ReturnType<typeof vi.fn>;
  }> = {},
) {
  const onDecide = props.onDecide ?? vi.fn().mockResolvedValue(undefined);
  const onReviewDiff = props.onReviewDiff ?? vi.fn();
  const onExpiryDeadline = props.onExpiryDeadline ?? vi.fn();
  const proposal = props.proposal ?? pendingProposal();
  const view = render(
    <WriteApprovalBar
      approvalGrant={props.approvalGrant === undefined ? grant : props.approvalGrant}
      disabled={props.disabled}
      onDecide={onDecide}
      onExpiryDeadline={onExpiryDeadline}
      onReviewDiff={onReviewDiff}
      proposal={proposal}
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
    expect(text).not.toContain(grant);
    expect(text).not.toContain(proposalId);
    expect(html).not.toContain(grant);
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
          approvalGrant={grant}
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
    await act(async () => {
      resolveDecide?.();
    });
  });

  it("disables all three actions at expiry deadline without inventing a decision", async () => {
    vi.useFakeTimers();
    const onDecide = vi.fn();
    const onReviewDiff = vi.fn();
    const onExpiryDeadline = vi.fn();
    const expiresAt = new Date(Date.now() + 5_000).toISOString();
    renderBar({
      proposal: pendingProposal({ expiresAt }),
      onDecide,
      onReviewDiff,
      onExpiryDeadline,
    });

    await act(async () => {
      await vi.advanceTimersByTimeAsync(5_001);
    });

    expect(onExpiryDeadline).toHaveBeenCalledTimes(1);
    expect(onDecide).not.toHaveBeenCalled();
    expect(screen.getByTestId("write-approval-approve")).toBeDisabled();
    expect(screen.getByTestId("write-approval-reject")).toBeDisabled();
    expect(screen.getByTestId("write-approval-review-diff")).toBeDisabled();

    await act(async () => {
      screen.getByTestId("write-approval-review-diff").click();
    });
    expect(onReviewDiff).not.toHaveBeenCalled();
  });

  it("never places the grant into console output on decide", async () => {
    const user = userEvent.setup();
    const errorSpy = vi.spyOn(console, "error").mockImplementation(() => {});
    const logSpy = vi.spyOn(console, "log").mockImplementation(() => {});
    const onDecide = vi.fn().mockResolvedValue(undefined);
    renderBar({ onDecide });
    await user.click(screen.getByTestId("write-approval-approve"));
    expect(onDecide).toHaveBeenCalledWith("approve_once");
    for (const call of [...errorSpy.mock.calls, ...logSpy.mock.calls]) {
      expect(JSON.stringify(call)).not.toContain(grant);
    }
    errorSpy.mockRestore();
    logSpy.mockRestore();
  });
});
