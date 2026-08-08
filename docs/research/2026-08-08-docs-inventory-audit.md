# Docs inventory audit — knowledge registry role/status fit

Date: 2026-08-08  
Mode: **read-only** (no source/tests/builds; this file is the only write)  
Controller: Claude  
Worker: docs-inventory-audit  
Corpus: `docs/**/*.md` as of this scan  

Related:

- `docs/decisions/ADR-0004-knowledge-layer-actors-and-registration.md`
- `docs/research/2026-08-08-knowledge-layer-research.md` (134 orphan claim)
- `docs/handoff/2026-08-08-pillow-controller-handoff.md` (stale handoff living sample)

---

## 0. Verdict for the controller

| Question | Answer |
| --- | --- |
| Does the proposed `role` enum cover the tree? | **Mostly.** With directory + filename heuristics, **`other` = 4 / 294 = 1.4%** (well under 15%). The real failure mode is not `other` inflation; it is **dual-role documents** and **status ambiguity**. |
| Is `status` ∈ {active, superseded, archived} enough? | **No.** Most docs are neither “active truth” nor “superseded.” Briefs are **consumed**, reports are **historical evidence**, designs may be **proposed vs accepted**, handoffs **rot by paragraph**. Three values cannot express that. |
| Is single-value `supersedes` / `superseded_by` enough? | **No.** At least one **1→N** supersession exists today (current handoff replaces **two** docs). Multi-merge and “report supersedes prior delivery status” (no target path) also appear. |
| Classification failed by `other` > 15%? | **No — `other` does not fail the 15% test.** Classification is **still not enough** for retirement/search without extra fields (see §7). |
| 134 orphans? | Independent recount: **132 zero-inbound** on **294** files (44.9%). Prior research measured **134 / 287** (46.7%). Same method family; delta explained in §4. |

**One-line:** Role enum is “good enough to label”; lifecycle model is **not** good enough to retire or rank. Do not ship migration that only has `active|superseded|archived` + single `supersedes`.

---

## 1. Method (independent of prior research)

| Item | Choice |
| --- | --- |
| Corpus | All `docs/**/*.md` (no `.json`) |
| Count | **294** markdown files |
| Inbound link | Other docs’ raw text contains target **basename without `.md`** → +1 document-level mention. Self excluded. |
| Not used | git history, runtime logs, wiki `[[links]]`, commits |
| Role assignment | Directory convention first, then filename (`*brief*`, `*report*`, `*verdict*`), then content head signals for edge cases |
| Status assignment | Only explicit supersession prose is treated as **查证** for `superseded`. Everything else is **推断** or **unknown** |
| Forbidden | cargo / pnpm / gate / tests / E2E / any non-report write |

Limits (same class as prior research):

- Basename match can false-positive on short stems (none observed as severe; all basenames unique).
- Misses references that only use wave IDs (`C3-E`) without filename.
- ADR short IDs (`ADR-000N`) counted separately in §4.3.

---

## 2. Corpus shape (today)

| Directory | md count | Default role (path) |
| --- | ---: | --- |
| `docs/execution/` | **224** | brief / report / edge |
| `docs/verification/` | **26** | verification |
| `docs/superpowers/` | **18** | plans + specs → mapped to `spec` with dual notes |
| `docs/research/` | **14** | research (+ this audit will make 15 after write) |
| `docs/handoff/` | **4** | handoff |
| `docs/decisions/` | **4** | decision |
| `docs/roadmap/` | **2** | roadmap |
| `docs/migration/` | **1** | migration |
| `docs/references/` | **1** md | reference |
| **Total** | **294** | |

Prior knowledge-layer research reported **287** md. Growth since that scan: **+7** files (new ADRs/research/execution/settings design and more 08-08 execution reports). Hand-off text still says “261+” — itself a staleness exhibit.

Date prefix `YYYY-MM-DD-`: **287 / 294**. Undated (7):

- four ADRs under `decisions/`
- `migration/legacy-capability-inventory.md`
- `references/upstream-baseline.md`
- `verification/worker-gate-entry.md`

Front-matter YAML: still effectively **absent** as a registry (no machine schema). Free-form `Status:` lines appear in **89 / 294** files but values are worker signals (`DONE`, `ACTIVE`, `BLOCKED`, `DONE_WITH_CONCERNS`, …), **not** the proposed lifecycle enum.

---

## 3. Full classification (role + status)

### 3.1 Role distribution (primary label)

| role | count | % | How assigned |
| --- | ---: | ---: | --- |
| `brief` | 115 | 39.1% | `execution/*brief*` |
| `report` | 101 | 34.4% | `execution/*report*` + `*verdict*` (2) |
| `verification` | 26 | 8.8% | `verification/**` |
| `spec` | 20 | 6.8% | `superpowers/specs/*` (5) + `superpowers/plans/*` (13) + 2 early plans in `execution/` |
| `research` | 16 | 5.4% | `research/**` (14) + 2 mis-filed under `execution/` (see §3.3) |
| `decision` | 4 | 1.4% | `decisions/ADR-*` |
| `handoff` | 4 | 1.4% | `handoff/**` |
| `roadmap` | 2 | 0.7% | `roadmap/**` |
| `migration` | 1 | 0.3% | `migration/**` |
| `reference` | 1 | 0.3% | `references/*.md` |
| **`other`** | **4** | **1.4%** | does not fit enum without stretching |
| **Total** | **294** | 100% | |

### 3.2 `other` detail (1.4% — does **not** trip 15% fail)

| path | Why not a clean role | Closest forced labels |
| --- | --- | --- |
| `execution/2026-08-04-wave-c4-d3b1-exact-grok4-routing-approval.md` | Slot routing / approval note, not ADR, not brief/report | decision? / other |
| `execution/2026-08-07-wave-c4-d3b3-cancel-tree-diagnosis.md` | Diagnosis write-up | research? / report? |
| `execution/2026-08-07-wave-c4-d3b3-cancel-tree-rootcause.md` | Root-cause note | research? / report? |
| `execution/2026-08-08-adapter-decoupling-proposal.md` | Architecture **proposal** (long, design-like) | spec? / research? |

If the controller **refuses** to map `superpowers/plans/*` to `spec`, add **13** plans (+2 execution plans if also refused) into a gray zone. Even then:

- plans-as-other ≈ 4 + 13 = **17 → 5.8%**
- plans + execution plans ≈ **19 → 6.5%**

Still under 15%. **The 15% `other` gate would pass; that gate is the wrong failure detector.**

### 3.3 Dual-role / “fits two labels” cases (the real fit problem)

These are **查证** path+content conflicts, not content-keyword noise:

| Document | Roles it simultaneously resembles | Why it matters |
| --- | --- | --- |
| `superpowers/plans/*` (13) | `spec` (implementation plan) **and** orchestration / sometimes acceptance | Enum has `spec` but no `plan`. Audit rules for “stale design” vs “historical task list” differ. |
| `superpowers/plans/2026-08-01-c3-final-acceptance-checkpoint.md` | plan + `verification` + mini-handoff | Title is checkpoint; lives under plans. |
| `superpowers/plans/…-real-desktop-process-acceptance.md` | plan + `verification` | Acceptance orchestration, not pure design. |
| `execution/2026-08-08-flake-analysis-VERDICT.md` | `report` + deep analysis + quasi-`verification` | Controller cites as methodology authority; not a wave delivery report. |
| `execution/2026-08-08-storage-barrier-verdict.md` | same | Handoff calls it reusable experimental method. |
| `execution/2026-08-08-ui-premium-research.md` | `research` but path is `execution/` | Path-based role registration will **mis-label** if directory is sole source of truth. |
| `execution/2026-08-08-upstream-grokbuild-audit.md` | audit / `research` / `report` hybrid | Same path mismatch. |
| `handoff/2026-08-08-round5-tech-debt-status.md` | `handoff` snapshot **and** execution status report | Explicitly superseded by full handoff, but is not a full handoff itself. |
| `research/2026-08-08-knowledge-layer-research.md` | `research` that **recommends decisions** (red lines, K0) | Not an ADR; decisions still live only as prose recommendations. |
| `research/2026-08-02-c4-security-acceptance-research.md` | research + acceptance criteria tone | Dual research/verification. |
| `roadmap/2026-07-30-stage1-plan-comparison.md` | roadmap + historical decision table | Status line points “current state moved to” another roadmap file — soft supersession. |
| `superpowers/specs/2026-08-02-c4-governed-process-git-design.md` | `spec` with Status **Proposed** while later waves implemented C4 | “Proposed” vs “Accepted” is not `active`/`superseded`. Spec may still be the design of record even after implementation. |
| `superpowers/specs/2026-08-08-settings-credentials-design.md` | `spec` **Proposed**, open Q1–Q9 | Active for decision-making, not “active product truth.” |
| ADR-0001…0004 | `decision` with Status **Accepted** | Accepted ≠ “load into every context”; Accepted ≠ cited. |

**Inference:** single `role` column is a primary label only. Without `secondary_roles[]` or a separate `artifact_kind`, auditors will fight about plans, verdicts, and mis-filed research.

### 3.4 Status distribution (honest)

Proposed: `active` | `superseded` | `archived`.

| status (this audit) | count | Basis |
| --- | ---: | --- |
| `superseded` (**查证**, explicit prose) | **3** | handoff chain + round5 (see §4) |
| `superseded` (**推断**, soft pointer) | **1** | roadmap plan-comparison → overall-status |
| `archived` | **0** | no `docs/archive/`, no archive markers |
| `active` as “current operational truth” | **very few** | effectively latest handoff + binding gate doc + Accepted ADRs + current roadmap status — **not** mechanically labeled |
| **unknown / not expressible** | **~290** | almost entire tree |

Worker completion markers (`DONE`, `COMPLETE`, `BLOCKED`) are **task outcome**, not document lifecycle. Mapping them to `active` is wrong; mapping them to `superseded` is also wrong.

**Briefs specially:** stem pairing in `execution/`:

| Pairing | count |
| --- | ---: |
| brief stem has matching `*-report` / verdict stem | **72** |
| brief with no same-stem report | **43** |
| report/verdict with no same-stem brief | **28** |

Paired briefs are best described as **`consumed`** (contract fulfilled) — **not in the proposed enum**. Calling them `active` pollutes search; calling them `superseded` invents a supersession edge that nobody wrote; calling them `archived` without policy is premature.

### 3.5 Status un-decidable samples (high value)

| File | Why status cannot be decided from fields alone |
| --- | --- |
| `handoff/2026-08-08-pillow-controller-handoff.md` | Claims to be current truth; known to rot by paragraph within a session (ADR-0004 context). No `last_verified`. |
| `handoff/2026-08-08-round5-tech-debt-status.md` | Same calendar day as current handoff; **explicitly superseded** by it, but still reads as a live status doc if opened alone. |
| `execution/2026-08-08-dual-theme-report.md` vs `…-dual-skins-report.md` | Same theme, different scopes (blocked persistence vs CSS-only complete). Neither supersedes the other; both “true.” |
| `execution/2026-08-08-flake-analysis-report.md` vs `…-VERDICT.md` vs later harness/storage fixes | Stacked analysis → verdict → fixes. No supersession edges; partial overwrite by later reports (**推断**). |
| `superpowers/specs/2026-08-02-c4-governed-process-git-design.md` | Header still “Proposed… not authorized” while C4 shipped and checkpointed. |
| Any `verification/*acceptance*` | Historically true acceptance record; not “active product state,” not “superseded.” Needs **immutable evidence** semantics. |
| 43 unpaired briefs | Open work? renamed report? absorbed into multi-brief report? **Cannot tell** without registry or git/task linkage. |

### 3.6 Inventory by role (full lists except bulk brief/report)

#### `decision` (4) — all Status: Accepted in body

| path | inbound (basename) | status note |
| --- | ---: | --- |
| `decisions/ADR-0001-clean-successor.md` | 1 | active decision; poorly linked |
| `decisions/ADR-0002-pillow-identity-boundary.md` | 1 | same |
| `decisions/ADR-0003-settings-authority-and-s0-scope.md` | 2 | better linked (ID mentions higher, §4.3) |
| `decisions/ADR-0004-knowledge-layer-actors-and-registration.md` | 0 | new; zero basename inbound |

#### `handoff` (4)

| path | inbound | status |
| --- | ---: | --- |
| `handoff/2026-08-01-pillow-controller-handoff.md` | 4 | **superseded** (explicit by 08-07) |
| `handoff/2026-08-07-pillow-controller-handoff.md` | 2 | **superseded** (explicit by 08-08) |
| `handoff/2026-08-08-round5-tech-debt-status.md` | 3 | **superseded** (explicit by 08-08 handoff; 1→N) |
| `handoff/2026-08-08-pillow-controller-handoff.md` | 3 | **active intent**; freshness not guaranteed |

#### `roadmap` (2)

| path | inbound | status |
| --- | ---: | --- |
| `roadmap/2026-07-30-stage1-plan-comparison.md` | 3 | **推断 superseded** (“current state moved to” overall-status) |
| `roadmap/2026-07-31-stage1-overall-status.md` | 13 | most-linked roadmap; still may lag HEAD |

#### `migration` / `reference` (2)

| path | inbound |
| --- | ---: |
| `migration/legacy-capability-inventory.md` | 5 |
| `references/upstream-baseline.md` | 13 |

#### `research` (16) — path under `research/` unless noted

| path | inbound | notes |
| --- | ---: | --- |
| `research/2026-07-30-grok-acp-protocol.md` | 1 | early protocol |
| `research/2026-07-30-grok-adapter-boundary.md` | 1 | early boundary |
| `research/2026-07-30-grok-security-acceptance.md` | 2 | |
| `research/2026-07-30-zbbody-reference-analysis.md` | 2 | |
| `research/2026-08-01-c3-governed-write-security-research.md` | 5 | |
| `research/2026-08-01-c3-grok-app-interaction-research.md` | 5 | |
| `research/2026-08-01-c3-zbbody-interaction-research.md` | 3 | |
| `research/2026-08-02-c4-git-governance-research.md` | 4 | |
| `research/2026-08-02-c4-security-acceptance-research.md` | 4 | dual research/acceptance tone |
| `research/2026-08-02-c4-terminal-git-interaction-research.md` | 4 | |
| `research/2026-08-02-c4-terminal-process-research.md` | 3 | |
| `research/2026-08-04-c4-d3b-grok4-process-ui-integration-map.md` | 1 | |
| `research/2026-08-08-desktop-delivery-research.md` | 1 | |
| `research/2026-08-08-knowledge-layer-research.md` | 1 | source of “134” |
| `execution/2026-08-08-ui-premium-research.md` | 0 | **mis-filed** research |
| `execution/2026-08-08-upstream-grokbuild-audit.md` | 3 | audit / research hybrid |

#### `verification` (26)

| inbound | path |
| ---: | --- |
| 1 | `verification/2026-07-29-slice-a-acceptance.md` |
| 4 | `verification/2026-07-30-slice-b1-acceptance.md` |
| 1 | `verification/2026-07-30-slice-b2-acceptance.md` |
| **0** | `verification/2026-07-30-wave-b2-h3-native-acceptance.md` |
| **0** | `verification/2026-07-30-wave-b3-foundation-acceptance.md` |
| 1 | `verification/2026-07-31-wave-b3-read-execution-acceptance.md` |
| 2 | `verification/2026-07-31-wave-c1-acceptance.md` |
| 3 | `verification/2026-07-31-wave-c1-checkpoint.md` |
| 1 | `verification/2026-07-31-wave-c1-visual-acceptance.md` |
| 4 | `verification/2026-08-01-wave-c2-acceptance.md` |
| 7 | `verification/2026-08-01-wave-c2-checkpoint.md` |
| **0** | `verification/2026-08-01-wave-c3c-acceptance.md` |
| **0** | `verification/2026-08-01-wave-c3d-acceptance.md` |
| **0** | `verification/2026-08-01-wave-c3e-acceptance.md` |
| 1 | `verification/2026-08-01-wave-c3f-cancel-resolution-fix-acceptance.md` |
| 1 | `verification/2026-08-01-wave-c3f-native-path-privacy-fix-acceptance.md` |
| 1 | `verification/2026-08-01-wave-c3f-provider-cancel-rpc-fix-acceptance.md` |
| 2 | `verification/2026-08-01-wave-c3f-run-events-schema-fix-acceptance.md` |
| 6 | `verification/2026-08-02-wave-c3-acceptance.md` |
| 5 | `verification/2026-08-02-wave-c3-checkpoint.md` |
| 1 | `verification/2026-08-04-wave-c4-d3a-controller-review.md` |
| 1 | `verification/2026-08-04-wave-c4-d3ar2-acceptance.md` |
| 1 | `verification/2026-08-04-wave-c4-d3b1-acceptance.md` |
| 4 | `verification/2026-08-07-wave-c4-checkpoint.md` |
| 1 | `verification/2026-08-07-wave-c4-d3b4-acceptance.md` |
| 3 | `verification/worker-gate-entry.md` |

#### `spec` (20) — plans stretched into `spec`

| path | inbound | note |
| --- | ---: | --- |
| `superpowers/specs/2026-07-29-pillow-first-design.md` | 8 | |
| `superpowers/specs/2026-08-01-c3-governed-write-design.md` | 9 | |
| `superpowers/specs/2026-08-02-c4-governed-process-git-design.md` | 8 | still “Proposed” in header |
| `superpowers/specs/2026-08-05-c4-git-changes-design.md` | 10 | top-linked design |
| `superpowers/specs/2026-08-08-settings-credentials-design.md` | 4 | Proposed; open questions |
| `superpowers/plans/2026-07-29-slice-a-desktop-foundation.md` | 1 | |
| `superpowers/plans/2026-07-30-slice-b1-conversation-kernel.md` | 1 | |
| `superpowers/plans/2026-07-31-c2-read-only-workbench-completion.md` | 5 | |
| `superpowers/plans/2026-07-31-c2e-isolation-restart-desktop-acceptance.md` | 4 | plan+acceptance |
| `superpowers/plans/2026-08-01-c3-governed-write-implementation.md` | 3 | |
| `superpowers/plans/2026-08-01-c3-final-acceptance-checkpoint.md` | **0** | plan+checkpoint |
| `superpowers/plans/2026-08-02-c4-governed-process-git-implementation.md` | 8 | |
| `superpowers/plans/2026-08-02-c4-next-round-controller-orchestration.md` | **0** | |
| `superpowers/plans/2026-08-03-c4-d2br-d3-controller-orchestration.md` | **0** | |
| `superpowers/plans/2026-08-04-c4-d3b2-workbench-mounting.md` | **0** | |
| `superpowers/plans/2026-08-05-c4-d3b3-provider-process-roundtrip.md` | 1 | |
| `superpowers/plans/2026-08-05-c4-d3b3-real-desktop-process-acceptance.md` | 1 | plan+acceptance |
| `superpowers/plans/2026-08-05-c4-git-changes-implementation.md` | 4 | |
| `execution/2026-07-30-wave-b2-implementation-plan.md` | **0** | plan in execution/ |
| `execution/2026-07-30-wave-b2-live-hardening-plan.md` | **0** | plan in execution/ |

#### `brief` (115) / `report` (101)

Too large to paste as a narrative list; machine classification is deterministic:

- **brief:** every `docs/execution/**/*brief*.md` → **115**, of which **101** have zero inbound.
- **report:** every `docs/execution/**/*report*.md` plus  
  `execution/2026-08-08-flake-analysis-VERDICT.md`,  
  `execution/2026-08-08-storage-barrier-verdict.md` → **101**, of which **16** have zero inbound.

Zero-inbound reports (should **not** auto-archive):

```
execution/2026-08-07-wave-c4-conversation-list-race-report.md
execution/2026-08-07-wave-c4-d3b4-r4-pane-mode-report.md
execution/2026-08-08-debt-a-pane-labels-report.md
execution/2026-08-08-debt-b2-lifecycle-impl-report.md
execution/2026-08-08-debt-c-fixture-hardening-report.md
execution/2026-08-08-debt-d-red-capture-report.md
execution/2026-08-08-dual-skins-report.md
execution/2026-08-08-dual-theme-report.md
execution/2026-08-08-flake-analysis-report.md
execution/2026-08-08-logo-rerender-report.md
execution/2026-08-08-portable-path-converge-report.md
execution/2026-08-08-settings-selfbrick-fix-report.md
execution/2026-08-08-step1a-contract-move-report.md
execution/2026-08-08-step1b-path-rule-report.md
execution/2026-08-08-step1d-brand-glass-report.md
execution/2026-08-08-theme-persistence-report.md
```

Many of these are same-day work already summarized in the current handoff without always naming the file — another reason **inbound ≠ importance**.

#### `other` (4) — listed in §3.2

---

## 4. Supersession

### 4.1 Explicit supersession (**查证**)

| Source (claims to replace) | Target(s) | Cardinality |
| --- | --- | --- |
| `handoff/2026-08-07-pillow-controller-handoff.md` | `handoff/2026-08-01-pillow-controller-handoff.md` | 1→1 |
| `handoff/2026-08-08-pillow-controller-handoff.md` | `handoff/2026-08-07-pillow-controller-handoff.md` **and** `handoff/2026-08-08-round5-tech-debt-status.md` | **1→2** |
| `execution/2026-08-01-wave-c3e-approval-ui-report.md` | “prior delivery status” (no path) | **non-addressable** |

Also **查证** soft pointer (not “本文件取代” wording):

| Source | Target | Text |
| --- | --- | --- |
| `roadmap/2026-07-30-stage1-plan-comparison.md` | `roadmap/2026-07-31-stage1-overall-status.md` | “current state moved to …” |

No other machine-usable “this file supersedes path X” edges were found in a tree-wide scan of supersession phrases. Mentions of the English word “supersedes” elsewhere are product-language or meta-discussion (ADR-0004, knowledge research), not edges.

**Implication for schema:** `supersedes` as a **single path** is already broken by the current handoff. Need `supersedes[]` / join table. Non-path supersession (“prior delivery status”) needs either forced path targets at write time or a different relation (`supersedes_claim` free text — weak).

### 4.2 Inferred supersession / stacks (**推断 only**)

| Pattern | Examples | Confidence |
| --- | --- | --- |
| Handoff date chain beyond explicit | 08-01 → 08-07 → 08-08 already explicit | high (covered) |
| Analysis → verdict → fix reports | flake-analysis-report → VERDICT → harness-flake-fixes / storage-barrier-verdict | medium; complementary more than replace |
| dual-theme (blocked) vs dual-skins (complete) | same day, different scope | **not** supersession |
| Older wave briefs after acceptance | 72 paired briefs | medium as **consumed**, not superseded |
| Design “Proposed” after implementation | C4 process/git design header vs C4 checkpoint | medium **staleness of header**, not file replace |
| Roadmap comparison → overall status | see soft pointer | medium-high |

**How many superseded-without-declaration?**  
If “superseded” means “no longer safe as operational truth,” then **dozens to low hundreds** of wave briefs/reports and intermediate handoffs are stale as *guidance*, but only **3** are declared. Quantifying exact inferred count requires a policy definition; under “must have explicit edge,” answer is: **~all other stale docs lack declarations**.

### 4.3 Contradictions (two claims of currency / conflicting facts)

| Pair | Conflict | Type |
| --- | --- | --- |
| `handoff/2026-08-08-round5-tech-debt-status.md` §1 vs `handoff/2026-08-08-pillow-controller-handoff.md` §4.5 | round5: `cancel-tree` root cause still unknown; handoff: high-confidence harness bug **already fixed** | **查证** content contradiction; supersession edge exists from handoff → round5 but **targets still readable as live** |
| `superpowers/specs/2026-08-02-c4-governed-process-git-design.md` header vs `verification/2026-08-07-wave-c4-checkpoint.md` | design still “Proposed / not authorized” vs C4 checkpoint accepted | header lifecycle vs reality |
| Multiple handoffs if an agent loads without registry | 08-01 / 08-07 / 08-08 all present; only prose marks chain | structural multi-current risk |

No second document was found that **explicitly** claims “I am the current controller handoff” against 08-08. The contradiction is **factual paragraphs**, not dual “current handoff” banners.

### 4.4 Cardinality summary

| Pattern | Present? |
| --- | --- |
| 1→1 supersession | yes (08-07 → 08-01) |
| **1→N supersession** | **yes** (08-08 handoff → two files) |
| N→1 merge | not explicitly declared; **推断** possible for multi-brief → one integration report |
| supersession without path | yes (C3-E report) |

**Report to controller:** single-value `supersedes` / `superseded_by` is insufficient.

---

## 5. Orphans — independent recount of “134”

### 5.1 Numbers

| Metric | Prior research | This audit |
| --- | ---: | ---: |
| Total md | 287 | **294** |
| Zero inbound (basename in other docs) | **134** (46.7%) | **132** (44.9%) |
| Inbound 1 | 85 | 90 |
| Inbound 2–3 | 38 | 39 |
| Inbound 4–9 | 28 | 30 |
| Inbound 10+ | 2 | 3 |

Path-aware matching (also count full relative path / `docs/...`) still yields **132** zeros — no change.

**Why not 134?**

1. Corpus grew by **7** files; some new files **reference** former orphans (e.g. knowledge-layer research itself cites ADR-0001/0002, early research, slice-a plan — those drop off the zero list if they only lacked refs before).
2. New zero-inbound files also appeared (e.g. ADR-0004, many 08-08 reports) — net **132**.
3. Same method family; this is not a claim that the prior count was wrong on its snapshot.

### 5.2 Zero-inbound by primary role

| role | zero-inbound | % of zeros |
| --- | ---: | ---: |
| `brief` | **101** | 76.5% |
| `report` | 16 | 12.1% |
| `spec` | 6 | 4.5% |
| `verification` | 5 | 3.8% |
| `other` | 2 | 1.5% |
| `decision` | 1 | 0.8% |
| `research` | 1 | 0.8% |
| handoff / roadmap / migration / reference | 0 | 0% |
| **Total** | **132** | 100% |

### 5.3 Should audit **alarm** on orphan?

| Class | Examples | Alarm? |
| --- | --- | --- |
| **Structural non-ref (expected)** | most `*brief*` after dispatch | **No** as “garbage.” Optional: “brief without report” after N days = **task hygiene** alarm, not knowledge decay. |
| **Historical evidence, rarely linked** | many `*report*`, unpaired acceptance files | **No** delete/archive alarm. Optional **retrieval demotion** only. |
| **Should be in the knowledge graph** | ADR-0001/0002 (basename weak; ID still thin), ADR-0004 (0), orphan verifications (5), orphan plans that defined waves | **Yes — graph coverage alarm** (“accepted decision never cited”), **not** retirement. |
| **Mis-filed high-value** | `execution/…-ui-premium-research.md`, verdicts only cited from handoff prose | **Warn on role/path mismatch**; do not archive. |
| **True junk** | none proven by zero-inbound alone | zero-inbound is **not** a junk detector in this corpus |

**Zero-inbound verification (alarm-worthy as coverage, not trash):**

- `verification/2026-07-30-wave-b2-h3-native-acceptance.md`
- `verification/2026-07-30-wave-b3-foundation-acceptance.md`
- `verification/2026-08-01-wave-c3c-acceptance.md`
- `verification/2026-08-01-wave-c3d-acceptance.md`
- `verification/2026-08-01-wave-c3e-acceptance.md`

**ADR short-ID mentions (excl. self), for coverage:**

| ID | other docs mentioning ID |
| --- | ---: |
| ADR-0001 | 1 |
| ADR-0002 | 2 |
| ADR-0003 | 7 |
| ADR-0004 | **0** |

Basename-only orphan lists **understate** ADR-0003 linkage and still show ADR-0001/0002 as nearly invisible to agents that do not search IDs.

---

## 6. If auto-retirement ran **today** (ADR-0004 §5: mark + archive, no hard delete)

### 6.1 Safe automatic marks (**explicit supersession only**)

Mark `status=superseded` (and optionally move to archive location later):

1. `docs/handoff/2026-08-01-pillow-controller-handoff.md`
2. `docs/handoff/2026-08-07-pillow-controller-handoff.md`
3. `docs/handoff/2026-08-08-round5-tech-debt-status.md`

Set `superseded_by` → `docs/handoff/2026-08-08-pillow-controller-handoff.md` (for #2 and #3); for #1 → 08-07 or transitively 08-08 (policy choice).

**False-positive risk: very low.** Coverage: **3 files**.

Optional soft mark (**推断**):

4. `docs/roadmap/2026-07-30-stage1-plan-comparison.md` → superseded_by overall-status  

**False-positive risk: low**, if treated as historical comparison kept readable.

### 6.2 Dangerous rules and false positives

| Rule | Would mark | False positives |
| --- | --- | --- |
| zero-inbound ⇒ retire | **132** files | ADR-0004, 5 verifications, 6 specs/plans, verdicts-class reports, methodology samples, unpaired briefs that may be open work |
| zero-inbound ∧ `*brief*` ⇒ archive | **101** briefs | **43** unpaired briefs (open/renamed/merged unknown); loses dispatch contracts |
| age > 7 days | large fraction of 07-29…07-31 foundation | early research + acceptance that are still authoritative history; **misses** same-day handoff/round5 contradiction |
| free-text Status not `ACTIVE` | many DONE reports | DONE means worker finished, not “retire knowledge” |
| path under execution only | 224 files | wipes the evidence corpus |

### 6.3 Signals needed to avoid false retirement

| Signal | Exists in tree today? |
| --- | --- |
| Explicit supersession path list | **Rare** (handoff chain only) |
| Registry written at dispatch/acceptance | **No** |
| `consumed` when report lands for same stem | **Inferable** from names (72 pairs), not recorded |
| `last_verified` / controller verdict record | **No** structured field; buried in handoff prose |
| `immutable_evidence` for verification/ADR | **No**; must be role-default policy |
| Task/run id linking brief↔report↔commit | **No** in docs (Keeper sessions scrubbed/placeholder in some reports) |
| Multi-target supersedes | Exists in prose; **not** schema-ready |

Without those, auto-retirement must stay at **§6.1 scale**.

---

## 7. Is role / status / supersedes enough?

### 7.1 Direct answer

| Piece | Enough? |
| --- | --- |
| `role` enum | **Barely for labeling** (`other` 1.4%). Weak for plans, proposals, verdicts, mis-filed research. |
| `status` 3-way | **Not enough.** Missing consumed / proposed / accepted / evidence / draft. |
| single `supersedes` | **Not enough** (1→N + pathless claims). |
| `content_digest` + `updated_at` | Necessary, not sufficient (detect change, not meaning). |

**Overall: not enough to implement ADR-0004 retirement/search goals safely.** Enough to start a **read-only registry prototype** if status defaults to `unknown` and retirement is explicit-edge-only.

### 7.2 Minimal schema changes (recommended)

Keep proposed columns; add the smallest set that fixes measured failures:

1. **`supersedes` → multi**  
   - `knowledge_document_links(from_id, to_id, rel)` with `rel ∈ {supersedes, related, implements, …}`  
   - or `supersedes_paths TEXT` JSON array if avoiding join table in K0.

2. **Expand `status` or split dimensions** (minimal useful set):

   | status | Meaning |
   | --- | --- |
   | `active` | current operational / binding |
   | `proposed` | design awaiting acceptance |
   | `accepted` | decision/design accepted (may still be historical) |
   | `consumed` | brief (or plan) fulfilled by a report/acceptance |
   | `superseded` | replaced by other document(s) |
   | `evidence` | immutable acceptance/report; not “current state” |
   | `unknown` | default until classified |
   | `archived` | retired from default search |

   If unwilling to expand enum: keep 3 values but add **`lifecycle_phase`** or **`retrieval_class`** (`primary` / `evidence` / `consumed` / `superseded` / `noise`).

3. **`role` small extensions (optional but cheap)**  
   - add `plan` (stop overloading `spec`)  
   - add `proposal` **or** allow `role=spec` + `status=proposed`  
   - add `diagnosis` only if cancel-tree style notes keep appearing; else `other` is fine at 1%.

4. **`secondary_roles` or tags** (optional JSON array) for duals: verdict+report, plan+verification.

5. **Provenance fields (high value, small)**  
   - `source_run_id` / `task_id`  
   - `paired_path` (brief↔report)  
   - `last_verified_at` + `verified_by_role` (controller|worker|lint)

6. **Policy defaults by role** (not necessarily columns):  
   - `verification` + `decision` → never auto-archive on orphan  
   - `brief` + paired report → auto `consumed`  
   - orphan brief > N days → **review queue**, not archive  
   - explicit supersession → `superseded` + exclude from default search

### 7.3 What not to do

- Do **not** treat zero-inbound as retirement.
- Do **not** treat worker `DONE` as document `superseded`.
- Do **not** assume directory = role without a correction table (mis-filed research already exists).
- Do **not** ship single `supersedes` column knowing 08-08 handoff is 1→2.

---

## 8. Uncertainties

1. **Inbound method** may miss ID-only or wave-only references; ADR coverage numbers are lower bounds on “remembered,” upper bounds on “orphaned.”
2. **43 unpaired briefs** were not manually opened one-by-one; some may pair under non-stem naming (multi-worker → one integration report).
3. **Handoff freshness** at report time was not re-validated against live git HEAD or in-flight workers (read-only, no gate). ADR-0004 already records the rot pattern.
4. **This audit file** is not included in the 294 counts above; after write, totals become 295 and this file starts at zero inbound until something links it.
5. Keeper bridge boot returned HTTP 400 in this environment; audit did not depend on it.
6. Chinese supersession lines are unambiguous; any English pathless “supersedes prior status” may be incomplete edges rather than true supersession.

---

## 9. Bottom line

- **`other` = 1.4%** → role table does **not** fail the 15% rule.  
- Classification still **fails operational needs** because **status is almost always undecidable** and **supersession is multi-valued and sparse**.  
- Independent orphan count: **132 / 294**, not 134 / 287 — same story, updated corpus.  
- Today’s safe auto-mark set is **three handoff-chain files**; anything broader needs new signals the tree does not yet have.  
- Minimal fix before migration: **multi supersedes**, **richer status (or retrieval_class)**, **role-aware non-retirement defaults**, optional **`plan`** role.

---

DOCS-AUDIT-COMPLETE
