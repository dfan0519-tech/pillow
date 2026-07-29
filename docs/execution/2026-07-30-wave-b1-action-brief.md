# Keeper IDE Wave B1 Action Brief

Date: 2026-07-30

Controller: Codex Desktop

## Objective

Build the first complete conversation path through a deterministic fake
runtime: persistent domain state, ordered runtime events and a streaming
conversation UI. This wave does not claim real Grok connectivity.

## Personnel And Ownership

| Worker | Assignment | Owned surfaces | Report |
| --- | --- | --- | --- |
| Grok A | Conversation domain and SQLite | `kernel-session`, migration 0002, conversation repository | `.superpowers/sdd/wave-b1-domain-report.md` |
| Grok B | Protocol and runtime host | `agent-protocol`, `runtime-host`, deterministic fixtures | `.superpowers/sdd/wave-b1-runtime-report.md` |
| Grok C | Conversation UI | gateway, reducer, timeline, unified sidebar and composer | `.superpowers/sdd/wave-b1-ui-report.md` |
| Codex controller | Architecture and acceptance | fixed contracts, integration order, reviews and native gates | `docs/verification/2026-07-30-slice-b1-acceptance.md` |

## Execution Rules

- Every worker starts from the same accepted Grok shell base in an isolated
  Git worktree.
- Workers cannot edit the legacy Keeper repository or merge their branches.
- No real credentials, API keys, network calls or provider-specific payloads.
- Every worker uses TDD, semantic commits and a completion report.
- Controller integrates in order A, B, C.
- Each branch receives spec and code-quality review before integration.

## User-Visible Target

- create and select conversations under one project;
- send a task into a deterministic simulated runtime;
- see assistant streaming plus plan, tool and sub-agent activity;
- cancel the active run;
- switch conversations while background work continues;
- restart Keeper IDE and recover the timeline.

The UI displays `模拟运行时` so this wave cannot be confused with Slice B2
real Grok ACP.
