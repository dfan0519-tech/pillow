# Debt C — Git acceptance fixture hardening report

**Signal:** `DEBT-C-FIXTURE-COMPLETE`  
**Worker:** grok3  
**Controller:** Claude  
**Date:** 2026-08-08  
**Scope:** Harness-only (`scripts/wave-c4-git-changes-acceptance.mjs`). No product source, no desktop E2E run by this worker.

## Summary

Two harness closures for C4 checkpoint residual risk §5.5:

1. **Removed the `vendor/` `.gitignore` bypass** so Project A root status E2E actually covers untracked directory porcelain (`? vendor/` with trailing slash).
2. **Isolated fixture host-git from the user global gitconfig** via `GIT_CONFIG_GLOBAL` pointing at an empty temp file.

Controller must re-run the full Git changes acceptance suite exclusively; this worker did not invoke any `scripts/wave-*-acceptance.mjs` or `test:e2e*`.

---

## 1.1 Remove `vendor/` gitignore bypass

### What changed

| Location | Before | After |
| --- | --- | --- |
| `buildProjectAFixture` | Wrote `.gitignore` with `vendor/\n` and committed it | No `.gitignore`; nested repo under `vendor/nested` left untracked on parent |
| `resetRootToCleanMain` | Re-wrote `.gitignore`, re-committed if dirty | Still `clean -fd -e vendor` (nested tree preserved); **no** gitignore restore |
| Product path exercised | Hidden: root looked clean | Real: root reports untracked directory `vendor/` |

`clean -fd -e vendor` is retained so nested `.git` under `vendor/nested` is never deleted by fixture reset.

### Scenario assertion changes (not weakened)

Exact dirty counts replace floors that would still pass if `vendor/` vanished again. Each Project A dirty scenario also asserts the untracked directory entry where relevant.

| Scenario | Before | After | Why not a weaken |
| --- | --- | --- | --- |
| **nested-repository** | Root `waitForGitClean` (twice) | Root exact **dirtyCount === 1**, entry `未跟踪` + `vendor/`, **error count === 0** (on first paint and after reselect `.`); nested still clean | Was the gitignore lie. Now RED→GREEN for trailing-slash status: status must paint and not fail |
| **modified-untracked** | `minDirty: 2`, note + untracked-new only | exact **3**: modified `note.txt` + untracked `untracked-new.txt` + untracked `vendor/` | Count raised and set membership includes the directory path |
| **rename** | `minDirty: 1` | exact **2**: renamed + `vendor/` | Floor 1 would still pass with only vendor; now requires rename **and** vendor |
| **conflict** | `minDirty: 1` | exact **2**: conflict + `vendor/` | Same strengthening |
| **text-diff** | `minDirty: 1` | exact **2** + assert `vendor/` visible before selecting `note.txt` | Same |
| **binary-oversized-diff** | `minDirty: 2` | exact **3** + assert `vendor/` | Same |
| **generation-drift** (pre-refresh) | `minDirty: 1` | exact **3** + assert `vendor/` | gen-marker + note + vendor |
| **restart-recovery** (both sessions) | `minDirty: 1` | exact **2** + assert restart-marker **and** `vendor/` | Same |

Unchanged on purpose:

| Scenario | Notes |
| --- | --- |
| **project-root-clean** | Still uses **Project B** (truly clean single-root). Comment already stated A is not a clean fixture. |
| **detached-head** | Branch/head labels only; does not assert clean. Root may show `vendor/` dirt without breaking the scenario. |
| **unsafe-config-denial** | Project Unsafe; no vendor tree. |
| **project-switch-isolation** | Still forbids A markers on B / B markers on A; `vendor/nested` remains an A-only foreign token on B. |
| **generation-drift** (post-rebuild) | Still forbids stale `gen-marker-a` / `generation-a-body`; rebuilt root may still show `vendor/` (nested tree kept). |

### RED→GREEN evidence (nested-repository)

Explicit assertions in `scenarioNestedRepository`:

1. After open on Project A root: `dirtyCount === 1` and entry kind `未跟踪` path `vendor/`.
2. `git-changes-error` is **absent** (status did not fail the whole call).
3. Nested `vendor/nested` remains clean and selectable.
4. Reselect root `.` → same dirty count, same untracked directory, still no error.

This is the harness counterpart to product regressions in `crates/git-executor` (`untracked_directory_entry_keeps_trailing_slash`, `untracked_directory_does_not_abort_the_whole_status`, etc.).

---

## 1.2 Fixture Git environment isolation

### Implementation

`fixtureGitEnv()` now sets:

- existing: `GIT_TERMINAL_PROMPT=0`, `GIT_CONFIG_NOSYSTEM=1`, author/committer name+email
- **new:** `GIT_CONFIG_GLOBAL=<tmpdir>/pillow-c4-git-fixture-gitconfig/gitconfig`

`ensureFixtureGlobalGitConfig()` creates that path once and always writes **empty** content so ambient keys cannot linger. Fixture git never opens or mutates the user’s real `~/.gitconfig`. Local `git config user.*` inside each fixture repo (via `initRepo`) still applies.

### Intent

Stop host `core.autocrlf`, `init.defaultBranch`, filters, aliases, etc. from changing fixture blobs/hashes across machines. Probe on this host: without isolation, `git add` warned about LF→CRLF; with empty `GIT_CONFIG_GLOBAL`, status is clean of that warning.

---

## Files touched (whitelist only)

- `scripts/wave-c4-git-changes-acceptance.mjs` — fixture + scenarios + env isolation
- `docs/execution/2026-08-08-debt-c-fixture-hardening-report.md` — this report

**Not modified:** `scripts/lib/c4-git-changes-acceptance.mjs` (existing `waitForGitDirty` / `assertGitEntryVisible` sufficient), product `crates/**` / `apps/**`, `.superpowers/c4-git-acceptance/**`, governed-process acceptance scripts.

---

## Gates run by this worker

| Command | Exit code | Notes |
| --- | --- | --- |
| `node --check scripts/wave-c4-git-changes-acceptance.mjs` | **0** | Syntax OK |
| `node --check scripts/lib/c4-git-changes-acceptance.mjs` | **0** | Unchanged; still parses |
| `node scripts/check-secrets.mjs` | **0** | `secret gate ok — 9 patterns self-tested, no findings in tracked files` |
| `git diff --check` | **0** | No conflict/whitespace errors; host may print CRLF hints from ambient autocrlf (unrelated to fixture env) |
| `git diff --cached --name-only` | **0** | Empty output (index clean) |

Desktop E2E **not** run (channel reserved for controller).

---

## Expected controller full Git E2E outcomes

When controller re-runs `scripts/wave-c4-git-changes-acceptance.mjs` exclusively:

| Scenario | Expected paint / counts |
| --- | --- |
| project-root-clean | Project B: `0 项变更`, no repo selector |
| nested-repository | Root: **1 项变更**, entry `未跟踪 vendor/`, no error; nested clean; selector shows `vendor/nested`; reselect root still 1 |
| modified-untracked | **3 项变更**: `note.txt` 修改, `untracked-new.txt` 未跟踪, `vendor/` 未跟踪 |
| rename | **2 项变更**: rename + `vendor/` |
| conflict | **2 项变更**: `conflict.txt` 冲突 + `vendor/` |
| detached-head | Branch label `分离 HEAD`; may still show `vendor/` dirt |
| text-diff | **2 项变更**; unified diff for `note.txt` |
| binary-oversized-diff | **3 项变更**; binary / oversized summaries for blob + huge |
| unsafe-config-denial | Fixed public error; path-free |
| generation-drift | Pre-refresh **3** entries; post-rebuild no stale gen-marker content |
| project-switch-isolation | No A paths on B, no B paths on A |
| restart-recovery | Both sessions **2 项变更**: `restart-marker.txt` + `vendor/` |

Immutability manifests (index/HEAD/porcelain hash) for read-only scenarios must still hold: harness only observes via UI; fixture setup mutations are intentional and captured as before/after equal where designed.

---

## Residual risks

1. **Controller must re-run E2E** — this worker only static-checked the harness. UI path rendering of trailing slash `vendor/` is assumed from product code (`entry.path` as stored); if a future UI normalizer strips trailing slashes, `assertGitEntryVisible(..., "vendor/")` would fail loudly (desired).
2. **Exact dirty counts** assume porcelain normal untracked collapse to a single `vendor/` entry. If a future git default or product status flag switches to `-uall`, counts rise and scenarios fail (also desired: force intentional update).
3. **Global isolation** covers fixture `runFixtureGit` only. The **product** process still uses the host git environment for real status during E2E; that is correct product behavior and outside this debt.
4. **Temp global config path** under `os.tmpdir()` is process-local empty file; concurrent fixtures sharing the same path only see empty content (always rewritten empty).
5. Keeper bridge boot was attempted; endpoint returned 400 on this host. Session continued under explicit worker contract from controller.

---

## Signal

`DEBT-C-FIXTURE-COMPLETE`
