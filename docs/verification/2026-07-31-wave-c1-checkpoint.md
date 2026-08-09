# Pillow Wave C1 Checkpoint

Date: 2026-07-31

Status: accepted working-tree checkpoint; intentionally unstaged and uncommitted

## Scope

This checkpoint records the accepted C1 integration set on top of controller
baseline `7b7f70f`. It does not authorize a commit and does not alter the
accepted historical evidence in `2026-07-31-wave-c1-acceptance.md`.

C1 closed the provider-originated, governed read-only path:

```text
provider proposal
  -> fixed B3 manifest validation
  -> project-scoped read execution
  -> durable tool receipt
  -> owning conversation timeline projection
```

Only `fs.list@1`, `fs.read@1`, and `fs.search@1` may cross that boundary.
Unknown provider tool names fail closed. Provider-native tools, arbitrary MCP
servers, filesystem writes, shell execution, Git mutation, credential access,
and network tools remain disabled.

## Current Working-Tree Boundary

- The complete C1/B3 integration remains unstaged and uncommitted.
- The controller verified 57 unstaged paths and zero staged paths on this
  checkpoint.
- No C1/B3 path may be reverted, overwritten, or selectively reset while C2
  is developed.
- legacy Keeper (out of tree) remains a frozen resource manager; Pillow
  product work belongs only in this repository.

## Fresh Verification

Executed in `<repo>` on 2026-07-31:

- `pnpm check` passed: Vitest 102/102, Rust workspace tests and doctests, and
  Clippy with `-D warnings`.
- `pnpm --filter @pillow/desktop build:ui` passed: 56 modules built.
- `pnpm --filter @pillow/desktop test:e2e` passed: 6/6 serial desktop tests.
- `cargo fmt --all -- --check` passed.
- `git diff --check` passed.
- No residual Pillow, Cargo, Rust, Node, or Playwright process was observed.

## Next Authorized Product Boundary

C2 is the project-isolated, read-only resource workbench: file tree, search,
recent files, read-only text/Markdown/image/basic-artifact views, contextual
resource preview, conversation-local real Agent activity, project switching,
and restart recovery. C2 excludes filesystem writes, terminal, Git, Obsidian,
key management, and new runtime connections.

## Residual Risks Carried Forward

1. The live Grok ACP test remains explicit opt-in; deterministic protocol,
   persistence, and desktop gates cover the local path.
2. C1 receipt reconciliation is correct at current scale but should gain a
   watermark or repair queue before very large histories.
3. C2 must preserve the C1 fixed-manifest and project-isolation boundaries;
   resource browsing is not authority to mutate a workspace.
