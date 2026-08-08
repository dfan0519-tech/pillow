# K0b — Knowledge indexer (scan + audit, report-only)

Date: 2026-08-08  
Worker: grok-k0b  
Mode: **register + audit only** — no retirement, archive, delete, or `docs/` mutation by the scanner  
Completion tag: **`K0B-INDEXER-COMPLETE`**

Related:

- ADR-0004: `docs/decisions/ADR-0004-knowledge-layer-actors-and-registration.md`
- Inventory audit: `docs/research/2026-08-08-docs-inventory-audit.md`
- This run’s audit output: `docs/research/2026-08-08-knowledge-audit-run.md`
- K0a storage: `crates/storage-sqlite` (`knowledge_documents` / `knowledge_document_links`)

---

## 1. Crate structure and rationale

New crate: **`crates/knowledge-indexer`** (`keeper-knowledge-indexer`).

```
crates/knowledge-indexer/
  Cargo.toml
  src/
    lib.rs              # public surface
    classify.rs         # pure: role + provisional status
    supersession.rs     # pure: explicit vs inferred extraction
    digest.rs           # pure: sha256:<hex>
    audit.rs            # pure-ish: report assembly + retirement buckets
    scan.rs             # walk → upsert → links → audit
    bin/knowledge_indexer.rs
  tests/indexer_rules.rs
```

**Why a crate (not `scripts/` Node):** these rules must be reusable by the product harness on user projects. A workspace Rust crate shares K0a types, path SSoT, and the SQLite repository without a second language boundary.

**Workspace change (only allowed extra write):** root `Cargo.toml` members += `"crates/knowledge-indexer"`.

**Dependencies:** `keeper-project` (path SSoT), `keeper-storage-sqlite` (register/update/links), `sha2`, `chrono`, `thiserror`, `uuid`.

---

## 2. Classification rules

### Role (directory first, then filename)

| Zone | Rule |
| --- | --- |
| `docs/decisions/` | `decision` |
| `docs/handoff/` | `handoff` (unless brief-shaped content → `other` + conflict flag) |
| `docs/research/` | `research` |
| `docs/verification/` | `verification` |
| `docs/roadmap/` / `migration/` / `references/` | matching role |
| `docs/superpowers/specs/` | `spec` |
| `docs/superpowers/plans/` | **`plan`** (K0a enum; inventory folded these into `spec`) |
| `docs/execution/` | filename: `*brief*` / `*report*`\|`*verdict*` / `*plan*` / `*research*`\|`*audit*` → role; else `other` |

Path/content **conflict** → `other` + `path_content_conflict=true` (reported; not re-labeled to a guessed role).

### Status — **when unsure → `unknown` (never invent `active`)**

| Condition | Status |
| --- | --- |
| Explicit self-claim “已被取代” | `superseded` |
| Peer first-person “本文件取代 this path” (second pass) | target → `superseded` if still `unknown` |
| ADR + `Status: Accepted` | `accepted` |
| Explicit `Status: Proposed` / not authorized | `proposed` |
| Brief with same-stem report/verdict | `consumed` |
| Everything else | **`unknown`** |

No date-based status. No “looks current” → `active`. Worker `DONE` markers are ignored for lifecycle.

**How unknown is guaranteed:** `classify_status_from_content` only returns non-unknown for the explicit cases above; default arm always notes `no explicit lifecycle → unknown`. `RegisterDocument.status` defaults through K0a to unknown when `None`. Tests assert report/handoff/research stay `unknown` and never `active`.

---

## 3. Supersession: 查证 vs 推断

| Kind | Detection | DB? |
| --- | --- | --- |
| **查证** first-person | Line is a self-declaration (`> 本文件取代…`, `This report supersedes…`), not a markdown table quoting others | Yes — `rel=supersedes` |
| **查证** pathless | `supersedes prior delivery status` | Yes — free-text `to_path` |
| **推断** | Soft “current state moved to …” (non-table) | **No** — audit §8 only |

1→N: declaration line + up to two wrap lines; multiple backtick/`docs/*.md` targets → multiple link rows (handoff 08-08 → two files).

Targets resolve to `to_document_id` when already registered; else `to_path` only. Invalid portable paths are skipped (never sixth path validator — uses `keeper_project::validate_portable_path`).

---

## 4. Idempotency and non-overwrite

| Behavior | Mechanism |
| --- | --- |
| No duplicate rows | Upsert on `(project_id, path)` via `get_by_path` → `register` or `update` |
| Digest change | Always update `content_digest`; list in audit §7 |
| Status preserve | If stored status ≠ `unknown`, scanner **does not** pass `status` in `UpdateDocument` |
| Link idempotency | Skip `add_link` when same supersedes endpoint already exists |
| Rescan count | Second scan: `registered_new=0`, same document/link counts |

---

## 5. Real `docs/` run (this repository)

```text
knowledge-indexer --repo <repo> --db %TEMP%\pillow-k0b-scan.db \
  --report docs/research/2026-08-08-knowledge-audit-run.md
```

- **DB:** temporary only (`%TEMP%\pillow-k0b-scan.db`) — not project durable store.
- **Read:** `docs/**/*.md` (excluding this audit-run + K0b report so prior audit text cannot self-cite every basename).
- **Write under docs/:** only the two whitelist deliverables (audit-run + this report).

### Results

| Metric | Value |
| --- | --- |
| Files registered | **296** |
| Links added (explicit supersedes) | **4** (1+2 path edges + 1 pathless) |
| `unknown` | **214 / 296 = 72.3%** (honest, expected high) |
| `consumed` | 72 |
| `accepted` | 4 |
| `proposed` | 3 |
| `superseded` | 3 |
| Roles | brief 115, report 102, verification 26, plan 15, research 17, spec 5, decision 4, handoff 4, other 4, roadmap 2, migration 1, reference 1 |
| Unpaired briefs | **43** |
| Zero-inbound | **102** (101 brief + 1 report) |
| Retirement candidates | **3** explicit-superseded handoffs only |
| `other` | same 4 as inventory |

Full itemized findings: `docs/research/2026-08-08-knowledge-audit-run.md`.

### Inventory number reconciliation

See audit §10. Headline:

- **+2 files** since inventory (294 → 296).
- **`plan` vs `spec`:** 15 plans are `plan` here; inventory counted them as `spec` (total still 20).
- **Orphans 132 → 102:** same basename method; inventory + knowledge-layer research now cite many former orphans; brief zeros still **101**.
- **Superseded 3, unpaired 43, other 4:** match.

---

## 6. Eight tests (coverage map)

| # | Requirement | Test |
| --- | --- | --- |
| 1 | Classification + path/content conflict → `other` | `classify_directory_and_filename_rules`, `path_content_conflict_yields_other` |
| 2 | Unsure → `unknown` (never active) | `unsure_status_is_unknown_never_active` + unit |
| 3 | Explicit supersession; inferred not DB | `explicit_one_to_many_and_inferred_not_explicit`, `scan_registers_one_to_many_links_not_inferred` |
| 4 | 1→N supersession | same + unit `extracts_one_to_two_chinese_handoff` |
| 5 | Idempotent double scan | `double_scan_is_idempotent` |
| 6 | Rescan preserves non-unknown status | `rescan_preserves_non_unknown_status` |
| 7 | verification/decision orphans ∉ retirement | `verification_decision_orphans_not_retirement_candidates` + audit unit |
| 8 | Path SSoT; `COM¹` rejected | `path_validation_uses_ssot_rejects_com_superscript` |

Also: `paired_brief_becomes_consumed`, ADR accepted / proposed status.

```text
cargo test -p keeper-knowledge-indexer
# 7 lib unit + 11 integration = 18 passed, exit 0
```

---

## 7. Commands and exit codes

| Command | Exit |
| --- | --- |
| `cargo test -p keeper-knowledge-indexer` | **0** |
| `cargo clippy -p keeper-knowledge-indexer --all-targets -- -D warnings` | **0** |
| `cargo fmt -p keeper-knowledge-indexer` | **0** |
| Full docs scan → temp DB + audit report | **0** |
| `node scripts/run-worker-gate.mjs` | **0** (`PILLOW_WORKER_GATE: OK`) |

---

## 8. What this worker did **not** touch

- `crates/storage-sqlite/**` (K0a frozen)
- `crates/kernel-project/**` (path SSoT frozen)
- `crates/git-executor/**`, `kernel-resource/**`, `runtime-host/**`
- `apps/desktop/**`
- `scripts/**`, `tests/fixtures/**`
- Any `docs/` file except the two whitelist outputs
- No git add/commit/stash/reset/clean/checkout
- No desktop E2E
- No auth/token files
- **No delete / archive / move of any document**

---

## 9. Residual risks

1. **Basename inbound** can false-positive on short stems (none observed severe; same method as inventory).
2. **Self-declaration heuristics** may miss unusual phrasing; soft/English path forms beyond the tested corpus need expansion when product ships.
3. **Audit report under `docs/`** must stay excluded (or written after scan) or it self-cites basenames and collapses orphan counts — CLI excludes the two K0b deliverables by default.
4. **Mis-filed dual-role docs** keep a single primary role; secondary roles deferred (inventory §3.3).
5. Scanner still does **not** set `active` for the current handoff — controller/human must promote intentionally (by design).

---

## 10. Gate

```text
node scripts/run-worker-gate.mjs
# PILLOW_WORKER_GATE: OK  (exit 0)
# Includes: workspace structure, secrets, cargo fmt --check, typecheck,
# pnpm tests, cargo test workspace, clippy -D warnings, ACP drift check.
```

Desktop E2E not run (controller-owned).

---

**K0B-INDEXER-COMPLETE**
