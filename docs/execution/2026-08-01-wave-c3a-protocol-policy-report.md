# Wave C3-A Report — Protocol And Approval Kernel

Owner: grok3 implementation worker
Controller session: `<keeper-session>`
Worker session: `<keeper-session>`
Correction pass agent: grok3
Date: 2026-08-01
Task: `c3a-protocol-policy` (controller rejection fix)

## Scope

Extend the fixed provider-neutral tool contract with strict `fs.write@1`
create/modify UTF-8 arguments, fixed write public errors, and a pure
`keeper-policy` approval state machine. No SQLite, filesystem execution,
runtime host, adapter, Tauri, or React work.

## Controller Rejection (why this correction pass)

Controller acceptance command `cargo check --workspace --all-targets` failed
at `crates/tool-executor/src/lib.rs:115`: `ToolArguments::FsWrite` was not
handled after C3-A introduced the write variant. No C3 wave may leave the
accepted C1/C2 workspace uncompilable.

Separately, the original 1 MiB canonical-argument cap was insufficient for a
valid `WRITE_CONTENT_BYTE_CAP` (262_144) UTF-8 write whose every content byte
is a non-NUL control character requiring six-byte JSON escaping (`\u00XX`).
The documented `fs.write@1` content byte boundary must work for all allowed
non-NUL UTF-8 content, not only ordinary source text.

## Exact Fix

1. **Fail-closed `FsWrite` arm** in `ReadToolExecutor::execute`: match
   `ToolArguments::FsWrite(_)` and return a canonical failed result with
   `ToolPublicErrorCode::ApprovalRequired`. No filesystem open/create/write
   is performed. Regression test asserts public outcome and that the target
   is neither created nor changed.
2. **Canonical argument cap** in `digest.rs`: replace the “modest escaping”
   1 MiB constant with a still-bounded formula:
   `(WRITE_CONTENT_BYTE_CAP * 6) + 4_096 + 256` — sixfold JSON escape of full
   write content, plus the existing path bound and schema envelope margin.
   Unit + contract tests cover full-cap `\u{1}` content (serializes as
   `\u0001`) and keep the one-byte-over-content-cap rejection.

## Changed Files (this correction pass)

| Path | Change |
| --- | --- |
| `crates/tool-executor/src/lib.rs` | Explicit fail-closed `FsWrite` → `ApprovalRequired` |
| `crates/tool-executor/tests/read_executor.rs` | Mutation-free write rejection regression |
| `crates/agent-protocol/src/tool/digest.rs` | Worst-case sixfold escape cap + unit test |
| `crates/agent-protocol/tests/tool_contract.rs` | Full-cap control-byte content validates |
| `docs/execution/2026-08-01-wave-c3a-protocol-policy-report.md` | Rejection, fix, fresh evidence |

### Prior C3-A whitelist (unchanged intent)

| Path | Change |
| --- | --- |
| `Cargo.toml` | Added `crates/kernel-policy` workspace member |
| `Cargo.lock` | Lockfile refresh for `keeper-policy` |
| `crates/kernel-policy/**` | Pure write approval state machine |
| `crates/agent-protocol/src/tool/types.rs` | `WriteMode`, `FsWrite*`, `ToolId::FsWrite`, cap |
| `crates/agent-protocol/src/tool/manifest.rs` | `fs.write@1` fixed manifest entry |
| `crates/agent-protocol/src/tool/validate.rs` | Strict write args/result validation |
| `crates/agent-protocol/src/tool/public_error.rs` | Write/approval public error codes |
| `crates/agent-protocol/src/lib.rs` | Re-exports for write types/constants |
| `crates/agent-protocol/src/tool/mod.rs` | Re-export new write types |
| `crates/agent-protocol/tests/tool_contract_security.rs` | Write reservation/public-error security |

No other paths modified in this correction pass. No stage/commit/reset.

## Contract Decisions

1. **`fs.write@1` arguments** accept exactly `path`, `mode` (`create`|`modify`),
   `content`. Unknown fields fail closed.
2. **Content rules**: UTF-8 byte length `1..=262_144`; NUL forbidden; empty file
   not supported in C3.0.
3. **Path grammar**: reuse existing portable project-relative rules; additionally
   reject `"."` for write (file path required).
4. **Results**: bind `path`, `mode`, `bytes_written` (exact sealed content length)
   and canonical `content_digest` = `sha256:` + lowercase hex of content UTF-8
   bytes.
5. **Public errors** (canonical message + retryable; wire rejects arbitrary
   messages):
   - `approval_required`
   - `approval_rejected` (non-retryable)
   - `approval_expired`
   - `base_changed`
   - `write_conflict` (non-retryable)
   - `path_rejected` (existing)
   - `write_failed`
6. **Policy transitions** (exact):
   ```text
   pending_approval -> approved -> applying -> succeeded | failed
   pending_approval -> rejected | expired | cancelled | interrupted
   approved         -> expired | cancelled | interrupted | applying
   applying         -> succeeded | failed | interrupted
   ```
   Terminal states never reopen. Expiry is true at `now >= expires_at`.
   `WRITE_PROPOSAL_TTL_SECS = 600`. Decisions only from `pending_approval`
   when not expired (`approve_once` → `approved`, `reject` → `rejected`).
7. **Historical read fixtures** remain authoritative for the first three
   manifests; `fs.write@1` is additive (no fixture-file edits).
8. **Read executor** never mutates the filesystem for `fs.write@1`; it returns
   `ApprovalRequired` until a later write-capable gateway/apply path exists.

## Test Commands And Results (fresh correction-pass evidence)

```text
cargo test -p keeper-agent-protocol --tests
```

- unit (`lib`): **13 passed** (includes
  `full_write_content_of_control_bytes_that_need_u00xx_escapes_is_within_cap`)
- `tool_contract`: **28 passed** (includes full-cap `\u{1}` content validates;
  one-byte-over content cap still rejects)
- `tool_contract_security`: **14 passed**
- **Total: 55 passed, 0 failed**

```text
cargo test -p keeper-policy --tests
```

- unit (`lib`): 0 passed (no unit tests)
- `write_policy`: **16 passed**
- **Total: 16 passed, 0 failed**

```text
cargo test -p keeper-tool-executor --all-targets
```

- unit (`lib`): 0 passed (no unit tests)
- `governed_gateway`: **4 passed**
- `read_executor`: **9 passed** (includes
  `fs_write_fails_closed_with_approval_required_and_does_not_mutate`)
- **Total: 13 passed, 0 failed**

```text
cargo check --workspace --all-targets
```

- **exit 0** (`Finished dev profile` in ~1m 43s). Prior non-exhaustive
  `ToolArguments::FsWrite` failure is gone.

```text
cargo fmt --all -- --check
```

- **exit 0**

```text
git diff --check
```

- **exit 0** (LF/CRLF warnings only on existing dirty tree; no whitespace errors)

```text
git diff --cached --name-only
```

- **empty** (no staged files)

## Residual Risks

1. **Write apply path still absent**: `ReadToolExecutor` only fails closed;
   approved write application, base-digest sealing, and ledger remain later C3
   waves.
2. **Adapter still rejects provider write grants** until later C3 waves wire
   `pillow/write_tool` and host sealing.
3. **Denied credential-like paths** (`.env`, `.pem`, `.git`, …) are host/executor
   policy, not protocol path grammar; protocol only enforces portable path shape.
4. **Canonical argument cap is larger** (~1.58 MiB) to admit worst-case control
   escaping; still bounded and only checked on the canonical digest string.
5. **No SQLite ledger / apply / UI** yet — approval state machine is pure and
   uncoupled from persistence.

## Working-Tree Governance

- HEAD baseline preserved; accepted dirty tree not reset.
- No staging, commit, discard, or force operations.
- `legacy Keeper (out of tree)` not modified.
- Correction pass does not use or create a Git worktree.

## COMPLETION: DONE
