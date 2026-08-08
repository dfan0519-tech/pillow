# Docs debt reconciliation — 43 unpaired briefs + 2 roadmaps

Date: 2026-08-09  
Worker: grok3  
Controller: Claude  
Mode: **docs only** (no cargo / gate / test / E2E; no delete/move/rename; no body rewrites)  
Authority sources (read-only evidence):

- `docs/research/2026-08-08-knowledge-audit-run.md` §2 / §4
- `docs/decisions/ADR-0004-knowledge-layer-actors-and-registration.md` §5
- `docs/research/2026-08-08-docs-inventory-audit.md`

Keeper Bridge boot: HTTP 400 (same class as inventory audit); work continued from written contract.

---

## 0. Verdict

| Bucket | Count |
| --- | ---: |
| 已完成,报告命名不同 | 13 |
| 已完成,成果并入他处 | 29 |
| 未完成 | 1 |
| 已作废 | 0 |
| 无法判定 | 0 |
| **Total unpaired briefs** | **43** |

Roadmap contradiction: **resolved mechanically** — plan-comparison is historical; overall-status remains the current Stage 1 ladder document (see §2).

Explicit supersession line written (查证 only): **1 file**.

---

## 1. 43 unpaired briefs — row-by-row

Legend for **声明**: `Y` = top-of-file supersession line added this pass; `N` = not added (no proven *replace-as-truth* edge; pairing is consumed/fulfillment, not supersession).

### 1.1 Early wave action briefs (4)

| # | Brief | 判定 | 证据 | 声明 |
| --- | --- | --- | --- | --- |
| 1 | `docs/execution/2026-07-30-wave-b1-action-brief.md` | **已完成,成果并入他处** | Brief names controller report path `docs/verification/2026-07-30-slice-b1-acceptance.md`. That file exists with verdict **PASS** for Slice B1. Worker paths under `.superpowers/sdd/` were orchestration artifacts, not `docs/execution/*report*`. | N |
| 2 | `docs/execution/2026-07-30-wave-b2-action-brief.md` | **已完成,成果并入他处** | `docs/verification/2026-07-30-slice-b2-acceptance.md` exists (PASS for merge). Overall status / C ladder treat B2 as Accepted. | N |
| 3 | `docs/execution/2026-07-30-wave-b3-action-brief.md` | **已完成,成果并入他处** | `docs/verification/2026-07-30-wave-b3-foundation-acceptance.md` + `docs/verification/2026-07-31-wave-b3-read-execution-acceptance.md` exist; roadmap ladder marks B3 Accepted. | N |
| 4 | `docs/execution/2026-07-31-wave-c1-action-brief.md` | **已完成,成果并入他处** | `docs/verification/2026-07-31-wave-c1-acceptance.md` (ACCEPTED) + `docs/verification/2026-07-31-wave-c1-checkpoint.md` + visual acceptance. | N |

### 1.2 C2 agent-activity fix briefs (4) — append-to-parent-report pattern

| # | Brief | 判定 | 证据 | 声明 |
| --- | --- | --- | --- | --- |
| 5 | `…/2026-07-31-wave-c2-agent-activity-dependent-tests-fix-brief.md` | **已完成,成果并入他处** | Brief whitelist: append only to `…-agent-activity-timeline-report.md`. Parent report has section **Dependent tests migration** with RED/GREEN and DONE. | N |
| 6 | `…/2026-07-31-wave-c2-agent-activity-review-fix-brief.md` | **已完成,成果并入他处** | Parent report section **Review-fix pass (Codex2 Important × 6)** Status `DONE`, maps the six findings. | N |
| 7 | `…/2026-07-31-wave-c2-agent-activity-review-fix2-brief.md` | **已完成,成果并入他处** | Parent report section with **Confirmed remaining Important findings (closed)** matches brief's two findings (`recordKey`, receipt/resource IDs); gates **review-fix 2**. | N |
| 8 | `…/2026-07-31-wave-c2-agent-activity-test-warning-fix-brief.md` | **已完成,成果并入他处** | Parent report section **Duplicate-key test fixture cleanup** Status `DONE`; closes residual from fix2. | N |

### 1.3 C2-C3 / C2-D fix briefs (5)

| # | Brief | 判定 | 证据 | 声明 |
| --- | --- | --- | --- | --- |
| 9 | `…/2026-07-31-wave-c2c3-preview-review-fix-brief.md` | **已完成,成果并入他处** | `…-c2c3-preview-report.md` section **Codex2 C2-C3 Review Fix — image DTO boundary (Important)** records the fix. | N |
| 10 | `…/2026-07-31-wave-c2d-open-project-race-fix-brief.md` | **已完成,成果并入他处** | `…-c2d-resource-workbench-integration-report.md` section **Controller rejection follow-up — open/register last-request-wins** with RED/GREEN. | N |
| 11 | `…/2026-07-31-wave-c2d-project-selection-race-fix-brief.md` | **已完成,成果并入他处** | Same report: **Controller acceptance fix — project selection last-request-wins race**. | N |
| 12 | `…/2026-07-31-wave-c2d-resource-workbench-controller-fix-brief.md` | **已完成,成果并入他处** | Same report: **Controller acceptance fix — stale `previewPathRequest` resurrection** (matches brief finding). | N |
| 13 | `…/2026-07-31-wave-c2d-stale-picker-registration-fix-brief.md` | **已完成,成果并入他处** | Same report: **Controller acceptance fix — stale picker registration** with dedicated RED/GREEN. | N |

### 1.4 C3 research briefs (3)

| # | Brief | 判定 | 证据 | 声明 |
| --- | --- | --- | --- | --- |
| 14 | `…/2026-08-01-wave-c3-research-grok1-grok-app-brief.md` | **已完成,报告命名不同** | Brief **Write only** path: `docs/research/2026-08-01-c3-grok-app-interaction-research.md`. File exists; agent/role match grok1 Grok App research. Not named `*report*`. | N |
| 15 | `…/2026-08-01-wave-c3-research-grok2-zbbody-brief.md` | **已完成,报告命名不同** | Deliverable present as `docs/research/2026-08-01-c3-zbbody-interaction-research.md` (grok2 zbbody research). | N |
| 16 | `…/2026-08-01-wave-c3-research-grok3-security-brief.md` | **已完成,报告命名不同** | Brief allowlists `docs/research/2026-08-01-c3-governed-write-security-research.md`. File exists. | N |

### 1.5 C3-A…E controller rejection / final-fix briefs (9)

Pattern (查证): each fix brief requires updating the **same-wave implementation report**, not a new `*-controller-*-report.md`. Reports still show COMPLETION DONE and document the rejection/final-fix pass.

| # | Brief | 判定 | 证据 | 声明 |
| --- | --- | --- | --- | --- |
| 17 | `…/2026-08-01-wave-c3a-controller-rejection-fix-brief.md` | **已完成,成果并入他处** | `…-c3a-protocol-policy-report.md` Task line: `c3a-protocol-policy (controller rejection fix)`; **COMPLETION: DONE**; rejection text matches brief. | N |
| 18 | `…/2026-08-01-wave-c3b-controller-rejection-fix-brief.md` | **已完成,成果并入他处** | `…-c3b-storage-ledger-report.md` Task: `… + controller rejection fix + controller final narrow fix`; **COMPLETION: DONE**. | N |
| 19 | `…/2026-08-01-wave-c3b-controller-final-fix-brief.md` | **已完成,成果并入他处** | Same report covers final narrow fix vocabulary/migration checks; brief whitelist includes that report. | N |
| 20 | `…/2026-08-01-wave-c3c-controller-rejection-fix-brief.md` | **已完成,成果并入他处** | `…-c3c-write-executor-report.md` records prior rejection pass + **final security fix**; **COMPLETION: DONE**. | N |
| 21 | `…/2026-08-01-wave-c3c-controller-final-security-fix-brief.md` | **已完成,成果并入他处** | Same report: Task is **final security fix**; findings F1/F2 closed. | N |
| 22 | `…/2026-08-01-wave-c3d-controller-rejection-fix-brief.md` | **已完成,成果并入他处** | `…-c3d-runtime-native-report.md` closes remaining rejection findings after first rejection repair; lists final-fix brief in changed-files table. | N |
| 23 | `…/2026-08-01-wave-c3d-controller-final-fix-brief.md` | **已完成,成果并入他处** | Same report: Owner/task = controller final fix; **COMPLETION: DONE**. | N |
| 24 | `…/2026-08-01-wave-c3e-controller-rejection-fix-brief.md` | **已完成,成果并入他处** | `…-c3e-approval-ui-report.md` section **Prior controller rejection findings — still closed**. | N |
| 25 | `…/2026-08-01-wave-c3e-controller-final-fix-brief.md` | **已完成,成果并入他处** | Same report: Task **controller final correction**; **Controller final-fix findings — closed**; English prose “supersedes prior delivery status” (pathless — already noted by inventory). | N |

### 1.6 C3 independent review + C3-F chain (8)

| # | Brief | 判定 | 证据 | 声明 |
| --- | --- | --- | --- | --- |
| 26 | `…/2026-08-01-wave-c3-codex2-final-review-brief.md` | **未完成** | Brief requires write-only path `docs/verification/2026-08-01-wave-c3-codex2-final-review.md`. **That file does not exist** (tree search). C3 later closed via controller acceptance (`docs/verification/2026-08-02-wave-c3-acceptance.md` / checkpoint) and fixture work by Codex2, but **no artifact fulfills this brief's named deliverable**. Not reclassified as 已作废 without an explicit cancel/replace statement. | N |
| 27 | `…/2026-08-01-wave-c3f-desktop-integration-brief.md` | **已完成,成果并入他处** | Original C3-F acceptance contract. Evidence path chain ends in `docs/execution/2026-08-01-wave-c3-integration-report.md` (intermediate) and final `docs/verification/2026-08-02-wave-c3-acceptance.md` (all eight scenarios PASS, including two-projects / forced-restart). | N |
| 28 | `…/2026-08-01-wave-c3f-forced-logout-recovery-brief.md` | **已完成,成果并入他处** | Brief requires producing `…-c3-integration-report.md` after interrupted run; report exists. Wave later controller-accepted. | N |
| 29 | `…/2026-08-01-wave-c3f-resume-after-schema-fix-brief.md` | **已完成,成果并入他处** | Brief points at accepted schema fix + re-run; schema acceptance exists; integration report + final C3 acceptance cover re-run outcome. | N |
| 30 | `…/2026-08-01-wave-c3f-resume-after-product-fixes-brief.md` | **已完成,成果并入他处** | Brief requires refresh of integration report + acceptance after schema/cancel/privacy acceptances; prerequisites exist under `docs/verification/2026-08-01-wave-c3f-*-acceptance.md`; final C3 acceptance records full scenario set. | N |
| 31 | `…/2026-08-01-wave-c3f-final-resume-after-provider-fix-brief.md` | **已完成,成果并入他处** | Integration report **title** is “Final Resume After Provider Cancel Recovery”; cites provider-cancel acceptance; later C3 acceptance is green on cancel recovery path. | N |
| 32 | `…/2026-08-01-wave-c3f-native-path-privacy-controller-rejection-fix-brief.md` | **已完成,报告命名不同** | Report `…-c3f-native-path-privacy-fix-report.md` + acceptance `…-c3f-native-path-privacy-fix-acceptance.md` address the same rejection contract (sidebar `title` / full-body path privacy). | N |
| 33 | `…/2026-08-01-wave-c3f-run-events-schema-controller-rejection-fix-brief.md` | **已完成,报告命名不同** | `…-c3f-run-events-schema-fix-report.md` cites this brief as **Governing brief** under controller rejection; Status DONE; matching verification acceptance exists. | N |

### 1.7 C4-R research (5)

| # | Brief | 判定 | 证据 | 声明 |
| --- | --- | --- | --- | --- |
| 34 | `…/2026-08-02-wave-c4r-research-dispatch-brief.md` | **已完成,成果并入他处** | Dispatch table assigns four sole report paths under `docs/research/`; all four files exist (see rows 35–38). | N |
| 35 | `…/2026-08-02-wave-c4r-grok1-terminal-process-research-brief.md` | **已完成,报告命名不同** | Allowlist: `docs/research/2026-08-02-c4-terminal-process-research.md` (exists; grok1). | N |
| 36 | `…/2026-08-02-wave-c4r-grok2-git-governance-research-brief.md` | **已完成,报告命名不同** | Deliverable: `docs/research/2026-08-02-c4-git-governance-research.md`. | N |
| 37 | `…/2026-08-02-wave-c4r-grok3-interaction-research-brief.md` | **已完成,报告命名不同** | Deliverable: `docs/research/2026-08-02-c4-terminal-git-interaction-research.md`. | N |
| 38 | `…/2026-08-02-wave-c4r-grok4-security-acceptance-research-brief.md` | **已完成,报告命名不同** | Deliverable: `docs/research/2026-08-02-c4-security-acceptance-research.md`. | N |

### 1.8 C4 D3AR / D3B (5)

| # | Brief | 判定 | 证据 | 声明 |
| --- | --- | --- | --- | --- |
| 39 | `…/2026-08-04-wave-c4-d3ar-grok1-process-authority-events-output-brief.md` | **已完成,报告命名不同** | Brief allowlists `…-d3ar-grok-process-authority-events-output-report.md` (drops the `1` in the filename). Report exists, Status **DONE_WITH_CONCERNS**, same task id. | N |
| 40 | `…/2026-08-04-wave-c4-d3ar2-grok1-live-output-lifecycle-brief.md` | **已完成,报告命名不同** | Brief allowlists `…-d3ar2-codex2-live-output-lifecycle-report.md` (worker is Codex2 despite `grok1` in brief filename). Report exists with matching lifecycle contract. | N |
| 41 | `…/2026-08-04-wave-c4-d3ar2-grok2-process-event-durability-brief.md` | **已完成,报告命名不同** | Report `…-d3ar2-grok4-process-event-durability-report.md` header field **Brief:** points at this exact brief path; worker was grok4 (brief text assigns grok4 despite `grok2` in brief name). | N |
| 42 | `…/2026-08-04-wave-c4-d3b-grok4-integration-research-brief.md` | **已完成,报告命名不同** | Allowlist-only research: `docs/research/2026-08-04-c4-d3b-grok4-process-ui-integration-map.md`. File exists; **Brief:** field cites this brief. | N |
| 43 | `…/2026-08-05-wave-c4-d3b4-grok4-git-workbench-repair-brief.md` | **已完成,成果并入他处** | Brief requires append to `…-grok4-git-workbench-report.md`. Report section **Narrow repair (Round 2B rework)** ends with `C4-D3B4-GROK4-GIT-WORKBENCH-REPAIR-COMPLETE`. | N |

---

## 2. “无法判定” list

**Empty.** Every of the 43 briefs had enough tree evidence for one of the allowed labels.  
(If future scanners re-open #26, the only open gap is the missing named Codex2 review file — classified **未完成**, not 无法判定.)

---

## 3. Roadmap contradiction (2 files)

| File | Prior scanner state | This pass |
| --- | --- | --- |
| `docs/roadmap/2026-07-30-stage1-plan-comparison.md` | non-superseded; soft prose “current state moved to overall-status” | **Historical.** Explicit supersession line **added** (see §4). |
| `docs/roadmap/2026-07-31-stage1-overall-status.md` | non-superseded | **Still the current Stage 1 ladder / status roadmap.** Header: `updated 2026-08-07`; Status: C3 accepted; C4 checkpointed through D3B4 with cancel-tree risk. |

### 3.1 Why overall-status is “current” (not both obsolete)

**查证:**

1. Plan-comparison’s own Status already points current state at overall-status (soft pointer, inventory audit §4.1, K0b §8).
2. Overall-status content is newer (through D3B4 / 2026-08-07) and matches the accepted C4 checkpoint narrative.
3. Current handoff `docs/handoff/2026-08-08-pillow-controller-handoff.md` is **handoff/ops**, not a third roadmap; it does not claim to replace the roadmap ladder document.

**Not claimed as 查证 (residual only):** overall-status does not inventory every 2026-08-08 brand/debt/UI round. That is **staleness risk**, not dual-current-roadmap contradiction. Controller may still choose to write a fresher roadmap; this pass does **not** invent one.

### 3.2 Disposition performed

- **Do not delete** either file.
- On plan-comparison, add mechanical supersession line (scanner-visible).
- Leave overall-status body untouched (still current for Stage 1 ladder).

---

## 4. Supersession declarations written (查证)

| File | Line added | Why 查证 (not 推断) |
| --- | --- | --- |
| `docs/roadmap/2026-07-30-stage1-plan-comparison.md` | `> 本文件已被 docs/roadmap/2026-07-31-stage1-overall-status.md 取代。` | File’s own Status already names that target; inventory audit soft pointer; matches handoff-style `本文件已被 … 取代` convention. |

**No other supersession lines written.**  
Completed fix/research briefs are **consumed / fulfilled** by reports (inventory: `consumed`, not `superseded`). Writing “已被 report 取代” on a brief would invent a lifecycle edge the corpus does not use for brief↔report pairing.

---

## 5. 疑似 only (推断 — not written into source docs)

These are useful for the controller / knowledge layer, **not** written as supersession prose:

| 疑似 | Reason (推断) |
| --- | --- |
| Intermediate C3-F resume briefs form a chronological active-contract chain (desktop → forced-logout → schema resume → product resume → final-resume) | Later briefs assume earlier blockers accepted; none declare “本文件取代 prior resume brief”. |
| `…-c3-integration-report.md` Status still **BLOCKED** on two-projects while `…-c3-acceptance.md` later accepts all scenarios | Integration report looks like a mid-chain snapshot; final truth lives in verification acceptance + `…-c3f-project-isolation-fix-report.md`. No explicit “report superseded by acceptance” edge. |
| C3-E report’s pathless “supersedes prior delivery status” | Already inventoried; still non-addressable. |
| Codex2 final-review brief may have been **operationally skipped** when controller source review closed C3 | Plausible process change; **no cancel receipt** → remains **未完成**, not 已作废. |
| Overall-status may lag post-2026-08-07 brand/debt work | Ops handoff is fresher for “what workers do now”; not a second roadmap claiming currency of the ladder table. |

---

## 6. What this worker did **not** touch

- Any `crates/**`, `apps/**`, `scripts/**`, `tests/**`
- `docs/decisions/**`, `docs/handoff/**`, `docs/research/**` (audit evidence left intact)
- Any delete / move / rename / body rewrite of existing docs
- Git (`add` / `commit` / `stash` / `reset` / `clean` / `checkout` / `restore`)
- cargo / pnpm / gates / unit / integration / E2E
- `.superpowers/**` (except not used)
- Auth / token files

**Writes this pass:**

1. Top supersession line on `docs/roadmap/2026-07-30-stage1-plan-comparison.md`
2. This report: `docs/execution/2026-08-09-docs-debt-reconciliation-report.md`

---

## 7. Residual risks

1. **One true open debt:** `wave-c3-codex2-final-review-brief` deliverable missing. C3 is product-accepted without that independent review file. Controller should either (a) commission the named review retroactively, (b) declare the brief 已作废 with an explicit supersession to controller acceptance, or (c) leave it as permanent incomplete evidence.
2. **Consumed ≠ superseded:** 42/43 briefs remain unpaired by *stem* scanners even after this reconciliation until the registry learns rename/merge rules or explicit `paired_path` links. This report is the pairing evidence; it does not rewrite brief filenames.
3. **Integration report vs acceptance:** mid-chain BLOCKED wording in `…-c3-integration-report.md` can still mislead agents that do not load the later acceptance. No edit made (body rewrite forbidden; supersession not explicit in source).
4. **Roadmap freshness:** overall-status is current *among roadmaps*, not guaranteed paragraph-fresh vs 08-08 handoff.
5. **Method limit:** evidence is docs tree text + prescribed allowlists only (no git history proof of who ran which brief). Per task rules, unpaired ≠ over-claim.

---

## 8. Counts for knowledge-layer feed

| 判定 | n | IDs in §1 |
| --- | ---: | --- |
| 已完成,报告命名不同 | 13 | 14–16, 32–33, 35–42 |
| 已完成,成果并入他处 | 29 | 1–13, 17–25, 27–31, 34, 43 |
| 未完成 | 1 | 26 |
| 已作废 | 0 | — |
| 无法判定 | 0 | — |

Roadmap: plan-comparison → **superseded (explicit)**; overall-status → **current ladder**.

---

DOCS-DEBT-COMPLETE
