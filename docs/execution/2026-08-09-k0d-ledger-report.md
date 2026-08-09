# K0d — Dispatch / acceptance ledger (report)

Date: 2026-08-09  
Worker: grok (K0d)  
Status: **K0D-LEDGER-BLOCKED** (implementation complete; full workspace gate blocked by parallel worker)  
`CARGO_TARGET_DIR=<repo>/target-k0d` throughout.

## 1. One-line

K0c could render document inventory but not controller situation. K0d adds the
two write points from ADR-0004 §2 — **dispatch** and **acceptance** — as SQLite
ledger rows, and extends the renderer with 在途派单 / 最近验收 / **已完成但未验收**.

## 2. Table structure and rationale

Migration: `crates/storage-sqlite/migrations/0011_knowledge_tasks.sql`  
Wired in `migrations.rs` after `0010`.

| Table | Role |
| --- | --- |
| `knowledge_tasks` | 派单台账: title, optional `brief_path`, **slot label** `worker_label`, `dispatched_at`, status, optional `completion_signal` |
| `knowledge_verdicts` | 验收结论: optional `task_id` (controller-self work allowed), `outcome`, gate/e2e, **`summary`**, `residuals`, `decided_at` |
| `knowledge_task_documents` | task × document M2M into `knowledge_documents` |

**Why these tables:** ADR-0004 §2 — registration is a byproduct of dispatch and
acceptance, not a maintenance duty. The only knowledge that does not already
exist as a file is the controller verdict; dispatch is the in-flight half.

**CHECK vs Rust (same split as 0009/0010):**

- SQL CHECK: shape, length bounds, enum membership (`dispatched|completed|blocked|abandoned`, `accepted|rejected|partial`).
- Rust: portable `brief_path` via `keeper_project::validate_portable_path`, privacy red lines (lease / session / host user path), worker_label slot grammar.

**No auto status migration:** `set_task_status` is the only path; writing a
verdict does not flip task status.

## 3. Repository API (`crates/storage-sqlite/src/knowledge.rs`)

| Method | Purpose |
| --- | --- |
| `dispatch_task` | Insert task at `status=dispatched` |
| `set_task_status` | Explicit status + optional completion signal |
| `record_verdict` | Insert verdict; `task_id` optional |
| `attach_task_document` / `list_task_documents` | Task outputs |
| `list_tasks_by_project` / `list_tasks_by_status` | Project-scoped lists |
| `list_verdicts_by_project` | Recent verdicts |
| `list_completed_without_verdict` | Core gap query |
| `get_task` / `get_verdict` | Point lookups |

Exports: `DispatchTask`, `SetTaskStatus`, `RecordVerdict`, `KnowledgeTask`,
`KnowledgeVerdict`, `TaskStatus`, `VerdictOutcome` (+ existing document types).

## 4. Renderer extensions (K0c + K0d)

`RenderInput` gains `tasks` and `verdicts`. Scan loads them from the same DB
(empty when no controller writes yet).

| Section | Content |
| --- | --- |
| **§7 在途派单** | `status=dispatched` — title, **slot**, brief, time |
| **§8 最近验收结论** | `decided_at` desc — outcome, summary, residuals, gate/e2e |
| **§9 已完成但未验收** | `completed` ∧ no verdict row — **never silent** |
| Ledger inventory | Task status counts |

**Honesty retained:** document `unknown` ratio (K0c) stays prominent; empty
ledger sections say “none recorded” rather than inventing 在途 Worker:0.

### How “completed without verdict” is detected

1. Storage: `list_completed_without_verdict` —
   `status='completed' AND NOT EXISTS (verdicts WHERE task_id = t.task_id)`.
2. Render: same filter in pure function over `RenderInput.tasks` +
   `verdicts` (task_ids that appear on any verdict). Listed under §9 with
   completion_signal; accepted tasks excluded.

## 5. §6 Privacy red line — how it is enforced and proven

**Never stored / never rendered:** lease id, Keeper session id, host user path.  
**Stored/rendered instead:** slot label (`grok3`) + timestamps + portable paths.

| Layer | Mechanism |
| --- | --- |
| Write | `contains_privacy_leak` / `validate_worker_label` reject lease marker, session shape, `C:\Users\…` / `/c/Users/…` / `/cygdrive/c/Users/…` (Alice synthetic exception matches check-secrets) |
| Path | `brief_path` only via `validate_portable_path` SSoT — no local path grammar |
| Render | Only `worker_label`, titles, summaries, portable brief paths, RFC3339 times |
| Tests | Privacy samples **assembled at runtime** so source files do not trip `check-secrets.mjs`; assertions still exercise full forbidden values |
| Gate | `node scripts/check-secrets.mjs` → ok |

Render tests: `assert_render_privacy_clean`,  
`render_output_contains_only_slot_labels_not_lease_or_session`.

## 6. Eight required tests — coverage map

| # | Requirement | Where |
| --- | ---: | --- |
| 1 | migration `0011` idempotent | `knowledge_ledger::migration_0011_is_idempotent_and_creates_ledger_tables` |
| 2 | dispatch → complete → accept round-trip | `dispatch_complete_accept_round_trip` |
| 3 | completed without verdict listed | storage `completed_without_verdict_is_detectable`; render `completed_without_verdict_is_listed_never_silent` |
| 4 | verdict without `task_id` | `verdict_may_have_no_task_id`; render `verdict_without_task_id_renders_as_controller_self` |
| 5 | illegal status / outcome rejected | `illegal_task_status_and_verdict_outcome_rejected` + SQL CHECK in migration test |
| 6 | project isolation | `tasks_and_verdicts_isolated_by_project` |
| 7 | render/write free of lease/session/host path | `privacy_red_lines_reject_*`; render privacy asserts |
| 8 | restart recovery | `restart_recovers_tasks_and_verdicts` |

Bonus: `recording_verdict_does_not_auto_change_task_status`.

## 7. Commands / exit codes

| Command | Exit |
| --- | ---: |
| `cargo test -p keeper-storage-sqlite --test knowledge_ledger` | **0** (9 passed) |
| `cargo test -p keeper-storage-sqlite --test knowledge_repository` | **0** (10 passed) |
| `cargo test -p keeper-knowledge-indexer` | **0** (lib + integration) |
| `cargo clippy -p keeper-storage-sqlite -p keeper-knowledge-indexer --all-targets -- -D warnings` | **0** |
| `cargo fmt -p keeper-storage-sqlite -p keeper-knowledge-indexer -- --check` | **0** |
| `node scripts/check-secrets.mjs` | **0** |
| `node scripts/run-worker-gate.mjs` | see §7.1 |

### 7.1 Full worker gate

```text
node scripts/run-worker-gate.mjs
# FAILED at step "cargo-fmt" (exit 1)
# Diff only in crates/runtime-host/src/host/process.rs
#           and crates/runtime-host/tests/governed_process_bridge.rs
```

Those paths are **outside this ticket’s write whitelist** (parallel worker in
flight). K0d must not rustfmt or edit them. Own crates fmt/clippy/tests are
green under `target-k0d`.

**Unblock:** runtime-host worker runs `cargo fmt` on its tree (or lands), then
controller re-runs `node scripts/run-worker-gate.mjs`.

## 8. Untouched (whitelist discipline)

- **Not modified:** `apps/desktop/**`, `crates/kernel-project/**`,
  `crates/git-executor/**`, `crates/kernel-resource/**`,
  `crates/runtime-host/**`, `crates/adapter-grok-acp/**`, `scripts/**`,
  `tests/fixtures/**`
- **Docs:** only this report under `docs/execution/`
- No git add/commit/stash/reset/clean/checkout
- No desktop E2E
- No auth/token reads
- No automatic status migration
- No storage of lease/session/machine paths

## 9. Residual risks

1. **Controller write path not wired** — tables and render exist; the live
   controller still must call `dispatch_task` / `record_verdict` as byproducts
   of dispatch/acceptance (out of this ticket).
2. **Scan DB is document-centric** — handoff from a pure docs scan still has
   empty §7–§9 until ledger rows are written into that DB.
3. **Full workspace gate** may flap while parallel `runtime-host` work leaves
   rustfmt drift; re-gate after that worker formats.
4. Privacy detector is pattern-aligned with `check-secrets.mjs` block rules, not
   a full reimplementation of every codeOnly machine-drive probe.

## 10. Completion

**K0D-LEDGER-BLOCKED: workspace cargo-fmt fails on parallel-worker unformatted `crates/runtime-host/**` (out of K0d whitelist)**

Implementation and package-local gates are complete; full worker gate waits on
the other worker’s fmt.
