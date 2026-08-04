# Wave C3-D Controller Rejection Fix Brief

Owner: grok3 implementation worker
Controller: Codex Desktop
Keeper worker session: `<keeper-session>`
Runtime allocation: `grok3` / `<keeper-lease>`
Status: active only for the C3-D rejection repair

## Outcome

The first C3-D delivery is **REJECTED** despite its existing focused tests being
green. Repair the authority, replay, expiry, recovery and lifecycle gaps below.
Do not start C3-E and do not touch renderer files.

## Confirmed rejection findings

1. **Restart recovery is incomplete.** `recover_write_proposals` only calls
   `recover_interrupted`, which handles `pending_approval` and `approved` rows.
   Orphaned `applying` rows are never enumerated, observed or reconciled even
   though the activated brief requires read-only digest reconciliation.
2. **Expired proposals can remain actionable in native listing.** A live entry
   is returned with its raw grant without checking `now >= expires_at`. The
   runtime waiter is authoritative, but listing must never expose a past-due
   proposal as actionable and must notify the exact waiter after a durable
   expiry transition.
3. **The native decision path busy-polls.** `wait_entry_cleared` calls
   `std::thread::sleep(5ms)` for up to five seconds. The brief requires
   condition/channel coordination and no polling.
4. **Host drop is incomplete.** `RuntimeHost::drop` releases governed reads but
   not its governed-write owner, allowing live grants/content/waiters to survive
   a drop path that did not explicitly call `shutdown`.
5. **Modify-mode terminal replay is not proven and currently conflicts.** The
   implementation captures the current base before checking durable terminal
   identity. After a successful modify the base has changed, so the same
   provider call recomputes a different fingerprint and cannot replay. Exact
   terminal replay must return the durable receipt without recapturing or
   applying, while changed path/mode/content for the same call identity must
   still conflict.
6. **Pre-wait failure cleanup is incomplete.** If requested-event emission
   fails, `interrupt_live_proposal` publishes a terminal marker but leaves the
   live entry and `pending_by_run` behind because the execute path returns
   before `wait_for_resolution`. Any error after durable reserve and before the
   waiter loop must durably fail closed and remove all sealed/grant/diff state.
7. **Native authority is not using the frozen require-only boundary.** The
   three Tauri write commands call
   `bind_existing_conversation_for_window`; the activated brief requires an
   already-bound window-scoped conversation capability through
   `require_conversation_root_for_window`. A command must not consume or create
   conversation authority as part of a write list/decision/diff request.
8. **The coordinator/native façade is not host-owner scoped.** Global
   `list_write_proposals` selects an arbitrary bound gateway and global native
   functions filter only by conversation id. Two host/database instances with
   colliding durable ids can cross-select state. Expose owner-scoped
   `RuntimeHost` methods (or an equivalent non-forgeable instance façade) and
   make `ConversationService` call those methods. Never choose “any” gateway.
9. **The adapter integration test is non-evidence.** The two tests in
   `governed_write_roundtrip.rs` compare string literals to themselves and do
   not exercise an ACP server request, runtime wait, native decision, provider
   response or event order.

## Required repair shape

- Keep `pillow/write_tool` as the only write entrypoint. Do not weaken
  provider-native permission denial or the read normalizer.
- Check deterministic durable identity before base capture for terminal replay.
  Compare every stored authority/argument field needed to distinguish exact
  replay from same-call conflict. Never trust provider input as host base.
- Make live lookup/list/decide/diff/cleanup owner-scoped. Owner mismatch must be
  indistinguishable from unavailable/denied at public boundaries.
- Replace polling with the existing condition variable or an exact one-shot
  acknowledgement. A native decision may return after durable terminal state
  plus exact waiter notification; it must not poll for map removal.
- At/after the exact expiry deadline, atomically persist `expired`, notify only
  the exact waiter, and stop returning the raw grant. No renderer timer owns
  expiry.
- On all failures after reserve and before normal waiting, transition safely,
  remove `live` and `pending_by_run`, notify the exact waiter if one exists, and
  drop sealed content/grant/diff.
- Add `release_write_owner` to all shutdown/drop paths. Drop must be idempotent.
- Implement full startup recovery for every `applying` row. Add only the narrow
  storage enumeration and executor observation surfaces needed:
  `MatchesProposed | StillBaseOrAbsent | Neither`; reuse executor path,
  canonicalization, symlink/reparse and digest checks. Runtime/desktop code must
  not join stored paths with direct `std::fs` and must not reconstruct content,
  diff or grant.
- Tauri commands must call `require_conversation_root_for_window` and then route
  through the same host instance owned by `ConversationService`.
- Replace the tautological adapter test with a real fake-ACP/provider round
  trip that proves durable pending precedes requested event, an exact native
  approve/reject wakes the request, resolved precedes provider completion, and
  provider-native write permission remains denied.
- Public errors stay fixed and path/content/grant-free. No production-visible
  test mutation hooks.

## Mandatory RED then GREEN cases

1. Successful **modify**, external disk mutation, exact same call replay:
   `Replayed`, no second apply, external mutation preserved.
2. Same call id with changed path, mode or content: conflict.
3. Requested-event callback failure: durable interrupted/failed terminal,
   no live entry/grant/diff, and a later distinct call on the same run is not
   permanently busy.
4. Listing exactly at/past expiry never returns an approval grant and wakes the
   original waiter with durable `expired`.
5. Drop a host with a pending write: exact waiter exits interrupted and a new
   independent host is unaffected.
6. Two host instances backed by different databases but deliberately colliding
   project/conversation/run/call ids cannot list, diff, decide or release each
   other's proposal.
7. Bare conversation UUID, project bootstrap permit, foreign window and stale
   window generation all fail the write commands until the conversation grant
   is already bound.
8. Restart reconciliation covers applying create absent/proposed/neither and
   modify base/proposed/neither, missing modify, symlink/reparse and project
   mismatch; no write occurs during observation.
9. SQLite, runtime events, Debug text and adapter/provider error responses do
   not contain raw grant or proposed/base content.
10. Real adapter roundtrip covers approve, reject and permission denial; no
    tautological assertions.

## Write whitelist

- Existing C3-D whitelist from
  `docs/execution/2026-08-01-wave-c3d-runtime-native-brief.md`
- `crates/storage-sqlite/src/write_proposal.rs` only for a narrow, read-only
  applying-row enumeration needed by restart recovery
- `crates/storage-sqlite/tests/write_proposal_repository.rs` only for that API
- This brief and
  `docs/execution/2026-08-01-wave-c3d-runtime-native-report.md`

Do not modify any renderer file. Do not stage or commit.

## Gates and report

Run the new RED/GREEN tests, then all protocol/session/runtime/adapter/tool-
executor/storage/native package tests, workspace check, Clippy with warnings
denied, `cargo fmt --all -- --check`, `git diff --check`, staging-empty check,
and residual-process cleanup.

Update the C3-D report with exact changed files, tests, remaining risks and
`COMPLETION: DONE` only when every rejection item is closed. Otherwise use
`COMPLETION: BLOCKED`.
