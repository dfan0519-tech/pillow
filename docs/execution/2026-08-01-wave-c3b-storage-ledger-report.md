# Wave C3-B Report — Durable Write Proposal Ledger

Owner: grok1 implementation worker
Controller session: `<keeper-session>`
Worker session (original): `<keeper-session>`
Worker session (controller rejection fix): `<keeper-session>`
Date: 2026-08-01
Task: `c3b-storage-ledger` + controller rejection fix + controller final narrow fix

## Scope

Add migration `0005` and a transactionally safe SQLite repository for
write-proposal metadata, approval decisions, transitions, apply leases, and
terminal receipts. SQLite stores digests and metadata only: never proposed/base
content or raw approval grants.

No filesystem apply, runtime host, adapter, Tauri, or React work.

## Controller Rejection (why the first correction pass)

C3-B focused suite was green, but controller source review found storage
authority and recovery gaps that would become hard dependencies of C3-C/D:

1. Caller could choose `expires_at` — storage must own the exact ten-minute TTL.
2. Metadata validation was incomplete (digests, byte caps, lease bounds);
   alias collisions could surface as raw SQLite failures.
3. `complete_apply` accepted arbitrary caller JSON for `result_json`.
4. No owner-independent, read-only `applying` reconciliation path for restart.
5. `complete_apply` did not enforce a still-live apply lease at `now`.

## Controller Final Narrow Fix (this pass)

Controller accepted the rejection-fix architecture but required three narrow
corrections before C3-B can be treated as storage-ready for C3-C:

1. Expand the fixed typed apply failure receipt vocabulary to the full
   apply-time C3 set: `base_changed`, `path_rejected`, `write_failed`.
2. Couple `result_json` to state in SQL: non-null only for `succeeded`/`failed`;
   null for every other proposal and transition state.
3. Drop the obsolete residual-risk claim that failure vocabulary only supports
   `write_failed`.

## Exact API / Schema Changes

### Migration `0005_write_proposals.sql`

- Canonical digest CHECKs: `sha256:` + exactly 64 lowercase hex on
  `proposal_fingerprint`, `proposed_digest`, optional `base_digest`, and
  `approval_grant_digest`.
- Byte bounds: `proposed_bytes` in `1..=262144`; `base_bytes` in `0..=262144`;
  create requires `base_digest IS NULL AND base_bytes = 0`.
- Identity bounds: `proposal_id` length 36; bounded `reservation_key`,
  `call_id`, `path`, `apply_owner`.
- **State/result coupling on `write_proposals`**: `succeeded`/`failed` require
  non-null `result_json` that is non-empty, `<= 8192` bytes, and
  `json_valid(...)`. Every other state requires `result_json IS NULL`.
- **Same state/result rule on `write_proposal_transitions`**: apply terminals
  require bounded valid JSON; every other transition requires null.
- Historical migrations `0001`–`0004` untouched.

### Repository API

| Change | Detail |
| --- | --- |
| TTL ownership | Removed `expires_at` from `WriteProposalReserveInput`. Added `reserve_at(input, now)`; `reserve` calls it with `Utc::now()`. Durable `created_at = now`, `expires_at = now + WRITE_PROPOSAL_TTL_SECS` (600) exactly. |
| Pre-SQL validation | UUID `proposal_id`; canonical digests; proposed `1..=WRITE_CONTENT_BYTE_CAP`; create/modify base rules; reject `lease_ms == 0` or `> 300_000` (no silent clamp). |
| Alias collisions | `proposal_id` / `reservation_key` foreign-identity collisions return `WriteReservationOutcome::Conflict` (or opaque `ReservationConflict`), never a raw SQLite error to callers. |
| Typed receipt | `WriteApplyReceipt::{Success, Failure}` with `deny_unknown_fields`. Success: path, mode, bytes_written, content_digest, recovered. Failure: closed `WriteApplyErrorCode` (`BaseChanged` / `PathRejected` / `WriteFailed` → wire `base_changed` / `path_rejected` / `write_failed`), recovered. Repository serializes canonical JSON; success fields must match stored proposal. |
| `complete_apply` | Signature is `(proposal_id, owner, receipt, now)`. Terminal state derived from receipt. Requires exact owner **and** live lease (`lease_expires_at > now`). Rejects `recovered: true` (recovery uses reconcile). Accepts all three apply failure codes. |
| Restart reconcile | `reconcile_applying(proposal_id, observation, now)` for `applying` only: `MatchesProposed` → recovered success; `StillBaseOrAbsent` → `interrupted` (no receipt); `Neither` → recovered opaque `write_failed`. Transactional, no FS work, replay-safe for identical terminal outcome. |
| `transition` | No longer accepts free-form `result_json`; apply terminals only via complete/reconcile. |

### New exports

`WriteApplyReceipt`, `WriteApplyErrorCode` (`BaseChanged`, `PathRejected`,
`WriteFailed`), `WriteApplyReconcileObservation`, `MAX_APPLY_LEASE_MS`,
`RESULT_JSON_BYTE_CAP`.

## Changed Files (whitelist only)

| Path | Change |
| --- | --- |
| `crates/storage-sqlite/migrations/0005_write_proposals.sql` | Digest/byte CHECKs; state/result_json coupling on proposals + transitions |
| `crates/storage-sqlite/src/write_proposal.rs` | TTL seam, validation, typed receipt with three apply codes, reconcile, lease |
| `crates/storage-sqlite/tests/write_proposal_repository.rs` | Adversarial TDD; failure vocabulary round-trip + complete-apply |
| `crates/storage-sqlite/tests/migrations.rs` | SQL invariant checks; direct adversarial state/result coupling |
| `crates/storage-sqlite/src/lib.rs` | Re-exports |
| `crates/storage-sqlite/Cargo.toml` | `serde` (workspace-pinned) for typed receipt |
| `docs/execution/2026-08-01-wave-c3b-storage-ledger-report.md` | This report |

No stage/commit/reset. `legacy Keeper (out of tree)` not modified.

## Gate Evidence (controller final narrow fix)

```text
cargo test -p keeper-storage-sqlite --test write_proposal_repository
```

- **23 passed**, 0 failed

```text
cargo test -p keeper-storage-sqlite --test migrations
```

- **4 passed**, 0 failed

```text
cargo test -p keeper-storage-sqlite --tests
```

All integration targets green:

| Target | Result |
| --- | --- |
| lib unit | 2 passed |
| conversation_event_replay_status | 1 passed |
| conversation_event_status | 7 passed |
| conversation_events | 6 passed |
| conversation_repository | 8 passed |
| migrations | 4 passed |
| project_repository | 3 passed |
| resource_recent_repository | 5 passed |
| tool_call_repository | 8 passed |
| write_proposal_repository | 23 passed |

```text
cargo check --workspace --all-targets
```

- **exit 0**

```text
cargo fmt --all -- --check
```

- **exit 0**

```text
git diff --check
```

- **exit 0** (existing LF/CRLF warnings on accepted dirty tree only)

```text
git diff --cached --name-only
```

- **empty** (no staged files)

## Adversarial Coverage Added

- Exact ten-minute TTL equality via `reserve_at`; no caller override field
- Uppercase / short / long / non-hex digests reject before SQL
- Zero / oversize proposed and base byte counts reject; empty modify base allowed
- Arbitrary receipt JSON, secret-bearing unknown keys, mismatched success fields
  cannot reach SQLite
- Closed apply failure vocabulary: serde round-trip for
  `base_changed` / `path_rejected` / `write_failed`; complete_apply persists each;
  approval/provider/git/free-text codes and message fields reject
- Raw grant and distinctive proposed/base strings absent from DB bytes and DTOs
- Same proposal_id / reservation_key with foreign identity → opaque Conflict
- Zero / oversize lease reject; expired old owner cannot complete after reclaim;
  exact lease-boundary complete fails (`expires_at > now` required)
- All three `applying` reconciliation outcomes, exact replay, conflicting replay
- Direct SQL: pending/rejected/interrupted cannot carry `result_json`; apply
  terminals cannot omit it (proposals and transitions)

## Residual Risks

1. **No host sealing yet**: sealed proposed/base UTF-8 payloads remain
   in-memory host concerns for C3-C/C3-D; this wave only persists digests and
   typed terminal receipts.
2. **Observation trust**: `reconcile_applying` trusts the host's read-only
   digest classification; it does not re-hash files (by design — no FS in
   storage). C3-D must classify correctly and never pass content. Restart
   opaque failure remains `write_failed` (host may still complete live apply
   with `base_changed` or `path_rejected` when classification is known at apply
   time).
3. **Lease reclaim does not auto-interrupt**: an expired apply lease can be
   reclaimed; terminalization of abandoned applies still goes through
   complete (live owner) or `reconcile_applying` (restart).

## Working-Tree Governance

- HEAD baseline preserved; accepted dirty tree not reset.
- No staging, commit, discard, or force operations.
- `legacy Keeper (out of tree)` not modified.
- Whitelist-only edits for this wave.

## COMPLETION: DONE
