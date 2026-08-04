# Pillow Wave C1 Action Brief

Date: 2026-07-31

Status: Controller-approved execution brief

## Goal

Advance Pillow as an industrial local AI IDE on two co-primary tracks:

1. Replace the generic light workbench styling with a faithful, Pillow-branded
   adaptation of the pinned Grok App desktop shell.
2. Connect one provider-originated governed read-tool request to the accepted B3
   execution kernel and persist the bounded result in the conversation timeline.

This wave does not add Obsidian, company orchestration, account billing, model
marketplaces, or legacy Keeper runtime dependencies.

## Product Decision

The Grok App screenshots and pinned source are the interaction baseline, not
loose inspiration. Pillow keeps its own project, conversation, approval and
runtime semantics, but the visible shell must use the upstream desktop grammar:

- matte dark workbench as the default C1 theme;
- one quiet left rail for product navigation, projects and conversations;
- broad, uncluttered conversation canvas;
- compact native title bar and window controls;
- floating bottom composer with project, runtime and permission context;
- restrained borders, radii and shadows;
- secondary runtime metadata through progressive disclosure;
- no dashboard cards, marketing hero, oversized empty-state copy or decorative
  effects.

The upstream repository is read-only:

`<workspace>\VaultSystem\references\grok-app`

Pinned commit:

`cbbbdf17bb8db1588b6707f3f42f6b00397e27f9`

MIT attribution in `THIRD_PARTY_NOTICES.md` must remain accurate. Grok names,
logos, account behavior and provider-specific domain state must not be copied
into Pillow.

## Track A: Desktop Shell Alignment

### Required behavior

- Launch directly into the main workbench.
- Empty state keeps project selection and project creation in the left rail and
  composer context, never in a blocking launcher.
- Projects contain their own conversation lists.
- The active project and active conversation are visually unambiguous without a
  saturated primary-color block.
- The conversation canvas remains dominant at 1280x800, 1440x900 and
  1920x1080.
- Existing timeline, run activity, resource pane, cancellation and recovery
  behavior remain functional.
- Agent collaboration appears inline inside the owning conversation rather than
  as a separate dashboard.
- Default dark tokens follow the pinned Grok App values closely. A future light
  theme may reuse the same semantic tokens but is not the C1 source of truth.
- Chinese text renders correctly; mojibake is a release blocker.

### Write ownership

The UI implementer owns:

- `apps/desktop/src/App.tsx`
- `apps/desktop/src/components/**`
- `apps/desktop/src/features/projects/**`
- `apps/desktop/src/features/workbench/**`
- `apps/desktop/src/features/conversations/**`
- `apps/desktop/src/styles/**`
- renderer unit tests except `apps/desktop/tests/e2e/**`

The UI implementer must not edit Rust, Cargo, SQLite, runtime adapters, E2E
files, Keeper, or the pinned upstream checkout.

## Track B: Provider Governed Read Round Trip

### Required behavior

Implement one deterministic provider-originated tool-call path:

```text
provider normalized tool proposal
  -> manifest/version validation
  -> B3 governed read gateway
  -> project-root enforcement
  -> durable tool-call lifecycle
  -> bounded ToolResult
  -> normalized conversation event
  -> restored timeline projection
```

Only the accepted read manifests are allowed:

- `fs.list@1`
- `fs.read@1`
- `fs.search@1`

Provider-native arbitrary tools remain disabled. No shell command, write,
network, credential, Git mutation or dynamic manifest is in scope. Duplicate
provider delivery must replay the exact terminal receipt without re-executing.
Cancellation, timeout and interrupted recovery must remain explicit.

### Write ownership

The runtime implementer owns:

- `crates/adapter-grok-acp/**`
- `crates/runtime-host/**`
- `apps/desktop/src-tauri/src/conversation_service/**`
- focused Rust tests under those areas

The runtime implementer may consume but must not rewrite:

- `crates/agent-protocol/**`
- `crates/tool-executor/**`
- `crates/storage-sqlite/**`

The current uncommitted B3 working tree at `<workspace>\pillow` is authoritative.
No B3 file may be reverted or replaced.

## Track C: Desktop Visual And Interaction Gate

### Required behavior

- Add deterministic desktop captures for empty, selected-project and active
  conversation states.
- Validate supported Windows viewport geometry.
- Check sidebar, timeline and composer for overlap, clipping and scroll traps.
- Verify keyboard focus, labels and reduced-motion behavior.
- Record a concise side-by-side acceptance matrix against the three supplied
  Grok App references and the pinned source.
- Fail the gate when the product still reads as a generic admin dashboard.

### Write ownership

The acceptance implementer owns:

- `apps/desktop/tests/e2e/**`
- screenshot and visual-test scripts under `scripts/**`
- `docs/verification/2026-07-31-wave-c1-visual-acceptance.md`

The acceptance implementer must not edit production renderer or Rust source.

## Shared Constraints

- Do not stage or commit.
- Do not modify `<workspace>\VaultSystem\keeper`.
- Do not modify shared vault rules, credentials or project metadata.
- Do not add a UI framework or icon dependency without controller approval.
- Preserve existing project-first ownership and durable conversation behavior.
- Keep implementation files focused and under the repository size policy.
- Report changed files, tests run, failures and residual risks.

## Acceptance Gates

1. Renderer typecheck and focused Vitest suites pass.
2. Rust focused tests pass for the provider round trip.
3. Workspace `pnpm check` passes.
4. Production renderer build passes.
5. Desktop E2E passes at supported Windows sizes.
6. Screenshots show a Grok-aligned Pillow shell with no overlap or mojibake.
7. Provider tool request is persisted, bounded, replayable and project-scoped.
8. No residual Pillow, Cargo or worker process remains after verification.
9. Independent controller review finds no Critical or Important issue.

## Personnel

| Role | Work package | Evaluation focus |
| --- | --- | --- |
| UI implementer | Grok-aligned Pillow desktop shell | Fidelity, interaction hierarchy, no regression |
| Runtime implementer | Provider-to-B3 governed read loop | Authority, idempotency, recovery, bounded output |
| Acceptance implementer | Visual and desktop interaction gate | Reproducible evidence, geometry, accessibility |
| Controller | Architecture, integration, review and final acceptance | Scope, cross-track contracts, industrial quality |
