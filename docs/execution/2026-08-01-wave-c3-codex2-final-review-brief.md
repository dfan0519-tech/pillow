# Wave C3 Independent Final Review Brief

Owner: Codex2 (`gpt-5.6-terra`, high, `fastMode=false`)

Controller session: `<keeper-session>`

Dormant until C3-D/E/F implementation and every controller development gate
are green. Read-only review; report file only.

## Objective

Independently decide whether C3 governed write is safe and product-correct at
the provider, runtime, persistence, filesystem, native-command, renderer and
restart boundaries. Green broad tests are evidence, not a substitute for
authority-path review.

## Review Lenses

### Provider and identity

- Only strict `pillow/write_tool` can propose `fs.write@1`.
- Provider-native permission and read aliases cannot reach write.
- JSON-RPC transport id is never durable call identity.
- Unknown/duplicate/foreign fields and conflicting replays fail closed.

### Grant and approval authority

- Every write asks; no allow-session/YOLO path.
- Raw grant is canonical 43-character base64url for 32 OS-random bytes,
  memory-only, constant-time checked, single-use and conversation-owned.
- Renderer supplies no project/root/content/base/digest/fingerprint.
- Expiry, cancellation, rejection and restart cannot apply.

### Content and filesystem

- Proposed/base content and diff never reach SQLite/events/logs/Debug.
- Size, UTF-8, NUL, path, denied secret names and reparse checks are exact.
- Base CAS occurs immediately before an atomic no-clobber create or atomic
  modify; post-write success is proven from actual disk bytes/digest.
- Cancellation and terminalization are lease-owner-aware and replay-safe.

### Runtime and recovery

- Durable pending precedes requested event; durable terminal receipt precedes
  resolved event/provider success.
- Waiters are exact, non-spinning, cancellation/expiry aware and fully drained
  at shutdown.
- Restart interrupts pending/approved; applying is reconciled read-only from
  disk; no content/grant/diff is reconstructed.
- Concurrent projects/conversations cannot wake, list, decide or open one
  another's proposals.

### Renderer and product shape

- Conversation remains the primary surface; one bar and one contextual diff,
  no dashboard/card stack.
- Grant and raw ids are absent from DOM, console, persistence and screenshots.
- Focus/Tab/Escape ownership is correct; stale async results cannot cross a
  project/conversation switch.
- Terminal rows are durable-derived; only success offers governed Open file.

## Evidence To Inspect

- C3 design and implementation plan.
- C3-A/B/C controller acceptance artifacts and worker reports.
- C3-D/E/F reports and changed files.
- Full controller gate logs, desktop screenshots and process cleanup evidence.
- HEAD/staging/dirty-tree status and whitelist compliance.

## Output

Write only `docs/verification/2026-08-01-wave-c3-codex2-final-review.md`.
List findings by `Critical`, `Important`, then `Minor`, each with exact
file/line evidence and a concrete exploit/failure path. End with one of:

- `VERDICT: ACCEPT — Critical 0 / Important 0`
- `VERDICT: REJECT — Critical N / Important N`

Do not edit product code, stage or commit.
