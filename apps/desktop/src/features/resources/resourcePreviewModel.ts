/**
 * Pure local preview helpers — no filesystem, network, or HTML interpretation.
 * Markdown is deliberately a small semantic subset for safe React rendering.
 */

import type { ResourcePreview } from "../../lib/resourceGateway";

export type MarkdownHeadingLevel = 1 | 2 | 3 | 4 | 5 | 6;

export type MarkdownBlock =
  | { type: "heading"; level: MarkdownHeadingLevel; text: string }
  | { type: "paragraph"; text: string }
  | { type: "unordered-list-item"; text: string }
  | { type: "ordered-list-item"; index: number; text: string }
  | { type: "code-block"; language: string; code: string };

export const ALLOWED_IMAGE_MEDIA_TYPES = [
  "image/png",
  "image/jpeg",
  "image/gif",
  "image/webp",
] as const;

export type AllowedImageMediaType = (typeof ALLOWED_IMAGE_MEDIA_TYPES)[number];

export function isAllowedImageMediaType(
  value: string,
): value is AllowedImageMediaType {
  return (ALLOWED_IMAGE_MEDIA_TYPES as readonly string[]).includes(value);
}

/**
 * Image previews render only when kind, mediaType, and dataUrl prefix all agree
 * with the four allow-listed MIME types.
 */
export function isAllowlistedImagePreview(preview: ResourcePreview): boolean {
  if (preview.kind !== "image") return false;
  if (!isAllowedImageMediaType(preview.mediaType)) return false;
  if (typeof preview.dataUrl !== "string" || preview.dataUrl.length === 0) {
    return false;
  }
  const prefix = `data:${preview.mediaType};base64,`;
  return preview.dataUrl.startsWith(prefix);
}

function headingLevel(marker: string): MarkdownHeadingLevel | null {
  const level = marker.length;
  if (level >= 1 && level <= 6) {
    return level as MarkdownHeadingLevel;
  }
  return null;
}

/**
 * Line-oriented Markdown subset: headings, paragraphs, unordered/ordered list
 * items, and fenced code blocks. Raw HTML remains plain text.
 */
export function parseMarkdownPreview(source: string): MarkdownBlock[] {
  const lines = source.replace(/\r\n/g, "\n").replace(/\r/g, "\n").split("\n");
  const blocks: MarkdownBlock[] = [];
  let i = 0;
  let paragraphParts: string[] = [];

  const flushParagraph = () => {
    if (paragraphParts.length === 0) return;
    const text = paragraphParts.join("\n").trimEnd();
    paragraphParts = [];
    if (text.length > 0) {
      blocks.push({ type: "paragraph", text });
    }
  };

  while (i < lines.length) {
    const line = lines[i] ?? "";

    // Fenced code block
    const fenceOpen = line.match(/^```([\w+-]*)\s*$/);
    if (fenceOpen) {
      flushParagraph();
      const language = fenceOpen[1] ?? "";
      i += 1;
      const codeLines: string[] = [];
      while (i < lines.length) {
        const codeLine = lines[i] ?? "";
        if (/^```\s*$/.test(codeLine)) {
          i += 1;
          break;
        }
        codeLines.push(codeLine);
        i += 1;
      }
      blocks.push({
        type: "code-block",
        language,
        code: codeLines.join("\n"),
      });
      continue;
    }

    // Blank line ends a paragraph
    if (line.trim() === "") {
      flushParagraph();
      i += 1;
      continue;
    }

    // Headings: # .. ######
    const headingMatch = line.match(/^(#{1,6})\s+(.+)$/);
    if (headingMatch) {
      flushParagraph();
      const level = headingLevel(headingMatch[1] ?? "");
      if (level) {
        blocks.push({
          type: "heading",
          level,
          text: (headingMatch[2] ?? "").trimEnd(),
        });
        i += 1;
        continue;
      }
    }

    // Unordered list
    const ulMatch = line.match(/^[-*]\s+(.+)$/);
    if (ulMatch) {
      flushParagraph();
      blocks.push({
        type: "unordered-list-item",
        text: (ulMatch[1] ?? "").trimEnd(),
      });
      i += 1;
      continue;
    }

    // Ordered list
    const olMatch = line.match(/^(\d+)\.\s+(.+)$/);
    if (olMatch) {
      flushParagraph();
      const index = Number.parseInt(olMatch[1] ?? "1", 10);
      blocks.push({
        type: "ordered-list-item",
        index: Number.isFinite(index) ? index : 1,
        text: (olMatch[2] ?? "").trimEnd(),
      });
      i += 1;
      continue;
    }

    paragraphParts.push(line);
    i += 1;
  }

  flushParagraph();
  return blocks;
}
