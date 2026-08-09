# Path validation robustness — 2026-08-09

**Status:** Implementation complete under whitelist. Full worker gate **BLOCKED-BY-CONCURRENT** (fmt/clippy failures only in `crates/**` owned by parallel workers).  
**Marker:** `PATH-VALIDATION-ROBUSTNESS-BLOCKED: BLOCKED-BY-CONCURRENT — gate fails only on parallel workers' crates/** (knowledge-indexer fmt, storage-sqlite clippy)`

---

## 1. Problem (as given; not re-argued)

Startup Grok executable / profile path checks treated **any** failed probe as
“path invalid” and degraded. Under heavy concurrent load, batch-16 E2E once
logged `runtime_profile_invalid` for a profile dir the harness had created and
not yet cleaned. Separate re-runs and unloaded full suites passed; root cause
of that flake was **not** proven.

**This ticket does not claim to fix that flake.** It fixes a correctness bug:
conflating “definitely missing/wrong type” with “probe could not decide”
is wrong regardless of whether transient `stat` failures caused the incident.

---

## 2. Three-way logic (implementation)

Location: `apps/desktop/src-tauri/src/runtime_composition.rs`.

| Probe result | Meaning | Disposition |
| --- | --- | --- |
| **ConfirmedOk** | `metadata` ok and type matches (file / dir) | Start normally |
| **ConfirmedReject** | `NotFound`, `NotADirectory`, or exists but wrong type | **Fail-closed**, **legacy** error codes unchanged |
| **Indeterminate** | Other `io::Error` (permission, sharing, `Interrupted`, timeout, …) | Limited retry; still fail-closed with **new** codes + `ErrorKind` |

### API surface

- `metadata_for_path` — single `std::fs::metadata` (plus test inject under cfg).
- `classify_path` — maps metadata/IO → three-way `PathPresence`.
- `require_path_with_retry` — retries **only** Indeterminate; ConfirmedOk /
  ConfirmedReject return immediately (no sleep, no extra IO).
- `require_executable_file` / `require_profile_dir` — public failure mapping.
- `current_user_home_dir` — same three-way for default profile home
  (`Ok(None)` for confirmed reject → existing `runtime_profile_missing`).

`Path::is_file()` / `is_dir()` are no longer used for configured-path authority
(they collapse NotFound and IO error into `false`).

### Fail-closed on uncertainty

After retries, Indeterminate still **rejects**. Uncertainty is never treated
as pass. Degradation still **never** opens Fake.

---

## 3. Retry parameters and rationale

| Constant | Value | Rationale |
| --- | --- | --- |
| `PATH_PROBE_MAX_ATTEMPTS` | **3** | 1 initial + 2 retries; enough for short AV/indexer/share locks without multi-second startup |
| `PATH_PROBE_BACKOFF_MS` | **`[10, 30]`** | Cap ~40 ms extra latency **only** on indeterminate path; total sleep only if first two probes are IO errors |
| Happy path | **1× metadata, 0 sleep** | ConfirmedOk / ConfirmedReject never re-probe |

Normal startup with healthy FS: **zero extra IO and zero sleep** vs pre-change
behavior (still one existence/type check per configured path).

---

## 4. Error codes

### Unchanged (definite reject)

| Code | When |
| --- | --- |
| `runtime_executable_missing` | Empty path, or confirmed not a file / not found |
| `runtime_profile_invalid` | Empty profile, or confirmed not a directory / not found |
| `runtime_profile_missing` | No resolvable home (env unset or confirmed missing default home) |

### New (probe indeterminate after retries)

| Code | When |
| --- | --- |
| `runtime_executable_probe_failed` | Executable path could not be verified (IO after retries) |
| `runtime_profile_probe_failed` | Profile / home path could not be verified (IO after retries) |

Both new codes are **degradable** (`is_degradable_runtime_error`) so Settings
remain reachable. They have dedicated `guidance_for_runtime_error` copy that
states the failure is **not** “confirmed missing”.

**Not reused:** old codes are never returned for indeterminate probes.

### `ErrorKind` surfacing

Message template includes:

```text
Could not verify … (io::ErrorKind::{Debug}): {path}. Filesystem probe failed after 3 attempts; this is not a confirmed missing …
```

`format_io_error_kind` uses `Debug` of `io::ErrorKind` (e.g. `PermissionDenied`,
`Interrupted`, `TimedOut`) for stable diagnostics without reading file contents.

---

## 5. Injection / production invisibility

Module `path_probe_test_hook` and the inject branch in `metadata_for_path` are
gated:

```rust
#[cfg(any(test, feature = "test-support"))]
```

- **Statics** (`PROBE_COUNT`, thread-local remaining/kind): only in that cfg.
- **Public inject API** (`reset`, `inject_io_errors`, `probe_count`): only in that cfg.
- **Production path:** `metadata_for_path` is a thin `std::fs::metadata` call;
  no counters, no hooks, no extra parameters on `require_*`.
- Cargo feature `test-support` on `pillow-desktop` is empty and **not** in
  default features; unit tests use `cfg(test)` without enabling the feature.
- Unit test `path_probe_inject_module_is_test_gated` asserts the test cfg and
  exercises the hook only under `cfg(test)`.

---

## 6. Tests (seven required + legacy)

All in `runtime_composition::tests` (file is under write whitelist).

| # | Requirement | Test |
| --- | --- | --- |
| 1 | Confirmed missing → fail-closed, **legacy** codes | `path_probe_missing_still_fail_closed_legacy_codes` |
| 2 | Exists + correct → ok, **exactly one probe** | `path_probe_existing_ok_single_probe_no_retry_overhead` |
| 3 | Injected IO → retries | `path_probe_io_error_triggers_retry_then_success` (count 3 = 2 inject + 1 real) |
| 4 | Retry then success → ok | same test |
| 5 | Retry exhausted → **new** codes + `ErrorKind` | `path_probe_io_error_exhausted_uses_distinct_code_with_error_kind` |
| 6 | Degrade is **not Fake** | `path_probe_failure_degrades_not_fake` (`Degraded`, no Fake language, Ready forbidden) |
| 7 | Existing startup tests retained | Full filter `runtime_composition`: **32 passed** |

Also extended `degrade_outcome_is_never_fake_selection` for the two new codes.

**Did not** weaken any legacy assertion or expected code for confirmed-missing paths.

---

## 7. Explicit non-claims

- **Does not claim the batch-16 flake is fixed.** Field incident root cause was
  not established; preferred hypothesis (transient stat) remains unproven.
- **Does not relax** any confirmed-missing / wrong-type rejection.
- **Does not** fall back to Fake on degrade.
- **Did not** run desktop E2E (controller-owned).
- **Did not** `git add/commit/stash/reset/clean/checkout/restore`.

---

## 8. What was not touched

| Area | Status |
| --- | --- |
| `crates/**` | **Not modified** (parallel workers) |
| `scripts/**` | Not modified |
| `tests/fixtures/**` | Not modified |
| `apps/desktop/src/**` (frontend) | Not modified |
| Other `docs/**` | Only this report |
| Auth/token files | Not read or printed |
| `.superpowers/` | Not modified |

**Touched:**

- `apps/desktop/src-tauri/src/runtime_composition.rs`
- `apps/desktop/src-tauri/Cargo.toml` (`test-support` feature declaration)
- `docs/execution/2026-08-09-path-validation-robustness-report.md`

---

## 9. Commands and exit codes

| Command | Exit | Notes |
| --- | --- | --- |
| `export CARGO_TARGET_DIR=<repo>/target-probe` | n/a | PowerShell: `$env:CARGO_TARGET_DIR=...` |
| `cargo test -p pillow-desktop --lib -- runtime_composition` | **0** | 32 passed |
| `node scripts/run-worker-gate.mjs` | **1** | Failed at `cargo-fmt` **only** on concurrent `crates/**` (see below). Our desktop file is fmt-clean. |
| `cargo fmt -p pillow-desktop` | **0** | Applied to whitelist crate only |
| `cargo clippy -p pillow-desktop --all-targets -- -D warnings` | **101** | Failed compiling **dependency** `keeper-storage-sqlite` (`clippy::derivable_impls` in `crates/storage-sqlite/src/knowledge.rs`) — concurrent worker tree, not this ticket |
| `corepack pnpm --filter @pillow/desktop build` | **0** | Vite production build ok |
| Desktop E2E | **not run** | Per hard ban |

### Concurrent gate failures (whitelist-external)

`cargo fmt --all -- --check` reported diffs only in:

- `crates/knowledge-indexer/src/audit.rs`
- `crates/storage-sqlite/tests/knowledge_repository.rs`

Clippy dependency failure:

- `crates/storage-sqlite/src/knowledge.rs` (`ListDocumentsQuery` Default impl)

These match the brief’s **BLOCKED-BY-CONCURRENT** exception.

---

## 10. Residual risks

1. **Flake may still appear** if the real cause is not transient metadata failure
   (e.g. path never created, wrong settings key, race deleting profile). This
   change only makes diagnosis honest and absorbs short IO jitter.
2. **Indeterminate still degrades** after ~40 ms — intentional fail-closed;
   prolonged FS outages still turn conversation off until restart/fix.
3. **PATH discovery** (`find_grok_on_path`) still uses `is_file()` best-effort;
   only **configured** exe/profile paths got the three-way treatment.
4. Frontend has no dedicated copy for the new codes; falls through generic
   guidance until a later UI ticket (frontend out of whitelist).
5. Full workspace gate remains red until parallel workers land clean
   `crates/**` fmt/clippy.

---

## 11. Completion markers

```text
PATH-VALIDATION-ROBUSTNESS-BLOCKED: BLOCKED-BY-CONCURRENT — worker gate fails only on parallel workers' crates/** (knowledge-indexer fmt; storage-sqlite clippy), not on path-validation changes
```

Implementation deliverables for this ticket (three-way probe, retries, new
codes, inject gating, seven tests, package-local green tests + desktop build)
are done under the write whitelist.
