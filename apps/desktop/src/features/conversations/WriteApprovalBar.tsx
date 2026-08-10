/**
 * Single conversation-owned write approval bar (Wave C3-E).
 * One pending proposal at a time; grant stays in component memory only.
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
  WriteDecision,
  WriteProposalView,
} from "../../lib/writeApprovalGateway";
import { AlwaysRuleDialog } from "../permissions/AlwaysRuleDialog";
import {
  writePrefixFromMode,
} from "../permissions/permissionPattern";
import "../../styles/write-approval.css";

export type WriteApprovalBarProps = {
  /** Display-safe proposal — must not include approvalGrant. */
  proposal: WriteProposalView;
  /**
   * Opaque one-shot grant for the exact owning pending proposal.
   * Held only in this component's memory for the native decision call.
   * Never rendered, logged, stored, or snapshotted.
   */
  approvalGrant: string | null;
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
  approvalGrant,
  disabled = false,
  onDecide,
  onReviewDiff,
  onExpiryDeadline,
  ruleSubject: ruleSubjectProp,
  permissionRulesGateway,
  projectId = null,
}: WriteApprovalBarProps) {
  const headingId = useId();
  const barRef = useRef<HTMLDivElement>(null);
  const approveRef = useRef<HTMLButtonElement>(null);
  const rejectRef = useRef<HTMLButtonElement>(null);
  const reviewRef = useRef<HTMLButtonElement>(null);
  /** Local grant mirror — never written into React state objects that re-render. */
  const grantRef = useRef<string | null>(approvalGrant);
  grantRef.current = approvalGrant;

  const [submitting, setSubmitting] = useState(false);
  const [deadlinePassed, setDeadlinePassed] = useState(false);
  const [alwaysRuleAction, setAlwaysRuleAction] = useState<
    "allow" | "deny" | null
  >(null);
  /** Approve/Reject require a live grant and a non-expired deadline. */
  const decisionLocked =
    disabled || submitting || deadlinePassed || !approvalGrant;
  /** Review diff is allowed without grant, but not after the displayed deadline. */
  const reviewLocked = disabled || submitting || deadlinePassed;
  const ruleSubject =
    ruleSubjectProp
    ?? readRuleSubject(proposal)
    ?? proposal.path;
  const showAlwaysRule = Boolean(permissionRulesGateway && ruleSubject);

  useLayoutEffect(() => {
    approveRef.current?.focus();
  }, [proposal.proposalId]);

  useEffect(() => {
    setSubmitting(false);
    setDeadlinePassed(false);
  }, [proposal.proposalId, proposal.expiresAt]);

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
      if (decisionLocked) return;
      const grant = grantRef.current;
      if (!grant) return;
      setSubmitting(true);
      try {
        await onDecide(decision);
      } finally {
        // Parent clears grant after settle; local ref follows prop updates.
        setSubmitting(false);
      }
    },
    [decisionLocked, onDecide],
  );

  function actionButtons(): HTMLButtonElement[] {
    return [approveRef.current, rejectRef.current, reviewRef.current].filter(
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
          disabled={decisionLocked}
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
          ruleSubject={ruleSubject}
        />
      ) : null}
    </div>
  );
}
