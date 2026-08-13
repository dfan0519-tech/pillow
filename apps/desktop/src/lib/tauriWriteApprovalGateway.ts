/**
 * Tauri write-approval gateway — exact C3-D command invokes only.
 * Validates camelCase host DTOs; never accepts root/content/digest fields.
 */

import { invoke } from "@tauri-apps/api/core";
import {
  FIXED_INVALID_WRITE_RESPONSE_MESSAGE,
  WriteApprovalGatewayError,
  isWritePublicErrorCode,
  normalizeWriteApprovalError,
  type WriteApprovalGateway,
  type WriteContentPageView,
  type WriteDecision,
  type WriteDiffView,
  type WriteMode,
  type WriteProposalState,
  type WriteProposalView,
} from "./writeApprovalGateway";
import {
  isAutoApprovedByRuleValue,
  isRuleSubjectValue,
} from "./permissionRulesGateway";

const WRITE_MODES: WriteMode[] = ["create", "modify"];

/** Write proposals can only ever be decided by a `write` or `edit` rule. */
const WRITE_RULE_PREFIXES = ["write", "edit"] as const;

const WRITE_STATES: WriteProposalState[] = [
  "pending_approval",
  "approved",
  "applying",
  "succeeded",
  "rejected",
  "expired",
  "cancelled",
  "interrupted",
  "failed",
];

const MAX_ID_CHARS = 256;
const MAX_PATH_CHARS = 1_024;
const MAX_SUMMARY_CHARS = 1_024;
const MAX_DIFF_UTF8_BYTES = 262_144;
const MAX_DIFF_LINE_COUNT = 4_000;
const MAX_LIST = 256;

function invalidWriteResponse(): WriteApprovalGatewayError {
  return new WriteApprovalGatewayError(
    "invalid_write_response",
    FIXED_INVALID_WRITE_RESPONSE_MESSAGE,
  );
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === "object" && value !== null && !Array.isArray(value);
}

function isNonEmptyString(value: unknown): value is string {
  return typeof value === "string" && value.trim().length > 0;
}

function isBoundedNonEmptyString(
  value: unknown,
  maxChars: number,
): value is string {
  return isNonEmptyString(value) && value.length <= maxChars;
}

function isSafeInteger(
  value: unknown,
  minimum = 0,
  maximum = Number.MAX_SAFE_INTEGER,
): value is number {
  return (
    typeof value === "number"
    && Number.isSafeInteger(value)
    && value >= minimum
    && value <= maximum
  );
}

function hasOnlyKeys(
  value: Record<string, unknown>,
  keys: readonly string[],
): boolean {
  const allowed = new Set(keys);
  return Object.keys(value).every((key) => allowed.has(key));
}

function isWriteMode(value: unknown): value is WriteMode {
  return typeof value === "string" && (WRITE_MODES as string[]).includes(value);
}

function isWriteState(value: unknown): value is WriteProposalState {
  return typeof value === "string" && (WRITE_STATES as string[]).includes(value);
}

/** Portable relative path: slash-separated components; never absolute/native. */
function isPortableRelativePath(value: unknown): value is string {
  if (typeof value !== "string" || value.length === 0 || value.length > MAX_PATH_CHARS) {
    return false;
  }
  if (
    value.startsWith("/")
    || value.startsWith("\\")
    || value.includes("\\")
    || value.includes(":")
    || value.includes("..")
  ) {
    return false;
  }
  return value.split("/").every((component) => {
    if (component.length === 0 || component === "." || component === "..") {
      return false;
    }
    if (component.length > 255) return false;
    return !/[<>:"|?*\u0000-\u001f]/.test(component);
  });
}

/** Finite parseable timestamp (ISO or Date-compatible). */
function isFiniteTimestamp(value: unknown): value is string {
  if (typeof value !== "string" || value.length === 0 || value.length > 128) {
    return false;
  }
  const ms = Date.parse(value);
  return Number.isFinite(ms);
}

function utf8ByteLength(value: string): number {
  return new TextEncoder().encode(value).length;
}

/**
 * Mirror tool-executor `count_lines` exactly:
 * empty → 0; count `\n` terminators; text without a trailing newline still has
 * one logical line.
 */
function countDiffLines(text: string): number {
  if (text.length === 0) {
    return 0;
  }
  const bytes = new TextEncoder().encode(text);
  let newlines = 0;
  for (const byte of bytes) {
    if (byte === 0x0a) {
      newlines += 1;
    }
  }
  if (bytes[bytes.length - 1] === 0x0a) {
    return newlines;
  }
  return newlines + 1;
}

function parseWriteProposalView(value: unknown): WriteProposalView {
  if (
    !isRecord(value)
    || !hasOnlyKeys(value, [
      "proposalId",
      "conversationId",
      "path",
      "mode",
      "state",
      "summary",
      "expiresAt",
      "decisionAvailable",
      "errorCode",
      "ruleSubject",
      "autoApprovedByRule",
      "diffTruncated",
      "contentDigest",
      "contentReviewComplete",
    ])
  ) {
    throw invalidWriteResponse();
  }
  if (
    !isBoundedNonEmptyString(value.proposalId, MAX_ID_CHARS)
    || !isBoundedNonEmptyString(value.conversationId, MAX_ID_CHARS)
    || !isPortableRelativePath(value.path)
    || !isWriteMode(value.mode)
    || !isWriteState(value.state)
    || typeof value.summary !== "string"
    || value.summary.length > MAX_SUMMARY_CHARS
    || !isFiniteTimestamp(value.expiresAt)
    || typeof value.decisionAvailable !== "boolean"
    || (value.diffTruncated !== undefined && typeof value.diffTruncated !== "boolean")
    || (value.contentReviewComplete !== undefined
      && typeof value.contentReviewComplete !== "boolean")
    || (value.contentDigest !== undefined && !isContentDigest(value.contentDigest))
    || !isRuleSubjectValue(value.ruleSubject)
    || (value.autoApprovedByRule !== undefined
      && !isAutoApprovedByRuleValue(value.autoApprovedByRule, WRITE_RULE_PREFIXES))
  ) {
    throw invalidWriteResponse();
  }

  // Reject authority-smuggling fields if a host ever nests them.
  // Raw grants must never reappear on the write proposal wire (EA-03).
  for (const forbidden of [
    "root",
    "content",
    "base",
    "digest",
    "fingerprint",
    "baseDigest",
    "proposedDigest",
    "proposedContent",
    "baseContent",
    "projectRoot",
    "approvalGrant",
    "approval_grant",
  ]) {
    if (forbidden in value) {
      throw invalidWriteResponse();
    }
  }

  if (
    value.errorCode !== undefined
    && !isWritePublicErrorCode(value.errorCode)
  ) {
    throw invalidWriteResponse();
  }

  const view: WriteProposalView = {
    proposalId: value.proposalId,
    conversationId: value.conversationId,
    path: value.path,
    mode: value.mode,
    state: value.state,
    summary: value.summary,
    expiresAt: value.expiresAt,
    decisionAvailable: value.decisionAvailable,
    ruleSubject: value.ruleSubject,
  };
  if (typeof value.diffTruncated === "boolean") {
    view.diffTruncated = value.diffTruncated;
  }
  if (isContentDigest(value.contentDigest)) {
    view.contentDigest = value.contentDigest;
  }
  if (typeof value.contentReviewComplete === "boolean") {
    view.contentReviewComplete = value.contentReviewComplete;
  }
  if (isAutoApprovedByRuleValue(value.autoApprovedByRule, WRITE_RULE_PREFIXES)) {
    view.autoApprovedByRule = {
      prefix: value.autoApprovedByRule.prefix,
      pattern: value.autoApprovedByRule.pattern,
      action: value.autoApprovedByRule.action,
    };
  }
  if (isWritePublicErrorCode(value.errorCode)) {
    view.errorCode = value.errorCode;
  }
  return view;
}

function parseWriteDiffView(value: unknown): WriteDiffView {
  if (
    !isRecord(value)
    || !hasOnlyKeys(value, [
      "proposalId",
      "path",
      "mode",
      "unified",
      "truncated",
      "lineCount",
      "byteCount",
      "contentDigest",
    ])
  ) {
    throw invalidWriteResponse();
  }
  if (
    !isBoundedNonEmptyString(value.proposalId, MAX_ID_CHARS)
    || !isPortableRelativePath(value.path)
    || !isWriteMode(value.mode)
    || typeof value.unified !== "string"
    || typeof value.truncated !== "boolean"
    || !isSafeInteger(value.lineCount, 0, MAX_DIFF_LINE_COUNT)
    || !isSafeInteger(value.byteCount, 0, MAX_DIFF_UTF8_BYTES)
    || (value.contentDigest !== undefined && !isContentDigest(value.contentDigest))
  ) {
    throw invalidWriteResponse();
  }

  const actualByteCount = utf8ByteLength(value.unified);
  if (actualByteCount > MAX_DIFF_UTF8_BYTES) {
    throw invalidWriteResponse();
  }
  // Counts must describe the exact reviewed text (native BoundedDiff contract).
  if (value.byteCount !== actualByteCount) {
    throw invalidWriteResponse();
  }
  if (value.lineCount !== countDiffLines(value.unified)) {
    throw invalidWriteResponse();
  }

  for (const forbidden of [
    "root",
    "content",
    "base",
    "digest",
    "fingerprint",
    "approvalGrant",
  ]) {
    if (forbidden in value) {
      throw invalidWriteResponse();
    }
  }

  const view: WriteDiffView = {
    proposalId: value.proposalId,
    path: value.path,
    mode: value.mode,
    unified: value.unified,
    truncated: value.truncated,
    lineCount: value.lineCount,
    byteCount: value.byteCount,
  };
  if (isContentDigest(value.contentDigest)) {
    view.contentDigest = value.contentDigest;
  }
  return view;
}

function isContentDigest(value: unknown): value is string {
  return typeof value === "string" && /^sha256:[0-9a-f]{64}$/.test(value);
}

function parseWriteContentPageView(value: unknown): WriteContentPageView {
  if (
    !isRecord(value)
    || !hasOnlyKeys(value, [
      "proposalId",
      "path",
      "mode",
      "contentDigest",
      "pageIndex",
      "pageCount",
      "text",
      "lineCount",
      "byteCount",
      "reviewComplete",
    ])
  ) {
    throw invalidWriteResponse();
  }
  if (
    !isBoundedNonEmptyString(value.proposalId, MAX_ID_CHARS)
    || !isPortableRelativePath(value.path)
    || !isWriteMode(value.mode)
    || !isContentDigest(value.contentDigest)
    || !isSafeInteger(value.pageIndex, 0, 1_048_576)
    || !isSafeInteger(value.pageCount, 1, 1_048_576)
    || typeof value.text !== "string"
    || !isSafeInteger(value.lineCount, 0, MAX_DIFF_LINE_COUNT)
    || !isSafeInteger(value.byteCount, 0, MAX_DIFF_UTF8_BYTES)
    || typeof value.reviewComplete !== "boolean"
    || value.pageIndex >= value.pageCount
  ) {
    throw invalidWriteResponse();
  }

  const actualByteCount = utf8ByteLength(value.text);
  if (actualByteCount > MAX_DIFF_UTF8_BYTES) {
    throw invalidWriteResponse();
  }
  if (value.byteCount !== actualByteCount) {
    throw invalidWriteResponse();
  }
  if (value.lineCount !== countDiffLines(value.text)) {
    throw invalidWriteResponse();
  }

  for (const forbidden of [
    "root",
    "content",
    "base",
    "digest",
    "fingerprint",
    "approvalGrant",
    "proposedContent",
    "unified",
  ]) {
    if (forbidden in value) {
      throw invalidWriteResponse();
    }
  }

  return {
    proposalId: value.proposalId,
    path: value.path,
    mode: value.mode,
    contentDigest: value.contentDigest,
    pageIndex: value.pageIndex,
    pageCount: value.pageCount,
    text: value.text,
    lineCount: value.lineCount,
    byteCount: value.byteCount,
    reviewComplete: value.reviewComplete,
  };
}

/** Tauri-only boundary. Renderer never opens SQLite or host memory. */
export class TauriWriteApprovalGateway implements WriteApprovalGateway {
  async listWriteProposals(conversationId: string): Promise<WriteProposalView[]> {
    try {
      const payload = await invoke<unknown>("list_write_proposals", {
        conversationId,
      });
      if (!Array.isArray(payload) || payload.length > MAX_LIST) {
        throw invalidWriteResponse();
      }
      return payload.map(parseWriteProposalView);
    } catch (error) {
      throw normalizeWriteApprovalError(error);
    }
  }

  async decideWriteProposal(input: {
    conversationId: string;
    proposalId: string;
    decision: WriteDecision;
  }): Promise<WriteProposalView> {
    try {
      // Host looks up the live grant; renderer never supplies grant material.
      const payload = await invoke<unknown>("decide_write_proposal", {
        conversationId: input.conversationId,
        proposalId: input.proposalId,
        decision: input.decision,
      });
      return parseWriteProposalView(payload);
    } catch (error) {
      throw normalizeWriteApprovalError(error);
    }
  }

  async openWriteProposalDiff(input: {
    conversationId: string;
    proposalId: string;
  }): Promise<WriteDiffView> {
    try {
      const payload = await invoke<unknown>("open_write_proposal_diff", {
        conversationId: input.conversationId,
        proposalId: input.proposalId,
      });
      return parseWriteDiffView(payload);
    } catch (error) {
      throw normalizeWriteApprovalError(error);
    }
  }

  async openWriteProposalContentPage(input: {
    conversationId: string;
    proposalId: string;
    pageIndex: number;
  }): Promise<WriteContentPageView> {
    try {
      const payload = await invoke<unknown>("open_write_proposal_content_page", {
        conversationId: input.conversationId,
        proposalId: input.proposalId,
        pageIndex: input.pageIndex,
      });
      return parseWriteContentPageView(payload);
    } catch (error) {
      throw normalizeWriteApprovalError(error);
    }
  }
}
