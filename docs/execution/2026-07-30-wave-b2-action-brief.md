# Keeper IDE Wave B2 Action Brief

Date: 2026-07-30

Status: Controller execution baseline

## Goal

Connect one real Grok Build runtime through the existing runtime-neutral
conversation kernel without weakening project isolation, persistence,
cancellation, shutdown, or secret boundaries.

## Product Truth

- Slice B1 is accepted on `main` at `0d7ea4f`.
- The current runtime is still the deterministic fake adapter.
- Grok Build `0.2.112` is installed locally and exposes `grok agent stdio`.
- Legacy Keeper contains a JavaScript ACP client that is reference material
  only. It is not successor code and cannot be copied wholesale.
- A successful single prompt is not B2 acceptance. Streaming, cancellation,
  restart truth, process cleanup, diagnostics redaction, and project isolation
  are release gates.

## Locked Architecture

```text
ConversationService
  -> RuntimeHost
  -> AgentAdapter trait
  -> Grok ACP adapter
  -> supervised grok agent stdio child
```

The adapter owns provider-specific JSON-RPC and session identifiers. The host
and renderer receive only normalized Keeper events.

## Work Waves

### B2-R: protocol and boundary research

Three read-only Grok lanes produce:

1. a sanitized live protocol transcript and capability matrix;
2. an exact B1-to-ACP mapping and implementation file plan;
3. a threat model, process supervision plan, and acceptance matrix.

No product code is modified during B2-R.

### B2-1: adapter core

- create `crates/adapter-grok-acp`;
- implement bounded JSON-RPC line framing;
- initialize, session new/load, prompt, cancel, shutdown, diagnostics;
- normalize supported ACP updates into the fixed Keeper vocabulary;
- reject malformed, oversized, out-of-order, secret-bearing, or unknown
  provider payloads at the adapter boundary;
- use fixture and fake-child tests before live execution.

### B2-2: launch and profile isolation

- supervised `grok agent --no-leader stdio` child process;
- explicit executable and project cwd validation;
- allow-listed environment with isolated HOME, USERPROFILE, APPDATA, and
  LOCALAPPDATA supplied by a credential/profile resolver below the renderer;
- no credential or account contents enter SQLite, logs, diagnostics, or UI;
- process-tree cancellation and shutdown remain bounded.

### B2-3: application integration

- persist a project-scoped runtime connection by opaque profile reference;
- expose probe and connection health through typed Tauri commands;
- permit explicit Fake or Grok selection; never silently fall back;
- start and resume real Grok sessions through the existing conversation
  service;
- show a truthful runtime badge and actionable connection errors.

### B2-4: live acceptance

- real Grok probe and one harmless project-scoped prompt;
- real streaming event projection;
- interactive cancellation with one terminal state;
- app restart with durable Keeper history and truthful provider resume or
  `Interrupted`;
- two projects remain isolated;
- shutdown leaves no Grok or Keeper child process and no listener residue;
- redacted diagnostic bundle contains no tokens, account identity, raw
  environment, or private session payloads.

## Immediate Engineering Cleanup

Before integration, isolate B1 test fault seams behind `cfg(test)` or a
dedicated non-default test-support feature. Update README status so it no
longer claims that runtime and UI are absent.

## Non-Goals

- Codex or OpenCode adapters;
- model routing or hidden provider fallback;
- provider account management UI;
- API key storage;
- Obsidian integration;
- file editor, terminal, Git, or write-capable tool approvals;
- autonomous company roles.

## Controller Gates

1. Every code lane has disjoint write scope and focused tests.
2. Adapter fixtures are sanitized and committed.
3. No credentials, account identities, machine auth paths, or provider raw
   payloads enter durable Keeper state.
4. All new source files remain at most 500 lines.
5. Typecheck, renderer tests, Rust workspace tests, Clippy, native E2E, release
   smoke, process cleanup, and independent review pass.
6. Main advances only by controller-reviewed fast-forward.

