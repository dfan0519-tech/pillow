# Wave C2-C3 Preview Codex2 Review Fix Brief

**Controller session:** `<keeper-session>`
**Source finding:** Codex2 C2-C3 independent review, Important severity
**Repository:** `<workspace>\pillow`
**Status:** targeted correction only; no stage or commit

## Finding to Close

`parseResourcePreview` currently accepts an image DTO when the media type and
`dataUrl` prefix agree and the URL is below a loose 2 MiB ceiling. It does not
require `truncated === false`, prove canonical base64, or prove that decoded
bytes equal `bytesReturned`.

This weakens the typed renderer boundary even though the current Rust host
produces valid payloads.

## Exclusive Write Whitelist

Modify only:

- `apps/desktop/src/lib/tauriResourceGateway.ts`
- `apps/desktop/tests/resource-preview.test.tsx`
- `docs/execution/2026-07-31-wave-c2c3-preview-report.md`

All other paths are read-only. Preserve the dirty tree. Do not stage, commit,
reset, revert, mount the component, or modify the Rust host.

## Required Correction

In the `kind === "image"` parser branch:

1. require `truncated === false`;
2. require `bytesReturned > 0` and `bytesReturned <= 1_048_576`;
3. derive the expected prefix from the allow-listed `mediaType`;
4. derive the exact encoded payload length as
   `4 * Math.ceil(bytesReturned / 3)`;
5. require `dataUrl.length === prefix.length + expectedEncodedLength`;
6. reject a payload unless it is canonical RFC 4648 standard base64:
   - standard alphabet only;
   - correct terminal padding;
   - decoding succeeds;
   - decoded byte length equals `bytesReturned`;
   - re-encoding the decoded binary equals the original payload.

Do not add a dependency. Browser `atob`/`btoa` or an equivalent pure local
validator may be used. Do not accept URL-safe base64, whitespace, data URL
parameters, alternate MIME spelling, or non-canonical pad bits.

Replace the loose 2 MiB bound with the exact 1 MiB-derived bound. The host
contract remains unchanged.

## Required Tests

Keep every existing test green and add explicit rejection cases for:

- `truncated: true` image;
- invalid base64 alphabet;
- invalid/non-canonical padding or pad bits;
- decoded length not equal to `bytesReturned`;
- a payload below the old 2 MiB ceiling but above the exact 1 MiB-derived
  encoded limit;
- MIME/prefix mismatch remains rejected;
- one valid small canonical image DTO remains accepted.

## Required Gates

```powershell
pnpm --filter @pillow/desktop exec vitest run tests/resource-preview.test.tsx
pnpm --filter @pillow/desktop typecheck
cargo test -p pillow-desktop resource_preview
cargo fmt --all -- --check
git diff --check
```

Append the correction, RED/GREEN evidence, exact tests, staging count, and
residual risk to the existing worker report. Post a Keeper completion message
to the controller and wait for lease release.
