# C4 D3B1 Codex2 repair brief — portable cwd parity

Read this file first. It is the complete task contract.

You are the actual account-2 Codex CLI worker. Boot Keeper as worker
`codex2-d3b1-cwd-repair` with parent controller session
`<session-id>`. Work only in `<repo>`. Do not delegate,
stage, commit, reset, clean, package, publish, or modify unrelated work.

## Defect

Controller source review found that native `LiveProcessProposalView.portable_cwd
= None` is projected as an empty string. The existing strict renderer contract
in `tauriExecGateway.ts` rejects an empty `portableCwd`; the contract's project
root sentinel is `.`.

## Required repair

- Project `None` as `.` and preserve an existing portable relative cwd exactly.
- Add a focused unit test proving both cases serialize to the renderer contract.
- Do not broaden DTOs, authority, commands, errors, or process policy.

## Exact write allowlist

- `apps/desktop/src-tauri/src/conversation_service/exec_service.rs`
- `apps/desktop/src-tauri/src/exec_commands_tests.rs`
- `docs/execution/2026-08-04-wave-c4-d3b1-codex2-portable-cwd-repair-report.md`

Run the focused exec command tests, `cargo test -p pillow-desktop --all-targets`,
fmt, and scoped diff check. Post a Keeper completion signal and exit.

COMPLETION: pending worker
