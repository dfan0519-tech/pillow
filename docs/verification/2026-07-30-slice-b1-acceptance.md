# Slice B1 Conversation Kernel Acceptance

Date: 2026-07-30

Worktree: `<workspace>/VaultSystem\keeper-ide-worktrees\integration-slice-b1`

Branch: `integration/slice-b1`

Verdict: **PASS for Slice B1 Task 4 (Tauri conversation integration)**

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

### Integration (Task 4 — this report)

Task 4 adds:

- `apps/desktop/src-tauri/src/conversation_service.rs` — single owner of repository + host
- `apps/desktop/src-tauri/src/conversation_events.rs` — typed renderer envelope
- `apps/desktop/src-tauri/src/conversation_commands.rs` — Tauri commands + acceptance hooks
- `apps/desktop/src/lib/tauriConversationGateway.ts` — typed invoke/listen gateway
- `apps/desktop/tests/tauri-conversation-gateway.test.ts`
- `apps/desktop/tests/e2e/conversation-recovery.spec.ts`
- `scripts/conversation-recovery-desktop.mjs`
- `docs/verification/2026-07-30-slice-b1-acceptance.md`

## Architecture order (authoritative)

For every runtime event:

1. **validate** protocol payload / kind;
2. **persist** event and resulting run status in SQLite;
3. **commit** (repository transactions);
4. **emit** the renderer event (`conversation://runtime-event`).

Never emit an event that has not been durably committed. Startup calls
`mark_inflight_interrupted()` once so non-terminal runs become `Interrupted`
through an explicit, idempotent recovery transaction.

## Evidence

### Persistence-before-emit

Rust test `conversation_service::tests::persistence_commits_before_renderer_emit`:

- `RecordingEmitter` hook opens a second SQLite connection at emit time and
  asserts the event row already exists for `(run_id, sequence)` before the
  emit is recorded;
- completed fixture includes ordered `assistant_delta`, `plan_updated`,
  `tool_call_completed`, `subagent_completed`, and terminal `turn_completed`.

### Project isolation

Rust test `projects_retain_isolated_conversation_lists`:

- two projects each create a conversation;
- `list_conversations(project_id)` returns only that project’s rows.

### Ordered stream

Same persistence test plus host fixture tests:

- renderer sequences are contiguous 1-based (`sequence == index + 1`);
- domain storage remains 0-based (`protocol_sequence - 1`) with DTO remap on load.

### Cancellation and residual process

Rust test `cancellation_yields_one_terminal_and_no_residual_slots`:

- barrier holds the host worker after registration;
- cancel lands before fixture body;
- exactly one terminal event (`run_cancelled`);
- run status `cancelled`;
- `in_flight_turns() == 0` and `host_is_idle()`.

Background selection test `selecting_another_conversation_does_not_stop_active_run`
proves loading another timeline does not stop the first run’s completion.

### Restart and `Interrupted` recovery

1. **Unit:** `restart_marks_inflight_interrupted_idempotently` seeds a Running
   run, reopens `ConversationService`, runs `recover_inflight` twice
   (`1` then `0`), and loads messages + events + `interrupted` status.
2. **Native desktop:** `scripts/conversation-recovery-desktop.mjs`
   - phase `create_stream`: real WebView2 → Tauri IPC → SQLite fake turn →
     completed timeline evidence;
   - full process tree cleanup;
   - phase `restore`: same temporary app-data → list/load timeline → same
     project/conversation/run IDs, event counts, `completed` status.

## Gate commands and results

| Gate | Command | Result |
| --- | --- | --- |
| Typecheck | `pnpm typecheck` | PASS (`tsc --noEmit`) |
| Unit tests (JS) | `pnpm test` | PASS — 80 tests |
| Desktop build | `pnpm --filter @keeper-ide/desktop build` | PASS |
| Rust fmt | `cargo fmt --all -- --check` (via `scripts/run-cargo.mjs`) | PASS |
| Rust tests | `cargo test --workspace` (via `scripts/run-cargo.mjs`) | PASS |
| Clippy | `cargo clippy --workspace --all-targets -- -D warnings` | PASS |
| E2E | `pnpm desktop:test:e2e` / conversation-recovery.spec | PASS — service suite + native restart restore |
| Release smoke | `pnpm desktop:smoke` | PASS — `startupMs: 1702` within 2500 ms budget |
| Diff hygiene | `git diff --check main...HEAD` | PASS (run at commit time) |

Conversation recovery native summary (representative):

```text
KEEPER_CONVERSATION_RECOVERY {"status":"passed","createEventCount":…,"restoredEventCount":…,"runStatus":"completed","ipcBoundary":"real-tauri-webview","webviewProfile":"temporary-and-populated"}
```

Release smoke summary (this machine):

```text
KEEPER_DESKTOP_SMOKE {"mode":"release","observedReadyMs":1721,"startupBudgetMs":2500,"startupMs":1702,"webviewProfile":"temporary-and-populated","readiness":"list-projects-and-lifecycle-command"}
```

## Remaining risks

1. **Fixture speed vs interactive cancel** — production honor delays are 1 ms
   per fixture step; users may only see a brief stream. Cancel is still proven
   under a registration barrier in tests.
2. **Append + transition are separate SQLite transactions** — order is still
   persist-then-emit; a crash between append and status transition can leave a
   durable event with a lagging status until the next restart recovery path
   (non-terminal → `Interrupted`). A combined transaction would be a future
   storage API extension, not a B1 redesign.
3. **`模拟运行时` badge** — Task 3 showed the badge only for
   `MockConversationGateway`. Task 4 wires `TauriConversationGateway` without
   redesigning `App.tsx`; the badge is not forced on the real host path. The
   host remains fake-only; provider UI is still absent.
4. **Dual SQLite connections** — project and conversation repositories open the
   same `keeper.db` path (busy timeout 5s). Fine for desktop single-writer
   patterns; high concurrent write load is out of scope.
5. **Pump thread join on app exit** — host shutdown is not yet hooked to Tauri
   exit; process termination still reaps workers. Explicit app-exit drain is a
   polish item, not required for B1 acceptance (restart recovery covers crash).

## Slice B2 boundary (not done here)

Slice B2 must introduce real provider execution behind the same protocol adapter
boundary. Slice B1 must not be cited as Grok connectivity evidence.
