# K1b — Handoff render that can replace the handwritten brief (report)

Date: 2026-08-09  
Worker: grok (K1b)  
Status: **K1B-HANDOFF-RENDER-BLOCKED** (implementation complete; full workspace gate blocked by parallel worker)  
`CARGO_TARGET_DIR=<repo>/target-k1b` throughout.

## 1. One-line

K0c document status + K0d/K0e dispatch/acceptance ledger are now **one ops-first
composite** (`render_handoff_view`) that can partially replace the handwritten
controller handoff — and that **prints, every time, what it still cannot replace**.

## 2. Implementation

| Piece | Path | Role |
| --- | --- | --- |
| Composite renderer | `crates/knowledge-indexer/src/render.rs` | `render_handoff_view` |
| Status view (kept) | same | `render_status_view` (docs-first; K0c/K0d tests) |
| Scan wiring | `crates/knowledge-indexer/src/scan.rs` | `handoff_markdown` ← composite |
| Export | `crates/knowledge-indexer/src/lib.rs` | `render_handoff_view` |
| CLI | `bin/knowledge_indexer.rs` | `--handoff` writes composite; report paths excluded from rescan |
| Unit tests | `render.rs` `unit_tests` | six K1b requirements + purity |
| Integration | `tests/indexer_rules.rs` | scan → pure handoff, five sections, 无在途 |
| Matrix | `docs/research/2026-08-09-handoff-renderability-matrix.md` | §-by-§ vs handwritten |
| This report | `docs/execution/2026-08-09-k1b-handoff-render-report.md` | |

### Composite section map (§4.1) and data sources

| § | Title | Data source |
| --- | --- | --- |
| — | Honesty: `unknown` ratio | Audit / document statuses |
| 1 | 当前在途 (Current in-flight) | `tasks` with `status=dispatched` → title, **slot label**, brief path |
| 2 | ⚠️ 待验收 (Pending acceptance) | `completed` ∧ no matching verdict — **逐条** table + bullet list |
| 3 | 最近验收结论 | `verdicts` ordered by `decided_at` → **summary** + **residuals** |
| 4 | 文档状态 | K0c inventory / lifecycle / supersession / contradictions / 待复核 / 预期孤儿 |
| 5 | 本渲染仍替不掉什么 | Fixed honest-gap table (always present; not data-dependent) |

**Archive:** default `include_archived=false` (K1a continuity). Archived paths never
enter the body unless the caller opts in.

**Empty ledger negative control:** §1 prints `**无在途**` with count 0 — never
crashes, never leaves a blank section body.

### How “替不掉什么” is expressed in the output

Section **§5** is a permanent table of structural limits (product intent, plan
narrative, runbook, pitfalls, git identity, ranked next steps, …). Separately,
empty §1–§3 are framed as **data holes** (controller has not called
`task dispatch` / `verdict record`), not as “nothing happened”. The preamble
warns that a handoff claiming completeness while omitting gaps is more
dangerous than one that lists unknowns — K0c’s `unknown` ratio honesty is kept
in a banner *and* under §4.1 statistics.

## 3. Renderability matrix summary

Against `docs/handoff/2026-08-08-pillow-controller-handoff.md` (**not modified**):

| Class | Count |
| --- | ---: |
| **已可渲染** | **2** |
| **可渲染但数据缺失** | **5** |
| **不可渲染** | **8** |

Full table: `docs/research/2026-08-09-handoff-renderability-matrix.md`.

Bottom line: the composite **can replace bookkeeping** (in-flight, pending
acceptance, verdict shells, document inventory) once the controller records
dispatch/acceptance. It **must not** replace product intent, cookbooks, or
experiential pitfall logs — and it says so in-band.

## 4. Six required tests

| # | Requirement | Where |
| --- | ---: | --- |
| 1 | Five body sections present | `k1b_composite_has_five_sections` + integration `render_from_scan_is_pure_and_covers_buckets` |
| 2 | Completed-without-verdict listed itemwise | `k1b_completed_without_verdict_listed_itemwise` (section-bounded; 逐条 list) |
| 3 | `unknown` ratio in output | `k1b_unknown_ratio_in_output` (+ banner + §4.1) |
| 4 | Archived excluded by default | `k1b_archived_excluded_from_body_by_default` (+ K1a integration) |
| 5 | Privacy: no lease / session / host path | `k1b_privacy_no_lease_session_or_host_path` (`assert_render_privacy_clean` + assembled needles) |
| 6 | Empty ledger → **无在途**, no crash/blank | `k1b_empty_ledger_says_no_inflight_not_blank` |

Privacy needles are **assembled at runtime** (no contiguous secret samples in
source) so `check-secrets.mjs` does not self-fire.

## 5. Commands / exit codes (package-local)

| Command | Exit |
| --- | ---: |
| `cargo test -p keeper-knowledge-indexer --all-targets` | **0** (lib 31 + indexer_rules 20 + ledger_cli 8) |
| `cargo clippy -p keeper-knowledge-indexer --all-targets -- -D warnings` | **0** |
| `cargo fmt -p keeper-knowledge-indexer -- --check` | **0** |
| `node scripts/check-secrets.mjs` | **0** (via gate prefix) |
| `node scripts/run-worker-gate.mjs` | **1** — see §8 |

## 6. Untouched (whitelist discipline)

- **Not modified:** `crates/storage-sqlite/**`, `crates/runtime-host/**`,
  `crates/adapter-grok-acp/**`, `crates/governance-port/**`, `apps/desktop/**`,
  `crates/kernel-project/**`, `crates/git-executor/**`, `scripts/**`,
  `tests/fixtures/**`
- **`docs/handoff/**` — not modified** (controller-owned)
- **No** automatic document rewrite
- **No** git add/commit/stash/reset/clean/checkout/restore
- **No** desktop E2E
- **No** auth/token reads
- **No** lease id / session id / machine path in render output

Only writes: `crates/knowledge-indexer/**`, this report, the matrix.

## 7. Residual risks

1. **Controller must call the ledger CLI** (or equivalent) at dispatch/acceptance —
   otherwise the composite honestly shows 无在途 / empty verdicts while real
   work proceeds off-book (same ADR-0004 failure mode at the registration layer).
2. **§5 cannot-replace list is fixed prose** in the renderer — it tracks the
   matrix classes but is not auto-derived from the handwritten file (which we
   must not rewrite). Drift between matrix and §5 is a human maintain cost.
3. **Deep verdict narrative** still lives only in report file bodies; the
   registry holds summary/residuals shells.
4. Full workspace gate may flap on concurrent workers outside this whitelist
   (see §8).

## 8. Full worker gate

```text
export CARGO_TARGET_DIR=<repo>/target-k1b
node scripts/run-worker-gate.mjs
# FAILED at step "cargo-fmt" (exit 1)
# Diff only in crates/adapter-grok-acp/** (prompt.rs, tests)
```

Those paths are **outside this ticket’s write whitelist** (parallel Codex/worker
on adapter). K1b must not rustfmt or edit them. Own crate fmt/clippy/tests are
green under `target-k1b`.

**Classification:** `BLOCKED-BY-CONCURRENT`  
**Unblock:** adapter-grok-acp worker runs `cargo fmt` on its tree (or lands),
then controller re-runs `node scripts/run-worker-gate.mjs`.

## 9. Completion

**K1B-HANDOFF-RENDER-BLOCKED: workspace cargo-fmt fails on parallel-worker unformatted `crates/adapter-grok-acp/**` (BLOCKED-BY-CONCURRENT)**

Implementation and package-local gates are complete; full worker gate waits on
the other worker’s fmt.
