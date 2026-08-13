/**
 * Permission-rules gateway — renderer boundary for host-owned rule CRUD.
 * Field names match the controller-fixed contract exactly (snake_case views).
 * Host evaluates rules; the renderer never decides allow/deny authority.
 */

import { invoke } from "@tauri-apps/api/core";

/** Match class for governed tools (controller contract). */
export type PermissionRulePrefix = "process" | "write" | "edit";

/** Rule effect. Project scope may only submit deny (UI + host). */
export type PermissionRuleAction = "allow" | "deny";

/** Storage / list scope. */
export type PermissionRuleScope = "global" | "project";

/**
 * Host-facing rule row. Field names are fixed by the batch contract —
 * do not rename to camelCase on this DTO.
 */
export type PermissionRuleView = {
  rule_id: string;
  scope: string;
  project_id?: string;
  prefix: string;
  pattern: string;
  action: string;
  created_at: string;
};

/**
 * A stored row the host could not read back as a rule.
 *
 * The host quarantines such rows: they are skipped by `listPermissionRules` and
 * never evaluated, so one bad row cannot disable the rule surface. They surface
 * here only so the user can delete them without a database editor.
 *
 * There is no `pattern` field on purpose — an unreadable row's pattern is either
 * not something this product wrote or carries a digest the host cannot vouch
 * for, so it is not re-displayed as a rule the user authored.
 */
export type UnreadablePermissionRuleView = {
  rule_id: string;
  scope?: string;
  project_id?: string;
  prefix?: string;
  action?: string;
  created_at?: string;
  /** Stable host reason code (`pattern`, `content_digest`, `rule_id`, …). */
  reason: string;
};

/**
 * Snapshot of the rule that auto-decided a proposal (receipt / timeline).
 * Present on decided proposals / durable events when the host auto-applied.
 */
export type AutoApprovedByRule = {
  prefix: string;
  pattern: string;
  action: string;
};

export type PermissionRulesGatewayErrorCode =
  | "permission_rules_invalid_response"
  | "permission_rules_command_failed"
  | "permission_rules_project_allow_forbidden"
  | "permission_rules_invalid_input"
  | "permission_rules_not_found";

export class PermissionRulesGatewayError extends Error {
  readonly code: PermissionRulesGatewayErrorCode | string;

  constructor(code: string, message: string) {
    super(message);
    this.name = "PermissionRulesGatewayError";
    this.code = code;
  }
}

export interface PermissionRulesGateway {
  listPermissionRules(
    scope: PermissionRuleScope,
    projectId?: string | null,
  ): Promise<PermissionRuleView[]>;
  addPermissionRule(
    scope: PermissionRuleScope,
    projectId: string | null | undefined,
    prefix: PermissionRulePrefix,
    pattern: string,
    action: PermissionRuleAction,
  ): Promise<PermissionRuleView>;
  deletePermissionRule(ruleId: string): Promise<void>;
  /** Rows the host skipped while loading rules, across every scope. */
  listUnreadablePermissionRules(): Promise<UnreadablePermissionRuleView[]>;
  /** Remove one quarantined row. The host refuses readable rules here. */
  deleteUnreadablePermissionRule(ruleId: string): Promise<void>;
}

const INVALID_RESPONSE =
  "Pillow received an invalid permission rules response.";
const MAX_LIST = 512;
const MAX_PATTERN = 512;
const MAX_ID = 128;
/** Host caps `match_subject` at 8192 chars (migration 0012 CHECK). */
const MAX_SUBJECT = 8192;

const PREFIXES: readonly PermissionRulePrefix[] = ["process", "write", "edit"];
const ACTIONS: readonly PermissionRuleAction[] = ["allow", "deny"];
const SCOPES: readonly PermissionRuleScope[] = ["global", "project"];

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === "object" && value !== null && !Array.isArray(value);
}

function isNonEmptyString(value: unknown): value is string {
  return typeof value === "string" && value.trim().length > 0;
}

function isPrefix(value: unknown): value is PermissionRulePrefix {
  return typeof value === "string"
    && (PREFIXES as readonly string[]).includes(value);
}

function isAction(value: unknown): value is PermissionRuleAction {
  return typeof value === "string"
    && (ACTIONS as readonly string[]).includes(value);
}

function isScope(value: unknown): value is PermissionRuleScope {
  return typeof value === "string"
    && (SCOPES as readonly string[]).includes(value);
}

/**
 * Parse one rule row. Accepts contract snake_case keys; tolerates camelCase
 * wire variants so mock fixtures and future host renames stay compatible.
 */
export function parsePermissionRuleView(value: unknown): PermissionRuleView {
  if (!isRecord(value)) {
    throw new PermissionRulesGatewayError(
      "permission_rules_invalid_response",
      INVALID_RESPONSE,
    );
  }
  const ruleId = value.rule_id ?? value.ruleId;
  const scope = value.scope;
  const projectId = value.project_id ?? value.projectId;
  const prefix = value.prefix;
  const pattern = value.pattern;
  const action = value.action;
  const createdAt = value.created_at ?? value.createdAt;

  if (
    !isNonEmptyString(ruleId)
    || ruleId.length > MAX_ID
    || typeof scope !== "string"
    || scope.trim().length === 0
    || typeof prefix !== "string"
    || prefix.trim().length === 0
    || typeof pattern !== "string"
    || pattern.length > MAX_PATTERN
    || typeof action !== "string"
    || action.trim().length === 0
    || !isNonEmptyString(createdAt)
  ) {
    throw new PermissionRulesGatewayError(
      "permission_rules_invalid_response",
      INVALID_RESPONSE,
    );
  }

  const view: PermissionRuleView = {
    rule_id: ruleId,
    scope: scope.trim(),
    prefix: prefix.trim(),
    pattern,
    action: action.trim(),
    created_at: createdAt,
  };
  if (typeof projectId === "string" && projectId.trim().length > 0) {
    view.project_id = projectId.trim();
  }
  return view;
}

export function parsePermissionRuleList(value: unknown): PermissionRuleView[] {
  if (!Array.isArray(value) || value.length > MAX_LIST) {
    throw new PermissionRulesGatewayError(
      "permission_rules_invalid_response",
      INVALID_RESPONSE,
    );
  }
  return value.map(parsePermissionRuleView);
}

/** Optional raw column text carried for recognition only. */
function optionalColumn(value: unknown): string | undefined {
  return typeof value === "string" && value.trim().length > 0
      && value.length <= MAX_PATTERN
    ? value.trim()
    : undefined;
}

/**
 * Parse one quarantined row.
 *
 * `rule_id` and `reason` are required — the id is what the repair command needs
 * and the reason is what the user is shown. Every other field is best-effort:
 * these rows are unreadable precisely because some column is not what the
 * product writes, so a strict parse here would re-create the failure it exists
 * to recover from.
 */
export function parseUnreadablePermissionRuleView(
  value: unknown,
): UnreadablePermissionRuleView {
  if (!isRecord(value)) {
    throw new PermissionRulesGatewayError(
      "permission_rules_invalid_response",
      INVALID_RESPONSE,
    );
  }
  const ruleId = value.rule_id ?? value.ruleId;
  const reason = value.reason;
  if (
    !isNonEmptyString(ruleId)
    || ruleId.length > MAX_ID
    || !isNonEmptyString(reason)
    || reason.length > MAX_ID
  ) {
    throw new PermissionRulesGatewayError(
      "permission_rules_invalid_response",
      INVALID_RESPONSE,
    );
  }

  const view: UnreadablePermissionRuleView = {
    rule_id: ruleId,
    reason: reason.trim(),
  };
  const scope = optionalColumn(value.scope);
  if (scope) view.scope = scope;
  const projectId = optionalColumn(value.project_id ?? value.projectId);
  if (projectId) view.project_id = projectId;
  const prefix = optionalColumn(value.prefix);
  if (prefix) view.prefix = prefix;
  const action = optionalColumn(value.action);
  if (action) view.action = action;
  const createdAt = optionalColumn(value.created_at ?? value.createdAt);
  if (createdAt) view.created_at = createdAt;
  return view;
}

export function parseUnreadablePermissionRuleList(
  value: unknown,
): UnreadablePermissionRuleView[] {
  if (!Array.isArray(value) || value.length > MAX_LIST) {
    throw new PermissionRulesGatewayError(
      "permission_rules_invalid_response",
      INVALID_RESPONSE,
    );
  }
  return value.map(parseUnreadablePermissionRuleView);
}

/**
 * Parse optional auto-approve receipt from a host DTO / event payload.
 * Contract field: `auto_approved_by_rule`.
 *
 * Accepts either:
 * - a full payload object that nests `auto_approved_by_rule`, or
 * - the rule snapshot object itself `{ prefix, pattern, action }`.
 */
export function parseAutoApprovedByRule(
  value: unknown,
): AutoApprovedByRule | undefined {
  if (!isRecord(value)) return undefined;

  let source: Record<string, unknown> | null = null;
  if (isRecord(value.auto_approved_by_rule)) {
    source = value.auto_approved_by_rule;
  } else if (isRecord(value.autoApprovedByRule)) {
    source = value.autoApprovedByRule;
  } else if (
    typeof value.prefix === "string"
    && typeof value.pattern === "string"
    && typeof value.action === "string"
  ) {
    source = value;
  }
  if (!source) return undefined;

  if (
    typeof source.prefix !== "string"
    || source.prefix.trim().length === 0
    || typeof source.pattern !== "string"
    || typeof source.action !== "string"
    || source.action.trim().length === 0
  ) {
    return undefined;
  }
  return {
    prefix: source.prefix.trim(),
    pattern: source.pattern,
    action: source.action.trim(),
  };
}

/** Read `rule_subject` from a pending proposal-shaped object. */
/**
 * Strict host-DTO checks for the two rule fields carried on governed proposal
 * views (`ruleSubject`, `autoApprovedByRule`).
 *
 * The write and process approval gateways validate host payloads against a
 * closed key allowlist and throw on anything unrecognised. That defence is
 * deliberate — it is what stops a host from smuggling grants or native paths
 * into the renderer — but it means a widened host DTO is a breaking change
 * unless these keys are admitted there too. They live here, next to the type
 * they validate, so the two gateways share one definition instead of growing
 * a third copy of the same invariant.
 */
export function isRuleSubjectValue(value: unknown): value is string {
  return typeof value === "string"
    && value.trim().length > 0
    && value.length <= MAX_SUBJECT;
}

export function isAutoApprovedByRuleValue(
  value: unknown,
  allowedPrefixes: readonly PermissionRulePrefix[],
): value is AutoApprovedByRule {
  return isRecord(value)
    && Object.keys(value).every((key) =>
      key === "prefix" || key === "pattern" || key === "action"
    )
    && isPrefix(value.prefix)
    && allowedPrefixes.includes(value.prefix)
    && isAction(value.action)
    && typeof value.pattern === "string"
    && value.pattern.length > 0
    && value.pattern.length <= MAX_PATTERN;
}

export function readRuleSubject(proposal: unknown): string | undefined {
  if (!isRecord(proposal)) return undefined;
  const raw = proposal.rule_subject ?? proposal.ruleSubject;
  if (typeof raw !== "string") return undefined;
  // Blank subjects are dropped, but a present one is returned byte-for-byte.
  // Process subjects are length-prefixed, so trimming an argument that ends in
  // whitespace would silently yield a subject that no longer decodes.
  return raw.trim().length > 0 ? raw : undefined;
}

/**
 * UI gate: project scope cannot submit allow. Host also rejects, but the
 * product must block the combination before submit.
 */
export function isProjectAllowForbidden(
  scope: PermissionRuleScope,
  action: PermissionRuleAction,
): boolean {
  return scope === "project" && action === "allow";
}

function normalizeGatewayError(error: unknown): PermissionRulesGatewayError {
  if (error instanceof PermissionRulesGatewayError) return error;
  if (
    isRecord(error)
    && isNonEmptyString(error.code)
    && typeof error.message === "string"
    && error.message.trim().length > 0
  ) {
    return new PermissionRulesGatewayError(error.code, error.message);
  }
  return new PermissionRulesGatewayError(
    "permission_rules_command_failed",
    "Permission rules command failed.",
  );
}

/** Tauri host gateway — storage and evaluation stay on the host. */
export class TauriPermissionRulesGateway implements PermissionRulesGateway {
  async listPermissionRules(
    scope: PermissionRuleScope,
    projectId?: string | null,
  ): Promise<PermissionRuleView[]> {
    try {
      const payload = await invoke<unknown>("list_permission_rules", {
        scope,
        projectId: projectId ?? null,
      });
      return parsePermissionRuleList(payload);
    } catch (error) {
      throw normalizeGatewayError(error);
    }
  }

  async addPermissionRule(
    scope: PermissionRuleScope,
    projectId: string | null | undefined,
    prefix: PermissionRulePrefix,
    pattern: string,
    action: PermissionRuleAction,
  ): Promise<PermissionRuleView> {
    if (isProjectAllowForbidden(scope, action)) {
      throw new PermissionRulesGatewayError(
        "permission_rules_project_allow_forbidden",
        "Project scope may only deny; allow rules must be global.",
      );
    }
    const trimmed = pattern.trim();
    if (!trimmed || trimmed.length > MAX_PATTERN) {
      throw new PermissionRulesGatewayError(
        "permission_rules_invalid_input",
        "Pattern must be a non-empty string within length limits.",
      );
    }
    if (!isPrefix(prefix) || !isAction(action) || !isScope(scope)) {
      throw new PermissionRulesGatewayError(
        "permission_rules_invalid_input",
        "Invalid permission rule fields.",
      );
    }
    try {
      const payload = await invoke<unknown>("add_permission_rule", {
        scope,
        projectId: projectId ?? null,
        prefix,
        pattern: trimmed,
        action,
      });
      return parsePermissionRuleView(payload);
    } catch (error) {
      throw normalizeGatewayError(error);
    }
  }

  async deletePermissionRule(ruleId: string): Promise<void> {
    if (!isNonEmptyString(ruleId) || ruleId.length > MAX_ID) {
      throw new PermissionRulesGatewayError(
        "permission_rules_invalid_input",
        "Rule id is required.",
      );
    }
    try {
      await invoke<unknown>("delete_permission_rule", { ruleId });
    } catch (error) {
      throw normalizeGatewayError(error);
    }
  }

  async listUnreadablePermissionRules(): Promise<
    UnreadablePermissionRuleView[]
  > {
    try {
      const payload = await invoke<unknown>("list_unreadable_permission_rules");
      return parseUnreadablePermissionRuleList(payload);
    } catch (error) {
      throw normalizeGatewayError(error);
    }
  }

  async deleteUnreadablePermissionRule(ruleId: string): Promise<void> {
    if (!isNonEmptyString(ruleId) || ruleId.length > MAX_ID) {
      throw new PermissionRulesGatewayError(
        "permission_rules_invalid_input",
        "Rule id is required.",
      );
    }
    try {
      await invoke<unknown>("delete_unreadable_permission_rule", { ruleId });
    } catch (error) {
      throw normalizeGatewayError(error);
    }
  }
}

/** In-memory gateway for unit tests (no Tauri). */
export class MockPermissionRulesGateway implements PermissionRulesGateway {
  private rules: PermissionRuleView[] = [];
  private unreadable: UnreadablePermissionRuleView[] = [];
  private seq = 0;

  constructor(
    initial: readonly PermissionRuleView[] = [],
    initialUnreadable: readonly UnreadablePermissionRuleView[] = [],
  ) {
    this.rules = initial.map((rule) => structuredClone(rule));
    this.unreadable = initialUnreadable.map((row) => structuredClone(row));
  }

  /** Test seam: replace store contents. */
  setRules(rules: readonly PermissionRuleView[]): void {
    this.rules = rules.map((rule) => structuredClone(rule));
  }

  /** Test seam: replace the quarantined rows the host would report. */
  setUnreadableRules(rows: readonly UnreadablePermissionRuleView[]): void {
    this.unreadable = rows.map((row) => structuredClone(row));
  }

  getSnapshot(): PermissionRuleView[] {
    return structuredClone(this.rules);
  }

  getUnreadableSnapshot(): UnreadablePermissionRuleView[] {
    return structuredClone(this.unreadable);
  }

  async listPermissionRules(
    scope: PermissionRuleScope,
    projectId?: string | null,
  ): Promise<PermissionRuleView[]> {
    return structuredClone(
      this.rules.filter((rule) => {
        if (rule.scope !== scope) return false;
        if (scope === "global") return true;
        if (!projectId) return false;
        return rule.project_id === projectId;
      }),
    );
  }

  async addPermissionRule(
    scope: PermissionRuleScope,
    projectId: string | null | undefined,
    prefix: PermissionRulePrefix,
    pattern: string,
    action: PermissionRuleAction,
  ): Promise<PermissionRuleView> {
    if (isProjectAllowForbidden(scope, action)) {
      throw new PermissionRulesGatewayError(
        "permission_rules_project_allow_forbidden",
        "Project scope may only deny; allow rules must be global.",
      );
    }
    const trimmed = pattern.trim();
    if (!trimmed) {
      throw new PermissionRulesGatewayError(
        "permission_rules_invalid_input",
        "Pattern is required.",
      );
    }
    if (scope === "project" && !projectId) {
      throw new PermissionRulesGatewayError(
        "permission_rules_invalid_input",
        "Project scope requires a project id.",
      );
    }
    this.seq += 1;
    const view: PermissionRuleView = {
      rule_id: `rule-mock-${this.seq}`,
      scope,
      prefix,
      pattern: trimmed,
      action,
      created_at: new Date().toISOString(),
    };
    if (scope === "project" && projectId) {
      view.project_id = projectId;
    }
    this.rules = [...this.rules, view];
    return structuredClone(view);
  }

  async deletePermissionRule(ruleId: string): Promise<void> {
    const next = this.rules.filter((rule) => rule.rule_id !== ruleId);
    if (next.length === this.rules.length) {
      throw new PermissionRulesGatewayError(
        "permission_rules_invalid_input",
        "Rule not found.",
      );
    }
    this.rules = next;
  }

  async listUnreadablePermissionRules(): Promise<
    UnreadablePermissionRuleView[]
  > {
    return structuredClone(this.unreadable);
  }

  async deleteUnreadablePermissionRule(ruleId: string): Promise<void> {
    const next = this.unreadable.filter((row) => row.rule_id !== ruleId);
    if (next.length === this.unreadable.length) {
      // Mirrors the host: readable rules are not reachable from this path.
      throw new PermissionRulesGatewayError(
        "permission_rules_not_found",
        "No unreadable permission rule carries that id.",
      );
    }
    this.unreadable = next;
  }
}
