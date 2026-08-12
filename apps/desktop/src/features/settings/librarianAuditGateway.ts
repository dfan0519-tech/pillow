/**
 * Renderer contract for the built-in, zero-token project knowledge audit.
 * The only host input is a project id. Host paths and raw command output never
 * cross this boundary; every response level is validated against exact keys.
 */
import { invoke } from "@tauri-apps/api/core";
import type {
  LibrarianAuditCheck,
  LibrarianAuditCheckId,
  LibrarianAuditFinding,
  LibrarianAuditGateway,
  LibrarianAuditReport,
  LibrarianAuditStatus,
} from "./librarianAuditTypes";

export const LIBRARIAN_AUDIT_REPORT_DTO_KEYS = [
  "projectId",
  "outcome",
  "checkedAt",
  "checks",
] as const;

export const LIBRARIAN_AUDIT_CHECK_DTO_KEYS = [
  "id",
  "status",
  "summary",
  "findings",
] as const;

export const LIBRARIAN_AUDIT_FINDING_DTO_KEYS = [
  "code",
  "summary",
] as const;

export const LIBRARIAN_AUDIT_CHECK_IDS: readonly LibrarianAuditCheckId[] = [
  "inflight_handoff_consistency",
  "pending_acceptance_visibility",
  "handoff_ledger_references",
  "inflight_inventory",
];

export type LibrarianAuditGatewayErrorCode =
  | "invalid_librarian_audit_response"
  | "librarian_audit_project_unavailable"
  | "librarian_audit_unavailable"
  | "librarian_audit_failed";

const ERROR_MESSAGES: Readonly<Record<LibrarianAuditGatewayErrorCode, string>> = {
  invalid_librarian_audit_response: "Pillow 收到无效的项目体检结果。",
  librarian_audit_project_unavailable: "无法读取当前项目。",
  librarian_audit_unavailable: "宿主尚未提供项目体检。",
  librarian_audit_failed: "项目体检失败。",
};

const MAX_PROJECT_ID_CHARS = 128;
const MAX_SUMMARY_CHARS = 1_000;
const MAX_FINDINGS_PER_CHECK = 256;
const FINDING_CODE = /^[a-z][a-z0-9_]{0,63}$/u;
const RFC3339 = /^\d{4}-\d{2}-\d{2}T\d{2}:\d{2}:\d{2}(?:\.\d{1,9})?(?:Z|[+-]\d{2}:\d{2})$/u;
const WINDOWS_ABSOLUTE_PATH = /[A-Za-z]:[\\/]/u;
const WINDOWS_UNC_PATH = /\\\\[^\\\s]+\\[^\\\s]+/u;
const POSIX_HOST_PATH = /(?:^|\s)\/(?:Users|home|root|etc|var|tmp|private|mnt|opt|srv|Volumes)(?:\/|\s|$)/iu;
const PRIVATE_IDENTIFIER_LABEL = /(?:lease|session)[ _-]?id\b|(?:租约|会话)\s*(?:id|标识)/iu;
const SECRET_VALUE = /(?:bearer\s+[A-Za-z0-9._-]+|sk-[A-Za-z0-9_-]{8,}|(?:api[_ -]?key|token|password)\s*[:=]\s*\S+)/iu;

export class LibrarianAuditGatewayError extends Error {
  readonly code: LibrarianAuditGatewayErrorCode;

  constructor(code: LibrarianAuditGatewayErrorCode) {
    super(ERROR_MESSAGES[code]);
    this.name = "LibrarianAuditGatewayError";
    this.code = code;
  }
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === "object" && value !== null && !Array.isArray(value);
}

function hasExactKeys(
  value: Record<string, unknown>,
  keys: readonly string[],
): boolean {
  const actual = Object.keys(value);
  return actual.length === keys.length
    && keys.every((key) => Object.prototype.hasOwnProperty.call(value, key));
}

function invalidResponse(): LibrarianAuditGatewayError {
  return new LibrarianAuditGatewayError("invalid_librarian_audit_response");
}

function isAuditStatus(value: unknown): value is LibrarianAuditStatus {
  return value === "pass" || value === "fail";
}

function isAuditCheckId(value: unknown): value is LibrarianAuditCheckId {
  return typeof value === "string"
    && (LIBRARIAN_AUDIT_CHECK_IDS as readonly string[]).includes(value);
}

/** Evidence is bounded, human-readable, and cannot carry host-private data. */
function isSafeEvidenceText(value: unknown): value is string {
  return typeof value === "string"
    && value.trim().length > 0
    && value.length <= MAX_SUMMARY_CHARS
    && !WINDOWS_ABSOLUTE_PATH.test(value)
    && !WINDOWS_UNC_PATH.test(value)
    && !POSIX_HOST_PATH.test(value)
    && !PRIVATE_IDENTIFIER_LABEL.test(value)
    && !SECRET_VALUE.test(value);
}

function parseFinding(value: unknown): LibrarianAuditFinding {
  if (
    !isRecord(value)
    || !hasExactKeys(value, LIBRARIAN_AUDIT_FINDING_DTO_KEYS)
    || typeof value.code !== "string"
    || !FINDING_CODE.test(value.code)
    || !isSafeEvidenceText(value.summary)
  ) {
    throw invalidResponse();
  }
  return { code: value.code, summary: value.summary };
}

function parseCheck(value: unknown): LibrarianAuditCheck {
  if (
    !isRecord(value)
    || !hasExactKeys(value, LIBRARIAN_AUDIT_CHECK_DTO_KEYS)
    || !isAuditCheckId(value.id)
    || !isAuditStatus(value.status)
    || !isSafeEvidenceText(value.summary)
    || !Array.isArray(value.findings)
    || value.findings.length > MAX_FINDINGS_PER_CHECK
  ) {
    throw invalidResponse();
  }
  const findings = value.findings.map(parseFinding);
  if (
    (value.status === "pass" && findings.length !== 0)
    || (value.status === "fail" && findings.length === 0)
  ) {
    throw invalidResponse();
  }
  return {
    id: value.id,
    status: value.status,
    summary: value.summary,
    findings,
  };
}

export function parseLibrarianAuditReport(
  value: unknown,
  expectedProjectId: string,
): LibrarianAuditReport {
  if (
    !isRecord(value)
    || !hasExactKeys(value, LIBRARIAN_AUDIT_REPORT_DTO_KEYS)
    || typeof value.projectId !== "string"
    || value.projectId.length === 0
    || value.projectId.length > MAX_PROJECT_ID_CHARS
    || value.projectId !== expectedProjectId
    || !isAuditStatus(value.outcome)
    || typeof value.checkedAt !== "string"
    || !RFC3339.test(value.checkedAt)
    || !Number.isFinite(Date.parse(value.checkedAt))
    || !Array.isArray(value.checks)
    || value.checks.length !== LIBRARIAN_AUDIT_CHECK_IDS.length
  ) {
    throw invalidResponse();
  }

  const parsed = value.checks.map(parseCheck);
  const byId = new Map(parsed.map((check) => [check.id, check]));
  if (byId.size !== LIBRARIAN_AUDIT_CHECK_IDS.length) {
    throw invalidResponse();
  }
  const checks = LIBRARIAN_AUDIT_CHECK_IDS.map((id) => {
    const check = byId.get(id);
    if (!check) throw invalidResponse();
    return check;
  });
  const expectedOutcome = checks.some((check) => check.status === "fail")
    ? "fail"
    : "pass";
  if (value.outcome !== expectedOutcome) {
    throw invalidResponse();
  }

  return {
    projectId: value.projectId,
    outcome: value.outcome,
    checkedAt: value.checkedAt,
    checks,
  };
}

function normalizeError(cause: unknown): LibrarianAuditGatewayError {
  if (cause instanceof LibrarianAuditGatewayError) return cause;
  if (isRecord(cause) && typeof cause.code === "string") {
    if (Object.prototype.hasOwnProperty.call(ERROR_MESSAGES, cause.code)) {
      return new LibrarianAuditGatewayError(
        cause.code as LibrarianAuditGatewayErrorCode,
      );
    }
  }
  return new LibrarianAuditGatewayError("librarian_audit_failed");
}

export class TauriLibrarianAuditGateway implements LibrarianAuditGateway {
  async runLibrarianAudit(projectId: string): Promise<LibrarianAuditReport> {
    try {
      const payload = await invoke<unknown>("run_librarian_audit", {
        projectId,
      });
      return parseLibrarianAuditReport(payload, projectId);
    } catch (cause) {
      // Host detail may carry native paths or private ids; never echo or log it.
      throw normalizeError(cause);
    }
  }
}

/** Injectable deterministic gateway for renderer tests; never a product fallback. */
export class MockLibrarianAuditGateway implements LibrarianAuditGateway {
  private callIndex = 0;

  constructor(private readonly reports: readonly LibrarianAuditReport[]) {
    if (reports.length === 0) {
      throw new Error("MockLibrarianAuditGateway requires at least one report.");
    }
  }

  async runLibrarianAudit(projectId: string): Promise<LibrarianAuditReport> {
    const index = Math.min(this.callIndex, this.reports.length - 1);
    this.callIndex += 1;
    return parseLibrarianAuditReport(this.reports[index], projectId);
  }
}
