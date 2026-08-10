import { invoke } from "@tauri-apps/api/core";
import {
  ExecGatewayError,
  isExecPublicError,
  normalizeExecError,
  type ExecDecision,
  type ExecGateway,
  type ExecOutputView,
  type ExecProposalView,
} from "./execGateway";
import {
  isAutoApprovedByRuleValue,
  isRuleSubjectValue,
} from "./permissionRulesGateway";
import { SHELL_PROGRAM_LABEL } from "../features/execution/executionTypes";

const STATES = ["pending_approval", "approved", "running", "succeeded", "failed", "rejected", "expired", "cancelled", "interrupted"] as const;
const ORIGINS = ["agent", "user"] as const;
const RISKS = ["standard", "high"] as const;
const STREAMS = ["stdout", "stderr"] as const;
const MAX_ID = 256;
const MAX_PATH = 1024;
const MAX_ARGV = 64;
const MAX_ARGV_BYTES = 32_768;
const MAX_OUTPUT_BYTES = 256 * 1024;
const MAX_OUTPUT_LINES = 4_000;
const MAX_CHUNKS = 4_000;
const MAX_LIST = 256;

const proposalKeys = ["proposalId", "conversationId", "origin", "state", "programLabel", "argv", "portableCwd", "riskClass", "summary", "expiresAt", "decisionAvailable", "errorCode", "ruleSubject", "autoApprovedByRule"];
/** Process proposals can only ever be decided by a `process` rule. */
const EXEC_RULE_PREFIXES = ["process"] as const;
const outputKeys = ["proposalId", "state", "chunks", "liveRetained", "truncated", "encodingLossy", "capturedBytes", "exitCode", "durationMs"];

function invalid(): ExecGatewayError {
  return new ExecGatewayError("process_unavailable", "Pillow received an invalid process response.");
}
function record(value: unknown): value is Record<string, unknown> { return typeof value === "object" && value !== null && !Array.isArray(value); }
function only(value: Record<string, unknown>, keys: readonly string[]): boolean { return Object.keys(value).every((key) => keys.includes(key)); }
function bounded(value: unknown, max: number): value is string { return typeof value === "string" && value.length > 0 && value.length <= max; }
function integer(value: unknown, min = 0, max = Number.MAX_SAFE_INTEGER): value is number { return typeof value === "number" && Number.isSafeInteger(value) && value >= min && value <= max; }
function enumValue<T extends string>(value: unknown, values: readonly T[]): value is T { return typeof value === "string" && (values as readonly string[]).includes(value); }
function portable(value: unknown): value is string {
  if (value === ".") return true;
  if (typeof value !== "string" || value.length === 0 || value.length > MAX_PATH || value.includes("\\") || value.includes(":") || value.startsWith("/") || value.startsWith("\\")) return false;
  return value.split("/").every((part) => part.length > 0 && part !== "." && part !== ".." && part.length <= 255 && !/[<>:"|?*\u0000-\u001f]/.test(part));
}
function utf8(value: string): number { return new TextEncoder().encode(value).length; }
function lines(value: string): number { return value.length === 0 ? 0 : value.split("\n").length; }
function timestamp(value: unknown): value is string { return typeof value === "string" && value.length > 0 && value.length <= 128 && Number.isFinite(Date.parse(value)); }

export function normalizeExecProposal(value: unknown): ExecProposalView {
  if (!record(value) || !only(value, proposalKeys) || !bounded(value.proposalId, MAX_ID) || !bounded(value.conversationId, MAX_ID) || !enumValue(value.origin, ORIGINS) || !enumValue(value.state, STATES) || !bounded(value.programLabel, 32) || !Array.isArray(value.argv) || value.argv.length > MAX_ARGV || !portable(value.portableCwd) || !enumValue(value.riskClass, RISKS) || typeof value.summary !== "string" || value.summary.length > 1024 || !timestamp(value.expiresAt) || typeof value.decisionAvailable !== "boolean") throw invalid();
  if (!value.argv.every((arg) => typeof arg === "string") || utf8(value.programLabel) + value.argv.reduce<number>((sum, arg) => sum + utf8(arg), 0) > MAX_ARGV_BYTES || (value.errorCode !== undefined && !isExecPublicError(value.errorCode))) throw invalid();
  // A shell proposal legitimately has no rule subject: it is decided by a human
  // per command, so there is nothing for a rule to match yet. The host says so
  // by sending an empty string. Requiring non-empty here rejected every shell
  // proposal, which surfaced as the approval bar simply never appearing — the
  // same failure shape as the widened-DTO seam, from the opposite direction.
  const ruleSubject = typeof value.ruleSubject === "string" ? value.ruleSubject : undefined;
  const subjectOk = ruleSubject !== undefined
    && (isRuleSubjectValue(ruleSubject)
      || (value.programLabel === SHELL_PROGRAM_LABEL && ruleSubject === ""));
  if (!subjectOk || (value.autoApprovedByRule !== undefined && !isAutoApprovedByRuleValue(value.autoApprovedByRule, EXEC_RULE_PREFIXES))) throw invalid();
  return { proposalId: value.proposalId, conversationId: value.conversationId, origin: value.origin, state: value.state, programLabel: value.programLabel, argv: [...value.argv], portableCwd: value.portableCwd, riskClass: value.riskClass, summary: value.summary, expiresAt: value.expiresAt, decisionAvailable: value.decisionAvailable, ruleSubject, ...(value.autoApprovedByRule === undefined ? {} : { autoApprovedByRule: { prefix: value.autoApprovedByRule.prefix, pattern: value.autoApprovedByRule.pattern, action: value.autoApprovedByRule.action } }), ...(value.errorCode === undefined ? {} : { errorCode: value.errorCode }) };
}

export function normalizeExecOutput(value: unknown): ExecOutputView {
  if (!record(value) || !only(value, outputKeys) || !bounded(value.proposalId, MAX_ID) || !enumValue(value.state, STATES) || !Array.isArray(value.chunks) || value.chunks.length > MAX_CHUNKS || typeof value.liveRetained !== "boolean" || typeof value.truncated !== "boolean" || typeof value.encodingLossy !== "boolean" || !integer(value.capturedBytes, 0, MAX_OUTPUT_BYTES) || !(value.exitCode === null || integer(value.exitCode, -2147483648, 2147483647)) || !(value.durationMs === null || integer(value.durationMs))) throw invalid();
  let byteCount = 0; let lineCount = 0; let previous = -1;
  const chunks = value.chunks.map((chunk) => {
    if (!record(chunk) || !only(chunk, ["stream", "sequence", "text"]) || !enumValue(chunk.stream, STREAMS) || !integer(chunk.sequence) || chunk.sequence <= previous || typeof chunk.text !== "string") throw invalid();
    previous = chunk.sequence; byteCount += utf8(chunk.text); lineCount += lines(chunk.text);
    return { stream: chunk.stream, sequence: chunk.sequence, text: chunk.text };
  });
  if (byteCount > MAX_OUTPUT_BYTES || lineCount > MAX_OUTPUT_LINES || value.capturedBytes < byteCount) throw invalid();
  return { proposalId: value.proposalId, state: value.state, chunks, liveRetained: value.liveRetained, truncated: value.truncated, encodingLossy: value.encodingLossy, capturedBytes: value.capturedBytes, exitCode: value.exitCode, durationMs: value.durationMs };
}

export class TauriExecGateway implements ExecGateway {
  async listExecProposals(conversationId: string): Promise<ExecProposalView[]> { try { const payload = await invoke<unknown>("list_exec_proposals", { conversationId }); if (!Array.isArray(payload) || payload.length > MAX_LIST) throw invalid(); return payload.map(normalizeExecProposal); } catch (error) { throw normalizeExecError(error); } }
  async decideExecProposal(input: { conversationId: string; proposalId: string; decision: ExecDecision }): Promise<ExecProposalView> { try { const payload = await invoke<unknown>("decide_exec_proposal", input); return normalizeExecProposal(payload); } catch (error) { throw normalizeExecError(error); } }
  async openExecOutput(input: { conversationId: string; proposalId: string }): Promise<ExecOutputView> { try { const payload = await invoke<unknown>("open_exec_output", input); return normalizeExecOutput(payload); } catch (error) { throw normalizeExecError(error); } }
}
