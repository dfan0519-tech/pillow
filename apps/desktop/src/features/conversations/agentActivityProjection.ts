/**
 * Pure projection of conversation-owned agent activity into UI-safe view models.
 * Filters by conversationId; never fabricates tools/sub-agents; strips raw IDs and host paths.
 */

import type {
  AgentActivityNoticeView,
  AgentActivityProjection,
  AgentActivityRecord,
  AgentActivityStepView,
  AgentArtifactView,
  AgentSubagentStepView,
  AgentTestResultView,
  AgentToolStepView,
  AgentWriteOutcomeView,
} from "./agentActivityTypes";

const OPAQUE_ERROR_MESSAGE = "无法完成该操作。";
const TOOL_OUTPUT_AVAILABLE = "工具已返回输出。";

const STATUS_LABEL: Record<string, string> = {
  pending: "等待中",
  running: "运行中",
  cancelling: "取消中",
  completed: "已完成",
  failed: "失败",
  cancelled: "已取消",
  interrupted: "已中断",
};

const STEP_STATUS_LABEL: Record<string, string> = {
  pending: "待办",
  active: "进行中",
  completed: "完成",
  failed: "失败",
  cancelled: "已取消",
};

const ITEM_STATUS_LABEL: Record<string, string> = {
  running: "运行中",
  completed: "已完成",
  failed: "失败",
  cancelled: "已取消",
};

const TEST_STATUS_LABEL: Record<string, string> = {
  passed: "通过",
  failed: "失败",
  skipped: "跳过",
};

/**
 * Project activity records belonging to `conversationId` only.
 * Missing / foreign / empty inputs yield an empty list (no invented work).
 */
export function projectAgentActivity(
  conversationId: string,
  activity:
    | AgentActivityRecord
    | readonly AgentActivityRecord[]
    | null
    | undefined,
): AgentActivityProjection[] {
  const records = normalizeRecords(activity).filter(
    (record) => record.conversationId === conversationId,
  );
  return records.map((record, index) => projectRecord(record, index));
}

function normalizeRecords(
  activity:
    | AgentActivityRecord
    | readonly AgentActivityRecord[]
    | null
    | undefined,
): AgentActivityRecord[] {
  if (activity == null) return [];
  if (Array.isArray(activity)) return [...activity];
  return [activity as AgentActivityRecord];
}

function projectRecord(
  record: AgentActivityRecord,
  index: number,
): AgentActivityProjection {
  const forbiddenIds = collectForbiddenIds(record);
  const status = normalizeToken(record.status) || "pending";
  const statusLabel = STATUS_LABEL[status] ?? humanizeToken(status);
  const roleLabel = sanitizeDisplayText(record.role, forbiddenIds);
  const currentStep = sanitizeDisplayText(record.currentStep, forbiddenIds);
  const isTerminal =
    status === "completed"
    || status === "failed"
    || status === "cancelled"
    || status === "interrupted";
  const isFailureLike =
    status === "failed" || status === "cancelled" || status === "interrupted";
  const conciseSuccess = status === "completed";

  const steps = projectSteps(record.steps, forbiddenIds);
  const tools = projectTools(record.tools, forbiddenIds);
  const subagents = projectSubagents(record.subagents, forbiddenIds);
  const artifacts = projectArtifacts(record.artifacts, forbiddenIds);
  const testResults = projectTestResults(record.testResults, forbiddenIds);
  const writeOutcomes = projectWriteOutcomes(record.writeOutcomes, forbiddenIds);
  // Process lifecycle is rendered as conversation-owned command activity rows
  // (processActivityModel), not as generic notice paragraphs.
  const notices = projectNotices(record.notices, forbiddenIds);
  const errorMessage = opaqueErrorMessage(
    record.errorMessage ?? firstToolError(record),
  );

  // recordKey is React-only and must stay stable across status/step updates and
  // array reorders. Prefer explicit recordIdentity, then run/session identity.
  // Append list index only as a legacy fallback when no stable identity exists.
  // Never include status, summary, or live labels; never write into DOM.
  const recordKey = buildRecordKey(record, index);

  return {
    conversationId: record.conversationId,
    recordKey,
    summaryLine: buildSummaryLine({ roleLabel, statusLabel, currentStep }),
    status,
    statusLabel,
    isTerminal,
    isFailureLike,
    roleLabel,
    currentStep,
    steps,
    tools,
    subagents,
    artifacts,
    testResults,
    writeOutcomes,
    notices,
    errorMessage,
    conciseSuccess,
  };
}

/**
 * Prefer a durable identity that survives reorder. Index is last-resort only.
 */
function buildRecordKey(record: AgentActivityRecord, index: number): string {
  const preferred = firstNonEmpty(
    record.recordIdentity,
    record.runId,
    record.sessionId,
  );
  if (preferred) return preferred;
  // Legacy records with no stable identity: index keeps sibling keys unique.
  return `legacy-record::${index}`;
}

function firstNonEmpty(
  ...values: Array<string | undefined | null>
): string | null {
  for (const value of values) {
    const trimmed = value?.trim();
    if (trimmed && trimmed.length > 0) return trimmed;
  }
  return null;
}

function buildSummaryLine(parts: {
  roleLabel: string | null;
  statusLabel: string;
  currentStep: string | null;
}): string {
  const segments: string[] = [];
  if (parts.roleLabel) segments.push(parts.roleLabel);
  segments.push(parts.statusLabel);
  if (parts.currentStep) segments.push(parts.currentStep);
  return segments.join(" · ");
}

function projectSteps(
  steps: AgentActivityRecord["steps"],
  forbiddenIds: readonly string[],
): AgentActivityStepView[] {
  if (!steps || steps.length === 0) return [];
  return steps.map((step, index) => {
    const status = normalizeToken(step.status) || "pending";
    return {
      key: stableKey(step.id, `step-${index}`),
      title: sanitizeDisplayText(step.title, forbiddenIds) ?? "步骤",
      status,
      statusLabel: STEP_STATUS_LABEL[status] ?? humanizeToken(status),
    };
  });
}

function projectTools(
  tools: AgentActivityRecord["tools"],
  forbiddenIds: readonly string[],
): AgentToolStepView[] {
  if (!tools || tools.length === 0) return [];
  return tools.map((tool, index) => {
    const status = normalizeToken(tool.status) || "running";
    const title =
      sanitizeDisplayText(tool.title, forbiddenIds)
      ?? sanitizeDisplayText(tool.name, forbiddenIds)
      ?? "工具";
    const isFailureLike = status === "failed" || status === "cancelled";
    const rawOutput = tool.output?.trim() ? tool.output : undefined;
    const hasError = Boolean(tool.error?.trim());
    // Raw provider output never enters the view model. Successful tools get a
    // fixed local presence state; failed/cancelled tools remain opaque.
    const output = isFailureLike
      ? (rawOutput || hasError ? OPAQUE_ERROR_MESSAGE : undefined)
      : rawOutput
        ? TOOL_OUTPUT_AVAILABLE
        : hasError
          ? OPAQUE_ERROR_MESSAGE
          : undefined;
    const truncationNotice = toolOutputTruncationNotice(tool);
    return {
      key: stableKey(tool.toolCallId, `tool-${index}`),
      title,
      status,
      statusLabel: ITEM_STATUS_LABEL[status] ?? humanizeToken(status),
      output,
      truncationNotice: truncationNotice ?? undefined,
      hasNestedDetail: Boolean(output) || Boolean(truncationNotice),
    };
  });
}

function projectSubagents(
  subagents: AgentActivityRecord["subagents"],
  forbiddenIds: readonly string[],
): AgentSubagentStepView[] {
  if (!subagents || subagents.length === 0) return [];
  return subagents.map((agent, index) => {
    const status = normalizeToken(agent.status) || "running";
    const title =
      sanitizeDisplayText(agent.title, forbiddenIds)
      ?? sanitizeDisplayText(agent.role, forbiddenIds)
      ?? "子代理";
    const progress =
      sanitizeDisplayText(agent.progress, forbiddenIds) ?? undefined;
    return {
      key: stableKey(agent.subagentId, `subagent-${index}`),
      title,
      status,
      statusLabel: ITEM_STATUS_LABEL[status] ?? humanizeToken(status),
      progress,
      hasNestedDetail: Boolean(progress),
    };
  });
}

/** Neutral chrome when every candidate label echoes a forbidden ID. */
const NEUTRAL_ARTIFACT_LABEL = "产物";

function projectArtifacts(
  artifacts: AgentActivityRecord["artifacts"],
  forbiddenIds: readonly string[],
): AgentArtifactView[] {
  if (!artifacts || artifacts.length === 0) return [];
  const views: AgentArtifactView[] = [];
  for (let index = 0; index < artifacts.length; index += 1) {
    const artifact = artifacts[index]!;
    if (!isPortableRelativePath(artifact.path)) continue;
    // Callback path stays the real portable path; only the display label is sanitized.
    const relativePath = artifact.path.trim();
    const pathLeaf = resourceLeafName(relativePath) ?? relativePath;
    const safeLabel = sanitizeDisplayText(artifact.label, forbiddenIds);
    const safePathLeaf = sanitizeDisplayText(pathLeaf, forbiddenIds);
    // If the path leaf itself echoes a forbidden ID, use neutral chrome.
    const label = safeLabel ?? safePathLeaf ?? NEUTRAL_ARTIFACT_LABEL;
    const view: AgentArtifactView = {
      key: stableKey(artifact.artifactId, `artifact-${index}`),
      label,
      relativePath,
    };
    const token = artifact.sourceToken?.trim();
    if (token) {
      // sourceToken stays in the view model only — never DOM text/attrs.
      view.sourceToken = token;
    }
    views.push(view);
  }
  return views;
}

function projectTestResults(
  results: AgentActivityRecord["testResults"],
  forbiddenIds: readonly string[],
): AgentTestResultView[] {
  if (!results || results.length === 0) return [];
  return results.map((result, index) => {
    const status = normalizeToken(result.status) || "passed";
    return {
      key: `test-${index}`,
      name: sanitizeDisplayText(result.name, forbiddenIds) ?? "测试",
      status,
      statusLabel: TEST_STATUS_LABEL[status] ?? humanizeToken(status),
      summary: sanitizeDisplayText(result.summary, forbiddenIds) ?? undefined,
    };
  });
}

function projectNotices(
  notices: AgentActivityRecord["notices"],
  forbiddenIds: readonly string[],
): AgentActivityNoticeView[] {
  if (!notices || notices.length === 0) return [];
  const views: AgentActivityNoticeView[] = [];
  for (let index = 0; index < notices.length; index += 1) {
    const notice = notices[index]!;
    const text = sanitizeDisplayText(notice.text, forbiddenIds);
    if (!text) continue;
    views.push({
      key: stableKey(notice.id, `notice-${index}`),
      text,
    });
  }
  return views;
}

const WRITE_TERMINAL_MESSAGE: Record<string, string> = {
  succeeded: "文件写入已完成",
  rejected: "写入请求已拒绝",
  expired: "写入请求已过期",
  cancelled: "写入请求已取消",
  interrupted: "写入请求已中断",
  failed: "写入未能完成",
  approved: "正在应用写入…",
  applying: "正在应用写入…",
  pending_approval: "等待批准写入",
};

function projectWriteOutcomes(
  outcomes: AgentActivityRecord["writeOutcomes"],
  forbiddenIds: readonly string[],
): AgentWriteOutcomeView[] {
  if (!outcomes || outcomes.length === 0) return [];
  const views: AgentWriteOutcomeView[] = [];
  for (let index = 0; index < outcomes.length; index += 1) {
    const outcome = outcomes[index]!;
    const state = normalizeToken(outcome.state);
    if (!state) continue;
    // Never surface proposal ids or host paths in the terminal message.
    const message =
      WRITE_TERMINAL_MESSAGE[state]
      ?? "写入状态已更新";
    const portable =
      isPortableRelativePath(outcome.path.trim())
      && !forbiddenIds.includes(outcome.path.trim())
        ? outcome.path.trim()
        : null;
    const canOpenFile = state === "succeeded" && Boolean(portable);
    views.push({
      key: stableKey(outcome.proposalId, `write-outcome-${index}`),
      message,
      state,
      canOpenFile,
      ...(canOpenFile && portable ? { relativePath: portable } : {}),
    });
  }
  return views;
}

function toolOutputTruncationNotice(
  tool: NonNullable<AgentActivityRecord["tools"]>[number],
): string | null {
  if (!tool.outputTruncated) return null;
  const omitted = tool.omittedOutputChars ?? 0;
  return `输出已截断，省略 ${omitted} 个字符`;
}

/**
 * Prefer a stable internal identifier; fall back to an ordinal key.
 * Values are React keys only and must never be written into DOM attributes.
 */
function stableKey(
  preferred: string | undefined | null,
  ...fallbackParts: Array<string | undefined | null>
): string {
  const primary = preferred?.trim();
  if (primary && primary.length > 0) return primary;
  for (const part of fallbackParts) {
    const trimmed = part?.trim();
    if (trimmed && trimmed.length > 0) return trimmed;
  }
  return "item";
}

function firstToolError(record: AgentActivityRecord): string | undefined {
  for (const tool of record.tools ?? []) {
    if (tool.error && tool.error.trim().length > 0) return tool.error;
  }
  return undefined;
}

/** True when the path is a portable project-relative POSIX-style path. */
export function isPortableRelativePath(path: string): boolean {
  if (typeof path !== "string") return false;
  const trimmed = path.trim();
  if (trimmed.length === 0) return false;
  if (trimmed.includes("\\")) return false;
  if (trimmed.startsWith("/")) return false;
  if (/^[A-Za-z]:/.test(trimmed)) return false;
  if (trimmed.includes("://")) return false;
  if (trimmed.split("/").some((part) => part === ".." || part === "")) {
    return false;
  }
  return true;
}

export function opaqueErrorMessage(error: unknown): string | null {
  if (error == null) return null;
  const message =
    typeof error === "string"
      ? error.trim()
      : error instanceof Error
        ? error.message.trim()
        : String(error).trim();
  if (message.length === 0) return null;
  if (looksLikeMachinePath(message) || looksLikeDiagnosticId(message)) {
    return OPAQUE_ERROR_MESSAGE;
  }
  return message;
}

function looksLikeMachinePath(message: string): boolean {
  return (
    /[\\/]/.test(message)
    || /[A-Za-z]:\\/.test(message)
    || message.includes("://")
    || /[A-Za-z]:\//.test(message)
  );
}

function looksLikeDiagnosticId(message: string): boolean {
  return /session[-_]|run[-_]|tool[-_]?call/i.test(message);
}

function resourceLeafName(path: string): string | null {
  const normalized = path.replace(/\\/g, "/");
  const parts = normalized.split("/").filter(Boolean);
  return parts[parts.length - 1] ?? null;
}

function normalizeToken(value: string | undefined): string {
  return (value ?? "").trim().toLowerCase();
}

function humanizeToken(value: string): string {
  const trimmed = value.trim();
  if (!trimmed) return "未知";
  return trimmed;
}

/**
 * Collect internal IDs that must never appear in display fields merely because
 * an upstream label/title/progress field repeats them. Merges the mapper's
 * forbiddenDisplayIds carrier (receipt / embedded resource / etc.) with IDs
 * still present on projected fields. The carrier itself is never rendered.
 */
function collectForbiddenIds(record: AgentActivityRecord): string[] {
  const ids: string[] = [];
  const seen = new Set<string>();
  const push = (value: string | undefined | null) => {
    const trimmed = value?.trim();
    if (!trimmed || trimmed.length === 0) return;
    if (seen.has(trimmed)) return;
    seen.add(trimmed);
    ids.push(trimmed);
  };

  // Mapper-supplied carrier first (covers IDs discarded before projection).
  for (const id of record.forbiddenDisplayIds ?? []) {
    push(id);
  }

  push(record.runId);
  push(record.sessionId);
  // recordIdentity is also an internal ID and must not appear in display.
  push(record.recordIdentity);
  for (const step of record.steps ?? []) {
    push(step.id);
  }
  for (const tool of record.tools ?? []) {
    push(tool.toolCallId);
  }
  for (const agent of record.subagents ?? []) {
    push(agent.subagentId);
  }
  for (const artifact of record.artifacts ?? []) {
    push(artifact.artifactId);
  }
  for (const outcome of record.writeOutcomes ?? []) {
    push(outcome.proposalId);
  }
  for (const notice of record.notices ?? []) {
    push(notice.id);
  }
  return ids;
}

/**
 * Sanitize exact or whole-token occurrences of known internal IDs from visible
 * labels/titles/progress. Does not mutate portable paths. Ordinary user text
 * without those tokens is preserved.
 */
function sanitizeDisplayText(
  value: string | undefined | null,
  forbiddenIds: readonly string[],
): string | null {
  if (value == null) return null;
  let cleaned = value
    .replace(/[\u0000-\u001f\u007f]/g, " ")
    .trim();
  if (cleaned.length === 0) return null;

  // Entire label is a session/run style identifier.
  if (/^(session|run)[-_]/i.test(cleaned)) return null;

  for (const id of forbiddenIds) {
    if (!id || id.length === 0) continue;
    if (cleaned === id) return null;
    const escaped = escapeRegExp(id);
    // Whole-token only: do not redact substrings of ordinary words.
    const tokenPattern = new RegExp(
      `(^|[^A-Za-z0-9_-])${escaped}(?=$|[^A-Za-z0-9_-])`,
      "g",
    );
    cleaned = cleaned
      .replace(tokenPattern, "$1")
      .replace(/\s{2,}/g, " ")
      .replace(/\s+([,.;:!?])/g, "$1")
      .trim();
    if (cleaned.length === 0) return null;
  }

  if (/^(session|run)[-_]/i.test(cleaned)) return null;
  return cleaned;
}

function escapeRegExp(value: string): string {
  return value.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");
}
