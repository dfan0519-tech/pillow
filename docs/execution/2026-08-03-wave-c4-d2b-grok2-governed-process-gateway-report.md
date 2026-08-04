# C4-D2B Grok2 development report — governed process gateway

Date: 2026-08-03

Worker: real Grok CLI slot `grok2`
Worker session: `<keeper-session>`
Controller parent session: `<keeper-session>`
Task: `c4-d2b-governed-process-gateway` (Task 7)
Status: **DONE** (controller owns acceptance)

## Objective

Implement the in-process `seal → decide → claim → execute → receipt` gateway for
`process.run@1` in `keeper-tool-executor`. Provider input is only a catalog
program label, structured argv and optional portable cwd. Native
executable/cwd/argv and the clean environment stay in host memory; SQLite
receives digests and typed terminal receipts only through the accepted D2A/D2AR
execution ledger. Execution always goes through `SupervisedProcessRunner`.

## Public boundary delivered

| Symbol | Role |
| --- | --- |
| `SealedProcessSpec` | Memory-only sealed program/argv/cwd/exe; Debug redacts native/argv |
| `PreparedExecProposal` | Durable metadata + sealed material |
| `GovernedProcessOutcome` | Succeeded / Failed / Cancelled / Busy / Terminal / Expired / Conflict / Rejected |
| `GovernedProcessError` | Fail-closed gateway errors including `CleanupUnproven` |
| `GovernedProcessToolGateway` | `open` / `open_with_runner` / `seal` / `reserve` / `decide` / `execute` |
| `ProcessRunner` | Trait; production = `SupervisedProcessRunner` |
| `CountingProcessRunner` | Spawn-count proof double |
| `CleanupUnprovenRunner` | Fail-closed cleanup path double |
| `PROCESS_CATALOG` | `cargo`, `rustc`, `node`, `python`, `python3` |
| `validate_program_catalog` | Allowlist + git/shell/shim denial |
| `build_clean_environment` | Base names only; sensitive exclusion; filtered PATH |
| `CLEANUP_UNPROVEN_PUBLIC_ERROR` | Fixed public token `process_cleanup_unproven` |

### Gateway flow

1. **seal** — catalog check, resolve absolute executable, resolve portable cwd
   under project root (no reparse/symlink escape), compute `argv_digest` +
   `proposal_fingerprint` binding program/argv/cwd/exe/native cwd.
2. **reserve** / **reserve_with_input** — ledger digests only (`program`,
   `argv_digest`, `portable_cwd`, fingerprint).
3. **decide** — conversation-scoped approve_once / reject via ledger.
4. **execute** — `claim_run`; **only** first `Claimed` builds `ProcessSpec` and
   invokes the runner. `Busy` / `Terminal` / `Expired` / `Conflict` / `Rejected`
   never spawn. Re-seal drift check before run. Clean env rebuilt at execute.
5. **receipt** — map process receipt → `ExecTerminalReceipt`;
   `complete_run` with `Succeeded` / `Failed` / `Cancelled` and exact
   cancellation-flag consistency (D2AR).

## Denial matrix

| Input class | Result |
| --- | --- |
| `cargo` / `rustc` / `node` / `python` / `python3` | Catalog allow; resolve via filtered PATH |
| Unknown label (`go`, `ruby`, `deno`) | `UnknownProgram` — no proposal |
| `git` | `DeniedProgram` |
| `cmd`, `powershell`, `pwsh`, `bash`, `sh` | `DeniedProgram` |
| `wscript`, `cscript`, `mshta` | `DeniedProgram` |
| `npm`, `pnpm`, `yarn`, `npx` | `DeniedProgram` |
| `*.cmd` / `*.bat` / `*.ps1` / `*.vbs` / labels with `.` | `DeniedProgram` |
| Absolute / `..` / path-shaped labels | `DeniedProgram` |
| Invalid portable cwd (`..`, missing dir, absolute) | `CwdRejected` |
| Fingerprint mismatch at execute | `FingerprintMismatch` — no spawn |
| Rejected proposal | `Rejected` / terminal — spawn count 0 |
| Expired approved/pending | `Expired` — spawn count 0 |
| Running (same owner / other owner / expired lease) | `Busy` — spawn count 0 |
| Cleanup unproven | Durable `Failed` + `GovernedProcessError::CleanupUnproven` — never success |

Git cannot enter through `process.run@1`: label `git` is catalog-denied before
resolution or ledger reservation.

## Spawn-authority proof

| Case | Evidence |
| --- | --- |
| Approve-once success | `CountingProcessRunner.spawn_count() == 1` |
| Terminal replay | second `execute` → `Terminal`, spawn count stays 1 |
| Reject | spawn count 0 |
| Expiry | spawn count 0 |
| Running + same/other/expired-lease claim | ledger `Busy`; gateway `Busy`; spawn count 0 |
| Static | `process.rs` contains no `std::process::Command` / `.spawn()` |

## Privacy evidence

- Sealed Debug redacts executable, native cwd and argv.
- `sealed_argv_and_native_paths_never_enter_sqlite` asserts secret argv and
  native paths absent from row Debug and raw SQLite bytes.
- Durable row stores `program` label + digests only.
- Clean env excludes `API_TOKEN`, `OPENAI_API_KEY`, `HOME`, `USERPROFILE`,
  `APPDATA`, `LOCALAPPDATA` and any `sensitive_name` key.

## Exact files (exclusive allowlist)

| Path | Action |
| --- | --- |
| `crates/tool-executor/Cargo.toml` | **Modified** — depend on `keeper-process-executor` |
| `crates/tool-executor/src/process.rs` | **Created** — gateway |
| `crates/tool-executor/src/lib.rs` | **Modified** — module + re-exports |
| `crates/tool-executor/tests/governed_process.rs` | **Created** — RED/GREEN suite |
| `docs/execution/2026-08-03-wave-c4-d2b-grok2-governed-process-gateway-report.md` | **Created** (this report) |

No protocol, policy, process-executor, storage, runtime-host, Tauri, renderer,
Git executor, root `Cargo.toml`/`Cargo.lock` or scripts were modified by this
worker.

HEAD preserved: `7b7f70f`. No stage, commit, reset, restore, clean, package or
publish.

## TDD gates

### RED (intent)

Tests encoded before/with the gateway surface. First compile of
`governed_process` failed until `process` module + exports existed. After first
implementation:

```text
cargo test -p keeper-tool-executor --test governed_process -- --nocapture
16 tests: 13 passed, 3 failed
- rejection_never_spawns (Rejected mapped as Terminal before classification fix)
- approval_expiry_returns_expired_without_spawn (same)
- no_direct_command_spawn_symbol (comment false-positive on "Command::spawn")
```

### GREEN

| Command | Result |
| --- | --- |
| `cargo test -p keeper-tool-executor --test governed_process` | **16/16** passed (+ **3** lib unit tests in `process::unit_tests`) |
| `cargo test -p keeper-tool-executor --tests` | **67** passed (lib 8; governed_gateway 4; governed_process 16; governed_write 17; read_executor 10; write_diff 12) |
| `cargo check -p keeper-tool-executor --all-targets` | exit 0 |
| `cargo clippy -p keeper-tool-executor --all-targets -- -D warnings` | exit 0 |
| `cargo fmt --all --check` | exit 0 |
| `git diff --check -- crates/tool-executor` | exit 0 |
| `cargo check --workspace --all-targets` | **blocked** by concurrent out-of-allowlist `keeper-git-executor` test `git_read_integration.rs` (E0615 method-as-field). Not caused by this worker. |
| `cargo check --workspace --all-targets --exclude keeper-git-executor` | exit 0 |

## Staging state

- Allowlist deliverables: unstaged only (`?? crates/tool-executor/` including
  new process module/tests; report untracked).
- `git diff --cached --name-only`: empty for this worker.
- Pre-existing C1–C3 / D1 / D2A dirty tree left untouched outside allowlist.

## Residual processes

Host has ambient `node` / `node_repl` / `python` processes (IDE/runtime). No
`cargo`/`rustc`/`pillow`/`fake_acp` left from this worker's test runs. Tests
rely on Job Object cleanup proof inside `SupervisedProcessRunner`; this worker
did not kill by numeric PID or process name.

## Residual risks (controller)

1. **Workspace check** is red solely because parallel D2B git-runner work
   (`crates/git-executor/tests/git_read_integration.rs`) is mid-flight outside
   this allowlist.
2. **Program resolution** walks host PATH + toolchain markers; a missing
   catalog tool yields opaque `ProgramUnavailable`, not PATH fallback to denied
   names.
3. **`node -e` / `python -c`** remain literal argv (design: high-risk, approval
   UI must disclose). Catalog does not rewrite argv.
4. **Runtime-host wiring** (Task 8) still required to surface the gateway in the
   product host; this crate is the seal/apply authority only.
5. Not controller source-reviewed; acceptance remains controller-owned.

## Completion

`COMPLETION: complete`
