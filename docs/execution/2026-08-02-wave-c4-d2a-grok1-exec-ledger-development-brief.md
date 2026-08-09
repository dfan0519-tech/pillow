# Pillow C4 D2A — metadata-only execution ledger development brief

## Worker identity and session

- Worker: real Grok CLI slot `grok1`.
- Keeper boot: `project=pillow`, `task=c4-d2a-exec-ledger`,
  `agentName=grok1`, `agentRole=worker`,
  `<local-parent-session>`.
- Repository: `this repository` only. Keeper and reference repositories are
  read-only.
- Preserve HEAD `7b7f70f` and the accepted dirty C1–C3 plus D1 tree. No stage,
  commit, reset, restore, clean, package or publish.

## Inputs to read completely

1. All three applicable AGENTS files.
2. `docs/superpowers/specs/2026-08-02-c4-governed-process-git-design.md`
3. Task 6 in `docs/superpowers/plans/2026-08-02-c4-governed-process-git-implementation.md`
4. Existing write/tool-call repository migrations, modules and tests in
   `crates/storage-sqlite/` as local patterns.
5. `crates/kernel-policy/src/process.rs` and the D1 policy report.

## Goal

Add migration `0007_exec_proposals.sql` and a typed SQLite repository for the
C4 process proposal lifecycle. SQLite is durable authority for metadata,
digests and terminal receipts only; sealed executable/argv/native cwd,
environment, raw grant and transcript remain outside storage.

## Required interfaces

Produce:

- `StoredExecProposal`
- `ExecProposalReserveInput`
- `ExecReservationOutcome`
- `ExecDecisionOutcome`
- `ExecRunClaimOutcome`
- `ExecTerminalReceipt`
- `SqliteExecProposalRepository`

The repository must support reservation/replay/conflict, approve-once/reject,
one execution claim, terminal completion, expiry, restart interruption,
conversation-scoped listing and replay of the same canonical receipt.

## Storage invariants

- Store proposal/project/conversation/run/tool-call identity, program label,
  argv digest, portable cwd, sealed fingerprint, lifecycle, timestamps, grant
  digest, exit metadata, byte counts, stream digests, truncation/encoding flags
  and bounded terminal result JSON.
- Never store raw argv, native executable/root/cwd, raw approval grant,
  arbitrary environment or stdout/stderr transcript.
- Use a partial unique index allowing at most one non-terminal execution
  proposal per run.
- An identical reservation replays; same identity with a different fingerprint
  conflicts; decisions and run claims are single-use.
- Startup recovery marks non-terminal proposals interrupted; it never replays
  authority.
- State transitions must match `keeper_policy::process` exactly and fail closed
  on invalid/terminal mutation.

## TDD workflow

1. Add RED migration/repository tests first for reservation/replay/conflict,
   non-terminal uniqueness, decision single-use, run claim, terminal receipt,
   expiry, restart recovery and conversation listing.
2. Add raw-byte and SQL-column checks proving no argv, native path, raw grant,
   environment or transcript bytes are retained.
3. Run
   `cargo test -p keeper-storage-sqlite --test exec_proposal_repository -- --nocapture`
   and record the expected missing migration/module failure.
4. Implement migration and repository using existing storage error and
   transaction patterns; do not add a new dependency.
5. Run:
   - `cargo test -p keeper-storage-sqlite --test exec_proposal_repository`
   - `cargo test -p keeper-storage-sqlite --tests`
   - `cargo check --workspace --all-targets`
   - `cargo fmt --all --check`
   - `git diff --check -- crates/storage-sqlite`

## Exclusive write allowlist

- `crates/storage-sqlite/migrations/0007_exec_proposals.sql`
- `crates/storage-sqlite/src/exec_proposal.rs`
- `crates/storage-sqlite/src/lib.rs`
- `crates/storage-sqlite/src/migrations.rs`
- `crates/storage-sqlite/tests/exec_proposal_repository.rs`
- `docs/execution/2026-08-02-wave-c4-d2a-grok1-exec-ledger-development-report.md`

No manifest, lockfile, protocol, policy, process executor, runtime-host, Tauri,
renderer or script file may change. Stop and report if a required interface
would need an out-of-allowlist change.

## Completion contract

Report schema columns/indexes, exact changed files, RED/GREEN commands and
counts, privacy byte-scan evidence, staging state and residual risks. Post one
Keeper completion message to the parent session and emit
`COMPLETION: complete` or a precise blocker. The controller owns integration
and acceptance.
