# Wave C3 Research Brief — grok2 / zbbody

Role: read-only collaboration and editor interaction researcher.

## Startup

1. Work from `this repository`.
2. POST Keeper boot with project `pillow`, task `c3-research-zbbody`, agentName
   `grok2`, agentRole `worker`.
3. Read `root AGENTS.md`, the shared vault `AGENTS.md`, and
   `this repository\AGENTS.md`.

## Scope

Inspect the local `zbbody reference clone (out of tree)` checkout at current HEAD
`00c293414333b6fcc4e4598389fa3952c8d0b09b`. Its worktree is independently
dirty; do not change it. Verify its license before recommending any reuse.

Trace actual components and state flows for:

- conversation-owned agent steps, tools, roles and output disclosure;
- editor, file-change and diff presentation;
- approval or confirmation interactions;
- project/conversation isolation and restored state;
- layout decisions that keep conversation central and auxiliary resources
  collapsible.

Record exact local paths and line numbers. Treat the project as conceptual-only
unless the license and provenance rules clearly permit more.

## Questions

1. Which collaboration patterns complement the Grok App baseline?
2. Which UI patterns create dashboard/card clutter and should be rejected?
3. What proposal/diff/approval state should be visible at each disclosure
   level?
4. Which focus, keyboard, empty and error states matter for Windows desktop?
5. What must Pillow implement independently?

## Output Contract

Write only:

`this repository\docs\research\2026-08-01-c3-zbbody-interaction-research.md`

The report must contain: license finding, evidence table with source
paths/lines, interaction/state-flow findings, adopt/adapt/avoid decisions,
recommended C3 UI contract, risks, and `COMPLETION: DONE`.

Do not modify code, other documentation, either worktree, Git staging, or
commits.
