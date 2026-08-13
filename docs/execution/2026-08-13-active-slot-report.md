# Active slot report — host is source of truth

Date: 2026-08-13 · Worker: grok2 · Scope: report which runtime slot is live

## Goal

List DTO exposes which slot (if any) is the process-active conversation runtime,
sourced from host `RuntimeServiceState.active_slot_id`. Renderer marks that row
and button copy from the list only — no session-local guess.

## Status

- Skeleton: yes
- Implementation: done
- Verification: commands below (exit codes recorded; not claiming gate ownership)

## Exit codes

| Command | EXIT | Log |
| --- | --- | --- |
| `node scripts/run-cargo.mjs test -p pillow-desktop --lib` | **0** | `.superpowers/round49/as-log1.txt` |
| `corepack pnpm --filter @pillow/desktop typecheck` | **0** | `.superpowers/round49/as-log2.txt` |
| `corepack pnpm --filter @pillow/desktop test` | **0** | `.superpowers/round49/as-log3.txt` |

Cargo: 223 passed, 0 failed, 2 ignored. Desktop vitest: 56 files / 736 tests passed.

## Red-then-green

### UI (before UI used host `isActive`)

- File: `apps/desktop/tests/runtime-slots-ui.test.tsx`
- Suite: `RuntimeSlotsSection — host-reported active slot`
- Tests:
  - `marks the host-reported active row on first load without a local start click`
  - `does not invent an active slot when host reports none active`
- Red log: `.superpowers/round49/as-red-ui.txt`
- Failure on old UI (`startedSlotId` session guess, no `data-active-runtime`):

```
Expected the element to have attribute:
  data-active-runtime="true"
Received:
  null
```

After wiring `slot.isActive` into list markup / badges / start button text, same
tests pass (EXIT=0).

### Host list DTO

- File: `apps/desktop/src-tauri/src/runtime_slot_commands_tests.rs`
- Test: `list_runtime_slots_reports_host_active_slot_from_runtime_state`
- Asserts: no active → all `is_active == false`; after start → only that
  `slot_id` is true; start another → flag moves (exclusivity).
- Ran green under the cargo lib suite above.

### Gateway closed key

- File: `apps/desktop/tests/runtime-slots-gateway.test.ts`
- `accepts isActive boolean and rejects missing or non-boolean isActive`
- Allowlist length pin: 19 → **20** with `isActive` required.

## DTO before / after

### Host `RuntimeSlotDto` (`runtime_slot_commands.rs`)

| | Before | After |
| --- | --- | --- |
| Active flag | *none* | `is_active: bool` → wire `isActive` |
| Source | n/a | `RuntimeServiceState::is_active_slot(project_id, slot_id)` |
| None live | UI guessed via session `startedSlotId` | every row `false` |

`list_runtime_slots` now takes `RuntimeServiceState` and sets `is_active` per
row. Create paths always return `is_active: false`. Update paths re-query
`is_active_slot` after mutation. **No lifecycle change** to
`activate_grok` / `shutdown*`.

### Renderer `RuntimeSlotView`

```ts
isActive: boolean; // host-reported; required on closed parse
```

## Renderer allowlist (same change batch)

`RUNTIME_SLOT_DTO_KEYS` in `runtimeSlotsGateway.ts` added `"isActive"`.
`parseRuntimeSlotView` requires `typeof value.isActive === "boolean"` and
rejects missing / non-boolean. Unknown keys still fail closed.

## UI behaviour

- Removed session-local `startedSlotId` entirely from `RuntimeSlotsSection`.
- List row: `data-active-runtime="true"|"false"`, optional
  `runtime-slots-list__item--active` class.
- Governed badge: `已启动` vs `未启动 · 需启动后才能对话` from `slot.isActive`.
- Start button: `重新启动` vs `启动` from `slot.isActive`.
- After successful `startRuntimeSlot`, re-lists so host flag is painted
  (mock also sets exclusivity on start).
- Styles for the active row live in `settings.css` (allowed surface).

## Restart behaviour (how verified)

Cannot run a full desktop remount E2E in this worker (no E2E / gate ownership).
Verified by unit equivalence of restart:

1. Seed list with `isActive: true` on one slot and `false` on another **without**
   clicking start → UI shows `已启动` / `重新启动` on the live row only
   (`host-reported active slot` tests). This is the remount case: session state
   is empty, host list alone drives the mark.
2. Host: start slot A → list reports only A; start B → only B
   (`list_runtime_slots_reports_host_active_slot_from_runtime_state`).
3. None active: both rows `data-active-runtime="false"`, buttons say `启动`.

## Residual risks

1. **NativeAdapter prepare path** (`status = native_adapter_prepared`) does not
   call `activate_grok`, so `active_slot_id` stays unset and list `isActive`
   stays false for those slots — honest for “conversation runtime”, but a user
   who treats “探活成功” as “live for chat” may still see `未启动` until a
   GrokShell path activates. Out of scope for this ticket (lifecycle untouched).
2. **List is the only paint path** for active state after start. If re-list after
   start fails, probe success text may show while the badge lags until next
   reload — same class of partial-failure as other list refreshes.
3. **No multi-project active flag on the DTO beyond project scope**: host
   `is_active_slot` already requires matching `project_id`; listing another
   project correctly shows all false when the live runtime is elsewhere.
4. **Manual visual check in a running Pillow shell** was not done here (no
   desktop smoke ownership). Styles are covered by attribute/class assertions
   and CSS under the allowed settings stylesheet.
5. Worker did not run full repo gate / E2E; only the three commands above.
