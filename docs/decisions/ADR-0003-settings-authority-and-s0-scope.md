# ADR-0003: Settings Authority Moves To SQLite; S0 Ships Before Credentials

Date: 2026-08-08

Status: Accepted

## Context

Runtime selection is currently **env-only**. `apps/desktop/src-tauri/src/runtime_composition.rs`
reads `PILLOW_RUNTIME`, `PILLOW_GROK_EXECUTABLE`, and `PILLOW_GROK_PROFILE` and has
no other authority. That is workable for a developer on one machine and unshippable
as a product: an ordinary user has no way to point Pillow at their Grok CLI without
editing machine environment variables.

`docs/superpowers/specs/2026-08-08-settings-credentials-design.md` proposed splitting
the work into **S0** (non-secret settings in SQLite + UI) and **S1** (OS-keychain-backed
credentials), and left nine questions to the controller.

The important finding in that design is a **correction to an earlier controller
assumption**: credential management was believed to be a hard blocker for external
delivery. It is not. Users who authenticate through the Grok CLI never need Pillow to
hold a secret. S0 alone removes the env requirement for them.

## Decision

### 1. SQLite becomes the authority for non-secret settings; env becomes an override

This preserves the project-wide rule that SQLite is the domain authority. Env is
retained as an allow-listed override rather than removed, because the entire E2E
harness and every acceptance script depend on env injection
(`PILLOW_RUNTIME`, `PILLOW_GROK_EXECUTABLE`, `PILLOW_GROK_PROFILE`, plus
`PILLOW_HOST_GIT` / `PILLOW_TEST_GIT` elsewhere). Removing env authority would
require rewriting the harness in the same change that introduces the settings layer —
two risky things at once.

**Amendment to the design (Q7):** an active env override must be **visible in the
settings UI** as an explicit "overridden by environment" state. A silent override
where the UI shows one value and the process uses another is a support trap, and we
already burned a round on exactly that class of bug (a green gate that hid an
unformatted file, a scanner that reported "no findings" while the finding sat at
line 878). Settings must not join that list.

### 2. S0 ships as its own plan, before S1

S0: `settings` tables + migration, host `SettingsService` with documented precedence,
desktop UI for runtime selection and the two Grok paths, and `runtime_composition`
reading resolved settings. No secret values exist in S0, so "no secrets in renderer
DTOs" is vacuously true and must be asserted anyway to keep it true later.

### 3. Settings edits do **not** go through C3 proposal/grant

C3 approval exists because a **provider** proposes a **filesystem mutation** the human
must review. Settings edits are user-originated in a first-party surface. Routing every
theme toggle through `approve_once` would train users to click through approvals and
**weaken the signal for real agent writes** — the approval prompt is the product's
core safety primitive and its value is entirely in how rarely it appears.

Credential lifecycle (S1) is the exception: enroll / rotate / delete reuse the
**grant + receipt machinery**, not a second policy framework.

### 4. Open questions Q1–Q9

| ID | Decision | Note |
| --- | --- | --- |
| Q1 | **(a)** two separate plans | S0 first |
| Q2 | **(c)** `fake` adapter in dev builds only | env override still selects it for automation; invalid stored enum **fails closed**, never silently falls back to Fake |
| Q3 | **(b)** global + project scope | project isolation is a governance primitive here, not a preference |
| Q4 | **deferred to S1** | not decidable usefully before the enroll surface exists |
| Q5 | **(a)** omit content fingerprint | |
| Q6 | **deferred to S1** | |
| Q7 | **(a)** retain allow-listed env override | **plus** the UI-visibility amendment above |
| Q8 | **`0009_settings.sql`** | verified against the live tree: `0001`–`0008` are taken |
| Q9 | **(a)** interim external delivery allowed | claim must read "Grok authentication is managed by the Grok CLI; Pillow does not store that login" |

### 5. C4 boundary is not reopened

Settings are soft configuration. `process.run@1` proposal/approval/grant/exec,
the child-environment denylist, and Git read G0 are untouched. S1 must **not**
introduce renderer-supplied env or free-form secret injection into process tools.
"Inject an API key into an approved network tool" would be a **new governed
capability with its own approval object** — never a settings flag.

## Consequences

- External beta becomes possible without keychain work, under a constrained claim.
- We must not market generic provider credential management until S1 is accepted.
- The precedence chain (env → project → global → default) becomes a thing that can be
  wrong in a way users can't see. It needs its own tests, and the UI must surface
  which layer won.
- One more schema migration to maintain; `0009` is reserved by this ADR.
