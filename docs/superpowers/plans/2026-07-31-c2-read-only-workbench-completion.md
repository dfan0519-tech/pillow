# C2 Read-Only Workbench Completion Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use
> superpowers:subagent-driven-development (recommended) or
> superpowers:executing-plans to implement this plan task-by-task. Steps use
> checkbox (`- [ ]`) syntax for tracking.

**Goal:** Turn the accepted C2 backend and unmounted renderer slices into one
project-isolated, restart-safe, read-only resource workbench.

**Final status (2026-08-01):** Completed and accepted as an unstaged,
uncommitted working-tree checkpoint. See
`docs/verification/2026-08-01-wave-c2-acceptance.md` and
`docs/verification/2026-08-01-wave-c2-checkpoint.md`.

**Architecture:** Keep `keeper-resource` and SQLite authoritative below Tauri.
The renderer consumes only typed, bounded DTOs. Resource browsing and preview
live in one collapsible contextual panel; conversation activity is projected
from the existing real `RunActivityView` for the owning conversation.

**Tech Stack:** Rust 2024, Tauri 2, rusqlite, React 19, TypeScript, Vitest,
Node desktop E2E.

## Global Constraints

- Work only in `this repository`; never edit `legacy Keeper (out of tree)`.
- C2 remains read-only: no file mutation, terminal, Git, network, credentials,
  Obsidian, MCP, or runtime expansion.
- Every operation requires `projectId`; no cwd or last-project fallback.
- Renderer code cannot read native files, SQLite, secrets, or machine roots.
- Preserve the C1 fixed-manifest and receipt boundaries.
- Tests are written and run RED before production implementation.
- Worker write whitelists must not overlap.
- No staging or commit without later explicit user authorization.

---

### Task 1: Bounded preview transport and renderer

**Status:** Accepted on 2026-07-31. Controller gates: Rust 9/9, Vitest 18/18,
typecheck, fmt, and diff. Codex2 found one Important image-DTO validation gap;
the targeted fix passed re-review with no remaining findings. The preview is
mounted in the workbench through accepted Task 2.

**Files:**

- Modify: `Cargo.toml`
- Modify: `Cargo.lock`
- Modify: `apps/desktop/src-tauri/Cargo.toml`
- Modify: `apps/desktop/src-tauri/src/lib.rs`
- Modify: `apps/desktop/src-tauri/src/resource_commands.rs`
- Modify: `apps/desktop/src-tauri/src/resource_commands_tests.rs`
- Modify: `apps/desktop/src/lib/resourceGateway.ts`
- Modify: `apps/desktop/src/lib/tauriResourceGateway.ts`
- Create: `apps/desktop/src/features/resources/resourcePreviewModel.ts`
- Create: `apps/desktop/src/features/resources/ResourcePreview.tsx`
- Create: `apps/desktop/src/styles/resource-preview.css`
- Create: `apps/desktop/tests/resource-preview.test.tsx`

**Interfaces:**

- Consumes:
  `ResourceReader::read(ProjectResourceCapability, ResourceReadRequest)`.
- Produces:

```ts
export type ResourcePreview = {
  path: string;
  kind: "text" | "markdown" | "image" | "binary";
  mediaType: string;
  content?: string;
  dataUrl?: string;
  bytesReturned: number;
  truncated: boolean;
};

previewProjectResource(input: {
  projectId: string;
  relativePath: string;
}): Promise<ResourcePreview>;
```

- Image transport is allowed only for complete, non-truncated PNG, JPEG, GIF,
  or WebP payloads within the existing 1 MiB read cap. The host returns one
  bounded `dataUrl`; raw byte arrays never enter renderer state.
- UTF-8 `.md` files use `kind: "markdown"`; other UTF-8 files use `"text"`.
- Unsupported/truncated binary uses `"binary"` with metadata only.

- [x] **Step 1: Write failing host and renderer tests**

Add cases proving project-id isolation, allow-listed MIME detection, 1 MiB
bound, no raw `bytes`, text/Markdown rendering, image `dataUrl`, unsupported
binary metadata, and machine-path redaction.

- [x] **Step 2: Run tests and record RED**

```powershell
cargo test -p pillow-desktop resource_preview
pnpm --filter @pillow/desktop exec vitest run tests/resource-preview.test.tsx
```

Expected: Rust symbols and `ResourcePreview` modules are missing.

- [x] **Step 3: Implement the minimal preview command and view**

The Tauri mapper must follow this decision:

```rust
match (extension, file.content_kind, file.truncated) {
    ("md", ResourceContentKind::Utf8, _) => PreviewKind::Markdown,
    (_, ResourceContentKind::Utf8, _) => PreviewKind::Text,
    ("png" | "jpg" | "jpeg" | "gif" | "webp",
     ResourceContentKind::Binary, false) => PreviewKind::Image,
    _ => PreviewKind::Binary,
}
```

`ResourcePreview.tsx` renders `<pre>` for text, semantic headings/code blocks
from a non-HTML Markdown model, `<img>` for accepted data URLs, and metadata for
unsupported binary. Do not use `dangerouslySetInnerHTML`.

- [x] **Step 4: Run GREEN gates**

```powershell
cargo test -p pillow-desktop resource_preview
pnpm --filter @pillow/desktop exec vitest run tests/resource-preview.test.tsx
pnpm --filter @pillow/desktop typecheck
cargo fmt --all -- --check
git diff --check
```

Expected: all commands exit 0.

### Task 2: Mount resource browsing and preview in the workbench

**Status:** Accepted on 2026-07-31. Controller acceptance: adversarial project
activation/stale-picker tests 10/10, complete six-suite C2-D regression 100/100,
typecheck, production UI build (65 modules), and diff check. Codex2 found two
Important last-request-wins gaps; both were fixed and the final re-review was
clean (Critical 0 / Important 0 / Minor 0). Nothing staged or committed.

**Files:**

- Modify: `apps/desktop/src/App.tsx`
- Modify: `apps/desktop/src/main.tsx`
- Modify: `apps/desktop/src/features/workbench/Workbench.tsx`
- Test: `apps/desktop/tests/resource-workbench-integration.test.tsx`
- Test: `apps/desktop/tests/project-workbench.test.tsx`

**Interfaces:**

- `App` receives `resourceGateway?: ResourceGateway`.
- `main.tsx` injects one `TauriResourceGateway`.
- `Workbench` receives the active-project-scoped gateway and passes
  `project.id` to `features/resources/ResourcePane`.
- Selecting a portable path calls `previewProjectResource`; the preview is
  cleared before a different project becomes active.

- [x] **Step 1: Write RED integration tests**

Test closed-by-default panel mounting, tree selection to preview, stale
project-A preview discard after switching to B, and no resource request when
there is no active project.

- [x] **Step 2: Run RED**

```powershell
pnpm --filter @pillow/desktop exec vitest run tests/resource-workbench-integration.test.tsx
```

Expected: `App`/`Workbench` do not accept the resource gateway.

- [x] **Step 3: Implement scoped mounting**

Use a project generation key:

```ts
const scope = { projectId: project.id, generation: resourceGeneration };
const preview = await resourceGateway.previewProjectResource({
  projectId: scope.projectId,
  relativePath,
});
if (scope.projectId !== activeProjectId || scope.generation !== resourceGeneration) {
  return;
}
```

Keep the conversation as the main surface. The resource browser and preview
share the existing right contextual region; do not add another sidebar.

- [x] **Step 4: Run GREEN**

```powershell
pnpm --filter @pillow/desktop exec vitest run tests/resource-workbench-integration.test.tsx tests/project-workbench.test.tsx
pnpm --filter @pillow/desktop typecheck
git diff --check
```

Expected: all commands exit 0.

### Task 3: Replace the parallel activity demo with a real timeline projection

**Status:** Accepted on 2026-07-31. The real owning conversation timeline now
mounts `AgentActivityDisclosure` from `RunActivityView` without a parallel
activity store. Controller acceptance: focused integration 23/23, six-suite
activity/resource regression 85/85, complete desktop Vitest 204/204,
typecheck, production UI build (68 modules), and diff check. Codex2 final
review: Spec Compliance compliant, Code Quality approved, Critical 0,
Important 0. The duplicate-key warning in one impossible synthetic fixture
was removed without changing production behavior. Nothing staged or committed.

**Files:**

- Create:
  `apps/desktop/src/features/conversations/agentActivityFromTimeline.ts`
- Modify:
  `apps/desktop/src/features/conversations/ConversationTimeline.tsx`
- Modify:
  `apps/desktop/src/features/conversations/AgentActivityDisclosure.tsx`
- Modify:
  `apps/desktop/src/features/conversations/agentActivityTypes.ts`
- Modify:
  `apps/desktop/src/features/conversations/agentActivityProjection.ts`
- Test:
  `apps/desktop/tests/agent-activity-timeline-integration.test.tsx`
- Test:
  `apps/desktop/tests/conversation-workbench.test.tsx`
- Test:
  `apps/desktop/tests/conversation-gateway-stream.test.tsx`
- Test:
  `apps/desktop/tests/conversation-fix-pass2.test.tsx`
- Test:
  `apps/desktop/tests/resource-workbench-integration.test.tsx`
- Create:
  `docs/execution/2026-07-31-wave-c2-agent-activity-timeline-report.md`

**Interfaces:**

- Consumes `timeline.conversationId` and each real `RunActivityView`.
- Produces `AgentActivityRecord` without creating new activity state.

```ts
export function agentActivityFromRun(
  conversationId: string,
  activity: RunActivityView,
): AgentActivityRecord;
```

Mapping rules:

- `activity.plan.steps` -> `steps`;
- `activity.tools` -> `tools`;
- `activity.subagents` -> `subagents`;
- `activity.artifacts` and tool resources -> portable `artifacts`;
- existing truncation/omission counts remain visible as summaries;
- `runId`, tool call IDs, and sub-agent IDs are keys only and never text.

- [x] **Step 1: Write RED mapping and DOM tests**

Use actual `RunActivityView` fixtures. Prove foreign conversation records are
absent, plan/tool/sub-agent order is preserved, artifact callbacks are
portable, and raw IDs do not appear.

- [x] **Step 2: Run RED**

```powershell
pnpm --filter @pillow/desktop exec vitest run tests/agent-activity-timeline-integration.test.tsx
```

Expected: `agentActivityFromRun` is missing.

- [x] **Step 3: Implement the pure mapper and timeline mount**

Replace the `RunActivity` render site only after the mapper tests pass.
Do not poll sessions or create a global task store.

- [x] **Step 4: Run GREEN**

```powershell
pnpm --filter @pillow/desktop exec vitest run tests/agent-activity-disclosure.test.tsx tests/agent-activity-timeline-integration.test.tsx tests/conversation-workbench.test.tsx
pnpm --filter @pillow/desktop exec vitest run tests/conversation-gateway-stream.test.tsx tests/conversation-fix-pass2.test.tsx tests/resource-workbench-integration.test.tsx
pnpm --filter @pillow/desktop typecheck
git diff --check
```

Expected: all commands exit 0.

### Task 4: Project isolation, restart recovery, and desktop acceptance

**Execution refinement:** The first-pass implementation is split into three
non-overlapping worker slices in
`docs/superpowers/plans/2026-07-31-c2e-isolation-restart-desktop-acceptance.md`.
It uses the real Tauri/WebView2 lifecycle and public UI through a test-scoped
CDP connection. Production code remains unchanged unless the controller
reproduces a product defect and issues a separate repair brief.

**Files:**

- Modify: `apps/desktop/package.json`
- Create: `apps/desktop/tests/e2e/c2-resource-workbench.spec.ts`
- Create: `scripts/wave-c2-resource-acceptance.mjs`
- Create: `scripts/lib/c2-activity-ui-acceptance.mjs`
- Create: `scripts/lib/c2-resource-ui-acceptance.mjs`
- Create: `docs/verification/2026-08-01-wave-c2-acceptance.md`

**Interfaces:**

- Scenario phase `create`: register projects A/B, browse/read distinct files,
  create conversation activity, and close through the normal app path.
- Scenario phase `restore`: reuse app data, prove project-specific recents and
  conversations restore without cross-project rows, then exit cleanly.

- [x] **Step 1: Write the failing two-project E2E**

Assertions must cover:

```text
A tree/search/preview never shows B-only marker
B tree/search/preview never shows A-only marker
A activity appears only in A conversation
restart restores A and B recents separately
resource panel remains contextual and keyboard reachable
```

- [x] **Step 2: Run RED**

```powershell
node --test --test-concurrency=1 apps/desktop/tests/e2e/c2-resource-workbench.spec.ts
```

Expected: acceptance scenario or wiring is missing.

- [x] **Step 3: Add only the acceptance harness required by the test**

The harness creates temporary project roots and app data, records exact child
PIDs, uses the normal close path, and removes only its own temporary
directories after PID verification.

- [x] **Step 4: Run focused GREEN and cleanup checks**

```powershell
node --test --test-concurrency=1 apps/desktop/tests/e2e/c2-resource-workbench.spec.ts
pnpm --filter @pillow/desktop test
pnpm --filter @pillow/desktop typecheck
```

Expected: all commands exit 0 and residual Pillow/Cargo/Rust/Node/Playwright
task processes equal 0.

### Task 5: Full gate, independent review, and C2 checkpoint

**Files:**

- Modify: `docs/roadmap/2026-07-31-stage1-overall-status.md`
- Modify: `README.md`
- Create: `docs/verification/2026-08-01-wave-c2-checkpoint.md`

- [x] **Step 1: Run the full integration gate**

```powershell
pnpm check
pnpm --filter @pillow/desktop build:ui
pnpm --filter @pillow/desktop test:e2e
cargo fmt --all -- --check
git diff --check
```

Expected: Vitest, Rust workspace/doctests, Clippy `-D warnings`, production
build, every serial desktop E2E, fmt, and diff all pass.

- [x] **Step 2: Run independent Codex2 review**

Review only after Step 1 is green. Reject Critical/Important issues involving
project isolation, renderer filesystem access, invented activity, path
leakage, restart loss, or C2 scope expansion.

- [x] **Step 3: Record checkpoint truth**

The checkpoint lists exact changed files, focused and full gate evidence,
provenance, residual risks, staging count, residual process count, and one
controller verdict. It must state that checkpoint acceptance does not
authorize stage or commit.

## Plan Self-Review

- Spec coverage: tree, search, recents, read-only text/Markdown/image/basic
  preview, conversation-owned activity, project isolation, and restart
  recovery each map to a task.
- Scope coverage: writes, terminal, Git, Obsidian, keys, MCP, network, and new
  runtimes remain excluded.
- Type consistency: `projectId` and portable `relativePath` cross every
  resource interface; the activity mapper consumes the existing
  `RunActivityView` and produces the accepted `AgentActivityRecord`.
- Placeholder scan: every step names concrete files, interfaces, commands, and
  expected outcomes.
