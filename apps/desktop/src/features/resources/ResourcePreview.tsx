/**
 * Standalone read-only resource preview (Wave C2-C3).
 * Controlled view over a typed host DTO — no FS, object URLs, network, or HTML.
 */

import type { ResourcePreview as ResourcePreviewDto } from "../../lib/resourceGateway";
import {
  isAllowlistedImagePreview,
  parseMarkdownPreview,
  type MarkdownBlock,
} from "./resourcePreviewModel";
import "../../styles/resource-preview.css";

export type ResourcePreviewProps = {
  preview: ResourcePreviewDto;
};

function MarkdownBlocks({ blocks }: { blocks: MarkdownBlock[] }) {
  return (
    <div className="resource-preview__markdown">
      {blocks.map((block, index) => {
        const key = `${block.type}-${index}`;
        switch (block.type) {
          case "heading": {
            const Tag = `h${block.level}` as "h1" | "h2" | "h3" | "h4" | "h5" | "h6";
            return (
              <Tag key={key} className="resource-preview__heading">
                {block.text}
              </Tag>
            );
          }
          case "paragraph":
            return (
              <p key={key} className="resource-preview__paragraph">
                {block.text}
              </p>
            );
          case "unordered-list-item":
            return (
              <div key={key} className="resource-preview__list-item resource-preview__list-item--ul">
                <span className="resource-preview__list-marker" aria-hidden="true">
                  •
                </span>
                <span>{block.text}</span>
              </div>
            );
          case "ordered-list-item":
            return (
              <div key={key} className="resource-preview__list-item resource-preview__list-item--ol">
                <span className="resource-preview__list-marker" aria-hidden="true">
                  {block.index}.
                </span>
                <span>{block.text}</span>
              </div>
            );
          case "code-block":
            return (
              <pre
                key={key}
                className="resource-preview__code"
                data-language={block.language || undefined}
              >
                <code>{block.code}</code>
              </pre>
            );
          default: {
            const _exhaustive: never = block;
            return _exhaustive;
          }
        }
      })}
    </div>
  );
}

function BinaryMetadata({ preview }: { preview: ResourcePreviewDto }) {
  return (
    <dl className="resource-preview__meta">
      <div>
        <dt>Media type</dt>
        <dd>{preview.mediaType}</dd>
      </div>
      <div>
        <dt>Kind</dt>
        <dd>binary</dd>
      </div>
      <div>
        <dt>Bytes returned</dt>
        <dd>{preview.bytesReturned}</dd>
      </div>
      <div>
        <dt>Truncated</dt>
        <dd>{preview.truncated ? "yes" : "no"}</dd>
      </div>
      <div>
        <dt>Status</dt>
        <dd>metadata only</dd>
      </div>
    </dl>
  );
}

/**
 * Controlled preview surface. Renders text, a small Markdown model, allow-listed
 * images, or binary metadata. Never uses dangerouslySetInnerHTML.
 */
export function ResourcePreview({ preview }: ResourcePreviewProps) {
  const kindClass = `resource-preview--${preview.kind}`;

  if (preview.kind === "text") {
    return (
      <div
        className={`resource-preview ${kindClass}`}
        data-testid="resource-preview"
        data-kind="text"
      >
        <div className="resource-preview__path">{preview.path}</div>
        <pre className="resource-preview__text">{preview.content ?? ""}</pre>
      </div>
    );
  }

  if (preview.kind === "markdown") {
    const blocks = parseMarkdownPreview(preview.content ?? "");
    return (
      <div
        className={`resource-preview ${kindClass}`}
        data-testid="resource-preview"
        data-kind="markdown"
      >
        <div className="resource-preview__path">{preview.path}</div>
        <MarkdownBlocks blocks={blocks} />
      </div>
    );
  }

  if (preview.kind === "image" && isAllowlistedImagePreview(preview)) {
    return (
      <div
        className={`resource-preview ${kindClass}`}
        data-testid="resource-preview"
        data-kind="image"
      >
        <div className="resource-preview__path">{preview.path}</div>
        <img
          className="resource-preview__image"
          src={preview.dataUrl}
          alt={preview.path}
        />
      </div>
    );
  }

  // binary, or image that failed allow-list agreement
  return (
    <div
      className={`resource-preview resource-preview--binary`}
      data-testid="resource-preview"
      data-kind="binary"
    >
      <div className="resource-preview__path">{preview.path}</div>
      <BinaryMetadata
        preview={{
          ...preview,
          kind: "binary",
          mediaType:
            preview.kind === "binary"
              ? preview.mediaType
              : preview.mediaType || "application/octet-stream",
        }}
      />
    </div>
  );
}
