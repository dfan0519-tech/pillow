# Grok1 brief: C4 governed terminal and process research

You are the real Grok CLI worker in slot `grok1`.

## Goal

Recommend the smallest safe architecture for C4's first checkpoint:
conversation-owned, project-scoped, approve-once command execution followed by
an interactive Windows terminal in a later checkpoint.

## Required evidence

1. Read the C3 design, acceptance and process-safety implementation in
   `<workspace>\pillow`.
2. Inspect the process/session and terminal-related implementation in the
   pinned local Grok App clone at
   `<workspace>\VaultSystem\references\grok-app`.
3. Inspect `<workspace>\zbbody` only as a conceptual reference; verify its license
   before recommending any direct reuse.
4. Map the current Pillow protocol, policy, runtime-host, tool-executor,
   SQLite and Tauri seams that a terminal capability would consume.
5. Compare one-shot execution, shell-string execution, argv execution and
   ConPTY/PTTY sessions. Make a recommendation for Windows.

## Required decisions

- command proposal and approval ownership;
- cwd canonicalization and project containment;
- executable/argv versus shell text representation;
- allow-listed environment and secret exclusion;
- stdout/stderr ordering, UTF-8/Windows encoding and bounded output;
- timeout, cancellation, restart and process-tree cleanup;
- durable metadata versus transcript persistence;
- one-shot checkpoint interfaces and later PTY interfaces;
- explicit non-goals and fail-closed cases.

## Write allowlist

Only create or update:

`<workspace>\pillow\docs\research\2026-08-02-c4-terminal-process-research.md`

Do not change any other file. Finish the report with `Status: DONE` or
`Status: BLOCKED`, changed-files truth, residual risks and a concise Keeper
completion/blocker signal.
