# Wave C3-B Brief — Durable Write Proposal Ledger

Owner: grok1 implementation worker

Controller session: `<keeper-session>`

Activated after controller accepted C3-A on 2026-08-01. Fresh controller gates:
agent-protocol 55/55, keeper-policy 16/16, tool-executor 13/13,
`cargo check --workspace --all-targets`, fmt and diff checks all passed; staging
remained empty.

## Startup

1. Work only in `this repository`.
2. POST Keeper boot with `project=pillow`, task `c3b-storage-ledger`,
   agentName `grok1`, agentRole `worker`, and
   `<local-parent-session>`.
3. Read all AGENTS files, the C3 design, and
   `docs/execution/2026-08-01-wave-c3a-protocol-policy-report.md`.
4. Preserve the accepted dirty tree. Do not reset, stage or commit.
5. Treat the C3-A report as worker evidence, not authority; the C3 design and
   this brief control any conflict.

## Goal

Add migration `0005` and a transactionally safe SQLite repository for
write-proposal metadata, approval decisions, transitions, apply leases and
terminal receipts. SQLite stores digests and metadata only: never proposed/base
content or raw approval grants.

## Required Stored Record

```rust
pub struct StoredWriteProposal {
    pub proposal_id: String,
    pub reservation_key: String,
    pub proposal_fingerprint: String,
    pub project_id: String,
    pub conversation_id: String,
    pub run_id: RunId,
    pub call_id: String,
    pub path: String,
    pub mode: WriteMode,
    pub proposed_digest: String,
    pub proposed_bytes: u64,
    pub base_digest: Option<String>,
    pub base_bytes: u64,
    pub state: WriteProposalState,
    pub expires_at: DateTime<Utc>,
    pub result_json: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}
```

Repository outcomes must distinguish `Reserved`, exact `Replay`, conflicting
fingerprint, active-run `Busy`, accepted decision, expired, terminal and apply
claim outcomes. Exact names may follow C3-A types, but semantics may not drift.

## Database Invariants

- FK ownership proves run -> conversation -> project.
- Unique project/conversation/run/call identity.
- One pending/approved/applying proposal per run.
- One approval decision per proposal.
- Approval grant digest can be consumed only once globally.
- No terminal state can transition or apply again.
- `succeeded` and apply `failed` require canonical receipt JSON.
- `pending_approval` and `approved` recover as `interrupted`.
- `applying` stays available for read-only digest reconciliation.
- Distinctive proposed/base strings must be absent from the SQLite file,
  repository records and transition rows.

## File Whitelist

- `crates/storage-sqlite/migrations/0005_write_proposals.sql`
- `crates/storage-sqlite/src/write_proposal.rs`
- `crates/storage-sqlite/tests/write_proposal_repository.rs`
- `crates/storage-sqlite/Cargo.toml`
- `crates/storage-sqlite/src/lib.rs`
- `crates/storage-sqlite/src/migrations.rs`
- `crates/storage-sqlite/tests/migrations.rs`
- `docs/execution/2026-08-01-wave-c3b-storage-ledger-report.md`

Do not edit historical migration SQL or any other file.

## Required TDD And Gates

1. Add migration/repository RED tests before production code and record RED.
2. Implement migration and repository with IMMEDIATE transactions where
   reservation/decision/claim races require serialization.
3. Cover reserve/replay/conflict/busy, ownership mismatch, expiry boundary,
   decision replay, grant replay, apply lease, terminal receipt, transition
   order and recovery.
4. Run:
   - `cargo test -p keeper-storage-sqlite --test write_proposal_repository`
   - `cargo test -p keeper-storage-sqlite --tests`
   - `cargo fmt --all -- --check`
   - `git diff --check`
5. Verify staging is empty.

## Completion Report

Write only
`docs/execution/2026-08-01-wave-c3b-storage-ledger-report.md` with changed files,
RED/GREEN evidence, SQL invariants, test output, risks, and
`COMPLETION: DONE` or `COMPLETION: BLOCKED`.
