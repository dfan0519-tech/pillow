# Knowledge status view (registry-rendered)

Source: **knowledge document registry only** (roles, statuses, links, audit buckets).
This is a **view**, not a maintained handoff. No hand-written project narrative is inlined.

---

## 1. Statistics

- **Total registered:** 296
- **`unknown` status ratio (UNCLASSIFIED):** **72.3%** (214/296)
- ⚠️ **Majority of registry rows are status=`unknown` (未分类).** Do not treat path lists as known operational state.

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

| status | count | note |
| --- | ---: | --- |
| `accepted` | 4 |  |
| `consumed` | 72 |  |
| `proposed` | 3 |  |
| `superseded` | 3 |  |
| `unknown` | 214 | **未分类 / UNCLASSIFIED** |

## 2. Current effective documents

Definition (registry-only): status ∉ {`superseded`, `archived`}.
Rows with status=`unknown` are **未分类** — listed in §2.2, not mixed into known-status role groups.

### 2.1 By role (status classified — not `unknown`)

#### `brief` (73)

- `docs/execution/2026-07-31-wave-c2-agent-activity-timeline-brief.md` — status=`consumed`
- `docs/execution/2026-07-31-wave-c2a-resource-core-brief.md` — status=`consumed`
- `docs/execution/2026-07-31-wave-c2b-resource-bridge-brief.md` — status=`consumed`
- `docs/execution/2026-07-31-wave-c2c1-resource-panel-brief.md` — status=`consumed`
- `docs/execution/2026-07-31-wave-c2c2-agent-activity-brief.md` — status=`consumed`
- `docs/execution/2026-07-31-wave-c2c3-preview-brief.md` — status=`consumed`
- `docs/execution/2026-07-31-wave-c2d-resource-workbench-integration-brief.md` — status=`consumed`
- `docs/execution/2026-07-31-wave-c2e-activity-driver-brief.md` — status=`consumed`
- `docs/execution/2026-07-31-wave-c2e-cdp-driver-fix-brief.md` — status=`consumed`
- `docs/execution/2026-07-31-wave-c2e-controller-rejection-fix-brief.md` — status=`consumed`
- `docs/execution/2026-07-31-wave-c2e-orchestrator-brief.md` — status=`consumed`
- `docs/execution/2026-07-31-wave-c2e-resource-driver-brief.md` — status=`consumed`
- `docs/execution/2026-08-01-desktop-process-cleanup-safety-fix-brief.md` — status=`consumed`
- `docs/execution/2026-08-01-wave-c3-broad-gate-expiry-fixture-fix-brief.md` — status=`consumed`
- `docs/execution/2026-08-01-wave-c3-codex2-profile-snapshot-api-development-brief.md` — status=`consumed`
- `docs/execution/2026-08-01-wave-c3-grok4-expiry-fixture-development-brief.md` — status=`consumed`
- `docs/execution/2026-08-01-wave-c3a-protocol-policy-brief.md` — status=`consumed`
- `docs/execution/2026-08-01-wave-c3b-storage-ledger-brief.md` — status=`consumed`
- `docs/execution/2026-08-01-wave-c3c-write-executor-brief.md` — status=`consumed`
- `docs/execution/2026-08-01-wave-c3d-runtime-native-brief.md` — status=`consumed`
- `docs/execution/2026-08-01-wave-c3e-approval-ui-brief.md` — status=`consumed`
- `docs/execution/2026-08-01-wave-c3e-controller-rejection-fix-brief.md` — status=`proposed`
- `docs/execution/2026-08-01-wave-c3f-cancel-resolution-arbitration-fix-brief.md` — status=`consumed`
- `docs/execution/2026-08-01-wave-c3f-project-isolation-fix-brief.md` — status=`consumed`
- `docs/execution/2026-08-01-wave-c3f-provider-cancel-rpc-completion-fix-brief.md` — status=`consumed`
- `docs/execution/2026-08-01-wave-c3f-real-profile-snapshot-harness-fix-brief.md` — status=`consumed`
- `docs/execution/2026-08-01-wave-c3f-run-events-schema-fix-brief.md` — status=`consumed`
- `docs/execution/2026-08-02-wave-c3-codex2-cli-desktop-smoke-parent-fixture-development-brief.md` — status=`consumed`
- `docs/execution/2026-08-02-wave-c3-codex2-cli-run-cargo-selftest-development-brief.md` — status=`consumed`
- `docs/execution/2026-08-02-wave-c3-codex2-incomplete-descendant-age-development-brief.md` — status=`consumed`
- `docs/execution/2026-08-02-wave-c3-codex2-vctip-process-boundary-development-brief.md` — status=`consumed`
- `docs/execution/2026-08-02-wave-c4-d1-grok1-protocol-development-brief.md` — status=`consumed`
- `docs/execution/2026-08-02-wave-c4-d1-grok2-policy-development-brief.md` — status=`consumed`
- `docs/execution/2026-08-02-wave-c4-d1-grok3-ui-primitives-development-brief.md` — status=`consumed`
- `docs/execution/2026-08-02-wave-c4-d1-grok4-git-parser-development-brief.md` — status=`consumed`
- `docs/execution/2026-08-02-wave-c4-d1r-codex2-rust-integration-brief.md` — status=`consumed`
- `docs/execution/2026-08-02-wave-c4-d1r-grok3-argv-display-brief.md` — status=`consumed`
- `docs/execution/2026-08-02-wave-c4-d1r2-grok4-clippy-fix-brief.md` — status=`consumed`
- `docs/execution/2026-08-02-wave-c4-d1r3-grok3-renderer-grant-boundary-fix-brief.md` — status=`consumed`
- `docs/execution/2026-08-02-wave-c4-d2a-codex2-process-executor-development-brief.md` — status=`consumed`
- `docs/execution/2026-08-02-wave-c4-d2a-grok1-exec-ledger-development-brief.md` — status=`consumed`
- `docs/execution/2026-08-02-wave-c4-d2ar-codex2-process-order-repair-brief.md` — status=`consumed`
- `docs/execution/2026-08-02-wave-c4-d2ar-grok1-single-claim-repair-brief.md` — status=`consumed`
- `docs/execution/2026-08-03-wave-c4-d2b-grok2-governed-process-gateway-brief.md` — status=`consumed`
- `docs/execution/2026-08-03-wave-c4-d2b-grok4-hardened-git-runner-brief.md` — status=`consumed`
- `docs/execution/2026-08-03-wave-c4-d2br-grok2-process-boundary-repair-brief.md` — status=`consumed`
- `docs/execution/2026-08-03-wave-c4-d2br-grok4-git-containment-repair-brief.md` — status=`consumed`
- `docs/execution/2026-08-03-wave-c4-d2br2-grok2-sealed-authority-repair-brief.md` — status=`consumed`
- `docs/execution/2026-08-03-wave-c4-d2br2-grok4-metadata-no-follow-repair-brief.md` — status=`consumed`
- `docs/execution/2026-08-04-wave-c4-d3a-codex2-typed-native-ts-gateways-brief.md` — status=`consumed`
- `docs/execution/2026-08-04-wave-c4-d3a-grok1-runtime-host-process-brief.md` — status=`consumed`
- `docs/execution/2026-08-04-wave-c4-d3ar2a-codex2-format-repair-brief.md` — status=`consumed`
- `docs/execution/2026-08-04-wave-c4-d3ar2b-codex2-adapter-event-exhaustiveness-repair-brief.md` — status=`consumed`
- `docs/execution/2026-08-04-wave-c4-d3ar2c-grok4-desktop-event-integration-repair-brief.md` — status=`consumed`
- `docs/execution/2026-08-04-wave-c4-d3b1-codex2-native-process-bridge-brief.md` — status=`consumed`
- `docs/execution/2026-08-04-wave-c4-d3b1-codex2-portable-cwd-repair-brief.md` — status=`consumed`
- `docs/execution/2026-08-04-wave-c4-d3b1-grok4-approval-arbitration-brief.md` — status=`consumed`
- `docs/execution/2026-08-04-wave-c4-d3b1-grok4-process-event-projection-brief.md` — status=`consumed`
- `docs/execution/2026-08-04-wave-c4-d3b2-codex2-live-retained-brief.md` — status=`consumed`
- `docs/execution/2026-08-04-wave-c4-d3b2-grok2-process-activity-brief.md` — status=`consumed`
- `docs/execution/2026-08-04-wave-c4-d3b2-grok4-app-refresh-brief.md` — status=`consumed`
- `docs/execution/2026-08-04-wave-c4-d3b2-grok4-workbench-mount-brief.md` — status=`consumed`
- `docs/execution/2026-08-04-wave-c4-d3b2-grok4-workbench-repair-brief.md` — status=`consumed`
- `docs/execution/2026-08-05-wave-c4-d3b3-codex2-provider-process-roundtrip-brief.md` — status=`consumed`
- `docs/execution/2026-08-05-wave-c4-d3b3-codex2-write-regression-contract-repair-brief.md` — status=`consumed`
- `docs/execution/2026-08-05-wave-c4-d3b3-grok4-real-desktop-process-e2e-brief.md` — status=`consumed`
- `docs/execution/2026-08-05-wave-c4-d3b3-grok4-run-cargo-cleanup-race-brief.md` — status=`consumed`
- `docs/execution/2026-08-05-wave-c4-d3b3-grok4-visual-proof-repair-brief.md` — status=`consumed`
- `docs/execution/2026-08-05-wave-c4-d3b4-grok1-native-git-brief.md` — status=`consumed`
- `docs/execution/2026-08-05-wave-c4-d3b4-grok2-git-bridge-brief.md` — status=`consumed`
- `docs/execution/2026-08-05-wave-c4-d3b4-grok3-git-pane-brief.md` — status=`consumed`
- `docs/execution/2026-08-05-wave-c4-d3b4-grok4-git-workbench-brief.md` — status=`consumed`
- `docs/execution/2026-08-06-wave-c4-d3b4-grok4-real-desktop-git-e2e-brief.md` — status=`consumed`

#### `decision` (4)

- `docs/decisions/ADR-0001-clean-successor.md` — status=`accepted`
- `docs/decisions/ADR-0002-pillow-identity-boundary.md` — status=`accepted`
- `docs/decisions/ADR-0003-settings-authority-and-s0-scope.md` — status=`accepted`
- `docs/decisions/ADR-0004-knowledge-layer-actors-and-registration.md` — status=`accepted`

#### `report` (1)

- `docs/execution/2026-08-08-debt-b-lifecycle-events-report.md` — status=`proposed`

#### `spec` (1)

- `docs/superpowers/specs/2026-08-02-c4-governed-process-git-design.md` — status=`proposed`

### 2.2 Unclassified (`status=unknown`) — 未分类

**Count:** 214 (72.3% of registry)

> These paths are **registered but not lifecycle-classified**. Listing them is inventory, not a claim that they are current operational truth.

#### role=`brief` (unclassified status) (42)

- `docs/execution/2026-07-30-wave-b1-action-brief.md` — status=`unknown` **未分类**
- `docs/execution/2026-07-30-wave-b2-action-brief.md` — status=`unknown` **未分类**
- `docs/execution/2026-07-30-wave-b3-action-brief.md` — status=`unknown` **未分类**
- `docs/execution/2026-07-31-wave-c1-action-brief.md` — status=`unknown` **未分类**
- `docs/execution/2026-07-31-wave-c2-agent-activity-dependent-tests-fix-brief.md` — status=`unknown` **未分类**
- `docs/execution/2026-07-31-wave-c2-agent-activity-review-fix-brief.md` — status=`unknown` **未分类**
- `docs/execution/2026-07-31-wave-c2-agent-activity-review-fix2-brief.md` — status=`unknown` **未分类**
- `docs/execution/2026-07-31-wave-c2-agent-activity-test-warning-fix-brief.md` — status=`unknown` **未分类**
- `docs/execution/2026-07-31-wave-c2c3-preview-review-fix-brief.md` — status=`unknown` **未分类**
- `docs/execution/2026-07-31-wave-c2d-open-project-race-fix-brief.md` — status=`unknown` **未分类**
- `docs/execution/2026-07-31-wave-c2d-project-selection-race-fix-brief.md` — status=`unknown` **未分类**
- `docs/execution/2026-07-31-wave-c2d-resource-workbench-controller-fix-brief.md` — status=`unknown` **未分类**
- `docs/execution/2026-07-31-wave-c2d-stale-picker-registration-fix-brief.md` — status=`unknown` **未分类**
- `docs/execution/2026-08-01-wave-c3-codex2-final-review-brief.md` — status=`unknown` **未分类**
- `docs/execution/2026-08-01-wave-c3-research-grok1-grok-app-brief.md` — status=`unknown` **未分类**
- `docs/execution/2026-08-01-wave-c3-research-grok2-zbbody-brief.md` — status=`unknown` **未分类**
- `docs/execution/2026-08-01-wave-c3-research-grok3-security-brief.md` — status=`unknown` **未分类**
- `docs/execution/2026-08-01-wave-c3a-controller-rejection-fix-brief.md` — status=`unknown` **未分类**
- `docs/execution/2026-08-01-wave-c3b-controller-final-fix-brief.md` — status=`unknown` **未分类**
- `docs/execution/2026-08-01-wave-c3b-controller-rejection-fix-brief.md` — status=`unknown` **未分类**
- `docs/execution/2026-08-01-wave-c3c-controller-final-security-fix-brief.md` — status=`unknown` **未分类**
- `docs/execution/2026-08-01-wave-c3c-controller-rejection-fix-brief.md` — status=`unknown` **未分类**
- `docs/execution/2026-08-01-wave-c3d-controller-final-fix-brief.md` — status=`unknown` **未分类**
- `docs/execution/2026-08-01-wave-c3d-controller-rejection-fix-brief.md` — status=`unknown` **未分类**
- `docs/execution/2026-08-01-wave-c3e-controller-final-fix-brief.md` — status=`unknown` **未分类**
- `docs/execution/2026-08-01-wave-c3f-desktop-integration-brief.md` — status=`unknown` **未分类**
- `docs/execution/2026-08-01-wave-c3f-final-resume-after-provider-fix-brief.md` — status=`unknown` **未分类**
- `docs/execution/2026-08-01-wave-c3f-forced-logout-recovery-brief.md` — status=`unknown` **未分类**
- `docs/execution/2026-08-01-wave-c3f-native-path-privacy-controller-rejection-fix-brief.md` — status=`unknown` **未分类**
- `docs/execution/2026-08-01-wave-c3f-resume-after-product-fixes-brief.md` — status=`unknown` **未分类**
- `docs/execution/2026-08-01-wave-c3f-resume-after-schema-fix-brief.md` — status=`unknown` **未分类**
- `docs/execution/2026-08-01-wave-c3f-run-events-schema-controller-rejection-fix-brief.md` — status=`unknown` **未分类**
- `docs/execution/2026-08-02-wave-c4r-grok1-terminal-process-research-brief.md` — status=`unknown` **未分类**
- `docs/execution/2026-08-02-wave-c4r-grok2-git-governance-research-brief.md` — status=`unknown` **未分类**
- `docs/execution/2026-08-02-wave-c4r-grok3-interaction-research-brief.md` — status=`unknown` **未分类**
- `docs/execution/2026-08-02-wave-c4r-grok4-security-acceptance-research-brief.md` — status=`unknown` **未分类**
- `docs/execution/2026-08-02-wave-c4r-research-dispatch-brief.md` — status=`unknown` **未分类**
- `docs/execution/2026-08-04-wave-c4-d3ar-grok1-process-authority-events-output-brief.md` — status=`unknown` **未分类**
- `docs/execution/2026-08-04-wave-c4-d3ar2-grok1-live-output-lifecycle-brief.md` — status=`unknown` **未分类**
- `docs/execution/2026-08-04-wave-c4-d3ar2-grok2-process-event-durability-brief.md` — status=`unknown` **未分类**
- `docs/execution/2026-08-04-wave-c4-d3b-grok4-integration-research-brief.md` — status=`unknown` **未分类**
- `docs/execution/2026-08-05-wave-c4-d3b4-grok4-git-workbench-repair-brief.md` — status=`unknown` **未分类**

#### role=`handoff` (unclassified status) (1)

- `docs/handoff/2026-08-08-pillow-controller-handoff.md` — status=`unknown` **未分类**

#### role=`migration` (unclassified status) (1)

- `docs/migration/legacy-capability-inventory.md` — status=`unknown` **未分类**

#### role=`other` (unclassified status) (4)

- `docs/execution/2026-08-04-wave-c4-d3b1-exact-grok4-routing-approval.md` — status=`unknown` **未分类**
- `docs/execution/2026-08-07-wave-c4-d3b3-cancel-tree-diagnosis.md` — status=`unknown` **未分类**
- `docs/execution/2026-08-07-wave-c4-d3b3-cancel-tree-rootcause.md` — status=`unknown` **未分类**
- `docs/execution/2026-08-08-adapter-decoupling-proposal.md` — status=`unknown` **未分类**

#### role=`plan` (unclassified status) (15)

- `docs/execution/2026-07-30-wave-b2-implementation-plan.md` — status=`unknown` **未分类**
- `docs/execution/2026-07-30-wave-b2-live-hardening-plan.md` — status=`unknown` **未分类**
- `docs/superpowers/plans/2026-07-29-slice-a-desktop-foundation.md` — status=`unknown` **未分类**
- `docs/superpowers/plans/2026-07-30-slice-b1-conversation-kernel.md` — status=`unknown` **未分类**
- `docs/superpowers/plans/2026-07-31-c2-read-only-workbench-completion.md` — status=`unknown` **未分类**
- `docs/superpowers/plans/2026-07-31-c2e-isolation-restart-desktop-acceptance.md` — status=`unknown` **未分类**
- `docs/superpowers/plans/2026-08-01-c3-final-acceptance-checkpoint.md` — status=`unknown` **未分类**
- `docs/superpowers/plans/2026-08-01-c3-governed-write-implementation.md` — status=`unknown` **未分类**
- `docs/superpowers/plans/2026-08-02-c4-governed-process-git-implementation.md` — status=`unknown` **未分类**
- `docs/superpowers/plans/2026-08-02-c4-next-round-controller-orchestration.md` — status=`unknown` **未分类**
- `docs/superpowers/plans/2026-08-03-c4-d2br-d3-controller-orchestration.md` — status=`unknown` **未分类**
- `docs/superpowers/plans/2026-08-04-c4-d3b2-workbench-mounting.md` — status=`unknown` **未分类**
- `docs/superpowers/plans/2026-08-05-c4-d3b3-provider-process-roundtrip.md` — status=`unknown` **未分类**
- `docs/superpowers/plans/2026-08-05-c4-d3b3-real-desktop-process-acceptance.md` — status=`unknown` **未分类**
- `docs/superpowers/plans/2026-08-05-c4-git-changes-implementation.md` — status=`unknown` **未分类**

#### role=`reference` (unclassified status) (1)

- `docs/references/upstream-baseline.md` — status=`unknown` **未分类**

#### role=`report` (unclassified status) (101)

- `docs/execution/2026-07-31-wave-c2-agent-activity-timeline-report.md` — status=`unknown` **未分类**
- `docs/execution/2026-07-31-wave-c2a-resource-core-report.md` — status=`unknown` **未分类**
- `docs/execution/2026-07-31-wave-c2b-resource-bridge-report.md` — status=`unknown` **未分类**
- `docs/execution/2026-07-31-wave-c2c1-resource-panel-report.md` — status=`unknown` **未分类**
- `docs/execution/2026-07-31-wave-c2c2-agent-activity-report.md` — status=`unknown` **未分类**
- `docs/execution/2026-07-31-wave-c2c3-preview-report.md` — status=`unknown` **未分类**
- `docs/execution/2026-07-31-wave-c2d-resource-workbench-integration-report.md` — status=`unknown` **未分类**
- `docs/execution/2026-07-31-wave-c2e-activity-driver-report.md` — status=`unknown` **未分类**
- `docs/execution/2026-07-31-wave-c2e-cdp-driver-fix-report.md` — status=`unknown` **未分类**
- `docs/execution/2026-07-31-wave-c2e-controller-rejection-fix-report.md` — status=`unknown` **未分类**
- `docs/execution/2026-07-31-wave-c2e-orchestrator-report.md` — status=`unknown` **未分类**
- `docs/execution/2026-07-31-wave-c2e-resource-driver-report.md` — status=`unknown` **未分类**
- `docs/execution/2026-08-01-desktop-process-cleanup-safety-fix-report.md` — status=`unknown` **未分类**
- `docs/execution/2026-08-01-wave-c3-broad-gate-expiry-fixture-fix-report.md` — status=`unknown` **未分类**
- `docs/execution/2026-08-01-wave-c3-codex2-profile-snapshot-api-development-report.md` — status=`unknown` **未分类**
- `docs/execution/2026-08-01-wave-c3-grok4-expiry-fixture-development-report.md` — status=`unknown` **未分类**
- `docs/execution/2026-08-01-wave-c3-integration-report.md` — status=`unknown` **未分类**
- `docs/execution/2026-08-01-wave-c3a-protocol-policy-report.md` — status=`unknown` **未分类**
- `docs/execution/2026-08-01-wave-c3b-storage-ledger-report.md` — status=`unknown` **未分类**
- `docs/execution/2026-08-01-wave-c3c-write-executor-report.md` — status=`unknown` **未分类**
- `docs/execution/2026-08-01-wave-c3d-runtime-native-report.md` — status=`unknown` **未分类**
- `docs/execution/2026-08-01-wave-c3e-approval-ui-report.md` — status=`unknown` **未分类**
- `docs/execution/2026-08-01-wave-c3f-cancel-resolution-arbitration-fix-report.md` — status=`unknown` **未分类**
- `docs/execution/2026-08-01-wave-c3f-native-path-privacy-fix-report.md` — status=`unknown` **未分类**
- `docs/execution/2026-08-01-wave-c3f-project-isolation-fix-report.md` — status=`unknown` **未分类**
- `docs/execution/2026-08-01-wave-c3f-provider-cancel-rpc-completion-fix-report.md` — status=`unknown` **未分类**
- `docs/execution/2026-08-01-wave-c3f-real-profile-snapshot-harness-fix-report.md` — status=`unknown` **未分类**
- `docs/execution/2026-08-01-wave-c3f-run-events-schema-fix-report.md` — status=`unknown` **未分类**
- `docs/execution/2026-08-02-wave-c3-codex2-cli-desktop-smoke-parent-fixture-development-report.md` — status=`unknown` **未分类**
- `docs/execution/2026-08-02-wave-c3-codex2-cli-run-cargo-selftest-development-report.md` — status=`unknown` **未分类**
- `docs/execution/2026-08-02-wave-c3-codex2-incomplete-descendant-age-development-report.md` — status=`unknown` **未分类**
- `docs/execution/2026-08-02-wave-c3-codex2-vctip-process-boundary-development-report.md` — status=`unknown` **未分类**
- `docs/execution/2026-08-02-wave-c4-d1-grok1-protocol-development-report.md` — status=`unknown` **未分类**
- `docs/execution/2026-08-02-wave-c4-d1-grok2-policy-development-report.md` — status=`unknown` **未分类**
- `docs/execution/2026-08-02-wave-c4-d1-grok3-ui-primitives-development-report.md` — status=`unknown` **未分类**
- `docs/execution/2026-08-02-wave-c4-d1-grok4-git-parser-development-report.md` — status=`unknown` **未分类**
- `docs/execution/2026-08-02-wave-c4-d1r-codex2-rust-integration-report.md` — status=`unknown` **未分类**
- `docs/execution/2026-08-02-wave-c4-d1r-grok3-argv-display-report.md` — status=`unknown` **未分类**
- `docs/execution/2026-08-02-wave-c4-d1r2-grok4-clippy-fix-report.md` — status=`unknown` **未分类**
- `docs/execution/2026-08-02-wave-c4-d1r3-grok3-renderer-grant-boundary-fix-report.md` — status=`unknown` **未分类**
- `docs/execution/2026-08-02-wave-c4-d2a-codex2-process-executor-development-report.md` — status=`unknown` **未分类**
- `docs/execution/2026-08-02-wave-c4-d2a-grok1-exec-ledger-development-report.md` — status=`unknown` **未分类**
- `docs/execution/2026-08-02-wave-c4-d2ar-codex2-process-order-repair-report.md` — status=`unknown` **未分类**
- `docs/execution/2026-08-02-wave-c4-d2ar-grok1-single-claim-repair-report.md` — status=`unknown` **未分类**
- `docs/execution/2026-08-03-wave-c4-d2b-grok2-governed-process-gateway-report.md` — status=`unknown` **未分类**
- `docs/execution/2026-08-03-wave-c4-d2b-grok4-hardened-git-runner-report.md` — status=`unknown` **未分类**
- `docs/execution/2026-08-03-wave-c4-d2br-grok2-process-boundary-repair-report.md` — status=`unknown` **未分类**
- `docs/execution/2026-08-03-wave-c4-d2br-grok4-git-containment-repair-report.md` — status=`unknown` **未分类**
- `docs/execution/2026-08-03-wave-c4-d2br2-grok2-sealed-authority-repair-report.md` — status=`unknown` **未分类**
- `docs/execution/2026-08-03-wave-c4-d2br2-grok4-metadata-no-follow-repair-report.md` — status=`unknown` **未分类**
- `docs/execution/2026-08-04-wave-c4-d3a-codex2-typed-native-ts-gateways-report.md` — status=`unknown` **未分类**
- `docs/execution/2026-08-04-wave-c4-d3a-grok1-runtime-host-process-report.md` — status=`unknown` **未分类**
- `docs/execution/2026-08-04-wave-c4-d3ar-grok-process-authority-events-output-report.md` — status=`unknown` **未分类**
- `docs/execution/2026-08-04-wave-c4-d3ar2-codex2-live-output-lifecycle-report.md` — status=`unknown` **未分类**
- `docs/execution/2026-08-04-wave-c4-d3ar2-grok4-process-event-durability-report.md` — status=`unknown` **未分类**
- `docs/execution/2026-08-04-wave-c4-d3ar2a-codex2-format-repair-report.md` — status=`unknown` **未分类**
- `docs/execution/2026-08-04-wave-c4-d3ar2b-codex2-adapter-event-exhaustiveness-repair-report.md` — status=`unknown` **未分类**
- `docs/execution/2026-08-04-wave-c4-d3ar2c-grok4-desktop-event-integration-repair-report.md` — status=`unknown` **未分类**
- `docs/execution/2026-08-04-wave-c4-d3b1-codex2-native-process-bridge-report.md` — status=`unknown` **未分类**
- `docs/execution/2026-08-04-wave-c4-d3b1-codex2-portable-cwd-repair-report.md` — status=`unknown` **未分类**
- `docs/execution/2026-08-04-wave-c4-d3b1-grok4-approval-arbitration-report.md` — status=`unknown` **未分类**
- `docs/execution/2026-08-04-wave-c4-d3b1-grok4-process-event-projection-report.md` — status=`unknown` **未分类**
- `docs/execution/2026-08-04-wave-c4-d3b2-codex2-live-retained-report.md` — status=`unknown` **未分类**
- `docs/execution/2026-08-04-wave-c4-d3b2-grok2-process-activity-report.md` — status=`unknown` **未分类**
- `docs/execution/2026-08-04-wave-c4-d3b2-grok4-app-refresh-report.md` — status=`unknown` **未分类**
- `docs/execution/2026-08-04-wave-c4-d3b2-grok4-workbench-mount-report.md` — status=`unknown` **未分类**
- `docs/execution/2026-08-04-wave-c4-d3b2-grok4-workbench-repair-report.md` — status=`unknown` **未分类**
- `docs/execution/2026-08-05-wave-c4-d3b3-codex2-provider-process-roundtrip-report.md` — status=`unknown` **未分类**
- `docs/execution/2026-08-05-wave-c4-d3b3-codex2-write-regression-contract-repair-report.md` — status=`unknown` **未分类**
- `docs/execution/2026-08-05-wave-c4-d3b3-grok4-real-desktop-process-e2e-report.md` — status=`unknown` **未分类**
- `docs/execution/2026-08-05-wave-c4-d3b3-grok4-run-cargo-cleanup-race-report.md` — status=`unknown` **未分类**
- `docs/execution/2026-08-05-wave-c4-d3b3-grok4-visual-proof-repair-report.md` — status=`unknown` **未分类**
- `docs/execution/2026-08-05-wave-c4-d3b4-grok1-native-git-report.md` — status=`unknown` **未分类**
- `docs/execution/2026-08-05-wave-c4-d3b4-grok2-git-bridge-report.md` — status=`unknown` **未分类**
- `docs/execution/2026-08-05-wave-c4-d3b4-grok3-git-pane-report.md` — status=`unknown` **未分类**
- `docs/execution/2026-08-05-wave-c4-d3b4-grok4-git-workbench-report.md` — status=`unknown` **未分类**
- `docs/execution/2026-08-06-wave-c4-d3b4-grok4-real-desktop-git-e2e-report.md` — status=`unknown` **未分类**
- `docs/execution/2026-08-07-wave-c4-conversation-list-race-report.md` — status=`unknown` **未分类**
- `docs/execution/2026-08-07-wave-c4-d3b4-r4-pane-mode-report.md` — status=`unknown` **未分类**
- `docs/execution/2026-08-08-ambient-glass-report.md` — status=`unknown` **未分类**
- `docs/execution/2026-08-08-debt-a-pane-labels-report.md` — status=`unknown` **未分类**
- `docs/execution/2026-08-08-debt-b2-lifecycle-impl-report.md` — status=`unknown` **未分类**
- `docs/execution/2026-08-08-debt-c-fixture-hardening-report.md` — status=`unknown` **未分类**
- `docs/execution/2026-08-08-debt-d-red-capture-report.md` — status=`unknown` **未分类**
- `docs/execution/2026-08-08-dual-skins-report.md` — status=`unknown` **未分类**
- `docs/execution/2026-08-08-dual-theme-report.md` — status=`unknown` **未分类**
- `docs/execution/2026-08-08-flake-analysis-VERDICT.md` — status=`unknown` **未分类**
- `docs/execution/2026-08-08-flake-analysis-report.md` — status=`unknown` **未分类**
- `docs/execution/2026-08-08-harness-flake-fixes-report.md` — status=`unknown` **未分类**
- `docs/execution/2026-08-08-knowledge-registry-k0a-report.md` — status=`unknown` **未分类**
- `docs/execution/2026-08-08-logo-rerender-report.md` — status=`unknown` **未分类**
- `docs/execution/2026-08-08-portable-path-converge-report.md` — status=`unknown` **未分类**
- `docs/execution/2026-08-08-settings-s0a-report.md` — status=`unknown` **未分类**
- `docs/execution/2026-08-08-settings-s0b-report.md` — status=`unknown` **未分类**
- `docs/execution/2026-08-08-settings-selfbrick-fix-report.md` — status=`unknown` **未分类**
- `docs/execution/2026-08-08-step1a-contract-move-report.md` — status=`unknown` **未分类**
- `docs/execution/2026-08-08-step1b-path-rule-report.md` — status=`unknown` **未分类**
- `docs/execution/2026-08-08-step1c-gate-hardening-report.md` — status=`unknown` **未分类**
- `docs/execution/2026-08-08-step1d-brand-glass-report.md` — status=`unknown` **未分类**
- `docs/execution/2026-08-08-storage-barrier-verdict.md` — status=`unknown` **未分类**
- `docs/execution/2026-08-08-theme-persistence-report.md` — status=`unknown` **未分类**

#### role=`research` (unclassified status) (17)

- `docs/execution/2026-08-08-ui-premium-research.md` — status=`unknown` **未分类**
- `docs/execution/2026-08-08-upstream-grokbuild-audit.md` — status=`unknown` **未分类**
- `docs/research/2026-07-30-grok-acp-protocol.md` — status=`unknown` **未分类**
- `docs/research/2026-07-30-grok-adapter-boundary.md` — status=`unknown` **未分类**
- `docs/research/2026-07-30-grok-security-acceptance.md` — status=`unknown` **未分类**
- `docs/research/2026-07-30-zbbody-reference-analysis.md` — status=`unknown` **未分类**
- `docs/research/2026-08-01-c3-governed-write-security-research.md` — status=`unknown` **未分类**
- `docs/research/2026-08-01-c3-grok-app-interaction-research.md` — status=`unknown` **未分类**
- `docs/research/2026-08-01-c3-zbbody-interaction-research.md` — status=`unknown` **未分类**
- `docs/research/2026-08-02-c4-git-governance-research.md` — status=`unknown` **未分类**
- `docs/research/2026-08-02-c4-security-acceptance-research.md` — status=`unknown` **未分类**
- `docs/research/2026-08-02-c4-terminal-git-interaction-research.md` — status=`unknown` **未分类**
- `docs/research/2026-08-02-c4-terminal-process-research.md` — status=`unknown` **未分类**
- `docs/research/2026-08-04-c4-d3b-grok4-process-ui-integration-map.md` — status=`unknown` **未分类**
- `docs/research/2026-08-08-desktop-delivery-research.md` — status=`unknown` **未分类**
- `docs/research/2026-08-08-docs-inventory-audit.md` — status=`unknown` **未分类**
- `docs/research/2026-08-08-knowledge-layer-research.md` — status=`unknown` **未分类**

#### role=`roadmap` (unclassified status) (2)

- `docs/roadmap/2026-07-30-stage1-plan-comparison.md` — status=`unknown` **未分类**
- `docs/roadmap/2026-07-31-stage1-overall-status.md` — status=`unknown` **未分类**

#### role=`spec` (unclassified status) (4)

- `docs/superpowers/specs/2026-07-29-pillow-first-design.md` — status=`unknown` **未分类**
- `docs/superpowers/specs/2026-08-01-c3-governed-write-design.md` — status=`unknown` **未分类**
- `docs/superpowers/specs/2026-08-05-c4-git-changes-design.md` — status=`unknown` **未分类**
- `docs/superpowers/specs/2026-08-08-settings-credentials-design.md` — status=`unknown` **未分类**

#### role=`verification` (unclassified status) (26)

- `docs/verification/2026-07-29-slice-a-acceptance.md` — status=`unknown` **未分类**
- `docs/verification/2026-07-30-slice-b1-acceptance.md` — status=`unknown` **未分类**
- `docs/verification/2026-07-30-slice-b2-acceptance.md` — status=`unknown` **未分类**
- `docs/verification/2026-07-30-wave-b2-h3-native-acceptance.md` — status=`unknown` **未分类**
- `docs/verification/2026-07-30-wave-b3-foundation-acceptance.md` — status=`unknown` **未分类**
- `docs/verification/2026-07-31-wave-b3-read-execution-acceptance.md` — status=`unknown` **未分类**
- `docs/verification/2026-07-31-wave-c1-acceptance.md` — status=`unknown` **未分类**
- `docs/verification/2026-07-31-wave-c1-checkpoint.md` — status=`unknown` **未分类**
- `docs/verification/2026-07-31-wave-c1-visual-acceptance.md` — status=`unknown` **未分类**
- `docs/verification/2026-08-01-wave-c2-acceptance.md` — status=`unknown` **未分类**
- `docs/verification/2026-08-01-wave-c2-checkpoint.md` — status=`unknown` **未分类**
- `docs/verification/2026-08-01-wave-c3c-acceptance.md` — status=`unknown` **未分类**
- `docs/verification/2026-08-01-wave-c3d-acceptance.md` — status=`unknown` **未分类**
- `docs/verification/2026-08-01-wave-c3e-acceptance.md` — status=`unknown` **未分类**
- `docs/verification/2026-08-01-wave-c3f-cancel-resolution-fix-acceptance.md` — status=`unknown` **未分类**
- `docs/verification/2026-08-01-wave-c3f-native-path-privacy-fix-acceptance.md` — status=`unknown` **未分类**
- `docs/verification/2026-08-01-wave-c3f-provider-cancel-rpc-fix-acceptance.md` — status=`unknown` **未分类**
- `docs/verification/2026-08-01-wave-c3f-run-events-schema-fix-acceptance.md` — status=`unknown` **未分类**
- `docs/verification/2026-08-02-wave-c3-acceptance.md` — status=`unknown` **未分类**
- `docs/verification/2026-08-02-wave-c3-checkpoint.md` — status=`unknown` **未分类**
- `docs/verification/2026-08-04-wave-c4-d3a-controller-review.md` — status=`unknown` **未分类**
- `docs/verification/2026-08-04-wave-c4-d3ar2-acceptance.md` — status=`unknown` **未分类**
- `docs/verification/2026-08-04-wave-c4-d3b1-acceptance.md` — status=`unknown` **未分类**
- `docs/verification/2026-08-07-wave-c4-checkpoint.md` — status=`unknown` **未分类**
- `docs/verification/2026-08-07-wave-c4-d3b4-acceptance.md` — status=`unknown` **未分类**
- `docs/verification/worker-gate-entry.md` — status=`unknown` **未分类**

### 2.3 Superseded / archived (not current)

**Count:** 3

- `docs/handoff/2026-08-01-pillow-controller-handoff.md` — status=`superseded` role=`handoff`
- `docs/handoff/2026-08-07-pillow-controller-handoff.md` — status=`superseded` role=`handoff`
- `docs/handoff/2026-08-08-round5-tech-debt-status.md` — status=`superseded` role=`handoff`

## 3. Supersession chains

Edges from registry `supersedes` links only (explicit/查证). Free-path claims have no resolved `to_document_id`.

| from | arity | targets |
| --- | ---: | --- |
| `docs/execution/2026-08-01-wave-c3e-approval-ui-report.md` | 1 (1→1) | **free-path:** `prior delivery status` |
| `docs/handoff/2026-08-07-pillow-controller-handoff.md` | 1 (1→1) | `docs/handoff/2026-08-01-pillow-controller-handoff.md` |
| `docs/handoff/2026-08-08-pillow-controller-handoff.md` | 2 (1→N) | `docs/handoff/2026-08-07-pillow-controller-handoff.md`; `docs/handoff/2026-08-08-round5-tech-debt-status.md` |

### Free-path / unresolved claims

- `docs/execution/2026-08-01-wave-c3e-approval-ui-report.md` → free-path claim: **prior delivery status** (no resolved document id)

## 4. Contradictions

Multiple non-superseded handoff/roadmap/migration docs in the same directory (audit §1).

- **docs/roadmap** — 2 non-superseded roadmap docs: docs/roadmap/2026-07-30-stage1-plan-comparison.md, docs/roadmap/2026-07-31-stage1-overall-status.md

## 5. Pending review (待复核)

Unpaired briefs (no matching report stem). **Review only — not retirement.**

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

## 6. Expected orphans (预期孤儿, 非问题)

`verification` / `decision` zero-inbound (or bucketed as expected). **Not a todo. Not retirement.**

**Count:** 0

_None in the expected-orphan bucket for this audit (may be zero-inbound empty or cited by other docs)._

### Separation from todos

_Expected orphans are not listed under §5 待复核. They are non-problems only._

---

*Generated by `keeper-knowledge-indexer` render (K0c). Pure registry projection.*
