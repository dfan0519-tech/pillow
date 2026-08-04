# C4 D3A typed native and TypeScript gateways — blocked handoff

Date: 2026-08-04
Worker: account-2 Codex CLI (`codex2`)
Keeper session: `<keeper-session>`
Parent: `<keeper-session>`

## Result

**BLOCKED — do not accept.** The strict TypeScript DTO boundary is present, but
the required native commands cannot be implemented without creating forbidden
second authorities.

## RED evidence

1. `cargo test -p pillow-desktop --lib c4_gateway -- --nocapture` failed as
   intended before native implementation: `is_exec_public_error` was absent.
2. `corepack pnpm --filter @pillow/desktop test -- c4-gateway-contract.test.ts`
   failed as intended before TypeScript implementation because
   `tauriExecGateway` and `tauriGitGateway` did not exist.

## Partial green evidence

- TypeScript contract tests: 3 C4 tests passed (the workspace test command
  executed 23 files / 272 tests due the existing Vitest command forwarding).
- `corepack pnpm --filter @pillow/desktop typecheck`: passed after normalizer
  implementation.
- `git diff --check -- apps/desktop/src/lib apps/desktop/tests/c4-gateway-contract.test.ts`:
  passed.

## TypeScript contract delivered

| Surface | Strictly accepted fields | Denials proven |
| --- | --- | --- |
| `ExecProposalView` | identity, fixed lifecycle/origin/risk enums, literal argv, portable cwd, expiry, `decisionAvailable`, fixed public error | unknown field, raw grant, run/internal id, native path, invalid enum |
| `ExecOutputView` | proposal id, fixed state, ordered tagged chunks, terminal metadata | unknown field, invalid stream/state, more than 256 KiB / 4,000 lines, oversized capture |
| Git discovery/status/diff | portable repository/path identity, fixed status/diff enum, generation, bounded files/diff | native path, unknown field, invalid enum, repository mismatch, stale generation |
| Native error mapping | closed process/Git error vocabularies with fixed renderer messages | native message text is discarded |

## Required blocker resolution

1. Task 8 must land the accepted `RuntimeHost` process bridge methods named by
   its brief: process list, decide, and open-output, including its opaque
   approval-grant retention and conversation/project/run ownership checks.
   The checked tree has no `src/host/process.rs` and no exported process bridge
   method. Calling `keeper-tool-executor` directly from Tauri would bypass the
   live host owner and weaken C3 authority.
2. The desktop needs an accepted source of the trusted `git.exe` plus a
   project-scoped generation owner. `GitReadExecutor::new` requires a trusted
   executable path but C4 has not supplied desktop configuration/composition
   for it. Guessing from `PATH`, a renderer argument, or a machine path would
   violate the fixed trusted-executable contract.

Until those two seams exist, native `exec_commands.rs` remains intentionally
RED and no Tauri command is registered. No process, terminal, PTY, Git
mutation, UI mount, runtime, or C3 authority was added.

## Files changed

- `apps/desktop/src-tauri/src/exec_commands.rs` — RED native contract anchor only.
- `apps/desktop/src-tauri/src/lib.rs` — exposes the RED anchor to lib tests.
- `apps/desktop/src/lib/execGateway.ts`
- `apps/desktop/src/lib/gitGateway.ts`
- `apps/desktop/src/lib/tauriExecGateway.ts`
- `apps/desktop/src/lib/tauriGitGateway.ts`
- `apps/desktop/tests/c4-gateway-contract.test.ts`
- This report.

## Repository state and residuals

- HEAD: `7b7f70f7360131cd1cfd984a8b19bdfb5725c21b` (unchanged).
- Staging: empty (`git diff --cached --name-only` returned no paths).
- No commit, reset, restore, clean, package, publish, process spawn, or Git
  mutation was performed.
- The pre-existing C1–C4 dirty tree remains untouched outside the stated
  allowlist.

## COMPLETION

`COMPLETION: blocked — awaiting Task 8 process bridge and accepted desktop Git trust/generation owner.`
