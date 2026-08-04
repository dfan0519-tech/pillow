# Wave C3-F Native Path Privacy Fix — Report

**Date**: 2026-08-01
**Worker session**: `<keeper-session>`
**Runtime**: `grok1` / `<keeper-lease>`
**Controller session (completion target)**: `<keeper-session>`
**Status**: `COMPLETION: DONE`

## Rejection addressed

A prior C3-F resume delivery weakened the privacy harness so absolute host-path
tokens were no longer checked against full body HTML, citing the existing
sidebar `title={project.rootPath}` tooltip. That is a product privacy defect:
native project roots must not appear in screenshots or accessibility text
(including `title` attributes).

## Contract delivered

1. Removed raw native `project.rootPath` from sidebar DOM text and attributes.
2. Project selection and visible project name preserved; tooltip is name-only.
3. C3-F `assertPrivacyClean` again rejects host-path prefixes in full body HTML.
4. No change to portable relative paths on resource/diff surfaces.
5. No DTO, native command, persistence, or layout changes.

## TDD evidence

### RED

Focused Vitest (before renderer fix):

```text
FAIL  tests/project-workbench.test.tsx
  > does not leak native project rootPath into sidebar DOM text or attributes

Expected the element to have attribute:
  title="Privacy Sentinel"
Received:
  title="F:\tmp\c3f-native-privacy-sentinel\project-alpha"
```

Sentinel root: `F:\tmp\c3f-native-privacy-sentinel\project-alpha` leaked via the
project-row `title` attribute into rendered DOM markup.

### GREEN

| Change | Path |
|--------|------|
| Name-only tooltip | `apps/desktop/src/features/projects/AppSidebar.tsx` — `title={project.name}` |
| Sentinel privacy Vitest | `apps/desktop/tests/project-workbench.test.tsx` |
| Restore full-body host-path HTML check | `scripts/lib/c3-governed-write-acceptance.mjs` |

`assertPrivacyClean` now asserts both visible text and full body HTML do not
contain `FORBIDDEN_HOST_PATH_TOKENS` (`file://`, `F:\`, `C:\`, `/Users/`,
`/home/`). The incorrect “project-row title exception” comment was removed.

## Gates

| Gate | Result |
|------|--------|
| Focused Vitest `project-workbench` + `visual-shell-contract` | **21/21 PASS** |
| Privacy test alone after fix | **PASS** |
| `node --check scripts/lib/c3-governed-write-acceptance.mjs` | **PASS** (exit 0) |
| Desktop `pnpm run typecheck` | **PASS** |
| Desktop `pnpm run build` | **PASS** (vite production) |
| Whitelist `git diff --check` | **PASS** (exit 0; LF/CRLF warning only on unrelated tracked visual-shell file) |
| HEAD | **unchanged** `7b7f70f7360131cd1cfd984a8b19bdfb5725c21b` |
| Staging | **empty** |

## Whitelist files touched

- `apps/desktop/src/features/projects/AppSidebar.tsx`
- `apps/desktop/tests/project-workbench.test.tsx`
- `scripts/lib/c3-governed-write-acceptance.mjs`
- `docs/execution/2026-08-01-wave-c3f-native-path-privacy-fix-report.md`

`visual-shell-contract.test.tsx` was **not** modified for this fix.

## Non-goals / out of scope

- No stage, commit, reset, restore, package, or publish.
- No runtime, native commands, storage, policy, executor, write-approval UI,
  other C3-F harness files, or fake ACP fixtures.
- Portable project-resource relative paths remain visible where intended.

## Residual risk

Full C3-F WebView2 acceptance orchestrator was not re-run in this worker
session (whitelist-bound renderer/harness unit fix). The restored
`assertPrivacyClean` full-body HTML host-path check will fail the live
acceptance if any other surface still embeds absolute roots; current product
leak was the sidebar `title={project.rootPath}` only.

## COMPLETION: DONE
