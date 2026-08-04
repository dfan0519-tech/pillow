# Wave C4 D3A controller review

Date: 2026-08-04

Verdict: **REJECTED / REPAIR REQUIRED**

The controller reviewed the Grok1 runtime-host delivery and the Codex2 typed
gateway handoff against the accepted C4 design. Neither slice is accepted yet.
HEAD remains `7b7f70f7360131cd1cfd984a8b19bdfb5725c21b`; staging is empty.

## Grok1 Task 8 findings

1. `runtime-host` opens and mutates `SqliteExecProposalRepository` beside
   `GovernedProcessToolGateway`. The pointer-to-database-path registry and
   direct list/transition/recovery calls recreate the mutable storage bypass
   removed during D2BR2. Runtime host must consume only fixed, capability-bound
   gateway operations.
2. `force_live_process_expiry_for_tests` is exported on the ordinary product
   surface. The seam must be absent outside `cfg(test)` or a non-default
   `test-support` feature.
3. `ProcessOutputView` advertises chunks while every live entry stores an empty
   vector. C4 requires actual bounded stdout/stderr data; empty or synthetic
   output is not acceptance evidence.
4. The accepted design requires fixed validated process proposal, decision,
   start and terminal runtime-event kinds. The delivery explicitly leaves that
   protocol work undone.
5. Raw approval grants cross the public runtime-host DTO and decision method.
   The host must retain the opaque grant internally and expose only whether a
   decision is available.
6. Durable list/replay/recovery paths are not consistently project-capability
   and conversation bound, and the global pointer/path registry can become
   stale.

These are authority-boundary and product-contract defects, not documentation
polish. Focused green tests do not override them.

## Codex2 Task 10 status

Codex2 correctly returned `BLOCKED` rather than inventing a second process
authority or guessing a trusted Git executable. Its strict TypeScript DTO
normalizers and tests remain useful partial work, while the native RED anchor
is intentionally not accepted. Codex2 stays paused until D3AR supplies an
accepted RuntimeHost surface and the controller issues a concrete desktop Git
trust/generation composition decision.

## Controller decision

- Dispatch D3AR to the actual Grok slot assigned by the live Keeper lease with an expanded but exact
  allowlist covering the required protocol, execution, gateway and runtime-host
  seams.
- Do not re-dispatch Codex2 until D3AR passes controller source review and
  focused gates.
- D3B remains blocked.
- No stage, commit, reset, restore, clean, package or publish is authorized.
