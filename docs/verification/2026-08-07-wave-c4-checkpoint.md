# Wave C4 Checkpoint — governed process and read-only Git

Date: 2026-08-07
Controller session: `<session-id>`
Branch / HEAD at checkpoint time: `main` / `62149be9eddd2288eb45fd9b8278641cf64c7a5f`
Staging: empty. This checkpoint records verification only; nothing is committed.

Verdict: **C4 CHECKPOINTED** — D1 through D3B4 are controller-accepted, with the
open risks in §5 recorded rather than closed.

Acceptance detail for the final slice:
`docs/verification/2026-08-07-wave-c4-d3b4-acceptance.md`.

## 1. What C4 delivers

Pillow can, inside the existing conversation-centred workbench:

- propose, approve once, and run a **governed process** through fixed contracts
  and policy, with supervised bounded execution, host-private approval grants,
  bounded memory-only live output, and durable project/conversation-scoped
  lifecycle events;
- surface process activity, a single arbitrated approval bar, and a bounded
  terminal-output contextual pane inside the owning conversation;
- close the provider → governed process → conversation loop over real
  Tauri/WebView2 with the production Grok ACP path;
- discover Git repositories below the active project, show branch/HEAD and a
  bounded status list, and open one bounded diff — as an **explicitly selected**
  mode of the contextual pane, behind native `GitCommandState` authority and a
  strict typed bridge, with project and generation isolation.

The renderer never supplies a native root, executable, argv, cwd or env. Public
error codes are fixed and path-free. Repository ids and paths are portable
project-relative strings.

## 2. Explicitly still out of scope

Interactive shell; Git mutation of any kind (stage, commit, checkout, branch,
merge, rebase, reset, discard, config); remote Git; credential or key
management; diff editing and patch application; Obsidian and later-stage
project-operations features; additional runtimes; arbitrary MCP or
provider-native tools. None of these were started.

## 3. Final controller gates

All run by the controller on the checkpoint tree.

| Gate | Result |
| --- | --- |
| `pnpm check` | exit 0 — renderer **370/370** across 30 files, Rust **731**, Clippy `-D warnings` clean |
| `pnpm --filter @pillow/desktop build` | exit 0 |
| **Complete serial desktop E2E ×3 consecutive** | **10/10, 10/10, 10/10** |
| `cargo fmt --all -- --check` | exit 0 |
| `git diff --check` | exit 0 |
| Staging | empty |
| Residual `pillow.exe` / 1420 listeners | 0 / 0 |

Three consecutive full-suite runs were required deliberately: this wave was
twice misled by a single green run (§5.1).

## 4. Defects this checkpoint found and repaired

The Round 4 controller gate was not a formality — it found four real problems
that every prior gate had missed.

1. **Critical, product.** Git Changes displaced the accepted C2 resource
   browser. `App.tsx` built a Git model without checking whether any repository
   was discovered, and `ResourcePane` ranked it ahead of Files/Preview, making
   tree/search/recents/preview unreachable with no way back. Repaired by making
   Changes an explicitly selected mode with resources as the default; Terminal
   and governed-write Diff keep absolute priority.
2. **Important, product.** A single untracked directory failed whole-repository
   Git status. `git status --porcelain=v2` emits `? vendor/`; the trailing
   separator produced an empty path component that aborted the entire parse as
   `git_path_rejected`. Repaired at the `?`/`!` record parser only, with
   containment unchanged and a regression test proving tracked records still
   reject a trailing separator.
3. **Real race, product.** Two concurrent same-project `listConversations` calls
   both dispatched a whole-list replace with no recency ordering, so a stale
   snapshot could erase the selected conversation. Repaired with a per-project
   monotonic `listSeq` and strict `>` discard. This race was real and is fixed —
   but see §5.1: it was **not** the cause of the failure that prompted it.
4. **Harness.** `pnpm check` could not reach exit 0 on a cold build because
   MSVC's detached `vctip.exe` outlived the 6s cleanup grace. Repaired by
   enabling the existing identity-gated allowance; no cleanup logic weakened.

## 5. Open risks carried past this checkpoint

### 5.1 `cancel-tree` root cause is unproven

The D3B3 `cancel-tree` scenario intermittently timed out waiting for
`composer-cancel`. It failed, then passed with no code change; after the
`listSeq` repair it failed again, then passed again. A dedicated exclusive-lane
investigation could **not reproduce it** (0/5 attempts) and therefore reached
its conclusion from green-path dumps plus static reasoning, not from a captured
RED.

The harness was changed to drop a 20s wait for a process status that provably
can never arrive (`process_started` is emitted only after
`execute_governed_process` returns), and to assert `composer-cancel` right after
send — a strictly stronger assertion than before. That change is justified on
its own merits.

It is **not** established that this fixed the intermittency. The same green-path
dumps show `composer-cancel` still visible after the full 20s wait, which
contradicts the proposed mechanism. Pre-change the suite passed 2 of 3 runs;
post-change 3 of 3. That is not a statistically meaningful difference.

Failure-path instrumentation now writes DOM-gate state and a screenshot to
`.superpowers/c4-cancel-tree-debug/` on a future RED. **Treat the next
`cancel-tree` failure as the first real diagnostic opportunity, not as a new
bug.**

### 5.2 A second intermittent scenario exists

During the investigation, `two-project-isolation` failed once waiting 60s for
`command-approval-bar`. The D3B3 suite therefore has intermittency in at least
two scenarios, which weakens any single-cause explanation of §5.1.

### 5.3 Process lifecycle events emit only after execution returns

`crates/adapter-grok-acp/src/prompt.rs:713-742` emits
`process_proposal_decided` / `process_started` / `process_terminal_outcome` only
after `execute_governed_process` returns. During a long-running approved
process the timeline stays at 待批准 and never shows 运行中. Cancel remains
correct because it is gated on run activity, not process status — but this is a
genuine observability gap inside accepted D3B3 scope.

### 5.4 Contextual pane header labels are redundant

With the new mode switcher, 变更 appears three times at 1024x720 (pane heading,
active tab, and the view's own section label). Cosmetic, but the design requires
this pane to be compact.

### 5.5 Harness portability

`fixtureGitEnv` does not isolate `GIT_CONFIG_GLOBAL`, so fixture setup can read
the user's global gitconfig. It does not write user config.

A developer-specific Git install path was previously hardcoded ahead of the
standard candidates in the product resolver (`git_commands.rs`), its tests, the
Git executor integration test, and the acceptance harness. It was removed before
this checkpoint's commit: only standard install locations remain, with PATH
lookup and the `PILLOW_HOST_GIT` / `PILLOW_TEST_GIT` overrides as fallbacks.

### 5.6 History audit and the secret gate

A full-history scan (117 commits, all refs) before the checkpoint push found:

| Pattern | History | HEAD |
| --- | --- | --- |
| Credential shapes (`xai-`/`sk-`/`ghp_`/PAT/private key) | **0** | 0 |
| Host user-profile paths (`C:\Users\<name>`) | **0** | 0 |
| Keeper session / lease ids | **0** | 0 |
| Developer Git install path | 1 blob | 0 (removed) |

The 305 raw `xai-` hits were false positives: a redaction test asserting
`!lower.contains("xai-")`, the `xai-org/grok-build` upstream reference, and a
security doc listing secret shapes to reject. No key-shaped value exists in any
blob.

The one real residual is `F:\Git\cmd\git.exe` inside
`crates/git-executor/tests/git_read_integration.rs` as committed at `62149be9`
and earlier. It is removed as of this checkpoint but remains in history, which
is now pushed. **Not rewritten**: it discloses a drive letter and a standard Git
layout — no credential, username or personal data — in a private repository,
and rewriting would change every commit SHA and break the checkpoint chain that
committed records reference by SHA. Recorded as accepted rather than silently
dropped.

Separately, 40 documentation files record the literal project root
(`<repo>`). Product, test and harness code is free of real machine paths;
`AGENTS.md` already models the preferred `<workspace>` placeholder convention,
which new documents should follow.

`scripts/check-secrets.mjs` now enforces this mechanically inside `pnpm check`.
It is built around the failure that produced this section: during the checkpoint
a hand-written regex reported "clean" while a developer-specific Git path sat in
product code. The gate therefore **self-tests every pattern against a
known-positive sample and fails closed if any pattern stops detecting**, and it
distinguishes a real machine path from the many synthetic adversarial fixtures
in this repo (`C:\project\..\escape`, `C:\pillow-missing-git-not-real\git.exe`)
by requiring the path to actually exist outside system locations on the
committer's machine. Verified end-to-end: reintroducing the original path makes
the gate fail on that exact line, and removing it makes it pass.

### 5.7 Environment change made outside the repository

A worker ran `corepack enable`, installing pnpm shims into
`C:\Program Files\nodejs\`. Benign and the same pinned pnpm 10.14.0 as
`packageManager`, but it is a machine-level change made without prior
authorization. Recorded so it is not mistaken for repository state.

## 5.8 Follow-up (2026-08-08): what the debt round changed

Recorded here so this checkpoint is not read as still-current. Detail:
`docs/handoff/2026-08-08-round5-tech-debt-status.md`.

- §5.4 (redundant pane labels) — **fixed**.
- §5.5 (fixture `vendor/` bypass, gitconfig isolation) — **fixed**, and removing
  the bypass exposed that §4.2 was only ever half-repaired.
- §5.3 (lifecycle events) — design accepted, **implementation still pending**.
- §5.1 / §5.2 (`cancel-tree`, `two-project-isolation`) — **still open**. A
  dedicated exclusive-lane investigation failed to reproduce in 8 runs and
  correctly declined to invent a cause. Failure-path instrumentation is now in
  place; treat the next RED as the diagnostic opportunity.

**Correction to §4.2.** The untracked-directory defect was reported there as
repaired. That was wrong: the same portable-path rule is implemented
independently in three layers — the parser (`status.rs`), the project-path
mapping (`discovery.rs`), and the renderer DTO validation
(`tauriGitGateway.ts`). Round 4 fixed only the first, and the acceptance
fixture's `.gitignore` bypass hid the rest. All three are now aligned with
regression tests at each layer, but the duplication itself is architectural
debt: it is the structural reason two consecutive rounds believed the defect
was closed. Converge the rule to a single source of truth before trusting any
future "path validation" change.

## 6. Next boundary

No new capability may start without an accepted design change. The candidates
are the remaining C4 boundary work or D runtime expansion; both require separate
design authorization. §5.1 should be resolved before C4 is treated as closed for
the purposes of building further process features on top of it.
