# Pillow — External Code Audit Brief

You are auditing a codebase you did not write. Report problems; do not fix them.
Findings may be written in Chinese or English.

---

## 1. What this is

Pillow is a local-first desktop AI coding IDE: a Tauri + WebView2 + React shell
over a Rust workspace. An AI agent (currently Grok, over the Agent Client
Protocol) runs inside it and can read files, write files, run processes, and read
Git state — but **only through a governance layer**.

The governance primitive is:

```
proposal → one-shot human approval → host execution → durable receipt
         → project isolation → invalidation on restart
```

The product's entire value proposition rests on that layer being correct. A
convenience bug is a nuisance; a governance bug means the agent did something
nobody authorised, or the record of what it did is wrong. **Weight your attention
accordingly.**

Rough shape:

- `crates/` — Rust workspace (13 crates): policy state machines, SQLite
  storage, runtime host, tool executors, the Grok ACP adapter, a governance port,
  runtime slots (credential storage + per-slot agent config)
- `apps/desktop/src-tauri/` — Tauri host (Rust): commands, composition root
- `apps/desktop/src/` — React renderer (TypeScript)
- `scripts/` — gate entry (`run-worker-gate.mjs`) and acceptance harnesses
- `docs/` — design records, acceptance evidence, execution reports

---

## 1b. Read this before anything else: the boundary

The governance layer above only means something if the agent **cannot act
outside it**. That containment is not a sandbox. It is a negotiated contract
with a third-party CLI (Grok Build) over ACP, and it consists of exactly three
things:

```jsonc
// crates/adapter-grok-acp/src/rpc/methods.rs
tools:              ["ask_user_question", "write", "run_terminal_cmd"]  // allowlist
disallowedTools:    ["search_tool", "use_tool"]                        // MCP meta-tools only
clientCapabilities: { fs: { readTextFile: true, writeTextFile: true }, terminal: true }
```

Native `write` reaches us as ACP `fs/write_text_file`; native `run_terminal_cmd`
reaches us as `terminal/*`. Both then enter the same proposal chain. Nothing
else is supposed to be reachable.

**We already got this catastrophically wrong once, and the code looked correct
the whole time.** Three failures stacked:

1. the profile omitted upstream's required `name` / `description`, so parsing
   failed, was logged on one line, and **silently fell back to the full default
   toolset**;
2. `tools: []` means *inherit everything* upstream, not "none";
3. several disallowed tool names were simply wrong.

For months the governance chain was exercised only by the fixture agent. A real
agent could read and write freely without ever raising a proposal. It was found
by a live probe, not by review — reading configuration code produced three
confident, wrong conclusions in a row.

So: **treat any claim about containment as unproven until it is observed at
runtime.** `scripts/probe-mcp-tool-exposure.mjs` (opt-in, never in the gate) is
how we check it now. Its `acp-fs-localfs-trap` and `shell-no-handler` variants
are deliberate sentinels: they assert *provider* behaviour, not ours, and are
expected to keep reproducing.

Three "half-open" shapes are known, all silent, all identical in form — grant a
hand without opening the line to the host and the agent acts locally with no
error:

| Trap | Condition |
| --- | --- |
| LocalFs | `write` allowlisted while `fs.writeTextFile` is false |
| LocalTerminal | shell tool allowlisted while `terminal` is false |
| ambient MCP | the two MCP meta-tools not denied |

Note the asymmetry, which is easy to misread: the filesystem backend is chosen
by `read && write` (**both**), the terminal backend by a **single** bool.

Two coupling tests pin the first two. **Check that they actually bite**, and
that nothing else can reintroduce a hand without its line.

Also worth challenging: `mcpServers: []` does **not** mean "no MCP" — sessions
merge servers from user config, plugins and an account gateway, and denying the
two meta-tools is the only lever a client has. If you can find a path that
reaches an MCP tool without those meta-tools, that is a serious finding.

---

## 2. Invariants that must hold

These are claimed by the design. **Your job includes checking whether the code
actually enforces them**, not assuming it does.

1. **Never fabricate a terminal state.** If a write/process cannot be durably
   resolved, the caller gets an opaque error — never a synthesised "success" or
   "cancelled".
2. **No silent fallback to the fake runtime.** If the real agent runtime is
   missing or misconfigured, the app fails closed or degrades visibly. It must
   never quietly substitute the deterministic fake.
3. **Approval is one-shot and scoped.** A grant authorises exactly one operation.
   Reuse, replay, or cross-scope use must be rejected.
4. **Secrets stay host-side.** Sealed file contents and raw approval grants live
   only in host memory. SQLite stores digests and metadata. The renderer never
   receives grant material.
5. **Project isolation.** Operations bound to project A must not observe or
   affect project B — including after restart.
6. **Restart invalidates live authority.** In-memory grants do not survive a
   restart; interrupted runs recover to an explicit `Interrupted` state.
7. **Ordering:** `validate → persist → commit → emit`. An event must never be
   emitted for something that was not durably committed first.
8. **Lifecycle callbacks fire after durable commit, never under a lock, never on
   replay.** Cancellation is level-triggered — a single snapshot at entry is
   considered fail-open.
9. **Paths from the agent are project-relative and portable.** No absolute paths,
   no traversal, no Windows device names.
10. **Native tool restriction is an allowlist, never an enumeration.** A denylist
    lost once already: the agent left through `workflow` / `monitor`, which no
    list had thought to name. The one denylist that remains covers exactly two
    MCP meta-tools and is a door, not a room inventory.
11. **Rule allow is conjunctive.** A shell command is split conservatively and
    *every* segment must match an allow rule; any deny wins; an unsplittable
    command asks. `Bash(cargo*)` must never authorise `cargo test; rm -rf x`.
    The splitter is deliberately incomplete — it only has to be right about *when
    it does not know*.
12. **A permission mode turns asking off, not governance off.** Even in bypass,
    the proposal, execution and durable receipt still happen, deny rules still
    win, and the receipt records which mode allowed it. A corrupt stored mode
    falls back to `default`, never to `bypass`. A project directory can never
    select a mode.
13. **Approval authority is never delegated to the provider.** Pillow must not
    send `yolomode` / `automode` / `always-approve` in session params. If the
    agent decides, our receipts are fiction.
14. **Slot secrets are user-encrypted at rest and never leave the host.** Keys
    live in DPAPI-protected files, are redacted from `Debug`, zeroed on drop, and
    absent from every DTO. Non-Windows fails closed rather than storing plaintext.
15. **Structured Git operations construct their own argv.** `git` is an execution
    engine (`-c core.pager=…`, `--upload-pack`, aliases). No caller-supplied flag
    or config may reach it, or the whole command-approval story is void.

---

## 3. Where to look first

Ordered by where a defect would hurt most.

1. **`crates/adapter-grok-acp/src/rpc/methods.rs` + `provider_writes.rs` +
   `provider_terminals.rs`** — the boundary and the two reverse-RPC entries
   (§1b). A defect here is not a bug, it is the absence of the product.
2. **`crates/runtime-host/src/`** — the coordinators. `governed_write.rs`,
   `governed_read.rs`, `host/process.rs`, `governed_tool_service.rs`.
   These hold live authority in memory with `Mutex` + `Condvar`, and deliberately
   perform some work *off-lock*. Two real races have already been found and fixed
   here; assume more exist.
3. **`crates/kernel-policy/`** — the write/exec state machines, plus the
   command segmenter and conjunctive rule evaluation (`permission_rules.rs`). Check that no
   illegal transition is reachable, and that terminal states are truly terminal.
4. **`crates/storage-sqlite/`** — persistence and migrations (`migrations/*.sql`).
   Check transaction boundaries, whether "commit" really committed, and whether
   any query can be influenced by agent-supplied strings.
5. **`crates/tool-executor/` and `crates/process-executor/`** — actual file
   writes and child processes. Windows job objects are used for process-tree
   cleanup. Check for orphaned children, unbounded output, and whether the
   working directory / environment can be escaped.
6. **`apps/desktop/src-tauri/src/`** — every `#[tauri::command]` is an entry point
   the renderer can call. Check argument validation and scope enforcement at that
   boundary specifically.
7. **`crates/runtime-slots/`** — credential storage and per-slot agent config.
   Check that a key cannot reach a DTO, a log, an error, or a config file.
8. **`crates/git-executor/`** — structured mutation (invariant 15).
9. **The rest of `crates/adapter-grok-acp/`** — the wire protocol is **hand-written**, not
   generated from a schema, so there is no compile-time signal if it drifts.
   Check parsing robustness against malformed or hostile agent output.

---

## 4. Where I am least confident

I am the controller who accepted most of this work. These are the places my own
review is weakest, so external eyes are worth the most here. Please do not treat
this list as exhaustive — blind spots are by definition not on it.

- **Concurrency in the coordinators.** Every race found so far was found by
  deliberate barrier-based experiments, not by reading. Reading alone has a poor
  track record here.
- **The decoupling refactor.** Six sequential steps landed in roughly one day,
  moving governance execution behind a port trait and cutting the adapter's
  dependency on the host. Each step was verified individually. **The composition
  of all six was never audited as a whole.**
- **Error mapping.** Errors are converted across several layers. Check whether
  any conversion loses a distinction that mattered — particularly whether
  anything that should be fail-closed can arrive as a benign variant.
- **Test quality, not just presence.** Many tests assert on strings or shapes.
  Where a test claims to prove an invariant, check that it would actually fail if
  the invariant were violated. Vacuous tests are worse than missing ones, because
  they buy false confidence.
- **The renderer/host boundary.** Redaction is asserted in tests. Check whether
  anything sensitive can still reach the renderer by a path those tests miss.
- **Windows-specific behaviour.** Path handling, device names, job objects,
  file locking. This code has only ever run on Windows.
- **The boundary contract with the provider.** It is upstream behaviour, not our
  code, and it can change under us without notice. Our evidence is a live probe
  run at a point in time, nothing stronger.
- **Everything added on 2026-08-11.** Governed write over ACP, governed shell,
  permission rules, permission modes, slots and credential storage, and Git
  mutation all landed inside one day. Each passed its own gate; **the composition
  was never reviewed as a whole.** The same warning as the decoupling refactor
  above, with less settling time.

---

## 5. Known issues — do not re-report

- An intermittent E2E failure under heavy machine load. Contributing causes have
  been found and fixed (a harness that crashed on control characters in its own
  captured output; a process-wide probe counter shared by tests that did not take
  the lock). A residue may remain under contention. **A proven root cause is very
  valuable** — the symptom alone is not a new finding.
- Known and accepted limits, do not re-report as discoveries: the interpreter
  confirmation table is finite, so a broad rule over an unrecognised wrapper is
  the most plausible wrong-allow; a rule like `node*` legitimately authorises
  `node any-script.js`; the event payload carries one representative rule while
  the durable receipt holds the full per-segment set; restart-recovered rows
  cannot reconstruct segments and are therefore not decision-capable.
- **Not claimed:** this is not an OS sandbox. Containment is over a verified
  provider tool path (§1b). Anything that bypasses the Grok tool layer entirely
  is out of scope for the current design — but if you find such a path *through*
  it, that is exactly what we want to know.
- Governance decoupling is incomplete by design. Steps 7–11 of the plan (state
  migration, single bind, run lease, removal of the free-function compatibility
  layer) are outstanding and known.
- Documentation under `docs/` contains machine-specific operational notes. A
  cleanup pass is in progress separately.

---

## 6. What NOT to do

- **Do not modify any code.** Report only. Other work is in flight; edits would
  collide.
- **Do not run the desktop E2E suite** (`test:e2e`, `scripts/wave-*-acceptance.mjs`).
  It drives a real desktop app, takes ~25 minutes, and is run exclusively by the
  controller.
- Building and running unit tests is fine if useful.
- **Do not weaken any assertion** to make something pass.
- Do not read or print credential/auth files.

---

## 7. How to report

For each finding:

| Field | Content |
| --- | --- |
| **Location** | `path/to/file.rs:LINE` |
| **Severity** | governance / correctness / robustness / quality |
| **Claim** | one sentence: what is wrong |
| **Why it matters** | concrete failure: what input or interleaving produces what wrong outcome |
| **Confidence** | verified by reading the code path / plausible but unconfirmed |

**Ranking matters more than volume.** A short list of confirmed governance
defects is far more useful than a long list of style observations. If you find
nothing serious in an area, say so explicitly — "I checked X and found nothing"
is real information.

Please state clearly which areas you actually covered and which you did not.
An audit that quietly skips an area is worse than one that admits the gap.
