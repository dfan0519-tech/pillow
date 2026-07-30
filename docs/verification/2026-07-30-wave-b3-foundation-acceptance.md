# Wave B3 Foundation Acceptance

Date: 2026-07-30

Status: Accepted foundation; executor work remains

Controller baseline: `1fea00a`

Integrated main: pending final acceptance commit

## Scope Accepted

This acceptance covers the security and presentation foundation for governed
read tools:

- project roots are authorized by the native host and scoped to a project,
  conversation, window instance, and current project generation;
- stale picker completions, window-label reuse, project switches, and delayed
  conversation binds fail closed;
- unsafe pathname-based new-folder creation is disabled and its renderer entry
  is removed;
- `fs.list@1`, `fs.read@1`, and `fs.search@1` have provider-neutral manifests,
  strict wire parsing, normalized argument digests, call reservation identity,
  proposal fingerprints, and opaque validated results;
- protocol results are bound to the owning proposal, requested resource scope,
  and per-call limits;
- the renderer retains activity per run, projects eventless durable runs,
  validates runtime event payloads, bounds text and collection cardinality,
  and keeps resources isolated by project;
- long timelines use explicit stable pagination instead of fixed-height
  virtualization.

## Explicit Non-Claims

This acceptance does not claim that Keeper can execute the three read tools.
The following still belong to the next B3 implementation package:

- a Keeper-owned read-only executor;
- atomic durable call reservation and replay lookup;
- execution-time filesystem object revalidation;
- cancellation, timeout, and byte/count enforcement against real files;
- SQLite persistence for tool transitions and audit receipts;
- provider return plumbing for bounded tool results.

Provider-native tools and MCP remain disabled. No shell, write operation,
browser control, network tool, credential lookup, Obsidian integration, or
autonomous project operation was added.

## Reference Repository Decision

`<workspace>/zbbody` is registered with Keeper as `VK-057` and remains a
read-only reference project in lifecycle state `watch`.

The reference was useful for interaction and lifecycle patterns, especially
the three-pane workbench, run activity disclosure, and resource navigation.
Its renderer-owned local backend, broad IPC, arbitrary MCP configuration,
permission bypasses, and provider-controlled execution patterns were not
adopted.

## Worker Record And Controller Evaluation

### Hypatia - Path Capability Boundary

Delivered the native path authority model and two hardening passes.

Controller evaluation: the first implementation was correctly rejected for
conversation reuse, Windows replacement risk, native recovery regression, and
orphan ordering. The accepted version removes unsafe folder creation and closes
the delayed picker, window reuse, project generation, and conversation bind
races with deterministic tests.

Accepted commits:

- `2a4179e`
- `512ac8b`
- `0273f37`

### Ohm - Provider-Neutral Tool Contract

Delivered fixed read-tool manifests, safe wire parsing, reservation semantics,
proposal/result binding, and adversarial protocol tests.

Controller evaluation: two review cycles found real replay, result-scope, and
diagnostic leakage defects. The final contract separates call identity from
proposal fingerprint, returns opaque validated evidence, rejects conflicting
replays, and prevents provider-controlled Serde text from entering operational
errors.

Accepted commits:

- `e150ed5`
- `9046a2b`
- `3e8fc72`
- `3d3af98`
- `0e42803`

### Kuhn - Durable Run Activity Projection

Delivered per-run projection, stable timeline pagination, bounded output and
collection budgets, strict runtime event validation, and resource-pane
navigation.

Controller evaluation: the initial fixed-height windowing and later pagination
implementation were rejected because they could skip content, retain
unbounded activity, accept malformed payloads, or briefly show a prior
project's resource. The accepted version closes those boundaries and keeps the
UI quiet and project-scoped.

Accepted commits:

- `1207c80`
- `fc816ca`
- `d2e9e50`

## Integrated Verification

Executed from `<workspace>/VaultSystem\keeper-ide` after all branches were merged:

- `git diff --check`: passed
- `cargo fmt --all -- --check`: passed
- `pnpm check`: passed
  - desktop Vitest: 11 files, 100 tests
  - Rust workspace: all non-opt-in tests passed
  - live Grok ACP test: one expected opt-in ignore
  - Clippy workspace with `-D warnings`: passed
- `pnpm --filter @keeper-ide/desktop build:ui`: passed
- `pnpm --filter @keeper-ide/desktop test:e2e`: 5 of 5 passed serially
  - persistence-before-emit, cancellation, and interrupted recovery
  - real Tauri restart and SQLite timeline restore
  - native project create/select/restore
  - desktop readiness and process-tree cleanup
  - Cargo wrapper signal cleanup

Two integration-only test repairs were required:

- removed stale `onNewProject` props after the unsafe feature was removed;
- changed a JSON fixture assertion from newline-sensitive text comparison to
  structural JSON comparison for Windows CRLF compatibility.

## Residual Risk And Next Gate

The next package must implement the executor behind the accepted contracts.
It must preserve `validate -> persist -> commit -> emit`, use SQLite as the
only operational authority, perform execution-time object revalidation, and
prove read-only behavior on Windows. B3 is not complete until real read
operations and durable receipts pass those gates.
