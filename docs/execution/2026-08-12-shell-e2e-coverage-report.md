# Shell proposal → UI E2E coverage report

Date: 2026-08-12  
Worker: `grok1` (write)  
Controller: Claude  
Scope: fake ACP shell scenario + desktop E2E shell stages  
Status: **COMPLETE**

## Completion signal

`SHELL-E2E-COVERAGE-COMPLETE`

## Why this ticket existed

Governed shell shipped unusable on a real machine: host correctly sent
`ruleSubject = ""` for per-command shell proposals; the renderer validator
required a non-empty subject; `normalizeExecProposal` threw; `listExecProposals`
threw; the approval bar never appeared. Gates and E2E stayed green because
`c4-governed-process` only drove structured `pillow/process_tool` proposals —
`terminal/*` had no UI coverage. Fix landed earlier; this ticket closes the
hole so the same silent disappearance fails red next time.

## Write boundary (this worker)

**In scope (written):**

- `crates/adapter-grok-acp/src/bin/fake_acp_agent/main.rs`
- `crates/adapter-grok-acp/src/bin/fake_acp_agent/scripts.rs`
- `apps/desktop/tests/e2e/c4-governed-process.spec.ts`
- `scripts/lib/c4-governed-process-acceptance.mjs`
- `scripts/wave-c4-governed-process-acceptance.mjs`
- this report

**Forbidden (codex1 concurrent — not touched by this worker):**

- `crates/kernel-policy/**`
- `crates/runtime-host/**`
- `crates/storage-sqlite/**`
- `crates/tool-executor/**`
- `crates/agent-protocol/**`

**Temporary mutation only (restored):**

- `apps/desktop/src/lib/tauriExecGateway.ts` — shell `ruleSubject === ""`
  exemption probe for T3

**Not run by this worker:**

- Desktop E2E (controller-owned after both parallel tickets land)

## Exit-code table

All commands used isolated `CARGO_TARGET_DIR` for Cargo. No command is piped
through `tail` or another exit-code-masking consumer.

| Gate / probe | Command | Exit code | Result | Notes |
|---|---|---:|---|---|
| Fake agent compile | `cargo build -p keeper-adapter-grok-acp --bin fake_acp_agent` | 0 | PASS | Isolated target `target-shell-e2e` |
| Syntax: process UI helpers | `node --check scripts/lib/c4-governed-process-acceptance.mjs` | 0 | PASS | |
| Syntax: wave orchestrator | `node --check scripts/wave-c4-governed-process-acceptance.mjs` | 0 | PASS | |
| Syntax: e2e wrapper | `node --check apps/desktop/tests/e2e/c4-governed-process.spec.ts` | 0 | PASS | |
| Secrets gate | `node scripts/check-secrets.mjs` | 0 | PASS | |
| T3 mutation RED | `vitest run tests/c4-gateway-contract.test.ts -t "accepts a shell proposal with no rule subject"` (exemption removed) | 1 | EXPECTED-RED | `ExecGatewayError: Pillow received an invalid process response.` |
| T3 restored GREEN | same focused vitest (exemption restored) | 0 | PASS | |
| Gateway restore proof | `git diff -- apps/desktop/src/lib/tauriExecGateway.ts` | 0 | PASS | Empty content diff (file restored; only CRLF warning) |
| Desktop E2E | *(not run — controller)* | — | DEFERRED | See § controller expectations |

## T1 — fake agent shell scenario

### Script names

| Script / tag | Role |
|---|---|
| `acp-governed-shell` | Standalone approve path via `terminal/*` |
| `acp-governed-shell-reject` | Standalone reject path via `terminal/*` |
| `governed-process-desktop` + `[c4p:shell-approve]` | Desktop multiplex approve |
| `governed-process-desktop` + `[c4p:shell-reject]` | Desktop multiplex reject |

### Sealed command (exact string shared with E2E)

```text
echo shell-e2e "double" 'single'
```

Constant: `SHELL_E2E_QUOTED_COMMAND` in
`crates/adapter-grok-acp/src/bin/fake_acp_agent/scripts.rs`  
Mirrored in `scripts/wave-c4-governed-process-acceptance.mjs` as
`SHELL_E2E_QUOTED_COMMAND`. Both `"` and `'` are intentional so JSON
escape / truncation of the display path fails character-equality.

### `terminal/*` sequence (approve)

1. `terminal/create` — `command` = sealed string, `args=[]`, `env=[]`,
   optional absolute `cwd` = session root, `outputByteLimit=4096`
2. *(host blocks on human approval + governed execute)*
3. On success result: read `terminalId`
4. `terminal/wait_for_exit`
5. `terminal/output`
6. `terminal/release`
7. Agent message: `acp-governed-shell-ok` (or `…-failed`)

### `terminal/*` sequence (reject)

1. `terminal/create` (same params)
2. Expect RPC error message containing `governed_terminal_rejected`
   (also accepts related rejected markers)
3. Agent message: `acp-governed-shell-reject-ok` (or `…-failed`)

Shape matches production Grok foreground lifecycle documented in
`docs/execution/2026-08-11-governed-shell-report.md` and
`crates/adapter-grok-acp/src/provider_terminals.rs`.

## T2 — E2E shell stages

### Orchestrator scenarios (new)

| Scenario id | Prompt tag | Assertions |
|---|---|---|
| `shell-approve` | `[c4p:shell-approve]` | `data-testid="command-approval-bar"` **visible**; `data-command-kind="shell"`; displayed command **===** sealed string (via `data-full-command`); quotes retained; Approve → activity `已成功` exit 0; assistant text `acp-governed-shell-ok` |
| `shell-reject` | `[c4p:shell-reject]` | Bar visible + exact command match; Reject → activity `已拒绝`; not left `运行中`; assistant text matches reject-ok/failed |

### Helper changes

- `waitForCommandApproval` accepts `commandKind: "shell"` and
  `expectedExactCommand` (character-identical compare).
- New `waitForShellCommandApproval({ expectedCommand })` wrapper.
- **Hard requirement:** bar visibility is asserted; “no crash” alone is not
  enough (that is what previously shipped green over a dead approval surface).

### Required scenario lists updated

- `scripts/wave-c4-governed-process-acceptance.mjs` → `REQUIRED_SCENARIOS`
- `apps/desktop/tests/e2e/c4-governed-process.spec.ts` → `REQUIRED_SCENARIOS`

Both now require `shell-approve` and `shell-reject`.

## T3 — mutation evidence

### What was mutated

In `apps/desktop/src/lib/tauriExecGateway.ts`, the shell exemption:

```ts
|| (value.programLabel === SHELL_PROGRAM_LABEL && ruleSubject === "")
```

was temporarily removed so `subjectOk` only accepted `isRuleSubjectValue`
(non-empty). Host-shaped shell DTOs with `ruleSubject: ""` then fail
`normalizeExecProposal`.

### RED evidence (exemption removed)

```text
FAIL  tests/c4-gateway-contract.test.ts > C4 gateway contract > accepts a shell proposal with no rule subject
ExecGatewayError: Pillow received an invalid process response.
  ❯ invalid src/lib/tauriExecGateway.ts:36:10
  ❯ Module.normalizeExecProposal src/lib/tauriExecGateway.ts:68:145
Exit code: 1
```

Causal chain to the new E2E:

1. Host still emits shell proposals with `ruleSubject=""`.
2. Without the exemption, `normalizeExecProposal` throws on every list/decide.
3. `listExecProposals` fails closed; the conversation never mounts a pending shell proposal.
4. New scenarios `shell-approve` / `shell-reject` call
   `waitForShellCommandApproval`, which **requires**
   `data-testid="command-approval-bar"` visible within timeout.
5. That wait fails with `command-approval-bar wait failed; everVisible=false`
   — the same silent-disappearance shape as the original production bug.

Desktop E2E itself was not run by this worker (controller-owned). The unit
probe exercises the **same validator branch** the E2E would hit first; the
new E2E adds the bar-must-render and character-exact command assertions that
a gateway-only unit test cannot cover.

### GREEN evidence (exemption restored)

```text
✓ tests/c4-gateway-contract.test.ts (11 tests | 10 skipped)
Tests  1 passed | 10 skipped
Exit code: 0
```

### Restore proof

```text
git diff -- apps/desktop/src/lib/tauriExecGateway.ts
# empty content diff
git status --short -- apps/desktop/src/lib/tauriExecGateway.ts
# (no line — file not dirty)
```

## Write-boundary audit

| Path class | Status |
|---|---|
| This worker’s allowlist files | Modified as listed above |
| `apps/desktop/src/lib/tauriExecGateway.ts` | Restored; not in final dirty set |
| `crates/kernel-policy/**` | Dirty in worktree from **codex1** concurrent work — **not modified by this worker** |
| `crates/runtime-host/**` | Same (codex1) |
| `crates/storage-sqlite/**` | Same (codex1) |
| `crates/tool-executor/**` | Not in this worker’s writes |
| `crates/agent-protocol/**` | Dirty from codex1 — not modified by this worker |
| Git write operations | None (`commit` / `push` / branch ops not used) |

Final `git status --short` for this worker’s paths only:

```text
 M apps/desktop/tests/e2e/c4-governed-process.spec.ts
 M crates/adapter-grok-acp/src/bin/fake_acp_agent/main.rs
 M crates/adapter-grok-acp/src/bin/fake_acp_agent/scripts.rs
 M scripts/lib/c4-governed-process-acceptance.mjs
 M scripts/wave-c4-governed-process-acceptance.mjs
?? docs/execution/2026-08-12-shell-e2e-coverage-report.md
```

## Residual risks — host → renderer contracts still without E2E

These are the next places a “something silently does not appear” bug can hide.
Listed deliberately after three green-gate silent failures:

1. **Write approval DTO widen / `ruleSubject` empty-or-new-field**  
   `tauriWriteApprovalGateway` has a closed key allowlist like exec. A host
   field change that the write path rejects still has C3 E2E, but a *new*
   write-origin path (e.g. different ACP method shape) could miss the bar the
   same way shell did if no scenario drives it.

2. **Permission-rule attribution on process / write**  
   `autoApprovedByRule` accept/reject is unit-tested; there is no E2E that
   asserts “Always allow” attribution chip appears after a rule match, or that
   a bad attribution shape fails closed without dropping the whole list.

3. **Git changes gateway closed-key surface**  
   `tauriGit*` / git workbench DTOs are partially covered by git E2E, but
   incremental host fields on status/diff/stage payloads can still throw the
   entire list normalizer. No mutation probe requires a green E2E stage to go
   red when a new host key is added without renderer admission.

4. **Conversation / run-event projection**  
   Run lifecycle events that drive composer cancel visibility, activity rows,
   and timeline tool chips are integration-tested in places, but a host event
   field rename that the renderer drops can still leave “Stop never appears”
   or “activity never updates” while other E2E paths stay green.

5. **Shell output / receipt UI specifics**  
   This ticket covers approve bar + exact command + reject. It does **not**
   assert shell-specific terminal pane labels, `data-command-kind="shell"` on
   activity/terminal after open, or that `argv === [command]` is never
   re-JSON-stringified on the activity row after success.

6. **Shell timeout / kill / cancel tree via `terminal/*`**  
   Structured process has cancel-tree and timeout scenarios. Shell has no
   long-running / timeout E2E over `terminal/create` + kill while pending.

7. **Multi-proposal / race for shell**  
   Structured process asserts single approval slot. Shell does not assert two
   concurrent `terminal/create` proposals or conversation-switch while a shell
   bar is pending.

8. **Settings / credentials / runtime-degrade banners**  
   Host config errors that disable conversation while leaving the shell up can
   still “not appear” without a dedicated E2E if the degrade path is not
   driven.

9. **Any future host DTO with a closed renderer allowlist**  
   The third silent failure was exactly “host widened, renderer closed key
   list not widened.” Until every host→renderer command has an **accept**
   direction test (payload admitted + UI surface appears), not only a reject
   suite, the next silent omission will look like a missing bar/chip/pane.

Recommended product rule (already in `AGENTS.md`): widen Rust DTO and
renderer allowlist in the **same** change, and pin **accept** direction with
a test that fails when the validator rejects everything.

## What the controller should see when E2E runs

Controller command (after both parallel tickets land), from repo root:

```text
pnpm --filter @pillow/desktop exec node --test --test-concurrency=1 tests/e2e/c4-governed-process.spec.ts
```

(or the existing `scripts/wave-c4-governed-process-acceptance.mjs` entry the
spec already wraps).

### Expected terminal record

A final line:

```text
KEEPER_WAVE_C4_GOVERNED_PROCESS_ACCEPTANCE {"schemaVersion":1,"status":"passed",...,"scenarios":[...]}
```

`scenarios` **must include**:

- `shell-approve`
- `shell-reject`

(plus prior required scenarios).

### Expected UI / evidence behavior

1. **shell-approve**  
   - Approval bar mounts (`command-approval-bar`).  
   - Kind shows Shell; `data-command-kind="shell"`.  
   - `command-approval-argv` / `data-full-command` equals  
     `echo shell-e2e "double" 'single'` with no JSON quotes around the whole
     string and both quote styles preserved.  
   - After Approve: activity **已成功**, exit 0.  
   - Timeline assistant contains `acp-governed-shell-ok`.  
   - Screenshot: `.superpowers/c4-process-acceptance/shell-approve-bar-1440x960.png`

2. **shell-reject**  
   - Same bar + exact command.  
   - After Reject: activity **已拒绝**, not left running.  
   - Screenshot: `shell-reject-bar-1440x960.png`

3. **If T3 regression reappears** (empty `ruleSubject` rejected again):  
   - Both shell scenarios fail at `waitForShellCommandApproval` with  
     `command-approval-bar wait failed; everVisible=false`  
   - Structured `reject` / `approve-success` may still pass — that split is
     the point of the coverage.

### Not expected from this worker

- A green E2E run result in this report (E2E was forbidden; controller owns it).
- Changes under the five concurrent crates.

---

`SHELL-E2E-COVERAGE-COMPLETE`
