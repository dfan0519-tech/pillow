# Pillow Wave C1 Final Acceptance

Date: 2026-07-31

Status: ACCEPTED

Controller baseline: `7b7f70f`

Commit and staging status: intentionally uncommitted; staging area empty

## Accepted Outcome

Wave C1 advances Pillow on both required product tracks:

1. The desktop opens as a restrained, dark, project-first AI workbench aligned
   with the pinned Grok App interaction baseline.
2. Provider-originated read requests now cross Pillow's governed B3 boundary,
   persist their lifecycle and bounded result, and project into the owning
   conversation without enabling arbitrary provider tools.

The legacy Keeper application, Obsidian integration, company orchestration,
account billing, model marketplaces, write tools, shell tools and network tools
remain outside this wave.

## Final Blocker Closure

| Blocker | Resolution | Result |
| --- | --- | --- |
| Provider-native permission names could bypass the B3 boundary | Unknown or non-Pillow tools fail closed; only `fs.list@1`, `fs.read@1` and `fs.search@1` reach the governed gateway | Closed |
| A terminal receipt could exist before its timeline projection | Startup reconciliation repairs only missing valid lifecycle suffixes from canonical terminal receipts | Closed |
| Failed, cancelled or expired reads appeared as successful empty output | Adapter returns explicit secret-safe RPC errors for failed, cancelled and deadline-exceeded outcomes | Closed |
| Raw JSON-RPC IDs were used as durable call identities | Calls use run-scoped `grok-acp:<run_id>:<ordinal>` identities, with same-run conflict rejection and cross-run isolation | Closed |
| Visual focus validation accepted transparent focus styling | Composer focus uses a visible focus token and the visual gate checks nontransparent color plus nonzero outline or shadow | Closed |

## Personnel And Controller Evaluation

| Worker | Package | Controller evaluation |
| --- | --- | --- |
| Chandrasekhar | Governed permission and public error semantics | Strong. Preserved the fixed-manifest authority boundary and added focused negative-path coverage. |
| Gibbs | Keyboard focus styling and visual assertion | Good and appropriately narrow. Fixed the accessibility regression without adding decorative UI. |
| Dirac | Provider request identity and replay semantics | Strong. The run-scoped registry closes both accidental replay and conflicting-ID ambiguity. |
| Sagan | Terminal receipt to timeline reconciliation | Strong. Repair is deterministic, suffix-only and does not re-execute tools or rewrite terminal ledger history. |
| Kierkegaard | Stale visual contract test update | Correctly identified a test contradiction rather than weakening the new focus requirement. |
| Aristotle | Independent final review | Accepted with no Critical, Important or Minor finding after focused reruns. |
| Controller | Architecture, integration, visual inspection and full gates | Accepted all packages after independent review and repository-wide verification. |

## Verification Evidence

Executed from `this repository`:

- `pnpm check`: passed;
  - workspace and source checks passed;
  - renderer typecheck passed;
  - Vitest: 11 files, 102 tests passed;
  - Rust workspace tests and doctests passed;
  - Clippy with `-D warnings` passed;
  - one live Grok ACP test remains intentionally opt-in and ignored.
- `pnpm --filter @pillow/desktop build:ui`: passed, 56 modules built.
- `pnpm --filter @pillow/desktop test:e2e`: 6 of 6 passed serially;
  - persistence-before-emit, cancellation and interrupted recovery;
  - real Tauri restart and SQLite conversation restore;
  - native project create, select and restore;
  - desktop readiness and process-tree cleanup;
  - Cargo wrapper signal cleanup;
  - Wave C1 geometry, focus and interaction acceptance.
- `cargo fmt --all -- --check`: passed.
- `git diff --check`: passed.
- Staged files: 0.
- Residual Pillow, Cargo, Rust, Node or Playwright processes: 0.

The visual gate generated nine deterministic captures for empty, selected
project and active conversation states at 1280x800, 1440x900 and 1920x1080.
The controller also inspected the representative selected-project capture at
1440x900. It has one quiet rail, a dominant conversation canvas, a stable
floating composer, no dashboard cards, no overlap and no mojibake.

## Residual Risk

1. The live Grok ACP integration test was not run because it is an explicit
   external-runtime opt-in. Fake-child and governed round-trip suites cover the
   protocol and persistence contract locally.
2. Startup reconciliation currently scans terminal receipts and their
   timelines. It is correct for the current scale, but should gain a projection
   watermark or repair queue before very large histories.
3. This acceptance does not authorize a commit. The complete C1 and B3 working
   tree remains available for user review as one unstaged integration set.

## Decision

Wave C1 is accepted for controller review and commit preparation. The next
product wave may build on this baseline without reopening the five closed
blockers unless new evidence appears.
