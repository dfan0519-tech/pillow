# Wave C3 Research Brief — grok3 / Security And Architecture

Role: read-only governed-write architecture researcher.

## Startup

1. Work from `this repository`.
2. POST Keeper boot with project `pillow`, task
   `c3-research-governed-write-security`, agentName `grok3`, agentRole
   `worker`.
3. Read `root AGENTS.md`, the shared vault `AGENTS.md`, and
   `this repository\AGENTS.md`.

## Scope

Inspect the current dirty Pillow working tree without modifying it. Focus on
`agent-protocol`, `tool-executor`, `storage-sqlite`, `runtime-host`, the Tauri
conversation service, and current renderer activity/resource contracts.

Use `legacy Keeper (out of tree)` only as a frozen read-only reference for
`operator-approval-authority` and related tests. Port behavior through new
typed contracts; do not propose copying the legacy implementation wholesale.

Evaluate three C3 approaches:

1. whole-file replacement after approval;
2. canonical patch proposal with base-hash compare-and-swap;
3. staged shadow-workspace transaction.

Assume the recommended first release may create or modify bounded UTF-8 text
files, but must exclude rename, delete, binary writes, directory mutation,
terminal, Git, network, credentials, Obsidian and new runtimes unless the
evidence strongly contradicts that boundary.

## Required Threat Analysis

- project and conversation ownership;
- proposal identity, replay and conflicting payloads;
- approval expiry, rejection, cancellation and restart recovery;
- path traversal, absolute/UNC/device paths, reserved names and ADS;
- symlink/junction/reparse and check/use races;
- base hash drift, partial writes, crash recovery and atomic replacement;
- renderer/provider authority bypass;
- bounded diff size, encoding and secret/debug leakage;
- durable append-only receipts and opaque UI errors.

## Output Contract

Write only:

`this repository\docs\research\2026-08-01-c3-governed-write-security-research.md`

The report must contain: current architecture map with exact local paths/lines,
three-option comparison, recommended contract and state machine, threat/control
matrix, proposed test matrix, task-boundary recommendations, residual risks,
and `COMPLETION: DONE`.

Do not modify code, other documentation, Git staging, or commits. Do not write
anything under the frozen Keeper product directory.
