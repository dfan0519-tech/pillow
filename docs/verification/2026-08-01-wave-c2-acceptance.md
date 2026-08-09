# Pillow Wave C2 Acceptance

Date: 2026-08-01

Verdict: ACCEPT

Status: accepted working-tree implementation; intentionally unstaged and
uncommitted

## Accepted Scope

C2 completes the Stage 1 read-only project workbench:

- project-scoped file tree, bounded text search, and recent files;
- read-only text, Markdown, PNG/JPEG/GIF/WebP, and basic artifact preview;
- typed Tauri/TypeScript resource boundary with project ID and portable-path
  ownership;
- conversation-owned plan, tool, sub-agent, artifact, test, truncation, and
  terminal activity through progressive disclosure;
- project-switch isolation, stale-request rejection, restart recovery,
  keyboard reachability, normal close, and owned-process cleanup.

C2 does not authorize filesystem writes, patch application, terminal, Git,
Obsidian, key management, arbitrary MCP/provider-native/network tools, or new
runtimes.

## Security and Authority Findings Closed

Independent review initially rejected the tree and the controller required
repairs before acceptance:

1. provider-controlled successful tool output could reach the activity DOM;
2. blank resource search could form an empty needle and panic on a UTF-8 byte
   boundary;
3. the new native blank-query error code was absent from the renderer typed
   allowlist;
4. resource-pane copy mentioned the excluded terminal capability;
5. a C2-E report described an obsolete WebView2 environment-variable path.

The final activity projection is fail-closed: raw `tool.output` text never
enters the production view model. A successful output shows only a fixed local
returned-output state; failures remain opaque. Governed artifact/resource
preview and structured test results are the supported detail channels. The
resource kernel now rejects empty or whitespace-only queries before scanning,
including Unicode whitespace, and the native/renderer error contract remains
opaque and typed.

## Fresh Controller Evidence

Executed in `this repository` across the final 2026-07-31/2026-08-01 closeout:

- `pnpm check`: exit 0.
  - desktop Vitest: 17 files, 206/206 tests;
  - Rust workspace tests: pass;
  - Rust doctests: pass;
  - Clippy workspace with `-D warnings`: pass.
- `pnpm --filter @pillow/desktop build`: exit 0; 68 modules transformed.
- `cargo fmt --all -- --check`: exit 0.
- `git diff --check`: exit 0.
- `git diff --cached --check`: exit 0.
- focused real desktop C2-E:
  `node --test --test-concurrency=1 apps/desktop/tests/e2e/c2-resource-workbench.spec.ts`:
  1/1 passed in about 189 seconds after forced-logoff recovery.
- complete serial desktop gate: `pnpm desktop:test:e2e`: 7/7 passed in one
  run, about 409 seconds.

The complete desktop run covered C2-E isolation/restart/normal close,
conversation persistence/cancellation/interruption recovery, real desktop
restart, project creation/restore, smoke-process cleanup, Cargo-wrapper signal
cleanup, and C1 visual acceptance.

## Independent Review

Codex2 reviewed the complete C2 authority and UI boundary after development
was green. Review findings were repaired and re-reviewed until the final
verdict was:

- ACCEPT;
- Critical: 0;
- Important: 0.

The final test-only follow-up was also accepted as preserving project
isolation and structured activity coverage.

## Working-Tree and Process Evidence

- HEAD: `7b7f70f7360131cd1cfd984a8b19bdfb5725c21b`.
- Staged paths: 0.
- Working-tree paths: 129 total (47 modified, 82 untracked), containing the
  shared C1/B3/C2 implementation and evidence set.
- Relevant residual Pillow, Node, Cargo, Rust, and WebView2 task processes: 0.
- The observed failed-run CDP port was released before checkpointing.

Because C1/B3 and C2 intentionally share one unstaged tree, Git cannot
reconstruct an independent per-wave patch. The live `git status --short` is
the authoritative exact path inventory. No file was reset, staged, or
committed during acceptance.

## Residual Risks

1. `RunActivity.tsx` still contains a legacy raw-output render path but has no
   production import or mount. It must remain unreachable or be retired in a
   later cleanup.
2. The C2-E runtime uses the deterministic fake ACP fixture over a real
   Tauri/WebView2/SQLite lifecycle; provider-live acceptance remains a separate
   explicit opt-in gate.
3. Search reports byte columns. Non-ASCII case-folding can make a byte column
   less intuitive even though the search remains bounded and safe.
4. Image transport is byte-bounded and signature/MIME validated; decoded pixel
   dimensions are not yet independently capped.

None of these residuals authorizes scope expansion in C2.
