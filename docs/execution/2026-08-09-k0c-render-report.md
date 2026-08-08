# K0c — Handoff render from registry (report)

Date: 2026-08-09  
Worker: grok1  
Status: **K0C-RENDER-COMPLETE**

## 1. One-line

The knowledge status handoff is now a **pure projection** of K0a registry rows +
K0b audit buckets (`render_status_view`), not a hand-maintained narrative.

## 2. Implementation

| Piece | Path | Role |
| --- | --- | --- |
| Pure renderer | `crates/knowledge-indexer/src/render.rs` | `RenderInput` → Markdown |
| Export | `crates/knowledge-indexer/src/lib.rs` | `render_status_view`, `RenderInput` |
| Scan wiring | `crates/knowledge-indexer/src/scan.rs` | `ScanResult.handoff_markdown` |
| CLI | `crates/knowledge-indexer/src/bin/knowledge_indexer.rs` | `--handoff PATH` |
| Unit tests | `render.rs` `unit_tests` | 7 pure-function / policy tests |
| Integration | `tests/indexer_rules.rs` | scan→render purity + empty registry |

### Sections rendered (all from registry / audit)

1. **Statistics** — total, role/status tables, **`unknown` ratio prominent** (UNCLASSIFIED / 未分类)
2. **Current effective documents** — live = status ∉ {superseded, archived}; known status by role; **`unknown` separate §2.2 未分类**
3. **Supersession chains** — `supersedes` links only; 1→1 / 1→N arity; free-path claims called out
4. **Contradictions** — audit multi-current handoff/roadmap/migration
5. **Pending review (待复核)** — unpaired briefs only
6. **Expected orphans (预期孤儿, 非问题)** — verification/decision bucket; **not mixed into §5 todos**

### Proof: no hand-written project narrative

- Renderer never reads file bodies for prose (only registry docs + links + `AuditReport`).
- Forbidden patterns from the hand-written handoff (`在途 Worker`, `C4 已 checkpoint`, `品牌/视觉升级`, `storage barrier`) are unit-tested as absent.
- Real sample contains only paths, roles, statuses, link edges, and audit buckets.

## 3. Live run (this repo)

```text
CARGO_TARGET_DIR=<repo>/target-grok1
cargo run -p keeper-knowledge-indexer --release -- \
  --repo <repo> \
  --db <repo>/target-grok1/k0c-pillow.db \
  --handoff <repo>/docs/research/2026-08-08-handoff-rendered-sample.md
```

| Metric | Value |
| --- | --- |
| Exit code | **0** |
| files_seen | 296 |
| registered_new | 296 |
| links_added | 4 |
| unknown_ratio | **72.3%** (214/296) |
| Output | `docs/research/2026-08-08-handoff-rendered-sample.md` |

Matches K0b audit ballpark (296 docs, ~72% unknown, 43 unpaired briefs, 1→N handoff supersession, free-path `prior delivery status`, roadmap contradiction).

## 4. Coverage vs hand-written handoff (§6)

Reference: `docs/handoff/2026-08-08-pillow-controller-handoff.md` (**not modified**).

### Covered (registry can replace)

| Handwritten section / concern | Render equivalent |
| --- | --- |
| “Which docs exist / what role” inventory | §1 role + §2 path lists |
| “This file supersedes X/Y” chains | §3 supersession (incl. 1→N + free-path) |
| Dual “current” roadmap/handoff confusion | §4 contradictions |
| Unpaired briefs needing attention | §5 待复核 |
| Evidence ADRs / acceptance as non-garbage | §6 预期孤儿 (when bucket non-empty) |
| Honest uncertainty about classification | §1 `unknown` **72.3%** + §2.2 未分类 |
| Superseded handoff history | §2.3 + §3 |

### Not covered (honest gaps — most important)

| Handwritten content | Why render cannot cover it |
| --- | --- |
| §1 一句话状态 (“C4 checkpointed”, “在途 Worker:0”, next step) | No **run/worker/slot** records in knowledge registry |
| §2 产品终点 / Stage lines / 红线 | Product intent is not a document-role fact; no `decision` row for verbal user confirmation |
| §3 四步计划进度 table | No task-progress / plan-status object in registry |
| §4 “已完成关键工作” narrative + controller judgment | Verdicts, flake diagnoses, barrier experiments live as prose inside reports — registry stores path+role+status only |
| Git HEAD / branch / remote | Outside knowledge tables |
| Worker dispatch state, lease, acceptance residual | K0b registers *documents*, not controller run ledger |
| “What to read next for the next controller” ranked shortlist | Would need explicit `active` marks + ranking policy beyond path inventory |
| Open decisions / Q1–Q9 from design specs | Specs are `unknown`/`proposed` paths; open questions are body prose |
| Risk list / “do not do X” operational rules | Not registered as structured facts |

**Bottom line:** the render proves ADR-0004 §3 for **document inventory + lifecycle + supersession + audit buckets**. It does **not** yet replace the handoff as a **controller situational brief**. That is expected: the failure mode of the handwritten handoff was *stale bookkeeping of document state under load*; the remaining gap is *controller verdicts and live ops*, which K0b never registered.

### What K0d (or next) must register to cover more

1. **Dispatch / acceptance ledger** — task id, brief path, slot, lease, accepted|rejected, residual one-liner, finished_at (byproduct of dispatch, per ADR-0004 §2).
2. **Controller verdict rows** — not re-prose of the report; structured accept/reject + residual pointer.
3. **Active pointer(s)** — which handoff/roadmap/spec is `active` (explicit set, never inferred to look “known” when unknown).
4. **Plan step status** — optional tiny table for the four-step roadmap rows.
5. **Open question objects** — or “implements decision” links from specs to ADR/decision rows after human/controller confirmation.
6. **Git snapshot optional** — HEAD/branch at render time *if* attached as scan metadata (still not free-form narrative).

Until (1)+(2) exist, any “在途 Worker:0” line would be **fabricated** if put in this renderer — correctly refused.

## 5. Tests (six requirements)

| # | Requirement | Where |
| --- | ---: | --- |
| 1 | Pure function: same registry → same output | `render::unit_tests::pure_function_stable`, `render_from_scan_is_pure_and_covers_buckets` |
| 2 | `unknown` ratio correct + prominent | `unknown_ratio_prominent_and_correct` (72/100 → 72.0% + 未分类) |
| 3 | 1→N supersession + free-path | `one_to_many_supersession_rendered`, integration scan |
| 4 | Empty registry honest “no data” | `empty_registry_is_honest`, `render_empty_registry_honest` |
| 5 | Contradictions + 待复核 sections | `contradictions_and_pending_review_sections` |
| 6 | Expected orphans = 非问题, not todos | `expected_orphans_labeled_non_problem_not_in_pending` |

Also: `no_handwritten_project_narrative_inlined`.

```text
cargo test -p keeper-knowledge-indexer
# lib unit: 14 passed
# integration indexer_rules: 13 passed
# exit: 0 (compiler success; PowerShell may surface cargo stderr as nse)
```

## 6. Commands / exit codes

| Command | Exit |
| --- | ---: |
| `cargo test -p keeper-knowledge-indexer` | **0** (all 27 tests) |
| `cargo run -p keeper-knowledge-indexer --release -- … --handoff …` | **0** |
| `node scripts/run-worker-gate.mjs` | **0** (`PILLOW_WORKER_GATE: OK`) |

`CARGO_TARGET_DIR=<repo>/target-grok1` used throughout.

## 7. Untouched (whitelist discipline)

- **Not modified:** `crates/storage-sqlite/**`, `crates/kernel-project/**`, `apps/desktop/**`, `scripts/**`, root `Cargo.toml`
- **Not modified:** existing handoff `docs/handoff/2026-08-08-pillow-controller-handoff.md` or any other pre-existing `docs/**` file
- **Only docs writes:**  
  - `docs/research/2026-08-08-handoff-rendered-sample.md`  
  - `docs/execution/2026-08-09-k0c-render-report.md`
- No git add/commit/stash/reset/clean/checkout
- No desktop E2E
- No auth/token reads
- No retirement/delete/archive of documents

## 8. Residual risks

1. **Expected orphans count can be 0** when inventory/research tables cite every verification basename (same as K0b). Policy still correct; bucket emptiness is corpus-coupling, not a renderer bug.
2. **“Current effective” still lists all non-superseded paths** — without `active` marks, §2.1 is “classified status inventory”, not “what the controller believes is true”.
3. **Free-path claims** are strings only; no resolution to a document until a path-shaped target is registered.
4. **CLI default stdout** now prints K0c view when neither `--report` nor `--handoff` is set (K0b printed audit). Callers that relied on default audit stdout should pass `--report -` or a path; intentional for K0c deliverable.
5. Parallel workers must keep distinct `CARGO_TARGET_DIR` (used `target-grok1` here).

## 9. Completion

**K0C-RENDER-COMPLETE**
