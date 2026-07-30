# Wave B2 Live Hardening Plan

Date: 2026-07-30
Base: `13ccb02`

## Why This Wave Exists

The protocol core and runtime-neutral composition boundary are merged, but B2
is still HOLD. Controller review found three missing industrial guarantees:

1. `probe()` currently treats an executable file as healthy without factual
   authentication evidence.
2. Windows cleanup proves only that the direct child exited, not that every
   descendant exited.
3. the conversation service still starts sessions with no project working
   directory, so the real adapter cannot open a project-scoped session.

## Package H1: Adapter Health And Process Ownership

Editable scope: `crates/adapter-grok-acp/**`, its Cargo metadata and tests.

Requirements:

- Replace file-presence probe semantics with explicit states:
  `unavailable`, `transport_ready_auth_unverified`, `healthy`, and
  `authentication_failed`.
- A successful `initialize` proves transport/protocol only. It must not claim
  authentication.
- Only a successful real prompt may establish `healthy=true`; recognized
  authentication failures clear it.
- Keep probe detail stable and secret-free; record the observed agent version.
- On Windows, own the Grok process tree with a Job Object configured for
  kill-on-close (or an equally strong native primitive).
- Cancellation/shutdown must fail if tree cleanup cannot be proved.
- Add a fake child that creates a descendant and tests that cancel/drop removes
  both.
- Keep the production ACP argv fixed. Test-only fake-child argv overrides must
  not become an untrusted product input.

## Package H2: Project-Scoped Desktop Wiring

Editable scope: desktop Rust service/composition and directly related tests.

Requirements:

- Add the Grok adapter dependency at the desktop composition layer only.
- Resolve each conversation's owning project from SQLite before
  `start_session`.
- Canonicalize and validate the project root before passing it as
  `working_directory`.
- Reject a missing/non-directory/out-of-scope project root before spawning.
- Production desktop startup uses `GrokAcpAdapter`; Fake is available only
  under explicit development/test selection.
- Discover executable and profile through local, non-secret configuration with
  truthful failure messages. Never read auth file contents.
- Use one concurrent Grok prompt until an adapter process pool is implemented.
- Preserve `validate -> persist -> commit -> emit` and all B1 recovery rules.

## Package H3: Controlled Native Acceptance Harness

Editable scope: new acceptance scripts/tests and verification documentation.

Requirements:

- Run only when explicitly enabled; normal CI must report SKIP, not PASS.
- Snapshot baseline Grok/Keeper process ids.
- Exercise real initialize, session/new, one read-only text turn, cooperative
  cancel, shutdown, and restart timeline recovery where available.
- Verify adapter name `grok-acp`, exact single terminal, committed timeline,
  no write permission, and zero new residual process ids.
- Never print account identifiers, profile contents, raw provider envelopes,
  tokens, or auth file data.
- Produce a machine-readable result plus a human verification report.

## Integration Order

1. H1 adapter hardening.
2. H2 desktop project wiring, rebased on H1 if the public config changes.
3. H3 harness adjustment and controlled live run.
4. Full workspace gates, independent review, and B2 PASS/HOLD report.

## Non-Claims

- No multi-provider routing.
- No write-capable tool approval UI.
- No durable cross-restart ACP session-id storage yet.
- No provider process pool; concurrency is intentionally one.
