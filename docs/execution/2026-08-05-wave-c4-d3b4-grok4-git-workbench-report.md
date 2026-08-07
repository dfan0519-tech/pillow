# C4 D3B4 Round 2B — Grok4 Git workbench composition report

Date: 2026-08-05  
Worker: grok4 (Grok CLI implementation worker)  
Task: `c4-d3b4-git-workbench`  
Brief: `docs/execution/2026-08-05-wave-c4-d3b4-grok4-git-workbench-brief.md`  
Design: `docs/superpowers/specs/2026-08-05-c4-git-changes-design.md`

## Keeper

- Attempted worker boot (`project: pillow`, `task: C4 D3B4 Round 2B Git workbench composition`, `agentName: grok4`, `agentRole: worker`) → HTTP 400 Bad Request (worker claims require a parent session id; none supplied by dispatcher).
- Did not impersonate the controller role. No completion post was attempted.
- Keeper used for record attempt only; no execution, judgment, Vault query, or prompt rewrite.

## Delivered

Mounted App-owned Git discovery/status/diff presentation onto the accepted
Round 2A contextual pane through Workbench pass-through, with strict project
and request-generation isolation.

### Changed files (allowlist only)

| Path | Change |
| --- | --- |
| `apps/desktop/src/main.tsx` | Construct exactly one `TauriGitGateway` and supply it to `App`. |
| `apps/desktop/src/App.tsx` | Optional `gitGateway`; own discovery/status/diff state; generation guards; build `GitChangesPaneModel`. |
| `apps/desktop/src/features/workbench/Workbench.tsx` | Optional `gitChanges` prop; pass unchanged to `ResourcePane`. |
| `apps/desktop/tests/c4-git-changes-integration.test.tsx` | **Created** focused composition suite (12 tests). |
| `docs/execution/2026-08-05-wave-c4-d3b4-grok4-git-workbench-report.md` | This report. |

No other paths were modified by this worker.

## State / authority decisions

1. **Single gateway instance** — `main.tsx` constructs one `TauriGitGateway` and
   passes it into `App`. Wire DTOs and public errors remain only in
   `gitGateway.ts`.
2. **App owns Git presentation** — repositories, selection, discovery
   generation, status, diff, loading, path-free error, and stale live in App.
   Workbench does not call Git and does not redeclare `GitChangesPaneModel`.
3. **Null model when inert** — With no active project or no `gitGateway`,
   `gitChanges={null}` and zero Git invokes. Existing resource / write-diff /
   terminal paths are unchanged.
4. **Discover on project selection** — Active project identity starts one
   discovery. Prefer available project-root repository id `.`, else the first
   available repository. Load status with the exact discovery generation. If
   none available: path-free `git_repository_not_found` and no status/diff.
5. **Synchronous clear on project switch** — Repositories, selection, status,
   diff, loading/error/stale clear before new discovery may settle.
6. **Controller-owned scope generation** — Monotonic generation bumps on
   project switch/clear, refresh, repository selection, and path selection.
   Every discovery/status/diff settle is guarded by captured project id +
   generation; late results are dropped without mutating visible state.
7. **Repository / path acceptance** — Repository selection accepts only an
   available id from the current discovery list; path selection accepts only a
   portable path present in the current status. Diff uses the current project /
   repository / discovery generation tuple. Never native paths.
8. **Refresh** — Invalidates prior requests, clears selected diff (and status
   presentation), re-discovers, and adopts only the new returned generation.
9. **Errors** — `normalizeGitError` only. Stale/mismatch → `stale=true` +
   fixed path-free message. Other codes → `stale=false`. Host error text is
   never echoed.
10. **No polling** — No timers, intervals, mutation, terminal Git, raw session
    ids, roots, executables, argv, cwd, or environment surfaces.
11. **Conversation remains dominant** — Existing resource-pane toggle opens
    the contextual pane; Changes is one mode with Terminal → write Diff →
    Changes precedence owned by `ResourcePane`.

## Gates

### Focused Vitest

```powershell
pnpm --filter @pillow/desktop exec vitest run tests/c4-git-changes-integration.test.tsx --no-cache
```

- **PASS**: 1 file, **12 tests**, exit 0.

### Typecheck

```powershell
pnpm --filter @pillow/desktop typecheck
```

- **PASS**: `tsc --noEmit`, exit 0.

### Diff check

```powershell
git diff --check -- apps/desktop/src/App.tsx apps/desktop/src/main.tsx apps/desktop/src/features/workbench/Workbench.tsx apps/desktop/tests/c4-git-changes-integration.test.tsx docs/execution/2026-08-05-wave-c4-d3b4-grok4-git-workbench-report.md
```

- **PASS**: exit 0 (CRLF normalization notices only; no whitespace errors).

### Staging

```powershell
git diff --cached --name-only
```

- **Empty** — nothing staged by this worker.

Desktop E2E was not run (brief forbids).

## Self-review against required behavior

| # | Requirement | Status |
| --- | --- | --- |
| 1 | No project / no gateway → no Git calls, `gitChanges=null` | Covered by tests; product path in App `useMemo` + effect. |
| 2 | Discover once; first available (prefer `.`); status with discovery gen; none available → path-free error | Covered. |
| 3 | Project switch clears Git content before new results | Covered with deferred B discovery. |
| 4 | Project id + monotonic generation guards late discovery/status/diff | Covered. |
| 5 | Available repo selection only; clear diff/error/stale; same discovery gen | Covered. |
| 6 | Path only from current status; exact project/repo/gen tuple; no native path | Covered. |
| 7 | Refresh invalidates, re-discovers, new generation only; stale/mismatch path-free | Covered. |
| 8 | No timer/poll/mutation/host echo | Product code has no timers; error normalization path-free. |
| 9 | Conversation dominant; existing pane toggle; ResourcePane precedence | Covered via Workbench + Terminal/Diff precedence test. |

## Residual risks for Round 3 real desktop Git E2E

- Real Tauri discovery/status/diff generation drift against temporary fixtures
  is not exercised here; only mock gateway deferred promises prove isolation.
- Resource pane still closes on project identity change (pre-existing Workbench
  behavior); users must re-open the pane after switching projects. Git state
  still loads for the active project regardless of pane open state.
- Nested multi-repo ordering and large dirty lists under production discovery
  bounds need fixture coverage in Round 3.
- Unsafe-config denial, detached HEAD, binary/oversized diffs, and restart
  recovery remain Round 3 host fixtures, not this worker’s scope.
- Full desktop suite / production build / process residual checks are
  controller gates.

## Tree hygiene

- HEAD not advanced; no stage, commit, reset, clean, restore, or unrelated
  reformat.
- Only allowlisted Git workbench paths were edited/created by this worker.
- Pre-existing dirty/untracked work elsewhere in the tree was left untouched.
- Staging remains empty from this worker.

## Completion

C4 D3B4 Round 2B Git workbench composition delivered for controller review.

`C4-D3B4-GROK4-GIT-WORKBENCH-COMPLETE`

---

## Narrow repair (Round 2B rework)

Date: 2026-08-05  
Worker: actual Grok CLI slot `grok4`  
Controller prior verdict: **REJECTED** pending this repair.

### Keeper

- Attempted worker boot once (`project: pillow`, `task: C4 D3B4 Round 2B Grok4 narrow repair…`, `agentName: grok4`, `agentRole: worker`) → HTTP 400 Bad Request (worker claims require a parent session id).
- Did not impersonate controller. Continued implementation without Keeper execution/judgment.

### Allowlist-only changes

| Path | Change |
| --- | --- |
| `apps/desktop/src/App.tsx` | Render-time Git ownership guard on `gitChanges` build. |
| `apps/desktop/tests/c4-git-changes-integration.test.tsx` | Ownership regression + arbitration fixture repair. |
| `docs/execution/2026-08-05-wave-c4-d3b4-grok4-git-workbench-report.md` | This narrow-repair section. |

No other paths modified. Nothing staged or committed.

### Fix 1 — Render-time owner guard

**Finding:** The project-change effect updates `gitScopeRef` and clears `gitState` only after React commits the render for a new `activeProject`. During that one paint, `gitChanges` could still be built from the prior project's `gitState`.

**Repair:** In the `gitChanges` `useMemo`, require `gitScopeRef.current.projectId === activeProject.id` before constructing a model. Otherwise pass `null`. Initial selection remains `null` until the project-change effect establishes the new scope and loading state. No polling, layout effects, native paths, or second project state machine.

**Regression:** Integration test records every Workbench render as `{ projectId, gitChanges }` via a thin wrapper mock. While Project B is the active project and B discovery is still deferred, no supplied model may carry Project A status/diff markers (including the first paint before B's effect owns the scope). DOM-only assertions are not used as the sole proof.

### Fix 2 — Approval arbitration fixture

**Finding:** Precedence test listed one actionable write and one actionable process request, then waited for `write-approval-bar`. Accepted arbitration chooses by earlier expiry, then opaque request id; with near-equal `Date.now()` expiries, `proc-prop-1` < `write-prop-1` so process legitimately won.

**Repair (test only; arbitration/product untouched):**
- Write-Diff phase: **write-only** gateway (empty process list).
- Terminal phase: **process-only** gateway (empty write list).
- Fixed ISO expiries under a pinned `vi.useFakeTimers` wall clock (`2026-08-05T10:00:00.000Z` now; write `12:00Z`, process `12:30Z`) so deadlines stay live without `Date.now()` arbitration races or setTimeout overflow.
- Assertions preserved: write Diff outranks Git Changes; Terminal outranks Diff and Changes.

### Fresh gates (serial)

```powershell
pnpm --filter @pillow/desktop exec vitest run tests/c4-git-changes-integration.test.tsx --no-cache
```

- **PASS**: 1 file, **13 tests**, exit **0**.

```powershell
pnpm --filter @pillow/desktop exec vitest run tests/approval-arbitration.test.ts tests/c4-git-changes-integration.test.tsx --no-cache
```

- **PASS**: 2 files, **20 tests** (7 arbitration + 13 integration), exit **0**.

```powershell
pnpm --filter @pillow/desktop typecheck
```

- **PASS**: `tsc --noEmit`, exit **0**.

```powershell
git diff --check -- apps/desktop/src/App.tsx apps/desktop/tests/c4-git-changes-integration.test.tsx docs/execution/2026-08-05-wave-c4-d3b4-grok4-git-workbench-report.md
```

- **PASS**: exit **0**.

```powershell
git diff --cached --name-only
```

- **Empty** — nothing staged by this worker. Exit **0**.

### Residual risk

- Ownership guard depends on the project-change effect always pairing `gitScopeRef` update with a `setGitState` that re-renders; if a future path updated scope without a state write, the memo would not re-evaluate ownership until another dep changed. Current App paths always set state with the scope bump.
- Workbench prop log is test-only instrumentation; production still has no DOM contract for closed resource panes after project switch.
- Real Tauri Git E2E and full desktop suite remain controller/Round 3 gates.
- Fake timers are scoped to the single precedence test and restored in `finally`.

### Staging / hygiene

- HEAD not advanced; no stage, commit, reset, clean, restore.
- Only the three allowlisted paths above were touched in this repair.
- Unrelated dirty/untracked work left untouched.

### Repair completion

`C4-D3B4-GROK4-GIT-WORKBENCH-REPAIR-COMPLETE`
