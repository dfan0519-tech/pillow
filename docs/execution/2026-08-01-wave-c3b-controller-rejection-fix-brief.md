# Wave C3-B Controller Rejection Fix Brief

Owner: grok1 implementation worker
Controller: Codex Desktop
Date: 2026-08-01

## Verdict

C3-B is not accepted yet. The focused suite is green, but controller source
review found storage authority and recovery gaps that would otherwise become
dependencies of C3-C/D.

## Startup

- Work only in `this repository`; boot Keeper as grok1 worker under controller
  session `<keeper-session>`.
- Read all governing AGENTS files, the C3 design, the C3-B brief and report.
- Preserve the complete dirty tree. No stage, commit, reset, discard, worktree,
  or edits under frozen `legacy Keeper (out of tree)`.

## Allowed Files

- `crates/storage-sqlite/migrations/0005_write_proposals.sql`
- `crates/storage-sqlite/src/write_proposal.rs`
- `crates/storage-sqlite/tests/write_proposal_repository.rs`
- `crates/storage-sqlite/tests/migrations.rs`
- `crates/storage-sqlite/src/lib.rs`
- `crates/storage-sqlite/Cargo.toml` only if the typed receipt needs an already
  workspace-pinned dependency
- `docs/execution/2026-08-01-wave-c3b-storage-ledger-report.md`

Stop and report if another path is required.

## Required Corrections

### 1. Storage owns the exact TTL

The caller must not choose `expires_at`. Remove it from reserve input or ignore
it completely. Add a deterministic `reserve_at(input, now)` seam and make the
normal `reserve` call it with `Utc::now()`. Durable `created_at = now` and
`expires_at = now + WRITE_PROPOSAL_TTL_SECS` exactly. Test exact equality.

### 2. Validate all bounded metadata before SQL

- `proposal_id` is a UUID and bounded.
- proposal fingerprint, proposed/base digest and approval-grant digest are
  exactly `sha256:` plus 64 lowercase hex characters.
- proposed bytes are `1..=WRITE_CONTENT_BYTE_CAP`.
- create requires no base digest and `base_bytes == 0`.
- modify requires a canonical base digest and `base_bytes <=
  WRITE_CONTENT_BYTE_CAP` (empty existing text is allowed).
- owner and lease remain bounded; reject `lease_ms == 0` and values above
  300,000 rather than silently changing caller intent.
- alias collisions on `proposal_id` or `reservation_key` must return an opaque
  conflict outcome/error, never a generic SQLite failure.

Add matching SQL CHECK constraints where practical, including canonical digest
shape and bounded byte counts. Historical migrations remain untouched.

### 3. Replace arbitrary receipt strings with a typed receipt

`complete_apply` must not accept caller-controlled JSON text. Define a strict
serde receipt type owned by this module, with no content/base/native-path/debug
fields and `deny_unknown_fields` on wire forms. It must express only:

- success: portable path, mode, bytes written, canonical content digest and a
  `recovered` boolean;
- apply failure: one fixed opaque C3 apply error code and a `recovered` boolean.

The repository serializes canonical JSON itself. Success fields must exactly
match the stored proposal path/mode/proposed bytes/proposed digest. A failed
receipt derives state `failed`; a success receipt derives `succeeded`. Keep the
stored `result_json` read model, but never accept it as input. Bound its SQL
length and require valid JSON.

### 4. Make restart reconciliation possible without applying

Add one repository method for an existing `applying` row that records exactly
one of these read-only observations:

- target digest equals proposed -> typed recovered success;
- target is still base/absent -> `interrupted` with no apply receipt;
- target is neither -> typed recovered opaque failure.

The method performs no filesystem work, accepts no content, works only from
`applying`, is transactional, and is replay-safe for an identical terminal
outcome. This is the only owner-independent terminalization route for restart
recovery.

### 5. Enforce apply lease expiry

`complete_apply` must require the exact owner and a lease still live at the
provided `now`. A stale worker cannot complete after lease expiry. Reclaim by a
new owner remains serialized. Add exact-boundary and stale-owner race tests.

## Required Adversarial Tests

- caller cannot extend/shorten the exact ten-minute TTL;
- uppercase, short, long and non-hex digests reject;
- zero/oversize content/base counts reject;
- arbitrary receipt JSON, secret-bearing keys/content and mismatched success
  fields cannot reach SQLite;
- raw grant and distinctive proposed/base strings are absent from DB bytes and
  repository/transition DTOs;
- same proposal/reservation alias with foreign identity fails closed;
- zero/oversize lease rejects; expired old owner cannot complete after reclaim;
- all three `applying` reconciliation outcomes, exact replay and conflicting
  replay behave deterministically.

## Mandatory Gates

```text
cargo test -p keeper-storage-sqlite --test write_proposal_repository
cargo test -p keeper-storage-sqlite --test migrations
cargo test -p keeper-storage-sqlite --tests
cargo check --workspace --all-targets
cargo fmt --all -- --check
git diff --check
git diff --cached --name-only
```

Update the existing C3-B report with controller rejection, exact API/schema
changes, fresh test counts and residual risk. End with `## COMPLETION: DONE` or
`## COMPLETION: BLOCKED`.
