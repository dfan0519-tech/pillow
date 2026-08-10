/**
 * Single conversation-owned process approval bar (C4.0).
 * Approve once · Reject · Review command.
 * Controlled component: sanitized DTOs + callbacks only; no raw grants.
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
import { AlwaysRuleDialog } from "../permissions/AlwaysRuleDialog";
import type {
  CommandDecision,
  CommandProposalView,
} from "./executionTypes";
import { formatArgvDisplay } from "./executionTypes";
import "../../styles/execution.css";

export type CommandApprovalBarProps = {
  /** Display-safe proposal — must not include approvalGrant. */
  proposal: CommandProposalView;
  /**
   * Sanitized enable signal: parent may authorize Approve/Reject.
   * Non-secret boolean only — never a raw grant string.
   */
  decisionAvailable: boolean;
  disabled?: boolean;
  onDecide: (decision: CommandDecision) => void | Promise<void>;
  /** Opens read-only sealed command / terminal review (not execute). */
  onReview: () => void | Promise<void>;
  /** Called when the displayed deadline elapses (disable + parent re-list). */
  onExpiryDeadline?: () => void;
  /**
   * Optional host-normalized match subject (`rule_subject`). When omitted,
   * falls back to program + argv display text.
   */
  ruleSubject?: string;
  /** When set, enables “always allow/deny this pattern” from the bar. */
  permissionRulesGateway?: PermissionRulesGateway;
  /** Active project — enables project-scoped deny rules from the dialog. */
  projectId?: string | null;
};

function riskLabel(risk: CommandProposalView["riskClass"]): string {
  return risk === "high" ? "高风险" : "标准";
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

function fallbackProcessSubject(proposal: CommandProposalView): string {
  const argvText = proposal.argv.join(" ").trim();
  return argvText
    ? `${proposal.programLabel} ${argvText}`.trim()
    : proposal.programLabel;
}

export function CommandApprovalBar({
  proposal,
  decisionAvailable,
  disabled = false,
  onDecide,
  onReview,
  onExpiryDeadline,
  ruleSubject: ruleSubjectProp,
  permissionRulesGateway,
  projectId = null,
}: CommandApprovalBarProps) {
  const headingId = useId();
  const barRef = useRef<HTMLDivElement>(null);
  const approveRef = useRef<HTMLButtonElement>(null);
  const rejectRef = useRef<HTMLButtonElement>(null);
  const reviewRef = useRef<HTMLButtonElement>(null);

  const [submitting, setSubmitting] = useState(false);
  const [deadlinePassed, setDeadlinePassed] = useState(false);
  const [alwaysRuleAction, setAlwaysRuleAction] = useState<
    "allow" | "deny" | null
  >(null);
  const decisionLocked =
    disabled || submitting || deadlinePassed || !decisionAvailable;
  const reviewLocked = disabled || submitting || deadlinePassed;
  const ruleSubject =
    ruleSubjectProp
    ?? readRuleSubject(proposal)
    ?? fallbackProcessSubject(proposal);
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
    async (decision: CommandDecision) => {
      if (decisionLocked) return;
      setSubmitting(true);
      try {
        await onDecide(decision);
      } finally {
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

  const argvText = formatArgvDisplay(proposal.argv);

  return (
    <div
      ref={barRef}
      aria-labelledby={headingId}
      className="command-approval-bar"
      data-testid="command-approval-bar"
      onKeyDown={handleBarKeyDown}
      role="region"
    >
      <div className="command-approval-bar__meta">
        <h2 className="command-approval-bar__title" id={headingId}>
          命令批准
        </h2>
        <p className="command-approval-bar__command">
          <code data-testid="command-approval-program">
            {proposal.programLabel}
          </code>
          <span aria-hidden="true" className="command-approval-bar__sep">
            {" "}
          </span>
          <code data-testid="command-approval-argv">{argvText}</code>
        </p>
        <p className="command-approval-bar__details">
          <span data-testid="command-approval-cwd">
            <code>{proposal.portableCwd}</code>
          </span>
          <span aria-hidden="true" className="command-approval-bar__sep">
            ·
          </span>
          <span data-testid="command-approval-risk">
            {riskLabel(proposal.riskClass)}
          </span>
          <span aria-hidden="true" className="command-approval-bar__sep">
            ·
          </span>
          <span data-testid="command-approval-expiry">
            截止 {formatExpiry(proposal.expiresAt)}
          </span>
        </p>
        {proposal.summary ? (
          <p
            className="command-approval-bar__summary"
            data-testid="command-approval-summary"
          >
            {proposal.summary}
          </p>
        ) : null}
        {proposal.riskClass === "high" ? (
          <p
            className="command-approval-bar__risk-note"
            data-testid="command-approval-risk-note"
            role="note"
          >
            高风险参数可见为字面参数；项目路径约束不是操作系统沙箱。
          </p>
        ) : null}
      </div>
      <div
        aria-label="命令批准操作"
        className="command-approval-bar__actions"
        data-testid="command-approval-actions"
      >
        <button
          ref={approveRef}
          className="command-approval-bar__btn command-approval-bar__btn--primary"
          data-testid="command-approval-approve"
          disabled={decisionLocked}
          onClick={() => void submit("approve_once")}
          type="button"
        >
          批准一次
        </button>
        <button
          ref={rejectRef}
          className="command-approval-bar__btn"
          data-testid="command-approval-reject"
          disabled={decisionLocked}
          onClick={() => void submit("reject")}
          type="button"
        >
          拒绝
        </button>
        <button
          ref={reviewRef}
          className="command-approval-bar__btn"
          data-testid="command-approval-review"
          disabled={reviewLocked}
          onClick={() => void onReview()}
          type="button"
        >
          查看命令
        </button>
        {showAlwaysRule ? (
          <>
            <button
              className="command-approval-bar__btn"
              data-testid="command-approval-always-allow"
              disabled={disabled || submitting || deadlinePassed}
              onClick={() => setAlwaysRuleAction("allow")}
              type="button"
            >
              总是允许此模式
            </button>
            <button
              className="command-approval-bar__btn"
              data-testid="command-approval-always-deny"
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
          prefix="process"
          projectId={projectId}
          ruleSubject={ruleSubject}
        />
      ) : null}
    </div>
  );
}
