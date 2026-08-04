# Pillow C2-A: Read-Only Resource Core

## Worker and ownership

Owner: a leased `grok1`/`grok2`/`grok3` CLI worker selected by the controller.
The original GLM allocation is retired after repeated non-executing attempts.

This is the first implementation task in C2. Work only in `<workspace>\pillow`.
Keeper and `<workspace>\VaultSystem\keeper` are read-only resource-management
infrastructure and must not be edited.

## Goal

Build the Rust-only, project-scoped read-only resource core used by the later
typed Tauri bridge and renderer work. It must support deterministic directory
listing, bounded text/binary reads, bounded text search, and durable recent
resource paths. It must not expose a write, process, Git, network, credential,
Obsidian, MCP, or runtime-launch capability.

## Required execution order

Do not spend an extended planning-only turn. Your first implementation action
must be a tool call that creates the focused failing test files, followed
immediately by the named RED test command. Only after observing the expected
failure may you create production files. If a contract detail is unclear, use
the narrowest interpretation that preserves read-only project isolation and
record it as residual risk; do not wait for a controller response.

## Exact write whitelist

- `Cargo.toml`
- `Cargo.lock` only if Cargo changes it
- `crates/kernel-resource/Cargo.toml`
- `crates/kernel-resource/src/lib.rs`
- `crates/kernel-resource/tests/read_only_resource_core.rs`
- `crates/storage-sqlite/Cargo.toml`
- `crates/storage-sqlite/src/lib.rs`
- `crates/storage-sqlite/src/migrations.rs`
- `crates/storage-sqlite/src/resource_recent.rs`
- `crates/storage-sqlite/migrations/0004_resource_recents.sql`
- `crates/storage-sqlite/tests/resource_recent_repository.rs`

Do not edit any other file. In particular, do not edit `crates/tool-executor`,
`crates/agent-protocol`, `apps/**`, C1/B3 files, project commands, existing
migrations, or documentation. Do not stage or commit.

## Public contract

Create crate package `keeper-resource` under `crates/kernel-resource`.
It may depend only on workspace dependencies and must have no Tauri or renderer
dependency.

Expose these serializable types and APIs (equivalent names are allowed only if
their purpose and fields remain exact):

```rust
ProjectResourceCapability::new(project_id: ProjectId, root: impl AsRef<Path>)
  -> Result<ProjectResourceCapability, ResourceError>

ResourceReader::list(&self, capability: &ProjectResourceCapability,
                     request: ResourceListRequest)
  -> Result<ResourceDirectory, ResourceError>
ResourceReader::read(&self, capability: &ProjectResourceCapability,
                     request: ResourceReadRequest)
  -> Result<ResourceFile, ResourceError>
ResourceReader::search(&self, capability: &ProjectResourceCapability,
                       request: ResourceSearchRequest)
  -> Result<ResourceSearchResults, ResourceError>
```

`ResourceEntry` contains `path`, `kind` (`file`, `directory`, `symlink`, or
`other`) and optional `size_bytes`. `ResourceFile` contains `path`,
`content_kind` (`utf8` or `binary`), optional UTF-8 `content`, optional raw
`bytes`, `bytes_returned`, and `truncated`. `ResourceSearchMatch` contains
`path`, one-based `line`, one-based byte `column`, and a bounded UTF-8
`preview`. Every list/search response has `truncated`.

The capability captures a canonical existing directory and the owning
`ProjectId`. All returned paths use `.` for root and `/` separators otherwise.

## Non-negotiable behavior

1. Requests accept only relative portable paths. Reject empty paths, absolute
   paths, drive prefixes, backslash paths, `.`/`..` components, and paths whose
   canonical target leaves the captured root. The dedicated root request is
   represented only by `.`.
2. At every component traversal and before consuming a file, reject symlinks
   and Windows reparse points. A reparse point must be surfaced as a
   `symlink` entry in a directory list but is never followed, read, or searched.
3. Deterministic directory output sorts by filename. Directories may be listed;
   files, symlinks and other entries retain their kind. Enforce a caller limit
   capped at 1,000 entries.
4. Text/binary reads are bounded: caller `max_bytes` is capped at 1 MiB,
   exactly reports `bytes_returned` and `truncated`, returns UTF-8 text only
   when the selected bytes are valid UTF-8, and otherwise returns binary bytes
   without treating them as text. The core never guesses an image/document
   renderer and never writes a file.
5. Search walks only regular files beneath a validated directory, skips binary
   or overlong lines, sorts paths deterministically, returns UTF-8 previews,
   and caps both candidate files and result count. It must make no unbounded
   read and must never follow a symlink or reparse point.
6. Recent resources are durable and project-scoped. Migration 0004 creates a
   table keyed by `(project_id, path)` with `opened_at`; recording an existing
   path updates its timestamp. Listing returns at most a caller limit capped
   at 50, ordered newest-first with a deterministic path tie-break. The SQLite
   repository validates a nonempty project id and portable relative path but
   leaves filesystem authorization to `ProjectResourceCapability`.
7. Follow existing Rust error style: public errors reveal no arbitrary machine
   path, key, process, or provider detail. Do not log file contents.

## Mandatory TDD evidence

Write each focused test first and run it while the implementation is absent or
does not yet satisfy the behavior. Record the failing command and expected
failure in the completion report before adding production code. The final tests
must cover at least:

- list output is sorted and a symlink/reparse entry is not traversed;
- absolute, `..`, backslash, and outside-root requests are rejected;
- read distinguishes valid UTF-8 from binary and obeys byte truncation;
- search is deterministic, bounded, and excludes symlink/reparse descendants;
- opening the SQLite repository applies migration 0004 idempotently;
- recent paths are isolated by project, upsert their timestamp, and obey the
  newest-first cap.

Run after each green step and at completion:

```powershell
cargo test -p keeper-resource
cargo test -p keeper-storage-sqlite --test resource_recent_repository
cargo fmt --all -- --check
git diff --check
```

## Completion report

Write `docs/execution/2026-07-31-wave-c2a-resource-core-report.md` only after
the implementation is complete. It must list changed files, every RED command
and outcome, every final command and outcome, residual risks, and a concise
Keeper completion message. Return only: status (`DONE`, `DONE_WITH_CONCERNS`,
`NEEDS_CONTEXT`, or `BLOCKED`), changed files, test summary, and concerns.
