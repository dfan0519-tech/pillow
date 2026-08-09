# Pillow documentation map

This tree is intentional product evidence: design, decisions, verification, and
execution history that show governance is real. It is not a dump of private
ops manuals.

## Start here (contributors)

| Path | Role |
| --- | --- |
| `decisions/` | Architecture Decision Records (product boundaries) |
| `superpowers/specs/` | Accepted design specs |
| `superpowers/plans/` | Implementation plans (some historical routing headers) |
| `verification/` | Acceptance and checkpoint evidence; how to run gates |
| `research/` | Protocol, security, interaction research |
| `migration/` | Legacy capability inventory (logical; no private layout) |
| `references/` | Upstream pin baselines |
| `roadmap/` | Stage status |

## Historical execution records

`execution/` holds wave briefs and worker reports. They document how work was
contracted and verified. They are **not** onboarding docs and do not define a
public multi-agent API. Worker labels in filenames (`grokN`, `codexN`) are
historical dispatch tags, not product surface area.

When reading older briefs:

- Treat “session bridge boot” or claim/release language as **local farm
  history**, not a requirement for contributors.
- Treat “legacy Keeper (out of tree)” as an optional external reference the
  original authors used; this repository builds without it.

## Controller handoff

`handoff/` may still contain maintainer dispatch notes. Controllers decide what
stays public versus moves to a private ops store. Do not treat handoff as the
contributor entry path; use `AGENTS.md` and `verification/worker-gate-entry.md`.
