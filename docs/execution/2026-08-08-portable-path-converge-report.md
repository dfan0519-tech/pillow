# Portable path converge report — COMPLETE (strict)

**Status:** `PATH-CONVERGE-COMPLETE`  
**Date:** 2026-08-08  
**Worker:** grok-worker  
**Scope:** move portable path grammar into `keeper-project`, converge all
whitelisted surfaces onto the **strict** device-name rule (supersedes v1
“zero behavior change” / previous BLOCKER report)

## Verdict

```
PATH-CONVERGE-COMPLETE
```

Deliberate behavior change: `git-executor` and the TS DTO mirror now reject
Windows superscript device stems (`COM¹`–`COM³`, `LPT¹`–`LPT³`) and
`COM0`/`LPT0`. `kernel-resource` keeps the same public error type and the same
strict superscript policy it already had; only `COM0`/`LPT0` is a small
additional tighten (see §COM0).

## Where the grammar lives

| Item | Location |
|------|----------|
| **SSoT** | `crates/kernel-project/src/portable_path.rs` |
| Public exports | `keeper_project::{validate_portable_path, validate_portable_path_or_directory, is_portable_path, is_portable_entry_path, PORTABLE_PATH_MAX_LEN, PORTABLE_COMPONENT_MAX_LEN, PathRejectDetail}` |
| Directory form | `validate_portable_path_or_directory` moved **intact** with file form (not split) |

No new crate. Not placed in `agent-protocol`.

## Error-type preservation (external surfaces)

| Consumer | External error form | Mapping |
|----------|---------------------|---------|
| **git-executor** | `ParseError::PathRejected { detail: &'static str }` | Thin module `crates/git-executor/src/portable_path.rs` re-exports / forwards to `keeper_project`; `status.rs` still maps `PathRejectDetail` → `ParseError::PathRejected { detail }` |
| **git-executor** | `GitReadError::PathRejected` (detail-free) | Unchanged: still `.map_err(|_| GitReadError::PathRejected)` after validate |
| **kernel-resource** | `ResourceError::PathRejected` (detail-free) | `keeper_project::validate_portable_path(path).map_err(\|_\| ResourceError::PathRejected)` |
| **TS** | boolean DTO reject (no typed detail) | Same accept/reject surface via `isPortablePath` / `isPortableEntryPath` |

`PathRejectDetail` remains `&'static str` (type alias re-export). Detail strings
(`"windows device name"`, etc.) are produced by the SSoT; porcelain mapping shape
is unchanged.

## Strict device-name rule (canonical)

Stem = first segment before `.`, then trim trailing space/dot, then
`to_ascii_uppercase()` (ASCII letters only; superscripts unchanged).

1. `CON` / `PRN` / `AUX` / `NUL` / `CLOCK$` / `CONIN$` / `CONOUT$`
2. `COM`/`LPT` + **ASCII digit** `0`–`9` (four-byte stem)
3. Explicit stems: `COM¹` `COM²` `COM³` `LPT¹` `LPT²` `LPT³`
   (U+00B9 / U+00B2 / U+00B3)

## COM0 / LPT0 — verification and policy

| Source | COM0 / LPT0 listed? |
|--------|---------------------|
| Microsoft Learn *Naming Files, Paths, and Namespaces* (Win32 fileio) | **No** — lists `COM1`–`COM9`, `COM¹`–`COM³`, `LPT1`–`LPT9`, `LPT¹`–`LPT³` only |
| Same MS page note on superscripts | **Yes** — Windows treats ISO-8859-1 superscripts ¹²³ as digits in COM#/LPT# device names |
| Microsoft 365 / SharePoint invalid-name tables | **Yes** — COM0–COM9 / LPT0–LPT9 |
| Wikipedia / DOS device literature | **Yes** — COM0–COM9 / LPT0–LPT9 |

**Conclusion:** Official Win32 naming doc does **not** list COM0/LPT0; secondary
and product docs disagree. Per brief §4:

> **fail-closed: reject COM0/LPT0** — **uncertain under conservative choice**.

Rationale: rejecting an odd filename costs far less than accepting a device alias.
Report marks this explicitly; not claimed as proven Win32 reserved status from
the primary naming page alone.

## Four rejection proofs (superscript devices)

| Surface | Proof |
|---------|--------|
| **1. keeper-project (SSoT)** | Unit: `portable_path::tests::rejects_superscript_com_lpt_device_stems`; contract: `tests/portable_path_contract.rs::shared_fixture_rejects_superscript_devices` + full fixture table |
| **2. git-executor** | Unit: `portable_path::tests::rejects_superscript_device_names`; contract: `tests/portable_path_contract.rs` (shared JSON) |
| **3. kernel-resource** | Unit: `portable_path_tests::rejects_superscript_com_lpt_devices` in `src/lib.rs` |
| **4. TS + fixture** | `tauriGitGateway.ts` strict `isWindowsDeviceComponent`; vitest `rejects superscript COM/LPT device stems` + cross-layer contract describe |

All six of `COM¹`…`LPT³` are in `tests/fixtures/portable-path-cases.json` under
**both** `fileForm.reject` and `entryForm.reject`.

## Contract table changes (stricten only)

`tests/fixtures/portable-path-cases.json` — **additions only** (no deletes, no
accept→reject removals, no new accepts that loosen rules):

| path | forms | note |
|------|-------|------|
| `COM0` | file + entry reject | fail-closed |
| `LPT0` | file + entry reject | fail-closed |
| `COM\u00b9` | file + entry reject | U+00B9 |
| `COM\u00b2` | file + entry reject | U+00B2 |
| `COM\u00b3` | file + entry reject | U+00B3 |
| `LPT\u00b9` | file + entry reject | U+00B9 |
| `LPT\u00b2` | file + entry reject | U+00B2 |
| `LPT\u00b3` | file + entry reject | U+00B3 |

Pre-existing accept/reject cases unchanged. Description field updated to name
SSoT and strict policy.

## Wiring

| Crate / file | Change |
|--------------|--------|
| `crates/kernel-project` | New `portable_path` module + exports; dev-deps for contract test |
| `crates/git-executor/Cargo.toml` | **Only** added `keeper-project` |
| `crates/git-executor/src/portable_path.rs` | Thin forward to `keeper_project` (public bool API + crate-local validate) |
| `crates/kernel-resource/src/lib.rs` | Local validator + device helper removed; call SSoT |
| `apps/desktop/src/lib/tauriGitGateway.ts` | Device check: `[0-9]` + superscript stems |
| `apps/desktop/tests/tauri-git-gateway.test.ts` | Explicit superscript / COM0 / LPT0 reject test |

Directory form remains **status-entry only**:
- `is_portable_path("vendor/") == false`
- `is_portable_entry_path("vendor/") == true`
- repo id / diff target still file-form only

## Test coverage map (§6)

| Requirement | How covered |
|-------------|-------------|
| 1. Cross-crate / cross-language contract table | `keeper-project` + `git-executor` integration tests load same JSON; vitest twin loads same JSON |
| 2. Superscript rejected at four surfaces | Unit tests + fixture cases above |
| 3. Existing git-executor / kernel-resource path tests | All retained; gate ran full suite |
| 4. Directory form constraints | Contract + unit tests assert entry-only trailing `/` |

## Commands / exit codes

| Command | Exit |
|---------|------|
| Keeper bridge boot | 400 (bridge rejected; work continued under whitelist) |
| `cargo test -p keeper-project -p keeper-git-executor -p keeper-resource` (focused) | 0 (all portable_path + full package tests green) |
| `node scripts/run-worker-gate.mjs` | **0** (`PILLOW_WORKER_GATE: OK`) |
| `corepack pnpm --filter @pillow/desktop build` | **0** |

Worker gate included: workspace check, secrets, rustfmt, typecheck, vitest,
cargo clippy/tests, ACP drift check.

## What was **not** touched

- `crates/storage-sqlite/**` (still has weaker local path checks — out of scope)
- `crates/runtime-host/**`, `crates/adapter-grok-acp/**`, `scripts/**`
- `apps/desktop/src/features/**`, `apps/desktop/src-tauri/**`
- `crates/agent-protocol/**`, `crates/tool-executor/**` (already had superscript arm; not whitelisted)
- `.superpowers/**` (except this ticket’s brief supersession by total control)
- No `git add` / `commit` / `stash` / `reset` / `clean` / `checkout` / `restore`
- No desktop E2E
- No auth/token reads
- No opportunistic refactors outside portable-path converge

## Residual risk

1. **Copies outside whitelist** (`agent-protocol`, `tool-executor`) already
   match superscript strictness but still own local copies — future drift possible
   until they also call `keeper_project`.
2. **storage-sqlite** `resource_recent` path checks remain weaker (no device
   names) — pre-existing; not modified.
3. **COM0/LPT0** rejection is conservative; if product later wants to allow
   literal `COM0` filenames that Win32 actually permits, that would require an
   explicit relax ticket (forbidden on this pass).
4. **Behavior change on git-executor**: porcelain parse, discovery, command
   gate, and public `is_portable_path` now reject paths that previously passed
   (superscript / COM0 family). Expected and contract-tested.
5. Desktop **E2E not run** (controller-owned); C2/C4 paths touched only via
   shared grammar + TS DTO mirror.

## Completion marker

```
PATH-CONVERGE-COMPLETE
```
