# Slice B2 Acceptance Report

Date: 2026-07-30
Branch: `integration/slice-b2`
Baseline: `0d7ea4f`
Accepted head before this report: `786176d`
Decision: HOLD pending H5 industrial hardening

## Scope

Slice B2 replaces the deterministic conversation runtime at the desktop
composition boundary with the first real local provider: Grok Build ACP over
stdio. The desktop now defaults to Grok ACP. Fake mode remains available only
through an explicit development/test selection.

This slice includes:

- A bounded `keeper-adapter-grok-acp` crate with NDJSON JSON-RPC framing.
- ACP initialize, session creation, text turns, permission handling,
  cooperative cancellation, terminal event mapping, and shutdown.
- Windows child-tree ownership through a kill-on-close Job Object.
- Project-scoped canonical working-directory resolution before runtime spawn.
- Factual probe states: unavailable, transport ready/auth unverified, healthy,
  and authentication failed.
- Desktop composition wiring with one concurrent real prompt.
- A controlled native acceptance harness with opt-in live semantics, redacted
  evidence, process-residual checks, and a strict SKIP-not-PASS contract.

## Integrated Packages

| Package | Integrated commit |
| --- | --- |
| B2 research and execution briefs | `0569438` through `38413bd` |
| B2-0 production/test seam cleanup | `bfe917c` |
| Grok ACP adapter core | `1846e6e` |
| Desktop adapter injection | `13ccb02` |
| H1 adapter hardening | `01128d4` |
| H2 project-scoped desktop wiring | `ce6573a` |
| H3 native acceptance harness | `435a13b` |
| H4 runtime profile and probe truth | `689f036` |
| Acceptance schema correction | `786176d` |

## Automated Gates

| Gate | Result |
| --- | --- |
| `git diff --check` | PASS |
| `cargo fmt --all -- --check` | PASS |
| `cargo test -p keeper-adapter-grok-acp` | PASS: 48 active tests; live test ignored by default |
| `cargo test -p keeper-ide-desktop --lib` | PASS: 47 tests |
| `pnpm acceptance:grok-acp:self-test` | PASS: 12 harness checks |
| `pnpm acceptance:grok-acp:dry-run` | PASS: status is `skipped`, never `passed` |
| `pnpm check` | PASS: workspace structure, typecheck, 80 frontend tests, all Rust tests, clippy with warnings denied |
| `node scripts/conversation-recovery-desktop.mjs` | PASS: real Tauri WebView IPC, 17 events restored, graceful exit |

The expected stderr from negative-path frontend tests was present, but every
test suite completed successfully.

## Controlled Live Acceptance

The controller claimed the registered `grok3` runtime slot and ran:

`pnpm acceptance:grok-acp:live`

The first run proved the Rust adapter path but exposed a stale Node harness
assertion that still expected the old single `probe.healthy` field. The
controller corrected the validator to require the full factual probe ladder
and added a positive self-test for that schema.

The second run passed:

- Real `grok-acp` adapter, never Fake.
- Probe before session: unavailable and not healthy.
- Probe after `session/new`: transport ready, authentication unverified.
- Probe after the first successful prompt: healthy.
- One read-only turn with exactly one `turn_completed`.
- One cooperative cancellation with exactly one `run_cancelled`.
- No write permission observed.
- Adapter shutdown succeeded.
- No new Grok or Keeper process remained.
- Redacted result passed the committed secret scanner.

Machine evidence is stored in the gitignored local artifact directory:
`artifacts/native-acceptance-grok-acp`.

## B1 Regression

The desktop recovery acceptance was rerun after Grok became the production
default. The acceptance explicitly selected Fake mode for its deterministic B1
fixture and passed both create/stream and restore phases:

- Real Tauri WebView IPC boundary.
- 17 persisted and restored runtime events.
- Completed run status.
- SQLite database persisted.
- Both desktop processes exited gracefully.
- No residual desktop process remained.

## Security and Product Boundaries

- The adapter receives only an executable path and a process-home path. It does
  not open or serialize authentication files.
- The child environment is allow-listed; credential-shaped parent variables
  are not inherited.
- Unknown/write permission requests fail closed in this slice.
- Raw ACP envelopes and provider-specific fields do not enter Keeper runtime
  events or acceptance reports.
- Missing executable/profile configuration fails truthfully and never falls
  back to Fake.

## Non-Claims and Residual Risks

- B2 is a single-provider, single-prompt production path. It is not yet a
  multi-provider router or process pool.
- ACP session identifiers are not yet restored across desktop restarts.
- Write-capable tool approval UI is not part of B2.
- Authentication classification still depends on sanitized provider error
  patterns.
- On Windows, the process is assigned to the Job Object immediately after
  spawn; there remains a small theoretical spawn-to-assignment race.
- Non-Windows descendant ownership has not received an equivalent live gate.

## Independent Review

The independent review found no P0 issue, but identified four P1 blockers and
one P2 contract weakness:

1. Read/search permission was based on tool kind without proving the target is
   inside the active project root.
2. The ACP client had one global event sink; a concurrent session create/load
   could detach the sink of an active prompt.
3. `cancel_grace` was configured but not enforced with process-tree escalation.
4. Windows launched the child before assigning it to the Job Object, leaving a
   small descendant escape window; H3 residual evidence was also name-based.
5. Test-only `extra_env` could override isolated-home variables.

These findings supersede the provisional PASS from functional and live gates.
Slice B2 must not merge to `main` until H5 closes the P1 findings, reruns all
non-live gates, and passes a fresh controlled live acceptance.
