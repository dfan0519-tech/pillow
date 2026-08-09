# C4-D1 Grok1 development report — process.run@1 protocol

Role: real Grok CLI slot `grok1`, implementation worker
Task: `c4-d1-process-protocol` (Task 1 of C4 governed process/git plan)
Date: 2026-08-02
Local session note: `<keeper-session>` (boot as controller; worker role requires `local parent session id`)
HEAD preserved: no stage/commit by this worker

## Status

**DONE_WITH_CONCERNS** — Task 1 protocol contract implemented with TDD; focused agent-protocol gates green. Residual workspace/compile and crate-root re-export concerns documented below (allowlist-constrained).

## Changed files (allowlist only)

| Path | Change |
| --- | --- |
| `crates/agent-protocol/src/tool/types.rs` | `ToolId::ProcessRun`, `PROCESS_ARGV_*` caps, `ProcessRunArguments`, `ProcessRunOutput`, `ToolArguments::ProcessRun` |
| `crates/agent-protocol/src/tool/manifest.rs` | Fixed `process.run@1` manifest: timeout 60_000 ms, output cap 262_144, program/argv/cwd args |
| `crates/agent-protocol/src/tool/wire.rs` | Comment only: process payloads convert in `validate.rs`; envelope parsers unchanged |
| `crates/agent-protocol/src/tool/validate.rs` | Strict decode/validate for process args + metadata-only result digests |
| `crates/agent-protocol/tests/tool_contract.rs` | RED→GREEN process.run contract suite (11 new tests); additive manifest length 5 |
| `crates/agent-protocol/tests/tool_contract_security.rs` | Security suite: reservation, authority fields, native paths, transcript denial (4 new tests) |
| `docs/execution/2026-08-02-wave-c4-d1-grok1-protocol-development-report.md` | This report |

No other paths modified by this worker.

## Contract locked

| Item | Value |
| --- | --- |
| Wire tool id | `process.run` / version `1` (`process.run@1`) |
| Args | exact `{ program, argv, cwd? }`; `deny_unknown_fields` |
| Program label | `^[a-z][a-z0-9_-]{0,31}$` (1..=32 ASCII) |
| Argv count cap | `64` (`PROCESS_ARGV_COUNT_CAP`) |
| Combined program+argv bytes | `32_768` (`PROCESS_ARGV_BYTE_CAP`) |
| Argv content | no control characters (incl. NUL) |
| cwd | optional; portable project-relative grammar (reuse `validate_project_path`) |
| Timeout | fixed `60_000` ms in manifest (provider cannot select) |
| Result | metadata only: `exit_code`, `duration_ms`, `stdout_sha256`, `stderr_sha256`, `captured_bytes`, `truncated`, `encoding_lossy`, `cancelled`, `timed_out` |
| Stream digests | `sha256:` + 64 lowercase hex (same as `ArgumentsDigest`) |
| `captured_bytes` | ≤ `262_144` |
| Approval | required by product contract; enforced later by policy/executor (not a manifest field today) |

## RED proof

```text
cargo test -p keeper-agent-protocol --tests process_run -- --nocapture
```

Expected and observed: compile failure before implementation.

- `error[E0599]: no variant ... named ProcessRun found for enum ToolId`
- `error[E0599]: no variant ... named ProcessRun found for enum ToolArguments`

## GREEN counts

```text
cargo test -p keeper-agent-protocol --tests
cargo fmt --all --check
git diff --check -- crates/agent-protocol
```

| Suite | Result |
| --- | --- |
| lib unit tests | **14** passed |
| `tool_contract` | **39** passed (includes 11 process.run) |
| `tool_contract_security` | **18** passed (includes 4 process.run) |
| **Total** | **71** passed, 0 failed |
| `cargo fmt --all --check` | exit 0 |
| `git diff --check -- crates/agent-protocol` | exit 0 |

## Scoped diff review

- Additive only: new tool id/variant, validators, and tests.
- Historical read fixtures still authoritative for first three manifests; `fs.write@1` and `process.run@1` additive (manifest length 5).
- No shell string, environment map, executable path, approval grant, or transcript fields accepted on wire.
- `wire.rs` envelope remains shared; typed conversion lives in `validate.rs` (matches write/read pattern).

## Cached-diff / staging state

- This worker did **not** run `git add`, `commit`, `reset`, `restore`, or `clean`.
- Index had a **pre-existing** staged `Cargo.lock` (+68 lines) unrelated to this allowlist; left untouched.
- Allowlist files remain unstaged working-tree edits only.
- Pre-existing dirty C1–C3 tree preserved.

## Residual processes

| Observation | Note |
| --- | --- |
| Multiple `msedgewebview2` | Host browser/Edge processes; not started by this task |
| No `pillow` / `fake_acp` / stuck `cargo` / `rustc` from this worker | Task used short `cargo test` only |

## Remaining risks / concerns

1. **Crate-root re-exports omitted (allowlist):** `mod.rs` / `lib.rs` were not in the write allowlist. `ProcessRunArguments`, `ProcessRunOutput`, and `PROCESS_ARGV_*` are `pub` inside `tool::types` but **not** re-exported at `keeper_agent_protocol::`. Integration tests exercise via `ToolId` / `ToolArguments::ProcessRun` match arms. Downstream constructors that need the named types will require a controller/integration re-export (mirror C3-A write types).
2. **Workspace non-exhaustive match:** Adding `ToolArguments::ProcessRun` will break `crates/tool-executor` exhaustive match until Task 7 (or a fail-closed arm) lands — same class of issue as C3-A `FsWrite`. Task 1 GREEN gate is package-scoped and green; full workspace check is intentionally out of scope.
3. **Program catalog not protocol-side:** Label grammar only; allowlist of `cargo`/`rustc`/`node`/`python`/`python3` and git/shell denial remain executor/policy (Task 5/7).
4. **Validator ambiguity resolved:** stream digests use `sha256:<hex>` (not bare hex); `cwd` omitted when `None` (`skip_serializing_if`) so digests stay stable without `"cwd": null`.

## Keeper signal

```text
DONE_WITH_CONCERNS — C4-D1 Task 1 process.run@1 protocol complete (grok1).
Changed (7 allowlist files): types/manifest/wire/validate + tool_contract + tool_contract_security + this report.
RED: ProcessRun missing on ToolId/ToolArguments.
GREEN: agent-protocol 71/71 (14 unit + 39 contract + 18 security); fmt 0; scoped diff --check 0.
Staging: worker did not stage; pre-existing staged Cargo.lock preserved; allowlist unstaged.
Risks: no crate-root re-export (allowlist); tool-executor non-exhaustive until later task; catalog denial not in protocol.
No commit.
```
