# Legacy Keeper Capability Inventory

Date: 2026-07-29

Source repository: legacy Keeper (out of tree; not published with Pillow)

Source baseline: `340d62f`

The inventory classifies behavior, not files. A `PORT` decision means the new
repository recreates the capability behind a new interface and imports only
the tests or algorithms that remain valid.

## Port

| Capability | Legacy evidence | Successor boundary |
| --- | --- | --- |
| Project identity | `lib/project-identity.js` | `crates/kernel-project` stable project/workspace identity |
| Conversation lifecycle | `lib/keeper-conversation.js` | `crates/kernel-session` conversation, turn, interruption, recovery |
| Human approval authority | `lib/operator-approval-authority.js` | `crates/kernel-policy` signed, expiring, replay-resistant decisions |
| Agent run safety | `lib/agent-run-safety.js` | `crates/runtime-host` launch validation and redaction |
| Runtime adapters | `lib/agent-runtime-adapters.js` | `crates/agent-protocol` plus one adapter crate per runtime |
| Resource health ledger | `lib/resource-ledger.js` | `crates/kernel-resource` public health and routing evidence |
| Secret separation | `lib/credential-store.js` | OS keychain service plus opaque secret references |
| Persistence ownership | `lib/persistence-authority.js` | transaction service and append-only audit writer |
| Completion evidence | `lib/agent-completion-report.js` | typed run result, artifact, test, and handoff receipts |
| Runtime quota evidence | `lib/quota-observations.js` | evidence observations with expiry and confidence |
| Project roles | `lib/project-role-assignments.js` | optional IDE team bindings after the base runtime is stable |

## Rewrite

| Capability | Reason |
| --- | --- |
| Agent Runner | Process lifecycle behavior is valuable, but the implementation is coupled to JSON state, legacy runtime names, and Keeper prompt packets. |
| Project intake | It mixes discovery, classification, identity creation, and status projection. The IDE needs explicit workspace onboarding. |
| Key and model catalog | It combines inventory, provider heuristics, testing, and UI concerns. Connections will become provider plugins backed by keychain references. |
| Keeper HTTP API | Routes grew around historical UI needs. The successor exposes a versioned typed command/event API. |
| Storage | JSON, backup pairs, shadow SQLite, and session indexes represent competing authorities. The successor starts with one SQLite authority. |
| Desktop integration | Electron-specific launch and static-server behavior is replaced by Tauri commands and a Rust host. |
| Knowledge APIs | Deferred to the Obsidian stage and rebuilt as a separate knowledge port. |

## Do Not Migrate

- `server.js`
- `public/app.js`
- `public/style.css`
- legacy dashboards and cinematic particle effects
- dashboard-derived project status
- flat JSON as an operational authority
- broad compatibility aliases and duplicate endpoint families
- automatic filesystem classification
- automatic rule-file injection
- automatic mutation of external projects
- provider-specific secrets in application configuration
- historical smoke tasks and synthetic conversation records
- UI representations of internal leases, process slots, and raw route codes

## Test Migration

Legacy tests are evidence, not a suite to copy wholesale.

Tests should be selected in this order:

1. security and secret-redaction invariants;
2. project and conversation identity invariants;
3. approval replay and expiry behavior;
4. runtime process cancellation, timeout, and recovery;
5. resource-health routing behavior;
6. import compatibility fixtures.

Renderer snapshot tests, route-compatibility tests, and tests asserting legacy
JSON shape are not migration targets.

## Freeze Rule

Until the successor imports a legacy backup and passes parity checks:

- keep the old repository and service intact;
- accept only critical security or data-loss fixes in the old repository;
- do not add product features or redesign its UI;
- do not delete legacy state, credentials, vault data, or audit records.

