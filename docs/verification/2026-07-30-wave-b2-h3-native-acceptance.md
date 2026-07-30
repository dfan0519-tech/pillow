# Wave B2-H3 Controlled Native Acceptance Harness

Date: 2026-07-30
Package: **H3 — harness only**
Branch: `wave-b2-h3-native-acceptance`

## Purpose

Provide an explicit opt-in harness that exercises real Grok ACP native
acceptance **only** when a controller enables it with non-secret configuration.
Normal CI and normal `cargo test --workspace` must **SKIP** the live path and
must never claim **PASS**.

This package builds the harness, self-tests, dry-run SKIP proof, and report
templates. **It does not run a billable/live Grok prompt in the worker session.**

## Deliverables

| Path | Role |
| --- | --- |
| `scripts/native-acceptance-grok-acp.mjs` | Harness entry (`--dry-run`, `--live`, `--self-test`) |
| `scripts/lib/native-acceptance-grok-core.mjs` | Opt-in parsing, result/report builders, evidence validation |
| `scripts/lib/grok-process.mjs` | Windows Grok/Keeper PID baseline, final, residual math |
| `scripts/lib/secret-scan.mjs` | Secret / raw-envelope pattern scan for harness output |
| `crates/adapter-grok-acp/tests/live_native_acceptance.rs` | `#[ignore]` live test + opt-in gate unit test |
| `docs/verification/2026-07-30-wave-b2-h3-native-acceptance.md` | This report template |
| `package.json` scripts | `acceptance:grok-acp*` |

## Opt-in contract (non-secret env)

Live execution requires **all** of:

| Variable | Meaning |
| --- | --- |
| `KEEPER_GROK_ACP_E2E=1` | Explicit live enable |
| `KEEPER_GROK_EXECUTABLE` | Path to Grok CLI binary |
| `KEEPER_GROK_ISOLATED_HOME` | Isolated profile home directory |
| `KEEPER_GROK_PROJECT_ROOT` | Project working directory for `session/new` |

Optional:

| Variable | Meaning |
| --- | --- |
| `KEEPER_GROK_ACCEPTANCE_RESULT` | Redacted adapter evidence JSON path |
| `KEEPER_GROK_ACCEPTANCE_OUT` | Harness `result.json` + `report.md` directory |

Paths are configuration references. The harness **never** prints profile
contents, account identity, raw ACP envelopes, tokens, or auth files.

## SKIP semantics

| Invocation | Expected status |
| --- | --- |
| `cargo test --workspace` (default) | Live test **ignored** (SKIP), never PASS |
| `pnpm acceptance:grok-acp:dry-run` | Machine JSON `status: "skipped"` |
| `pnpm acceptance:grok-acp` (no opt-in) | Same SKIP path as dry-run |
| `pnpm acceptance:grok-acp:live` without full opt-in | Exit ≠ 0, status skipped/failed — **not PASS** |
| Controller live with full opt-in | Exercised only outside this worker session |

## Live exercises (controller)

When opted in, the ignored Rust test exercises:

1. **Probe truth (aligned with H1)** — `adapter_name` is always `grok-acp` (rejects `fake`):
   - before initialize/session: `healthy=false`, detail `unavailable`
   - after `session/new`: `healthy=false`, detail `transport_ready_auth_unverified`
   - after the first successful real prompt: `healthy=true`, detail `healthy`
   - file presence alone never claims healthy
2. **session/new** — `start_session` with project root `working_directory`
3. **One read-only response** — text prompt forbidding tools/mutation; exact **one** terminal `turn_completed`
4. **Cooperative cancellation** — second turn cancelled; exact **one** terminal `run_cancelled`
5. **Shutdown** — adapter shutdown / child tree cleanup
6. **No write permission** — fail if write/edit/execute tool activity is observed
7. **Process residuals** — Node harness snapshots baseline/final Grok and Keeper PIDs; requires zero new residual PIDs

## H5 PID-bound ownership proof

The live adapter evidence includes `ownedProviderRootPid` (a non-secret PID) and
`ownedTreeClean`. The Rust acceptance test records the PID while the adapter owns
the provider, shuts down the adapter, and asserts the same PID has exited. The
Node validator requires both fields and refuses PASS without `ownedTreeClean: true`.
Grok/Keeper process-name snapshots remain secondary residual diagnostics only.

Machine output: `artifacts/native-acceptance-grok-acp/result.json` (gitignored)
Human report: `artifacts/native-acceptance-grok-acp/report.md` (gitignored)

## Human report template (generated)

The harness writes Markdown matching this structure:

```markdown
# Wave B2-H3 Controlled Native Acceptance Report
Generated: <iso-timestamp>
Mode: dry-run | live
Status: SKIPPED | PASSED | FAILED
...
## Checks
- optIn / probe / sessionNew / readOnlyTurn / cooperativeCancel /
  shutdown / residualPids / singleTerminal / noWritePermission /
  adapterNameGrokAcp
## Process residuals
## Non-claims
## Controller live run checklist
```

## Worker gate evidence (this package)

Commands run in the H3 worker session (no live Grok prompt):

| Gate | Command | Expected |
| --- | --- | --- |
| Script self-test | `pnpm acceptance:grok-acp:self-test` | PASS |
| Dry-run SKIP | `pnpm acceptance:grok-acp:dry-run` | `status: skipped` |
| Secret scan | embedded in self-test + dry-run write path | clean |
| Workspace unit/integration | `cargo test -p keeper-adapter-grok-acp` / workspace | green; live test ignored |
| Syntax | `node --check` on harness modules via self-test | PASS |

### Dry-run result contract

```json
{
  "schemaVersion": 1,
  "harness": "native-acceptance-grok-acp",
  "status": "skipped",
  "mode": "dry-run",
  "skipReason": "…",
  "checks": {
    "probe": { "status": "skipped" },
    "sessionNew": { "status": "skipped" },
    "readOnlyTurn": { "status": "skipped" },
    "cooperativeCancel": { "status": "skipped" },
    "shutdown": { "status": "skipped" },
    "residualPids": { "status": "skipped" }
  }
}
```

**SKIP is not PASS.** Controllers must not relabel dry-run or ignored tests as
live Grok connectivity evidence.

## What remains for the controller

1. H1 probe states and H2 desktop wiring are the merge base for H4; the live
   test already asserts the H1 probe ladder (`unavailable` →
   `transport_ready_auth_unverified` → `healthy`).
2. Provision an **isolated** Grok profile home (not the interactive user profile).
3. Set the four opt-in env vars; never commit them or paste auth material.
4. Run `pnpm acceptance:grok-acp:live` on Windows with a real Grok install.
5. Confirm `result.json` `status=passed`, residual PIDs clean, secret-scan clean.
6. Attach only redacted reports to the B2 PASS/HOLD package.

## Non-claims

- No multi-provider routing.
- No write-capable tool approval UI.
- No durable cross-restart ACP session-id storage.
- No provider process pool.
- This H3 worker commit does **not** prove live Grok connectivity.

## Commit

Record the H3 implementation SHA after commit in the controller integration report.
