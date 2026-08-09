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

- `crates/` — Rust workspace (~12 crates): policy state machines, SQLite
  storage, runtime host, tool executors, the Grok ACP adapter, a governance port
- `apps/desktop/src-tauri/` — Tauri host (Rust): commands, composition root
- `apps/desktop/src/` — React renderer (TypeScript)
- `scripts/` — gate entry (`run-worker-gate.mjs`) and acceptance harnesses
- `docs/` — design records, acceptance evidence, execution reports

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

---

## 3. Where to look first

Ordered by where a defect would hurt most.

1. **`crates/runtime-host/src/`** — the coordinators. `governed_write.rs`,
   `governed_read.rs`, `host/process.rs`, `governed_tool_service.rs`.
   These hold live authority in memory with `Mutex` + `Condvar`, and deliberately
   perform some work *off-lock*. Two real races have already been found and fixed
   here; assume more exist.
2. **`crates/kernel-policy/`** — the write/exec state machines. Check that no
   illegal transition is reachable, and that terminal states are truly terminal.
3. **`crates/storage-sqlite/`** — persistence and migrations (`migrations/*.sql`).
   Check transaction boundaries, whether "commit" really committed, and whether
   any query can be influenced by agent-supplied strings.
4. **`crates/tool-executor/` and `crates/process-executor/`** — actual file
   writes and child processes. Windows job objects are used for process-tree
   cleanup. Check for orphaned children, unbounded output, and whether the
   working directory / environment can be escaped.
5. **`apps/desktop/src-tauri/src/`** — every `#[tauri::command]` is an entry point
   the renderer can call. Check argument validation and scope enforcement at that
   boundary specifically.
6. **`crates/adapter-grok-acp/`** — the wire protocol is **hand-written**, not
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

---

## 5. Known issues — do not re-report

- An intermittent E2E failure under heavy machine load: a restart degrades with
  `runtime_profile_invalid` on a directory the harness never deletes. Root cause
  unproven. Path probing was recently changed to distinguish "definitely absent"
  from "cannot determine". **If you can find the actual cause, that is very
  valuable** — but do not report the symptom alone as a new finding.
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
