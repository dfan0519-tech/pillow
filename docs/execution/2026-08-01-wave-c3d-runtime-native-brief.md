# Wave C3-D Brief — Runtime, Adapter And Native Commands

Owner: grok1 implementation worker

Controller session: `<keeper-session>`

Dormant until C3-A/B/C are accepted.

## Controller Activation Addendum

This brief becomes active only after the controller records C3-C acceptance.
Implement it as two internal milestones in one worker lease: D1 strict
provider/runtime coordination first, then D2 native/service exposure. Do not
start D2 until D1 focused tests are green. The worker must stop and report if
an accepted C3-A/B/C contract would need to be weakened.

## Goal

Connect strict provider method `pillow/write_tool` to the governed-write
gateway, persist a pending proposal before emitting activity, wait for an exact
one-shot native decision, return a provider receipt, and expose typed Tauri
commands for listing, deciding and opening the diff.

The adapter must continue denying provider-native write permissions.

## Event Contract

Add summary-only runtime events:

```rust
pub struct WriteProposalRequestedPayload {
    pub proposal_id: String,
    pub path: String,
    pub mode: WriteMode,
    pub summary: String,
    pub expires_at: DateTime<Utc>,
}

pub struct WriteProposalResolvedPayload {
    pub proposal_id: String,
    pub path: String,
    pub mode: WriteMode,
    pub state: WriteProposalState,
    pub error_code: Option<WritePublicErrorCode>,
}
```

Never put content, diff, approval grant, native path, session id or debug
metadata in event payloads.

Add dedicated `write_proposal_requested` and `write_proposal_resolved` runtime
event kinds. Do not encode approval state as generic tool output containing a
serialized proposal, receipt, or provider request.

## Provider Contract

Accept only method `pillow/write_tool` with exact keys:

```json
{
  "sessionId": "active session",
  "callId": "portable id",
  "toolId": "fs.write",
  "toolVersion": 1,
  "arguments": { "path": "relative/file", "mode": "create", "content": "..." }
}
```

Foreign session, unknown key, unknown tool/version, bad call id or conflicting
transport/request identity fail closed. `session/request_permission` for write
remains rejected.

`pillow/write_tool` is the only accepted write entrypoint. Do not extend the
existing read normalizer so `fs.write` can pass through `pillow/read_tool`, ACP
filesystem aliases, or provider-native permission methods. Preserve the exact
provider `callId` as durable call identity; JSON-RPC transport ids remain
transport-only.

## Native Contract

```text
list_write_proposals(conversationId) -> WriteProposalDto[]
decide_write_proposal(conversationId, proposalId, approvalGrant, decision)
  -> WriteProposalDto
open_write_proposal_diff(conversationId, proposalId) -> WriteDiffDto
```

DTOs expose only project-relative path, mode, state, summary, expiry, bounded
diff/truncation, opaque error and the live one-time grant when applicable.
Decision callers cannot submit project id, root, content, base, digest or
fingerprint.

Freeze the raw grant representation as exactly 32 cryptographically secure
random bytes encoded with base64url without padding (43 ASCII characters).
Generate it from the operating-system CSPRNG (`getrandom` or an equivalent
auditable OS primitive), never UUIDs, timestamps or a PRNG. Only
`sha256:<lowercase hex>` of those 43 grant bytes may reach SQLite. Reject any
decision grant that is not canonical 43-character base64url before digesting;
compare the raw grant to the in-memory owner with constant-time equality before
calling storage. The grant may appear only in the owning conversation's live
pending DTO; redact it from every `Debug` implementation and never log it.

`open_write_proposal_diff` returns a diff only when the exact proposal remains
in the active in-memory coordinator and belongs to the supplied conversation.
After restart, recovered non-terminal proposals have no diff and no grant and
must be `interrupted`; native commands must not reconstruct either from disk.

Every Tauri command must accept `WebviewWindow` and `AppState` internally,
derive the proposal's project/root from the authoritative conversation, and
require the existing window-scoped conversation capability before listing,
deciding, opening a diff, or routing a succeeded file. A bare valid
conversation UUID is not authority. The public invoke payload remains the
frozen fields above; renderer callers still do not submit project id or root.

## Runtime Requirements

- Register run with host-owned project/conversation/root capability.
- Seal base/proposed content and raw grant only in the active coordinator.
- Persist metadata `pending_approval` before emitting requested event.
- At most one pending write per run; other runs/conversations may progress.
- Wait through a condition/channel with cancellation and expiry; no busy spin.
- Wake only the exact proposal/run.
- Approve verifies grant/metadata, applies, persists terminal receipt, emits
  resolved event, then responds to provider.
- Reject/expire/cancel return fixed opaque provider errors.
- Shutdown interrupts all waiters. Restart recovery never applies; applying
  rows use the read-only digest reconciliation from the design.

The runtime wait owns expiry: at the exact deadline it durably transitions the
pending proposal to `expired`, emits the resolved summary, wakes only the
owning request, and destroys the in-memory grant/content/diff. Native listing
may reconcile a due pending/approved row before returning, but must never show
a past-due proposal as actionable. Do not delegate expiry authority to a
renderer timer.

The coordinator is the sole live authority for `(proposal id, project,
conversation, run, call, fingerprint, sealed contents, raw grant, diff,
waiter)`. A native decision first resolves that exact in-memory record, then
passes only the host-owned fingerprint and computed grant digest to storage.
Never trust a renderer-supplied fingerprint/digest. Remove the in-memory entry
only after the terminal durable transition has succeeded and the exact waiter
has been notified. On persistence or emit failure, fail closed without a
second write.

No public production mutation hooks may be introduced for tests. Test-only
seams must be private `#[cfg(test)]` code or existing explicit `test-support`
features that cannot be invoked by a normal production dependency.

## File Whitelist

- `crates/agent-protocol/src/types.rs`
- `crates/agent-protocol/src/validate.rs`
- `crates/agent-protocol/src/lib.rs`
- `crates/agent-protocol/tests/protocol_fixtures.rs`
- `crates/kernel-session/src/domain.rs`
- `crates/kernel-session/tests/*`
- `crates/runtime-host/src/governed_write.rs`
- `crates/runtime-host/src/lib.rs`
- `crates/runtime-host/src/host/mod.rs`
- `crates/runtime-host/tests/governed_write_bridge.rs`
- `crates/tool-executor/src/write.rs` and `crates/tool-executor/src/lib.rs`
  only for a narrow public read-only applying-target observation API if the
  accepted C3-C surface cannot perform restart reconciliation safely
- `crates/tool-executor/tests/governed_write.rs` only for that observation API
- `crates/adapter-grok-acp/src/provider_writes.rs`
- `crates/adapter-grok-acp/src/lib.rs`
- `crates/adapter-grok-acp/src/prompt.rs`
- `crates/adapter-grok-acp/src/bin/fake_acp_agent/scripts.rs`
- `crates/adapter-grok-acp/tests/governed_write_roundtrip.rs`
- `apps/desktop/src-tauri/src/write_commands.rs`
- `apps/desktop/src-tauri/src/write_commands_tests.rs`
- `apps/desktop/src-tauri/src/conversation_service/write_service.rs`
- `apps/desktop/src-tauri/src/conversation_service/mod.rs`
- `apps/desktop/src-tauri/src/conversation_service/service.rs`
- `apps/desktop/src-tauri/src/conversation_service/reconciliation.rs`
- `apps/desktop/src-tauri/src/app_state.rs`
- `apps/desktop/src-tauri/src/lib.rs`
- affected Cargo manifests only when compilation proves necessary
- `docs/execution/2026-08-01-wave-c3d-runtime-native-report.md`

Do not touch renderer files.

Restart reconciliation must not duplicate lexical/canonical/reparse/regular
file checks in runtime-host or desktop code and must not join a stored path
with `std::fs` directly. If C3-C lacks the required surface, add one narrow
executor method that accepts the existing `ProjectCapability` plus immutable
stored proposal metadata and returns only
`MatchesProposed | StillBaseOrAbsent | Neither`. It performs no write and
returns no content. Cover create absent/proposed/neither, modify base/proposed/
neither, missing modify, symlink/reparse and project mismatch before using it
from recovery.

## TDD And Gates

Write RED tests for strict adapter parsing, native write denial, run binding,
durable-before-event, exact wakeup, cross-conversation/project denial, altered
decision payload, expiry/cancel/replay/conflict, shutdown and restart recovery.

Also test canonical grant length/alphabet, malformed grant rejection,
wrong-but-well-formed grant rejection, Debug/log/event/SQLite secret absence,
diff unavailable after restart, two simultaneous conversations waking only
their own waiter, and terminal replay returning evidence without applying a
second time.

Run focused agent-protocol/kernel-session/runtime/adapter/native tests, then all
affected package tests, fmt, diff check and staging-empty check.

## Completion

Write only `docs/execution/2026-08-01-wave-c3d-runtime-native-report.md` with
changed files, RED/GREEN evidence, exact command DTOs, concurrency/recovery
evidence, risks and `COMPLETION: DONE` or `COMPLETION: BLOCKED`.
