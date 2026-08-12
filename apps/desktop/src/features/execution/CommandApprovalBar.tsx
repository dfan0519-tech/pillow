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
import { encodeProcessSubject } from "../permissions/processSubject";
import type {
  CommandDecision,
  CommandProposalView,
} from "./executionTypes";
import {
  COMMAND_DISPLAY_EXPAND_THRESHOLD,
  formatCommandArgvDisplay,
  isShellProgram,
} from "./executionTypes";
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
  /** Host-normalized shell subjects from the same conservative split used for rules. */
  ruleSegments?: readonly string[];
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

/**
 * Fallback match subject for structured commands when the host omits
 * `ruleSubject`. Shell commands must never invent a subject in the renderer.
 */
/**
 * Subject to use when the host omitted `rule_subject`.
 *
 * Must use the host encoding. A space-joined fallback does not decode host-side,
 * so any rule saved from it would fail closed and never fire again — silently
 * useless to the user, who believes they granted something.
 */
function fallbackProcessSubject(proposal: CommandProposalView): string {
  return encodeProcessSubject(proposal.programLabel, proposal.argv);
}

export function CommandApprovalBar({
  proposal,
  decisionAvailable,
  disabled = false,
  onDecide,
  onReview,
  onExpiryDeadline,
  ruleSubject: ruleSubjectProp,
  ruleSegments: ruleSegmentsProp,
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
  const [commandExpanded, setCommandExpanded] = useState(false);
  const [alwaysRuleAction, setAlwaysRuleAction] = useState<
    "allow" | "deny" | null
  >(null);
  const decisionLocked =
    disabled || submitting || deadlinePassed || !decisionAvailable;
  const reviewLocked = disabled || submitting || deadlinePassed;
  const isShell = isShellProgram(proposal.programLabel);
  const ruleSubject = isShell
    ? ""
    : ruleSubjectProp
      ?? readRuleSubject(proposal)
      ?? fallbackProcessSubject(proposal);
  const ruleSubjects = isShell
    ? [...(ruleSegmentsProp ?? [])]
    : ruleSubject
      ? [ruleSubject]
      : [];
  const showAlwaysRule = Boolean(
    permissionRulesGateway && ruleSubjects.length > 0,
  );
  const commandText = formatCommandArgvDisplay(
    proposal.programLabel,
    proposal.argv,
  );
  const isLongCommand = commandText.length > COMMAND_DISPLAY_EXPAND_THRESHOLD;

  useLayoutEffect(() => {
    approveRef.current?.focus();
  }, [proposal.proposalId]);

  useEffect(() => {
    setSubmitting(false);
    setDeadlinePassed(false);
    setCommandExpanded(false);
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

  return (
    <div
      ref={barRef}
      aria-labelledby={headingId}
      className="command-approval-bar"
      data-testid="command-approval-bar"
      data-command-kind={isShell ? "shell" : "structured"}
      onKeyDown={handleBarKeyDown}
      role="region"
    >
      <div className="command-approval-bar__meta">
        <h2 className="command-approval-bar__title" id={headingId}>
          命令批准
        </h2>
        {isShell ? (
          <p className="command-approval-bar__command command-approval-bar__command--shell">
            <span
              className="command-approval-bar__kind"
              data-testid="command-approval-kind"
            >
              Shell 命令
            </span>
            <code
              className={
                commandExpanded
                  ? "command-approval-bar__command-text command-approval-bar__command-text--expanded"
                  : "command-approval-bar__command-text"
              }
              data-full-command={commandText}
              data-testid="command-approval-argv"
              title={commandText}
            >
              {commandText}
            </code>
            {isLongCommand ? (
              <button
                className="command-approval-bar__expand"
                data-testid="command-approval-expand-command"
                onClick={() => setCommandExpanded((open) => !open)}
                type="button"
              >
                {commandExpanded ? "收起" : "展开完整命令"}
              </button>
            ) : null}
          </p>
        ) : (
          <p className="command-approval-bar__command">
            <code data-testid="command-approval-program">
              {proposal.programLabel}
            </code>
            <span aria-hidden="true" className="command-approval-bar__sep">
              {" "}
            </span>
            <code
              className={
                commandExpanded
                  ? "command-approval-bar__command-text command-approval-bar__command-text--expanded"
                  : "command-approval-bar__command-text"
              }
              data-full-command={commandText}
              data-testid="command-approval-argv"
              title={commandText}
            >
              {commandText}
            </code>
            {isLongCommand ? (
              <button
                className="command-approval-bar__expand"
                data-testid="command-approval-expand-command"
                onClick={() => setCommandExpanded((open) => !open)}
                type="button"
              >
                {commandExpanded ? "收起" : "展开完整命令"}
              </button>
            ) : null}
          </p>
        )}
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
            {isShell
              ? "Shell 命令按字面执行；项目路径约束不是操作系统沙箱。"
              : "高风险参数可见为字面参数；项目路径约束不是操作系统沙箱。"}
          </p>
        ) : null}
        {permissionRulesGateway && isShell ? (
          <p
            className={
              ruleSubjects.length > 0
                ? "command-approval-bar__rule-note"
                : "command-approval-bar__rule-note command-approval-bar__rule-note--blocked"
            }
            data-testid="command-approval-shell-rule-note"
            role="note"
          >
            {ruleSubjects.length > 0
              ? `该 Shell 命令已拆为 ${ruleSubjects.length} 段；自动放行必须分别保存 ${ruleSubjects.length} 条允许规则。`
              : "命令结构过于复杂，无法安全拆分；本次需逐次确认，不提供“总是允许”。"}
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
              {isShell
                ? `逐段设置总是允许（${ruleSubjects.length} 条规则）`
                : "总是允许此模式"}
            </button>
            {!isShell ? (
              <button
                className="command-approval-bar__btn"
                data-testid="command-approval-always-deny"
                disabled={disabled || submitting || deadlinePassed}
                onClick={() => setAlwaysRuleAction("deny")}
                type="button"
              >
                总是拒绝此模式
              </button>
            ) : null}
          </>
        ) : null}
      </div>
      {alwaysRuleAction && permissionRulesGateway ? (
        <AlwaysRuleDialog
          exactSubjectPatterns={isShell}
          gateway={permissionRulesGateway}
          initialAction={alwaysRuleAction}
          onClose={() => setAlwaysRuleAction(null)}
          prefix="process"
          projectId={projectId}
          ruleSubjects={ruleSubjects}
        />
      ) : null}
    </div>
  );
}
