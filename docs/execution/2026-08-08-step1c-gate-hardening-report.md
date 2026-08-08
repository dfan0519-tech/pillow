# Step 1c — Gate hardening report

**Date:** 2026-08-08  
**Worker:** Grok (worker slot not assigned at dispatch; Keeper bridge boot returned HTTP 400)  
**Nature:** Gate / scripts / docs only. No product `src/**` edits. No git commit. No desktop E2E. No live ACP.  
**Completion signal:** `STEP1C-GATE-HARDENING-COMPLETE`

---

## 1. Task A — feature-gate blind spot

### 1.1 Direction chosen (and why)

**Primary fix:** enable `test-support` for `keeper-runtime-host` via a **package self-dep** in
`crates/runtime-host/Cargo.toml` `[dev-dependencies]`:

```toml
keeper-runtime-host = { path = ".", features = ["test-support"] }
```

**Reasoning:**

| Option | Pros | Cons | Verdict |
| --- | --- | --- | --- |
| Always pass `--features` in worker scripts | Easy | Easy to bypass with ad-hoc `cargo test -p` | Incomplete alone |
| Sole full-test script only | Process control | Discipline-only; trap remains for focused cargo | Incomplete alone |
| Remove feature gating from tests | Eliminates cfg trap | Forbidden: would touch product/test source or weaken seams | Rejected |
| **Self-dep + sole gate entry** | Fixes root cause for single-package *and* documents process | Cargo-idiomatic; release still leaves feature off | **Chosen** |

Self-dep is the standard Cargo pattern for “features required when this package is under test.”
It does **not** enable `test-support` for production library consumers (dev-dependencies only).

**Secondary process fix:** sole compliant Worker gate command + docs (below).

### 1.2 Sole gate entry

| Entry | Command |
| --- | --- |
| **Canonical** | `pnpm check` |
| Alias | `pnpm gate` |
| Direct | `node scripts/run-worker-gate.mjs` |

Documented as the **only** compliant Worker gate in:

- `docs/verification/worker-gate-entry.md`

Pipeline (order):

1. `node scripts/check-workspace.mjs`
2. `node scripts/check-secrets.mjs`
3. `pnpm typecheck`
4. `pnpm test`
5. `node scripts/run-cargo.mjs test --workspace`
6. `node scripts/run-cargo.mjs clippy --workspace -- -D warnings`
7. `node scripts/check-acp-protocol-drift.mjs` (soft-skip offline)

### 1.3 Empirical count — `governed_write_bridge`

| Invocation | Before fix | After fix |
| --- | --- | --- |
| `cargo test -p keeper-runtime-host --test governed_write_bridge -- --list` **without** `--features` | **16** tests | **19** tests |
| Same with `--features test-support` | 19 tests | 19 tests |
| Workspace path (feature unification from desktop dev-dep) | 19 tests | 19 tests |

Feature-gated tests now present without explicit `--features`:

- `listing_past_expiry_never_returns_grant_and_wakes_waiter`
- `storage_failure_after_live_never_fabricates_terminal`
- `direct_diff_at_expiry_returns_not_live_and_wakes_waiter`

Related: `governed_process_bridge` also compiles its `test-support` expiry case
(`expiry_returns_expired_without_spawn`); focused run **20 passed**.

**No tests deleted or weakened.**

### 1.4 Files (Task A)

| Path | Change |
| --- | --- |
| `crates/runtime-host/Cargo.toml` | self-dep with `features = ["test-support"]` + comments |
| `scripts/run-worker-gate.mjs` | **new** sole gate orchestrator |
| `package.json` | `check` / `gate` → `run-worker-gate.mjs`; add `check:acp-drift*` |
| `docs/verification/worker-gate-entry.md` | **new** sole-entry contract |

---

## 2. Task B — ACP protocol drift detection

### 2.1 Baseline file

**Path:** `docs/references/acp-protocol-lock-baseline.json`

**Format (schemaVersion 1):**

```json
{
  "schemaVersion": 1,
  "description": "...",
  "source": {
    "repository": "https://github.com/xai-org/grok-build",
    "lockfilePath": "Cargo.lock",
    "baselineCommit": "5da6962e4adb9c857f3def762542b52b4ec3e522",
    "verifiedIdenticalAtHead": "afbc0fb710320c7add294c2106d447ecc3e3af2e",
    "checkRef": "main",
    "captured": "2026-08-08",
    "evidence": "docs/execution/2026-08-08-upstream-grokbuild-audit.md"
  },
  "crates": [
    {
      "name": "agent-client-protocol",
      "version": "0.10.4",
      "source": "registry+https://github.com/rust-lang/crates.io-index",
      "checksum": "10eeef5e80864f9c3c148a3f395c3e35a66d37ec7561c7845b2bffae8e841759"
    },
    {
      "name": "agent-client-protocol-schema",
      "version": "0.11.4",
      "source": "registry+https://github.com/rust-lang/crates.io-index",
      "checksum": "ca68e7e55681ce56546c0cecc6bc8f20493d24b44c6d93ec46174f310730bba2"
    }
  ]
}
```

Values match the 2026-08-08 upstream audit (baseline and HEAD `Cargo.lock` byte-identical for these crates).

### 2.2 Script behavior

**Path:** `scripts/check-acp-protocol-drift.mjs`

| Mode | Behavior | Exit |
| --- | --- | --- |
| Default (online) | HTTPS GET of upstream `Cargo.lock` as **text only** (`node:https`, no clone/build/install/script execution) | `0` if match; `1` if drift |
| `--offline` | Print `ACP_DRIFT_CHECK: SKIPPED (未核对): offline flag set` | `0` |
| Network failure | Print `ACP_DRIFT_CHECK: SKIPPED (未核对): network unavailable or fetch failed (...)` | `0` |
| Drift | Print `ACP_DRIFT_CHECK: DRIFT` + per-field differences | `1` |
| `--lockfile <path>` | Compare against local lock text (fixture / offline lab) | `0`/`1` |
| `--self-test` | Parse, match, version drift, checksum drift, offline skip, fixture ok/drift, URL | `0` |

Markers (never silent “verified”):

- `ACP_DRIFT_CHECK: OK`
- `ACP_DRIFT_CHECK: DRIFT`
- `ACP_DRIFT_CHECK: SKIPPED (未核对)`

### 2.3 Proof that drift is detected

1. **Self-test** mutates version `0.10.4 → 0.99.0` and checksum → all-`f` → both reported as drift.
2. **Manual baseline mutation:** wrote temp baseline with `agent-client-protocol` version `0.0.0-mutated`, compared to real upstream lockfile:

```text
ACP_DRIFT_CHECK: DRIFT: upstream Cargo.lock diverges from baseline
  - agent-client-protocol: version 0.0.0-mutated -> 0.10.4
```

exit code **1**.

3. **Online OK path:**

```text
ACP_DRIFT_CHECK: fetching https://raw.githubusercontent.com/xai-org/grok-build/main/Cargo.lock
ACP_DRIFT_CHECK: OK: agent-client-protocol@0.10.4, agent-client-protocol-schema@0.11.4
```

exit code **0**.

---

## 3. Required gates — exit codes

| Command | Exit | Notes |
| --- | --- | --- |
| `node scripts/check-secrets.mjs` | **0** | 9 patterns self-tested, no findings |
| `node --check scripts/check-acp-protocol-drift.mjs` | **0** | |
| `node --check scripts/run-worker-gate.mjs` | **0** | |
| `node scripts/check-acp-protocol-drift.mjs --self-test` | **0** | |
| `node scripts/check-acp-protocol-drift.mjs --offline` | **0** | 未核对 |
| `node scripts/check-acp-protocol-drift.mjs` (online) | **0** | OK |
| Mutated baseline vs lock | **1** | expected DRIFT |
| `node scripts/run-cargo.mjs test -p keeper-runtime-host --test governed_write_bridge` | **0** | **19** passed |
| `node scripts/run-cargo.mjs test -p keeper-runtime-host --test governed_process_bridge` | **0** | **20** passed |
| `node scripts/run-cargo.mjs test --workspace` | **0** | run with isolated `CARGO_TARGET_DIR=target-step1c-gate` after default `target/` hit concurrent LNK1104 from other cargo processes on the machine |
| First default-`target/` workspace attempt | **101** | `LINK : fatal error LNK1104` on `governed_process_bridge-*.exe` while other agents held the build dir — **not** a product failure |
| `node scripts/run-cargo.mjs clippy --workspace -- -D warnings` | **0** | isolated target dir |
| `git diff --check` (scoped to this task’s paths) | **0** | |
| `git diff --cached --name-only` | empty | staging clean |

Full `pnpm check` end-to-end (including renderer typecheck/tests) was **not** re-run as a single process in this slot because concurrent agents saturated the default cargo target; the Rust half of the gate was proven via the same `run-cargo.mjs` commands the gate invokes.

---

## 4. Whitelist compliance

Touched only:

- `scripts/**` (new gate + drift scripts)
- root `package.json` (scripts section)
- `crates/runtime-host/Cargo.toml` (feature/dep only)
- `docs/references/acp-protocol-lock-baseline.json`
- `docs/verification/worker-gate-entry.md`
- this report

**Not** modified: `crates/*/src/**`, `apps/desktop/src/**`, `apps/desktop/src-tauri/src/**`.

---

## 5. Residual risks

1. **Process discipline still matters for non-gate commands.** Self-dep closes the runtime-host single-package trap; other future feature-gated packages would need the same pattern or the sole gate.
2. **ACP drift is lockfile-level only.** Upstream can change agent behavior without bumping `agent-client-protocol`; live acceptance remains the release gate for behavioral drift (out of scope here).
3. **Offline soft-skip.** A long offline period can leave the pin unchecked; the gate prints 未核对 but stays green.
4. **Concurrent workspace cargo** on a shared `target/` can still produce Windows `LNK1104` flakes; retry or isolated `CARGO_TARGET_DIR` is operational hygiene, not a logic bug.
5. **Working tree pollution from other workers** (unrelated WIP on product paths) existed during this task; this worker did not stage/commit/reset those files. One mid-task overwrite of `Cargo.toml` was observed and re-applied; final tree was re-checked for the self-dep.

---

## 6. Explicit non-actions

- Did **not** execute upstream repository code (no install/build/scripts).
- Did **not** start a real ACP session or burn quota.
- Did **not** run desktop E2E / smoke (controller-owned).
- Did **not** `git add` / commit / stash / reset / clean / checkout restore.
- Did **not** read or print auth/token files.
- Did **not** delete or weaken any tests.
- Did **not** change product source under `src/**`.

---

## 7. Bottom line

- Feature blind spot closed at the Cargo package boundary **and** locked down by a sole documented Worker gate entry.
- ACP wire-crate lock is now machine-checkable with honest offline semantics.
- Empirical proof: single-package `governed_write_bridge` lists/runs **19** tests (was **16**).

`STEP1C-GATE-HARDENING-COMPLETE`
