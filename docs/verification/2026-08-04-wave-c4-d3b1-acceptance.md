# Wave C4 D3B1 Controller Acceptance

Date: 2026-08-04

Verdict: **ACCEPTED AS AN INTEGRATION FOUNDATION**

## Accepted Boundary

D3B1 establishes the reviewed process-governance integration seams without
claiming that the complete D3B workbench experience is mounted:

- native Tauri commands list process proposals, decide one proposal, and open
  host-retained bounded output through the existing conversation-owned
  `RuntimeHost` authority;
- renderer DTOs contain no approval grant, executable path, project root,
  database path, provider/session identity, or mutable authority;
- the four accepted durable process events cross the strict TypeScript gateway
  and project only fixed conversation-owned lifecycle notices;
- process output remains pull-on-demand through `ExecGateway` and is not copied
  into timeline activity;
- a deterministic pure selector chooses one visible write-or-process approval
  candidate by decision availability, active scope, expiry, opaque request id,
  and the fixed write-domain tie break.

The selector is deliberately not mounted yet. The single approval bar,
process activity row, terminal-output contextual-pane mode, project-switch
generation guards, and real process UI E2E remain D3B2 work.

## Controller Source Review

- Native commands derive the active project/conversation root from window
  capability and do not accept a renderer-supplied native root.
- `decide_exec_proposal` parses only `approve_once` or `reject` before reaching
  host authority.
- `portable_cwd` is serialized as `.` for the project root and otherwise as a
  portable relative path.
- Process event normalization rejects unknown fields, raw grants, native cwd,
  transcript/output fields, debug metadata, invalid enums, oversized argv, and
  oversized captured-byte metadata.
- Activity projection filters by both run and conversation and renders neither
  program/argv/cwd nor proposal/run/session identifiers.
- Approval arbitration returns presentation-safe scope and summary/risk data
  only; invalid expiries and foreign project/conversation candidates fail
  closed.

## Fresh Controller Evidence

- Focused renderer gate: 4 files, 45/45 tests passed.
- Desktop TypeScript typecheck: exit 0.
- Focused native process-command gate: 6/6 tests passed.
- `pnpm check`: exit 0; 24 desktop test files and 281/281 tests, Rust workspace
  tests and doctests, and Clippy with `-D warnings`.
- `pnpm --filter @pillow/desktop build`: exit 0; 73 modules transformed.
- `pnpm desktop:test:e2e`: exit 0; 8/8 real Tauri/WebView2 scenarios in
  772.2 seconds.
- `cargo fmt --all -- --check`: exit 0.
- `git diff --check`: exit 0; only line-ending conversion warnings were
  emitted.
- Relevant Pillow/E2E residual process count after the run: zero.
- HEAD remained `62149be9eddd2288eb45fd9b8278641cf64c7a5f`; no files were staged or
  committed by this acceptance pass.

## Next Boundary

D3B2 mounts these seams into the conversation-centred workbench. It must keep
one approval-bar slot across write and process domains, progressively disclose
command lifecycle and bounded output in the existing contextual pane, discard
late results on project/conversation switches, and add focused UI tests plus a
real Tauri process-governance E2E. It must not reopen process authority, add an
interactive shell, create a second dashboard, or begin Git mutation.
