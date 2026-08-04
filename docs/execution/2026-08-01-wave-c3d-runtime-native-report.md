# Wave C3-D Report — Runtime, Adapter And Native Commands

Owner: grok3 implementation worker (controller final fix)
Controller: Codex Desktop
Keeper worker session: `<keeper-session>` / bridge `<keeper-session>`
Runtime allocation: `grok3` / `<keeper-lease>`
Date: 2026-08-01
Task: C3-D controller final fix (fail-closed storage, recovery Neither, durable list, diff expiry, clippy)

## Scope

Close the five remaining controller rejection findings after the first
rejection repair. Do not start C3-E. No renderer files. No stage/commit.

## Final-fix findings closed

| # | Finding | Repair |
| --- | --- | --- |
| 1 | Fabricated terminal on storage failure | `finalize_*` / `reconcile_expired_for_owner` use one transition + optional read; never invent metadata; quarantine live secrets and return `Gateway`; `apply` Err same path |
| 2 | Unresolvable root mapped to `Neither` | Missing/unresolvable project root leaves applying row unchanged and returns `Gateway`; only executor observation may emit `Neither` |
| 3 | List empty after restart (no bound run) | `list_write_proposals` uses host `governed_write_gateway` for durable rows; live state remains owner-scoped |
| 4 | Diff open ignored expiry/terminal | `open_write_proposal_diff` reconciles expiry first; requires non-terminal and `now < expires_at` |
| 5 | Workspace Clippy on unit struct | `ResourceReader::default()` → `ResourceReader` in resource core test |

## Changed files (final fix)

### Storage

| Path | Change |
| --- | --- |
| `crates/storage-sqlite/Cargo.toml` | Non-default `test-support` feature |
| `crates/storage-sqlite/src/write_proposal.rs` | `set_fail_writes_for_tests` / write-attempt counter; `begin_mutation` on all mutators |

### Runtime host

| Path | Change |
| --- | --- |
| `crates/runtime-host/Cargo.toml` | `test-support` enables storage `test-support` |
| `crates/runtime-host/src/governed_write.rs` | Fail-closed finalize/reconcile; host-gateway list; diff expiry; recovery root error; apply quarantine |
| `crates/runtime-host/src/host/mod.rs` | List passes host write gateway |
| `crates/runtime-host/tests/governed_write_bridge.rs` | +4 adversarial/restart cases (17 total with test-support) |

### Resource (clippy only)

| Path | Change |
| --- | --- |
| `crates/kernel-resource/tests/read_only_resource_core.rs` | Mechanical `ResourceReader` unit-struct construct |

### Docs

| Path | Change |
| --- | --- |
| `docs/execution/2026-08-01-wave-c3d-runtime-native-report.md` | This report |
| `docs/execution/2026-08-01-wave-c3d-controller-final-fix-brief.md` | Brief (controller) |

## Exact command DTOs (unchanged)

```text
list_write_proposals(conversationId) -> WriteProposalDto[]
decide_write_proposal(conversationId, proposalId, approvalGrant, decision)
  -> WriteProposalDto
open_write_proposal_diff(conversationId, proposalId) -> WriteDiffDto
```

Grant contract, event contract, and provider `pillow/write_tool` contract unchanged.

## RED / GREEN evidence (final fix)

| Case | Test |
| --- | --- |
| Storage fail after live: no fabricated terminal, Gateway only, one failed write, no grant/diff | `storage_failure_after_live_never_fabricates_terminal` (`--features test-support`) |
| Unresolvable applying root leaves applying; valid root still reconciles | `unresolvable_applying_root_leaves_row_and_errors` |
| Restart list without bound run returns durable interrupted, no grant/diff | `durable_list_after_restart_without_bound_run` |
| Direct diff at/past expiry fails, wakes waiter, durable expired | `direct_diff_at_expiry_returns_not_live_and_wakes_waiter` (`--features test-support`) |
| Prior rejection suite retained | 13 prior bridge cases + lib grant/delivery tests |

## Gates run

```text
cargo test -p keeper-runtime-host --features test-support --tests --lib -- --test-threads=1
  PASS  (16 lib + 17 governed_write_bridge + other host suites)
cargo test -p keeper-tool-executor --tests --lib
  PASS  (incl. governed_write 17 + write_diff 12)
cargo test -p keeper-storage-sqlite --tests --lib
  PASS  (incl. write_proposal_repository)
cargo test -p keeper-adapter-grok-acp --tests --lib
  PASS  (incl. 3 governed_write_roundtrip)
cargo test -p pillow-desktop --lib write_
  PASS  (5 write_ tests)
cargo test -p keeper-resource --tests --lib
  PASS  (9 resource core)
cargo check --workspace
  PASS
cargo clippy --workspace --all-targets -- -D warnings
  PASS
cargo fmt --all -- --check
  PASS
git diff --check
  PASS (LF/CRLF warnings only)
git diff --cached --name-only
  empty (staging empty)
```

## Remaining risks

1. Process-wide coordinator remains; isolation is owner-UUID only (by design).
2. Storage fail-write seam is non-default `test-support` only; production has no mutation hooks.
3. `recover_write_proposals` without a root resolver now fails closed if any
   `applying` row exists (desktop always supplies project roots).
4. C3-E approval UI and C3-F desktop integration remain separate waves.

## COMPLETION: DONE
