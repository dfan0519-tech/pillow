# C4-D2AR Grok1 repair report — single execution claim and cancelled receipt

Date: 2026-08-02

Worker: real Grok CLI slot `grok1`
Worker session: `<keeper-session>`
Controller parent session: `<keeper-session>`
Task: `c4-d2ar-single-claim-repair`
Status: **DONE** (controller owns acceptance)

## Objective

Repair the C4 process ledger so it no longer imports renewable C3 write-apply
lease reclaim semantics into process execution authority. `Approved -> Running`
is the only one-shot spawn claim; abnormal running rows stay Busy until
owner-aware completion/interrupt or restart recovery.

## Root cause (controller rejection)

The initial D2A ledger mirrored write leases:

| Defect | Effect |
| --- | --- |
| Same-owner live reclaim returned `Claimed` | Caller could treat second claim as spawn authority |
| Expired running lease reassigned to any owner as `Claimed` | Second process could start while first may still live |
| Generic `transition` allowed `Running -> Cancelled/Interrupted` | Bypassed run owner |
| `complete_run` only accepted Succeeded/Failed | Policy-defined post-spawn Cancelled receipt could not persist |

## Claim matrix (after repair)

| Current state | Caller | Lease | `claim_run` outcome |
| --- | --- | --- | --- |
| `Approved` | any valid owner | n/a (writes new lease) | **`Claimed`** (only path) |
| `Running` | same owner | live | **`Busy(current)`** |
| `Running` | different owner | live | **`Busy(current)`** |
| `Running` | any owner | expired | **`Busy(current)`** — never reassigns |
| Terminal | any | n/a | `Terminal` |
| Expired approved/pending at claim | any | n/a | `Expired` |
| Other non-approved | any | n/a | `Conflict` |

Owner/lease fields are written only on the first `Approved -> Running`
transition. Failed claim attempts while `Running` leave the row unchanged.

## Cancelled receipt coupling

| Path | State | Receipt / terminal columns |
| --- | --- | --- |
| Pre-spawn cancel (`transition` from pending/approved) | `cancelled` | **receipt-free** — all terminal columns NULL |
| Post-spawn cancel (`complete_run(..., Cancelled, receipt)`) | `cancelled` | **same bounded receipt + terminal metadata** as succeeded/failed |
| Succeeded / Failed | apply terminals | receipt required; `receipt.cancelled == false` |
| Cancelled complete | apply terminal | receipt required; `receipt.cancelled == true` |
| Flag mismatch | n/a | **fail closed** (`InvalidResult`); row stays `running` |
| Transition history | cancelled | optional `result_json` (null pre-spawn; receipt post-spawn) |

SQL `CHECK` on `exec_proposals` and `exec_proposal_transitions` updated so
`cancelled` may be either fully receipt-bearing or fully receipt-free; other
non-apply states remain receipt-free.

## Behavior changes (minimal)

1. **`claim_run`**: if state is `Running`, always return `Busy(current)`. No
   same-owner renewal, no expired-lease reassignment.
2. **`transition`**: reject **all** mutations from `Running` (including
   Cancelled/Interrupted). Pre-spawn cancel/expiry/reject from pending/approved
   unchanged.
3. **`complete_run`**: accept `Succeeded | Failed | Cancelled` with exact
   cancellation-flag consistency; owner + live lease still required.
4. **`interrupt_run`**: unchanged (owner-aware live interrupt).
5. **`recover_interrupted`**: unchanged (restart-only force interrupt of
   pending/approved/running).

Public types unchanged.

## Exact files (exclusive allowlist)

| Path | Action |
| --- | --- |
| `crates/storage-sqlite/migrations/0007_exec_proposals.sql` | **Modified** — cancelled optional receipt CHECK |
| `crates/storage-sqlite/src/exec_proposal.rs` | **Modified** — claim/transition/complete_run |
| `crates/storage-sqlite/tests/exec_proposal_repository.rs` | **Modified** — RED/GREEN matrix tests |
| `docs/execution/2026-08-02-wave-c4-d2ar-grok1-single-claim-repair-report.md` | **Created** (this report) |

No other storage, manifest, lockfile, process executor, protocol, policy,
runtime, Tauri or renderer file changed by this worker.

HEAD preserved: `7b7f70f`. No stage, commit, reset, restore, clean, package
or publish.

## TDD evidence

### RED (intent before production edit)

New/rewritten tests encoded the rejected semantics as assertions:

- first `Approved -> Running` → `Claimed`
- later claims while `Running` → `Busy` (same owner, other owner, expired lease)
- generic `transition` rejects every next state from `Running`
- `complete_run(..., Cancelled, receipt)` with `cancelled: true` persists + replays
- mismatched state/receipt cancellation flags → `InvalidResult`

Against the pre-repair implementation those assertions would fail at:

- same-owner reclaim returning `Claimed` instead of `Busy`
- expired-lease reassignment returning `Claimed`
- `Running -> Cancelled/Interrupted` via generic `transition`
- `complete_run` rejecting `Cancelled` and missing flag consistency

### GREEN

| Command | Result |
| --- | --- |
| `cargo test -p keeper-storage-sqlite --test exec_proposal_repository` | **25/25** passed |
| `cargo test -p keeper-storage-sqlite --tests` | all targets green (lib 2; conversation* 1+7+6+8; exec **25**; migrations 11; project 3; resource 5; tool_call 8; write 25) |
| `cargo check --workspace --all-targets` | exit 0 |
| `cargo clippy -p keeper-storage-sqlite --all-targets -- -D warnings` | exit 0 |
| `cargo fmt --all --check` | exit 0 (after scoped rustfmt of allowlisted Rust) |
| `git diff --check -- crates/storage-sqlite` | exit 0 (LF/CRLF warnings only on pre-existing tracked files) |

## Privacy

Existing `argv_native_path_grant_env_and_transcript_never_enter_sqlite` remains
green. Repair adds no argv/native/env/grant/transcript storage. Digests and
bounded `result_json` only.

## Staging state

- Allowlist deliverables: unstaged only (`??` / modified within allowed paths).
- `git diff --cached --name-only`: empty for this worker.
- Pre-existing C1–C3 / D1 / D2A dirty tree left untouched outside allowlist.

## Residual risks (controller)

1. **Host wiring**: Task 7 gateway must treat only first `Claimed` as spawn
   authority; `Busy` must never start a second process.
2. **Lease expiry mid-run**: expired running rows remain `Busy` on claim and
   still require live lease for `complete_run` / `interrupt_run`; recovery is
   the interrupt path for orphans (by design).
3. **Pre- vs post-spawn cancelled** are both `cancelled` but differ on receipt
   presence; readers must tolerate optional terminal columns on that state.
4. Integration with process-executor and approval UI not in this repair scope.
5. Not controller source-reviewed; independent acceptance remains controller-owned.

## Completion

`COMPLETION: complete`
