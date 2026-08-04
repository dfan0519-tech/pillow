# Pillow C4 D3A - typed native and TypeScript gateways brief

## Worker and authority

- Worker: **actual account-2 Codex CLI**, `CODEX_HOME=%USERPROFILE%\.codex2`,
  model `gpt-5.6-terra`, reasoning `high`, `fastMode=false`/default service tier.
  This is not a Codex Desktop subagent.
- Keeper boot: `project=pillow`, `task=c4-d3a-typed-native-ts-gateways`,
  `agentName=codex2`, `agentRole=worker`, parent
  `<keeper-session>`.
- Work only in `<workspace>\pillow`; Keeper and references are read-only.
- Preserve HEAD `7b7f70f` and the accepted dirty C1-C4 D2B tree. No stage,
  commit, reset, restore, clean, package or publish.
- Work directly in this CLI instance; do not spawn or substitute subagents.

## Inputs

Read all AGENTS files, the accepted C4 design, Task 10 in the implementation
plan, D1 UI DTOs, accepted D2BR2 process/Git APIs, and existing C2/C3 typed
Tauri/TypeScript gateway patterns.

## Goal

Implement strict native/Tauri and TypeScript gateway parity for:

- conversation-capability-bound process list/decide/open-output operations;
- project-capability-bound Git discover/status/diff operations;
- bounded sanitized DTOs carrying fixed enums, portable paths, repository
  identity and generation;
- strict TypeScript normalizers that reject unknown fields, invalid enums,
  native paths, raw approval grants, raw internal/session identifiers,
  oversized output and stale/mismatched generation before renderer state.

Do not mount UI, edit workbench components, create terminal/PTY/Git mutation,
add another runtime or weaken C3 authority.

## RED -> GREEN evidence

Write failing parity/security tests first for invalid enum/unknown fields,
native path, raw grant/internal ID, oversized output, capability mismatch,
cross-project/conversation access and stale generation. Prove one canonical
public error vocabulary and Rust/TS field parity.

Run:

- `cargo test -p pillow-desktop --lib c4_gateway -- --nocapture`
- `cargo test -p pillow-desktop --lib`
- `corepack pnpm --filter @pillow/desktop test -- c4-gateway-contract.test.ts`
- `corepack pnpm --filter @pillow/desktop typecheck`
- `corepack pnpm --filter @pillow/desktop build`
- `cargo check --workspace --all-targets`
- `cargo clippy -p pillow-desktop --all-targets -- -D warnings`
- `cargo fmt --all --check`
- `git diff --check -- apps/desktop/src-tauri apps/desktop/src/lib apps/desktop/tests/c4-gateway-contract.test.ts`

## Exclusive write allowlist

- `apps/desktop/src-tauri/Cargo.toml`
- `apps/desktop/src-tauri/src/exec_commands.rs`
- `apps/desktop/src-tauri/src/git_commands.rs`
- `apps/desktop/src-tauri/src/conversation_service/exec_service.rs`
- `apps/desktop/src-tauri/src/conversation_service/git_service.rs`
- `apps/desktop/src-tauri/src/conversation_service/mod.rs`
- `apps/desktop/src-tauri/src/lib.rs`
- `apps/desktop/src/lib/execGateway.ts`
- `apps/desktop/src/lib/gitGateway.ts`
- `apps/desktop/src/lib/tauriExecGateway.ts`
- `apps/desktop/src/lib/tauriGitGateway.ts`
- `apps/desktop/tests/c4-gateway-contract.test.ts`
- `docs/execution/2026-08-04-wave-c4-d3a-codex2-typed-native-ts-gateways-report.md`

Do not edit runtime-host, executor crates, protocol/policy/storage, renderer
components/styles, scripts, root manifests/lock or other tests. Stop and report
if a required dependency cannot be added within the desktop manifest.

## Completion

Report exact files, Rust/TS parity matrix, privacy rejection evidence,
RED/GREEN counts, staging/HEAD, residual processes and risks. Post one Keeper
completion message and emit `COMPLETION: complete` or a precise blocker.
Controller alone accepts.
