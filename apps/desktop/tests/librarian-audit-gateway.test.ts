import { invoke } from "@tauri-apps/api/core";
import { beforeEach, describe, expect, it, vi } from "vitest";
import {
  LIBRARIAN_AUDIT_CHECK_DTO_KEYS,
  LIBRARIAN_AUDIT_FINDING_DTO_KEYS,
  LIBRARIAN_AUDIT_REPORT_DTO_KEYS,
  LibrarianAuditGatewayError,
  TauriLibrarianAuditGateway,
  parseLibrarianAuditReport,
} from "../src/features/settings/librarianAuditGateway";
import type { LibrarianAuditReport } from "../src/features/settings/librarianAuditTypes";

vi.mock("@tauri-apps/api/core", () => ({
  invoke: vi.fn(),
}));

const PROJECT = "project-1";

const GOOD_REPORT: LibrarianAuditReport = {
  projectId: PROJECT,
  outcome: "fail",
  checkedAt: "2026-08-12T03:00:00.000Z",
  checks: [
    {
      id: "inflight_handoff_consistency",
      status: "fail",
      summary: "交接中的在途计数与台账不一致。",
      findings: [
        {
          code: "inflight_count_mismatch",
          summary: "交接计数 0，台账计数 2。",
        },
      ],
    },
    {
      id: "pending_acceptance_visibility",
      status: "pass",
      summary: "待验收任务均已在交接中提及。",
      findings: [],
    },
    {
      id: "handoff_ledger_references",
      status: "pass",
      summary: "交接中的任务引用均存在于台账。",
      findings: [],
    },
    {
      id: "inflight_inventory",
      status: "pass",
      summary: "台账完整枚举 2 项在途任务。",
      findings: [],
    },
  ],
};

function cloneReport(): LibrarianAuditReport {
  return structuredClone(GOOD_REPORT);
}

describe("librarian audit DTO contract", () => {
  it("accepts the complete three-level DTO and pins non-empty closed key lists", () => {
    expect(parseLibrarianAuditReport(GOOD_REPORT, PROJECT)).toEqual(GOOD_REPORT);
    expect(LIBRARIAN_AUDIT_REPORT_DTO_KEYS).toEqual([
      "projectId",
      "outcome",
      "checkedAt",
      "checks",
    ]);
    expect(LIBRARIAN_AUDIT_CHECK_DTO_KEYS).toEqual([
      "id",
      "status",
      "summary",
      "findings",
    ]);
    expect(LIBRARIAN_AUDIT_FINDING_DTO_KEYS).toEqual(["code", "summary"]);
  });

  it("rejects unknown or missing keys at every DTO level", () => {
    expect(() =>
      parseLibrarianAuditReport(
        { ...GOOD_REPORT, nativePath: "C:\\private" },
        PROJECT,
      )
    ).toThrow(LibrarianAuditGatewayError);

    const missing = cloneReport() as Partial<LibrarianAuditReport>;
    delete missing.checkedAt;
    expect(() => parseLibrarianAuditReport(missing, PROJECT)).toThrow();

    const checkExtra = cloneReport();
    Object.assign(checkExtra.checks[0]!, { grant: "smuggled" });
    expect(() => parseLibrarianAuditReport(checkExtra, PROJECT)).toThrow();

    const findingExtra = cloneReport();
    Object.assign(findingExtra.checks[0]!.findings[0]!, {
      sessionId: "must-not-pass",
    });
    expect(() => parseLibrarianAuditReport(findingExtra, PROJECT)).toThrow();
  });

  it("requires exactly the four distinct checks and matching project", () => {
    const missing = cloneReport();
    missing.checks.pop();
    expect(() => parseLibrarianAuditReport(missing, PROJECT)).toThrow();

    const duplicate = cloneReport();
    duplicate.checks[3] = structuredClone(duplicate.checks[0]!);
    expect(() => parseLibrarianAuditReport(duplicate, PROJECT)).toThrow();

    expect(() => parseLibrarianAuditReport(GOOD_REPORT, "other-project")).toThrow();
  });

  it("pins PASS/FAIL findings and overall outcome in both directions", () => {
    const passWithFinding = cloneReport();
    passWithFinding.checks[1]!.findings = [
      { code: "invented_problem", summary: "不应出现在 PASS 中。" },
    ];
    expect(() => parseLibrarianAuditReport(passWithFinding, PROJECT)).toThrow();

    const failWithoutFinding = cloneReport();
    failWithoutFinding.checks[0]!.findings = [];
    expect(() => parseLibrarianAuditReport(failWithoutFinding, PROJECT)).toThrow();

    const falseGreen = cloneReport();
    falseGreen.outcome = "pass";
    expect(() => parseLibrarianAuditReport(falseGreen, PROJECT)).toThrow();

    const allPass = cloneReport();
    allPass.outcome = "pass";
    allPass.checks[0] = {
      ...allPass.checks[0]!,
      status: "pass",
      summary: "交接中的在途计数与台账一致。",
      findings: [],
    };
    expect(parseLibrarianAuditReport(allPass, PROJECT).outcome).toBe("pass");
  });

  it.each([
    "发现于 C:\\Users\\someone\\private.md",
    "发现于 /home/someone/private.md",
    "session id: hidden-value",
    "lease_id=hidden-value",
  ])("rejects privacy-unsafe evidence text: %s", (summary) => {
    const unsafe = cloneReport();
    unsafe.checks[0]!.findings[0]!.summary = summary;
    expect(() => parseLibrarianAuditReport(unsafe, PROJECT)).toThrow();
  });
});

describe("TauriLibrarianAuditGateway", () => {
  beforeEach(() => {
    vi.mocked(invoke).mockReset();
  });

  it("invokes the host with the exact command name and projectId-only input", async () => {
    vi.mocked(invoke).mockResolvedValueOnce(GOOD_REPORT);
    const gateway = new TauriLibrarianAuditGateway();

    await expect(gateway.runLibrarianAudit(PROJECT)).resolves.toEqual(GOOD_REPORT);
    expect(invoke).toHaveBeenCalledTimes(1);
    expect(invoke).toHaveBeenCalledWith("run_librarian_audit", {
      projectId: PROJECT,
    });
    const args = vi.mocked(invoke).mock.calls[0]![1] as Record<string, unknown>;
    expect(Object.keys(args)).toEqual(["projectId"]);
    expect(args).not.toHaveProperty("rootPath");
    expect(args).not.toHaveProperty("handoffPath");
    expect(args).not.toHaveProperty("ledgerPath");
  });

  it("rejects hostile responses and never echoes host error details", async () => {
    vi.mocked(invoke)
      .mockResolvedValueOnce({ ...GOOD_REPORT, nativePath: "C:\\private" })
      .mockRejectedValueOnce({
        code: "librarian_audit_failed",
        message: "failed at C:\\Users\\private session id=hidden",
      });
    const gateway = new TauriLibrarianAuditGateway();

    await expect(gateway.runLibrarianAudit(PROJECT)).rejects.toMatchObject({
      code: "invalid_librarian_audit_response",
    });
    try {
      await gateway.runLibrarianAudit(PROJECT);
      expect.unreachable("expected failure");
    } catch (error) {
      expect(error).toBeInstanceOf(LibrarianAuditGatewayError);
      expect((error as LibrarianAuditGatewayError).code).toBe(
        "librarian_audit_failed",
      );
      expect((error as Error).message).toBe("项目体检失败。");
      expect((error as Error).message).not.toMatch(/Users|session|hidden/u);
    }
  });
});
