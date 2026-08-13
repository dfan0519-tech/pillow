/**
 * Wave C3-E — host-generated write diff preview surface.
 */
import { render, screen } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import { WriteDiffPreview } from "../src/features/workbench/WriteDiffPreview";
import type { WriteDiffView } from "../src/lib/writeApprovalGateway";
import "../src/styles/write-approval.css";

const proposalId = "prop-raw-diff-id-hidden";

function fixtureDiff(overrides: Partial<WriteDiffView> = {}): WriteDiffView {
  return {
    proposalId,
    path: "src/example.ts",
    mode: "modify",
    unified: "@@ -1,2 +1,2 @@\n-const a = 1;\n+const a = 2;\n",
    truncated: false,
    lineCount: 4,
    byteCount: 52,
    ...overrides,
  };
}

describe("WriteDiffPreview", () => {
  it("shows portable path, mode, counts and unified text", () => {
    render(<WriteDiffPreview diff={fixtureDiff()} />);
    expect(screen.getByTestId("write-diff-path")).toHaveTextContent(
      "src/example.ts",
    );
    expect(screen.getByTestId("write-diff-mode")).toHaveTextContent("修改");
    expect(screen.getByTestId("write-diff-line-count")).toHaveTextContent("4 行");
    expect(screen.getByTestId("write-diff-byte-count")).toHaveTextContent(
      "52 字节",
    );
    expect(screen.getByTestId("write-diff-unified")).toHaveTextContent(
      "const a = 2",
    );
    expect(screen.queryByTestId("write-diff-truncated")).toBeNull();
  });

  it("shows host truncation flag without raw proposal ids or native paths", () => {
    const { container } = render(
      <WriteDiffPreview
        diff={fixtureDiff({
          truncated: true,
          lineCount: 4000,
          byteCount: 262144,
          unified: "+line\n".repeat(20),
        })}
      />,
    );
    expect(screen.getByTestId("write-diff-truncated")).toHaveTextContent(
      "差异已截断",
    );
    expect(screen.queryByTestId("write-diff-content-digest")).toBeNull();
    const text = container.textContent ?? "";
    const html = container.innerHTML;
    expect(text).not.toContain(proposalId);
    expect(html).not.toContain(proposalId);
    expect(text).not.toMatch(/F:\\|C:\\|\\\\server/);
    expect(html).not.toMatch(/data-proposal|approvalGrant/i);
  });

  it("shows the proposed-content digest next to a truncated warning (EA-04)", () => {
    render(
      <WriteDiffPreview
        diff={fixtureDiff({
          truncated: true,
          contentDigest:
            "sha256:cccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccc",
          lineCount: 4,
          byteCount: 52,
        })}
      />,
    );
    expect(screen.getByTestId("write-diff-content-digest")).toHaveTextContent(
      "sha256:cccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccc",
    );
    expect(screen.getByTestId("write-diff-truncated")).toHaveTextContent(
      "完整拟写入内容",
    );
  });
});
