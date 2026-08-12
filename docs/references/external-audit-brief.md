# Pillow — External Code Audit Brief

You are auditing a codebase you did not write. **Report problems; do not fix
them.** Findings may be written in Chinese or English.

Repository snapshot: `ce017a5` · 182 commits · Windows 11 · Rust workspace
(17 crates) + Tauri/WebView2 + React renderer.

This brief is self-contained. You should not need to ask us anything to start.

---

## 0. What we are asking for

**Find what we did not.** We have 641 renderer tests, a workspace gate, and a
10-scenario E2E suite, all green. That is exactly why we need you: everything
we thought to check, we checked. The interesting failures are in what we did
not think to check.

Two requests that matter more than coverage:

1. **Attack the governance boundary, not the test suite.** If you can make the
   agent write a file, run a process, or reach the network without a human
   decision reaching the host first, that is the finding we want.
2. **Distrust this document.** Every claim below names a file. If a claim and
   the code disagree, **the code is the truth and the disagreement is a
   finding.**

§7 lists what we already know is missing — 13 items, several of them load-
bearing. Reporting those back is not useful; **finding something not on that
list is.**

---

## 1. The one-sentence architecture

Pillow is a local-first AI IDE. An agent runs as a child process; every side
effect it wants must become a **proposal** a human approves **once**, which the
**host** then executes, leaving a **durable receipt**.

```
proposal → one-shot human approval → host execution → durable receipt
         → project isolation → invalidation on restart
```

Ordering invariant: `validate → persist → commit → emit`. Nothing reaches the
UI before it is durable.

**The renderer is a view.** It never executes, never holds authority, and in
this snapshot makes no outbound network requests at all.

A convenience bug is a nuisance. A governance bug means the agent did something
nobody authorised, or the record of what it did is wrong. **Weight accordingly.**

---

## 2. The boundary — read this before anything else

The agent is `grok` (Grok Build) speaking **ACP** (Agent Client Protocol) over
NDJSON JSON-RPC on stdio. We do not control its source. We control what we
declare at session start.

```jsonc
// crates/adapter-grok-acp/src/rpc/methods.rs
tools:              ["ask_user_question", "write", "run_terminal_cmd"]
disallowedTools:    ["search_tool", "use_tool"]   // the two MCP meta-tools
clientCapabilities: { fs: { readTextFile: true, writeTextFile: true }, terminal: true }
mcpServers:         []
```

| Surface | Mechanism |
| --- | --- |
| **Write** | native `write` → ACP `fs/write_text_file` → host governs |
| **Process** | native `run_terminal_cmd` → ACP `terminal/*` → same chain |
| **MCP** | **cut** — ambient MCP cannot be disabled from our side, so denying the two meta-tools is the only lever |

**The allowlist governs native tools and is fail-closed. The denylist closes
one door; it is not an enumeration of rooms.** A test fails if a third name
appears in the denylist.

### 2a. Three half-open traps (all silent)

This is the failure shape that has actually bitten us three times: **a hand is
granted but the line back to the host is not opened.** The agent then does the
thing itself, the host never sees it, and nothing errors.

| Trap | Condition |
| --- | --- |
| LocalFs | allowlist contains `write` ∧ `writeTextFile = false` |
| LocalTerminal | allowlist contains a shell tool ∧ `terminal = false` |
| Ambient MCP | meta-tools not denied |

🔴 **The predicate is asymmetric, which is easy to get wrong:** fs requires
**both** `readTextFile` ∧ `writeTextFile`; terminal is a **single** bool. Two
coupling tests in `methods.rs` pin the first two.

🔴 **A high-value attack: find a fourth trap of this shape.**

### 2b. History you should know

For months we believed this boundary held. It did not. Three independent
failures stacked:

1. `_meta.agentProfile` was missing required `name` / `description`, so upstream
   failed to parse it, logged one line, and **silently fell back to the full
   tool set**
2. `tools: []` means **inherit all** upstream, not "none"
3. the disabled names were wrong (real short ids are `write`,
   `run_terminal_cmd`)

**Every layer reported success.** It was found by a live probe — not by review,
not by tests. That is why §6 exists.

---

## 3. Permission model

**Modes:** `default` / `accept_edits` / `bypass`.

🔴 **`bypass` disables the asking, not the governing.** Proposal, host
execution and durable receipt all still happen; deny rules still win; an unknown
enum value fails closed to `default`; the project directory itself cannot be
mode-selected.

🔴 **Approval authority is never delegated to the provider.** `methods.rs`
carries a test forbidding `yolomode` / `automode` / `always-approve` in session
params.

**Shell rules:** commands are conservatively segmented; an allow rule must match
**every** segment (conjunctive); any denied segment rejects; a command that
cannot be segmented reliably fails closed to asking. `Bash(cargo*)` does **not**
authorise `cargo test; rm -rf x`. Rules save **per segment**; an unsplittable
command offers no rule entry point rather than offering the whole string.

File: `crates/kernel-policy/src/permission_rules.rs`.

---

## 4. Slots, roles, execution paths

**One slot record. Auth and role are fields of it, not separate kinds.**

```rust
// crates/storage-sqlite/src/runtime_slot.rs
SlotAuth = ApiKey | GrokLogin | LocalCli | Oauth
SlotRole = Controller | Librarian | Custom(String)
```

🔴 **Governance is derived from auth, never stored beside it.** As two
independent fields they could disagree; derived, the disagreement is
unrepresentable. `governance_from_auth()` has its three mappings pinned.

**Controller** — the slot you converse with — is at most one per project and
**must be a governed auth**. A `LocalCli` in Pillow's own conversation would be
exactly the §2a trap, so it is refused.

**Execution paths** (`crates/runtime-slots/src/execution_path.rs`):

| Path | Meaning |
| --- | --- |
| `GrokShell` | Grok ACP child; private config `base_url` + bearer `env_key` |
| `NativeAdapter` | **Pillow is the harness**; model HTTP via `pillow-provider-adapters` |
| `NotRunnable` | status-only registration (LocalCli) |

The path is **derived** from auth + provider profile, not chosen by the user. A
provider whose request shape `GrokShell` cannot express is **refused at
startup** rather than stuffed into it — otherwise it fails later looking like a
bad key.

🔴 **Audit target:** `NativeAdapter` is newer and far less exercised than
`GrokShell`. We assert Pillow-as-harness preserves the §2 properties on that
path. **Verify that claim rather than accepting it.**

**Secrets:** API keys and OAuth tokens are stored under **DPAPI**
(`CryptProtectData` + `CRYPTPROTECT_UI_FORBIDDEN`), Debug-redacted, zeroed on
drop, never placed on a DTO. Non-Windows fails closed. The **refresh token
never reaches the agent process** — only a live access token is injected.

---

## 5. Role dispatch — deliberately not a second authority

"The controller calls another role" is mechanically **running a process**, and
processes are already governed. So dispatch builds an ordinary governed process
proposal and travels the same approval chain, rules and receipts. **There is no
second approval UI and no separate executor.**

- `crates/role-dispatch` — pure domain; builds a structured `ProposedCommand`
- `crates/runtime-host/src/dispatch.rs` — hands it to the existing chain

Pinned both directions: a rejected dispatch reports **no** start; an approved
one reports a start — so the zero-start assertion cannot pass vacuously.
`argv` is a structured array never joined into a shell string; task text can
never occupy the program or cwd position; self-dispatch is refused; a `LocalCli`
target cannot have its governance label upgraded in transit.

🔴 **Audit target:** get task text into `program`/`cwd`, or make a dispatch
execute without an approval.

---

## 6. Live probes are the only trustworthy boundary evidence

```bash
PILLOW_MCP_PROBE=1 node scripts/probe-mcp-tool-exposure.mjs
# narrow with PILLOW_MCP_PROBE_ONLY=<id>;  "not run" never counts as pass
```

| id | Expectation |
| --- | --- |
| `shipped-profile-write` | write **reaches the client** and never touches disk |
| `shipped-profile-mcp` | **zero** tool calls; agent answers `NO_MCP` |
| `acp-fs-localfs-trap` | **must still write to disk** — known negative, a sentinel |
| `shell-no-handler` | `terminal/create` reaches the client, no local process |

🔴 **The criterion is conjunctive: "reached the client" AND "not executed by the
agent".** Checking only "not executed" reads a *broken feature* as *working
governance*.

🔴 The two sentinels verify **the provider's behaviour, not our code**. If
upstream changes, they can go quiet without our code changing.

---

## 7. What we know we have not done

These are known. Finding something NOT on this list is worth far more than
confirming anything on it.

1. 🔴 **Nobody has run a real task.** Slot equality, roles, dispatch, OAuth,
   provider adapters, CLI detection and the librarian audit all have unit and
   E2E evidence and **zero live evidence**. A green gate is not a working
   product.
2. **5 provider login profiles carry `ClientIdSource::Unknown`.** We refused to
   invent client ids — embedding another product's would mean impersonating its
   official client. Those need user-supplied values and are incomplete by
   design.
3. **Consumer-subscription OAuth is described but not reachable end to end.**
   A `chatgpt-codex` adapter **does** ship
   (`crates/provider-adapters/src/adapters.rs`) with the vendor endpoint,
   rewritten authorization header and restricted model list. What does not
   exist is a path from it to a working conversation — see 4. Treat any claim
   elsewhere in this repo that "no such adapter ships" as stale; this line is
   the correct one.
4. 🔴 **`NativeAdapter` never activates a conversation.** Starting such a slot
   reaches `status = "native_adapter_prepared"` and stops
   (`apps/desktop/src-tauri/src/runtime_slot_commands.rs`). It deliberately
   does **not** call `activate_grok`. So the adapter path is: request shaping
   proven by unit tests, credential injection wired, **and no runtime behind
   it.** Every live conversation today is `GrokShell`.
5. 🔴 **`NativeAdapter`'s "Full" governance label is by construction, not by
   probe.** For `GrokShell` we have live probes (§6) showing the agent cannot
   act unilaterally. For `NativeAdapter` we have an argument — Pillow is the
   harness, therefore it governs — and no equivalent probe. **This is the
   weakest load-bearing claim in the system.**
6. **OAuth slots cannot be dispatch targets.** `role-dispatch` refuses them
   with `role_dispatch_unsupported_auth`.
7. **The wire label `governed_grok` now covers OAuth and `NativeAdapter`
   slots too.** It is a compatibility label kept for the renderer's closed key
   allowlist, not a statement that the slot runs the Grok shell. It is
   misleading and we have not renamed it.
8. **The dispatch ledger is in-memory.** Its durability across restart is not
   established.
9. **Interpreter confirmation is a finite table.** An unrecognised wrapper plus
   an over-broad rule is our most likely mis-allow path. A rule like `node*`
   legitimately authorises `node any-script.js`.
10. **Event payloads carry only a representative rule**; the durable receipt is
   authoritative. **Audit receipts, not events.**
11. **Restart-recovered rows cannot recover their segments**, so they are never
   auto-decided.
12. **This is not an OS sandbox.** We govern the provider's verified tool paths.
   An agent that finds an unverified path is outside this model — and that is
   exactly the finding we want.
13. **`search_replace` is deliberately unexposed.**

---

## 8. Where the seams are — where we have broken it before

**The renderer validates host DTOs against a closed key allowlist.** Three
separate times the host added a field, the renderer rejected the whole payload,
and a UI surface vanished silently **with the gate green**.

If you want a bug class with history here: **any host DTO change not matched in
the same change by `apps/desktop/src/features/**/…Gateway.ts`.** Allowlist
lengths are pinned so an emptied validator cannot pass silently.

Related: **a test that only asserts rejection is false green.** We try to pin
both directions everywhere; where we did not, that is a finding.

---

## 9. How to run it

```bash
node scripts/run-worker-gate.mjs               # fmt, clippy -D warnings, workspace tests, secret scan, ACP drift
corepack pnpm typecheck
corepack pnpm --filter @pillow/desktop test    # 641 tests
corepack pnpm desktop:test:e2e                 # 10 scenarios, ~25 min, real WebView2
```

The gate is the acceptance authority. It has produced false reds twice — a
process-wide static polluted by tests not holding a shared lock, and a 5s
renderer timeout that load could exceed. Both fixed; noted so a red you see is
judged on evidence rather than assumed environmental.

---

## 10. Our own failure log — for calibration

Included so you can see the shape of mistakes this codebase attracts.

1. **Concluding from a suppressed signal.** `cmd | tail` reported a red suite as
   green. A sandbox-denied probe was read as "signed out" and reported as fact.
   A missing commit marker was read as a mismatch, failing 22 of 22 records
   while its pass state was unreachable. **Absence of evidence became negative
   evidence, repeatedly.**
2. **Exit code is not outcome.** Icon generation exited 0 with the artwork
   broken; a build exited 0 without embedding the new icon.
3. **A controlled experiment must differ by one variable.** We compared two
   agent profiles; neither had parsed.
4. **`cargo check` does not compile `#[cfg(test)]`** — and neither does clippy,
   so an import it calls unused may be exactly what the tests need.
5. **Filtering keys is not vetting values.** A redactor passed a `slot=` token
   through intact, carrying a host path into a renderer-bound DTO.
6. **A worker's own gate result is not evidence.** Sandboxes hide failures;
   scoped runs hide crates.

---

## 11. What a maximally useful report looks like

- **Ranked by exploitability**, not by count.
- Each finding: the file, the precondition, and **the concrete sequence** that
  produces the effect.
- Explicitly separate **"the code permits this"** from **"we observed this"**.
  We over-trusted the former for months — see §2b.
- If you conclude a property **holds**, name the mechanism that enforces it.
  "Looks fine" and "is enforced by X" are different findings to us.

**If you only have time for one thing: §2 and §2a.**
