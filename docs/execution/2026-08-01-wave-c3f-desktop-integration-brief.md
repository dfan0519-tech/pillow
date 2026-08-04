# Wave C3-F Brief — Real Desktop Integration And Adversarial Acceptance

Owner: grok3 harness worker; controller owns the verdict

Controller session: `<keeper-session>`

Status: ACTIVE. C3-D and C3-E are controller-accepted.

## Goal

Prove the production Tauri/WebView2 path from a fake ACP
`pillow/write_tool` request through the conversation-owned approval UI to
governed disk application and the exact provider receipt. This is an
acceptance harness, not another product implementation wave.

## Required Scenarios

1. Reject create: one bar, bounded diff opens, Reject produces durable
   terminal evidence, provider receives fixed rejection, target stays absent.
2. Approve create: Approve once produces exactly one file with exact UTF-8
   bytes; provider receives matching path/mode/bytes/digest; replay does not
   write twice.
3. Approve modify: captured base matches, atomic replacement succeeds, success
   alone offers Open file through the resource pane.
4. Base drift: establish the external drift as explicit test setup after the
   proposal is pending; approval fails with fixed `base_changed`, external
   bytes are not clobbered.
5. Cancellation while pending: Stop cancels the owning run/waiter, removes the
   active bar, leaves no mutation, and does not affect another conversation.
6. Two projects/two conversations: proposal, grant, diff, terminal outcome and
   resource open never appear in the foreign selection; switching back
   restores the exact native pending state.
7. Forced restart while pending: restart recovery marks the row interrupted,
   exposes neither grant nor diff, never auto-applies, and leaves target/base
   unchanged. A prior terminal success remains visible as history.
8. Conflicting call replay: same call identity with changed content fails
   closed and cannot create a second proposal or apply.

Expiry must be proven in deterministic Rust/native tests. Do not wait ten
minutes in desktop E2E and do not add a production-visible clock or mutation
hook. A database-time test fixture is allowed only in a test-only harness and
must never touch project contents or ship in the production command surface.

## Visual Gates

- Capture approval bar with resource pane closed and with bounded diff open at
  the supported Windows desktop sizes used by C2 acceptance.
- Conversation remains visually dominant; no dashboard rail or permanent
  approval-card stack.
- No grant, proposal/run/session id, native path, raw JSON or provider output
  is visible in screenshots or accessibility text.
- Keyboard: initial Approve focus, Tab loop, Escape rejects only while the bar
  owns focus, and global Stop is not triggered by that Escape.

## File Whitelist

- `apps/desktop/tests/e2e/c3-governed-write.spec.ts`
- `scripts/wave-c3-governed-write-acceptance.mjs`
- `scripts/lib/c3-governed-write-acceptance.mjs`
- `apps/desktop/package.json`
- fake ACP fixture/script files already whitelisted by C3-D only when an
  additional scenario is compilation-proven necessary
- `docs/execution/2026-08-01-wave-c3-integration-report.md`
- Generated evidence only: `.superpowers/c3-acceptance/*.png` and
  `.superpowers/c3-acceptance/*.json`

Do not change product renderer/native implementation. If a product defect is
found, stop with a minimal reproduction and let the controller issue a
separate non-overlapping fix brief.

## Process And Evidence Rules

- Drive actual Tauri commands and WebView2 UI. Do not call write executor or
  mutate the target file directly to simulate approval success.
- Reuse the C2 supervised-process, isolated `CARGO_TARGET_DIR`, temporary app
  data/WebView2 profile, build-time CDP and readiness helpers. Do not use the
  repository debug executable and do not bake CDP arguments into production
  configuration.
- Use `KEEPER_FAKE_ACP_SCRIPT` through the production runtime composition. If
  C3-D's existing `governed-write-create` fixture cannot express a required
  scenario, add the smallest deterministic fake-agent scenario and prove it in
  the adapter tests; do not add a production test command or clock.
- Test setup may create isolated temporary project roots and seed pre-run base
  files; after a proposal is pending, only the named base-drift scenario may
  externally alter its target.
- Serialize desktop cases that share ports, app-data roots or process names.
- On every case, record pre/post file SHA-256, proposal terminal state, provider
  response, screenshot path where applicable, and residual process count.
- End with normal-close cleanup plus forced-restart cleanup. Relevant Pillow,
  fake ACP, Node, Cargo/Rust and test-owned WebView2 residuals must be zero.

## Gates

- focused C3 desktop acceptance serially;
- complete desktop E2E serially after focused green;
- `pnpm check`;
- `pnpm --filter @pillow/desktop build`;
- `cargo fmt --all -- --check`;
- `git diff --check`;
- `git diff --cached --name-only` empty.

## Completion

Write the integration report with exact commands, pass counts, screenshots,
hashes, restart/cross-project evidence, residual process counts, changed
files, risks and `COMPLETION: DONE` or `COMPLETION: BLOCKED`. Do not stage or
commit.
