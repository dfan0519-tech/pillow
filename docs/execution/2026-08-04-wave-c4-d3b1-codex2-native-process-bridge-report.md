# C4 D3B1 Codex2 report — native process bridge

**Worker:** `codex2-d3b1-native`  
**Date:** 2026-08-04  
**HEAD:** `62149be9eddd2288eb45fd9b8278641cf64c7a5f`  
**Staging:** empty; no commit created.

## Delivered

Implemented the three existing renderer contract commands in the desktop Tauri
invoke handler:

- `list_exec_proposals`
- `decide_exec_proposal`
- `open_exec_output`

The commands require the existing window/project/conversation capability before
delegating to the owning `ConversationService`. The service in turn delegates
only to its `RuntimeHost` governed-process authority. It adds no proposal
store, executor, grant, policy, or transcript storage.

`ExecProposalDto` projects the host view into the existing camel-case renderer
contract. It exposes only `decisionAvailable`; grants and host-retention fields
are absent. `ExecOutputDto` projects bounded host output and intentionally omits
host-private program, argv, live-retention, and per-chunk lossy fields.

All public failures map only to the established `EXEC_PUBLIC_ERRORS` vocabulary.
Output opens map a non-live record to `process_output_unavailable`; decision
verbs are parsed before host access and reject every value except
`approve_once` and `reject`.

## Changed files

- `apps/desktop/src-tauri/src/exec_commands.rs`
- `apps/desktop/src-tauri/src/conversation_service/exec_service.rs` (new)
- `apps/desktop/src-tauri/src/conversation_service/mod.rs`
- `apps/desktop/src-tauri/src/lib.rs`
- `apps/desktop/src-tauri/src/exec_commands_tests.rs` (new)
- this report

Pre-existing untracked D3B1 briefs were preserved unchanged.

## Test evidence

TDD RED:

```text
cargo test -p pillow-desktop --lib exec_commands_tests
FAILED to compile before implementation: missing Exec DTO exports,
exec_service module, decide_exec_proposal, and open_exec_output.
```

Focused GREEN:

```text
cargo test -p pillow-desktop --lib exec_commands_tests
5 passed; 0 failed
```

Required package gate:

```text
cargo test -p pillow-desktop --all-targets
91 library tests passed; 11 integration tests passed; 0 failed
```

Formatting and diff gates:

```text
cargo fmt --all --check
git diff --check
both passed
```

Focused coverage asserts DTO serialization/redaction, ownership error
projection, invalid decision rejection, unavailable output, and registration in
the product invoke handler.

## Residual risks

- This is a native bridge slice only. Controller integration remains responsible
  for renderer wiring and end-to-end desktop acceptance.
- The exact worker Keeper boot was attempted first but returned HTTP 400:
  `worker claims require parentSessionId`. The dispatcher supplied no parent
  session ID, so no worker session exists to receive a valid completion message.
  The required completion POST was attempted before exit and returned HTTP 400:
  `sessionId is required`. No session was fabricated and no role was changed.

## Completion signal

Required completion payload, pending a valid worker session:

```text
DONE — pillow C4 D3B1 native process bridge complete (unstaged): exact
list_exec_proposals / decide_exec_proposal / open_exec_output commands are
registered; 5 focused tests and pillow-desktop all-targets passed; fmt and diff
checks passed. Keeper worker boot was rejected because parentSessionId was not
provided.
```

COMPLETION: complete
