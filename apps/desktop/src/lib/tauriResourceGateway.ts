/**
 * Tauri resource gateway — typed, project-scoped, read-only bridge.
 * Validates host payloads exactly; never grants renderer filesystem access.
 */

import { invoke } from "@tauri-apps/api/core";
import {
  ResourceGatewayError,
  normalizeResourceGatewayError,
  type RecentResource,
  type ResourceContentKind,
  type ResourceEntry,
  type ResourceEntryKind,
  type ResourceFile,
  type ResourceGateway,
  type ResourcePreview,
  type ResourcePreviewKind,
  type ResourceSearchResult,
} from "./resourceGateway";

const INVALID_RESPONSE_MESSAGE = "Pillow received an invalid resource response.";

const ENTRY_KINDS: ResourceEntryKind[] = [
  "file",
  "directory",
  "symlink",
  "other",
];

const CONTENT_KINDS: ResourceContentKind[] = ["utf8", "binary"];

const PREVIEW_KINDS: ResourcePreviewKind[] = [
  "text",
  "markdown",
  "image",
  "binary",
];

const ALLOWED_IMAGE_MEDIA_TYPES = [
  "image/png",
  "image/jpeg",
  "image/gif",
  "image/webp",
] as const;

const MAX_PATH_CHARS = 4_096;
const MAX_CONTENT_CHARS = 1_048_576;
const MAX_PREVIEW_CHARS = 2_048;
const MAX_LIST = 1_000;

/** RFC 4648 standard base64 alphabet + optional terminal padding only. */
const STANDARD_BASE64_BODY_RE = /^[A-Za-z0-9+/]*={0,2}$/;

function invalidResourceResponse(): ResourceGatewayError {
  return new ResourceGatewayError("invalid_resource_response", INVALID_RESPONSE_MESSAGE);
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === "object" && value !== null && !Array.isArray(value);
}

function isNonEmptyString(value: unknown): value is string {
  return typeof value === "string" && value.trim().length > 0;
}

function isSafeInteger(
  value: unknown,
  minimum = 0,
  maximum = Number.MAX_SAFE_INTEGER,
): value is number {
  return (
    typeof value === "number"
    && Number.isSafeInteger(value)
    && value >= minimum
    && value <= maximum
  );
}

function hasOnlyKeys(
  value: Record<string, unknown>,
  keys: readonly string[],
): boolean {
  const allowed = new Set(keys);
  return Object.keys(value).every((key) => allowed.has(key));
}

/** Portable relative path: `.` or slash-separated components; never absolute. */
function isPortableRelativePath(value: unknown): value is string {
  if (typeof value !== "string" || value.length === 0 || value.length > MAX_PATH_CHARS) {
    return false;
  }
  if (value === ".") return true;
  if (
    value.startsWith("/")
    || value.startsWith("\\")
    || value.includes("\\")
    || value.includes(":")
    || value.includes("..")
  ) {
    return false;
  }
  return value.split("/").every((component) => {
    if (component.length === 0 || component === "." || component === "..") {
      return false;
    }
    if (component.length > 255) return false;
    return !/[<>:"|?*\u0000-\u001f]/.test(component);
  });
}

function isEntryKind(value: unknown): value is ResourceEntryKind {
  return typeof value === "string" && (ENTRY_KINDS as string[]).includes(value);
}

function isContentKind(value: unknown): value is ResourceContentKind {
  return typeof value === "string" && (CONTENT_KINDS as string[]).includes(value);
}

function isPreviewKind(value: unknown): value is ResourcePreviewKind {
  return typeof value === "string" && (PREVIEW_KINDS as string[]).includes(value);
}

function isAllowedImageMediaType(
  value: unknown,
): value is (typeof ALLOWED_IMAGE_MEDIA_TYPES)[number] {
  return (
    typeof value === "string"
    && (ALLOWED_IMAGE_MEDIA_TYPES as readonly string[]).includes(value)
  );
}

function dataUrlPrefixForMediaType(mediaType: string): string {
  return `data:${mediaType};base64,`;
}

/**
 * Exact encoded length for `byteLength` raw bytes under RFC 4648 base64.
 * Replaces any loose data-URL character ceiling for image previews.
 */
function expectedBase64EncodedLength(byteLength: number): number {
  return 4 * Math.ceil(byteLength / 3);
}

/**
 * Canonical RFC 4648 standard base64 only:
 * standard alphabet, correct terminal padding, successful decode,
 * decoded length equals `expectedByteLength`, and re-encode equals input.
 * Rejects URL-safe alphabet, whitespace, and non-canonical pad bits.
 */
function isCanonicalStandardBase64(
  encoded: string,
  expectedByteLength: number,
): boolean {
  if (
    !Number.isSafeInteger(expectedByteLength)
    || expectedByteLength <= 0
    || encoded.length !== expectedBase64EncodedLength(expectedByteLength)
  ) {
    return false;
  }
  if (!STANDARD_BASE64_BODY_RE.test(encoded)) {
    return false;
  }

  const remainder = expectedByteLength % 3;
  const expectedPadding = remainder === 0 ? 0 : 3 - remainder;
  let padding = 0;
  for (let i = encoded.length - 1; i >= 0 && encoded[i] === "="; i -= 1) {
    padding += 1;
  }
  if (padding !== expectedPadding) {
    return false;
  }
  if (encoded.slice(0, encoded.length - padding).includes("=")) {
    return false;
  }

  try {
    const binary = atob(encoded);
    if (binary.length !== expectedByteLength) {
      return false;
    }
    // Non-canonical pad bits decode in some engines but re-encode differ.
    if (btoa(binary) !== encoded) {
      return false;
    }
    return true;
  } catch {
    return false;
  }
}

function parseResourceEntry(value: unknown): ResourceEntry {
  if (!isRecord(value) || !hasOnlyKeys(value, ["path", "kind", "sizeBytes"])) {
    throw invalidResourceResponse();
  }
  if (!isPortableRelativePath(value.path) || !isEntryKind(value.kind)) {
    throw invalidResourceResponse();
  }
  if (value.sizeBytes !== undefined && !isSafeInteger(value.sizeBytes, 0)) {
    throw invalidResourceResponse();
  }
  const entry: ResourceEntry = {
    path: value.path,
    kind: value.kind,
  };
  if (value.sizeBytes !== undefined) {
    entry.sizeBytes = value.sizeBytes;
  }
  return entry;
}

function parseResourceFile(value: unknown): ResourceFile {
  if (
    !isRecord(value)
    || !hasOnlyKeys(value, [
      "path",
      "contentKind",
      "content",
      "bytesReturned",
      "truncated",
    ])
  ) {
    throw invalidResourceResponse();
  }
  if (
    !isPortableRelativePath(value.path)
    || !isContentKind(value.contentKind)
    || !isSafeInteger(value.bytesReturned, 0, MAX_CONTENT_CHARS)
    || typeof value.truncated !== "boolean"
  ) {
    throw invalidResourceResponse();
  }

  if (value.contentKind === "utf8") {
    if (value.content !== undefined) {
      if (typeof value.content !== "string" || value.content.length > MAX_CONTENT_CHARS) {
        throw invalidResourceResponse();
      }
    }
  } else if (value.content !== undefined) {
    throw invalidResourceResponse();
  }

  const file: ResourceFile = {
    path: value.path,
    contentKind: value.contentKind,
    bytesReturned: value.bytesReturned,
    truncated: value.truncated,
  };
  if (typeof value.content === "string") {
    file.content = value.content;
  }
  return file;
}

function parseSearchResult(value: unknown): ResourceSearchResult {
  if (
    !isRecord(value)
    || !hasOnlyKeys(value, ["path", "line", "column", "preview"])
  ) {
    throw invalidResourceResponse();
  }
  if (
    !isPortableRelativePath(value.path)
    || !isSafeInteger(value.line, 1)
    || !isSafeInteger(value.column, 1)
    || typeof value.preview !== "string"
    || value.preview.length > MAX_PREVIEW_CHARS
  ) {
    throw invalidResourceResponse();
  }
  return {
    path: value.path,
    line: value.line,
    column: value.column,
    preview: value.preview,
  };
}

function parseRecentResource(value: unknown): RecentResource {
  if (!isRecord(value) || !hasOnlyKeys(value, ["path", "openedAt"])) {
    throw invalidResourceResponse();
  }
  if (!isPortableRelativePath(value.path) || !isNonEmptyString(value.openedAt)) {
    throw invalidResourceResponse();
  }
  return {
    path: value.path,
    openedAt: value.openedAt,
  };
}

function parseResourcePreview(value: unknown): ResourcePreview {
  if (
    !isRecord(value)
    || !hasOnlyKeys(value, [
      "path",
      "kind",
      "mediaType",
      "content",
      "dataUrl",
      "bytesReturned",
      "truncated",
    ])
  ) {
    throw invalidResourceResponse();
  }
  if (
    !isPortableRelativePath(value.path)
    || !isPreviewKind(value.kind)
    || typeof value.mediaType !== "string"
    || value.mediaType.length === 0
    || value.mediaType.length > 128
    || !isSafeInteger(value.bytesReturned, 0, MAX_CONTENT_CHARS)
    || typeof value.truncated !== "boolean"
  ) {
    throw invalidResourceResponse();
  }

  // Reject raw-byte transport if a host ever tries to smuggle it.
  if ("bytes" in value) {
    throw invalidResourceResponse();
  }

  const kind = value.kind;
  const mediaType = value.mediaType;

  if (kind === "text") {
    if (mediaType !== "text/plain; charset=utf-8") {
      throw invalidResourceResponse();
    }
    if (value.dataUrl !== undefined) {
      throw invalidResourceResponse();
    }
    if (value.content !== undefined) {
      if (typeof value.content !== "string" || value.content.length > MAX_CONTENT_CHARS) {
        throw invalidResourceResponse();
      }
    }
  } else if (kind === "markdown") {
    if (mediaType !== "text/markdown; charset=utf-8") {
      throw invalidResourceResponse();
    }
    if (value.dataUrl !== undefined) {
      throw invalidResourceResponse();
    }
    if (value.content !== undefined) {
      if (typeof value.content !== "string" || value.content.length > MAX_CONTENT_CHARS) {
        throw invalidResourceResponse();
      }
    }
  } else if (kind === "image") {
    if (!isAllowedImageMediaType(mediaType)) {
      throw invalidResourceResponse();
    }
    if (value.content !== undefined) {
      throw invalidResourceResponse();
    }
    // Host may mark truncated image-extension paths as binary; the renderer
    // image branch only accepts complete, non-truncated image DTOs.
    if (value.truncated !== false) {
      throw invalidResourceResponse();
    }
    if (!isSafeInteger(value.bytesReturned, 1, MAX_CONTENT_CHARS)) {
      throw invalidResourceResponse();
    }
    if (typeof value.dataUrl !== "string" || value.dataUrl.length === 0) {
      throw invalidResourceResponse();
    }
    const prefix = dataUrlPrefixForMediaType(mediaType);
    const expectedEncodedLength = expectedBase64EncodedLength(value.bytesReturned);
    if (value.dataUrl.length !== prefix.length + expectedEncodedLength) {
      throw invalidResourceResponse();
    }
    if (!value.dataUrl.startsWith(prefix)) {
      throw invalidResourceResponse();
    }
    const encoded = value.dataUrl.slice(prefix.length);
    if (!isCanonicalStandardBase64(encoded, value.bytesReturned)) {
      throw invalidResourceResponse();
    }
  } else {
    // binary
    if (mediaType !== "application/octet-stream") {
      throw invalidResourceResponse();
    }
    if (value.content !== undefined || value.dataUrl !== undefined) {
      throw invalidResourceResponse();
    }
  }

  const preview: ResourcePreview = {
    path: value.path,
    kind,
    mediaType,
    bytesReturned: value.bytesReturned,
    truncated: value.truncated,
  };
  if (typeof value.content === "string") {
    preview.content = value.content;
  }
  if (typeof value.dataUrl === "string") {
    preview.dataUrl = value.dataUrl;
  }
  return preview;
}

function parseEntryArray(payload: unknown): ResourceEntry[] {
  if (!Array.isArray(payload) || payload.length > MAX_LIST) {
    throw invalidResourceResponse();
  }
  return payload.map(parseResourceEntry);
}

function parseSearchArray(payload: unknown): ResourceSearchResult[] {
  if (!Array.isArray(payload) || payload.length > MAX_LIST) {
    throw invalidResourceResponse();
  }
  return payload.map(parseSearchResult);
}

function parseRecentArray(payload: unknown): RecentResource[] {
  if (!Array.isArray(payload) || payload.length > 50) {
    throw invalidResourceResponse();
  }
  return payload.map(parseRecentResource);
}

/** Tauri-only boundary. The renderer does not access the filesystem directly. */
export class TauriResourceGateway implements ResourceGateway {
  async listProjectResources(input: {
    projectId: string;
    relativePath?: string;
  }): Promise<ResourceEntry[]> {
    try {
      const payload = await invoke<unknown>("list_project_resources", {
        projectId: input.projectId,
        relativePath: input.relativePath,
      });
      return parseEntryArray(payload);
    } catch (error) {
      throw normalizeResourceGatewayError(error);
    }
  }

  async readProjectResource(input: {
    projectId: string;
    relativePath: string;
  }): Promise<ResourceFile> {
    try {
      const payload = await invoke<unknown>("read_project_resource", {
        projectId: input.projectId,
        relativePath: input.relativePath,
      });
      return parseResourceFile(payload);
    } catch (error) {
      throw normalizeResourceGatewayError(error);
    }
  }

  async searchProjectResources(input: {
    projectId: string;
    query: string;
    relativePath?: string;
  }): Promise<ResourceSearchResult[]> {
    try {
      const payload = await invoke<unknown>("search_project_resources", {
        projectId: input.projectId,
        query: input.query,
        relativePath: input.relativePath,
      });
      return parseSearchArray(payload);
    } catch (error) {
      throw normalizeResourceGatewayError(error);
    }
  }

  async listRecentProjectResources(input: {
    projectId: string;
  }): Promise<RecentResource[]> {
    try {
      const payload = await invoke<unknown>("list_recent_project_resources", {
        projectId: input.projectId,
      });
      return parseRecentArray(payload);
    } catch (error) {
      throw normalizeResourceGatewayError(error);
    }
  }

  async previewProjectResource(input: {
    projectId: string;
    relativePath: string;
  }): Promise<ResourcePreview> {
    try {
      const payload = await invoke<unknown>("preview_project_resource", {
        projectId: input.projectId,
        relativePath: input.relativePath,
      });
      return parseResourcePreview(payload);
    } catch (error) {
      throw normalizeResourceGatewayError(error);
    }
  }
}
