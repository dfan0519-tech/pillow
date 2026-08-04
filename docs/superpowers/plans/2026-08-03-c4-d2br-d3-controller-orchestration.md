# Pillow C4 D2BR/D3 Controller Orchestration

Date: 2026-08-03

## Current gate

D1, D2A and D2B are accepted. D2BR2 closed the remaining process public-surface
and Git metadata no-follow defects; fresh controller focused and aggregate
gates passed. D3A runtime-host coordination and typed native/TS gateways are
now dispatched. D3B cannot begin until both D3A slices pass controller review.

No stage, commit, reset, restore, clean, package or publish is authorized.
HEAD remains `7b7f70f`; all accepted and in-flight work remains an unstaged
working-tree checkpoint.

## Wave D2BR - dispatched repair gate

| Owner | Development task | Exclusive product scope | Acceptance dependency |
| --- | --- | --- | --- |
| Actual Grok CLI `.grok2` | Process argument/capability/native-authority boundary repair | `crates/tool-executor` allowlist in the repair brief | Controller source review plus focused gateway/package/check/Clippy/fmt/diff gates |
| Actual Grok CLI `.grok4` | Git config parsing, fail-closed probes, reparse containment, diff generation contract | `crates/git-executor` allowlist in the repair brief | Controller source review plus real-repository/package/check/Clippy/fmt/diff gates |
| Codex Desktop controller | Contract ownership, conflict arbitration and acceptance | Planning/status/verification documents only | Both worker completion reports and fresh evidence |

The workers emit one completion report and one Keeper completion message. No
periodic conversation monitor is created. Completion is not acceptance.

## Wave D3A - authority and typed bridge

Starts only after D2BR acceptance.

| Owner | Development task | Planned ownership |
| --- | --- | --- |
| Actual Grok CLI `.grok1` | Task 8 runtime-host process coordination | `crates/runtime-host/Cargo.toml`, `crates/runtime-host/src/host/process.rs`, host module exports, runtime-host tests and its report |
| Actual account-2 Codex CLI (`codex2`, `gpt-5.6-terra`, high, `fastMode=false`) | Task 10 typed native/Tauri and TypeScript gateways | C4 native command/service modules, Tauri registration, typed TS gateways, contract tests and its report |
| Codex Desktop controller | Review provider/run ownership, cancellation ordering, DTO parity, privacy and stale-generation handling | No product implementation |

These two development tasks may run in parallel only after their briefs lock
non-overlapping paths and the accepted D2BR APIs. The controller reruns focused
Rust/TS tests and workspace/type/build gates before D3B.

## Wave D3B - mounted workbench and real acceptance harness

Starts only after D3A acceptance. Common entry points are serialized.

1. Actual Grok CLI `.grok3` performs Task 11: mount the accepted process/Git
   DTOs into the existing conversation-centred workbench and contextual pane.
   It owns the renderer integration files and race tests.
2. After the Task 11 product surface is source-accepted, actual Grok CLI
   `.grok4` performs Task 12 harness development: real Tauri/WebView2 adversarial
   scenarios and deterministic fixtures only. Product defects return to their
   owning worker; the harness may not weaken waits or cleanup.
3. Actual Grok CLI `.grok2` remains the first repair lane for process/Git
   boundary regressions; actual account-2 Codex CLI remains the repair lane for
   high-risk native/typed parity defects. Neither may edit another active
   worker's files.

## Controller-only final gate

After all D3 reports, the controller personally performs:

- source review against the C4 design and all repair contracts;
- focused Rust, TypeScript and renderer suites;
- `corepack pnpm check`;
- production desktop UI build;
- serial real desktop C4 E2E;
- `cargo fmt --all --check` and `git diff --check`;
- HEAD/staging/dirty-slice invariants;
- stable-ownership residual process proof and screenshot review at the locked
  viewports.

Only the controller may write the C4 acceptance/checkpoint verdict. A worker's
green report never advances the milestone by itself.
