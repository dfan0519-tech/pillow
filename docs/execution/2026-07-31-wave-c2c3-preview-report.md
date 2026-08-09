# Wave C2-C3 Preview Transport and Renderer — Worker Report

**Controller session:** `<keeper-session>`
**Worker:** grok-c2c3-worker
**Worker Keeper session:** `<keeper-session>`
**Stage:** Stage 1 / C2 read-only project resource workbench
**Baseline HEAD:** `7b7f70f7360131cd1cfd984a8b19bdfb5725c21b`
**Date:** 2026-07-31
**Status:** Worker complete (unstaged). Completion does **not** imply acceptance.

## Objective delivered

Bounded, project-scoped `preview_project_resource` host command plus standalone
read-only `ResourcePreview` renderer. Preview is **not** mounted into App /
Workbench / ConversationTimeline (later task).

## Exact changed files (this wave)

### Modified (whitelist)

| Path | Change |
|------|--------|
| `Cargo.toml` | Workspace dep `base64 = "0.22"` |
| `Cargo.lock` | Resolved base64 for pillow-desktop |
| `apps/desktop/src-tauri/Cargo.toml` | `base64.workspace = true` |
| `apps/desktop/src-tauri/src/lib.rs` | Register `preview_project_resource` |
| `apps/desktop/src-tauri/src/resource_commands.rs` | Preview DTO, classification, command |
| `apps/desktop/src-tauri/src/resource_commands_tests.rs` | 9× `resource_preview_*` tests |
| `apps/desktop/src/lib/resourceGateway.ts` | `ResourcePreview*` types + gateway method |
| `apps/desktop/src/lib/tauriResourceGateway.ts` | Invoke + exact DTO parse / reject |

### Created (whitelist)

| Path | Change |
|------|--------|
| `apps/desktop/src/features/resources/resourcePreviewModel.ts` | Markdown subset + image allow-list helpers |
| `apps/desktop/src/features/resources/ResourcePreview.tsx` | Standalone controlled renderer |
| `apps/desktop/src/styles/resource-preview.css` | Preview styles |
| `apps/desktop/tests/resource-preview.test.tsx` | Gateway + DOM coverage (17 tests) |
| `docs/execution/2026-07-31-wave-c2c3-preview-report.md` | This report |

No other paths were intentionally written. Pre-existing dirty tree left intact
(no stage, commit, revert, or wholesale reformat of unrelated work).

## Public contract

- Tauri: `preview_project_resource(projectId, relativePath) -> ResourcePreviewDto`
- TS: `ResourceGateway.previewProjectResource({ projectId, relativePath }) -> ResourcePreview`
- Kinds: `text` | `markdown` | `image` | `binary`
- Never serializes a `bytes` property
- Host-only filesystem; renderer receives typed bounded DTO only

## Host classification (implemented)

1. Resolve registered `projectId` only (no cwd / last-project fallback).
2. Read via `ResourceReader::read` + `MAX_READ_BYTES` (1 MiB).
3. Allow-listed image extensions (`png`/`jpg`/`jpeg`/`gif`/`webp`) always go
   through image rules on raw bytes (GIF/WebP may be valid UTF-8):
   - complete + matching signature → `kind: image` + `data:<mime>;base64,...`
   - truncated / signature mismatch → `kind: binary` metadata only
4. Other valid UTF-8: `.md` → markdown; else text.
5. Other binary → `application/octet-stream` metadata only.
6. Successful preview updates only that project's recents.
7. Errors are opaque codes/messages (no roots, temp paths, or contents).

Image signatures:

| Ext | Signature |
|-----|-----------|
| `.png` | `89 50 4E 47 0D 0A 1A 0A` |
| `.jpg`/`.jpeg` | `FF D8 FF` |
| `.gif` | `GIF87a` or `GIF89a` |
| `.webp` | `RIFF` @0..4 and `WEBP` @8..12 |

## Renderer rules (implemented)

- Text → `<pre>`
- Markdown → pure local model (headings, paragraphs, ul/ol items, fenced code)
- Raw HTML remains text nodes; **no** `dangerouslySetInnerHTML`
- Images only when `kind` + `mediaType` + `dataUrl` prefix agree on allow-list
- Binary / mismatched image → metadata only
- Portable DTO `path` only; no object URLs, FS, network, or persistence

## RED evidence (test-first)

### Rust (before production wiring)

```text
cargo test -p pillow-desktop resource_preview
→ compile errors: no method named `preview_project_resource` on AppState
  (E0599 ×13 sites in resource_commands_tests.rs)
exit 1
```

### TypeScript (before modules existed)

```text
pnpm --filter @pillow/desktop exec vitest run tests/resource-preview.test.tsx
→ Failed to resolve import "../src/features/resources/ResourcePreview"
  (and resourcePreviewModel)
exit 1
```

## GREEN evidence (final)

```text
cargo test -p pillow-desktop resource_preview
→ test result: ok. 9 passed; 0 failed; 0 ignored; 0 measured; 72 filtered out
exit 0

pnpm --filter @pillow/desktop exec vitest run tests/resource-preview.test.tsx
→ Test Files  1 passed (1)
→ Tests       17 passed (17)
exit 0

pnpm --filter @pillow/desktop typecheck
→ tsc --noEmit
exit 0

cargo fmt --all -- --check
exit 0

git diff --check
exit 0
```

### Rust tests (9)

1. `resource_preview_unknown_and_empty_project_ids_fail_closed`
2. `resource_preview_is_isolated_by_project_id`
3. `resource_preview_classifies_markdown_and_text`
4. `resource_preview_allowlisted_images_require_extension_and_signature`
5. `resource_preview_mismatched_or_unsupported_binary_is_metadata_only`
6. `resource_preview_truncated_binary_never_becomes_image`
7. `resource_preview_serialized_dto_never_includes_bytes`
8. `resource_preview_errors_are_opaque_without_machine_paths`
9. `resource_preview_updates_only_matching_project_recents`

### TypeScript tests (17)

- Gateway: invoke shape, DTO accept, reject bytes/extra, bounds, kind/MIME,
  opaque errors
- Model: markdown subset, HTML-as-text, image triple allow-list
- Renderer: text `<pre>`, semantic markdown, no unsafe HTML, image data URL,
  MIME mismatch → metadata, binary metadata, portable path only

## Integrity checklist

| Item | Result |
|------|--------|
| Whitelist violation | **None** intentional |
| Staging | **None** (`git diff --cached` empty) |
| Commit | **None** |
| Residual child process | Worker stops after controller captures completion |
| Scope expansion | **None** (no App/Workbench mount, no write/Git/terminal/MCP) |
| Machine-path leak | **None** in command errors (opaque messages) |
| Raw-byte transport | **None** (DTO omits `bytes`; parser rejects smuggled `bytes`) |
| Legacy Keeper (out of tree) mutation | **None** |
| Extra Rust/npm deps beyond base64 | **None** |

## Notes for controller / Codex2 review

1. Image extension paths that fail signature or are truncated are forced to
   `binary` metadata even if the payload is valid UTF-8 (e.g. `bad.webp`).
2. Only workspace `base64 = "0.22"` was added.
3. Mounting into the workbench is explicitly out of scope for C2-C3.
4. Controller should independently re-run the five required gates before
   acceptance and Codex2 protocol/security review.

## Completion handoff

Worker posts completion to controller session `<keeper-session>`
and awaits controller capture before releasing Keeper runtime lease.

---

## Codex2 C2-C3 Review Fix — image DTO boundary (Important)

**Controller session:** `<keeper-session>`
**Fix worker:** grok-c2c3-preview-fix
**Fix Keeper session:** `<keeper-session>`
**Severity closed:** Important — weak image `parseResourcePreview` acceptance
**Status:** Targeted correction only (unstaged). No stage / commit / host change.

### Finding

`parseResourcePreview` previously accepted an image DTO when media type and
`dataUrl` prefix agreed and the URL was under a loose 2 MiB character ceiling.
It did **not** require `truncated === false`, prove canonical RFC 4648 base64,
or prove that decoded bytes equal `bytesReturned`. That weakened the typed
renderer boundary even though the current Rust host produces valid payloads.

### Correction (whitelist only)

| Path | Change |
|------|--------|
| `apps/desktop/src/lib/tauriResourceGateway.ts` | Image branch: require `truncated === false`; `bytesReturned ∈ [1, 1_048_576]`; exact `dataUrl` length = `prefix + 4·ceil(bytesReturned/3)`; canonical standard base64 (alphabet, padding, decode, length match, re-encode equality). Removed loose 2 MiB `MAX_DATA_URL_CHARS`. |
| `apps/desktop/tests/resource-preview.test.tsx` | Added accept + explicit rejection cases for the hardened image contract. |
| `docs/execution/2026-07-31-wave-c2c3-preview-report.md` | This section. |

No other paths written. Rust host contract unchanged. Dirty tree preserved.

### Image branch rules (post-fix)

1. `truncated === false`
2. `bytesReturned > 0` and `bytesReturned <= 1_048_576`
3. Prefix derived from allow-listed `mediaType` only (`data:<mime>;base64,`)
4. Encoded payload length exact: `4 * Math.ceil(bytesReturned / 3)`
5. `dataUrl.length === prefix.length + expectedEncodedLength`
6. Canonical RFC 4648 standard base64 only:
   - standard alphabet (`A–Z a–z 0–9 + /`)
   - correct terminal padding
   - `atob` succeeds
   - decoded byte length equals `bytesReturned`
   - `btoa(decoded) === original payload` (rejects non-canonical pad bits)
7. Rejects URL-safe base64, whitespace, data-URL parameters, alternate MIME
   spelling, and MIME/prefix mismatch

### RED (conceptual / review finding)

Pre-fix gateway would accept:

- `truncated: true` image with matching prefix under 2 MiB
- Non-canonical / wrong-alphabet base64 under the loose ceiling
- `bytesReturned` disagreeing with decoded length when length stayed under 2 MiB
- Payloads under 2 MiB but longer than the exact 1 MiB-derived encoded bound

(Review-only finding; production host currently emits valid payloads.)

### GREEN evidence (gates re-run after fix)

```text
pnpm --filter @pillow/desktop exec vitest run tests/resource-preview.test.tsx
→ Test Files  1 passed (1)
→ Tests       18 passed (18)
exit 0

pnpm --filter @pillow/desktop typecheck
→ tsc --noEmit
exit 0

cargo test -p pillow-desktop resource_preview
→ test result: ok. 9 passed; 0 failed; 0 ignored; 0 measured; 72 filtered out
exit 0

cargo fmt --all -- --check
exit 0

git diff --check
exit 0
```

### Exact TypeScript tests (18)

Prior 17 remain green. New / extended coverage:

- **accept** small canonical PNG DTO (`iVBORw0KGgo=`, 8 bytes, `truncated: false`)
- **reject** `truncated: true` image
- **reject** invalid base64 alphabet (URL-safe `_`)
- **reject** non-canonical pad bits (`AB==` for 1 byte)
- **reject** invalid terminal padding count (`AAA=` for 1 byte)
- **reject** decoded length ≠ `bytesReturned` (`AAAA` with claim 2)
- **reject** payload under old 2 MiB ceiling but above exact 1 MiB-derived
  encoded limit (`bytesReturned: 1_048_576` with encoded length
  `maxEncoded + 4`)
- **reject** MIME / dataUrl prefix mismatch (`image/png` + `data:image/jpeg;…`)

### Integrity

| Item | Result |
|------|--------|
| Whitelist paths only | **Yes** (3 files) |
| Staging count | **0** (`git diff --cached` empty) |
| Commit | **None** |
| Rust host / mount / other tree | **Unchanged** |
| New dependencies | **None** (`atob`/`btoa` only) |
| Residual risk | Renderer-side model (`isAllowlistedImagePreview`) still does a lighter
  triple check for display; security boundary is the gateway parse. Host is
  trusted to keep emitting canonical payloads; gateway now fail-closes if it
  does not. Truncated image-extension paths remain host-classified as binary
  metadata, so they never reach this branch from a correct host. |

### Fix completion handoff

Post completion to controller session `<keeper-session>` and wait
for lease release. No stage or commit.
