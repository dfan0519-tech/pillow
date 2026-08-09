# Publishable tree cleanup report

| Field | Value |
| --- | --- |
| Date | 2026-08-10 |
| Worker | grok-worker (cleanup; no slot claim recorded — bridge boot HTTP 400) |
| Mode | Working-tree only; **no git write operations** |
| `CARGO_TARGET_DIR` | `<repo>/target-cleanup` |
| Signal | **PUBLISHABLE-TREE-CLEANUP-COMPLETE** |

## 1. Goal (one line)

Remove or relocate content that is only meaningful on the maintainer machine, or that misleads external contributors — **without** rewriting git history and **without** changing product behavior.

## 2. Public entry files — how they stay self-contained

### 2.1 `CLAUDE.md` (public)

- Removed mandatory Keeper Bridge boot and absolute vault paths.
- Instructs agents to follow root `AGENTS.md` only.
- Points maintainers (optionally) at gitignored `.superpowers/RUNBOOK.md`.
- A clone of only this repo can build/test without being sent to a non-existent service.

### 2.2 `AGENTS.md` (public)

**Kept (true for any contributor):**

- Product order (IDE → knowledge → ops)
- Repository boundary (self-contained product; optional external refs read-only)
- Engineering rules (SQLite, adapters, approval, project-scope, tests)
- Multi-runtime isolation **principle** (no slot farm names, no claim/release API)
- Required gates + pointers to `pnpm check` / `worker-gate-entry.md`

**Moved (not discarded)** → `.superpowers/RUNBOOK.md`:

- Keeper record-only policy and boot/claim/release shape
- Bridge URL `http://127.0.0.1:7890` and boot JSON body
- Out-of-tree vault / legacy Keeper / zbbody layout notes
- Full Grok CLI slot isolation (`grok1–4`, `GROK_HOME` trap, outer `auth.json`)
- Local `CARGO_TARGET_DIR` concurrency notes

### 2.3 `README.md` (public)

- Short product pitch + Stage 1 status summary (no wave laundry list)
- Explicit: external session/vault tools are **not** required to build
- Build/check commands and design-authority map
- Link to `docs/README.md`

### 2.4 `.cursor/rules/_vault-shared.mdc`

- No absolute vault paths
- No forced QUERY/INGEST against an out-of-tree vault
- Points at root `AGENTS.md`; optional local runbook only if present

### 2.5 `docs/README.md` (new)

- Contributor map: ADR / specs / verification / research first
- Declares `execution/` as historical evidence, not onboarding
- Notes handoff is controller-owned

## 3. What was moved (not deleted)

| Content | From | To |
| --- | --- | --- |
| Bridge boot + claim/release + vault paths + slot isolation | `CLAUDE.md`, `AGENTS.md` | `.superpowers/RUNBOOK.md` (gitignored) |
| Forced vault Cursor rules | `.cursor/rules/_vault-shared.mdc` | Neutral public rule; private overlays remain local-only convention |

**Nothing operationally required by the controller was discarded.** Content that still lives only in `docs/handoff/**` is listed as controller action items in §7.

## 4. Product / test fixture literal changes

Only fixture **literals** were replaced. Assertions keep the same redaction / rejection semantics.

| File | Before (machine layout) | After (synthetic) | Assertion strength |
| --- | --- | --- | --- |
| `apps/desktop/tests/project-workbench.test.tsx` | `<workspace>/VaultSystem\pillow` | `C:\Users\Alice\Projects\pillow` | Unchanged project DTO shape/usage |
| `apps/desktop/tests/tauri-project-gateway.test.ts` | same | same Alice path | Unchanged invoke payload expectations |
| `apps/desktop/tests/agent-activity-disclosure.test.tsx` | `<repo>\…` absolute artifact + error paths | `C:\Users\Alice\Projects\pillow\…` | Still asserts absolute paths and secret filenames **do not** appear in DOM; still requires portable relative open path |
| `apps/desktop/tests/agent-activity-timeline-integration.test.tsx` | `<repo>\secret.rs` resource path | Alice Projects path | Privacy expectations unchanged (absolute paths redacted) |
| `apps/desktop/tests/tauri-git-gateway.test.ts` | `<repo>\secret` in reject list | Alice Projects path | Still expects `GitGatewayError` for absolute/backslash/traversal paths |

No assertion was removed, broadened to always-true, or converted from hard fail to soft warn.

## 5. Docs scrub (except `docs/handoff/**`)

### 5.1 Bulk rewrite (script under gitignored `.superpowers/scrub-publishable-tree.mjs`)

~133 tracked docs files first pass, plus follow-up fixes for double-escaped paths and bridge narrative.

| Pattern | Replacement |
| --- | --- |
| `<repo>`, `<repo>` | `<repo>` |
| `<workspace>/VaultSystem/keeper` and `<workspace>\VaultSystem\keeper` | `legacy Keeper (out of tree)` |
| vault / references / zbbody absolute or sibling paths | out-of-tree phrasing |
| `http://127.0.0.1:7890…`, `/api/agent-bridge/boot` | “local session bridge (optional; out of tree)” |
| `parentSessionId=…` farm contract lines | “local parent session id” / optional session note |

### 5.2 High-value rewrites (manual polish)

- `docs/decisions/ADR-0001`, `ADR-0002` — clean successor / identity without drive layout
- `docs/migration/legacy-capability-inventory.md` — source = out-of-tree legacy
- `docs/roadmap/2026-07-31-stage1-overall-status.md` — legacy frozen, not a product destination
- `docs/superpowers/specs/2026-08-08-settings-credentials-design.md` — drop Keeper slot-routing / AGENTS slot-section cross-ref
- Selected research boot footers

### 5.3 Intentionally **not** deleted

- Entire `docs/` tree kept (governance evidence)
- Project name `zbbody` as a **conceptual reference** in research/reports (no local absolute layout)
- Historical worker labels in filenames (`grokN`/`codexN`) — historical dispatch tags; called out in `docs/README.md`
- `docs/research/2026-08-09-pre-opensource-audit.md` — left intact as the audit evidence corpus (still contains private-surface inventory by design)
- `docs/handoff/**` — **untouched** (controller-owned; see §7)

### 5.4 Residual public-surface scan (post-cleanup)

After cleanup, `git grep` for `<workspace>`, `<workspace>`, `127.0.0.1:7890`, `agent-bridge/boot`, `parentSessionId` on tracked files **excluding** handoff + pre-opensource audit returned **no hits**.

`apps/**` has **no** remaining `VaultSystem` / `<workspace>` fixtures.

## 6. Git operations for the controller (worker did **not** run these)

```bash
# Stop tracking gitignored superpowers reports (files remain on disk)
git rm --cached .superpowers/sdd/grok-shell-prototype-report.md
git rm --cached .superpowers/sdd/wave-b1-ui-report.md

# Then stage intentional public-tree cleanup when ready (example; controller chooses commit shape)
# git add CLAUDE.md AGENTS.md README.md .cursor/rules/_vault-shared.mdc docs/ apps/desktop/tests/
```

Confirm `.gitignore` already contains `/.superpowers/` so those two paths do not reappear after untrack.

**No** `git add/commit/stash/reset/clean/checkout/restore/rm` was executed by this worker.

## 7. `docs/handoff/**` — recommendation list only (not modified)

| Path | Suggestion | Why |
| --- | --- | --- |
| `docs/handoff/2026-08-08-pillow-controller-handoff.md` | **Move out of public tree** (private ops repo or gitignored ops store) | Full claim/release, slot pool, account tiers, vault wiki absolute paths, bridge curl, container homes |
| `docs/handoff/2026-08-07-pillow-controller-handoff.md` | Same — **move private** | Worker routing + Keeper constraints + absolute paths |
| `docs/handoff/2026-08-01-pillow-controller-handoff.md` | Same — **move private** or heavy rewrite | Boot examples + sibling layout |
| `docs/handoff/2026-08-08-round5-tech-debt-status.md` | Low sensitivity; **may stay public** or fold into issues | Internal schedule, not protocol secrets |

After relocation, keep a short public stub or rely on `docs/README.md` so contributors are not linked into missing private manuals.

Extract durable farm ops into `.superpowers/RUNBOOK.md` (already seeded) + private handoff archive rather than re-expanding into tracked docs.

## 8. Gate results

```text
export CARGO_TARGET_DIR=<repo>/target-cleanup
node scripts/run-worker-gate.mjs
```

| Step | Result |
| --- | --- |
| `scripts/check-workspace.mjs` | ok |
| `scripts/check-secrets.mjs` | ok (9 patterns self-tested; no findings) |
| `cargo fmt --all -- --check` | ok |
| `pnpm typecheck` | ok |
| `pnpm test` (desktop Vitest) | ok (fixture path tests included) |
| Rust workspace tests + Clippy path via gate | ok |
| ACP protocol drift check | ok |
| **Overall** | **`PILLOW_WORKER_GATE: OK` — exit code `0`** |

Keeper Bridge boot (optional local): HTTP **400** — expected without parent session; work used task contract only.

Desktop E2E: **not run** (controller-exclusive).

## 9. What this worker did **not** touch

- Git history / any git write
- `docs/handoff/**`
- `scripts/**` (except executing gates)
- `tests/fixtures/**`
- `crates/**` product logic (no changes)
- `apps/**` except the five desktop test fixture files listed in §4
- `.superpowers/**` except creating `RUNBOOK.md` and a one-shot scrub helper (both gitignored)
- Auth/token files
- Concurrent workers’ out-of-whitelist trees (no conflict observed; gate green)

## 10. Residual risk

| Risk | Severity | Notes |
| --- | --- | --- |
| History still contains host user paths in 3 commits | Accepted by task | User will not upload history; no rewrite this ticket |
| `docs/handoff/**` still public if pushed as-is | **High for ops surface** | Controller must relocate before public remote |
| Historical `docs/execution/*` still mention worker labels (`grokN`/`codexN`) in filenames and some narrative | Low–medium | Explained in `docs/README.md`; not a live API |
| Pre-opensource audit doc still catalogs private surface | Low | Intentional research record; do not treat as contributor onboarding |
| Conceptual `zbbody` project name remains in research | Low | No local drive layout; verify third-party citation policy if desired |
| Crate names still `keeper-*` | Branding only | ADR-0002 already documents transitional names |
| Two `.superpowers/sdd/*` files still **tracked** until controller `git rm --cached` | Medium | Listed in §6 |

## 11. Changed surface inventory (by area)

| Area | Approx. dirty paths (working tree) |
| --- | ---: |
| `docs/execution/` | 112 |
| `docs/research/` | 14 |
| `docs/superpowers/` | 12 |
| `docs/verification/` | 10 |
| `apps/desktop/tests/` | 5 |
| `docs/decisions/` | 2 |
| Root entry + README + cursor rule + docs map + migration/roadmap | 8-ish |
| **Total ~** | **163** status lines |

Exact commit selection is controller responsibility.

---

**PUBLISHABLE-TREE-CLEANUP-COMPLETE**
