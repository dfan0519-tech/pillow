# Wave C3-F Product Fix Report — Cancel Resolution Arbitration

Owner: Grok runtime worker (`grok-c3f-worker`)
Worker session: `<keeper-session>` (brief); bridge session `<keeper-session>`
Runtime allocation: `grok3` / `<keeper-lease>`
Controller session (completion): `<keeper-session>`
Date: 2026-08-01

**COMPLETION: DONE**

---

## Defect (confirmed)

When Stop cancels a run with a live governed-write proposal, the write
coordinator durably resolves the proposal as `cancelled` (or
`interrupted` when mid-apply), but host cancel marks
`BoundedDelivery` and `ProtocolGuardSink` cancelled before the adapter can
emit the corresponding `write_proposal_resolved`. The UI therefore received
`run_cancelled` without the conversation-owned write terminal and could not
event-drive pending-proposal refresh.

## Contract implemented

1. After host cancel wins, at most one narrowly validated
   `write_proposal_resolved` is permitted, with state/error pair exactly:
   - `cancelled` / `cancelled`, or
   - `interrupted` / `interrupted`.
2. That safe resolution is ordered **before** the single host-forced
   `run_cancelled` terminal.
3. Every other post-cancel event remains rejected (write success, reject,
   expire, failure, ordinary tool/assistant output, turn completion).
4. Malformed payload, mismatched state/error pair, or duplicate safe
   resolution fails closed (no bad event; host still forces `run_cancelled`).
5. Bounded-queue cancel arbitration, sequence validation, one-terminal
   enforcement, and shutdown behavior are preserved (only the narrow
   post-cancel write-resolution slot was added).
6. Storage durable transition remains the governed-write waiter's
   responsibility before emit; this fix only unblocks the already-emitted
   safe resolution on the host ingress path. No target mutation on cancel.

## TDD evidence

### RED (before implementation)

Focused unit tests failed with the pre-fix behaviour:

```text
post_cancel_safe_write_resolution_cancelled_before_run_cancelled
  left:  [RunStarted, RunCancelled]
  right: [RunStarted, WriteProposalResolved, RunCancelled]

post_cancel_safe_write_resolution_interrupted_before_run_cancelled
  left:  [RunStarted, RunCancelled]
  right: [RunStarted, WriteProposalResolved, RunCancelled]

post_cancel_safe_resolution_through_bounded_delivery_before_run_cancelled
  left:  [RunStarted, RunCancelled]
  right: [RunStarted, WriteProposalResolved, RunCancelled]

post_cancel_blocks_success_mismatch_malformed_duplicate_and_ordinary
  assertion: at most one post-cancel safe resolution — left: 0, right: 1
```

### GREEN (after implementation)

```text
cargo test -p keeper-runtime-host --tests -- --test-threads=1
  lib:                     22 passed
  fake_runtime:             8 passed
  governed_read_bridge:     4 passed
  governed_write_bridge:   16 passed  (includes 2 new cancel-arbitration tests)
  host_cancel:              2 passed
  host_delivery_progress:   3 passed
  host_drop_cancel:         2 passed
  host_invariants:          4 passed
  host_run_ownership:       1 passed
  host_shutdown_spawn_race: 4 passed
  host_shutdown_sync_turn:  3 passed
  protocol_guard:           5 passed
  TOTAL:                  74 passed, 0 failed
  TEST_EXIT=0
```

## Exact event order (happy path)

After `RunStarted` (+ optional `WriteProposalRequested`), host cancel, then
adapter emit of safe cancelled resolution:

```text
RunStarted
WriteProposalRequested          # optional; already committed pre-cancel
WriteProposalResolved           # state=cancelled, error_code=cancelled
RunCancelled                    # host-forced, reason=host; sole terminal
```

Interrupted pair uses the same order with
`state=interrupted` / `error_code=interrupted`.

Sequence numbers remain host-validated and monotonic: the safe resolution
uses the adapter's next sequence; `force_run_cancelled` uses the subsequent
sequence.

## Negative cases proven

| Case | Result |
|------|--------|
| `write_proposal_resolved` succeeded (no error) after cancel | Rejected; only `run_cancelled` |
| `write_proposal_resolved` rejected / mismatched pairs | Rejected; only `run_cancelled` |
| Malformed resolved payload (missing fields) | Rejected; only `run_cancelled` |
| Ordinary `ToolCallOutput` / `AssistantDelta` after cancel | Rejected; only `run_cancelled` |
| `TurnCompleted` after cancel (existing host_cancel tests) | Rejected; only `run_cancelled` |
| Second safe `cancelled`/`cancelled` resolution | At most one resolution; one `run_cancelled` |
| Delivery: second write resolution after cancel | `RuntimeError::Cancelled` |
| Delivery: `TurnCompleted` after cancel | `RuntimeError::Cancelled` |

## Implementation (smallest host change)

### `crates/runtime-host/src/delivery.rs`

- Shared helper `is_safe_post_cancel_write_resolution` validates kind +
  payload pair (`cancelled`/`cancelled` or `interrupted`/`interrupted`).
- After cancel, `may_commit_after_cancel` allows:
  - any `run_cancelled`, and
  - at most one safe `write_proposal_resolved`.
- Flag `post_cancel_write_resolution_committed` enforces the single slot
  under the same lock as terminal commit arbitration.
- Payload validation at the queue boundary closes the race where delivery
  cancel lands slightly before the protocol `AtomicBool`.

### `crates/runtime-host/src/sink.rs`

- `ProtocolGuardSink` tracks `post_cancel_safe_resolution_emitted`.
- On `host_cancelled()`, if no terminal yet and the event is a safe write
  resolution and not yet emitted: `emit_validated` it, then
  `force_run_cancelled`, then return `RuntimeError::Cancelled`.
- All other post-cancel events still force `run_cancelled` only.

### `crates/runtime-host/src/worker.rs`

**Not modified.** Terminal ordering closes in delivery + sink alone.

### Tests

- Unit tests in `delivery.rs` / `sink.rs` (RED→GREEN).
- Integration-style tests in
  `crates/runtime-host/tests/governed_write_bridge.rs`:
  - `cancel_arbitration_emits_safe_write_resolution_before_run_cancelled`
  - `cancel_arbitration_blocks_success_ordinary_and_duplicate_safe_resolution`
- Minor: `#[cfg(feature = "test-support")]` on pre-existing helper
  `is_terminal_list_state` so clippy `-D warnings` is clean without
  test-support.

## Gates

| Gate | Result |
|------|--------|
| Focused new cancellation/arbitration tests | PASS (6 unit + 2 integration) |
| `cargo test -p keeper-runtime-host --tests -- --test-threads=1` | PASS 74/74 |
| `cargo check --workspace --all-targets` | PASS |
| `cargo clippy -p keeper-runtime-host --all-targets -- -D warnings` | PASS |
| `cargo fmt -p keeper-runtime-host -- --check` | PASS |
| whitelist `git diff --check` | PASS (LF/CRLF warning only) |
| HEAD unchanged | `7b7f70f7360131cd1cfd984a8b19bdfb5725c21b` |
| Staging empty | Yes |

## Changed files (whitelist only)

| Path | Change |
|------|--------|
| `crates/runtime-host/src/delivery.rs` | Post-cancel safe write-resolution commit slot |
| `crates/runtime-host/src/sink.rs` | Protocol-guard post-cancel safe resolution path |
| `crates/runtime-host/tests/governed_write_bridge.rs` | Integration arbitration tests + cfg helper |
| `docs/execution/2026-08-01-wave-c3f-cancel-resolution-arbitration-fix-report.md` | This report |

Not edited: adapter, protocol crate, native conversation service, renderer,
storage, policy, executor, C3-F harness, fake ACP fixtures, worker.rs.

## Residual risks

1. **End-to-end C3-F desktop scenario 5 not re-run here** — this product fix
   unblocks the missing conversation-owned write terminal on the host path;
   full Tauri/WebView2 cancel-while-pending acceptance remains controller-
   owned.
2. **Adapter still owns emit timing** — if the adapter never emits
   `write_proposal_resolved` after durable cancel, the host cannot invent one
   (by design; no fabricated resolution). The host only permits the safe
   event when the adapter does emit it post-cancel.
3. **Pre-cancel resolution + cancel** — a resolution that already committed
   before cancel is unchanged; the post-cancel slot is only for the late
   durable-cancel path.
4. **Delivery kind+payload check is fail-closed** — a success-shaped
   `write_proposal_resolved` after cancel is rejected even if the protocol
   AtomicBool lags; this is intentional defense in depth.

## Out of scope / hygiene

- No stage, commit, reset, restore, package, or publish.
- No writes to the frozen legacy Keeper tree.
- No changes outside the write whitelist.

---

**COMPLETION: DONE**
