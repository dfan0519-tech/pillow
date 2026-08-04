# Pillow Wave C3-F Native Path Privacy Fix Acceptance

Date: 2026-08-01

Verdict: ACCEPT

Scope: remove native project roots from sidebar markup and restore the strict
C3-F full-DOM privacy gate.

## Controller Finding Closed

The preceding harness delivery was rejected because it treated
`title={project.rootPath}` as an allowed historical exception and stopped
checking host-path prefixes in full body HTML. That contradicted the C3-F
accessibility/privacy gate.

The corrected implementation replaces the raw-root tooltip with the project
name, preserves selection and layout, adds a sentinel native-root DOM test and
again rejects native path prefixes in full body markup. Portable relative
resource paths remain in scope and are not suppressed.

## Fresh Controller Evidence

- Focused sidebar and visual-shell Vitest: 2 files, 21/21 passed.
- `node --check scripts/lib/c3-governed-write-acceptance.mjs`: passed.
- Desktop TypeScript check: passed.
- Desktop production build: passed; 73 modules transformed.
- Whitelist `git diff --check`: passed.
- HEAD remains `7b7f70f7360131cd1cfd984a8b19bdfb5725c21b`; staging is empty.

## Boundary

This acceptance closes only the native-path privacy rejection. C3-F remains
active pending the separate cancel-resolution fix and fresh real
Tauri/WebView2 acceptance. It does not authorize staging or commit.
