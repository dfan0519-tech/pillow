# Wave C2-B — Project Resource Bridge

## Owner and dependency

- **Owner:** leased `grok3` CLI worker, after C2-A is reported complete and its
  public crate interface is frozen.
- **Input contract:** C2-A's `keeper-resource` read-only capability. Do not
  recreate directory traversal, read, search, or recent-item persistence in
  the Tauri or TypeScript layers.
- **Objective:** expose typed, project-scoped, read-only resource operations to
  the desktop renderer. This is a bridge slice, not UI work.

## Exact write whitelist

- `apps/desktop/src-tauri/Cargo.toml`
- `apps/desktop/src-tauri/src/app_state.rs`
- `apps/desktop/src-tauri/src/lib.rs`
- `apps/desktop/src-tauri/src/resource_commands.rs` (new)
- `apps/desktop/src-tauri/src/resource_commands_tests.rs` (new, if needed)
- `apps/desktop/src/lib/resourceGateway.ts` (new)
- `apps/desktop/src/lib/tauriResourceGateway.ts` (new)
- `apps/desktop/tests/resource-gateway.test.ts` (new)
- `docs/execution/2026-07-31-wave-c2b-resource-bridge-report.md` (new)

No React components, CSS, `App.tsx`, `Workbench.tsx`, conversation code,
provider/runtime code, terminal/Git code, or C2-A files may be changed. Do not
stage, commit, or alter existing C1/B3 work.

## Required public contract

The renderer receives only portable relative paths and bounded content. It
never receives a project root, a native handle, a machine path, or an error
containing either.

- `listProjectResources({ projectId, relativePath? }) -> ResourceEntry[]`
- `readProjectResource({ projectId, relativePath }) -> ResourceFile`
- `searchProjectResources({ projectId, query, relativePath? }) -> ResourceSearchResult[]`
- `listRecentProjectResources({ projectId }) -> RecentResource[]`

The bridge must validate Tauri payloads exactly at the TypeScript boundary and
surface normalized opaque command errors. `projectId` is required on every
operation, and the Rust command resolves its root only from the registered
project store. A stale/unknown project fails closed; it must never fall back to
the current directory or the most recently opened project.

Both sides use camelCase DTOs. Successful reads update only the matching
project's recent list. Binary or oversized files return preview metadata, never
unbounded bytes. The bridge has no mutation operation and no direct renderer
filesystem access.

## Test-first and verification

1. Add the TypeScript contract tests first and demonstrate their first run is
   red before implementing the gateway.
2. Add focused Rust tests for project-id isolation and fail-closed invalid
   inputs before adding command wiring.
3. Run:

   ```powershell
   cargo test -p pillow-desktop resource
   pnpm --filter @pillow/desktop test -- resource-gateway
   pnpm --filter @pillow/desktop typecheck
   cargo fmt --all -- --check
   git diff --check
   ```

The completion report names changed files, the initial RED test output, final
test output, known limitations, and confirms no stage/commit.
