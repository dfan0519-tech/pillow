# C4 D3B3 — `cancel-tree` 真根因定位与修复报告

**Worker:** grok-worker-d3b3  
**Controller session:** `<session-id>`  
**Worker session:** `<session-id>`  
**Date:** 2026-08-07  
**Worktree:** `<repo>`  
**Scope:** Runtime-evidence root cause for intermittent `composer-cancel` timeout in `cancel-tree`; harness-only fix if classified as harness fragility.

**Completion signal:** `C4-D3B3-CANCEL-TREE-ROOTCAUSE-COMPLETE`

---

## 0. Prior state (not re-litigated)

- Disproven as *this* failure’s root: `conversations_loaded` out-of-order list race. Real race, fixed via per-project `listSeq` + strict `>` discard (`App.tsx` / `conversationReducer.ts`), unit 93/93. **Fix retained; not modified this round.**
- Historical RED: `cancel-tree: getByTestId('composer-cancel') waitFor visible 60s timeout` as sole C4 checkpoint blocker (controller fact).

---

## 1. E2E runs (exclusive authorized command only)

Command (all runs):

```bash
corepack pnpm --filter @pillow/desktop exec node --test --test-concurrency=1 \
  tests/e2e/c4-governed-process.spec.ts
```

Environment note: first attempt failed before scenarios because `cmd.exe` could not find `pnpm` (only `corepack`). Fixed for this shell with `corepack enable` / `corepack prepare pnpm@…` so `pnpm` is on PATH. Not a product issue.

| # | Approx local time | Wall clock | Suite | `cancel-tree` | Notes |
| --- | --- | --- | --- | --- | --- |
| 0 | ~16:57 | ~5s | FAIL (infra) | n/a | `pnpm` not on PATH for nested `cmd.exe` build |
| 1 | ~16:58–17:05 | ~445s | FAIL | **PASS** | Failed later: `two-project-isolation` waiting for `command-approval-bar` 60s. Full cancel-tree debug timeline in stdout (see §2). |
| 2 | ~17:05–17:12 | ~411s | **PASS** | **PASS** | `msApproveToCancelStart≈20560` (still had 20s running wait) |
| 3 | ~17:12–17:19 | ~402s | **PASS** | **PASS** | same 20s pattern |
| 4 | ~17:20–17:27 | ~400s | **PASS** | **PASS** | **After harness fix:** `msApproveToCancelStart=550`, `markersBefore=2`, `cancelClicks=2` |
| 5 | ~17:27–17:35 | ~463s | **PASS** | **PASS** | Confirm post-fix; full suite green again |

**`cancel-tree` RED this round: 0 / 5 scenario attempts.** Per task rule: **本轮未复现** the historic `composer-cancel` 60s timeout. Conclusions below use green-path runtime dumps + product event ordering, not a captured RED dump of a missing cancel button.

Residuals after each completed run: `pillow.exe = 0`, port `1420` listeners = 0 (supervisor-owned cleanup).

Artifacts:

- Logs: `.superpowers/c4-cancel-tree-debug/e2e-run{1,2,3,4-after-fix,5-confirm}.log`
- Run1 stdout (richest): terminal capture of `[cancel-tree-debug]` marks + suite failure record
- Acceptance record (last green): `.superpowers/c4-process-acceptance/c4-process-acceptance-record.json`

---

## 2. Answers to the five runtime questions

### Q1 — At step 6 (`cancelOwningRun`), what is `run.status` / `selectActiveRun`?

**Answer (green path, high confidence for green; no RED sample):**  
Direct React state is not exposed to Playwright. Product DOM gate:

- `composer-cancel` mounts **iff** `canCancel === isRunActive(run)` under ownership (`Composer.tsx` / `Workbench.tsx` / `App.tsx`).
- `isRunActive` ⇔ `pending | running | cancelling`.

Runtime dumps (run1):

| Phase | `composer-cancel.visible` | Process activity statuses | Selected conv rows |
| --- | --- | --- | --- |
| after-send | **true** | `["待批准"]` | 1 |
| after-approval-visible | **true** | `["待批准"]` | (approval bar live) |
| after-approve | **true** | `["待批准"]` | 1 |
| after-running-wait (~20s) | **true** | `["待批准"]` | 1 |
| before-cancelOwningRun | **true** | — | markersBefore=2 |

**Inference:** `selectActiveRun` non-null and active at cancel start on the green path.  
**Confidence:** high for green path; **N/A for RED** (not reproduced).

### Q2 — Did `composer-cancel` never appear, or appear then vanish?

**Green path (3+ runs):** Appeared by after-send and **remained visible** through approve and the (then) 20s wait; `cancelOwningRun` saw `everVisibleDuringWait=true` immediately (`tMs≈4`).

**RED path (historic):** Prior diagnosis argued never-visible for full 60s wait (Playwright `waitFor({state:"visible"})` succeeds on first paint). **This worker did not capture a RED dump** confirming that under listSeq-fixed code.

**Confidence:** high for green; medium that historic RED was “never visible during wait” vs “appeared then gone” (prior static diagnosis preferred never-visible / ownership-null).

### Q3 — Approve → run terminal duration vs 20s wait + process scan?

| Metric | Run1 (pre-fix) | Run4 (post-fix) |
| --- | --- | --- |
| `waitForCommandActivity(运行中, 20s)` | **always fail**, `elapsedMs≈20034` | **removed** |
| Process scan (`c4p-child`) | ~0.5s, `markersBefore=2` | ≤5s poll, `markersBefore=2` |
| Approve → cancel start | **~20600 ms** | **~550 ms** |
| Cancel click path | `clicks=2` (5s first-click settle + second click) | `clicks=2` |

Body text during the failed “运行中” wait still showed agent activity “运行中” and process row “待批准” + submitting approval bar — i.e. the **turn** was live; the **process activity row** was not labeled “运行中”.

**Confidence:** high (measured).

### Q4 — How long does `long-tree` keep the process alive?

Fake script (`fake_acp_agent/scripts.rs` ~907–912): parent `-e` script spawns `/*c4p-child*/setInterval(()=>{},1e9)` then parks with another `setInterval`. Lives until host cancel / **fixed `PROCESS_TIMEOUT = 60_000ms`**.

Runtime: `markersBefore=2` consistently before cancel ⇒ child tree was live at cancel start. After cancel, residual markers forced to 0 by scenario.  
Does **not** naturally exit in the first ~20–30s. Can reach host timeout only if cancel is never issued before 60s.

**Confidence:** high.

### Q5 — Product defect or harness fragility? **Must classify.**

## **Classification: harness timing fragility (primary), with product event-ordering nuance**

### Why product cancel behavior is correct (not a “user cannot cancel while process runs” defect)

1. `canCancel` is bound to **run** activity (`isRunActive`), not process activity status.
2. Green-path dumps prove `composer-cancel` is visible **from accepted send**, through approval, through approve, while execute is still blocking (`approveDisabled: true`, process row still `待批准`).
3. Cancel succeeded (`markersBefore=2` → residual 0, suite green including cancel-tree).
4. Therefore: **during a governed long process, the user *can* cancel via Stop.** This is not “product forbids cancel mid-execute.”

### Why the old harness was fragile

1. After approve it waited up to **20s** for process activity status **「运行中」**.
2. Adapter emits `process_started` / `process_terminal_outcome` **only after** `execute_governed_process` returns (`prompt.rs` ~713–742). For long-tree, that is cancel-or-timeout later — **not** mid-flight.
3. Runtime: process row stays **「待批准」** for the entire pre-cancel window; the wait **deterministically** burns ~20s (3/3 instrumented pre-fix runs, `ok:false`, `elapsedMs≈20000`).
4. That delay is not the intermittent variable by itself, but it **pushes cancel ~20s later**, closer to the fixed 60s process timeout and any other UI races (selection ownership, turn completion edge cases). Historic intermittent RED fits “cancel looked too late / run no longer active or ownership lost,” not “Stop never implemented.”
5. Correct harness assertion: prove run-active cancel gate (`composer-cancel`) while the tree is live, click cancel, prove terminal + child cleanup — **not** wait for a process-row status that product cannot paint mid-flight.

### What we did **not** claim

- We did **not** re-prove the list-race RED under listSeq-fixed code (not reproduced).
- We did **not** authorize or change product emission order for `process_started` (observability improvement would be a separate product design).

**Confidence:** high that 20s「运行中」wait is harness-wrong; medium that it was the sole intermittent trigger of historic RED (list race was also real and is fixed).

---

## 3. Harness fix (implemented)

### Files (whitelist only)

| Path | Change |
| --- | --- |
| `scripts/wave-c4-governed-process-acceptance.mjs` | `scenarioCancelTree`: remove 20s process「运行中」wait; assert `composer-cancel` after send and after approve; short ≤5s marker poll; failure dump to `.superpowers/c4-cancel-tree-debug/` |
| `scripts/lib/c4-governed-process-acceptance.mjs` | `dumpCancelTreeUiState`; `cancelOwningRun` optional debug dumps + visibility sampling on timeout |

### Why this is the right assertion

- Asserts the **product cancel gate** (run active under ownership), not an unreachable process-row label.
- Cancels ~**0.5s** after approve (run4: 550ms) instead of ~**20.5s**, while child markers still appear (`markersBefore=2`).
- Still requires cancelled/interrupted/failed terminal, child cleanup, approval absent, composer idle — **no** weakened authority/privacy/cleanup.

### Not done

- No timeout lengthening as the “fix.”
- No product source changes (`App.tsx`, reducer, Workbench, Composer, crates/**).
- No git commit/stash/reset.

### Failure scaffolding retained (why)

- `dumpCancelTreeUiState` + `cancelOwningRun({ debug })` dumps on timeout only.
- Writes under gitignored `.superpowers/c4-cancel-tree-debug/`.
- Reason: cancel-tree RED was **not** reproduced; if it returns, dumps answer Q1–Q2 without another instrument round.

---

## 4. Product patch draft (**not implemented** — not required for primary classification)

If controller later wants mid-flight process「运行中」in the activity row (observability only; not required for cancel):

1. Emit / stream a started signal **before** blocking `wait_for_resolution` / execute join, or push a local “approved → running” projection on successful decide accept before host terminal receipt.
2. Impact: adapter event ordering, durable sink semantics, replay/idempotency tests, UI process activity model.
3. **Do not** couple cancel availability to process activity status — keep `isRunActive(run)`.

No product patch applied this round.

---

## 5. Residual risks

| Risk | Notes |
| --- | --- |
| Historic cancel-tree RED not reproduced | listSeq fix may already have removed the dominant flake; 20s wait was residual amplifier |
| `two-project-isolation` flake | Run1 failed there after cancel-tree green; out of scope; suite later green |
| `cancelOwningRun` often `clicks=2` | First click may not clear bar within 5s while decide still blocks; second click path works; not failing |
| Process activity stays「待批准」mid-flight | Product observability gap; cancel still works |
| Full serial desktop gate (git + process + …) | Not run (forbidden); controller owns multi-suite acceptance |

---

## 6. Staging / git

- **No** `git add` / commit / stash / reset / clean / checkout / restore.
- Whitelist script files remain untracked/modified in the dirty worktree only.
- Did **not** touch `.superpowers/c4-git-acceptance/`.
- Did **not** kill processes by name/PID batch; cleanup via harness supervisor only.
- Did **not** read/print auth tokens.

---

## 7. Explicitly not done

- No product code edits.
- No revert of listSeq conversation-list race fix.
- No full `test:e2e`, no `test:e2e:c4:git`, no other wave acceptance scripts.
- No weakening of cleanup/privacy assertions.
- No fabricated RED dumps.

---

## 8. Controller review focus

1. Confirm `scenarioCancelTree` no longer waits for process「运行中」and asserts `composer-cancel` instead.
2. Re-run exclusive process suite serially multiple times (controller gate).
3. Optionally re-run full C4 serial checkpoint including git — worker was not authorized to.
4. If cancel-tree RED returns: inspect `.superpowers/c4-cancel-tree-debug/*-timeout.json` for `everVisibleDuringWait`, `selectedConversationRows`, `emptyStage`, `activityStatuses`.
