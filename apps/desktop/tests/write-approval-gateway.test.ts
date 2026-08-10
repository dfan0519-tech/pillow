/**
 * Wave C3-E — write approval gateway invoke contract and pure helpers.
 */
import { invoke } from "@tauri-apps/api/core";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { TauriWriteApprovalGateway } from "../src/lib/tauriWriteApprovalGateway";
import {
  WriteApprovalGatewayError,
  isCanonicalApprovalGrant,
  normalizeWriteApprovalError,
  selectOldestPendingProposal,
  stripApprovalGrant,
  writeTerminalMessage,
  type WriteProposalView,
} from "../src/lib/writeApprovalGateway";

vi.mock("@tauri-apps/api/core", () => ({
  invoke: vi.fn(),
}));

const conversationId = "00000000-0000-0000-0000-0000000000c1";
const otherConversationId = "00000000-0000-0000-0000-0000000000c2";
const proposalId = "00000000-0000-0000-0000-0000000000p1";
/** Canonical 43-char base64url grant (32 OS-random bytes, no padding). */
const grant = "abcdefghijklmnopqrstuvwxyz0123456789ABCDEFG";

function pendingProposal(
  overrides: Partial<WriteProposalView> = {},
): WriteProposalView {
  return {
    proposalId,
    conversationId,
    path: "src/example.ts",
    mode: "create",
    state: "pending_approval",
    summary: "Create src/example.ts",
    expiresAt: "2026-08-01T12:10:00.000Z",
    approvalGrant: grant,
    ruleSubject: "src/example.ts",
    ...overrides,
  };
}

describe("TauriWriteApprovalGateway contract", () => {
  beforeEach(() => {
    vi.mocked(invoke).mockReset();
  });

  it("invokes list/decide/diff with exact camelCase keys only", async () => {
    vi.mocked(invoke)
      .mockResolvedValueOnce([pendingProposal()])
      .mockResolvedValueOnce({
        ...pendingProposal({ state: "approved", approvalGrant: undefined }),
      })
      .mockResolvedValueOnce({
        proposalId,
        path: "src/example.ts",
        mode: "create",
        unified: "--- /dev/null\n+++ b/src/example.ts\n+export {}\n",
        truncated: false,
        lineCount: 3,
        byteCount: 46,
      });

    const gateway = new TauriWriteApprovalGateway();
    await gateway.listWriteProposals(conversationId);
    await gateway.decideWriteProposal({
      conversationId,
      proposalId,
      approvalGrant: grant,
      decision: "approve_once",
    });
    await gateway.openWriteProposalDiff({ conversationId, proposalId });

    expect(invoke).toHaveBeenNthCalledWith(1, "list_write_proposals", {
      conversationId,
    });
    expect(invoke).toHaveBeenNthCalledWith(2, "decide_write_proposal", {
      conversationId,
      proposalId,
      approvalGrant: grant,
      decision: "approve_once",
    });
    expect(invoke).toHaveBeenNthCalledWith(3, "open_write_proposal_diff", {
      conversationId,
      proposalId,
    });

    for (const call of vi.mocked(invoke).mock.calls) {
      const args = call[1] as Record<string, unknown>;
      expect(args).not.toHaveProperty("root");
      expect(args).not.toHaveProperty("content");
      expect(args).not.toHaveProperty("base");
      expect(args).not.toHaveProperty("digest");
      expect(args).not.toHaveProperty("fingerprint");
      expect(args).not.toHaveProperty("proposedContent");
      expect(args).not.toHaveProperty("baseContent");
    }
  });

  it("rejects payloads that smuggle root/content/digest fields", async () => {
    vi.mocked(invoke).mockResolvedValueOnce([
      {
        ...pendingProposal(),
        root: "F:\\secret\\project",
      },
    ]);
    const gateway = new TauriWriteApprovalGateway();
    await expect(gateway.listWriteProposals(conversationId)).rejects.toMatchObject({
      code: "invalid_write_response",
    });
  });

  it("maps known host error codes to fixed renderer copy and never accepts host message text", () => {
    const secretMessage =
      "grant=abcdefghijklmnopqrstuvwxyz0123456789ABCDEFG token=super-secret-no-slash";
    const error = normalizeWriteApprovalError({
      code: "write_invalid_grant",
      message: secretMessage,
    });
    expect(error).toBeInstanceOf(WriteApprovalGatewayError);
    expect(error.code).toBe("write_invalid_grant");
    expect(error.message).toBe("Approval grant was rejected.");
    expect(error.message).not.toContain(grant);
    expect(error.message).not.toContain("super-secret");

    const pathLeaking = normalizeWriteApprovalError({
      code: "write_unavailable",
      message: "failed at F:\\secret\\project\\src\\example.ts",
    });
    expect(pathLeaking.code).toBe("write_unavailable");
    expect(pathLeaking.message).toBe("Write proposal is unavailable.");
    expect(pathLeaking.message).not.toMatch(/[\\/]/);

    const unknown = normalizeWriteApprovalError({
      code: "some_unknown_code",
      message: secretMessage,
    });
    expect(unknown.code).toBe("write_unavailable");
    expect(unknown.message).toBe("Write proposal is unavailable.");
    expect(unknown.message).not.toContain(grant);
  });

  it("rewrites existing WriteApprovalGatewayError instances to fixed copy and never echoes their message", () => {
    const secretMessage =
      `grant=${grant} token=instance-secret-token-no-slash`;
    const knownInstance = new WriteApprovalGatewayError(
      "write_invalid_grant",
      secretMessage,
    );
    const normalizedKnown = normalizeWriteApprovalError(knownInstance);
    expect(normalizedKnown).toBeInstanceOf(WriteApprovalGatewayError);
    expect(normalizedKnown.code).toBe("write_invalid_grant");
    expect(normalizedKnown.message).toBe("Approval grant was rejected.");
    expect(normalizedKnown.message).not.toContain(grant);
    expect(normalizedKnown.message).not.toContain("instance-secret-token-no-slash");
    expect(normalizedKnown).not.toBe(knownInstance);

    const invalidInstance = new WriteApprovalGatewayError(
      "invalid_write_response",
      secretMessage,
    );
    const normalizedInvalid = normalizeWriteApprovalError(invalidInstance);
    expect(normalizedInvalid.code).toBe("invalid_write_response");
    expect(normalizedInvalid.message).toBe(
      "Pillow received an invalid write approval response.",
    );
    expect(normalizedInvalid.message).not.toContain(grant);
    expect(normalizedInvalid.message).not.toContain("instance-secret");
  });

  it("accepts camelCase proposal and diff DTOs within closed bounds", async () => {
    const unified = "@@ -1 +1 @@\n-old\n+new\n";
    const byteCount = new TextEncoder().encode(unified).length;
    // Executor count_lines: empty→0; newline terminators; no trailing NL adds one logical line.
    const lineCount = 3;
    vi.mocked(invoke)
      .mockResolvedValueOnce([pendingProposal({ mode: "modify", path: "lib/mod.rs" })])
      .mockResolvedValueOnce({
        proposalId,
        path: "lib/mod.rs",
        mode: "modify",
        unified,
        truncated: true,
        lineCount,
        byteCount,
      });
    const gateway = new TauriWriteApprovalGateway();
    const list = await gateway.listWriteProposals(conversationId);
    expect(list[0]?.path).toBe("lib/mod.rs");
    expect(list[0]?.mode).toBe("modify");
    const diff = await gateway.openWriteProposalDiff({ conversationId, proposalId });
    expect(diff.truncated).toBe(true);
    expect(diff.lineCount).toBe(lineCount);
    expect(diff.byteCount).toBe(byteCount);
  });

  it("rejects diffs whose byteCount or lineCount do not describe the reviewed text", async () => {
    const gateway = new TauriWriteApprovalGateway();
    const utf8 = new TextEncoder();

    // Empty text: executor count_lines → 0, bytes → 0.
    vi.mocked(invoke).mockResolvedValueOnce({
      proposalId,
      path: "src/example.ts",
      mode: "create",
      unified: "",
      truncated: false,
      lineCount: 1,
      byteCount: 0,
    });
    await expect(
      gateway.openWriteProposalDiff({ conversationId, proposalId }),
    ).rejects.toMatchObject({ code: "invalid_write_response" });

    // Final newline: "a\n" → 1 line, 2 bytes; claim 2 lines must fail.
    vi.mocked(invoke).mockResolvedValueOnce({
      proposalId,
      path: "src/example.ts",
      mode: "create",
      unified: "a\n",
      truncated: false,
      lineCount: 2,
      byteCount: 2,
    });
    await expect(
      gateway.openWriteProposalDiff({ conversationId, proposalId }),
    ).rejects.toMatchObject({ code: "invalid_write_response" });

    // Unicode: multi-byte chars must match TextEncoder length, not JS string length.
    const unicode = "héllo\n世界";
    const unicodeBytes = utf8.encode(unicode).length;
    expect(unicodeBytes).not.toBe(unicode.length);
    vi.mocked(invoke).mockResolvedValueOnce({
      proposalId,
      path: "src/example.ts",
      mode: "create",
      unified: unicode,
      truncated: false,
      lineCount: 2,
      byteCount: unicode.length, // wrong: char length, not UTF-8
    });
    await expect(
      gateway.openWriteProposalDiff({ conversationId, proposalId }),
    ).rejects.toMatchObject({ code: "invalid_write_response" });

    // Matching empty / final-newline / Unicode cases must accept.
    vi.mocked(invoke).mockResolvedValueOnce({
      proposalId,
      path: "src/example.ts",
      mode: "create",
      unified: "",
      truncated: false,
      lineCount: 0,
      byteCount: 0,
    });
    await expect(
      gateway.openWriteProposalDiff({ conversationId, proposalId }),
    ).resolves.toMatchObject({ unified: "", lineCount: 0, byteCount: 0 });

    vi.mocked(invoke).mockResolvedValueOnce({
      proposalId,
      path: "src/example.ts",
      mode: "create",
      unified: "a\n",
      truncated: false,
      lineCount: 1,
      byteCount: 2,
    });
    await expect(
      gateway.openWriteProposalDiff({ conversationId, proposalId }),
    ).resolves.toMatchObject({ unified: "a\n", lineCount: 1, byteCount: 2 });

    vi.mocked(invoke).mockResolvedValueOnce({
      proposalId,
      path: "src/example.ts",
      mode: "create",
      unified: unicode,
      truncated: false,
      lineCount: 2,
      byteCount: unicodeBytes,
    });
    await expect(
      gateway.openWriteProposalDiff({ conversationId, proposalId }),
    ).resolves.toMatchObject({
      unified: unicode,
      lineCount: 2,
      byteCount: unicodeBytes,
    });
  });

  it("rejects malformed grants and grants on terminal states", async () => {
    const gateway = new TauriWriteApprovalGateway();

    vi.mocked(invoke).mockResolvedValueOnce([
      pendingProposal({ approvalGrant: "too-short" }),
    ]);
    await expect(gateway.listWriteProposals(conversationId)).rejects.toMatchObject({
      code: "invalid_write_response",
    });

    vi.mocked(invoke).mockResolvedValueOnce([
      pendingProposal({ approvalGrant: "not!valid+base64url/chars=========" }),
    ]);
    await expect(gateway.listWriteProposals(conversationId)).rejects.toMatchObject({
      code: "invalid_write_response",
    });

    vi.mocked(invoke).mockResolvedValueOnce([
      {
        ...pendingProposal({
          state: "succeeded",
          approvalGrant: grant,
        }),
      },
    ]);
    await expect(gateway.listWriteProposals(conversationId)).rejects.toMatchObject({
      code: "invalid_write_response",
    });

    vi.mocked(invoke).mockResolvedValueOnce([
      {
        ...pendingProposal(),
        approvalGrant: undefined,
      },
    ]);
    await expect(gateway.listWriteProposals(conversationId)).rejects.toMatchObject({
      code: "invalid_write_response",
    });
  });

  it("rejects unknown error codes and invalid expiresAt", async () => {
    const gateway = new TauriWriteApprovalGateway();

    vi.mocked(invoke).mockResolvedValueOnce([
      pendingProposal({ errorCode: "not_a_public_code" as never }),
    ]);
    await expect(gateway.listWriteProposals(conversationId)).rejects.toMatchObject({
      code: "invalid_write_response",
    });

    vi.mocked(invoke).mockResolvedValueOnce([
      pendingProposal({ expiresAt: "not-a-timestamp" }),
    ]);
    await expect(gateway.listWriteProposals(conversationId)).rejects.toMatchObject({
      code: "invalid_write_response",
    });

    vi.mocked(invoke).mockResolvedValueOnce([
      pendingProposal({ errorCode: "approval_rejected" }),
    ]);
    await expect(gateway.listWriteProposals(conversationId)).resolves.toMatchObject([
      { errorCode: "approval_rejected" },
    ]);
  });

  it("rejects oversized UTF-8 diffs and non-portable paths", async () => {
    const gateway = new TauriWriteApprovalGateway();

    // Oversized UTF-8: multi-byte chars make byte length > char length.
    const oversized = " ag".repeat(90_000); // 3 UTF-8 bytes each → > 262144
    expect(new TextEncoder().encode(oversized).length).toBeGreaterThan(262_144);
    vi.mocked(invoke).mockResolvedValueOnce({
      proposalId,
      path: "src/example.ts",
      mode: "create",
      unified: oversized,
      truncated: true,
      lineCount: 4000,
      byteCount: 262144,
    });
    await expect(
      gateway.openWriteProposalDiff({ conversationId, proposalId }),
    ).rejects.toMatchObject({ code: "invalid_write_response" });

    vi.mocked(invoke).mockResolvedValueOnce({
      proposalId,
      path: "src/example.ts",
      mode: "create",
      unified: "ok\n",
      truncated: false,
      lineCount: 4001,
      byteCount: 3,
    });
    await expect(
      gateway.openWriteProposalDiff({ conversationId, proposalId }),
    ).rejects.toMatchObject({ code: "invalid_write_response" });

    vi.mocked(invoke).mockResolvedValueOnce([
      pendingProposal({ path: "F:\\secret\\project\\src\\file.ts" }),
    ]);
    await expect(gateway.listWriteProposals(conversationId)).rejects.toMatchObject({
      code: "invalid_write_response",
    });

    vi.mocked(invoke).mockResolvedValueOnce([
      pendingProposal({ path: "../escape.ts" }),
    ]);
    await expect(gateway.listWriteProposals(conversationId)).rejects.toMatchObject({
      code: "invalid_write_response",
    });
  });
});

describe("write approval pure helpers", () => {
  it("recognizes only canonical 43-char base64url grants", () => {
    expect(isCanonicalApprovalGrant(grant)).toBe(true);
    expect(isCanonicalApprovalGrant("short")).toBe(false);
    expect(isCanonicalApprovalGrant(`${grant}x`)).toBe(false);
    expect(isCanonicalApprovalGrant("abcdefghijklmnopqrstuvwxyz0123456789ABCDE+")).toBe(
      false,
    );
  });

  it("selects oldest pending by expiresAt then proposalId and filters foreign", () => {
    const list: WriteProposalView[] = [
      pendingProposal({
        proposalId: "p-b",
        expiresAt: "2026-08-01T12:20:00.000Z",
      }),
      pendingProposal({
        proposalId: "p-a",
        conversationId: otherConversationId,
        expiresAt: "2026-08-01T12:00:00.000Z",
      }),
      pendingProposal({
        proposalId: "p-c",
        expiresAt: "2026-08-01T12:10:00.000Z",
      }),
      pendingProposal({
        proposalId: "p-d",
        expiresAt: "2026-08-01T12:10:00.000Z",
      }),
      pendingProposal({
        proposalId: "p-done",
        state: "succeeded",
        expiresAt: "2026-08-01T11:00:00.000Z",
        approvalGrant: undefined,
      }),
    ];

    const selected = selectOldestPendingProposal(list, conversationId);
    expect(selected?.proposalId).toBe("p-c");
    expect(selected?.conversationId).toBe(conversationId);
  });

  it("strips approvalGrant without mutating other fields", () => {
    const original = pendingProposal();
    const stripped = stripApprovalGrant(original);
    expect(stripped.approvalGrant).toBeUndefined();
    expect(stripped.proposalId).toBe(original.proposalId);
    expect(original.approvalGrant).toBe(grant);
  });

  it("uses fixed path-free terminal messages", () => {
    expect(writeTerminalMessage("succeeded")).toBe("文件写入已完成");
    expect(writeTerminalMessage("rejected")).toBe("写入请求已拒绝");
    expect(writeTerminalMessage("failed")).not.toMatch(/[\\/]/);
    expect(writeTerminalMessage("expired")).not.toMatch(/example/);
  });
});
