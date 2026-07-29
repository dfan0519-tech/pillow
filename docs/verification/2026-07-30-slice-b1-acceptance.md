# Slice B1 Conversation Kernel Acceptance

Date: 2026-07-30

Worktree: `<workspace>/VaultSystem\keeper-ide-worktrees\integration-slice-b1`

Branch: `integration/slice-b1`

Verdict: **PASS for Slice B1 Task 4 (Tauri conversation integration) after Acceptance Fix 1**

## Explicit non-claim

**Real provider execution is Slice B2 and is not implemented in Slice B1.**

This wave proves only the deterministic fake runtime path:

- no Grok ACP, Codex, Claude, GLM, or other live provider adapter;
- no account authentication, API keys, credentials, model picker, or quota state;
- no Obsidian integration;
- no redesign of accepted Domain, Runtime, or UI modules beyond the Task 4 wiring surface.

The host always uses `RuntimeHost::with_fake`. The UI remains free of provider-specific controls.

## Integrated commit list (Domain → Runtime → UI → Integration)

Integrated in order on `integration/slice-b1` (read-only legacy `<workspace>/VaultSystem\keeper` not modified):

### Domain (Task 1)

| SHA | Message |
| --- | --- |
| `a6563e6` | `feat(session): add runtime-neutral conversation domain` |
| `e5aecc1` | `feat(storage): add conversation SQLite authority` |
| `6bd2f13` | `fix(session): harden event sequence and dual-connection replay` |

### Runtime (Task 2)

| SHA | Message |
| --- | --- |
| `953942b` | `feat(agent-protocol): introduce provider-neutral runtime protocol types` |
| `b1d2975` | `feat(runtime-host): add deterministic fake adapter and bounded host` |
| `96b9582` … `affce2c` | runtime-host cancel/slot/shutdown hardening fixes |

### UI (Task 3)

| SHA | Message |
| --- | --- |
| `5bf1a64` | `feat(desktop): add typed conversation gateway, mock stream, and reducer` |
| `46dcbc7` | `feat(desktop): wire conversation workbench timeline into the shell` |
| `cb8b31e` … `c3c9b88` | conversation UI tests and fix passes |

### Integration (Task 4 — original SHAs + acceptance fix)

Original Task 4 commits:

| SHA | Message |
| --- | --- |
| `2898385` | `feat(desktop): wire conversation service through Tauri commands` |
| `1d9c5af` | `feat(desktop): add Tauri conversation gateway and shell wiring` |
| `cfcb5c4` | `docs(verification): record Slice B1 conversation acceptance` |

Acceptance Fix 1 (closes controller Important findings without redesign):

| SHA | Message |
| --- | --- |
| *(tip of this fix series on `integration/slice-b1`)* | atomic event+status, active-run cleanup, graceful shutdown, E2E process truth |

See `git log cfcb5c4..HEAD --oneline` for the fix commit SHAs after this report lands.

Task 4 surfaces:

- `apps/desktop/src-tauri/src/conversation_service.rs` — single owner of repository + host
- `apps/desktop/src-tauri/src/conversation_events.rs` — typed renderer envelope
- `apps/desktop/src-tauri/src/conversation_commands.rs` — Tauri commands + acceptance hooks
- `apps/desktop/src/lib/tauriConversationGateway.ts` — typed invoke/listen gateway
- `apps/desktop/tests/tauri-conversation-gateway.test.ts`
- `apps/desktop/tests/e2e/conversation-recovery.spec.ts`
- `scripts/conversation-recovery-desktop.mjs` (approved acceptance infrastructure)
- `docs/verification/2026-07-30-slice-b1-acceptance.md`

## Architecture order (authoritative)

For every runtime event:

1. **validate** protocol payload / kind;
2. **persist** event and resulting run status in SQLite;
3. **commit** (single IMMEDIATE transaction via `append_event_with_status`);
4. **emit** the renderer event (`conversation://runtime-event`).

Never emit an event that has not been durably committed. Startup calls
`mark_inflight_interrupted()` once so non-terminal runs become `Interrupted`
through an explicit, idempotent recovery transaction.

Application exit runs idempotent `ConversationService::shutdown` (host reject +
pump drain + zero residual slots). Native acceptance requests graceful window
close after publishing evidence so the E2E path exercises app exit, not only
`taskkill /T /F`.

## Evidence

### Atomic event + resulting run status

- Repository API `SqliteConversationRepository::append_event_with_status`
  commits event append and legal status transition in one SQLite transaction.
- Crash-window regression
  `append_event_with_status_is_atomic_against_crash_window` shows the old
  two-transaction path can leave durable `turn_completed` while status is still
  `Running`, and proves the atomic API makes event + `Completed` visible
  together on a second connection.
- Service test `persistence_commits_before_renderer_emit`:
  `RecordingEmitter` opens a **second** SQLite connection at emit time and
  asserts both the event row and the resulting status (`Running` /
  `Completed` / …) are already durable.

### Active-run cleanup

- Completed / failed / cancelled / pump-error pumps remove their exact `RunId`
  from the service `active` registry (no foreign-key delete).
- Observable helper: `ConversationService::active_run_count()`.
- Tests:
  - `sequential_turns_return_active_run_count_to_zero` (many turns → 0);
  - `cancellation_yields_one_terminal_and_no_residual_slots` → 0;
  - `pump_error_removes_active_run_entry` → 0;
  - `spawn_failure_removes_active_run_entry` → failed spawn leaves no residual;
  - `drain` remains correct when pumps self-remove concurrently.

### Graceful application shutdown

- `ConversationService::shutdown` is idempotent: rejects new work, joins host
  workers, drains pumps, requires zero active / workers / slots, or returns a
  bounded `shutdown_incomplete` / `drain_timeout` error.
- Wired on Tauri `RunEvent::ExitRequested` and `RunEvent::Exit`.
- Acceptance renderer calls `getCurrentWindow().close()` after publishing
  evidence so the process exits through that path.

### Native E2E cleanup and report truth

- `scripts/conversation-recovery-desktop.mjs` waits for:
  1. evidence JSON;
  2. child process exit (graceful app path);
  3. fresh process snapshot with no `keeper-ide.exe`;
  4. no inherited port listener;
  then returns from each phase.
- `ProcessSupervisor.cleanup()` does not return until known PIDs are dead **and**
  (when enforcing desktop clean) a fresh CIM snapshot has no `keeper-ide.exe`
  and no port listener — not `isProcessAlive` alone.
- E2E file order is recovery then project-launcher with
  `--test-concurrency=1` so recovery cannot poison process-baseline tests.

## Gate commands and results

| Gate | Command | Result |
| --- | --- | --- |
| Typecheck | `pnpm typecheck` | PASS (`tsc --noEmit`) |
| Unit tests (JS) | `pnpm test` | PASS — 80 tests |
| Desktop build | `pnpm --filter @keeper-ide/desktop build` | PASS |
| Rust fmt | `cargo fmt --all -- --check` (via `scripts/run-cargo.mjs`) | PASS |
| Rust tests | `cargo test --workspace` (via `scripts/run-cargo.mjs`) | PASS |
| Clippy | `cargo clippy --workspace --all-targets -- -D warnings` | PASS |
| E2E (×2) | `pnpm desktop:test:e2e` | PASS twice — 5/5 tests; no leftover `keeper-ide.exe` after each run |
| Release smoke | `pnpm desktop:smoke` | PASS — `startupMs: 1424` within 2500 ms budget |
| Diff hygiene | `git diff --check main...HEAD` | PASS (run at commit time) |
| Worktree | `git status --short` | clean after commits |

Conversation recovery native summary:

```text
KEEPER_CONVERSATION_RECOVERY {"status":"passed","createEventCount":17,"restoredEventCount":17,"runStatus":"completed","databaseBytes":73728,"webviewProfile":"temporary-and-populated","webviewProfileEntries":207,"durationMs":28805,"ipcBoundary":"real-tauri-webview","gracefulExit":true}
```

Release smoke summary (this machine):

```text
KEEPER_DESKTOP_SMOKE {"mode":"release","observedReadyMs":1463,"startupBudgetMs":2500,"startupMs":1424,"webviewProfile":"temporary-and-populated","readiness":"list-projects-and-lifecycle-command"}
```

## Remaining risks

1. **Fixture speed vs interactive cancel** — production honor delays are 1 ms
   per fixture step; users may only see a brief stream. Cancel is still proven
   under a registration barrier in tests.
2. **`模拟运行时` badge** — Task 3 showed the badge only for
   `MockConversationGateway`. Task 4 wires `TauriConversationGateway` without
   redesigning `App.tsx`; the badge is not forced on the real host path. The
   host remains fake-only; provider UI is still absent.
3. **Dual SQLite connections** — project and conversation repositories open the
   same `keeper.db` path (busy timeout 5s). Fine for desktop single-writer
   patterns; high concurrent write load is out of scope.

(Removed after Acceptance Fix 1: two-transaction event/status crash window;
missing app-exit conversation drain handler.)

## Slice B2 boundary (not done here)

Slice B2 must introduce real provider execution behind the same protocol adapter
boundary. Slice B1 must not be cited as Grok connectivity evidence.
