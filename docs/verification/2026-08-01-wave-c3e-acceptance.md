# Pillow Wave C3-E Acceptance

Date: 2026-08-01

Verdict: ACCEPT

Scope: conversation-owned governed-write approval bar, bounded diff preview,
one-shot grant handling, terminal timeline projection and renderer/native DTO
validation.

## Controller Findings Closed

The initial delivery was rejected because raw grants could survive an
authoritative list refresh, expired proposals could leave a diff available,
caught host objects could reach logs, and the renderer DTO checks were too
permissive. The first repair closed those issues but was rejected again because
an already-constructed gateway error could preserve secret-bearing message text
and diff counters were bounded without proving that they described the exact
reviewed text.

The accepted implementation:

- retains the raw one-shot grant only in component memory and atomically drops
  it when the authoritative proposal is absent, terminal, revoked or expired;
- disables Approve, Reject and Review diff at expiry, closes the diff and
  re-lists through the native authority;
- maps every host/gateway error to fixed renderer copy and never logs caught
  error objects, paths, grants or tokens;
- validates closed proposal/diff DTOs, canonical 43-character base64url grants,
  finite timestamps, portable paths and public error codes;
- requires diff `byteCount` to equal the exact UTF-8 byte length and
  `lineCount` to match the native executor rule, including empty, trailing
  newline and Unicode cases;
- presents one progressive approval row in the owning conversation and opens
  the bounded diff in the existing contextual resource pane.

## Fresh Controller Evidence

- Focused C3-E Vitest command: 5 files, 45/45 passed.
- Full desktop Vitest: 21 files, 243/243 passed.
- `pnpm --filter @pillow/desktop typecheck`: passed.
- `pnpm --filter @pillow/desktop build`: passed; Vite production build
  transformed 73 modules.
- `git diff --check`: passed with only existing line-ending notices.
- HEAD remains `7b7f70f7360131cd1cfd984a8b19bdfb5725c21b`; staging is empty.

## Boundary

This acceptance freezes the C3-E renderer interaction and DTO boundary and
authorizes C3-F real Tauri/WebView2 integration acceptance. It does not yet
declare C3 complete and does not authorize terminal, Git, runtime expansion,
staging or commit.
