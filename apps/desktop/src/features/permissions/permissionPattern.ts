/**
 * Pure helpers for permission-rule pattern suggestion and display.
 * No host calls; approval bars and Settings share this logic.
 */

import type {
  AutoApprovedByRule,
  PermissionRuleAction,
  PermissionRulePrefix,
} from "../../lib/permissionRulesGateway";

/**
 * Word-boundary warning shown whenever the user is about to write a rule.
 * Prefix match has no word boundary: `cargo` also matches `cargotest`.
 */
export const PERMISSION_WORD_BOUNDARY_NOTE =
  "模式按前缀/通配匹配，没有词边界：例如「cargo」会同时命中「cargotest」。"
  + "若只想匹配该程序及其参数，请使用「cargo *」这类带空格与通配符的形态。";

/** Compact timeline / chip summary, e.g. `allow process:cargo *`. */
export function formatRuleSummary(rule: AutoApprovedByRule): string {
  const action = rule.action.trim() || "allow";
  const prefix = rule.prefix.trim() || "process";
  const pattern = rule.pattern.trim();
  return pattern.length > 0
    ? `${action} ${prefix}:${pattern}`
    : `${action} ${prefix}`;
}

/**
 * Visible label when a rule auto-decided a proposal.
 * Auto-approve must never be silent in the timeline.
 */
export function autoRuleDecisionLabel(rule: AutoApprovedByRule): string {
  const isDeny = rule.action.trim() === "deny";
  return isDeny ? "已由规则拒绝" : "已由规则自动批准";
}

/**
 * Suggest a durable pattern from the host `rule_subject`.
 * Process subjects prefer a word-boundary-safe `program *` / `program arg *`
 * shape rather than a bare token that over-matches.
 */
export function suggestPatternFromSubject(
  prefix: PermissionRulePrefix,
  ruleSubject: string,
): string {
  const subject = ruleSubject.trim();
  if (!subject) {
    return prefix === "process" ? "*" : "**/*";
  }

  if (prefix === "process") {
    // Already a glob-like pattern — keep it.
    if (subject.includes("*")) return subject;
    const tokens = subject.split(/\s+/).filter(Boolean);
    if (tokens.length === 0) return "*";
    if (tokens.length === 1) {
      // Bare program → force space+wildcard so `cargo` does not hit `cargotest`.
      return `${tokens[0]} *`;
    }
    // program + first arg, then wildcard (e.g. `cargo test *`).
    return `${tokens[0]} ${tokens[1]} *`;
  }

  // write / edit: path subject. Keep globs; exact paths stay editable.
  return subject;
}

/** Infer write vs edit prefix from write proposal mode when host omits it. */
export function writePrefixFromMode(
  mode: string | undefined,
): PermissionRulePrefix {
  return mode === "create" ? "write" : "edit";
}

/** Human label for prefix select options. */
export function prefixLabel(prefix: PermissionRulePrefix): string {
  switch (prefix) {
    case "process":
      return "process（命令）";
    case "write":
      return "write（创建）";
    case "edit":
      return "edit（修改）";
  }
}

export function actionLabel(action: PermissionRuleAction): string {
  return action === "allow" ? "允许" : "拒绝";
}

export function scopeLabel(scope: string): string {
  return scope === "project" ? "项目" : "全局";
}
