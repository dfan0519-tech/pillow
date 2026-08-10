/**
 * Confirm dialog for “always allow / deny this pattern” from an approval bar.
 * Shows the exact pattern that will be written and the word-boundary warning.
 */

import { useId, useState, type FormEvent } from "react";
import type {
  PermissionRuleAction,
  PermissionRulePrefix,
  PermissionRuleScope,
  PermissionRulesGateway,
} from "../../lib/permissionRulesGateway";
import {
  isProjectAllowForbidden,
} from "../../lib/permissionRulesGateway";
import {
  PERMISSION_WORD_BOUNDARY_NOTE,
  actionLabel,
  prefixLabel,
  suggestPatternFromSubject,
} from "./permissionPattern";
import "../../styles/permission-rules.css";

export type AlwaysRuleDialogProps = {
  gateway: PermissionRulesGateway;
  /** Derived host match subject; used only as the initial pattern suggestion. */
  ruleSubject: string;
  prefix: PermissionRulePrefix;
  /** Initial action the user chose from the approval bar. */
  initialAction: PermissionRuleAction;
  /** Active project — enables project-scoped deny. */
  projectId: string | null;
  onClose: () => void;
  /** Called after a rule is successfully created. */
  onCreated?: () => void;
};

export function AlwaysRuleDialog({
  gateway,
  ruleSubject,
  prefix,
  initialAction,
  projectId,
  onClose,
  onCreated,
}: AlwaysRuleDialogProps) {
  const titleId = useId();
  const patternId = useId();
  const suggested = suggestPatternFromSubject(prefix, ruleSubject);

  const [action, setAction] = useState<PermissionRuleAction>(initialAction);
  const [scope, setScope] = useState<PermissionRuleScope>(() => {
    // Always-allow is global-only; deny defaults to project when available.
    if (initialAction === "allow") return "global";
    return projectId ? "project" : "global";
  });
  const [pattern, setPattern] = useState(suggested);
  const [submitting, setSubmitting] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const projectAllowBlocked = isProjectAllowForbidden(scope, action);
  const canSubmit =
    !submitting
    && pattern.trim().length > 0
    && !projectAllowBlocked
    && (scope !== "project" || Boolean(projectId));

  async function handleSubmit(event: FormEvent) {
    event.preventDefault();
    if (!canSubmit) return;
    setSubmitting(true);
    setError(null);
    try {
      await gateway.addPermissionRule(
        scope,
        scope === "project" ? projectId : null,
        prefix,
        pattern.trim(),
        action,
      );
      onCreated?.();
      onClose();
    } catch (cause) {
      const message =
        cause instanceof Error ? cause.message : "添加权限规则失败。";
      setError(message);
    } finally {
      setSubmitting(false);
    }
  }

  // When user switches to allow, force global scope (project+allow is illegal).
  function handleActionChange(next: PermissionRuleAction) {
    setAction(next);
    if (next === "allow" && scope === "project") {
      setScope("global");
    }
  }

  function handleScopeChange(next: PermissionRuleScope) {
    setScope(next);
    if (next === "project" && action === "allow") {
      // Prefer keeping deny if user insists on project; do not allow project+allow.
      setAction("deny");
    }
  }

  return (
    <div
      aria-labelledby={titleId}
      aria-modal="true"
      className="permission-rule-dialog-backdrop"
      data-testid="always-rule-dialog"
      role="dialog"
    >
      <form
        className="permission-rule-dialog"
        onSubmit={(event) => void handleSubmit(event)}
      >
        <header className="permission-rule-dialog__head">
          <h2 id={titleId}>
            {action === "allow" ? "总是允许此模式" : "总是拒绝此模式"}
          </h2>
          <button
            aria-label="关闭"
            className="permission-rule-dialog__close"
            onClick={onClose}
            type="button"
          >
            ×
          </button>
        </header>

        <p className="permission-rule-dialog__lead">
          将写入下列确切模式。请确认后再保存；保存后匹配的后续提案会按规则自动决定。
        </p>

        <label className="permission-rule-field" htmlFor={patternId}>
          <span className="permission-rule-field__label">确切模式</span>
          <input
            autoComplete="off"
            className="permission-rule-field__control"
            data-testid="always-rule-pattern"
            id={patternId}
            onChange={(event) => setPattern(event.target.value)}
            spellCheck={false}
            type="text"
            value={pattern}
          />
        </label>

        <p
          className="permission-rule-dialog__exact"
          data-testid="always-rule-exact-preview"
        >
          将写入：
          <code>
            {action} {prefix}:{pattern.trim() || "…"}
          </code>
        </p>

        <p
          className="permission-rule-word-boundary"
          data-testid="always-rule-word-boundary-note"
          role="note"
        >
          {PERMISSION_WORD_BOUNDARY_NOTE}
        </p>

        <div className="permission-rule-dialog__meta">
          <label className="permission-rule-field">
            <span className="permission-rule-field__label">前缀</span>
            <select
              aria-label="规则前缀"
              className="permission-rule-field__control"
              data-testid="always-rule-prefix"
              disabled
              value={prefix}
            >
              <option value={prefix}>{prefixLabel(prefix)}</option>
            </select>
          </label>

          <label className="permission-rule-field">
            <span className="permission-rule-field__label">动作</span>
            <select
              aria-label="规则动作"
              className="permission-rule-field__control"
              data-testid="always-rule-action"
              onChange={(event) =>
                handleActionChange(event.target.value as PermissionRuleAction)
              }
              value={action}
            >
              <option value="allow">{actionLabel("allow")}</option>
              <option value="deny">{actionLabel("deny")}</option>
            </select>
          </label>

          <label className="permission-rule-field">
            <span className="permission-rule-field__label">作用域</span>
            <select
              aria-label="规则作用域"
              className="permission-rule-field__control"
              data-testid="always-rule-scope"
              onChange={(event) =>
                handleScopeChange(event.target.value as PermissionRuleScope)
              }
              value={scope}
            >
              <option value="global">全局</option>
              <option value="project" disabled={!projectId}>
                当前项目{projectId ? "" : "（需打开项目）"}
              </option>
            </select>
          </label>
        </div>

        {projectAllowBlocked ? (
          <p
            className="permission-rule-project-allow-block"
            data-testid="always-rule-project-allow-blocked"
            role="status"
          >
            项目作用域只能拒绝，不能允许。允许规则必须使用全局作用域，防止恶意项目自我授权。
          </p>
        ) : null}

        {error ? (
          <p
            className="permission-rule-error"
            data-testid="always-rule-error"
            role="alert"
          >
            {error}
          </p>
        ) : null}

        <div className="permission-rule-dialog__actions">
          <button onClick={onClose} type="button">
            取消
          </button>
          <button
            className="permission-rule-dialog__submit"
            data-testid="always-rule-submit"
            disabled={!canSubmit}
            type="submit"
          >
            {submitting ? "保存中…" : "保存规则"}
          </button>
        </div>
      </form>
    </div>
  );
}
