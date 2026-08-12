/**
 * Role-dispatch gateway — closed-key allowlist + distinct error copy.
 * Accept and reject directions are both pinned (reject-only suites are false-green).
 */
import { invoke } from "@tauri-apps/api/core";
import { beforeEach, describe, expect, it, vi } from "vitest";
import {
  ROLE_DISPATCH_ERROR_MESSAGES,
  ROLE_DISPATCH_PUBLIC_ERRORS,
  normalizeRoleDispatchError,
  roleDispatchErrorMessage,
  type RoleDispatchPublicError,
} from "../src/lib/roleDispatchGateway";
import {
  ROLE_DISPATCH_PROPOSAL_KEYS,
  ROLE_DISPATCH_RECORD_KEYS,
  TauriRoleDispatchGateway,
  normalizeRoleDispatchProposal,
  normalizeRoleDispatchRecord,
} from "../src/lib/tauriRoleDispatchGateway";

vi.mock("@tauri-apps/api/core", () => ({
  invoke: vi.fn(),
}));

const conversationId = "00000000-0000-0000-0000-0000000000c1";
const proposalId = "00000000-0000-0000-0000-0000000000p1";
const assignmentId = "00000000-0000-0000-0000-0000000000a1";
const initiatorSlotId = "00000000-0000-0000-0000-0000000000s1";
const targetSlotId = "00000000-0000-0000-0000-0000000000s2";
const callId = "00000000-0000-0000-0000-0000000000call";
const runId = "00000000-0000-0000-0000-0000000000run";

/** Nested process proposal — exact host shape admitted by exec allowlist. */
function hostExecProposal(overrides: Record<string, unknown> = {}) {
  return {
    proposalId,
    conversationId,
    origin: "agent",
    state: "pending_approval",
    programLabel: "node",
    argv: ["--version", "--pillow-task", "fix the module"],
    portableCwd: ".",
    riskClass: "standard",
    summary: "Role dispatch to worker",
    expiresAt: "2026-08-12T12:00:00.000Z",
    decisionAvailable: true,
    ruleSubject: "node --version --pillow-task fix the module",
    ...overrides,
  };
}

/** Exact host RoleDispatchProposalDto (camelCase). */
function hostProposalDto(overrides: Record<string, unknown> = {}) {
  return {
    assignmentId,
    initiatorSlotId,
    targetSlotId,
    targetRole: "custom",
    targetRoleCustomName: "写码",
    governance: "full",
    taskText: "fix the module",
    program: "node",
    argv: ["--version", "--pillow-task", "fix the module"],
    portableCwd: ".",
    callId,
    runId,
    proposal: hostExecProposal(),
    ...overrides,
  };
}

function hostRecordDto(overrides: Record<string, unknown> = {}) {
  return {
    assignmentId,
    initiatorSlotId,
    targetSlotId,
    governance: "none",
    proposalId,
    callId,
    claimedGovernedExecution: false,
    exitCode: 0,
    durationMs: 42,
    ...overrides,
  };
}

describe("role-dispatch closed-key allowlist (accept + reject)", () => {
  it("accepts the exact host propose DTO including nested process proposal", () => {
    const view = normalizeRoleDispatchProposal(hostProposalDto());
    expect(view).toMatchObject({
      assignmentId,
      targetSlotId,
      governance: "full",
      taskText: "fix the module",
      proposal: {
        proposalId,
        state: "pending_approval",
        decisionAvailable: true,
        programLabel: "node",
      },
    });
    // Nested proposal is the same shape the CommandApprovalBar consumes.
    expect(view.proposal.state).toBe("pending_approval");
  });

  it("accepts LocalCli / ungoverned propose DTO (governance none)", () => {
    const view = normalizeRoleDispatchProposal(
      hostProposalDto({ governance: "none" }),
    );
    expect(view.governance).toBe("none");
    expect(view.proposal.state).toBe("pending_approval");
  });

  it("rejects unknown keys on the propose DTO (smuggle / widen fail-closed)", () => {
    expect(() =>
      normalizeRoleDispatchProposal({
        ...hostProposalDto(),
        approvalGrant: "raw-grant-must-not-pass",
      }),
    ).toThrow();
    expect(() =>
      normalizeRoleDispatchProposal({
        ...hostProposalDto(),
        nativePath: "C:/secret",
      }),
    ).toThrow();
    expect(() =>
      normalizeRoleDispatchProposal({
        ...hostProposalDto(),
        unexpected: true,
      }),
    ).toThrow();
  });

  it("rejects missing required fields and bad governance enum", () => {
    const { taskText: _t, ...missingTask } = hostProposalDto();
    expect(() => normalizeRoleDispatchProposal(missingTask)).toThrow();
    expect(() =>
      normalizeRoleDispatchProposal(hostProposalDto({ governance: "partial" })),
    ).toThrow();
    expect(() =>
      normalizeRoleDispatchProposal(hostProposalDto({ governance: "elevated" })),
    ).toThrow();
  });

  it("rejects propose payloads that are not pending_approval (no false executed)", () => {
    for (const state of ["approved", "running", "succeeded", "failed"]) {
      expect(() =>
        normalizeRoleDispatchProposal(
          hostProposalDto({
            proposal: hostExecProposal({ state }),
          }),
        ),
      ).toThrow();
    }
  });

  it("rejects nested process proposal with unknown keys or missing decisionAvailable", () => {
    expect(() =>
      normalizeRoleDispatchProposal(
        hostProposalDto({
          proposal: { ...hostExecProposal(), nativeRoot: "F:\\secret" },
        }),
      ),
    ).toThrow();
    const { decisionAvailable: _d, ...noDecision } = hostExecProposal();
    expect(() =>
      normalizeRoleDispatchProposal(
        hostProposalDto({ proposal: noDecision }),
      ),
    ).toThrow();
  });

  it("accepts exact host list record DTO (ungoverned with exit code)", () => {
    const view = normalizeRoleDispatchRecord(hostRecordDto());
    expect(view).toMatchObject({
      governance: "none",
      claimedGovernedExecution: false,
      exitCode: 0,
      durationMs: 42,
    });
  });

  it("accepts governed record that claims governed execution", () => {
    const view = normalizeRoleDispatchRecord(
      hostRecordDto({
        governance: "full",
        claimedGovernedExecution: true,
        exitCode: 0,
      }),
    );
    expect(view.governance).toBe("full");
    expect(view.claimedGovernedExecution).toBe(true);
  });

  it("rejects ungoverned record that claims governed execution (elevation)", () => {
    expect(() =>
      normalizeRoleDispatchRecord(
        hostRecordDto({
          governance: "none",
          claimedGovernedExecution: true,
        }),
      ),
    ).toThrow();
  });

  it("rejects unknown keys on record DTO", () => {
    expect(() =>
      normalizeRoleDispatchRecord({
        ...hostRecordDto(),
        secretToken: "x",
      }),
    ).toThrow();
  });

  it("exports closed key lists matching host DTO field sets", () => {
    expect([...ROLE_DISPATCH_PROPOSAL_KEYS].sort()).toEqual(
      [
        "assignmentId",
        "argv",
        "callId",
        "governance",
        "initiatorSlotId",
        "portableCwd",
        "program",
        "proposal",
        "runId",
        "targetRole",
        "targetRoleCustomName",
        "targetSlotId",
        "taskText",
      ].sort(),
    );
    expect([...ROLE_DISPATCH_RECORD_KEYS].sort()).toEqual(
      [
        "assignmentId",
        "callId",
        "claimedGovernedExecution",
        "durationMs",
        "exitCode",
        "governance",
        "initiatorSlotId",
        "proposalId",
        "targetSlotId",
      ].sort(),
    );
  });
});

describe("role_dispatch_* error codes — distinct human copy", () => {
  it("maps every public code to a unique non-empty message (not a single collapse)", () => {
    const messages = ROLE_DISPATCH_PUBLIC_ERRORS.map(
      (code) => ROLE_DISPATCH_ERROR_MESSAGES[code],
    );
    for (const message of messages) {
      expect(message.length).toBeGreaterThan(8);
      expect(message).not.toBe("派单失败");
    }
    // Distinct: self-dispatch and target-missing must not share copy.
    expect(ROLE_DISPATCH_ERROR_MESSAGES.role_dispatch_self_dispatch).not.toBe(
      ROLE_DISPATCH_ERROR_MESSAGES.role_dispatch_target_missing,
    );
    expect(ROLE_DISPATCH_ERROR_MESSAGES.role_dispatch_task_empty).not.toBe(
      ROLE_DISPATCH_ERROR_MESSAGES.role_dispatch_unsupported_auth,
    );
    // Unique set (minus intentional fallback availability pair if any).
    const unique = new Set(messages);
    expect(unique.size).toBe(messages.length);
  });

  it.each([
    [
      "role_dispatch_self_dispatch",
      "不能把任务派给总控自己",
    ],
    [
      "role_dispatch_target_missing",
      "找不到目标槽位",
    ],
    [
      "role_dispatch_role_mismatch",
      "角色与请求的角色不一致",
    ],
    [
      "role_dispatch_shell_join_forbidden",
      "shell 命令字符串",
    ],
    [
      "role_dispatch_task_empty",
      "任务描述不能为空",
    ],
    [
      "role_dispatch_unsupported_auth",
      "认证方式尚不支持",
    ],
  ] as const)(
    "normalizeRoleDispatchError(%s) keeps distinct action guidance",
    (code, fragment) => {
      const err = normalizeRoleDispatchError({
        code,
        message: "host secret path=C:\\Users\\token=abc",
      });
      expect(err.code).toBe(code);
      expect(err.message).toContain(fragment);
      // Never echo host message / paths / tokens.
      expect(err.message).not.toContain("token=");
      expect(err.message).not.toContain("C:\\Users");
      expect(err.message).toBe(ROLE_DISPATCH_ERROR_MESSAGES[code]);
    },
  );

  it("covers remaining host codes with fixed copy helpers", () => {
    const rest: RoleDispatchPublicError[] = [
      "role_dispatch_task_in_program_or_cwd",
      "role_dispatch_program_empty",
      "role_dispatch_local_cli_executable_empty",
      "role_dispatch_empty_argv_element",
      "role_dispatch_process_unavailable",
      "role_dispatch_ledger_unavailable",
      "role_dispatch_unavailable",
    ];
    for (const code of rest) {
      expect(roleDispatchErrorMessage(code)).toBe(
        ROLE_DISPATCH_ERROR_MESSAGES[code],
      );
      expect(normalizeRoleDispatchError({ code }).message).toBe(
        ROLE_DISPATCH_ERROR_MESSAGES[code],
      );
    }
  });

  it("unknown codes collapse to unavailable without host text", () => {
    const err = normalizeRoleDispatchError({
      code: "something_else",
      message: "secret=xyz",
    });
    expect(err.code).toBe("role_dispatch_unavailable");
    expect(err.message).not.toContain("secret=");
  });
});

describe("TauriRoleDispatchGateway invoke contract", () => {
  beforeEach(() => {
    vi.mocked(invoke).mockReset();
  });

  it("proposes with exact camelCase keys and parses accept payload", async () => {
    vi.mocked(invoke).mockResolvedValueOnce(hostProposalDto());
    const gateway = new TauriRoleDispatchGateway();
    const view = await gateway.proposeRoleDispatch({
      conversationId,
      initiatorSlotId,
      targetSlotId,
      taskText: "fix the module",
    });
    expect(invoke).toHaveBeenCalledWith("propose_role_dispatch", {
      conversationId,
      initiatorSlotId,
      targetSlotId,
      taskText: "fix the module",
    });
    expect(view.proposal.state).toBe("pending_approval");
    expect(view.proposal.decisionAvailable).toBe(true);
  });

  it("lists records and rejects elevation smuggle on wire", async () => {
    vi.mocked(invoke).mockResolvedValueOnce([
      hostRecordDto({ governance: "none", claimedGovernedExecution: false }),
    ]);
    const gateway = new TauriRoleDispatchGateway();
    const list = await gateway.listRoleDispatches(conversationId);
    expect(invoke).toHaveBeenCalledWith("list_role_dispatches", {
      conversationId,
    });
    expect(list).toHaveLength(1);
    expect(list[0]?.governance).toBe("none");

    vi.mocked(invoke).mockResolvedValueOnce([
      hostRecordDto({ governance: "none", claimedGovernedExecution: true }),
    ]);
    await expect(gateway.listRoleDispatches(conversationId)).rejects.toMatchObject({
      code: "role_dispatch_unavailable",
    });
  });

  it("maps host role_dispatch_self_dispatch without swallowing", async () => {
    vi.mocked(invoke).mockRejectedValueOnce({
      code: "role_dispatch_self_dispatch",
      message: "internal host detail with path",
    });
    const gateway = new TauriRoleDispatchGateway();
    await expect(
      gateway.proposeRoleDispatch({
        conversationId,
        initiatorSlotId,
        targetSlotId: initiatorSlotId,
        taskText: "x",
      }),
    ).rejects.toMatchObject({
      code: "role_dispatch_self_dispatch",
      message: ROLE_DISPATCH_ERROR_MESSAGES.role_dispatch_self_dispatch,
    });
  });
});
