/**
 * Pure, renderer-safe arbitration for the one visible governed-approval bar.
 *
 * This deliberately accepts only the fields needed to decide presentation.
 * Native grants, process authority, run/session identifiers, paths, and debug
 * metadata must remain outside this model.
 */

export type ApprovalDomain = "write" | "process";

export type ApprovalScope = Readonly<{
  projectId: string;
  conversationId: string;
}>;

export type ApprovalCandidate = Readonly<{
  domain: ApprovalDomain;
  projectId: string;
  requestId: string;
  conversationId: string;
  expiresAt: string;
  decisionAvailable: boolean;
  summary: string;
  risk: string;
}>;

/** The only domain data a renderer approval bar may receive from arbitration. */
export type VisibleApproval = Readonly<{
  domain: ApprovalDomain;
  requestId: string;
  conversationId: string;
  expiresAt: string;
  summary: string;
  risk: string;
}>;

function expiryTime(expiresAt: string): number | null {
  const timestamp = Date.parse(expiresAt);
  return Number.isFinite(timestamp) ? timestamp : null;
}

function comparesBefore(
  candidate: ApprovalCandidate,
  candidateExpiry: number,
  current: ApprovalCandidate,
  currentExpiry: number,
): boolean {
  if (candidateExpiry !== currentExpiry) return candidateExpiry < currentExpiry;

  if (candidate.requestId !== current.requestId) {
    return candidate.requestId < current.requestId;
  }

  // The contract defines the only cross-domain equality winner.
  return candidate.domain === "write" && current.domain === "process";
}

/**
 * Selects exactly one live request for a project conversation, without
 * mutating its inputs. Invalid expiry timestamps fail closed because their
 * ordering cannot be trusted for a decision deadline.
 */
export function selectVisibleApproval(
  scope: ApprovalScope,
  candidates: readonly ApprovalCandidate[],
): VisibleApproval | null {
  let selected: ApprovalCandidate | null = null;
  let selectedExpiry: number | null = null;

  for (const candidate of candidates) {
    if (
      !candidate.decisionAvailable
      || candidate.projectId !== scope.projectId
      || candidate.conversationId !== scope.conversationId
    ) {
      continue;
    }

    const candidateExpiry = expiryTime(candidate.expiresAt);
    if (candidateExpiry === null) continue;

    if (
      selected === null
      || selectedExpiry === null
      || comparesBefore(candidate, candidateExpiry, selected, selectedExpiry)
    ) {
      selected = candidate;
      selectedExpiry = candidateExpiry;
    }
  }

  if (selected === null) return null;

  return {
    domain: selected.domain,
    requestId: selected.requestId,
    conversationId: selected.conversationId,
    expiresAt: selected.expiresAt,
    summary: selected.summary,
    risk: selected.risk,
  };
}
