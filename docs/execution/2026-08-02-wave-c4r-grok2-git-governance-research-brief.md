# Grok2 brief: C4 Git governance research

You are the real Grok CLI worker in slot `grok2`.

## Goal

Design a project-scoped Git capability ladder that begins with read-only
repository discovery/status/diff/branch and later adds narrowly governed
stage, unstage, commit, branch switch and single-file restore.

## Required evidence

1. Read Pillow's accepted C2 resource and C3 write/diff/approval boundaries.
2. Inspect Git-related state and UI flows in the pinned local Grok App clone.
3. Inspect `<workspace>\zbbody` conceptually and verify its license before any
   direct reuse recommendation.
4. Map current Pillow project canonicalization, resource paths, approvals,
   receipts, SQLite and contextual-pane seams.
5. Compare native library approaches (`git2`, `gix`) with supervised Git CLI
   execution for the required Windows features.

## Required decisions

- repository discovery below a project root, including nested repositories;
- stable status/diff DTOs and bounded binary/large-file behavior;
- rename, submodule, symlink, ignored and untracked semantics;
- branch/worktree identity and refresh/invalidation rules;
- exact approval payloads for stage, unstage, commit, switch and restore;
- dirty-tree, conflict, detached-HEAD and base-drift handling;
- commit-message and author/credential boundaries;
- explicit exclusions: reset-hard, clean, force checkout, rebase, merge,
  push/publish and arbitrary argument passthrough;
- test fixtures and adversarial acceptance scenarios.

## Write allowlist

Only create or update:

`<workspace>\pillow\docs\research\2026-08-02-c4-git-governance-research.md`

Do not change any other file. Finish with `Status: DONE` or
`Status: BLOCKED`, changed-files truth, residual risks and one Keeper signal.
