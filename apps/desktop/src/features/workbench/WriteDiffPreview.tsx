/**
 * Host-generated governed-write diff preview for the contextual resource pane.
 * Source-bearing: do not log unified text or snapshot beyond focused fixtures.
 */

import type { WriteDiffView } from "../../lib/writeApprovalGateway";
import "../../styles/write-approval.css";

export type WriteDiffPreviewProps = {
  diff: WriteDiffView;
};

function modeLabel(mode: WriteDiffView["mode"]): string {
  return mode === "create" ? "创建" : "修改";
}

export function WriteDiffPreview({ diff }: WriteDiffPreviewProps) {
  return (
    <section
      aria-label="写入差异"
      className="write-diff-preview"
      data-testid="write-diff-preview"
    >
      <header className="write-diff-preview__header">
        <h3 className="write-diff-preview__title">写入差异</h3>
        <p className="write-diff-preview__path" data-testid="write-diff-path">
          <code>{diff.path}</code>
        </p>
        <p className="write-diff-preview__meta">
          <span data-testid="write-diff-mode">{modeLabel(diff.mode)}</span>
          <span aria-hidden="true" className="write-diff-preview__sep">
            ·
          </span>
          <span data-testid="write-diff-line-count">{diff.lineCount} 行</span>
          <span aria-hidden="true" className="write-diff-preview__sep">
            ·
          </span>
          <span data-testid="write-diff-byte-count">{diff.byteCount} 字节</span>
        </p>
        {diff.contentDigest ? (
          <p
            className="write-diff-preview__digest"
            data-testid="write-diff-content-digest"
          >
            内容摘要 {diff.contentDigest}
          </p>
        ) : null}
        {diff.truncated ? (
          <p
            className="write-diff-preview__truncation"
            data-testid="write-diff-truncated"
            role="status"
          >
            差异已截断，须在批准栏审查完整拟写入内容后才能批准
          </p>
        ) : null}
      </header>
      <pre
        className="write-diff-preview__body"
        data-testid="write-diff-unified"
        tabIndex={0}
      >
        {diff.unified}
      </pre>
    </section>
  );
}
