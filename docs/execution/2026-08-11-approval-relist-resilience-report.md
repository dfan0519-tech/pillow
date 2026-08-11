# Approval re-list resilience — a pending proposal could go permanently invisible

**Date:** 2026-08-11
**Scope:** `apps/desktop/src/features/workbench/Workbench.tsx`,
`apps/desktop/src/styles/write-approval.css`, one new test file
**Origin:** task #10, class B — the `two-project-isolation` mechanism from
`docs/execution/2026-08-08-flake-analysis-VERDICT.md`

## What was wrong

The approval bar is driven by a re-list that runs on three triggers: project or
conversation identity change, a refresh-token bump from a runtime event, and a
decision settling. There is deliberately **no polling**.

`relistApprovals` caught list failures, cleared every approval surface, logged
one line, and returned. No retry.

That is survivable for most events, because another event follows and bumps the
token again. **It is not survivable for a proposal.** A pending proposal is the
last event of a turn — the agent is blocked waiting for the human, so no further
lifecycle event is coming. The sequence is:

1. Agent proposes a write or a command; host persists it and emits once.
2. App bumps the token; Workbench re-lists.
3. The list call fails transiently.
4. Every approval surface is cleared. `approvalsUnavailable` did not exist, so
   the stage renders exactly like an idle one.
5. **Nothing ever re-lists.** The proposal stays durably pending on the host,
   the agent stays blocked, and the UI shows a calm empty composer.

The only escape was switching conversations, which the user has no reason to
suspect. This is the same shape as the three DTO-seam defects in the handoff
§5: something silently fails to appear, and the gate is green.

## What changed

**Retry, then report.** `relistApprovals` now retries up to
`APPROVAL_LIST_ATTEMPTS` (3) with delays `[250ms, 1000ms]`, each retry
re-checking the generation guard so a scope change still aborts. On success the
alarm clears. When every attempt fails, the surfaces are cleared **and**
`approvalsUnavailable` is set, which renders a banner in the composer stack:

> **无法读取待批准项**
> 如果代理正在等待你批准,这里暂时看不到。请重试;重试无效时重新打开会话。 [重试]

The banner never fabricates a bar — there is nothing to approve until a real
list succeeds. It only says the list is unreadable, which is the honest state.

**Exhaustion is keyed off the attempt counter, not the delay table.** The first
implementation decided "last attempt" by asking whether a retry delay existed at
that index. The mutation probe below caught what that costs: shrink the attempt
budget below the delay table and the loop falls out reporting nothing —
silently reintroducing the exact defect being fixed. The counter is now the only
authority and the delay table has a fallback.

## Evidence

New file `apps/desktop/tests/approval-relist-resilience.test.tsx`, 6 tests.
Call counts are asserted, not just DOM state — DOM state alone cannot tell a
retry apart from a slow first call.

| Test | Pins |
| --- | --- |
| recovers a transient failure without alarming | 2 list calls, bar present, no banner |
| survives failures right up to the budget | 3 list calls, bar present, no banner |
| says the list is unreadable | banner present, **no bar**, exactly 3 calls, no grant in DOM |
| recovers when the user retries | banner → click 重试 → bar, banner clears |
| blocks the retry button while retrying | button disabled + 重试中… mid-flight, exactly 6 calls after one manual retry |
| does not alarm when readable and empty | 1 call, no banner, no bar |

The retry button is disabled while a retry is in flight. Without that, repeated
clicks stack overlapping retry loops and multiply list calls against a host
that is already failing — the call-count assertion is what pins it, since a
disabled attribute alone would still pass if the loops overlapped.

### Mutation probe

Both directions were verified by mutating the fix and confirming the right
tests redden — a suite that only asserted the banner renders would be false
green.

| Mutation | Result |
| --- | --- |
| `APPROVAL_LIST_ATTEMPTS = 1` (retry removed) | the 2 retry tests red; manual-retry and no-alarm tests stay **green** |
| `setApprovalsUnavailable(true)` removed | the 2 reporting tests red; retry tests stay **green** |

The mutations redden disjoint sets, so the tests are pinning two independent
properties rather than one thing twice.

## Verification

| Command | Exit |
| --- | --- |
| `corepack pnpm typecheck` | 0 |
| `corepack pnpm --filter @pillow/desktop test` | 0 — **517 passed** (was 511) |
| `node scripts/run-worker-gate.mjs` | 0 — `PILLOW_WORKER_GATE: OK` |
| `corepack pnpm desktop:test:e2e` | recorded at acceptance |

A first E2E run was started and then **killed on purpose**: the tree was edited
after it launched, and accepting a run against a tree that was still being
written is one of this project's recorded pitfalls. Gate and E2E were re-run
sequentially on the frozen tree instead.

No host DTO changed, so there is no renderer seam to widen — the closed key
allowlist in `tauriExecGateway.ts` is untouched.

## Residual risks

1. **The listener-ready window is still open.** `subscribe()` in
   `tauriConversationGateway.ts` registers `listen()` fire-and-forget; an event
   emitted before registration resolves is lost. Judged low risk and **not**
   fixed here: the host only emits proposal events in response to a turn the
   user types, and every identity change re-lists, so the list is the safety
   net. This fix hardens that net rather than closing the window.
2. **Retry budget is fixed, not adaptive.** A host that is down for more than
   ~1.25s lands on the banner. That is intended — the human is already waiting,
   so a long silent retry is worse than saying so.
3. **The banner is the only recovery affordance.** If the user dismisses it by
   switching conversations, the identity-change re-list covers them; there is no
   separate background reconciler.
