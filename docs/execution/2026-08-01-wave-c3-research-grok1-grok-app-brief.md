# Wave C3 Research Brief — grok1 / Grok App

Role: read-only interaction and state-flow researcher.

## Startup

1. Work from `this repository`.
2. POST Keeper boot with project `pillow`, task
   `c3-research-grok-app`, agentName `grok1`, agentRole `worker`.
3. Read `root AGENTS.md`, the shared vault `AGENTS.md`, and
   `this repository\AGENTS.md`.

## Scope

Inspect the fixed local Grok App reference at
`pinned Grok App reference (out of tree)`, source commit
`cbbbdf17bb8db1588b6707f3f42f6b00397e27f9`, MIT. Focus on actual components
and state transitions for:

- permission or approval requests inside a conversation;
- proposed file changes and diff/resource opening;
- tool progress, completion and failure disclosure;
- accept/reject/cancel/expiry states;
- preserving conversation dominance while files and diffs remain contextual.

Trace source files and state flow rather than relying on screenshots or README
claims. Record exact local paths and line numbers. Distinguish conceptual
reference, adapted implementation and direct reuse under
`docs/references/upstream-baseline.md`.

## Questions

1. Which interaction primitives should Pillow adopt for C3?
2. Which Grok App behaviors are unsafe or too provider-specific?
3. How should a proposal summary expand into diff, approval and result without
   permanent card stacks?
4. Which data belongs to the conversation timeline versus the contextual
   resource pane?
5. What keyboard/focus/error states must desktop acceptance cover?

## Output Contract

Write only:

`this repository\docs\research\2026-08-01-c3-grok-app-interaction-research.md`

The report must contain: evidence table with source paths/lines, state-flow
diagram in text, adopt/adapt/avoid decisions, provenance classification,
recommended C3 UI contract, risks, and `COMPLETION: DONE`.

Do not modify code, other documentation, Git staging, or commits. Do not write
anything under the frozen Keeper product directory.
