import { describe, expect, it, vi } from "vitest";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));

import {
  normalizeExecProposal,
  normalizeExecOutput,
} from "../src/lib/tauriExecGateway";
import { isTerminalKind, type RuntimeEventDto } from "../src/lib/conversationGateway";
import {
  normalizeGitDiscovery,
  normalizeGitDiff,
  normalizeGitStatus,
} from "../src/lib/tauriGitGateway";

const proposal = {
  proposalId: "proposal-1",
  conversationId: "conversation-1",
  origin: "agent",
  state: "pending_approval",
  programLabel: "cargo",
  argv: ["test"],
  portableCwd: ".",
  riskClass: "standard",
  summary: "Run tests",
  expiresAt: "2026-08-04T00:00:00.000Z",
};

const output = {
  proposalId: "proposal-1",
  state: "succeeded",
  chunks: [],
  liveRetained: false,
  truncated: false,
  encodingLossy: false,
  capturedBytes: 0,
  exitCode: 0,
  durationMs: 1,
};

describe("C4 gateway contract", () => {
  it("rejects unknown, authority, and invalid enum fields before exec renderer state", () => {
    expect(() => normalizeExecProposal({ ...proposal, state: "spawned" })).toThrow();
    expect(() => normalizeExecProposal({ ...proposal, nativePath: "C:/secret" })).toThrow();
    expect(() => normalizeExecProposal({ ...proposal, approvalGrant: "raw-grant" })).toThrow();
    expect(() => normalizeExecProposal({ ...proposal, runId: "internal-run" })).toThrow();
    expect(() => normalizeExecProposal({ ...proposal, unexpected: true })).toThrow();
  });

  it("rejects native paths and oversized terminal output", () => {
    expect(() => normalizeExecProposal({ ...proposal, portableCwd: "C:/native" })).toThrow();
    expect(() =>
      normalizeExecOutput({
        ...output,
        chunks: [{ stream: "stdout", sequence: 0, text: "x".repeat(262_145) }],
        capturedBytes: 262_145,
      }),
    ).toThrow();
  });

  it("preserves host-owned live-retention presentation metadata", () => {
    expect(normalizeExecOutput({ ...output, liveRetained: true })).toMatchObject({
      liveRetained: true,
      chunks: [],
    });
    expect(normalizeExecOutput(output).liveRetained).toBe(false);
  });

  it("requires only the exact live-retained output metadata key", () => {
    const { liveRetained: _liveRetained, ...missingLiveRetained } = output;
    expect(() => normalizeExecOutput(missingLiveRetained)).toThrow();
    expect(() => normalizeExecOutput({ ...output, liveRetained: "true" })).toThrow();
    expect(() => normalizeExecOutput({ ...output, approvalGrant: "raw-grant" })).toThrow();
    expect(() => normalizeExecOutput({ ...output, debug: { retained: true } })).toThrow();
  });

  it("rejects Git native paths, invalid enums, foreign identity, and stale generations", () => {
    expect(() =>
      normalizeGitDiscovery({ repositories: [{ repositoryId: "C:/native", available: true }], generation: 3 }),
    ).toThrow();
    expect(() =>
      normalizeGitStatus(
        {
          repositoryId: "repo",
          generation: 3,
          branch: "main",
          detached: false,
          headShort: "abc1234",
          dirtyCount: 1,
          files: [{ path: "a.ts", kind: "mutated" }],
          truncated: false,
        },
        { repositoryId: "repo", generation: 4 },
      ),
    ).toThrow();
    expect(() =>
      normalizeGitDiff(
        {
          repositoryId: "other",
          generation: 4,
          path: "a.ts",
          kind: "text",
          unified: "diff --git a/a.ts b/a.ts\n",
          truncated: false,
          lineCount: 1,
          byteCount: 25,
        },
        { repositoryId: "repo", generation: 4 },
      ),
    ).toThrow();
  });

  it("keeps process terminal metadata non-terminal at the conversation-run boundary", () => {
    const event: RuntimeEventDto = {
      eventId: "event-process-terminal",
      runId: "run-private-id",
      conversationId: "conversation-owner",
      sequence: 4,
      createdAt: "2026-08-04T00:00:00.000Z",
      kind: "process_terminal_outcome",
      payload: {
        proposal_id: "process-private-id",
        program: "node",
        state: "succeeded",
        exit_code: 0,
        duration_ms: 12,
        captured_bytes: 8,
      },
    };

    expect(isTerminalKind(event.kind)).toBe(false);
    expect(JSON.stringify(event.payload)).not.toContain("output");
    expect(JSON.stringify(event.payload)).not.toContain("grant");
  });
});
