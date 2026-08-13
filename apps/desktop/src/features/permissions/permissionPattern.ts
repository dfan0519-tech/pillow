/**
 * Pure helpers for permission-rule pattern suggestion and display.
 * No host calls; approval bars and Settings share this logic.
 */

import type {
  AutoApprovedByRule,
  PermissionRuleAction,
  PermissionRulePrefix,
} from "../../lib/permissionRulesGateway";
import {
  decodeProcessSubject,
  formatProcessSubjectForDisplay,
  spaceJoinIsBijective,
} from "./processSubject";

/**
 * Word-boundary warning shown whenever the user is about to write a rule.
 * Prefix match has no word boundary: `cargo` also matches `cargotest`.
 */
export const PERMISSION_WORD_BOUNDARY_NOTE =
  "模式按前缀/通配匹配，没有词边界：例如「cargo」会同时命中「cargotest」。"
  + "若只想匹配该程序及其参数，请使用「cargo *」这类带空格与通配符的形态。";

/**
 * A rule pattern as a human should read it.
 *
 * A process pattern may be an exact v1 subject, which carries a control
 * character and hex length prefixes. Display only — never write a stored
 * pattern back from this value.
 */
export function displayRulePattern(prefix: string, pattern: string): string {
  return prefix === "process"
    ? formatProcessSubjectForDisplay(pattern)
    : pattern;
}

/** Compact timeline / chip summary, e.g. `allow process:cargo *`. */
export function formatRuleSummary(rule: AutoApprovedByRule): string {
  const action = rule.action.trim() || "allow";
  const prefix = rule.prefix.trim() || "process";
  const pattern = displayRulePattern(prefix, rule.pattern).trim();
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
  if (ruleSubject.trim().length === 0) {
    return prefix === "process" ? "*" : "**/*";
  }

  // Trimming would corrupt a v1 subject whose final argument ends in
  // whitespace, so process subjects are read exactly as the host produced them.
  const subject = prefix === "process" ? ruleSubject : ruleSubject.trim();

  if (prefix === "process") {
    // Already a glob-like pattern — keep it.
    if (subject.includes("*")) return subject;

    // Host subjects are v1-encoded (see processSubject.ts). Splitting one on
    // whitespace would slice through the encoding and suggest a rule that can
    // never match, so decode instead of tokenizing the raw string.
    const tokens = decodeProcessSubject(subject);
    if (!tokens || tokens.length === 0) {
      // Legacy or malformed: the host will not auto-match this subject at all.
      // Show it verbatim rather than inventing a shape it does not have.
      return subject;
    }
    if (!spaceJoinIsBijective(tokens)) {
      // Argv with embedded spaces or empty elements has no faithful
      // space-joined pattern — that ambiguity is exactly what EA-02 exploited.
      // The exact canonical subject is the only pattern that means this argv.
      return subject;
    }
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

/**
 * Why the host could not read a stored row as a rule.
 *
 * Codes come from the host (`UnreadablePermissionRuleReason`); an unknown code
 * still renders something honest rather than blank.
 */
export function unreadableRuleReasonLabel(reason: string): string {
  switch (reason) {
    case "column":
      return "存储列缺失或类型不对";
    case "scope":
      return "作用域无法识别";
    case "prefix":
      return "前缀无法识别";
    case "action":
      return "动作无法识别";
    case "scope_shape":
      return "作用域与项目归属不一致";
    case "project_allow":
      return "项目作用域写着「允许」，产品从不这样写入";
    case "pattern":
      return "模式为空、只有空白或含控制字符（空模式会被内核当作匹配全部）";
    case "rule_id":
      return "规则 ID 不是合法 UUID";
    case "created_at":
      return "创建时间不是合法时间戳";
    case "content_digest":
      return "内容摘要与规则内容不一致（该行在产品之外被改写过）";
    default:
      return "无法读取";
  }
}

/**
 * Scope/prefix/action of an unreadable row, for recognition only.
 * Unreadable columns are dropped rather than guessed.
 */
export function unreadableRuleShapeLabel(row: {
  scope?: string;
  prefix?: string;
  action?: string;
}): string {
  const parts = [
    row.scope ? scopeLabel(row.scope) : null,
    row.prefix ?? null,
    row.action === "allow" || row.action === "deny"
      ? actionLabel(row.action)
      : row.action ?? null,
  ].filter((part): part is string => part !== null);
  return parts.length > 0 ? parts.join(" · ") : "字段不可读";
}
