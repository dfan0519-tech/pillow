export type LibrarianAuditCheckId =
  | "inflight_handoff_consistency"
  | "pending_acceptance_visibility"
  | "handoff_ledger_references"
  | "inflight_inventory";

export type LibrarianAuditStatus = "pass" | "fail";

export type LibrarianAuditFinding = {
  code: string;
  summary: string;
};

export type LibrarianAuditCheck = {
  id: LibrarianAuditCheckId;
  status: LibrarianAuditStatus;
  summary: string;
  findings: LibrarianAuditFinding[];
};

export type LibrarianAuditReport = {
  projectId: string;
  outcome: LibrarianAuditStatus;
  checkedAt: string;
  checks: LibrarianAuditCheck[];
};

export interface LibrarianAuditGateway {
  runLibrarianAudit(projectId: string): Promise<LibrarianAuditReport>;
}
