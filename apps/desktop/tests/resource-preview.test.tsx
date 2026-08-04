/**
 * Wave C2-C3 — preview gateway parsing + standalone ResourcePreview renderer.
 * Isolated from workbench mounting; no host filesystem access.
 */
import { render, screen, within } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { invoke } from "@tauri-apps/api/core";
import {
  ResourceGatewayError,
  type ResourcePreview,
} from "../src/lib/resourceGateway";
import { TauriResourceGateway } from "../src/lib/tauriResourceGateway";
import { ResourcePreview as ResourcePreviewView } from "../src/features/resources/ResourcePreview";
import {
  parseMarkdownPreview,
  isAllowlistedImagePreview,
} from "../src/features/resources/resourcePreviewModel";

vi.mock("@tauri-apps/api/core", () => ({
  invoke: vi.fn(),
}));

const projectId = "00000000-0000-0000-0000-0000000000aa";

const textPreview: ResourcePreview = {
  path: "src/main.rs",
  kind: "text",
  mediaType: "text/plain; charset=utf-8",
  content: "fn main() {}",
  bytesReturned: 12,
  truncated: false,
};

const markdownPreview: ResourcePreview = {
  path: "docs/guide.md",
  kind: "markdown",
  mediaType: "text/markdown; charset=utf-8",
  content: "# Title\n\nParagraph with <script>alert(1)</script>\n\n- item one\n1. first\n\n```rs\nfn x() {}\n```",
  bytesReturned: 80,
  truncated: false,
};

const imagePreview: ResourcePreview = {
  path: "assets/icon.png",
  kind: "image",
  mediaType: "image/png",
  dataUrl: "data:image/png;base64,iVBORw0KGgo=",
  bytesReturned: 8,
  truncated: false,
};

const binaryPreview: ResourcePreview = {
  path: "blob.bin",
  kind: "binary",
  mediaType: "application/octet-stream",
  bytesReturned: 64,
  truncated: false,
};

describe("previewProjectResource gateway contract", () => {
  beforeEach(() => {
    vi.mocked(invoke).mockReset();
  });

  it("invokes preview_project_resource with projectId and relativePath", async () => {
    vi.mocked(invoke).mockResolvedValue(textPreview);
    const gateway = new TauriResourceGateway();

    await gateway.previewProjectResource({
      projectId,
      relativePath: "src/main.rs",
    });

    expect(invoke).toHaveBeenCalledWith("preview_project_resource", {
      projectId,
      relativePath: "src/main.rs",
    });
  });

  it("accepts exact ResourcePreview DTOs for text, markdown, image, and binary", async () => {
    const gateway = new TauriResourceGateway();

    for (const dto of [textPreview, markdownPreview, imagePreview, binaryPreview]) {
      vi.mocked(invoke).mockResolvedValueOnce(dto);
      await expect(
        gateway.previewProjectResource({
          projectId,
          relativePath: dto.path,
        }),
      ).resolves.toEqual(dto);
    }
  });

  it("rejects raw bytes and unknown extra fields", async () => {
    const gateway = new TauriResourceGateway();

    vi.mocked(invoke).mockResolvedValueOnce({
      ...textPreview,
      bytes: [0, 1, 2],
    });
    await expect(
      gateway.previewProjectResource({ projectId, relativePath: "src/main.rs" }),
    ).rejects.toMatchObject({
      code: "invalid_resource_response",
    });

    vi.mocked(invoke).mockResolvedValueOnce({
      ...binaryPreview,
      extra: "nope",
    });
    await expect(
      gateway.previewProjectResource({ projectId, relativePath: "blob.bin" }),
    ).rejects.toMatchObject({
      code: "invalid_resource_response",
    });
  });

  it("enforces bounds on path, content, dataUrl, and bytesReturned", async () => {
    const gateway = new TauriResourceGateway();

    vi.mocked(invoke).mockResolvedValueOnce({
      ...textPreview,
      path: "C:/Windows/system32",
    });
    await expect(
      gateway.previewProjectResource({ projectId, relativePath: "x" }),
    ).rejects.toMatchObject({ code: "invalid_resource_response" });

    vi.mocked(invoke).mockResolvedValueOnce({
      ...textPreview,
      path: "../secret",
    });
    await expect(
      gateway.previewProjectResource({ projectId, relativePath: "x" }),
    ).rejects.toMatchObject({ code: "invalid_resource_response" });

    vi.mocked(invoke).mockResolvedValueOnce({
      ...textPreview,
      content: "x".repeat(1_048_577),
    });
    await expect(
      gateway.previewProjectResource({ projectId, relativePath: "x" }),
    ).rejects.toMatchObject({ code: "invalid_resource_response" });

    vi.mocked(invoke).mockResolvedValueOnce({
      ...imagePreview,
      dataUrl: `data:image/png;base64,${"A".repeat(2_097_153)}`,
    });
    await expect(
      gateway.previewProjectResource({ projectId, relativePath: "x" }),
    ).rejects.toMatchObject({ code: "invalid_resource_response" });

    vi.mocked(invoke).mockResolvedValueOnce({
      ...textPreview,
      bytesReturned: 1_048_577,
    });
    await expect(
      gateway.previewProjectResource({ projectId, relativePath: "x" }),
    ).rejects.toMatchObject({ code: "invalid_resource_response" });
  });

  it("rejects invalid kind or mediaType combinations", async () => {
    const gateway = new TauriResourceGateway();

    vi.mocked(invoke).mockResolvedValueOnce({
      ...textPreview,
      kind: "html",
    });
    await expect(
      gateway.previewProjectResource({ projectId, relativePath: "x" }),
    ).rejects.toMatchObject({ code: "invalid_resource_response" });

    vi.mocked(invoke).mockResolvedValueOnce({
      ...imagePreview,
      kind: "image",
      mediaType: "image/svg+xml",
      dataUrl: "data:image/svg+xml;base64,AAAA",
    });
    await expect(
      gateway.previewProjectResource({ projectId, relativePath: "x" }),
    ).rejects.toMatchObject({ code: "invalid_resource_response" });
  });

  it("accepts a small canonical image DTO and rejects non-canonical image payloads", async () => {
    const gateway = new TauriResourceGateway();
    const pngPrefix = "data:image/png;base64,";
    // PNG signature (8 bytes) → canonical RFC 4648 standard base64.
    const canonicalPng: ResourcePreview = {
      path: "assets/icon.png",
      kind: "image",
      mediaType: "image/png",
      dataUrl: `${pngPrefix}iVBORw0KGgo=`,
      bytesReturned: 8,
      truncated: false,
    };

    vi.mocked(invoke).mockResolvedValueOnce(canonicalPng);
    await expect(
      gateway.previewProjectResource({
        projectId,
        relativePath: canonicalPng.path,
      }),
    ).resolves.toEqual(canonicalPng);

    // truncated image DTO is never accepted on the typed image branch
    vi.mocked(invoke).mockResolvedValueOnce({
      ...canonicalPng,
      truncated: true,
    });
    await expect(
      gateway.previewProjectResource({ projectId, relativePath: "x" }),
    ).rejects.toMatchObject({ code: "invalid_resource_response" });

    // invalid base64 alphabet (URL-safe / non-standard characters)
    vi.mocked(invoke).mockResolvedValueOnce({
      ...canonicalPng,
      dataUrl: `${pngPrefix}iVBORw0KGgo_`,
      bytesReturned: 8,
    });
    await expect(
      gateway.previewProjectResource({ projectId, relativePath: "x" }),
    ).rejects.toMatchObject({ code: "invalid_resource_response" });

    // non-canonical pad bits: atob may succeed, but btoa re-encode differs
    vi.mocked(invoke).mockResolvedValueOnce({
      path: "assets/one.bin.png",
      kind: "image",
      mediaType: "image/png",
      dataUrl: `${pngPrefix}AB==`,
      bytesReturned: 1,
      truncated: false,
    });
    await expect(
      gateway.previewProjectResource({ projectId, relativePath: "x" }),
    ).rejects.toMatchObject({ code: "invalid_resource_response" });

    // wrong terminal padding for claimed byte length (length-correct, pad count wrong)
    vi.mocked(invoke).mockResolvedValueOnce({
      path: "assets/one.bin.png",
      kind: "image",
      mediaType: "image/png",
      dataUrl: `${pngPrefix}AAA=`,
      bytesReturned: 1,
      truncated: false,
    });
    await expect(
      gateway.previewProjectResource({ projectId, relativePath: "x" }),
    ).rejects.toMatchObject({ code: "invalid_resource_response" });

    // decoded length must equal bytesReturned (AAAA → 3 bytes, claim 2)
    vi.mocked(invoke).mockResolvedValueOnce({
      path: "assets/icon.png",
      kind: "image",
      mediaType: "image/png",
      dataUrl: `${pngPrefix}AAAA`,
      bytesReturned: 2,
      truncated: false,
    });
    await expect(
      gateway.previewProjectResource({ projectId, relativePath: "x" }),
    ).rejects.toMatchObject({ code: "invalid_resource_response" });

    // Below old 2 MiB ceiling, above exact 1 MiB-derived encoded limit.
    // Old loose check: dataUrl.length <= 2_097_152; new: exact length from bytesReturned.
    const maxEncodedFor1MiB = 4 * Math.ceil(1_048_576 / 3);
    const overExactEncoded = maxEncodedFor1MiB + 4;
    const overExactDataUrl = `${pngPrefix}${"A".repeat(overExactEncoded)}`;
    expect(overExactDataUrl.length).toBeLessThan(2_097_152);
    expect(overExactDataUrl.length).toBeGreaterThan(
      pngPrefix.length + maxEncodedFor1MiB,
    );
    vi.mocked(invoke).mockResolvedValueOnce({
      path: "assets/big.png",
      kind: "image",
      mediaType: "image/png",
      dataUrl: overExactDataUrl,
      bytesReturned: 1_048_576,
      truncated: false,
    });
    await expect(
      gateway.previewProjectResource({ projectId, relativePath: "x" }),
    ).rejects.toMatchObject({ code: "invalid_resource_response" });

    // MIME / dataUrl prefix mismatch remains rejected
    vi.mocked(invoke).mockResolvedValueOnce({
      ...canonicalPng,
      mediaType: "image/png",
      dataUrl: "data:image/jpeg;base64,iVBORw0KGgo=",
    });
    await expect(
      gateway.previewProjectResource({ projectId, relativePath: "x" }),
    ).rejects.toMatchObject({ code: "invalid_resource_response" });
  });

  it("normalizes host errors to portable opaque failures", async () => {
    vi.mocked(invoke).mockRejectedValue({
      code: "resource_path_rejected",
      message: "Resource path was rejected.",
    });
    const gateway = new TauriResourceGateway();
    await expect(
      gateway.previewProjectResource({
        projectId,
        relativePath: "C:/Windows",
      }),
    ).rejects.toMatchObject({
      name: "ResourceGatewayError",
      code: "resource_path_rejected",
      message: "Resource path was rejected.",
    });

    vi.mocked(invoke).mockRejectedValue(
      new Error("open F:\\\\secret\\\\project\\\\file.bin"),
    );
    await expect(
      gateway.previewProjectResource({ projectId, relativePath: "file.bin" }),
    ).rejects.toMatchObject({
      code: "resource_command_failed",
      message: "Resource command failed.",
    });
  });
});

describe("resourcePreviewModel markdown subset", () => {
  it("parses headings, paragraphs, lists, and fenced code", () => {
    const blocks = parseMarkdownPreview(
      "# H1\n\nHello world\n\n- alpha\n* beta\n\n1. one\n2. two\n\n```ts\nconst n = 1;\n```\n",
    );
    expect(blocks).toEqual([
      { type: "heading", level: 1, text: "H1" },
      { type: "paragraph", text: "Hello world" },
      { type: "unordered-list-item", text: "alpha" },
      { type: "unordered-list-item", text: "beta" },
      { type: "ordered-list-item", index: 1, text: "one" },
      { type: "ordered-list-item", index: 2, text: "two" },
      { type: "code-block", language: "ts", code: "const n = 1;" },
    ]);
  });

  it("treats raw HTML as plain text content, never as structure", () => {
    const blocks = parseMarkdownPreview(
      "<script>alert(1)</script>\n\n## Safe\n\n<div onclick=\"x\">nope</div>",
    );
    expect(blocks.some((b) => b.type === "heading" && b.text === "Safe")).toBe(true);
    const texts = blocks.flatMap((b) => {
      if ("text" in b) return [b.text];
      if (b.type === "code-block") return [b.code];
      return [];
    });
    expect(texts.join("\n")).toContain("<script>alert(1)</script>");
    expect(texts.join("\n")).toContain("<div onclick=\"x\">nope</div>");
  });

  it("accepts only allow-listed image kind/mediaType/dataUrl triples", () => {
    expect(isAllowlistedImagePreview(imagePreview)).toBe(true);
    expect(
      isAllowlistedImagePreview({
        ...imagePreview,
        mediaType: "image/jpeg",
        dataUrl: "data:image/jpeg;base64,AAAA",
      }),
    ).toBe(true);
    expect(
      isAllowlistedImagePreview({
        ...imagePreview,
        mediaType: "image/png",
        dataUrl: "data:image/jpeg;base64,AAAA",
      }),
    ).toBe(false);
    expect(
      isAllowlistedImagePreview({
        ...imagePreview,
        kind: "binary",
      }),
    ).toBe(false);
    expect(
      isAllowlistedImagePreview({
        ...imagePreview,
        mediaType: "image/svg+xml",
        dataUrl: "data:image/svg+xml;base64,AAAA",
      }),
    ).toBe(false);
  });
});

describe("ResourcePreview renderer", () => {
  it("renders text in a pre element with portable path only", () => {
    render(<ResourcePreviewView preview={textPreview} />);
    expect(screen.getByText("src/main.rs")).toBeInTheDocument();
    const pre = screen.getByText("fn main() {}");
    expect(pre.tagName).toBe("PRE");
    expect(document.body.textContent).not.toMatch(/[A-Za-z]:\\/);
  });

  it("renders markdown semantic subset without interpreting HTML", () => {
    const { container } = render(<ResourcePreviewView preview={markdownPreview} />);
    expect(screen.getByRole("heading", { level: 1, name: "Title" })).toBeInTheDocument();
    expect(screen.getByText("item one")).toBeInTheDocument();
    expect(screen.getByText("first")).toBeInTheDocument();
    expect(screen.getByText("fn x() {}")).toBeInTheDocument();
    expect(screen.getByText(/script/i)).toBeInTheDocument();
    expect(container.querySelector("script")).toBeNull();
    expect(container.innerHTML).not.toContain("dangerouslySetInnerHTML");
    // Ensure no raw HTML API was used by checking no injected nodes.
    expect(container.querySelectorAll("script").length).toBe(0);
  });

  it("does not use dangerouslySetInnerHTML anywhere in the preview tree", () => {
    const { container } = render(<ResourcePreviewView preview={markdownPreview} />);
    const html = container.innerHTML;
    expect(html).not.toMatch(/dangerouslySetInnerHTML/i);
    // React would interpret HTML only via that API or similar; assert no unexpected tags.
    expect(container.querySelector("script")).toBeNull();
    expect(container.querySelector("iframe")).toBeNull();
  });

  it("renders valid allow-listed image data URLs", () => {
    render(<ResourcePreviewView preview={imagePreview} />);
    const img = screen.getByRole("img");
    expect(img).toHaveAttribute("src", imagePreview.dataUrl);
    expect(img).toHaveAttribute("alt", imagePreview.path);
  });

  it("rejects MIME/dataUrl mismatch by falling back to metadata only", () => {
    const mismatched: ResourcePreview = {
      path: "assets/icon.png",
      kind: "image",
      mediaType: "image/png",
      dataUrl: "data:image/jpeg;base64,AAAA",
      bytesReturned: 4,
      truncated: false,
    };
    render(<ResourcePreviewView preview={mismatched} />);
    expect(screen.queryByRole("img")).toBeNull();
    expect(screen.getByText("assets/icon.png")).toBeInTheDocument();
    expect(screen.getByText("metadata only")).toBeInTheDocument();
    expect(screen.getByText("binary")).toBeInTheDocument();
  });

  it("renders binary metadata only", () => {
    render(<ResourcePreviewView preview={binaryPreview} />);
    expect(screen.getByText("blob.bin")).toBeInTheDocument();
    expect(screen.getByText("application/octet-stream")).toBeInTheDocument();
    expect(screen.getByText(/64/)).toBeInTheDocument();
    expect(screen.queryByRole("img")).toBeNull();
    expect(document.querySelector("pre")).toBeNull();
  });

  it("never surfaces host machine paths", () => {
    render(
      <ResourcePreviewView
        preview={{
          ...textPreview,
          path: "notes/readme.md",
          content: "safe body",
        }}
      />,
    );
    const root = screen.getByTestId("resource-preview");
    expect(within(root).queryByText(/F:\\/i)).toBeNull();
    expect(within(root).queryByText(/C:\\/i)).toBeNull();
    expect(within(root).getByText("notes/readme.md")).toBeInTheDocument();
  });
});

describe("ResourceGatewayError portable-path-only behavior for preview", () => {
  it("keeps ResourceGatewayError opaque", () => {
    const err = new ResourceGatewayError(
      "resource_path_rejected",
      "Resource path was rejected.",
    );
    expect(err.message).not.toMatch(/[\\/]/);
    expect(err.message).not.toMatch(/[A-Za-z]:\\/);
  });
});
