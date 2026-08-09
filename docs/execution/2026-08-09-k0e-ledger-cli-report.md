# K0e — Ledger CLI + stale detection (report)

Date: 2026-08-09  
Worker: grok (K0e)  
Status: **K0E-LEDGER-CLI-BLOCKED** (implementation complete; full workspace gate blocked by parallel worker)  
`CARGO_TARGET_DIR=<repo>/target-k0e` throughout.

## 1. One-line

K0d built the dispatch/acceptance ledger with no write entrypoint. K0e adds
non-interactive CLI subcommands on `knowledge-indexer` and a **report-only**
stale check that mechanically flags handoff documents whose claims disagree
with the ledger.

## 2. Subcommand design

Binary: `knowledge-indexer` (package `keeper-knowledge-indexer`).

| Subcommand | Purpose |
| --- | --- |
| `task dispatch` | Record a dispatch (title, optional brief path, **slot label**, project) |
| `task complete` | Explicit status set (default `completed`) + optional completion signal |
| `verdict record` | Record acceptance (outcome, summary, optional gate/e2e/residuals/task) |
| `ledger status` | Print 在途派单 / 待验收 / 最近结论 |
| `ledger stale` | Compare a handoff markdown file to the ledger (detect only) |
| `scan` (default) | Existing K0b–K0d docs scan (unchanged behavior) |

Shared flags: `--db PATH` (required for ledger commands), `--project NAME`
(default `pillow`), `--repo DIR` (project root when creating the FK row).

### Non-interactive call examples

```bash
export CARGO_TARGET_DIR=<repo>/target-k0e
DB=path/to/knowledge.db

# Dispatch (controller byproduct)
knowledge-indexer task dispatch \
  --db "$DB" --project pillow --repo . \
  --title "K0e ledger CLI" --slot grok3 \
  --brief docs/execution/k0e-brief.md
# stdout: ok dispatched task_id=<uuid> status=dispatched slot=grok3 title=...

# Complete
knowledge-indexer task complete \
  --db "$DB" --task-id <uuid> \
  --status completed --signal "K0E-LEDGER-CLI-COMPLETE"

# Verdict
knowledge-indexer verdict record \
  --db "$DB" --project pillow \
  --task-id <uuid> --outcome accepted \
  --summary "Accepted" --gate "exit 0" --residuals "none"

# Status
knowledge-indexer ledger status --db "$DB" --project pillow

# Stale check (never rewrites the document)
knowledge-indexer ledger stale \
  --db "$DB" --project pillow \
  --doc docs/handoff/2026-08-08-pillow-controller-handoff.md
```

No command reads stdin. Integration test runs the binary with `Stdio::null()`.

## 3. Exit codes

| Command | Exit | Meaning |
| --- | ---: | --- |
| `task dispatch` / `task complete` / `verdict record` | **0** | recorded |
| same | **1** | validation / storage / missing flags |
| `ledger status` | **0** / **1** | printed / error |
| `ledger stale` | **0** | fresh (no findings) |
| `ledger stale` | **3** | **STALE** findings (scriptable, distinct from I/O errors) |
| `ledger stale` | **1** | read/load error |
| `scan` | **0** / **1** / **2** | ok / scan failed / write report path failed |
| unknown / help misuse | **1** | usage |

## 4. Stale detection — how the three cases work

Pure function: `check_stale(doc_text, &LedgerSnapshot) -> StaleReport`  
in `crates/knowledge-indexer/src/stale.rs`. **Never mutates docs or ledger.**

### Case 1 — in-flight count mismatch

Extract claims from lines matching (among others):

- `在途 Worker: N` / `在途Worker: N` (the ADR-0004 failure mode)
- `在途派单: N`
- `Active Grok Worker | N` / `Grok 在途 Worker | N`
- `In-flight workers: N` / `dispatched count: N`

Compare each claimed `N` to `count(tasks where status=dispatched)`.  
If unequal → finding with both sides: claimed vs ledger count + line excerpt.

### Case 2 — completed without verdict, unmentioned

Uses storage's `list_completed_without_verdict` (loaded into
`LedgerSnapshot.completed_without_verdict`).  
For each such task, require the handoff to mention **title** (case-insensitive
substring) or **task_id**. Silence → finding naming `title`, `task_id`, `slot`.

### Case 3 — document mentions a task absent from the ledger (reverse)

Extract explicit mentions (script/controller-friendly, not NLP):

- `task-title: …` / `任务标题: …`
- `task-id: …`
- `Task: …` / `在途任务: …` / `派单: …`
- Markdown table rows `| title | \`grokN\` |` (slot-shaped second column)

A mention is missing if no ledger task has that title (normalized) or id.  
Finding states the mention and excerpt.

### Negative control (no false alarm)

When:

1. every `在途 Worker: N` equals ledger dispatched count,
2. every completed-without-verdict task is mentioned by title or id,
3. every extracted task mention exists in the ledger,

→ `findings` is empty, report says **fresh**, exit **0**.  
Covered by unit test `consistent_document_not_flagged` and integration
`stale_negative_control_consistent_not_flagged`. Without this, a detector that
always alarms would pass the three positive cases.

## 5. Privacy

| Layer | Mechanism |
| --- | --- |
| Write | K0d storage rejects lease marker, session shape, host user paths; slot label grammar only |
| CLI args | `--slot` is a label; lease-as-slot fails at storage (tested) |
| Status / stale output | Only `task_id`, titles, `worker_label`, portable paths, outcomes, counts |
| Tests | Forbidden samples **assembled at runtime** (no contiguous literals in source); `assert_privacy_clean` on status, stale report, and binary stdout/stderr |

Gate: `node scripts/check-secrets.mjs` → ok.

## 6. Six required tests — coverage map

| # | Requirement | Where |
| --- | ---: | --- |
| 1 | dispatch → complete → verdict round-trip | `ledger_cli::dispatch_complete_verdict_round_trip` (+ lib unit) |
| 2 | `ledger status` lists 在途 / 待验收 | `ledger_cli::ledger_status_lists_inflight_and_pending` |
| 3 | Stale cases 1 / 2 / 3 | `stale_case1_*`, `stale_case2_*`, `stale_case3_*` (+ lib units) |
| 4 | Negative control | `stale_negative_control_consistent_not_flagged` |
| 5 | Privacy assertions | `privacy_cli_output_and_stale_report_clean` + asserts in other tests |
| 6 | Non-interactive (no stdin) | `cli_binary_noninteractive_dispatch_status_stale` (`Stdio::null()`, exit 3 on stale) |

## 7. Commands / package-local gates

| Command | Exit |
| --- | ---: |
| `cargo test -p keeper-knowledge-indexer --all-targets` | **0** (lib 23 + indexer_rules 13 + ledger_cli 8) |
| `cargo clippy -p keeper-knowledge-indexer --all-targets -- -D warnings` | **0** |
| `cargo fmt -p keeper-knowledge-indexer -- --check` | **0** |
| `node scripts/check-secrets.mjs` | **0** |
| `node scripts/run-worker-gate.mjs` | see §7.1 |

### 7.1 Full worker gate

```text
node scripts/run-worker-gate.mjs
# FAILED at step "cargo-fmt" (exit 1)
# Diff only in crates/adapter-grok-acp/src/prompt.rs
```

That path is **outside this ticket’s write whitelist** (parallel worker in
flight). K0e must not rustfmt or edit it. Own crate fmt/clippy/tests are green
under `target-k0e`.

**Classification:** `BLOCKED-BY-CONCURRENT`  
**Unblock:** adapter-grok-acp worker runs `cargo fmt` on its tree (or lands),
then controller re-runs `node scripts/run-worker-gate.mjs`.

## 8. Untouched (whitelist discipline)

- **Not modified:** `crates/storage-sqlite/**`, `crates/runtime-host/**`,
  `crates/adapter-grok-acp/**`, `crates/governance-port/**`, `apps/desktop/**`,
  `crates/kernel-project/**`, `crates/git-executor/**`, `scripts/**`,
  `tests/fixtures/**`
- **Docs:** only this report under `docs/execution/`
- No git add/commit/stash/reset/clean/checkout/restore
- No desktop E2E
- No auth/token reads
- No automatic document rewrite
- No automatic task status migration (only explicit `task complete`)
- No storage of lease/session/machine paths

## 9. Residual risks

1. **Stale mention extraction is pattern-based**, not full NLP. Handoffs that
   only narrate task titles in free prose (without `task-title:` / table / etc.)
   may miss reverse checks; case 1 (`在途 Worker: N`) and case 2 (title
   substring) still catch the ADR failure modes.
2. **Controller must call the CLI** at dispatch/acceptance — registration is a
   byproduct only if wired into the controller path (out of this ticket).
3. **Full workspace gate** flaps while parallel `adapter-grok-acp` work leaves
   rustfmt drift.
4. `task complete` does not require `--project` (looks up by `task_id`);
   `ledger status` / `stale` require the project row to exist (created on first
   dispatch).

## 10. Files written (whitelist only)

| Path | Role |
| --- | --- |
| `crates/knowledge-indexer/src/ledger.rs` | Dispatch/complete/verdict/status helpers |
| `crates/knowledge-indexer/src/stale.rs` | Pure stale detection + report format |
| `crates/knowledge-indexer/src/bin/knowledge_indexer.rs` | CLI subcommands |
| `crates/knowledge-indexer/src/lib.rs` | Module exports |
| `crates/knowledge-indexer/tests/ledger_cli.rs` | Six-requirement integration tests |
| `docs/execution/2026-08-09-k0e-ledger-cli-report.md` | This report |

## 11. Completion

**K0E-LEDGER-CLI-BLOCKED: workspace cargo-fmt fails on parallel-worker unformatted `crates/adapter-grok-acp/**` (BLOCKED-BY-CONCURRENT)**

Implementation and package-local gates are complete; full worker gate waits on
the other worker’s fmt.
