# Wave C3-F Integration Report — Final Resume After Provider Cancel Recovery

Owner: grok3 harness worker
Worker bridge session: `<keeper-session>`
Brief worker sessions: `<keeper-session>` / recovery `<keeper-session>` / Grok1 recovery `<keeper-session>`
Controller session (signal): `<keeper-session>`
Runtime allocation: `grok3` (final pass on this worker)
Date: 2026-08-01
HEAD: `7b7f70f7360131cd1cfd984a8b19bdfb5725c21b`
Staging: empty
Status: **BLOCKED** (new product isolation defect on two-projects)

Worker completion is not controller acceptance.

## Starting truth

Controller-accepted prerequisites (unchanged):

- run-event schema upgrade
- safe cancel-resolution arbitration
- full-DOM native-path privacy
- **provider cancel RPC completion and same-process next-turn recovery**
  (`docs/verification/2026-08-01-wave-c3f-provider-cancel-rpc-fix-acceptance.md`)

HEAD unchanged; staging empty; accepted dirty product tree frozen for this
pass. Harness whitelist only.

## Formerly blocked sequence — now proven green

The prior blocker was: after Stop while a write proposal is pending, the next
same-process conversation/turn hung (conflict-replay user message never
appeared).

### This pass (batch-core, single long-lived UI process)

| Scenario | Result | Notes |
| --- | --- | --- |
| `reject-create` | **PASS** | bar + bounded diff + reject terminal; target absent |
| `approve-create` | **PASS** | exact UTF-8 create; sha `3705c35f…499b14` (26 bytes) |
| `approve-modify` | **PASS** | modify sha `0592b3a7…fc1ff3` (27 bytes); Open file |
| `base-drift` | **PASS** | external drift preserved; `写入未能完成` |
| `cancellation` | **PASS** | non-success terminal; bar cleared; target unmutated |
| `conflict-replay` | **PASS** | **user message appears after cancel** — cancel recovery fixed |
| `two-projects` | **FAIL** | foreign project still shows approval bar (product) |
| `forced-restart` | not reached | blocked by two-projects |

Cancel → next-turn (conflict) is no longer the release blocker.

## Commands executed

### 1. Node syntax

```text
node --check scripts/wave-c3-governed-write-acceptance.mjs
node --check scripts/lib/c3-governed-write-acceptance.mjs
node --check apps/desktop/tests/e2e/c3-governed-write.spec.ts
```

Result: **PASS** (`NODE_CHECK_OK`).

### 2. Adapter / fake-ACP fixture tests

```text
cargo test -p keeper-adapter-grok-acp --tests -- --test-threads=1
```

Result: all suites green (PowerShell may surface cargo stderr as wrapper exit 1):

| Suite | Result |
| --- | --- |
| unit | 59 passed |
| `adapter_fake_child` | 19 passed |
| `governed_read_roundtrip` | 3 passed |
| `governed_write_roundtrip` | 4 passed (includes `cancel_while_pending_write_completes_provider_rpc_and_next_turn`) |
| `h5_runtime_fake_child` | 6 passed |
| `live_native_acceptance` | 4 passed, 1 ignored |
| `protocol_fixtures` | 3 passed |

### 3. Focused C3-F real Tauri/WebView2 acceptance (serial)

```text
node --test --test-concurrency=1 apps/desktop/tests/e2e/c3-governed-write.spec.ts
```

Latest failing terminal record (representative of isolation block):

```json
{
  "schemaVersion": 1,
  "status": "failed",
  "ipcBoundary": "real-tauri-webview",
  "error": "two-projects: foreign selection must not show approval bar\n\n1 !== 0",
  "cases": [
    "reject-create",
    "approve-create",
    "approve-modify",
    "base-drift",
    "cancellation",
    "conflict-replay"
  ]
}
```

Duration ~210–289s per focused run (includes acceptance debug rebuild).

## Product defect — minimal deterministic reproduction

**Class:** cross-project write-approval isolation leak
**Boundary rule applied:** product code frozen; stop with reproduction.

### Observation

1. Project A starts a live pending create (`[c3f:create-a] pending for isolation A`).
2. Approval bar is visible with path `out/created-a.txt`.
3. Harness selects Project B (aria-pressed + active project name prove B is active).
4. After ≥8s settle for React `clearWriteScope` / conversation null scope,
   `write-approval-bar` count remains **1**.
5. Brief §6 requires proposal/grant/diff/terminal/resource surfaces never appear
   in the foreign selection.

### Why this is not a harness defect

- Product UI gate in `Workbench.tsx` is:

  ```text
  pendingProposal && conversationId
    && pendingProposal.conversationId === conversationId
  ```

- Under a correct project switch, `clearConversationScope` sets
  `selectedConversationId` to null before the new project activates; the bar
  must unmount. Residual bar after multi-second settle means either:
  - `selectedConversationId` still equals the foreign (A) conversation while
    Project B is active, or
  - pending proposal / bar is not conversation-scoped as the render gate claims.
- Harness only drives public sidebar project buttons and WebView2 UI.
- Two-projects uses a dedicated app-data / WebView / Grok profile slice so
  batch-core conversation residuals cannot pollute selection.

### Minimal reproduction (no product edit)

1. Preserve dirty tree; do not stage/commit.
2. Ensure residual `pillow` / `fake_acp_agent` = 0.
3. Run:

```text
node --test --test-concurrency=1 apps/desktop/tests/e2e/c3-governed-write.spec.ts
```

4. Observe batch-core scenarios through `conflict-replay` pass, then
   `two-projects` fails with:

```text
foreign selection must not show approval bar
```

### Suggested non-overlapping product fix direction (controller brief only)

- On project switch, guarantee conversation selection is null **and** write
  approval state is cleared before the new project paints.
- Re-list / park foreign pending proposals so no bar can mount unless
  `conversationId` belongs to the active project.
- Add a unit/integration proof: pending write on A, select B → bar count 0;
  select A again → pending bar restored for the same proposal.

## Harness changes this pass (whitelist only)

| Path | Change |
| --- | --- |
| `scripts/lib/c3-governed-write-acceptance.mjs` | Full-DOM path privacy retained; composer idle diagnostics; conversation restore for pending `新对话` titles; foreign-bar settle + diagnostics; selectProject idle settle |
| `scripts/wave-c3-governed-write-acceptance.mjs` | Dedicated app-data slice for two-projects; cancel/conflict idle handling; isolation restore prefers bar restore |
| `apps/desktop/tests/e2e/c3-governed-write.spec.ts` | Unchanged |
| `apps/desktop/package.json` | Unchanged |
| Fake ACP fixtures | Unchanged |

No product renderer/native/runtime/policy/executor/storage edits.

## Evidence screenshots (from green batch-core portion)

| Artifact | Bytes | SHA-256 |
| --- | --- | --- |
| `reject-bar-closed-1440x960.png` | 47932 | `409F3C2311B86826B93B0B2C57E819DD6B10F131A6AE2E7E1DBF415EB8899583` |
| `reject-diff-open-1440x960.png` | 58789 | `5723A178FE01DC6496F6B20CD27F87F2A06605995E35305FB7CE6D1FDC84415D` |
| `approve-create-bar-1440x960.png` | 48453 | `EFCD2C36BA5F08A80B2AA917E0249406B270FFF54906E893D892FF9B67BF251A` |
| `approve-modify-diff-1440x960.png` | 61713 | `B22C08822788461ED82293402511033B251F8DA12098D26B4EF225047C0CA334` |
| `conversation-dominant-1024x720.png` | 39602 | `80938901C0EEB3D9546EB764B8D9BE3B8516FC24ABEC1448C5AEE2EA63872060` |
| Isolation / forced-restart matrix | **Not produced** | blocked at two-projects |

Logs under `.superpowers/c3-acceptance/focused-run-*.log`.

### Per-scenario file hashes (batch-core)

| Scenario | pre exists | post sha / state |
| --- | --- | --- |
| reject-create | false | absent |
| approve-create | false | `3705c35f60ae1e8de5682e44ff81b48082b4a78cfbcc4a4d37fbb908ed499b14` |
| approve-modify | `20c8f8f8…22604d1c` | `0592b3a7b93d7b71e068247e446abf31dcf962fe7e37aca411d23ebaf1fc1ff3` |
| base-drift | base then drift `e589bddb…10960da5` | drift preserved (not clobbered) |
| cancellation | false | absent |
| conflict-replay | false | absent; single proposal |

## Residual processes

| Check | Result |
| --- | --- |
| After focused runs | `pillow=0`, `fake_acp_agent=0` |
| Staging | empty |
| HEAD | `7b7f70f7360131cd1cfd984a8b19bdfb5725c21b` |

## Gates not run (blocked)

Per brief: remaining gates only after focused C3-F is green. Not executed:

- complete `pnpm --filter @pillow/desktop test:e2e`
- `pnpm check`
- `pnpm --filter @pillow/desktop build` (beyond acceptance debug builds)
- `cargo fmt --all -- --check`
- full-tree `git diff --check`

## Residual risks

1. **Two-project approval-bar isolation is release-blocking** for C3-F scenario 6.
2. Unproven under a green isolation path: forced-restart, keyboard Escape
   contract matrix completion, full second-viewport capture set.
3. Full desktop E2E + `pnpm check` unrun under this wave.
4. Conflict fixture may leave the ACP turn draining after reject (composer not
   always fully idle); batch-core still records conflict as pass because the
   bar clears and the target stays absent. Separate from isolation defect.

## Process hygiene

- No old Keeper writes
- No reset/restore
- No stage/commit
- No package/publish
- No product implementation edits
- Dirty tree preserved; HEAD unchanged

## Completion

**COMPLETION: BLOCKED**

Blocked on product two-project write-approval isolation: after a live pending
write on Project A, selecting Project B still leaves `write-approval-bar`
mounted. Provider cancel RPC recovery is effective (cancel then conflict-replay
in one process is green). Remaining C3-F scenarios and repository gates were
not run past this product stop.
