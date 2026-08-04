# Wave C3-F Product Fix Report — Provider Cancel RPC Completion

Owner: Grok adapter worker
Worker session: `<keeper-session>` (bridge boot: `<keeper-session>`)
Controller session to signal: `<keeper-session>`
Runtime allocation: `grok3` / `<keeper-lease>`
Date: 2026-08-01

**COMPLETION: DONE**

## Root cause (confirmed)

On a pending governed write, host cancel durably produces a cancelled receipt.
Runtime-host post-cancel arbitration commits safe `write_proposal_resolved`,
forces `run_cancelled`, then returns `RuntimeError::Cancelled` to the emitter.

In `handle_write_tool`, `sink.emit(event)?` treated that expected signal as a
hard failure and exited **before** responding to the original
`pillow/write_tool` server request. The provider child kept waiting for that
request id (up to the 180s wait), and the prompt loop never reached
`end_prompt` / event-sink clear. The next same-process turn failed with
`Internal { message: "acp_busy" }`.

## Fix (adapter-only)

### 1. Safe post-cancel resolution → continue provider completion

In `handle_write_tool`, when emitting `WriteProposalResolved`:

- If `sink.emit` returns `RuntimeError::Cancelled` **and** all of:
  - turn cancel token is set;
  - receipt delivery is `Executed`;
  - resolved state/error is exactly `cancelled`/`cancelled` or
    `interrupted`/`interrupted`;
- then treat the emit as already-committed safe arbitration and **continue**
  to `provider_write_response` + a single RPC error response.

Every other sink error still propagates (fail closed). Success responses are
never synthesized on the cancel path.

### 2. Prompt-loop cleanup after host-forced terminal

When the final terminal emit returns `RuntimeError::Cancelled` while the turn
cancel token is set (host already forced `run_cancelled`), still:

- `turns.end_prompt()` (clear live turn → no `acp_busy`);
- clear provider event sink;
- release `operation_gate` on `send_turn` return.

## Exact provider response (cancel-while-pending)

| Field | Value |
|-------|--------|
| JSON-RPC | error (never success) |
| code | `-32042` |
| message | `governed_write_cancelled` |

Source: `ProviderWriteResponseError::Cancelled::rpc_error()` in
`provider_writes.rs`. No path, content, grant, or debug fields.

For mid-apply interrupt the parallel fixed pair is `-32045` /
`governed_write_interrupted` (same continue path when state/error is
`interrupted`/`interrupted`).

## Event order (host spawn cancel path)

Exactly:

1. `write_proposal_requested` (pre-cancel, while pending)
2. host cancel (durable proposal → cancelled)
3. **exactly one** `write_proposal_resolved` with
   `state=cancelled`, `error_code=cancelled`
4. **exactly one** `run_cancelled` (host-forced after safe resolution)

No target file mutation on cancel.

## Cleanup proof

After cancel-while-pending join:

| Check | Result |
|-------|--------|
| Join budget | `< 5s` (observed ~0.8s; not 180s) |
| `host.is_fully_idle()` | true (no live worker, sync gate free, slots 0) |
| Live turn | cleared (`end_prompt`) |
| Provider request | answered once with fixed error (fake agent unblocks) |
| Operation gate | released with `send_turn`/`spawn` worker exit |
| Same adapter next turn | completes within `< 5s` budget; approve path writes file |

## RED / GREEN evidence

### RED (before fix)

```text
cargo test -p keeper-adapter-grok-acp --test governed_write_roundtrip \
  cancel_while_pending_write_completes_provider_rpc_and_next_turn -- --test-threads=1

test cancel_while_pending_write_completes_provider_rpc_and_next_turn ... FAILED
panicked at crates/adapter-grok-acp/tests/governed_write_roundtrip.rs:
next same-process turn must admit and complete after cancel recovery:
Internal { message: "acp_busy" }
```

### GREEN (after fix)

```text
test cancel_while_pending_write_completes_provider_rpc_and_next_turn ... ok
# full governed_write_roundtrip: 4/4 ok in ~2s
# full keeper-adapter-grok-acp --tests --test-threads=1: all pass
```

## Gates

| Gate | Result |
|------|--------|
| Focused RED→GREEN cancel-then-next-turn | PASS |
| `cargo test -p keeper-adapter-grok-acp --tests -- --test-threads=1` | PASS |
| `cargo check --workspace --all-targets` | PASS |
| `cargo clippy -p keeper-adapter-grok-acp --all-targets -- -D warnings` | PASS |
| `cargo fmt -p keeper-adapter-grok-acp -- --check` | PASS |
| whitelist `git diff --check` | PASS (LF/CRLF warning only) |
| HEAD unchanged | `7b7f70f7360131cd1cfd984a8b19bdfb5725c21b` |
| Staging empty | Yes |

## Changed files (whitelist only)

| Path | Change |
|------|--------|
| `crates/adapter-grok-acp/src/prompt.rs` | Safe post-cancel `sink.emit` continue; terminal emit cleanup on host `Cancelled` |
| `crates/adapter-grok-acp/tests/governed_write_roundtrip.rs` | TDD test: spawn cancel-while-pending + same-process next turn |
| `crates/adapter-grok-acp/src/bin/fake_acp_agent/scripts.rs` | Minimal clippy cleanups required for package `-D warnings` (no scenario logic change for cancel path) |
| `docs/execution/2026-08-01-wave-c3f-provider-cancel-rpc-completion-fix-report.md` | This report |

Not edited: runtime-host, protocol, native conversation service, renderer,
storage, policy, executor, C3-F harness, frozen Keeper tree. No stage/commit.

## Residual risks

1. **Interrupted path** is covered by the same receipt matcher but the
   integration test exercises cancel-while-pending (`cancelled`/`cancelled`)
   only; mid-apply `interrupted` relies on unit-level pairing + host
   arbitration already proven in C3-F arbitration fix.
2. **Desktop/WebView2 matrix** still belongs to controller resume after product
   fixes; this report proves adapter-host same-process recovery only.
3. **Non-write host cancel** also benefits from the terminal-emit cleanup
   (avoids sticky `acp_busy`); ordinary cancel regressions remain covered by
   existing adapter cancel tests.

## Controller handoff

Signal session `<keeper-session>`: product fix DONE. Unstage/commit
not performed. Ready for controller review/acceptance and C3-F resume.
