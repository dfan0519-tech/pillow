# DECOUPLE Step 1 — Freeze governance port (report)

**Marker:** `DECOUPLE-STEP1-COMPLETE`  
**Date:** 2026-08-09  
**Worker:** Grok (port slot) · **Controller:** Claude  
**Scope:** Proposal §9 row 1 only — pure add, zero consumer  

---

## 1. One-line outcome

New crate `keeper-governance-port` (`crates/governance-port`) freezes the adapter-facing governed execution port: closed errors, observer/cancel traits, opaque checked outcomes, §8.3 timing rustdoc, and a testkit. **No production consumer**, **no existing behavior change**, **no edits** under `runtime-host` / `adapter-grok-acp` / other forbidden trees.

---

## 2. Crate structure

```text
crates/governance-port/
  Cargo.toml                 # name = keeper-governance-port
  src/
    lib.rs                   # crate rustdoc (timing red lines) + re-exports
    cancel.rs                # CancellationProbe (+ AtomicBool impl)
    error.rs                 # closed error enums + Display/Error
    lifecycle.rs             # write/process lifecycle enums + observers
    outcome.rs               # opaque outcomes + CheckedGovernedOutcome
    port.rs                  # GovernedExecutionPort trait
    testkit.rs               # Panic / scripted doubles, recording observers
```

**Dependencies (first-party only):**

| Dep | Why |
|---|---|
| `keeper-agent-protocol` | Proposal DTOs / receipts / payloads |
| `keeper-policy` | Name `ExecProposalState` / `ExecDecision` / `is_exec_terminal` for closed process validation (public on agent-protocol fields but **not** re-exported there) |

**Dev-deps:** `chrono`, `serde_json`, `uuid` (test fixtures only).

**No new third-party crates.** Workspace members only plus already-present transitive deps of agent-protocol.

---

## 3. Closed error sets (why closed)

| Enum | Variants | Escape hatch |
|---|---|---|
| `GovernedReadExecutionError` | RunNotBound, InvalidProposal, ReservationConflict, InProgress, InvalidContext, Unavailable | **None** |
| `GovernedWriteExecutionError` | + Busy, NotLive, ShuttingDown, LifecycleDeliveryFailed | **None** |
| `GovernedProcessExecutionError` | + CleanupUnproven (distinct), LifecycleDeliveryFailed | **None** |
| `LifecycleObserverError` | RunNoLongerLive, DeliveryFailed | **None** |
| `GovernedOutcomeValidationError` | InvalidDeliveryShape, InvalidLifecycleOrder, InvalidTerminalShape, ConflictingProviderOutcome | **None** |

Design rules:

- No `Other(String)`, `Box<dyn Error>`, or open `Unavailable` bag as a dump for new states.
- Host-only control errors (OwnershipMismatch, InvalidGrant, …) stay host-side and map to `Unavailable` in later steps — not expanded here.
- `CleanupUnproven` is a **first-class** process variant so Step 4+ cannot silently collapse it.

Unit tests exhaustively `match` every variant **without** `_` arms.

---

## 4. Opaque outcomes

```rust
pub struct GovernedReadOutcome(GovernedReadReceipt);      // field private
pub struct GovernedWriteOutcome(GovernedWriteReceipt);
pub struct GovernedProcessOutcome(GovernedProcessReceipt);

pub trait CheckedGovernedOutcome<R>: Sized {
    fn try_from_receipt(receipt: R) -> Result<Self, GovernedOutcomeValidationError>;
    fn into_receipt(self) -> R;
    fn receipt(&self) -> &R;
}
```

External crates **cannot** write `GovernedWriteOutcome(raw_receipt)` (private tuple field). Production hosts and testkit **must** go through `try_from_receipt`.

### Validation rules (minimum freeze)

**Read**

- Result state must be terminal: Succeeded | Failed | Cancelled.

**Write**

- `resolved.state` terminal.
- `Replayed` ⇒ `requested == None`.
- `Executed` ⇒ `requested == Some(...)`.
- Exactly one of `provider_ok` / `provider_error`.
- Succeeded ⇒ ok only, no error_code; non-success ⇒ error only, `resolved.error_code == provider_error`.

**Process**

- `terminal` present; `state` terminal and matches terminal payload; proposal_id consistent.
- `Replayed` ⇒ zero lifecycle (`requested`/`decided`/`started` all None).
- `Executed` ⇒ `requested` present.
- `started.is_some()` ⇒ `decided.is_some()` (order).
- Flag consistency: Cancelled requires `cancelled`; Succeeded forbids cancel/timeout flags and requires `provider_ok`; provider_ok flags must match receipt flags.

Illegal shapes return a closed `GovernedOutcomeValidationError` — covered by unit tests.

---

## 5. Traits

### `CancellationProbe`

- Level-triggered `is_cancelled()`.
- `AtomicBool` / `&AtomicBool` impls load `SeqCst`.
- Documented: snapshot-once is fail-open.

### Observers

- `GovernedWriteObserver` / `GovernedProcessObserver` with borrowed lifecycle enums.
- Process has **no** Terminal variant (terminal only on receipt).
- Documented: same-thread, serial, no store, no lock, no replay callback, fail-closed on error.

### `GovernedExecutionPort`

Object-safe, `'static`, `Send + Sync`:

- `execute_read(run_id, proposal, &dyn CancellationProbe)`
- `execute_write(..., &mut dyn GovernedWriteObserver)`
- `execute_process(..., &mut dyn GovernedProcessObserver)`

No authority parameters (project/root/capability/grant/list/decide). Synchronous by design.

---

## 6. Rustdoc timing conventions (enumerated)

Written primarily in `src/lib.rs` crate docs, reinforced on traits/errors:

| # | Convention | Regression class if violated |
|---|---|---|
| 1 | Callback **must not** precede durable commit (Requested after reserve commit; Decided after decision commit; Started after claim Claimed/Running) | Governance timing regression |
| 2 | **Must not** hold coordinator lock while calling observers | Governance timing / deadlock regression |
| 3 | **No** terminal process callback; **no** observer call on `Replayed` | Governance timing regression |
| 4 | Cancel is level-triggered and re-read every wait cycle; snapshot-once is forbidden; C3F safe-resolution arbitration must be retained | **Fail-open** |
| 5 | Never degrade `CleanupUnproven` to cancelled/unavailable | **Fail-open** |
| 6 | Observer failure ⇒ durable fail-closed cleanup + `LifecycleDeliveryFailed` | **Fail-open** if authority left live |
| 7–10 | Block until durable terminal/replay/error; same-thread serial observers; no re-enter execute from observer; host control + execution port same owner-scoped service | Contract / future ownership |

These cannot be enforced by the type system; they are frozen in documentation for Step 4+ implementors and conformance tests.

---

## 7. Testkit shape

Public module `keeper_governance_port::testkit`:

| Type | Role |
|---|---|
| `PanicGovernedExecutionPort` | Any execute call panics (non-governed adapter tests) |
| `CheckedScriptedGovernedExecutionPort` | Queue of pre-checked outcomes; unexpected call panics; `assert_exhausted` |
| `ScriptedRead` / `ScriptedWrite` / `ScriptedProcess` | Script entries (optional run_id match, optional lifecycle delivery) |
| `RecordingWriteObserver` / `RecordingProcessObserver` | Assert callback counts/order; optional fail-next |
| `FixedCancellationProbe` | Live AtomicBool-backed probe |
| `checked_*_outcome(...)` | Panic-on-invalid helpers that still go through validation |

No production no-op / `Option` default.

---

## 8. Four test categories (how covered)

| Requirement | Tests |
|---|---|
| 1. Each legal outcome shape constructible | `legal_read_outcomes_construct`, `legal_write_outcomes_construct`, `legal_process_outcomes_construct` |
| 2. Illegal receipt shapes unconstructable | `illegal_write_shapes_are_rejected`, `illegal_process_shapes_are_rejected` (delivery, lifecycle order, terminal, provider conflict) |
| 3. Error sets exhaustive (no `_`) | `*_execution_errors_are_closed_without_wildcard`, `observer_and_validation_errors_are_closed_without_wildcard` |
| 4. Testkit usable | `scripted_write_delivers_requested_and_returns_checked_outcome`, cancel probe tests, AtomicBool probe |

**Result:** `cargo test -p keeper-governance-port --all-targets` → **12 passed, exit 0**.

---

## 9. Zero existing-behavior proof

| Argument | Evidence |
|---|---|
| **Structural** | `cargo metadata` shows **zero** workspace packages depend on `keeper-governance-port`. Nothing can call the new traits. |
| **Whitelist** | Only `crates/governance-port/**`, root `Cargo.toml` (+1 workspace member), `Cargo.lock` (new package entry only), and this report. |
| **Forbidden trees** | Did **not** edit `runtime-host`, `adapter-grok-acp`, `storage-sqlite`, `knowledge-indexer`, desktop, scripts, fixtures, or other docs. Parallel dirty on those paths is **other workers** (observed but not touched). |
| **No injection** | No `GrokAcpAdapter` constructor change, no free-function wrap, no desktop composition change. |

Therefore existing production paths are unchanged by construction, not by careful dual-path maintenance.

---

## 10. Commands and exit codes

`CARGO_TARGET_DIR=<repo>/target-port` held for all cargo work.

| Command | Exit |
|---|---|
| `cargo fmt -p keeper-governance-port -- --check` | **0** |
| `cargo clippy -p keeper-governance-port --all-targets -- -D warnings` | **0** |
| `cargo test -p keeper-governance-port --all-targets` | **0** (12 tests) |
| `node scripts/run-worker-gate.mjs` | **1** — failed at step **secrets** (see residual) |
| workspace `cargo test --workspace --all-targets` (diagnostic, not gate) | **101** — 1 unrelated desktop composition test failure under parallel dirty tree |

### Worker gate detail

```text
PILLOW_WORKER_GATE: start
workspace-structure: ok
secrets: FAILED — 6 findings, all in out-of-scope parallel dirty files:
  crates/knowledge-indexer/src/render.rs  (keeper-session / keeper-lease / user-profile-path
                                           — negative-assert test strings)
  crates/storage-sqlite/src/knowledge.rs  (same pattern)
```

These files are **explicitly forbidden** for this ticket and pre-existed as other workers' dirty state. This Step 1 tree introduces **no** secret/session/lease/machine-path hits under `crates/governance-port/**`.

---

## 11. What was **not** touched

- `crates/runtime-host/**` (Codex Step 2 — observed concurrent dirty `host/process.rs`; not edited here)
- `crates/adapter-grok-acp/**`
- `crates/storage-sqlite/**`, `crates/knowledge-indexer/**`
- `crates/kernel-project/**`, `crates/git-executor/**`, `crates/kernel-resource/**`
- `apps/desktop/**`, `scripts/**`, `tests/fixtures/**`
- Any `docs/` file except this report
- No git add/commit/stash/reset/clean/checkout/restore
- No desktop E2E
- No auth/token reads
- No Step 2–11 work (no host impl, no cancellation migration, no consumer inject, no §8.4 risk fix)

---

## 12. Residual risks

1. **Full worker gate red on foreign secrets.** Parallel knowledge-indexer / storage-sqlite dirty content fails `check-secrets.mjs`. Controller should re-run the gate after those workers clean/allowlist, or on a clean tree with only this step's files.
2. **Timing contracts are documentation-only** until Step 4+ conformance tests (by design of Step 1).
3. **`keeper-policy` dependency** is a small deviation from “only agent-protocol” in the proposal text, required because `ExecProposalState` is not re-exported from agent-protocol. No third-party crate added.
4. **Process validation is structural, not a full production-shape oracle.** It freezes impossible combinations; later steps may tighten further when host mapping lands.
5. **§8.4 host risks untouched** (commit→EventSink non-atomicity; process Decided/Started observer-failure cleanup gap) — correctly deferred to Step 2 / separate acceptance.

---

## 13. Completion

**`DECOUPLE-STEP1-COMPLETE`**

Deliverables match proposal §9 row 1. Existing behavior is structurally unchanged (zero consumers). Port crate gates are green under `CARGO_TARGET_DIR=<repo>/target-port`. Full workspace worker gate is currently blocked by **out-of-whitelist parallel dirty secrets**, not by this crate.
