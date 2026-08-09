# K1a — Retirement (mark + archive, never auto true-delete)

Date: 2026-08-09  
Worker: grok-k1a  
Controller: Claude  
Workspace: `<repo>`  
`CARGO_TARGET_DIR`: `<repo>/target-k1a`  
Signal: **K1A-RETIREMENT-BLOCKED: BLOCKED-BY-CONCURRENT**  
(Implementation complete; sole gate failure is `cargo fmt --all` on
`crates/runtime-host/tests/**` — outside whitelist, concurrent with Codex.)

---

## 1. One-line

ADR-0004 §5 is enforced in storage + indexer: retirement is **explicit archive**
(status write only) plus **default retrieval exclusion**; scanners only emit
**candidate lists**; **no delete/purge API**.

---

## 2. Archive status migration (still explicit)

| Operation | Behavior |
| --- | --- |
| `SqliteKnowledgeRepository::archive(id)` | Explicit `status = archived`. Idempotent. |
| `SqliteKnowledgeRepository::unarchive(id)` | If archived → `unknown`; if already live → no-op (does not invent prior status). |
| Scanner / link write / register | **Never** calls archive. Never auto-sets `archived`. |

Proof of non-automatic migration:

- Storage test `adding_supersedes_link_does_not_auto_change_status` — link write alone does not change status.
- Indexer test `archive_preserves_non_unknown_on_rescan_and_is_explicit_only` — Active survives rescan; only explicit `archive()` reaches `archived`; rescan preserves `archived`.
- Indexer `scan.rs` asserts (test) that `.archive(` is absent from the scanner path.

Non-`unknown` judgments (K0b): rescan still only refines `unknown` → stronger status; Active / Evidence / Archived / etc. are preserved. Archive is a separate **explicit** write on the chosen document only and does not touch sibling rows (`archive_does_not_overwrite_other_docs_non_unknown_status`).

**No migration 0012:** `archived` already exists in `0010_knowledge.sql` CHECK. No schema change required.

---

## 3. Default exclusion — hard to forget

ADR-0004: *「归档后默认检索不可见,否则『淘汰』是假的。」*

### Query (storage)

- `list_by_project(project_id)` → **always** `status != 'archived'` (SQL filter in the default branch).
- `list_documents(project_id, ListDocumentsQuery { include_archived })` is the only opt-in.
- `ListDocumentsQuery::default()` has `include_archived: false`.
- Point lookup `get` / `get_by_path` still returns archived rows (explicit id/path is not default search).

### Render (indexer)

- `RenderInput.include_archived` (required field; call sites set `false` for defaults).
- **Single choke-point** at the start of `render_status_view`:
  1. drop archived documents,
  2. drop links whose from/to id or `to_path` is archived (so supersession sections cannot re-leak basenames),
  3. strip archived paths from audit buckets.
- Visibility banner always states default exclusion vs opt-in.
- Scan’s handoff render always passes `include_archived: false`.
- Scanner **internal** inventory uses `list_documents(..., include_archived: true)` so rescan still sees archived rows for digest/status preservation — that is admin path, not default retrieval.

---

## 4. Retirement candidate rules (list only)

Implemented in `audit::build_retirement_buckets`:

| Rule | Implementation |
| --- | --- |
| Explicit supersedes **and** superseder registered **and** target registered | `rel=supersedes` + `from_document_id` in registry + `to_document_id` Some and resolved → path enters `retirement_candidates` |
| Zero-inbound | **Not** a signal; never adds candidates |
| `verification` / `decision` orphans | `expected_orphans_non_problem` with label **预期孤儿,非问题**; not candidates unless (separately) an explicit supersedes link targets them |
| Orphan brief | `briefs_pending_review` / unpaired briefs (待复核); not candidates |
| Date old / “looks stale” | Not implemented as a signal |
| `status=superseded` alone | **Not** enough without a resolved supersedes link |

Scanner **only produces the list**. Applying archive remains a human/controller call to `archive()`.

---

## 5. Why true delete is not implemented

ADR-0004: real deletion requires **approval + receipt**. Approval path is not designed. This ticket **intentionally leaves blank**:

- No `delete` / `purge` / `remove_document` on `SqliteKnowledgeRepository`.
- No `DELETE FROM knowledge_documents` in the module.
- Indexer has no delete/purge/auto-archive apply path.

Source assertions: `knowledge_api_has_no_delete_or_purge_entry`, `indexer_has_no_delete_or_purge_api`.

---

## 6. Nine required tests

| # | Requirement | Where |
| --- | --- | --- |
| 1 | archive / unarchive round-trip | `storage-sqlite` `archive_unarchive_round_trip` |
| 2 | default query excludes archived; `include_archived` shows | `default_list_excludes_archived_include_flag_shows_them` |
| 3 | render default excludes archived (separate path) | `render_default_excludes_archived_include_flag_shows` |
| 4 | candidates only explicit supersession + registered target | `retirement_candidates_only_explicit_supersession_registered_target` + audit unit tests |
| 5 | zero-inbound not in candidates (negative) | `zero_inbound_never_retirement_candidate_negative` |
| 6 | verification/decision orphans labeled + not candidates | `expected_orphans_verification_decision_labeled_not_candidates` |
| 7 | orphan brief → 待复核 not candidates | `orphan_brief_goes_to_pending_review_not_retirement` |
| 8 | archive / rescan does not clobber non-unknown | `archive_preserves_non_unknown_on_rescan_and_is_explicit_only` + storage sibling-doc test |
| 9 | no delete/purge API surface | both crates’ no-delete tests |

---

## 7. Command exit codes

| Command | Exit / result |
| --- | --- |
| `cargo test -p keeper-storage-sqlite --test knowledge_repository --test knowledge_ledger` | knowledge_repository **14 ok**, knowledge_ledger **9 ok** |
| `cargo test -p keeper-knowledge-indexer` | indexer_rules **20 ok**, ledger_cli **8 ok**, lib unit tests ok |
| `cargo fmt -p keeper-storage-sqlite -p keeper-knowledge-indexer -- --check` | **exit 0** (whitelist packages clean) |
| `node scripts/run-worker-gate.mjs` | **FAILED at cargo-fmt** — diffs only in `crates/runtime-host/tests/governed_{process,read,write}_bridge.rs` (forbidden path; concurrent worker). Workspace/secrets steps **ok**. |

Environment: `CARGO_TARGET_DIR=<repo>/target-k1a`.

---

## 8. What this worker did **not** touch

- `crates/runtime-host/**`
- `crates/governance-port/**`
- `crates/adapter-grok-acp/**`
- `apps/desktop/**`
- `crates/kernel-project/**`
- `crates/git-executor/**`
- `scripts/**`
- `tests/fixtures/**`
- Any `docs/` file other than this report
- No git add/commit/stash/reset/clean/checkout/restore
- No desktop E2E
- No auth/token files
- No rewrite of migrations `0001`–`0011`; no new `0012` (not needed)

Whitelist writes only:

- `crates/storage-sqlite/src/knowledge.rs`, `lib.rs`, `tests/knowledge_repository.rs`
- `crates/knowledge-indexer/**` (lib, audit, scan, render, tests)
- `docs/execution/2026-08-09-k1a-retirement-report.md`

---

## 9. Residual risks

1. **Call sites outside whitelist** that previously assumed `list_by_project` returns archived rows will now miss them unless they opt in. Intended; concurrent workers editing other crates should adopt `include_archived` when doing inventory.
2. **Unarchive → `unknown`** only — prior `active`/`superseded` is not restored (no previous-status column). Callers must re-label explicitly.
3. **True delete** remains future work (approval + receipt). Archive growth is unbounded by design (ADR-0004 consequences).
4. **Keeper bridge boot** returned HTTP 400 in this environment; work did not depend on it.
5. **Gate:** `BLOCKED-BY-CONCURRENT` — `cargo fmt --all` fails only on
   `runtime-host` bridge tests (Codex in flight). This worker did not format
   or edit those files. Controller should re-run gate after concurrent settle
   or with a whitelist-scoped fmt step.

---

## 10. Files changed (summary)

| Area | Change |
| --- | --- |
| `storage-sqlite` knowledge | `ListDocumentsQuery`, default-excluding list SQL, `archive` / `unarchive`, docs |
| `knowledge-indexer` audit | candidates from resolved supersedes links only |
| `knowledge-indexer` render | `include_archived` choke-point (docs + links + audit) |
| `knowledge-indexer` scan | full inventory via `include_archived: true`; render default false |
| tests | nine required scenarios + no-delete surface |

---

K1A-RETIREMENT-BLOCKED: BLOCKED-BY-CONCURRENT — worker-gate cargo-fmt only fails on runtime-host tests (out of whitelist; concurrent)
