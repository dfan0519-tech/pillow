# ADR-0004: The Knowledge Base Is Registered By The Work, Not Maintained By Anyone

Date: 2026-08-08

Status: Accepted

## Context

Stage 2's goal, in the product owner's words: the harness should auto-fill and
retire a knowledge base along fixed paths, so that context limits and project
disorder stop degrading communication.

This repository is the first instance of the problem. `docs/` holds 261+ files.
Which document is current is recorded only as hand-written prose — a line saying
"this file supersedes X". Research over the tree found **134 documents with no
inbound reference at all**, including ADRs, acceptance records, and research
reports: not garbage, just forgotten.

The failure is observable in this project's own operation. On 2026-08-08 the
controller hand-maintained the handoff document and edited it six or seven times
across one session. It was still stale at the moment context was compacted —
§5 announced an in-flight UI worker that had finished hours earlier. The cause
was not ignorance of the rule; the controller was tracking four workers and
bookkeeping is the first thing dropped under load.

## Decision

### 1. The actor taxonomy is not human-vs-agent

An earlier draft split writes into "human edits directly, agents need approval".
The product owner corrected this: **humans will not edit the knowledge base.**
The actors are all agents — controller, worker, and optionally a dedicated
information-manager role.

This matters because "approval" can no longer mean "ask the person every time".
At the frequency knowledge writes occur, a human-in-the-loop prompt would
produce exactly the click-through fatigue that makes the *code* approval prompt
worthless — and that prompt is the product's core safety primitive, valuable
precisely because it is rare.

So the question is which agent role may write what, with what audit trail —
not whether a human is present.

### 2. Registration is a byproduct of dispatch and acceptance, not a duty

Two candidate designs were considered and both rejected in pure form.

**Controller maintains it.** Rejected on evidence: the controller *is* the thing
that fails today, and the proposed remedy ("require every round to update
status") is the rule that already exists and already lapsed. Enforcement by
instruction does not survive contact with a busy controller. It also spends the
scarcest resource in the system — controller context — on bookkeeping instead of
judgment.

**A dedicated information manager records it.** Rejected as the primary
mechanism because a recorder only knows what it is handed. On 2026-08-08 the
controller's acceptance produced findings that a scribe working from the same
worker reports could not have produced: that a cross-layer drift test was
tautological because the parser had been converged to a single implementation;
that a light-theme contrast value reported as passing measured 3.57:1; that a
delivery-research table row was in fact a product-bricking defect that had to
preempt the round. **The valuable content of the knowledge base is the
controller's verdict, which is exactly the part that does not survive being
relayed.**

The resolution is that most of it need not be maintained at all. Nearly
everything worth recording is *already a durable artifact*: the brief states what
was asked, the worker report states what was done, the gate and E2E logs state
whether it passed, the commit states what landed. The only thing that is not
already a file is the controller's verdict — accepted or rejected, why, and what
residuals remain.

Therefore: **dispatch and acceptance write to the registry as a byproduct of
happening.** Dispatch records the task (brief, whitelist, slot, lease).
Acceptance records the verdict. Neither is a step someone must remember, for the
same reason `validate → persist → commit → emit` does not rely on anyone
remembering to persist: it is on the path.

### 3. The handoff document is rendered, not written

A hand-written status document goes stale within hours; this one did. A document
rendered from registry records cannot. Handoff becomes a view, not an artifact
someone maintains.

### 4. The information-manager role is an auditor, not a scribe

Scribing should be mechanical. Auditing needs judgment but can be performed from
artifacts already in the tree, without the controller feeding it: contradictions
(two documents each claiming to be current), orphans, residuals that no round has
touched in N rounds. The 134-orphan finding was produced exactly this way — an
agent read `docs/` and found it; nobody reported it upward.

### 5. Retirement marks and archives; deletion is approved and receipted

Automatic retirement may mark decayed and move to an archive location. Real
deletion requires approval and leaves a receipt.

The evidence is the 134 unreferenced files: a plausible rule like "clean up
documents nothing links to" would have destroyed the ADRs and acceptance records
that exist precisely to show why past decisions were made. Losing those is worse
than the disorder being solved.

One addition: **archived content must be excluded from search by default**,
otherwise retirement is cosmetic and the base keeps growing in the only
dimension that matters.

## Consequences

- Dispatch and acceptance acquire a recording step. It must be cheap and a
  natural byproduct of what the controller already does, or it will be cut under
  load — which is the failure this ADR exists to prevent.
- Two sources of truth are avoided: the registry is authoritative for document
  role, status, and supersession; prose in the documents is not.
- An archive grows without bound in K0. Tiering is deferred, not solved.
- The first slice ships no Obsidian projection and changes no existing workflow.
  It only registers and audits, so the claim "the handoff can be generated and
  cannot go stale" becomes testable before anything depends on it.
