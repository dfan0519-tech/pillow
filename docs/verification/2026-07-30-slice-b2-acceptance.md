# Slice B2 Acceptance Report

Date: 2026-07-30
Branch: `integration/slice-b2`
Baseline: `0d7ea4f`
Accepted head before this report: `d2ab0a5`
Decision: PASS for fast-forward merge to `main`

## Scope

Slice B2 replaces the deterministic conversation runtime at the desktop
composition boundary with the first real local provider: Grok Build ACP over
stdio. The desktop defaults to Grok ACP. Fake mode remains available only
through explicit development/test selection.

The accepted B2 boundary is intentionally narrow:

- Real provider text conversations and streamed normalized events.
- Factual probe states before and after authenticated provider use.
- One in-flight provider prompt, cooperative cancellation, bounded escalation,
  and fail-closed process-tree cleanup.
- Project-specific canonical working directories.
- A session-level plaintext profile with no provider tools, no MCP servers,
  no terminal, and no file-system client capabilities.
- Redacted provider failures that cannot persist credentials, account identity,
  local profile paths, raw ACP envelopes, or provider session identifiers.

B2 does not grant Grok direct read, write, shell, web, MCP, memory, or subagent
tools. Project-aware code tools require a later sandboxed Keeper tool gateway.

## Integrated Packages

| Package | Integrated commit(s) |
| --- | --- |
| B2 research and execution briefs | `0569438` through `38413bd` |
| Production/test seam cleanup | `bfe917c` |
| Grok ACP adapter core | `1846e6e` |
| Desktop adapter injection and project scope | `13ccb02`, `ce6573a` |
| Probe, process, and native acceptance foundation | `01128d4`, `435a13b`, `689f036`, `786176d` |
| H5 project permission and runtime serialization | `71f9e54`, `c6be866` |
| H5/H6 Windows ownership and process evidence | `e70264a`, `2bac3d5`, `2d7899a` |
| H6 cancellation cleanup and redacted diagnostics | `5cd2049`, `097ed95`, `7c8b5fc` |
| H7 deterministic plaintext session profile | `5c01822`, `4e9eaff` |
| H7 cancellation error cleanup proof | `d880b21`, `d2ab0a5` |
| H7 public provider-error redaction | `f2f6878` |

## Automated Gates

| Gate | Result |
| --- | --- |
| `git diff --check` | PASS for the final integration worktree |
| `cargo fmt --all -- --check` | PASS |
| `cargo test -p keeper-adapter-grok-acp --locked` | PASS: 80 active tests; one explicit live test ignored by default |
| `cargo test -p keeper-ide-desktop --lib` | PASS: 47 tests |
| `pnpm acceptance:grok-acp:self-test` | PASS: 12 harness checks |
| `pnpm acceptance:grok-acp:dry-run` | PASS: result is `skipped`, never `passed` |
| `pnpm check` | PASS: structure, typecheck, 80 frontend tests, all Rust tests, and clippy with warnings denied |
| `node scripts/conversation-recovery-desktop.mjs` | PASS: real Tauri WebView IPC, 17 events restored, graceful exit |

Expected stderr from negative-path frontend tests was present. Every suite
completed successfully.

## Controlled Live Acceptance

The controller claimed the registered `grok3` runtime slot through Keeper,
used its isolated profile home without reading authentication files, and ran:

`pnpm acceptance:grok-acp:live`

The final H7 run passed:

- Real `grok-acp` adapter, never Fake.
- Provider version evidence accepted as the bounded version `0.2.112`.
- Probe before session: unavailable and not healthy.
- Probe after `session/new`: transport ready, authentication unverified.
- Probe after the first successful prompt: healthy.
- Deterministic plaintext turn completed with exactly one terminal event.
- Cooperative cancellation completed with exactly one `run_cancelled`.
- No provider tool activity or write permission was observed.
- Adapter shutdown succeeded.
- The owned provider PID/tree was proven gone.
- Global residual scan was clean.
- Keeper released the Grok runtime lease in `finally`.

The machine summary was:

`status=passed`, `mode=live`, `residualClean=true`, `elapsedMs=11849`.

An earlier run also passed the Rust adapter checks but the outer harness saw an
unrelated Grok process launched by another task during its global process scan.
No process was terminated or interfered with. A clean follow-up run passed all
outer checks.

Machine evidence remains in the gitignored local directory:
`artifacts/native-acceptance-grok-acp`.

## B1 Desktop Regression

The desktop recovery acceptance was rerun after H7. It explicitly selected
Fake mode for the deterministic B1 fixture and passed both create/stream and
restore phases:

- Real Tauri WebView IPC boundary.
- 17 persisted and restored runtime events.
- Completed run status.
- SQLite database persisted.
- Temporary WebView profile was populated.
- Both desktop processes exited gracefully.
- No residual desktop process remained.

The final run completed in 34,323 ms.

## Security and Product Boundaries

- Production launch argv remains fixed as `agent --no-leader stdio`.
- Session creation and load both inject the same Keeper-owned profile.
- The profile has `tools=[]`, a defensive disallowed-tool list, and
  `mcpServers=[]`.
- Client capabilities declare no terminal and no filesystem read/write.
- The adapter never opens or serializes authentication files.
- Child environment inheritance is allow-listed; credential-shaped parent
  variables and untrusted profile overrides are rejected.
- Cancellation that ends in an RPC/transport error must prove tree cleanup
  before it can be reported as cancelled.
- Cleanup failure emits a failed terminal event and clears connection,
  sessions, probe state, and active-turn state.
- Public error messages and codes are sanitized before event construction.
- Windows child creation is suspended until assignment to a kill-on-close Job
  Object succeeds.
- PID checks treat access-denied or ambiguous process-open errors as failure,
  not proof that a process is gone.

## Independent Review

The first final review identified three P1 blockers:

1. Provider-side read tools could execute without an ACP permission callback.
2. A transport error after cancellation could be reported as cancelled before
   process cleanup was proven.
3. Raw provider error text could carry credentials, identity, or local paths
   into persisted runtime events.

H7 closed all three:

- The production session profile now exposes no provider tools.
- Cancel-plus-error paths terminate and prove the provider tree before
  reporting cancellation; cleanup failure reports `TurnFailed`.
- Provider errors and unknown stop reasons are sanitized before event creation.

The independent re-review at `d2ab0a5` reported:

- Profile boundary: CLOSED.
- Cancellation cleanup proof: CLOSED.
- Public error redaction: CLOSED.
- Final verdict: NO P0/P1/P2.

## Non-Claims and Residual Risks

- B2 is a single-provider, single-prompt plaintext runtime. It is not yet a
  multi-provider router or process pool.
- B2 deliberately has no code-reading or code-writing tools. Those require a
  sandboxed Keeper-owned tool gateway with project-root enforcement.
- ACP session identifiers are not yet restored across desktop restarts.
- Authentication classification still depends on sanitized provider error
  patterns.
- Non-Windows descendant ownership has not received an equivalent live gate.
- The global process-name residual scan can report unrelated concurrent Grok
  work; owned Job/PID evidence remains the authoritative cleanup proof.
