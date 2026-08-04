# Wave B3 Governed Read Execution Acceptance

Date: 2026-07-31

Status: Accepted kernel boundary; provider round trip remains

Controller baseline: `7b7f70f`

Integrated main: pending acceptance commit

## Scope Accepted

This package turns the accepted B3 contracts into a Pillow-owned read
execution kernel:

- `keeper-tool-executor` implements only `fs.list@1`, `fs.read@1`, and
  `fs.search@1`;
- every path is project relative, revalidated against the canonical project
  root at execution time, and rejected when a link or Windows reparse point
  is encountered;
- directory results are deterministic, reads are byte bounded, search lines
  and previews are memory bounded, and all operations check cancellation and
  deadlines;
- JSON expansion beyond a manifest output cap becomes the canonical
  `output_limit_exceeded` public result rather than an invalid or oversized
  receipt;
- SQLite migration `0003_tool_calls.sql` owns the call reservation, proposal
  fingerprint, lifecycle history, execution lease, and terminal receipt;
- one active lease owner may execute a call, expired leases may be recovered,
  and only the current owner may commit the terminal receipt;
- same-call retries replay the exact persisted terminal result; changed
  arguments under the same call identity are rejected;
- the governed gateway persists `proposed -> validated -> executing ->
  terminal` before returning a result to its caller.

## Security Boundary

No file mutation, patch, rename, delete, shell, terminal, process launch,
network request, browser control, credential lookup, arbitrary MCP tool, or
provider-native tool was enabled.

The executor does not accept raw provider payloads. It accepts only the opaque
`ValidatedToolProposal` produced by `keeper-agent-protocol`, and it returns only
proposal-bound `ValidatedToolResult` values with canonical public errors.

## Focused Verification

The package adds:

- 8 real-filesystem executor tests;
- 4 governed gateway and replay tests;
- 7 SQLite reservation, migration, lifecycle, lease, and receipt tests.

Covered cases include deterministic list/search order, UTF-8 and binary-safe
bounded reads, symlink/reparse rejection where the operating system permits
the fixture, project mismatch, cancellation, oversized JSON output, long-line
search bounds, call-id conflicts, interrupted execution recovery, active lease
exclusion, terminal replay, invalid transitions, and missing durable context.

## Integrated Verification

Executed from `<workspace>\pillow` after the final lease and resource-boundary pass:

- `git diff --check`: passed;
- `cargo fmt --all -- --check`: passed;
- `pnpm check`: passed;
  - desktop Vitest: 11 files, 100 tests;
  - Rust workspace: all non-opt-in tests passed;
  - live Grok ACP: one expected opt-in ignore;
  - Clippy workspace with `-D warnings`: passed;
- `pnpm --filter @pillow/desktop build:ui`: passed;
- `pnpm --filter @pillow/desktop test:e2e`: 5 of 5 passed serially;
  - persistence-before-emit, cancellation, and interrupted recovery;
  - real Tauri restart and SQLite timeline restore;
  - native project create/select/restore;
  - desktop readiness and process-tree cleanup;
  - Cargo wrapper signal cleanup.

No Pillow, Cargo, or Rust compiler process remained after the desktop gate.

## Explicit Non-Claims

This acceptance does not claim that the current Grok conversation adapter can
invoke these tools. Provider-native ACP tools remain disabled. The next B3
package must add the controlled provider round trip:

1. expose only Pillow's fixed read manifests to an eligible run;
2. normalize a provider request into `ToolProposal`;
3. invoke `GovernedReadToolGateway`;
4. return the bounded committed result to the same provider turn;
5. project the persisted tool lifecycle and receipt into the owning
   conversation timeline.

That package must preserve the current one-executor lease, proposal
fingerprint, project capability, persistence-before-return, and secret-safe
error boundaries.
