import { describe, expect, it } from "vitest";
import {
  selectVisibleApproval,
  type ApprovalCandidate,
  type ApprovalScope,
} from "../src/features/conversations/approvalArbitration";

const scope: ApprovalScope = {
  projectId: "project-a",
  conversationId: "conversation-a",
};

function candidate(
  overrides: Partial<ApprovalCandidate> = {},
): ApprovalCandidate {
  return {
    domain: "process",
    projectId: scope.projectId,
    requestId: "request-b",
    conversationId: scope.conversationId,
    expiresAt: "2026-08-04T12:10:00.000Z",
    decisionAvailable: true,
    summary: "Run the selected command",
    risk: "standard",
    ...overrides,
  };
}

describe("approval arbitration", () => {
  it("selects the oldest available request across writes and processes", () => {
    const selected = selectVisibleApproval(scope, [
      candidate({ domain: "write", requestId: "write-new", expiresAt: "2026-08-04T12:30:00.000Z" }),
      candidate({ domain: "process", requestId: "process-old", expiresAt: "2026-08-04T12:00:00.000Z" }),
      candidate({ domain: "write", requestId: "write-middle", expiresAt: "2026-08-04T12:20:00.000Z" }),
    ]);

    expect(selected).toMatchObject({ domain: "process", requestId: "process-old" });
  });

  it("uses the opaque request id to break equal-expiry ties", () => {
    const selected = selectVisibleApproval(scope, [
      candidate({ domain: "write", requestId: "request-z" }),
      candidate({ domain: "process", requestId: "request-a" }),
    ]);

    expect(selected).toMatchObject({ domain: "process", requestId: "request-a" });
  });

  it("chooses write when expiry and opaque request id are both equal", () => {
    const selected = selectVisibleApproval(scope, [
      candidate({ domain: "process", requestId: "same-request" }),
      candidate({ domain: "write", requestId: "same-request", risk: "write-review" }),
    ]);

    expect(selected).toEqual({
      domain: "write",
      requestId: "same-request",
      conversationId: scope.conversationId,
      expiresAt: "2026-08-04T12:10:00.000Z",
      summary: "Run the selected command",
      risk: "write-review",
    });
  });

  it("is project- and conversation-scoped", () => {
    const selected = selectVisibleApproval(scope, [
      candidate({ projectId: "project-b", requestId: "foreign-project", expiresAt: "2026-08-04T11:00:00.000Z" }),
      candidate({ conversationId: "conversation-b", requestId: "foreign-conversation", expiresAt: "2026-08-04T11:01:00.000Z" }),
    ]);

    expect(selected).toBeNull();
  });

  it("returns no selection for empty inputs or unavailable decisions", () => {
    expect(selectVisibleApproval(scope, [])).toBeNull();
    expect(
      selectVisibleApproval(scope, [
        candidate({ domain: "write", decisionAvailable: false }),
        candidate({ requestId: "process-unavailable", decisionAvailable: false }),
      ]),
    ).toBeNull();
  });

  it("fails closed when an expiry cannot be ordered", () => {
    expect(
      selectVisibleApproval(scope, [
        candidate({ requestId: "bad-expiry", expiresAt: "not-a-timestamp" }),
      ]),
    ).toBeNull();
  });

  it("does not mutate candidates and projects no authority or debug fields", () => {
    const requests = [
      candidate({ domain: "process", requestId: "later", expiresAt: "2026-08-04T12:20:00.000Z" }),
      candidate({ domain: "write", requestId: "earlier", expiresAt: "2026-08-04T12:00:00.000Z" }),
    ];
    const before = structuredClone(requests);

    const selected = selectVisibleApproval(scope, requests);

    expect(requests).toEqual(before);
    expect(selected).toEqual({
      domain: "write",
      requestId: "earlier",
      conversationId: scope.conversationId,
      expiresAt: "2026-08-04T12:00:00.000Z",
      summary: "Run the selected command",
      risk: "standard",
    });
    expect(Object.keys(selected ?? {})).toEqual([
      "domain",
      "requestId",
      "conversationId",
      "expiresAt",
      "summary",
      "risk",
    ]);
  });
});
