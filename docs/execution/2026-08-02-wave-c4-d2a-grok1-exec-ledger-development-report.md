# C4-D2A Grok1 development report — metadata-only execution ledger

Date: 2026-08-02

Worker: real Grok CLI slot `grok1`
Worker session: `<keeper-session>`
Controller parent session: `<keeper-session>`
Task: `c4-d2a-exec-ledger`
Status: **DONE** (not controller-accepted)

## Objective

Add migration `0007_exec_proposals.sql` and a typed SQLite repository for the
C4 process proposal lifecycle. SQLite is durable authority for metadata,
digests and terminal receipts only; sealed executable/argv/native cwd,
environment, raw grant and transcript remain outside storage.

## Public boundary delivered

| Symbol | Role |
| --- | --- |
| `StoredExecProposal` | Durable identity + digests + lifecycle + terminal fields |
| `ExecProposalReserveInput` | Host reservation payload (no argv/native/env/grant/transcript) |
| `ExecReservationOutcome` | `Reserved` / `Replay` / `Conflict` / `Busy` |
| `ExecDecisionOutcome` | `Accepted` / `Terminal` / `Conflict` / `Expired` |
| `ExecRunClaimOutcome` | `Claimed` / `Busy` / `Terminal` / `Expired` / `Conflict` |
| `ExecTerminalReceipt` | Typed terminal receipt (`deny_unknown_fields`) |
| `SqliteExecProposalRepository` | Transactional ledger API |
| `MAX_RUN_LEASE_MS` | `300_000` |
| `EXEC_RESULT_JSON_BYTE_CAP` | `8_192` |
| `ExecProposalStoreError` | Fail-closed store errors |

### Repository methods

| Method | Behavior |
| --- | --- |
| `reserve` / `reserve_at` | Storage-owned TTL (`EXEC_PROPOSAL_TTL_SECS` = 600); replay / conflict / busy |
| `decide` | Conversation-scoped approve_once / reject; grant digest single-use |
| `claim_run` | Single-use `Approved → Running` with owner + lease |
| `complete_run` | Terminal `Succeeded` / `Failed` with typed receipt; identical replay |
| `interrupt_run` | Owner-aware live interrupt of `running` |
| `transition` | Policy-validated non-claim transitions only |
| `recover_interrupted` | Marks `pending_approval` / `approved` / `running` → `interrupted` |
| `history` | Transition log |
| `list_for_conversation` | Conversation-scoped listing |
| `list_running` | Read-only `running` enumeration |

State transitions use `keeper_policy::process` (`validate_exec_transition`,
`is_exec_terminal`, `is_exec_expired`) exactly and fail closed on invalid or
terminal mutation.

## Schema (`0007_exec_proposals.sql`)

### Tables

**`exec_proposals`** columns:

| Column | Notes |
| --- | --- |
| `proposal_id` | PK, UUID length 36 |
| `reservation_key` | UNIQUE, 1..=512 |
| `proposal_fingerprint` | `sha256:` + 64 lowercase hex |
| `project_id` / `conversation_id` / `run_id` | FK graph |
| `call_id` | 1..=128 |
| `program` | program label 1..=32, `^[a-z][a-z0-9_-]{0,31}$` |
| `argv_digest` | canonical digest only (no raw argv) |
| `portable_cwd` | optional portable relative path; no `\` / `:` / absolute |
| `state` | pending_approval, approved, running, succeeded, failed, rejected, expired, cancelled, interrupted |
| `expires_at` | storage-owned |
| `result_json` | required only for succeeded/failed; ≤8192, `json_valid` |
| `run_owner` / `run_lease_expires_at` | claim lease |
| `started_at` / `completed_at` | timestamps |
| `exit_code`, `duration_ms`, `stdout_digest`, `stderr_digest`, `captured_bytes` | terminal metadata |
| `truncated`, `encoding_lossy`, `cancelled`, `timed_out` | 0/1 flags |
| `created_at` / `updated_at` | timestamps |

Also: `UNIQUE (project_id, conversation_id, run_id, call_id)` and
state/result/terminal-field coupling CHECK (apply terminals require digests +
flags + `result_json`; all other states keep terminal fields null).

**`exec_proposal_transitions`**: append-only history; same state/result rule;
`UNIQUE (proposal_id, state)`.

**`exec_approval_decisions`**: one decision per proposal; globally unique
`approval_grant_digest` (digest only — never raw grant).

### Indexes

| Index | Purpose |
| --- | --- |
| `idx_exec_proposals_one_active_per_run` | **Partial unique** on `run_id` WHERE state IN (`pending_approval`, `approved`, `running`) |
| `idx_exec_proposals_conversation_created` | conversation listing |
| `idx_exec_proposals_state` | state scans |
| `idx_exec_proposal_transitions_order` | history order |
| `idx_exec_approval_decisions_grant` | grant lookup |

### Explicit non-storage

No columns for: raw argv, native executable/root/cwd, environment, stdout,
stderr, transcript, or raw approval grant.

## Changed files (allowlist only)

| Path | Action |
| --- | --- |
| `crates/storage-sqlite/migrations/0007_exec_proposals.sql` | **Created** |
| `crates/storage-sqlite/src/exec_proposal.rs` | **Created** |
| `crates/storage-sqlite/src/lib.rs` | **Modified** — module + re-exports |
| `crates/storage-sqlite/src/migrations.rs` | **Modified** — apply 0007 after 0006 |
| `crates/storage-sqlite/tests/exec_proposal_repository.rs` | **Created** |
| `docs/execution/2026-08-02-wave-c4-d2a-grok1-exec-ledger-development-report.md` | **Created** (this report) |

**Not modified:** Cargo.toml/lockfile, protocol, policy, process-executor,
tool-executor, runtime-host, Tauri, renderer, scripts.

HEAD preserved: `7b7f70f`. No stage, commit, reset, restore, clean, package
or publish.

## TDD gates

### RED

```text
cargo test -p keeper-storage-sqlite --test exec_proposal_repository -- --nocapture
```

- **Expected:** unresolved imports / missing module (`SqliteExecProposalRepository`,
  `ExecProposalReserveInput`, etc.) before implementation.
- Observed on first compile attempt: E0432/E0433 missing exports (module absent).

### GREEN

```text
cargo test -p keeper-storage-sqlite --test exec_proposal_repository
cargo test -p keeper-storage-sqlite --tests
cargo check --workspace --all-targets
cargo fmt --all --check
git diff --check -- crates/storage-sqlite
```

| Command | Result |
| --- | --- |
| `exec_proposal_repository` | **22/22** passed |
| `keeper-storage-sqlite --tests` | all targets green (lib 2; conversation* 1+7+6+8; exec 22; migrations 11; project 3; resource 5; tool_call 8; write 25) |
| `cargo check --workspace --all-targets` | exit 0 |
| `cargo fmt --all --check` | exit 0 (after one `cargo fmt --all` pass) |
| `git diff --check -- crates/storage-sqlite` | exit 0 (LF/CRLF warnings only on pre-existing tracked files) |

## Privacy byte-scan evidence

Test `argv_native_path_grant_env_and_transcript_never_enter_sqlite`:

1. Host-only secrets: raw argv token, native exe path, native cwd, raw grant,
   environment value, stdout/stderr transcript strings.
2. Reserve → decide (grant **digest** only) → claim → complete with stream
   **digests** only.
3. Asserts secrets absent from:
   - `StoredExecProposal` debug formatting
   - history debug formatting
   - raw SQLite file bytes (`std::fs::read`)
   - `result_json` payloads
4. `PRAGMA table_info(exec_proposals)` forbids columns named argv / executable /
   native_cwd / environment / stdout / stderr / transcript / approval_grant /
   raw_grant.
5. Required columns present: `program`, `argv_digest`, `portable_cwd`,
   `proposal_fingerprint`, digests, flags, `result_json`.

## Staging state

- Allowlist files: unstaged only (`??` / dirty within allowed paths).
- `git diff --cached --name-only`: empty for this worker’s deliverables.
- Pre-existing C1–C3 / D1 dirty tree left untouched outside allowlist.

## Residual risks (controller)

1. **Integration not in scope:** Task 7 (`GovernedProcessToolGateway`) must wire
   seal → reserve → decide → claim → execute → receipt; storage does not spawn.
2. **Run events:** C4 process event kinds are not part of 0007; runtime/event
   surface remains a later task.
3. **Portable cwd validation** is storage-bounded (no absolute/`\`/`:`/`..`);
   full project containment / reparse checks remain executor-side.
4. **Program catalog** (deny shell/git labels) is not enforced in storage —
   only label grammar. Catalog policy is tool-executor scope.
5. **Lease re-claim after expiry** allows a new owner to reclaim `running` when
   the prior lease is dead (mirrors write apply). Live interrupt still requires
   exact owner + live lease.
6. Not controller source-reviewed; independent acceptance remains controller-owned.

## Completion

`COMPLETION: complete`
