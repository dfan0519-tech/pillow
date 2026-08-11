# Pillow Stage 1 Overall Status

Date: 2026-07-31; updated 2026-08-11

Status: Stage 1 active. C3 accepted. C4 checkpointed through D3B4, then
**superseded on 2026-08-11**: the governed boundary that C1-C4 assumed did not
actually exist, was rebuilt, and governed write and governed shell now run over
ACP for a real agent. See "2026-08-11 Boundary Rebuild" below before reading the
milestone table — several rows describe capability that was, at the time,
exercised only by the fixture agent.

Authority:

- Product design:
  `docs/superpowers/specs/2026-07-29-pillow-first-design.md`
- C1 checkpoint:
  `docs/verification/2026-07-31-wave-c1-checkpoint.md`
- C2 checkpoint:
  `docs/verification/2026-08-01-wave-c2-checkpoint.md`
- Completed C2 plan:
  `docs/superpowers/plans/2026-07-31-c2-read-only-workbench-completion.md`

## Locked Product Sequence

1. Stage 1: industrial local AI IDE.
2. Stage 2: governed Obsidian knowledge layer.
3. Stage 3: one-person-company project operating system.

Only Stage 1 is active. `legacy Keeper (out of tree)` remains a frozen,
independent resource manager and is not a Pillow product-code destination.

## 2026-08-11 Boundary Rebuild (read this first)

A live probe against a real Grok established that Pillow's tool boundary had
never taken effect. Three independent failures stacked:

1. `_meta.agentProfile` omitted the required `name` / `description`, so upstream
   failed to parse it, logged one line, and **silently fell back to its full
   default toolset**;
2. `tools: []` means *inherit all* upstream, not "none";
3. several disallowed names were wrong (the real short ids are `write` and
   `run_terminal_cmd`).

Net effect: through C1-C4 the governance chain was exercised **only by the
fixture agent**. A real agent could read and write freely without raising a
proposal. Evidence and reproduction:
`docs/research/2026-08-10-mcp-tool-exposure-probe.md`.

What replaced it:

| Surface | Mechanism |
| --- | --- |
| Native tools | Allowlist (fail-closed). A denylist is an enumeration, and a live probe watched the agent leave through a cell nobody had enumerated |
| Ambient MCP | Cut by denying the two MCP meta-tools — the only lever a client has, because `mcpServers: []` does not mean "no MCP" |
| Write | Native `write` -> ACP `fs/write_text_file` -> existing governed write chain |
| Process | Native `run_terminal_cmd` -> ACP `terminal/*` -> same chain |
| Rules | Conservative segmentation, allow is conjunctive per segment, deny wins, unsplittable asks |
| Modes | `default` / `accept_edits` / `bypass`; bypass turns asking off, not governance |

Three half-open traps are documented in the controller handoff: giving the agent
a hand without opening the line to the host makes it act locally, silently, with
no error. Two coupling tests pin them.

**Only Grok Build speaks ACP.** Codex exposes `mcp-server` and `app-server` but
no ACP mode, so a CLI that does not speak ACP cannot be governed by Pillow at
all — it can only be launched. That is why the built-in agent shell is Grok
Build, with the model swapped via the user's own key.

Standing rule from this day: **a boundary claim needs live evidence.** Reading
configuration code produced three wrong conclusions in a row.

---

## Stage 1 Capability Ladder

| Milestone | State | Current truth |
| --- | --- | --- |
| A desktop foundation | Accepted | Tauri/React shell, native project entry, SQLite project authority |
| B1 conversation kernel | Accepted | Durable project-scoped conversations, streaming, cancellation, recovery |
| B2 Grok ACP | Accepted | Real adapter behind runtime-neutral boundary |
| B3 governed reads | Accepted | Fixed `fs.list@1`, `fs.read@1`, `fs.search@1`, receipts and replay |
| C1 provider-to-B3 loop | Accepted checkpoint | Provider proposals reach governed reads and owning timeline |
| C2 read-only workbench | Accepted checkpoint | Project-scoped resources/activity, isolation, restart recovery, full gates and independent review accepted |
| C3 governed write/diff/approval | Accepted checkpoint | One bounded UTF-8 create/modify, host diff, approve-once/reject, base-hash CAS, atomic apply, receipts, isolation and restart invalidation; full controller gates green |
| C4 terminal and Git | Superseded by the 2026-08-11 rebuild | D1-D3B1 foundations, D3B2 process workbench mounting, D3B3 provider-to-governed-process real desktop loop, and D3B4 read-only Git Changes have each passed controller source review and full gates. D3B4 Round 4 gate: `pnpm check` exit 0 (renderer 370/370, Rust 731, Clippy `-D warnings`), production build. That gate found and repaired a Critical regression (Git Changes displaced the accepted C2 resource browser) and an Important defect (one untracked directory failed whole-repository Git status). Checkpointed on 2026-08-07 after three consecutive 10/10 serial desktop E2E runs; the intermittent D3B3 `cancel-tree` scenario is carried as an open risk with an unproven root cause (see the checkpoint record). Interactive shell and Git mutation remain unstarted |
| D multi-slot runtimes | In progress (2026-08-11) | Slots with API-key or login auth, Grok Build as the built-in shell. Non-ACP CLIs are recorded and their account shown, but explicitly **not governed** |
| E industrial Alpha | Not started | Packaging, import, security, performance, diagnostics |

## C2 Status

| C2 unit | State | Controller evidence | Product availability |
| --- | --- | --- | --- |
| C2-A resource core + SQLite recents | Accepted | Resource core 9/9 after blank-query/Unicode hardening; fmt, Clippy and diff | Backend-ready |
| C2-B typed Tauri/TS bridge | Accepted | Native command 16/16; typed renderer gateway 22/22; opaque error contract | Bridge-ready |
| C2-C1 collapsible resource panel | Accepted | Vitest 8/8, typecheck, diff | Mounted in contextual pane |
| C2-C2 conversation Agent activity | Accepted + mounted | Raw provider output fail-closed; structured activity retained; final desktop Vitest 206/206; Codex2 Critical 0 / Important 0 | Mounted in owning conversation timeline |
| C2-C3 read-only preview | Accepted | Rust 9/9, Vitest 18/18, typecheck, fmt, diff; Codex2 re-review clean | Mounted in contextual pane |
| C2-D resource workbench integration | Accepted | Controller race gate 10/10, six-suite regression 100/100, typecheck, production build, diff; Codex2 final review clean | Available for project-scoped browse/search/recents/preview |
| C2-E isolation/restart/desktop acceptance | Accepted | Focused real Tauri/WebView2 1/1; full desktop E2E 7/7; normal close and process cleanup | Verified |
| C2 checkpoint + independent review | Accepted | Full gates green; independent Codex2 final review ACCEPT; controller verdict ACCEPT | Checkpoint recorded |

C2 is now checkpointed. The resource panel and bounded preview share one
contextual project-scoped gateway. Real plan/tool/sub-agent/artifact activity
is projected into the owning conversation with stable internal identity and
portable resource routing. Provider-controlled raw tool output is never
rendered by the production disclosure; the UI shows a fixed returned-output
state and exposes governed artifacts and structured test results separately.

## C2 Completion Definition

C2 is accepted only when the desktop can:

1. switch between two projects without resource, preview, or activity leakage;
2. browse tree/search/recents through the typed project-scoped bridge;
3. open bounded text, Markdown, image, and basic artifact previews without
   renderer filesystem access;
4. show real plan/tool/sub-agent/output/test activity inside its owning
   conversation through progressive disclosure;
5. restart and restore projects, conversations, resource recents, and
   conversation-owned activity without inventing state;
6. pass focused tests, full `pnpm check`, production UI build, desktop E2E,
   `cargo fmt --check`, `git diff --check`, process cleanup, and independent
   review.

## Explicit C2 Exclusions

- file writes, rename, delete, patch application, or save;
- terminal and process execution;
- Git status, diff, branch, worktree, commit, restore, or publish;
- Obsidian or company-operation features;
- credential/key management;
- Codex/OpenCode or other runtime expansion;
- arbitrary MCP/provider-native/network tools.

## Checkpoint Governance

- The protected pre-checkpoint baseline was `7b7f70f`.
- The user authorized an audited checkpoint commit on 2026-08-04 after the
  controller completed the D3AR2 aggregate gate.
- Historical worker session and lease identifiers plus machine-specific paths
  are redacted from committed execution records.
- D3B1 integration seams are controller-accepted; D3B2 must mount them without
  reopening process authority or creating a second dashboard.

## Final C2 Gate

Fresh controller evidence on 2026-08-01:

- `pnpm check`: exit 0; Vitest 206/206, Rust workspace/doctests, Clippy
  `-D warnings`;
- production UI build: exit 0, 68 modules;
- focused C2-E real desktop acceptance: 1/1;
- complete serial desktop E2E: 7/7 in one run;
- `cargo fmt --all -- --check`, `git diff --check`, and cached diff check:
  exit 0;
- independent Codex2 review: ACCEPT, Critical 0 / Important 0;
- HEAD `7b7f70f`, staging empty, relevant residual processes 0.

## C3 Accepted Boundary

The accepted C3 design is
`docs/superpowers/specs/2026-08-01-c3-governed-write-design.md`; execution is
tracked by
`docs/superpowers/plans/2026-08-01-c3-governed-write-implementation.md`.

C3.0 implements one bounded UTF-8 create/modify proposal at a time, with a
host-built diff, mandatory one-shot human approval, base-hash compare-and-swap,
atomic host apply, restart invalidation and durable receipts. Rename, delete,
binary writes, directories, terminal, Git, Obsidian, credentials, arbitrary
MCP/provider-native tools and runtime expansion remain outside C3.

Final execution status: C3-A protocol/policy, C3-B durable metadata-only
ledger, C3-C bounded diff/atomic host executor, C3-D strict
provider/runtime/native bridge, C3-E approval/diff UI and C3-F real desktop
adversarial acceptance are accepted. C3-A passed
55 protocol tests, 16 policy tests and 13 executor tests. C3-B passed the full
storage suite and workspace all-targets compile after two controller
rejection/fix passes. C3-C passed controller source review only after two
additional security corrections removed a production-visible mutation hook
and made apply cancellation lease-owner-aware. Fresh controller evidence for
C3-C: write repository 24/24, focused write executor 28/28, executor all
targets 46 tests, protocol 55 tests, policy 16 tests, full storage tests,
workspace all-targets check, fmt, diff check and empty staging. C3-D passed two
controller rejection/fix cycles followed by focused runtime, executor, storage,
adapter, native and resource tests, full workspace check/Clippy, `pnpm check`,
production build, fmt and diff gates. C3-E then passed two controller
rejection/fix cycles and fresh 45/45 focused UI, 243/243 desktop UI, typecheck,
production build and Git invariant gates. C3-F then passed all eight real
Tauri/WebView2 scenarios, process/profile safety, production build, the full
246-test renderer gate, Rust workspace/doctests and Clippy, and all 8 serial
desktop E2E cases. Detailed evidence is in
`docs/verification/2026-08-02-wave-c3-acceptance.md` and the checkpoint in
`docs/verification/2026-08-02-wave-c3-checkpoint.md`.

C4 implementation is active under the accepted governed-process/Git design.
The D1 protocol, policy, Git parser and isolated UI foundations are accepted
after controller correction passes removed schema, parser, Clippy and renderer
grant-boundary defects. D2A is also accepted after controller rejection and
repair passes fixed capture-before-resume ordering, unread ACP stderr,
single-claim execution authority, owner bypass and cancellation receipt
coupling. Fresh D2A evidence is renderer 269/269, Rust workspace/doctests,
workspace Clippy, process executor 10/10, ACP 95 passed with one explicit
live-only ignore, execution repository 25/25, production build 73 modules,
fmt/diff checks, unchanged HEAD and empty staging. D2B workers completed the
governed process gateway and hardened read-only Git runner, and their focused
and package tests plus workspace checks were green. Controller source review
nevertheless rejected both slices: the process gateway left raw argument,
project-capability and native-authority bypasses, while the Git runner had a
fail-open configuration probe that did not parse Git's real NUL wire form plus
filesystem containment and stale-generation gaps. D2BR1 workers completed
their listed repairs and all focused gates were green, but the controller
rejected again: the process gateway still exposed its mutation-capable store
and a mutable fixed-timeout seam, while Git metadata reads could still follow
config/commondir/hooks/attributes links outside project authority. D2BR2 was
then dispatched to actual Grok2 and Grok4 with non-overlapping allowlists. Both
D2BR2 repairs subsequently passed controller source review, focused suites,
workspace check/Clippy, full `pnpm check` with renderer 269/269, production
build, fmt/diff and Git invariant gates. D2B is accepted. The first D3A
delivery was rejected because it recreated mutable SQLite authority, exposed
test expiry and grant material, and omitted real output and the fixed event
contract. D3AR/D3AR2 then repaired those defects, completed the typed desktop
foundation, and passed the fresh controller aggregate gate recorded below.
D3B1 native process commands, strict process-event projection and the pure
write/process approval arbitration model are now controller-accepted as an
integration foundation. D3B2 workbench mounting and real process-governance
desktop E2E are the next Stage 1 slice. Stage 2 work and runtime expansion
remain outside the active product scope; the controller continues to own every
acceptance verdict and aggregate gate.

## C4 D3AR2 Controller Acceptance

D3AR2 closes the rejected D3A authority/output/event gaps without creating a
second process ledger. Runtime host continues through the governed process
gateway, approval grants remain host-private, live output is bounded and
memory-only, and proposal/decision/start/terminal events are durable and
project/conversation scoped.

Fresh controller evidence on 2026-08-04:

- `pnpm check`: exit 0; desktop Vitest 272/272, Rust workspace and doctests,
  Clippy `-D warnings`;
- production UI build: exit 0;
- real serial desktop E2E: 8/8 in 711.8 seconds;
- `cargo fmt --all --check` and `git diff --check`: exit 0;
- staging was empty and relevant residual process count was zero before the
  user-authorized checkpoint preparation.

Detailed evidence is recorded in
`docs/verification/2026-08-04-wave-c4-d3ar2-acceptance.md`.

## C4 D3B1 Controller Acceptance

D3B1 adds the native list/decide/open-output command composition, carries the
four durable process events through strict renderer normalization into fixed
conversation-owned activity notices, and introduces a deterministic
write/process approval arbitration model. The arbitration model is not yet
mounted, so this is an integration-foundation acceptance rather than complete
D3B product acceptance.

Fresh controller evidence on 2026-08-04:

- focused renderer tests: 45/45; focused native command tests: 6/6;
- desktop TypeScript typecheck: exit 0;
- `pnpm check`: exit 0; desktop Vitest 281/281, Rust workspace and doctests,
  Clippy `-D warnings`;
- production UI build: exit 0, 73 modules;
- real serial desktop E2E: 8/8 in 772.2 seconds;
- `cargo fmt --all -- --check` and `git diff --check`: exit 0;
- HEAD `62149be9eddd2288eb45fd9b8278641cf64c7a5f`, staging unchanged, relevant
  residual process count zero.

Detailed evidence is recorded in
`docs/verification/2026-08-04-wave-c4-d3b1-acceptance.md`.
