/**
 * Settings surface for host-owned permission rules.
 * Global allow/deny + project deny-only. Project+allow is blocked in the UI.
 */

import { useCallback, useEffect, useState, type FormEvent } from "react";
import type {
  PermissionRuleAction,
  PermissionRulePrefix,
  PermissionRuleScope,
  PermissionRuleView,
  PermissionRulesGateway,
  UnreadablePermissionRuleView,
} from "../../lib/permissionRulesGateway";
import { isProjectAllowForbidden } from "../../lib/permissionRulesGateway";
import {
  PERMISSION_WORD_BOUNDARY_NOTE,
  actionLabel,
  displayRulePattern,
  prefixLabel,
  scopeLabel,
  unreadableRuleReasonLabel,
  unreadableRuleShapeLabel,
} from "./permissionPattern";
import "../../styles/permission-rules.css";

export type PermissionRulesSectionProps = {
  gateway: PermissionRulesGateway;
  /** Active project id; enables project-scoped deny rules. */
  projectId: string | null;
};

const PREFIX_OPTIONS: PermissionRulePrefix[] = ["process", "write", "edit"];

export function PermissionRulesSection({
  gateway,
  projectId,
}: PermissionRulesSectionProps) {
  const [globalRules, setGlobalRules] = useState<PermissionRuleView[]>([]);
  const [projectRules, setProjectRules] = useState<PermissionRuleView[]>([]);
  const [unreadableRules, setUnreadableRules] = useState<
    UnreadablePermissionRuleView[]
  >([]);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [notice, setNotice] = useState<string | null>(null);

  const [formScope, setFormScope] = useState<PermissionRuleScope>("global");
  const [formPrefix, setFormPrefix] = useState<PermissionRulePrefix>("process");
  const [formPattern, setFormPattern] = useState("");
  const [formAction, setFormAction] = useState<PermissionRuleAction>("allow");
  const [saving, setSaving] = useState(false);

  const projectAllowBlocked = isProjectAllowForbidden(formScope, formAction);

  const reload = useCallback(async () => {
    setLoading(true);
    setError(null);
    try {
      const globalList = await gateway.listPermissionRules("global", null);
      setGlobalRules(globalList);
      if (projectId) {
        const projectList = await gateway.listPermissionRules(
          "project",
          projectId,
        );
        setProjectRules(projectList);
      } else {
        setProjectRules([]);
      }
    } catch (cause) {
      const message =
        cause instanceof Error ? cause.message : "加载权限规则失败。";
      setError(message);
    }
    // Quarantined rows load separately: this list is the recovery path, so a
    // failure here must report itself without hiding the rules that did load.
    try {
      setUnreadableRules(await gateway.listUnreadablePermissionRules());
    } catch (cause) {
      const message =
        cause instanceof Error ? cause.message : "加载无法读取的规则失败。";
      setError(message);
    } finally {
      setLoading(false);
    }
  }, [gateway, projectId]);

  useEffect(() => {
    void reload();
  }, [reload]);

  // If project closes while form is on project scope, fall back to global.
  useEffect(() => {
    if (!projectId && formScope === "project") {
      setFormScope("global");
    }
  }, [formScope, projectId]);

  // Switching to project while action is allow → force deny in the form.
  useEffect(() => {
    if (formScope === "project" && formAction === "allow") {
      setFormAction("deny");
    }
  }, [formAction, formScope]);

  async function handleAdd(event: FormEvent) {
    event.preventDefault();
    if (projectAllowBlocked) return;
    const pattern = formPattern.trim();
    if (!pattern) {
      setError("模式不能为空。");
      return;
    }
    if (formScope === "project" && !projectId) {
      setError("打开项目后才能添加项目规则。");
      return;
    }
    setSaving(true);
    setError(null);
    setNotice(null);
    try {
      await gateway.addPermissionRule(
        formScope,
        formScope === "project" ? projectId : null,
        formPrefix,
        pattern,
        formAction,
      );
      setFormPattern("");
      setNotice("已添加权限规则。");
      await reload();
    } catch (cause) {
      const message =
        cause instanceof Error ? cause.message : "添加权限规则失败。";
      setError(message);
    } finally {
      setSaving(false);
    }
  }

  async function handleDelete(ruleId: string) {
    setError(null);
    setNotice(null);
    try {
      await gateway.deletePermissionRule(ruleId);
      setNotice("已删除权限规则。");
      await reload();
    } catch (cause) {
      const message =
        cause instanceof Error ? cause.message : "删除权限规则失败。";
      setError(message);
    }
  }

  async function handleDeleteUnreadable(ruleId: string) {
    setError(null);
    setNotice(null);
    try {
      await gateway.deleteUnreadablePermissionRule(ruleId);
      setNotice("已删除无法读取的规则行。");
      await reload();
    } catch (cause) {
      const message =
        cause instanceof Error ? cause.message : "删除无法读取的规则行失败。";
      setError(message);
    }
  }

  function handleScopeChange(next: PermissionRuleScope) {
    setFormScope(next);
    if (next === "project") {
      setFormAction("deny");
    }
  }

  return (
    <section
      aria-labelledby="permission-rules-heading"
      className="permission-rules"
      data-testid="permission-rules-section"
    >
      <header className="permission-rules__head">
        <h3 id="permission-rules-heading">权限规则</h3>
        <p>
          规则决定是否自动批准或拒绝写入/命令提案；每次执行仍有审计收据。
          允许规则只能写在全局；项目只能额外拒绝。
        </p>
      </header>

      {loading ? (
        <p className="permission-rules__status">正在加载权限规则…</p>
      ) : (
        <>
          <UnreadableRuleList
            onDelete={(id) => void handleDeleteUnreadable(id)}
            rules={unreadableRules}
          />
          <RuleList
            emptyLabel="尚无全局规则。"
            onDelete={(id) => void handleDelete(id)}
            rules={globalRules}
            testId="permission-rules-global-list"
            title="全局规则"
          />
          <RuleList
            emptyLabel={
              projectId
                ? "尚无项目拒绝规则。"
                : "打开项目后可查看与添加项目拒绝规则。"
            }
            onDelete={(id) => void handleDelete(id)}
            rules={projectRules}
            testId="permission-rules-project-list"
            title="项目拒绝规则"
          />
        </>
      )}

      <form
        className="permission-rules-form"
        data-testid="permission-rules-add-form"
        onSubmit={(event) => void handleAdd(event)}
      >
        <h4 className="permission-rules-form__title">添加规则</h4>

        <div className="permission-rules-form__row">
          <label className="permission-rule-field">
            <span className="permission-rule-field__label">作用域</span>
            <select
              aria-label="权限规则作用域"
              className="permission-rule-field__control"
              data-testid="permission-rules-form-scope"
              onChange={(event) =>
                handleScopeChange(event.target.value as PermissionRuleScope)
              }
              value={formScope}
            >
              <option value="global">全局</option>
              <option disabled={!projectId} value="project">
                当前项目
              </option>
            </select>
          </label>

          <label className="permission-rule-field">
            <span className="permission-rule-field__label">前缀</span>
            <select
              aria-label="权限规则前缀"
              className="permission-rule-field__control"
              data-testid="permission-rules-form-prefix"
              onChange={(event) =>
                setFormPrefix(event.target.value as PermissionRulePrefix)
              }
              value={formPrefix}
            >
              {PREFIX_OPTIONS.map((prefix) => (
                <option key={prefix} value={prefix}>
                  {prefixLabel(prefix)}
                </option>
              ))}
            </select>
          </label>

          <label className="permission-rule-field">
            <span className="permission-rule-field__label">动作</span>
            <select
              aria-label="权限规则动作"
              className="permission-rule-field__control"
              data-testid="permission-rules-form-action"
              disabled={formScope === "project"}
              onChange={(event) =>
                setFormAction(event.target.value as PermissionRuleAction)
              }
              title={
                formScope === "project"
                  ? "项目作用域只能拒绝，不能允许"
                  : undefined
              }
              value={formAction}
            >
              <option
                disabled={formScope === "project"}
                value="allow"
              >
                允许
              </option>
              <option value="deny">拒绝</option>
            </select>
          </label>
        </div>

        <label className="permission-rule-field">
          <span className="permission-rule-field__label">模式</span>
          <input
            aria-label="权限规则模式"
            autoComplete="off"
            className="permission-rule-field__control"
            data-testid="permission-rules-form-pattern"
            onChange={(event) => setFormPattern(event.target.value)}
            placeholder="例如 cargo * 或 src/**/*.rs"
            spellCheck={false}
            type="text"
            value={formPattern}
          />
        </label>

        <p
          className="permission-rule-word-boundary"
          data-testid="permission-rules-word-boundary-note"
          role="note"
        >
          {PERMISSION_WORD_BOUNDARY_NOTE}
        </p>

        {formScope === "project" ? (
          <p
            className="permission-rule-project-allow-block"
            data-testid="permission-rules-project-allow-blocked"
            role="status"
          >
            项目作用域只能拒绝，不能允许。允许规则必须使用全局作用域，防止恶意项目自我授权。
            允许选项已禁用；提交前不会发送 project+allow。
          </p>
        ) : null}

        {error ? (
          <p
            className="permission-rule-error"
            data-testid="permission-rules-error"
            role="alert"
          >
            {error}
          </p>
        ) : null}
        {notice ? (
          <p
            className="permission-rules__notice"
            data-testid="permission-rules-notice"
          >
            {notice}
          </p>
        ) : null}

        <div className="permission-rules-form__actions">
          <button
            data-testid="permission-rules-form-submit"
            disabled={
              saving
              || loading
              || projectAllowBlocked
              || formPattern.trim().length === 0
              || (formScope === "project" && !projectId)
            }
            type="submit"
          >
            {saving ? "添加中…" : "添加规则"}
          </button>
        </div>
      </form>
    </section>
  );
}

/**
 * Rows the host could not read as rules.
 *
 * Renders nothing when the store is clean, so the ordinary Settings page is
 * unchanged. When it does render, it is the only in-product way to remove such
 * a row — the alternative used to be editing SQLite by hand.
 */
function UnreadableRuleList({
  rules,
  onDelete,
}: {
  rules: UnreadablePermissionRuleView[];
  onDelete: (ruleId: string) => void;
}) {
  if (rules.length === 0) return null;

  return (
    <div
      className="permission-rules-list-block permission-rules-list-block--unreadable"
      data-testid="permission-rules-unreadable-block"
    >
      <h4 className="permission-rules-list-block__title">无法读取的规则</h4>
      <p
        className="permission-rules__status"
        data-testid="permission-rules-unreadable-note"
        role="alert"
      >
        以下 {rules.length} 行无法作为规则读取，已被忽略：它们不参与任何自动批准或拒绝。
        产品不会写出这样的行，因此它们通常来自绕过产品的写入或改写。删除后其余规则不受影响。
      </p>
      <ul
        className="permission-rules-list"
        data-testid="permission-rules-unreadable-list"
      >
        {rules.map((rule) => (
          <li
            className="permission-rules-list__item"
            data-testid="permission-rule-unreadable-row"
            key={rule.rule_id}
          >
            <div className="permission-rules-list__body">
              <span
                className="permission-rules-list__badge permission-rules-list__badge--deny"
                data-testid="permission-rule-unreadable-state"
              >
                已忽略
              </span>
              <span data-testid="permission-rule-unreadable-shape">
                {unreadableRuleShapeLabel(rule)}
              </span>
              <span
                className="permission-rules-list__scope"
                data-testid="permission-rule-unreadable-reason"
              >
                {unreadableRuleReasonLabel(rule.reason)}
              </span>
            </div>
            <button
              aria-label={`删除无法读取的规则 ${rule.rule_id}`}
              className="permission-rules-list__delete"
              data-testid="permission-rule-unreadable-delete"
              onClick={() => onDelete(rule.rule_id)}
              type="button"
            >
              删除
            </button>
          </li>
        ))}
      </ul>
    </div>
  );
}

function RuleList({
  title,
  rules,
  emptyLabel,
  onDelete,
  testId,
}: {
  title: string;
  rules: PermissionRuleView[];
  emptyLabel: string;
  onDelete: (ruleId: string) => void;
  testId: string;
}) {
  return (
    <div className="permission-rules-list-block">
      <h4 className="permission-rules-list-block__title">{title}</h4>
      {rules.length === 0 ? (
        <p className="permission-rules__status" data-testid={`${testId}-empty`}>
          {emptyLabel}
        </p>
      ) : (
        <ul className="permission-rules-list" data-testid={testId}>
          {rules.map((rule) => (
            <li
              className="permission-rules-list__item"
              data-testid="permission-rule-row"
              key={rule.rule_id}
            >
              <div className="permission-rules-list__body">
                <span
                  className={
                    rule.action === "deny"
                      ? "permission-rules-list__badge permission-rules-list__badge--deny"
                      : "permission-rules-list__badge permission-rules-list__badge--allow"
                  }
                  data-testid="permission-rule-action"
                >
                  {actionLabel(
                    rule.action === "deny" ? "deny" : "allow",
                  )}
                </span>
                <code data-testid="permission-rule-pattern">
                  {rule.prefix}:{displayRulePattern(rule.prefix, rule.pattern)}
                </code>
                <span
                  className="permission-rules-list__scope"
                  data-testid="permission-rule-scope"
                >
                  {scopeLabel(rule.scope)}
                </span>
              </div>
              <button
                aria-label={`删除规则 ${rule.prefix}:${
                  displayRulePattern(rule.prefix, rule.pattern)
                }`}
                className="permission-rules-list__delete"
                data-testid="permission-rule-delete"
                onClick={() => onDelete(rule.rule_id)}
                type="button"
              >
                删除
              </button>
            </li>
          ))}
        </ul>
      )}
    </div>
  );
}
