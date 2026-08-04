# Pillow Wave C4-R research dispatch brief

Date: 2026-08-02

State: authorized research round; product implementation not authorized

## Objective

Produce the evidence required to design C4 terminal and Git without widening
the accepted C3 authority boundary. Four real Grok CLI slots work in parallel
on non-overlapping reports. The controller integrates their findings, writes
the design and performs the design gate.

## Shared constraints

- Work only from `<workspace>\pillow` for Pillow artifacts.
- Treat `<workspace>\VaultSystem\keeper` as frozen and read-only.
- Local references `<workspace>\VaultSystem\references\grok-app` and
  `<workspace>\zbbody` are read-only.
- Read `<workspace>\AGENTS.md`, the shared vault rules and this repository's
  `AGENTS.md`, then POST a fresh Keeper boot with `project=pillow`, the exact
  task and `agentRole=worker`.
- Do not modify source, tests, manifests, lockfiles, shared vault files or
  reference repositories.
- Do not stage, commit, reset, clean, restore, package or publish.
- Do not expose credentials, native account data or raw session identifiers.
- Record exact local source paths and pinned commits when citing reference
  implementations. Concepts may be recommended; source adaptation requires a
  separate provenance decision.
- End with one Keeper completion or blocker signal. Do not create a periodic
  monitor.

## Assignments

| Slot | Domain | Sole report path |
| --- | --- | --- |
| grok1 | governed command execution and Windows process lifecycle | `docs/research/2026-08-02-c4-terminal-process-research.md` |
| grok2 | Git discovery, status/diff/branch and governed mutations | `docs/research/2026-08-02-c4-git-governance-research.md` |
| grok3 | conversation-centred terminal/Git interaction | `docs/research/2026-08-02-c4-terminal-git-interaction-research.md` |
| grok4 | threat model, adversarial scenarios and acceptance gates | `docs/research/2026-08-02-c4-security-acceptance-research.md` |

## Controller integration gate

The controller will reject a report that lacks concrete state flow, authority
owner, persistence boundary, failure handling, test seams, provenance or
explicit exclusions. Research completion does not authorize implementation.
