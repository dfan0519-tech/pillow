/**
 * Single conversation-owned write approval bar (Wave C3-E / EA-03).
 * One pending proposal at a time; decisions use host-private grants only.
 * Renderer holds a non-authority decisionAvailable bit — never raw grant.
 */

import {
  useCallback,
  useEffect,
  useId,
  useLayoutEffect,
  useRef,
  useState,
  type KeyboardEvent as ReactKeyboardEvent,
} from "react";
import {
  readRuleSubject,
  type PermissionRulesGateway,
} from "../../lib/permissionRulesGateway";
import type {
  WriteApprovalGateway,
  WriteDecision,
  WriteProposalView,
} from "../../lib/writeApprovalGateway";
import { AlwaysRuleDialog } from "../permissions/AlwaysRuleDialog";
import {
  writePrefixFromMode,
} from "../permissions/permissionPattern";
import "../../styles/write-approval.css";

export type WriteApprovalBarProps = {
  /** Display-safe proposal — never includes raw approval grant. */
  proposal: WriteProposalView;
  /**
   * Non-authority availability bit from the host.
   * Enables Approve/Reject when true; host still owns the grant.
   */
  decisionAvailable: boolean;
  disabled?: boolean;
  onDecide: (decision: WriteDecision) => void | Promise<void>;
  onReviewDiff: () => void | Promise<void>;
  /** Called when the displayed deadline elapses (disable + parent re-list). */
  onExpiryDeadline?: () => void;
  /**
   * Optional host-normalized match subject (`rule_subject`). When omitted,
   * falls back to the portable proposal path.
   */
  ruleSubject?: string;
  /** When set, enables “always allow/deny this pattern” from the bar. */
  permissionRulesGateway?: PermissionRulesGateway;
  /** Active project — enables project-scoped deny rules from the dialog. */
  projectId?: string | null;
  /**
   * Optional write gateway used only to page sealed proposed content when
   * the host-built diff is truncated (EA-04). Decisions still go through
   * `onDecide`; this never carries a grant.
   */
  writeApprovalGateway?: WriteApprovalGateway;
};

function modeLabel(mode: WriteProposalView["mode"]): string {
  return mode === "create" ? "创建" : "修改";
}

function formatExpiry(expiresAt: string): string {
  const ms = Date.parse(expiresAt);
  if (Number.isNaN(ms)) return expiresAt;
  try {
    return new Date(ms).toLocaleString();
  } catch {
    return expiresAt;
  }
}

export function WriteApprovalBar({
  proposal,
  decisionAvailable,
  disabled = false,
  onDecide,
  onReviewDiff,
  onExpiryDeadline,
  ruleSubject: ruleSubjectProp,
  permissionRulesGateway,
  projectId = null,
  writeApprovalGateway,
}: WriteApprovalBarProps) {
  const headingId = useId();
  const barRef = useRef<HTMLDivElement>(null);
  const approveRef = useRef<HTMLButtonElement>(null);
  const rejectRef = useRef<HTMLButtonElement>(null);
  const reviewRef = useRef<HTMLButtonElement>(null);
  const reviewContentRef = useRef<HTMLButtonElement>(null);

  const [submitting, setSubmitting] = useState(false);
  const [deadlinePassed, setDeadlinePassed] = useState(false);
  const [alwaysRuleAction, setAlwaysRuleAction] = useState<
    "allow" | "deny" | null
  >(null);
  const [reviewPageIndex, setReviewPageIndex] = useState(-1);
  const [reviewPageText, setReviewPageText] = useState<string | null>(null);
  const [reviewPageMeta, setReviewPageMeta] = useState<{
    pageIndex: number;
    pageCount: number;
  } | null>(null);
  const [localReviewComplete, setLocalReviewComplete] = useState(false);
  const [reviewBusy, setReviewBusy] = useState(false);

  const contentDigest = proposal.contentDigest;
  const hostReviewComplete = proposal.contentReviewComplete === true;
  const diffTruncated = proposal.diffTruncated === true;
  const reviewComplete = !diffTruncated || hostReviewComplete || localReviewComplete;
  const truncatedUnreviewed = diffTruncated && !reviewComplete;

  /** Approve/Reject require host decision availability and a live deadline. */
  const decisionLocked =
    disabled || submitting || deadlinePassed || !decisionAvailable;
  const approveLocked = decisionLocked || truncatedUnreviewed;
  /** Review diff is allowed without decision, but not after the displayed deadline. */
  const reviewLocked = disabled || submitting || deadlinePassed;
  const ruleSubject =
    ruleSubjectProp
    ?? readRuleSubject(proposal)
    ?? proposal.path;
  const showAlwaysRule = Boolean(permissionRulesGateway && ruleSubject);

  useLayoutEffect(() => {
    if (truncatedUnreviewed) {
      (reviewContentRef.current ?? rejectRef.current)?.focus();
    } else {
      approveRef.current?.focus();
    }
  }, [proposal.proposalId, truncatedUnreviewed]);

  useEffect(() => {
    setSubmitting(false);
    setDeadlinePassed(false);
    setReviewPageIndex(-1);
    setReviewPageText(null);
    setReviewPageMeta(null);
    setLocalReviewComplete(proposal.contentReviewComplete === true);
    setReviewBusy(false);
  }, [proposal.proposalId, proposal.expiresAt, proposal.contentReviewComplete]);

  useEffect(() => {
    const expiresMs = Date.parse(proposal.expiresAt);
    if (Number.isNaN(expiresMs)) return;

    const fire = () => {
      setDeadlinePassed(true);
      onExpiryDeadline?.();
    };

    const remaining = expiresMs - Date.now();
    if (remaining <= 0) {
      fire();
      return;
    }
    const timer = window.setTimeout(fire, remaining);
    return () => window.clearTimeout(timer);
  }, [proposal.proposalId, proposal.expiresAt, onExpiryDeadline]);

  const submit = useCallback(
    async (decision: WriteDecision) => {
      if (decision === "approve_once") {
        if (approveLocked) return;
      } else if (decisionLocked) {
        return;
      }
      setSubmitting(true);
      try {
        await onDecide(decision);
      } finally {
        setSubmitting(false);
      }
    },
    [approveLocked, decisionLocked, onDecide],
  );

  const loadContentPage = useCallback(async () => {
    const openPage = writeApprovalGateway?.openWriteProposalContentPage;
    if (!openPage || reviewBusy || reviewLocked) return;
    const nextIndex = reviewPageIndex < 0 ? 0 : reviewPageIndex + 1;
    setReviewBusy(true);
    try {
      const page = await openPage({
        conversationId: proposal.conversationId,
        proposalId: proposal.proposalId,
        pageIndex: nextIndex,
      });
      setReviewPageIndex(page.pageIndex);
      setReviewPageText(page.text);
      setReviewPageMeta({
        pageIndex: page.pageIndex,
        pageCount: page.pageCount,
      });
      if (page.reviewComplete) {
        setLocalReviewComplete(true);
      }
      void onReviewDiff();
    } finally {
      setReviewBusy(false);
    }
  }, [
    onReviewDiff,
    proposal.conversationId,
    proposal.proposalId,
    reviewBusy,
    reviewLocked,
    reviewPageIndex,
    writeApprovalGateway,
  ]);

  function actionButtons(): HTMLButtonElement[] {
    return [
      approveRef.current,
      rejectRef.current,
      reviewRef.current,
      reviewContentRef.current,
    ].filter(
      (node): node is HTMLButtonElement => node !== null && !node.disabled,
    );
  }

  function handleBarKeyDown(event: ReactKeyboardEvent<HTMLDivElement>) {
    const bar = barRef.current;
    if (!bar) return;
    const active = document.activeElement;
    const ownsFocus = active instanceof Node && bar.contains(active);

    if (event.key === "Escape") {
      if (!ownsFocus) return;
      event.preventDefault();
      event.stopPropagation();
      if (!decisionLocked) {
        void submit("reject");
      }
      return;
    }

    if (event.key !== "Tab" || !ownsFocus) return;

    const buttons = actionButtons();
    if (buttons.length === 0) return;
    const currentIndex = buttons.findIndex((button) => button === active);
    if (currentIndex < 0) return;

    event.preventDefault();
    const delta = event.shiftKey ? -1 : 1;
    const nextIndex = (currentIndex + delta + buttons.length) % buttons.length;
    buttons[nextIndex]?.focus();
  }

  return (
    <div
      ref={barRef}
      aria-labelledby={headingId}
      className="write-approval-bar"
      data-testid="write-approval-bar"
      onKeyDown={handleBarKeyDown}
      role="region"
    >
      <div className="write-approval-bar__meta">
        <h2 className="write-approval-bar__title" id={headingId}>
          写入批准
        </h2>
        <p className="write-approval-bar__path" data-testid="write-approval-path">
          <code>{proposal.path}</code>
        </p>
        <p className="write-approval-bar__details">
          <span data-testid="write-approval-mode">{modeLabel(proposal.mode)}</span>
          <span aria-hidden="true" className="write-approval-bar__sep">
            ·
          </span>
          <span data-testid="write-approval-expiry">
            截止 {formatExpiry(proposal.expiresAt)}
          </span>
        </p>
        {proposal.summary ? (
          <p className="write-approval-bar__summary" data-testid="write-approval-summary">
            {proposal.summary}
          </p>
        ) : null}
        {contentDigest ? (
          <p
            className="write-approval-bar__digest"
            data-testid="write-approval-content-digest"
          >
            内容摘要 {contentDigest}
          </p>
        ) : null}
        {truncatedUnreviewed ? (
          <p
            className="write-approval-bar__truncation"
            data-testid="write-approval-truncated-locked"
            role="status"
          >
            差异已截断，须审查完整拟写入内容后才能批准
          </p>
        ) : null}
        {reviewPageText !== null ? (
          <pre
            className="write-approval-bar__review-body"
            data-testid="write-approval-content-page"
            tabIndex={0}
          >
            {reviewPageText}
          </pre>
        ) : null}
        {reviewPageMeta ? (
          <p className="write-approval-bar__review-meta" data-testid="write-approval-content-page-meta">
            完整内容 {reviewPageMeta.pageIndex + 1}/{reviewPageMeta.pageCount}
          </p>
        ) : null}
      </div>
      <div
        aria-label="写入批准操作"
        className="write-approval-bar__actions"
        data-testid="write-approval-actions"
      >
        <button
          ref={approveRef}
          className="write-approval-bar__btn write-approval-bar__btn--primary"
          data-testid="write-approval-approve"
          disabled={approveLocked}
          onClick={() => void submit("approve_once")}
          type="button"
        >
          批准一次
        </button>
        <button
          ref={rejectRef}
          className="write-approval-bar__btn"
          data-testid="write-approval-reject"
          disabled={decisionLocked}
          onClick={() => void submit("reject")}
          type="button"
        >
          拒绝
        </button>
        <button
          ref={reviewRef}
          className="write-approval-bar__btn"
          data-testid="write-approval-review-diff"
          disabled={reviewLocked}
          onClick={() => void onReviewDiff()}
          type="button"
        >
          查看差异
        </button>
        {truncatedUnreviewed && writeApprovalGateway?.openWriteProposalContentPage ? (
          <button
            ref={reviewContentRef}
            className="write-approval-bar__btn"
            data-testid="write-approval-review-content"
            disabled={reviewLocked || reviewBusy}
            onClick={() => void loadContentPage()}
            type="button"
          >
            {reviewPageIndex < 0 ? "审查完整内容" : "下一页"}
          </button>
        ) : null}
        {showAlwaysRule ? (
          <>
            <button
              className="write-approval-bar__btn"
              data-testid="write-approval-always-allow"
              disabled={disabled || submitting || deadlinePassed}
              onClick={() => setAlwaysRuleAction("allow")}
              type="button"
            >
              总是允许此模式
            </button>
            <button
              className="write-approval-bar__btn"
              data-testid="write-approval-always-deny"
              disabled={disabled || submitting || deadlinePassed}
              onClick={() => setAlwaysRuleAction("deny")}
              type="button"
            >
              总是拒绝此模式
            </button>
          </>
        ) : null}
      </div>
      {alwaysRuleAction && permissionRulesGateway ? (
        <AlwaysRuleDialog
          gateway={permissionRulesGateway}
          initialAction={alwaysRuleAction}
          onClose={() => setAlwaysRuleAction(null)}
          prefix={writePrefixFromMode(proposal.mode)}
          projectId={projectId}
          ruleSubjects={[ruleSubject]}
        />
      ) : null}
    </div>
  );
}
