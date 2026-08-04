# Pillow Wave C3-D Acceptance

Date: 2026-08-01

Verdict: ACCEPT

Scope: provider `fs.write@1` transport, run-scoped governed-write coordination,
durable proposal lifecycle, typed native commands, restart reconciliation and
provider permission denial.

## Controller Findings Closed

The first delivery was rejected because applying-state recovery, expiry,
polling, drop cleanup, replay, window authority and adapter evidence were not
yet strict enough. The first repair was rejected again because persistence
failures could fabricate terminal state, an unresolvable applying root could be
treated as interrupted, restart listing depended on an active run binding, a
direct diff could outlive expiry, and the full-workspace Clippy gate was not
green.

The accepted implementation:

- publishes a terminal proposal only after that exact terminal row is durably
  verified; storage failures quarantine all live payload/grant authority and
  return an opaque gateway error;
- keeps an applying row unchanged when its project root cannot be resolved;
- lists durable proposals through the host-owned write gateway after restart,
  without requiring an active run;
- reconciles expiry before direct diff access and wakes only the owning waiter;
- releases governed read and write owner state on host drop;
- binds native commands to the selected window, project and conversation;
- denies provider-native write permission and proves approve, reject and
  provider round trips with real fake-ACP fixtures;
- keeps proposal content and raw grants memory-only and never returns native
  roots or mutable digests to the renderer.

## Fresh Controller Evidence

- `cargo test -q -p keeper-runtime-host --features test-support --tests --lib
  -- --test-threads=1`: passed, including 17 governed-write bridge cases.
- `cargo test -q -p keeper-tool-executor --tests --lib`: passed.
- `cargo test -q -p keeper-storage-sqlite --tests --lib`: passed, including
  the 25-case write-proposal repository suite.
- `cargo test -q -p keeper-adapter-grok-acp --tests --lib`: passed, including
  the governed-write round trip.
- `cargo test -q -p pillow-desktop --lib write_`: 5 passed.
- `cargo test -q -p keeper-resource --tests --lib`: passed.
- `cargo check -q --workspace`: passed.
- `cargo clippy -q --workspace --all-targets -- -D warnings`: passed.
- `cargo fmt --all -- --check`: passed.
- `pnpm check`: exit 0; renderer tests, Rust workspace/doctests and Clippy
  completed successfully; desktop Rust reported 85/85 and project commands
  11/11 in this run.
- `pnpm --filter @pillow/desktop build`: exit 0; Vite production build completed
  with 68 modules.
- `git diff --check`: passed with only existing line-ending notices.
- HEAD remains `7b7f70f`; staging is empty.

## Boundary

This acceptance freezes the C3-D DTO/runtime/native contract and authorizes
C3-E conversation approval/diff UI implementation. It does not make C3 a
completed product capability and does not authorize terminal, Git, runtime
expansion, staging or commit.
