# Pillow

Pillow is a local-first, project-scoped AI development environment.

The product roadmap is intentionally ordered:

1. build a reliable industrial AI IDE;
2. connect the IDE to a governed Obsidian knowledge layer;
3. extend the project model into a one-person-company operating system.

The legacy Keeper repository at `<workspace>\VaultSystem\keeper` remains an
independent resource manager for shared knowledge, credentials, project
registration, and Agent coordination. Pillow consumes those capabilities only
through explicit interfaces; all new IDE product code belongs in this
repository.

## Current Status

**Slice B1 (conversation kernel) is accepted.** The desktop shell owns a
project-scoped conversation service with durable SQLite authority, a
provider-neutral runtime host, and the deterministic fake adapter path. Every
runtime event follows `validate -> persist -> commit -> emit`.

**Slice B2 (real Grok ACP adapter) is accepted.** The controlled live gate
proved plaintext Grok ACP execution, cancellation cleanup, provider-tool
shutdown, and public-error redaction behind the B1 protocol boundary.

**Wave C1 is accepted as one governed, provider-to-B3 read-only loop.** Pillow
owns project-scoped path authority, strict read-tool contracts, bounded
`fs.list@1`, `fs.read@1`, and `fs.search@1` execution, SQLite call leases,
durable lifecycle history, exact terminal receipt replay, and the controlled
provider proposal-to-timeline projection. Unknown provider tools fail closed.

**C2 read-only workbench is accepted.**
The desktop now provides a project-scoped tree, bounded search, recent files,
text/Markdown/image/basic-artifact preview, conversation-owned Agent activity,
two-project isolation, and restart recovery. Agent activity uses real persisted
run/event/tool/sub-agent/artifact data and progressive disclosure. Raw
provider-controlled tool output is fail-closed in the activity DOM; governed
resource/artifact previews and structured test results are the supported detail
channels. The final controller gate passed all 206 desktop Vitest cases, the
Rust workspace/doctests and Clippy `-D warnings`, the production UI build, and
all 7 serial desktop E2E cases. Independent Codex2 review accepted the final
tree with no Critical or Important findings.

**C3 governed write/diff/approval is accepted.** Pillow now closes one bounded
provider-to-human-to-host UTF-8
create/modify loop with a host-built diff, mandatory approve-once/reject,
base-hash compare-and-swap, atomic apply, metadata-only lifecycle receipts,
project isolation and restart invalidation. The controller accepted all eight
real Tauri/WebView2 adversarial scenarios, the 246-test renderer suite, Rust
workspace/doctests and Clippy, the production build, and all 8 serial desktop
E2E cases. Arbitrary provider-native/MCP tools, terminal execution, Git,
network tools, credentials, Obsidian and additional runtimes remain disabled.
**C4 governed process and read-only Git foundations are active.** D1 through
D3AR2 are controller-accepted: Pillow has fixed process contracts and policy,
supervised bounded execution, one-shot durable proposal authority, hardened
read-only Git parsing, real bounded process output, durable process lifecycle
events, and typed desktop gateway foundations. The fresh checkpoint gate passed
272/272 desktop tests, the Rust workspace and doctests, Clippy with warnings
denied, the production build, and 8/8 real desktop E2E scenarios. D3B process
governance UI integration is the next design and implementation boundary.

## Design Authority

- `docs/superpowers/specs/2026-07-29-pillow-first-design.md`
- `docs/migration/legacy-capability-inventory.md`
- `docs/references/upstream-baseline.md`
- `docs/decisions/ADR-0001-clean-successor.md`
- `docs/verification/2026-07-31-wave-b3-read-execution-acceptance.md`
- `docs/verification/2026-07-31-wave-c1-checkpoint.md`
- `docs/verification/2026-08-01-wave-c2-acceptance.md`
- `docs/verification/2026-08-01-wave-c2-checkpoint.md`
- `docs/verification/2026-08-01-wave-c3c-acceptance.md`
- `docs/verification/2026-08-01-wave-c3d-acceptance.md`
- `docs/verification/2026-08-02-wave-c3-acceptance.md`
- `docs/verification/2026-08-02-wave-c3-checkpoint.md`
- `docs/verification/2026-08-04-wave-c4-d3ar2-acceptance.md`
- `docs/roadmap/2026-07-31-stage1-overall-status.md`
- `docs/superpowers/plans/2026-07-31-c2-read-only-workbench-completion.md`
- `docs/superpowers/plans/2026-08-01-c3-governed-write-implementation.md`
