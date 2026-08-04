# Wave C4 D3AR2 Controller Acceptance

Date: 2026-08-04

Verdict: **ACCEPTED**

## Accepted Boundary

D3AR2 repairs and accepts the C4 D3A process authority, output and event
integration boundary:

- runtime host consumes the governed process gateway instead of creating a
  second mutable SQLite authority;
- process approval grants remain host-private;
- supervised execution returns real bounded stdout/stderr and preserves
  Windows descendant cleanup guarantees;
- proposal requested, proposal decided, process started and terminal outcome
  events are fixed, validated, durable and project/conversation scoped;
- desktop Rust and TypeScript gateways recognize the same fixed public error
  and event vocabulary;
- the product UI is not yet mounted. That work belongs to D3B.

## Fresh Controller Evidence

- `pnpm check`: exit 0; 23 desktop test files and 272/272 tests, Rust workspace
  tests and doctests, and Clippy with `-D warnings`.
- `pnpm --filter @pillow/desktop build`: exit 0; production Vite build.
- `pnpm desktop:test:e2e`: exit 0; 8/8 real Tauri/WebView2 scenarios in
  711.8 seconds.
- `cargo fmt --all --check`: exit 0.
- `git diff --check`: exit 0; only line-ending conversion warnings were
  emitted.
- Relevant Pillow/E2E residual process count after the run: zero.
- Protected baseline before the checkpoint: `7b7f70f7360131cd1cfd984a8b19bdfb5725c21b`.

The first E2E controller invocation used a 10-minute total budget and was
terminated while the serial suite was beginning the project-launcher case.
Process timestamps proved that the preceding conversation-recovery case had
consumed approximately 8 minutes 47 seconds. Re-running the identical suite
with a 30-minute controller budget passed 8/8, so the initial termination was
a harness budget error rather than a product regression.

## Checkpoint Hygiene

Before staging, the controller scanned the complete candidate tree and
redacted machine-specific user/workspace paths plus ephemeral Keeper session
and lease identifiers from committed execution records. No credential or
bearer-token pattern was found. Ignored build caches and runtime logs are not
part of the checkpoint.

## Next Boundary

D3B may integrate the accepted process lifecycle into the conversation-centred
workbench through three reviewed slices: native command composition, typed
event/projection state, then progressive-disclosure UI mounting and real
desktop acceptance. D3B must not reopen process policy/executor authority or
introduce a second dashboard.
