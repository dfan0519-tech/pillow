# Knowledge audit run — K0b indexer

Date: 2026-08-08
Mode: **register + audit only** (no retirement, no delete, no docs/ mutation)

---

## 1. Registration summary

- **Total registered:** 296
- **`unknown` ratio:** 72.3% (214/296)

### Role distribution

| role | count |
| --- | ---: |
| `brief` | 115 |
| `decision` | 4 |
| `handoff` | 4 |
| `migration` | 1 |
| `other` | 4 |
| `plan` | 15 |
| `reference` | 1 |
| `report` | 102 |
| `research` | 17 |
| `roadmap` | 2 |
| `spec` | 5 |
| `verification` | 26 |

### Status distribution

| status | count |
| --- | ---: |
| `accepted` | 4 |
| `consumed` | 72 |
| `proposed` | 3 |
| `superseded` | 3 |
| `unknown` | 214 |

## 2. Contradictions

- **docs/roadmap** — 2 non-superseded roadmap docs: docs/roadmap/2026-07-30-stage1-plan-comparison.md, docs/roadmap/2026-07-31-stage1-overall-status.md

## 3. Orphans (zero text-inbound), by role

**Total zero-inbound:** 102

### `brief` (101)

- `docs/execution/2026-07-30-wave-b1-action-brief.md`
- `docs/execution/2026-07-30-wave-b2-action-brief.md`
- `docs/execution/2026-07-30-wave-b3-action-brief.md`
- `docs/execution/2026-07-31-wave-c1-action-brief.md`
- `docs/execution/2026-07-31-wave-c2-agent-activity-dependent-tests-fix-brief.md`
- `docs/execution/2026-07-31-wave-c2-agent-activity-review-fix2-brief.md`
- `docs/execution/2026-07-31-wave-c2-agent-activity-test-warning-fix-brief.md`
- `docs/execution/2026-07-31-wave-c2a-resource-core-brief.md`
- `docs/execution/2026-07-31-wave-c2b-resource-bridge-brief.md`
- `docs/execution/2026-07-31-wave-c2c1-resource-panel-brief.md`
- `docs/execution/2026-07-31-wave-c2c2-agent-activity-brief.md`
- `docs/execution/2026-07-31-wave-c2c3-preview-brief.md`
- `docs/execution/2026-07-31-wave-c2c3-preview-review-fix-brief.md`
- `docs/execution/2026-07-31-wave-c2d-open-project-race-fix-brief.md`
- `docs/execution/2026-07-31-wave-c2d-project-selection-race-fix-brief.md`
- `docs/execution/2026-07-31-wave-c2d-resource-workbench-controller-fix-brief.md`
- `docs/execution/2026-07-31-wave-c2d-resource-workbench-integration-brief.md`
- `docs/execution/2026-07-31-wave-c2d-stale-picker-registration-fix-brief.md`
- `docs/execution/2026-07-31-wave-c2e-activity-driver-brief.md`
- `docs/execution/2026-07-31-wave-c2e-cdp-driver-fix-brief.md`
- `docs/execution/2026-07-31-wave-c2e-controller-rejection-fix-brief.md`
- `docs/execution/2026-07-31-wave-c2e-orchestrator-brief.md`
- `docs/execution/2026-07-31-wave-c2e-resource-driver-brief.md`
- `docs/execution/2026-08-01-desktop-process-cleanup-safety-fix-brief.md`
- `docs/execution/2026-08-01-wave-c3-broad-gate-expiry-fixture-fix-brief.md`
- `docs/execution/2026-08-01-wave-c3-codex2-final-review-brief.md`
- `docs/execution/2026-08-01-wave-c3-codex2-profile-snapshot-api-development-brief.md`
- `docs/execution/2026-08-01-wave-c3-grok4-expiry-fixture-development-brief.md`
- `docs/execution/2026-08-01-wave-c3-research-grok1-grok-app-brief.md`
- `docs/execution/2026-08-01-wave-c3-research-grok2-zbbody-brief.md`
- `docs/execution/2026-08-01-wave-c3-research-grok3-security-brief.md`
- `docs/execution/2026-08-01-wave-c3a-controller-rejection-fix-brief.md`
- `docs/execution/2026-08-01-wave-c3a-protocol-policy-brief.md`
- `docs/execution/2026-08-01-wave-c3b-controller-final-fix-brief.md`
- `docs/execution/2026-08-01-wave-c3b-controller-rejection-fix-brief.md`
- `docs/execution/2026-08-01-wave-c3b-storage-ledger-brief.md`
- `docs/execution/2026-08-01-wave-c3c-controller-final-security-fix-brief.md`
- `docs/execution/2026-08-01-wave-c3c-controller-rejection-fix-brief.md`
- `docs/execution/2026-08-01-wave-c3c-write-executor-brief.md`
- `docs/execution/2026-08-01-wave-c3d-controller-rejection-fix-brief.md`
- `docs/execution/2026-08-01-wave-c3e-approval-ui-brief.md`
- `docs/execution/2026-08-01-wave-c3e-controller-final-fix-brief.md`
- `docs/execution/2026-08-01-wave-c3e-controller-rejection-fix-brief.md`
- `docs/execution/2026-08-01-wave-c3f-cancel-resolution-arbitration-fix-brief.md`
- `docs/execution/2026-08-01-wave-c3f-final-resume-after-provider-fix-brief.md`
- `docs/execution/2026-08-01-wave-c3f-forced-logout-recovery-brief.md`
- `docs/execution/2026-08-01-wave-c3f-native-path-privacy-controller-rejection-fix-brief.md`
- `docs/execution/2026-08-01-wave-c3f-project-isolation-fix-brief.md`
- `docs/execution/2026-08-01-wave-c3f-provider-cancel-rpc-completion-fix-brief.md`
- `docs/execution/2026-08-01-wave-c3f-real-profile-snapshot-harness-fix-brief.md`
- `docs/execution/2026-08-01-wave-c3f-resume-after-product-fixes-brief.md`
- `docs/execution/2026-08-01-wave-c3f-resume-after-schema-fix-brief.md`
- `docs/execution/2026-08-01-wave-c3f-run-events-schema-fix-brief.md`
- `docs/execution/2026-08-02-wave-c3-codex2-cli-desktop-smoke-parent-fixture-development-brief.md`
- `docs/execution/2026-08-02-wave-c3-codex2-cli-run-cargo-selftest-development-brief.md`
- `docs/execution/2026-08-02-wave-c3-codex2-incomplete-descendant-age-development-brief.md`
- `docs/execution/2026-08-02-wave-c3-codex2-vctip-process-boundary-development-brief.md`
- `docs/execution/2026-08-02-wave-c4-d1-grok1-protocol-development-brief.md`
- `docs/execution/2026-08-02-wave-c4-d1-grok2-policy-development-brief.md`
- `docs/execution/2026-08-02-wave-c4-d1-grok3-ui-primitives-development-brief.md`
- `docs/execution/2026-08-02-wave-c4-d1-grok4-git-parser-development-brief.md`
- `docs/execution/2026-08-02-wave-c4-d1r-codex2-rust-integration-brief.md`
- `docs/execution/2026-08-02-wave-c4-d1r-grok3-argv-display-brief.md`
- `docs/execution/2026-08-02-wave-c4-d1r2-grok4-clippy-fix-brief.md`
- `docs/execution/2026-08-02-wave-c4-d1r3-grok3-renderer-grant-boundary-fix-brief.md`
- `docs/execution/2026-08-02-wave-c4-d2a-codex2-process-executor-development-brief.md`
- `docs/execution/2026-08-02-wave-c4-d2a-grok1-exec-ledger-development-brief.md`
- `docs/execution/2026-08-02-wave-c4-d2ar-codex2-process-order-repair-brief.md`
- `docs/execution/2026-08-02-wave-c4-d2ar-grok1-single-claim-repair-brief.md`
- `docs/execution/2026-08-02-wave-c4r-grok1-terminal-process-research-brief.md`
- `docs/execution/2026-08-02-wave-c4r-grok2-git-governance-research-brief.md`
- `docs/execution/2026-08-02-wave-c4r-grok3-interaction-research-brief.md`
- `docs/execution/2026-08-02-wave-c4r-grok4-security-acceptance-research-brief.md`
- `docs/execution/2026-08-02-wave-c4r-research-dispatch-brief.md`
- `docs/execution/2026-08-03-wave-c4-d2b-grok2-governed-process-gateway-brief.md`
- `docs/execution/2026-08-03-wave-c4-d2b-grok4-hardened-git-runner-brief.md`
- `docs/execution/2026-08-03-wave-c4-d2br-grok2-process-boundary-repair-brief.md`
- `docs/execution/2026-08-03-wave-c4-d2br-grok4-git-containment-repair-brief.md`
- `docs/execution/2026-08-03-wave-c4-d2br2-grok2-sealed-authority-repair-brief.md`
- `docs/execution/2026-08-03-wave-c4-d2br2-grok4-metadata-no-follow-repair-brief.md`
- `docs/execution/2026-08-04-wave-c4-d3a-codex2-typed-native-ts-gateways-brief.md`
- `docs/execution/2026-08-04-wave-c4-d3a-grok1-runtime-host-process-brief.md`
- `docs/execution/2026-08-04-wave-c4-d3ar-grok1-process-authority-events-output-brief.md`
- `docs/execution/2026-08-04-wave-c4-d3ar2-grok1-live-output-lifecycle-brief.md`
- `docs/execution/2026-08-04-wave-c4-d3ar2a-codex2-format-repair-brief.md`
- `docs/execution/2026-08-04-wave-c4-d3ar2b-codex2-adapter-event-exhaustiveness-repair-brief.md`
- `docs/execution/2026-08-04-wave-c4-d3b1-codex2-native-process-bridge-brief.md`
- `docs/execution/2026-08-04-wave-c4-d3b1-codex2-portable-cwd-repair-brief.md`
- `docs/execution/2026-08-04-wave-c4-d3b1-grok4-approval-arbitration-brief.md`
- `docs/execution/2026-08-04-wave-c4-d3b2-codex2-live-retained-brief.md`
- `docs/execution/2026-08-04-wave-c4-d3b2-grok4-app-refresh-brief.md`
- `docs/execution/2026-08-04-wave-c4-d3b2-grok4-workbench-mount-brief.md`
- `docs/execution/2026-08-04-wave-c4-d3b2-grok4-workbench-repair-brief.md`
- `docs/execution/2026-08-05-wave-c4-d3b3-codex2-provider-process-roundtrip-brief.md`
- `docs/execution/2026-08-05-wave-c4-d3b3-codex2-write-regression-contract-repair-brief.md`
- `docs/execution/2026-08-05-wave-c4-d3b3-grok4-real-desktop-process-e2e-brief.md`
- `docs/execution/2026-08-05-wave-c4-d3b3-grok4-run-cargo-cleanup-race-brief.md`
- `docs/execution/2026-08-05-wave-c4-d3b3-grok4-visual-proof-repair-brief.md`
- `docs/execution/2026-08-05-wave-c4-d3b4-grok1-native-git-brief.md`
- `docs/execution/2026-08-05-wave-c4-d3b4-grok4-git-workbench-repair-brief.md`
- `docs/execution/2026-08-06-wave-c4-d3b4-grok4-real-desktop-git-e2e-brief.md`

### `report` (1)

- `docs/execution/2026-08-08-knowledge-registry-k0a-report.md`

## 4. Unpaired briefs (待复核, not retirement)

**Count:** 43

- `docs/execution/2026-07-30-wave-b1-action-brief.md`
- `docs/execution/2026-07-30-wave-b2-action-brief.md`
- `docs/execution/2026-07-30-wave-b3-action-brief.md`
- `docs/execution/2026-07-31-wave-c1-action-brief.md`
- `docs/execution/2026-07-31-wave-c2-agent-activity-dependent-tests-fix-brief.md`
- `docs/execution/2026-07-31-wave-c2-agent-activity-review-fix-brief.md`
- `docs/execution/2026-07-31-wave-c2-agent-activity-review-fix2-brief.md`
- `docs/execution/2026-07-31-wave-c2-agent-activity-test-warning-fix-brief.md`
- `docs/execution/2026-07-31-wave-c2c3-preview-review-fix-brief.md`
- `docs/execution/2026-07-31-wave-c2d-open-project-race-fix-brief.md`
- `docs/execution/2026-07-31-wave-c2d-project-selection-race-fix-brief.md`
- `docs/execution/2026-07-31-wave-c2d-resource-workbench-controller-fix-brief.md`
- `docs/execution/2026-07-31-wave-c2d-stale-picker-registration-fix-brief.md`
- `docs/execution/2026-08-01-wave-c3-codex2-final-review-brief.md`
- `docs/execution/2026-08-01-wave-c3-research-grok1-grok-app-brief.md`
- `docs/execution/2026-08-01-wave-c3-research-grok2-zbbody-brief.md`
- `docs/execution/2026-08-01-wave-c3-research-grok3-security-brief.md`
- `docs/execution/2026-08-01-wave-c3a-controller-rejection-fix-brief.md`
- `docs/execution/2026-08-01-wave-c3b-controller-final-fix-brief.md`
- `docs/execution/2026-08-01-wave-c3b-controller-rejection-fix-brief.md`
- `docs/execution/2026-08-01-wave-c3c-controller-final-security-fix-brief.md`
- `docs/execution/2026-08-01-wave-c3c-controller-rejection-fix-brief.md`
- `docs/execution/2026-08-01-wave-c3d-controller-final-fix-brief.md`
- `docs/execution/2026-08-01-wave-c3d-controller-rejection-fix-brief.md`
- `docs/execution/2026-08-01-wave-c3e-controller-final-fix-brief.md`
- `docs/execution/2026-08-01-wave-c3e-controller-rejection-fix-brief.md`
- `docs/execution/2026-08-01-wave-c3f-desktop-integration-brief.md`
- `docs/execution/2026-08-01-wave-c3f-final-resume-after-provider-fix-brief.md`
- `docs/execution/2026-08-01-wave-c3f-forced-logout-recovery-brief.md`
- `docs/execution/2026-08-01-wave-c3f-native-path-privacy-controller-rejection-fix-brief.md`
- `docs/execution/2026-08-01-wave-c3f-resume-after-product-fixes-brief.md`
- `docs/execution/2026-08-01-wave-c3f-resume-after-schema-fix-brief.md`
- `docs/execution/2026-08-01-wave-c3f-run-events-schema-controller-rejection-fix-brief.md`
- `docs/execution/2026-08-02-wave-c4r-grok1-terminal-process-research-brief.md`
- `docs/execution/2026-08-02-wave-c4r-grok2-git-governance-research-brief.md`
- `docs/execution/2026-08-02-wave-c4r-grok3-interaction-research-brief.md`
- `docs/execution/2026-08-02-wave-c4r-grok4-security-acceptance-research-brief.md`
- `docs/execution/2026-08-02-wave-c4r-research-dispatch-brief.md`
- `docs/execution/2026-08-04-wave-c4-d3ar-grok1-process-authority-events-output-brief.md`
- `docs/execution/2026-08-04-wave-c4-d3ar2-grok1-live-output-lifecycle-brief.md`
- `docs/execution/2026-08-04-wave-c4-d3ar2-grok2-process-event-durability-brief.md`
- `docs/execution/2026-08-04-wave-c4-d3b-grok4-integration-research-brief.md`
- `docs/execution/2026-08-05-wave-c4-d3b4-grok4-git-workbench-repair-brief.md`

## 5. Superseded but still referenced

- `docs/handoff/2026-08-01-pillow-controller-handoff.md` — 5 other document(s) still mention basename
- `docs/handoff/2026-08-07-pillow-controller-handoff.md` — 4 other document(s) still mention basename
- `docs/handoff/2026-08-08-round5-tech-debt-status.md` — 5 other document(s) still mention basename

## 6. Unclassifiable / path-content conflict (`other`)

**Count:** 4

- `docs/execution/2026-08-04-wave-c4-d3b1-exact-grok4-routing-approval.md` — execution path without classifiable filename → other; no explicit lifecycle → unknown
- `docs/execution/2026-08-07-wave-c4-d3b3-cancel-tree-diagnosis.md` — execution path without classifiable filename → other; no explicit lifecycle → unknown
- `docs/execution/2026-08-07-wave-c4-d3b3-cancel-tree-rootcause.md` — execution path without classifiable filename → other; no explicit lifecycle → unknown
- `docs/execution/2026-08-08-adapter-decoupling-proposal.md` — execution path without classifiable filename → other; no explicit lifecycle → unknown

## 7. Digest changes (rescan)

_None (first scan or content unchanged)._

## 8. Inferred supersession (not written to DB)

- `docs/roadmap/2026-07-30-stage1-plan-comparison.md` → None — soft pointer without path: Status: Historical comparison; current state moved to

## 9. Retirement / review buckets (report only)

### Explicit superseded (safe mark candidates)

- `docs/handoff/2026-08-01-pillow-controller-handoff.md`
- `docs/handoff/2026-08-07-pillow-controller-handoff.md`
- `docs/handoff/2026-08-08-round5-tech-debt-status.md`

### Briefs pending review (not retirement)

Count: 43

_Listed in §4; zero-inbound is never a retirement signal for briefs._

### Expected orphans (verification/decision) — 非问题


### Retirement candidates

Only documents with **explicit** supersession status. Zero-inbound alone never appears here.

- `docs/handoff/2026-08-01-pillow-controller-handoff.md`
- `docs/handoff/2026-08-07-pillow-controller-handoff.md`
- `docs/handoff/2026-08-08-round5-tech-debt-status.md`

---

## 10. Comparison with inventory audit (`2026-08-08-docs-inventory-audit.md`)

| Metric | Inventory audit | This K0b scan | Notes |
| --- | ---: | ---: | --- |
| Total `docs/**/*.md` | 294 | **296** | Corpus +2 since inventory (excludes this audit-run deliverable via `--exclude`) |
| `brief` | 115 | **115** | Match |
| `report` | 101 | **102** | +1 (`knowledge-registry-k0a-report.md` post-inventory) |
| `verification` | 26 | **26** | Match |
| `research` | 16 | **17** | +1 inventory audit file itself now in corpus |
| `decision` | 4 | **4** | Match |
| `handoff` | 4 | **4** | Match |
| `roadmap` / `migration` / `reference` | 2 / 1 / 1 | **2 / 1 / 1** | Match |
| `other` | 4 | **4** | Same four unclassifiable execution notes |
| `spec` + `plan` | 20 (all as `spec`) | **5 + 15 = 20** | K0a enum has `plan`; inventory folded plans into `spec` |
| Explicit `superseded` | 3 | **3** | Handoff chain only |
| Unpaired briefs | 43 | **43** | Match |
| Paired briefs → `consumed` | 72 (described) | **72** | Match |
| Zero-inbound total | 132 | **102** | Inventory + knowledge-layer research cite many basenames; +2 files also add refs. Method same (basename substring). |
| Zero-inbound `brief` | 101 | **101** | Match |
| Zero-inbound `verification` / `decision` | 5 / 1 | **0 / 0** | Those paths are now mentioned by inventory/research tables |
| `unknown` status | ~290 (no enum yet) | **214 (72.3%)** | Honest default; remainder are accepted/consumed/proposed/superseded only |

**Explicit supersession edges written (查证 only):**

1. `handoff/2026-08-07…` → `handoff/2026-08-01…` (1→1)
2. `handoff/2026-08-08…` → `handoff/2026-08-07…` **and** `handoff/2026-08-08-round5…` (1→2)
3. `execution/…-c3e-approval-ui-report.md` → free-text `prior delivery status` (pathless)

Soft roadmap pointer is **inferred only** (not in DB).

**Policy checks:**

- Zero-inbound is **not** listed as retirement.
- `verification` / `decision` orphans (if any) labeled **预期孤儿,非问题**.
- Unpaired briefs → **待复核**, not retirement.
- Retirement candidates = the 3 explicit-superseded handoffs only.

---

*Generated by `keeper-knowledge-indexer` (K0b).*
