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
  encodeProcessSubject,
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
 * Whether a process **allow** pattern has no argument constraint.
 *
 * Mirrors `keeper_policy::is_overbroad_process_allow_pattern`. Overbroad
 * means a later, semantically different argv of the same program (or a glued
 * program-name glob such as `node*`) would match. Deny is out of scope.
 */
export function isOverbroadProcessAllowPattern(pattern: string): boolean {
  if (pattern.length === 0) return true;
  if (pattern.includes(String.fromCharCode(0x1e))) {
    return decodeProcessSubject(pattern) === null;
  }
  const tokens = pattern.split(" ").filter((token) => token.length > 0);
  if (tokens.length === 0) return true;
  const program = tokens[0];
  if (program.length === 0 || program.includes("*")) return true;
  return !tokens.slice(1).some((token) => !isOnlyWildcards(token));
}

function isOnlyWildcards(token: string): boolean {
  return token.length > 0 && [...token].every((ch) => ch === "*");
}

/**
 * Suggest a durable pattern from the host `rule_subject`.
 *
 * Process **deny** (and callers that omit `action`) keep the historical
 * word-boundary-safe `program *` / `program arg *` shape. Process **allow**
 * keeps every concrete argv token so a one-click save cannot persist `node*`
 * / `node *` from `node build.js`.
 */
export function suggestPatternFromSubject(
  prefix: PermissionRulePrefix,
  ruleSubject: string,
  action?: PermissionRuleAction,
): string {
  if (ruleSubject.trim().length === 0) {
    return prefix === "process" ? "*" : "**/*";
  }

  // Trimming would corrupt a v1 subject whose final argument ends in
  // whitespace, so process subjects are read exactly as the host produced them.
  const subject = prefix === "process" ? ruleSubject : ruleSubject.trim();

  if (prefix === "process") {
    if (action === "allow") {
      return suggestProcessAllowPattern(subject);
    }
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

/**
 * Allow-side process suggestion. Mirrors
 * `keeper_policy::suggest_process_allow_pattern`.
 */
export function suggestProcessAllowPattern(subject: string): string {
  const tokens = decodeProcessSubject(subject);
  if (!tokens || tokens.length === 0 || !spaceJoinIsBijective(tokens)) {
    return subject;
  }
  return tokens.join(" ");
}

const DELEGATION_PROGRAMS = new Set([
  "xargs",
  "npx",
  "pnpx",
  "bunx",
  "uvx",
  "env",
]);

function programBasename(program: string): string {
  const base = program.split(/[/\\]/).pop() ?? program;
  return base.length > 4 && base.slice(-4).toLowerCase() === ".exe"
    ? base.slice(0, -4)
    : base;
}

function contrastProcessTokens(tokens: readonly string[]): string[] {
  const contrast = [...tokens];
  if (contrast.length === 1) {
    contrast.push(`${contrast[0]}.other`);
  } else if (contrast[1]!.startsWith("-")) {
    contrast.splice(1, 0, `${contrast[0]}.other`);
  } else {
    contrast[1] = `${contrast[1]}.other`;
  }
  return contrast;
}

function wildcardStarMatch(pattern: string, subject: string): boolean {
  const escaped = pattern
    .replace(/[.+?^${}()|[\]\\]/g, "\\$&")
    .replace(/\*/g, ".*");
  return new RegExp(`^${escaped}$`).test(subject);
}

/**
 * Host-faithful enough for contrast examples: prefix or `*` glob against a
 * decodable subject, then the same confirmation gate the host uses for allow.
 */
export function processAllowPatternWouldAutoAllow(
  pattern: string,
  subject: string,
): boolean {
  const tokens = decodeProcessSubject(subject);
  if (!tokens || tokens.length === 0) return false;
  if (!processSubjectLooksConfirmedForAllow(tokens)) return false;
  if (pattern.length === 0) return true;
  if (spaceJoinIsBijective(tokens)) {
    const flat = tokens.join(" ");
    return flat.startsWith(pattern) || wildcardStarMatch(pattern, flat);
  }
  return subject.startsWith(pattern) || wildcardStarMatch(pattern, subject);
}

function processSubjectLooksConfirmedForAllow(tokens: readonly string[]): boolean {
  const program = programBasename(tokens[0] ?? "");
  if (DELEGATION_PROGRAMS.has(program.toLowerCase())) return false;
  for (const token of tokens.slice(1)) {
    if (token === "-exec" || token === "-execdir" || token === "eval") {
      return false;
    }
    if (token.startsWith("--")) {
      const name = token.slice(2).split("=")[0] ?? "";
      if (
        name === "eval"
        || name === "print"
        || name === "command"
        || name === "execute"
        || name === "expression"
      ) {
        return false;
      }
    }
  }
  return true;
}

/**
 * Display form of a different command this overbroad allow would also
 * auto-approve, derived from `fromSubject`. Null when the pattern is not
 * overbroad or no real auto-allow contrast exists.
 */
export function contrastProcessCommandForOverbroadAllow(
  pattern: string,
  fromSubject: string,
): string | null {
  if (!isOverbroadProcessAllowPattern(pattern)) return null;
  const tokens = decodeProcessSubject(fromSubject);
  if (!tokens || tokens.length === 0) return null;
  if (DELEGATION_PROGRAMS.has(programBasename(tokens[0] ?? "").toLowerCase())) {
    return null;
  }
  const contrast = contrastProcessTokens(tokens);
  const subject = encodeProcessSubject(contrast[0]!, contrast.slice(1));
  if (subject === fromSubject) return null;
  if (!processAllowPatternWouldAutoAllow(pattern, subject)) return null;
  return formatProcessSubjectForDisplay(subject);
}

/** Concrete save-time warning, or null when the allow pattern is not overbroad. */
export function overbroadProcessAllowWarning(
  pattern: string,
  fromSubject: string,
): string | null {
  if (!isOverbroadProcessAllowPattern(pattern)) return null;
  const example = contrastProcessCommandForOverbroadAllow(pattern, fromSubject);
  if (example) {
    return (
      `这条允许规则没有参数约束。保存后还会自动放行「${example}」，`
      + "与当前审批的命令不是同一条。若只想允许本次命令，请写上具体参数。"
    );
  }
  return "这条允许规则没有参数约束，会自动放行该程序的其他命令。若只想允许本次命令，请写上具体参数。";
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
