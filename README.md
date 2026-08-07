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
**C4 governed process and read-only Git are controller-accepted through D3B4.**
D1 through D3B1 established fixed process contracts and policy, supervised
bounded execution, one-shot durable proposal authority, hardened read-only Git
parsing, real bounded process output, durable process lifecycle events, native
process commands, strict renderer projection, and a deterministic single-bar
arbitration model. D3B2 mounted process activity, approval and the terminal
output pane; D3B3 closed the provider-to-governed-process desktop loop; D3B4
added read-only Git Changes as an explicitly selected mode of the contextual
pane, with native `GitCommandState` authority, a strict typed bridge, and
project/generation isolation. Pillow can discover repositories below the active
project, show branch/HEAD and a bounded status list, and open one bounded diff.
It cannot stage, commit, checkout, reset, discard, mutate configuration, reach a
remote, or expose a terminal.

The fresh D3B4 Round 4 controller gate passed `pnpm check` (renderer 370/370,
Rust 731, Clippy with warnings denied), the production build, and three
consecutive 10/10 real serial desktop E2E runs. That gate also found and repaired a Critical
regression in which Git Changes displaced the accepted C2 resource browser, and
an Important defect in which a single untracked directory failed whole-repository
Git status. Evidence:
`docs/verification/2026-08-07-wave-c4-d3b4-acceptance.md`.

**C4 is checkpointed** through D3B4. One intermittent D3B3 scenario has an
unproven root cause and is recorded as an open risk in the checkpoint, not
closed. Interactive shell and Git mutation have not been started. Evidence: `docs/verification/2026-08-07-wave-c4-checkpoint.md`.

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
- `docs/verification/2026-08-07-wave-c4-d3b4-acceptance.md`
- `docs/verification/2026-08-07-wave-c4-checkpoint.md`
- `docs/roadmap/2026-07-31-stage1-overall-status.md`
- `docs/superpowers/plans/2026-07-31-c2-read-only-workbench-completion.md`
- `docs/superpowers/plans/2026-08-01-c3-governed-write-implementation.md`
