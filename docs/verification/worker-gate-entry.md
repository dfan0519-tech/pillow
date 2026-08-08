# Worker Gate Entry (sole compliant)

**Status:** binding for all Pillow Workers  
**Updated:** 2026-08-08 (step 1c gate hardening)

## Sole command

```bash
pnpm check
```

Equivalent direct entry (same pipeline):

```bash
node scripts/run-worker-gate.mjs
```

**This is the only compliant Worker gate.** Do not treat single-crate
`cargo test -p …` or partial script subsets as a green gate for task completion.

Controller-owned gates (desktop E2E, smoke, live ACP) are out of scope here and
must not be run by Workers unless a task contract explicitly assigns them.

## What the gate runs

| Step | Command |
| --- | --- |
| Workspace structure | `node scripts/check-workspace.mjs` |
| Secrets / machine paths | `node scripts/check-secrets.mjs` |
| TypeScript typecheck | `pnpm typecheck` |
| Package (renderer) tests | `pnpm test` |
| Rust workspace tests | `node scripts/run-cargo.mjs test --workspace` |
| Clippy | `node scripts/run-cargo.mjs clippy --workspace -- -D warnings` |
| ACP protocol lock drift | `node scripts/check-acp-protocol-drift.mjs` |

## Why single-crate cargo is not enough

Some integration tests in `keeper-runtime-host` are compiled only when the
`test-support` feature is enabled (expiry and storage-failure seams in
`governed_write_bridge` / `governed_process_bridge`).

Historically, that feature was turned on for workspace builds only via
feature unification from `apps/desktop/src-tauri` **dev-dependencies**. A Worker
who ran:

```bash
cargo test -p keeper-runtime-host --test governed_write_bridge
```

could see **16** tests green while the full gate compiled **19**. That blind
spot already caused a real miss (2026-08-08 B2).

### Mitigations in place

1. **Package self-dep** (`crates/runtime-host/Cargo.toml` dev-dependencies):
   `keeper-runtime-host = { path = ".", features = ["test-support"] }` so even
   single-package `cargo test -p keeper-runtime-host` enables the seams.
2. **Sole gate entry** (this document + `pnpm check`): Workers complete against
   the full workspace pipeline, not ad-hoc package filters.
3. **Tests are not deleted or weakened**; feature-gated coverage remains.

## ACP protocol drift check

- Baseline: `docs/references/acp-protocol-lock-baseline.json`
- Script: `node scripts/check-acp-protocol-drift.mjs`
- Fetches upstream `Cargo.lock` as **text only** (no clone build, no install,
  no upstream script execution, no live ACP).
- **Drift** → exit 1 with name/version/checksum differences.
- **Offline / network error** → exit 0 with explicit
  `ACP_DRIFT_CHECK: SKIPPED (未核对)` (never silent “verified”, never gate-red
  solely for offline).
- Self-test: `node scripts/check-acp-protocol-drift.mjs --self-test`

## Explicit non-entries

These are **not** substitutes for the sole gate:

- `cargo test -p <crate>`
- `cargo test -p keeper-runtime-host --test governed_write_bridge`
- renderer-only `pnpm test` / `pnpm typecheck`
- desktop E2E / smoke / live ACP acceptance

## Related evidence

- Step 1c report: `docs/execution/2026-08-08-step1c-gate-hardening-report.md`
- Upstream ACP audit: `docs/execution/2026-08-08-upstream-grokbuild-audit.md`
- Upstream design pin: `docs/references/upstream-baseline.md`
