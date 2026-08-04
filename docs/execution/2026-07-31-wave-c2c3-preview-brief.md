# Wave C2-C3 Preview Transport and Renderer Brief

**Controller session:** `<keeper-session>`
**Stage:** Stage 1 / C2 read-only project resource workbench
**Worker role:** implementation only; the controller owns scope, integration
decisions, acceptance, and Keeper completion
**Repository:** `<workspace>\pillow`
**Baseline HEAD:** `7b7f70f7360131cd1cfd984a8b19bdfb5725c21b`

## Objective

Implement one bounded, project-scoped preview command and one standalone
read-only preview renderer. This task does not mount the preview into the main
workbench; mounting is a later non-overlapping task.

The host remains the only filesystem boundary. The renderer receives a typed,
bounded DTO and never receives project roots, file handles, or raw binary byte
arrays.

## Frozen Scope

This task may:

- preview UTF-8 text;
- classify portable `.md` paths as Markdown;
- preview complete PNG, JPEG, GIF, and WebP images as one bounded `dataUrl`;
- return metadata-only previews for unsupported or truncated binary files;
- render text, a small non-HTML Markdown model, safe allow-listed images, and
  binary metadata.

This task must not:

- write, rename, delete, or create project files;
- add terminal, Git, Obsidian, key, MCP, network, or runtime behavior;
- mount anything into `App`, `Workbench`, or `ConversationTimeline`;
- modify existing C1, C2-A, C2-B, C2-C1, or C2-C2 behavior outside the frozen
  interfaces below;
- touch `<workspace>\VaultSystem\keeper`;
- stage or commit.

## Exclusive Write Whitelist

The Worker may modify only:

- `Cargo.toml`
- `Cargo.lock`
- `apps/desktop/src-tauri/Cargo.toml`
- `apps/desktop/src-tauri/src/lib.rs`
- `apps/desktop/src-tauri/src/resource_commands.rs`
- `apps/desktop/src-tauri/src/resource_commands_tests.rs`
- `apps/desktop/src/lib/resourceGateway.ts`
- `apps/desktop/src/lib/tauriResourceGateway.ts`

The Worker may create only:

- `apps/desktop/src/features/resources/resourcePreviewModel.ts`
- `apps/desktop/src/features/resources/ResourcePreview.tsx`
- `apps/desktop/src/styles/resource-preview.css`
- `apps/desktop/tests/resource-preview.test.tsx`
- `docs/execution/2026-07-31-wave-c2c3-preview-report.md`

All other paths are read-only. Existing dirty changes are user work and must
not be discarded, reformatted wholesale, reverted, staged, or committed.

## Required Public Contract

```ts
export type ResourcePreviewKind =
  | "text"
  | "markdown"
  | "image"
  | "binary";

export type ResourcePreview = {
  path: string;
  kind: ResourcePreviewKind;
  mediaType: string;
  content?: string;
  dataUrl?: string;
  bytesReturned: number;
  truncated: boolean;
};

export type PreviewProjectResourceInput = {
  projectId: string;
  relativePath: string;
};

export interface ResourceGateway {
  // existing methods remain unchanged
  previewProjectResource(
    input: PreviewProjectResourceInput,
  ): Promise<ResourcePreview>;
}
```

The Tauri command name is `preview_project_resource`; its arguments remain
`projectId` and `relativePath`.

## Host Classification Rules

The command must resolve a registered `projectId` and read only through the
existing `ResourceReader::read` capability path with the existing
`MAX_READ_BYTES` cap. There is no cwd or last-project fallback.

1. Valid UTF-8:
   - lower-case portable extension `.md` -> `kind: "markdown"` and
     `mediaType: "text/markdown; charset=utf-8"`;
   - every other UTF-8 file -> `kind: "text"` and
     `mediaType: "text/plain; charset=utf-8"`;
   - `content` may be present, `dataUrl` must be absent.
2. Binary:
   - an image is allowed only when `truncated == false`, the lower-case
     extension is allow-listed, and the signature agrees:
     - `.png`: `89 50 4E 47 0D 0A 1A 0A`;
     - `.jpg` or `.jpeg`: `FF D8 FF`;
     - `.gif`: `GIF87a` or `GIF89a`;
     - `.webp`: `RIFF` at bytes `0..4` and `WEBP` at bytes `8..12`;
   - accepted images return `kind: "image"`, the exact allow-listed MIME, and
     one `data:<mime>;base64,...` string;
   - every unsupported, signature-mismatched, or truncated binary returns
     `kind: "binary"` and `mediaType: "application/octet-stream"` with no
     `content` and no `dataUrl`.
3. The DTO must never serialize a `bytes` property.
4. Portable relative paths only; error responses remain opaque and must not
   include project roots, temp roots, rejected absolute paths, or file
   contents.
5. Successful preview updates only that project's existing recent-resource
   record, consistently with a successful read.

Use workspace dependency `base64 = "0.22"` if an encoder dependency is needed;
do not add any other Rust or npm dependency.

## Renderer Rules

`ResourcePreview.tsx` is a controlled, standalone view:

- text uses `<pre>`;
- Markdown is converted by a pure local model into a deliberately small
  semantic subset: headings, paragraphs, unordered/ordered list items, and
  fenced code blocks;
- raw HTML is rendered as text, never interpreted;
- `dangerouslySetInnerHTML` is forbidden;
- images render only when both `kind`, `mediaType`, and the `dataUrl` prefix
  agree with the four allow-listed MIME types;
- unsupported binary renders metadata only;
- all user-visible paths are the portable DTO `path`, never host paths;
- no object URL, direct filesystem call, native handle, network fetch, or
  persistence is introduced.

## Test-First Sequence

1. Write the Rust and renderer tests first.
2. Run both focused commands and record the expected RED result in the report.
3. Implement the smallest production change that satisfies the contract.
4. Run the focused GREEN gates.

Required Rust coverage:

- project A cannot preview project B content;
- unknown/empty project IDs fail closed;
- Markdown and text classification;
- each allow-listed image requires both extension and matching signature;
- mismatched extension/signature and unsupported binary stay metadata-only;
- a file larger than 1 MiB is truncated and never becomes an image;
- serialized DTO has no `bytes`;
- errors do not contain machine paths or rejected absolute-path fragments;
- recents update only the matching project.

Required TypeScript/DOM coverage:

- exact DTO parsing and rejection of extra/raw-byte fields;
- bounds on path, content, data URL, and `bytesReturned`;
- text and Markdown semantic rendering;
- raw HTML is not interpreted and no unsafe HTML API is used;
- valid image data URL rendering;
- MIME/data URL mismatch rejection;
- binary metadata rendering;
- portable-path-only error behavior.

## Required Gates

```powershell
cargo test -p pillow-desktop resource_preview
pnpm --filter @pillow/desktop exec vitest run tests/resource-preview.test.tsx
pnpm --filter @pillow/desktop typecheck
cargo fmt --all -- --check
git diff --check
```

Every command must exit `0` before completion. Do not run broad formatting over
unrelated files.

## Completion Contract

The Worker must:

1. write `docs/execution/2026-07-31-wave-c2c3-preview-report.md`;
2. list exact changed files;
3. record RED evidence and final GREEN command outputs/counts;
4. state whether any whitelist violation, staging, commit, residual process,
   scope expansion, machine-path leak, or raw-byte transport remains;
5. post a Keeper completion message to the controller session
   `<keeper-session>`;
6. stop its child process and release its Keeper runtime lease after the
   controller captures completion.

Completion does not imply acceptance. The controller will independently rerun
the gates and then send the result to Codex2 for a read-only protocol and
security review.
