# Composition flake report — `injected_adapter_routes_probe_session_turn_and_cancel`

Date: 2026-08-13
Worker: grok4 (default model grok-4.6)
Controller: Claude
Status: **FLAKE-FIX-COMPLETE** (worker completion; controller gate is separate)

Allowlist:

- `apps/desktop/src-tauri/src/conversation_service/tests/composition.rs`
- this report

No git writes. No worker-gate. No E2E.

## Defect

`injected_adapter_routes_probe_session_turn_and_cancel` waited on
`adapter.arm_calls >= 2`, then took a **single** timeline snapshot and
resolved the cancel target with
`find(unfinished).or_else(runs.last())`.

`arm_turn` increments before the worker reaches `send_turn`. The wait
therefore exits on a proxy signal. When the second run is not yet
visible as `pending` / `running` / `cancelling`, `find` misses and
`or_else` cancels the first (completed) run.
`assert_eq!(send_result.run_id, run_id)` then fails.

That assertion is the product claim ("cancel hits the second turn").
It is kept. The flake made the workspace `--lib` / `--workspace` gate
look like a product regression.

## Reproduction (RED, before the fix)

Isolated repeats of this test stay green. The race needs a snapshot
that lands **before** the second unfinished run exists. That is what
`or_else(runs.last())` turns into a wrong-target cancel instead of a
timeout.

**Forced, stable red** (same wait/fallback shape as the original, with
an 80 ms delay in the send thread so `create_run` cannot land before
the snapshot):

```text
node scripts/run-cargo.mjs test -p pillow-desktop --lib \
  conversation_service::tests::composition::injected_adapter_routes_probe_session_turn_and_cancel \
  -- --exact --nocapture
```

| Run | EXIT | Result |
| --- | ---: | --- |
| forced-delay 1 | 101 | `assert_eq!(send_result.run_id, run_id)` — left = second turn, right = first completed run |

Log: `.superpowers/round47/repro-forced-delay.txt`

**Failure rate (forced delay): 1 / 1.** The delay is only a
reproduction lever; it was removed before the fix landed. External
report on the unmodified wait under full `--lib` load: **11 / 25**.
Controller isolated repeats of 15 stayed green — consistent with
"needs a too-early snapshot, not a broken product path".

## Fix

Wait for the **same facts the assertion depends on**, not `arm_calls`:

1. `adapter.send_calls >= 2` — `send_turn` has entered the adapter
   (arm already happened; the cancel flag exists; later
   `assert_eq!(send_calls, 2)` is meaningful).
2. Timeline has an unfinished run (`pending` / `running` /
   `cancelling`) — that run id is the cancel target.

The loop has a 3 s deadline. Timeout panics with both counters and
says it was waiting for the second unfinished timeline run after
`send_turn`.

**`or_else(runs.last())` is gone.** If the unfinished run is not
there, the test times out. A wrong object is worse than a hard miss.

`assert_eq!(send_result.run_id, run_id)` is unchanged.

Why not wait only on timeline pending: `create_run` commits *before*
`arm_turn`. A pending-only wait can cancel before the adapter has
armed the run (`cancel_turn` → `RunNotFound`, treated as `Ok`). The
hold then times out and the turn completes. `send_calls >= 2` closes
that window.

Why not a fixed sleep: that only lowers the probability. This waits
for a determined condition or fails closed.

## Exit codes (actually run)

Toolchain: slot rustup home had no active toolchain; commands used the
host rustup/cargo install (no user paths in this report).

| # | Command | EXIT | Note |
| --- | --- | ---: | --- |
| R1 | `node scripts/run-cargo.mjs test -p pillow-desktop --lib conversation_service::tests::composition::injected_adapter_routes_probe_session_turn_and_cancel -- --exact --nocapture` | 101 | Forced-delay RED |
| G1 | same focused command after the fix | 0 | 1 passed |
| L1 | `node scripts/run-cargo.mjs test -p pillow-desktop --lib` | 0 | close-out; `.superpowers/round47/flake-log1.txt`; 213 passed, target test ok |
| L2–L9 | same `--lib` | 0 | `.superpowers/round47/flake-log{2-9}.txt` |
| L10–L13 | same `--lib` | 101 | **not this flake** — mid-loop compile of `write_service.rs` missing DTO fields (`content_digest` / `content_review_complete` / `diff_truncated`). Parallel EA-04 worker. ~8–14 s each; no tests ran |
| L14–L43 | same `--lib` | 0 | 30 consecutive; `.superpowers/round47/flake-log{14-43}.txt` |

L14: 213 passed, target test ok.
L30 / L43: 216 passed, target test ok (other workers added tests
during the loop; this change did not).

Summary table: `.superpowers/round47/stability-summary.txt`.

## Post-fix stability

**30 / 30 consecutive full `--lib` runs green (L14–L43).**
Composition cancel-path failures in that window: **0**.

Earlier L1–L9 were also green (9 more `--lib` executions). L10–L13
do not count as this test flaking.

## Residual risk — same-directory proxy waits

| Test | Wait | Risk |
| --- | --- | --- |
| `one_prompt_admission_matches_production_grok_host_config` (same file) | `arm_calls < 1` then immediate second `send_turn` | **Listed.** Slot is acquired in `spawn_turn` *before* `arm_turn`, so waiting for arm is late, not early. Assertion is "second prompt rejected", not "cancel hits run N". Conservative, not the same class. Still a proxy signal. |
| `registration.rs` `send_turn_admission_linearized_with_shutdown` | `Barrier` + `sleep(40ms)` before asserting shutdown is still running | Different pattern (fixed delay, not `arm_calls`). Not this bug. |
| `control.rs` cancel / one-prompt tests | `Barrier` on `after_worker_registered` | Deterministic; no proxy counter. |
| Adapter `hold_for_cancel` / `hold_until_release` loops | wait on cancel/release flags, 5 s deadline | Fixture body, not the test's precondition. |

No other `or_else(runs.last())` in this test directory.

## Changed files

| Path | Action |
| --- | --- |
| `apps/desktop/src-tauri/src/conversation_service/tests/composition.rs` | Replace `arm_calls` wait + `or_else(last)` with bounded wait on `send_calls >= 2` and an unfinished timeline run |
| `docs/execution/2026-08-13-composition-flake-report.md` | This report |

Support file `tests/support.rs` was not changed.

## FLAKE-FIX-COMPLETE
