# Wave C4 Research — Governed Terminal and Process Execution

Date: 2026-08-02
Researcher: grok1 (Grok CLI slot; controller-role Keeper boot)
Workspace: `this repository`
Pillow HEAD inspected: `7b7f70f7360131cd1cfd984a8b19bdfb5725c21b`
Grok App pin: `cbbbdf17bb8db1588b6707f3f42f6b00397e27f9` (MIT)
zbbody pin: `00c293414333b6fcc4e4598389fa3952c8d0b09b` (Apache-2.0)
Scope: smallest safe architecture for C4 first checkpoint (approve-once one-shot
command execution) and later interactive Windows terminal; **no product
implementation authorized**

Keeper Bridge: session `<keeper-session>`, project `pillow`,
task `C4 governed terminal and process research`, agentName `grok1`.

---

## 1. Executive recommendation

**Ship C4.0 as conversation-owned, project-scoped, approve-once one-shot
process execution (`process.run@1`), not as an interactive terminal and not as
free-form shell text.**

| Choice | Decision |
| --- | --- |
| First checkpoint | One-shot argv execution under Job Object + bounded capture |
| Approval | Mirror C3: host-sealed proposal, opaque one-time grant, `approve_once` / `reject` only |
| Command representation | Structured `{ program, argv[], cwd }` — no shell metacharacters in C4.0 |
| Interactive terminal | Explicit later checkpoint (`process.pty@1` / ConPTY); separate interfaces |
| Shell-string execution | Non-goal for C4.0; if ever added, separate tool + stricter policy |
| Persistence | SQLite metadata + digests + exit/timeout flags; bounded transcript optional and capped |
| Windows process tree | Reuse ACP pattern: CREATE_SUSPENDED → Job Object (kill-on-close) → resume |
| Environment | Allow-listed OS base only; secret-name exclusion; no wholesale parent inherit |
| Provider authority | Provider proposes only; never approves; adapter continues to deny native tools |

This is the lowest-complexity model that reuses C3 ownership, approval, ledger,
and process-safety machinery without opening ConPTY, network side effects, or
session-scoped auto-approve.

**Do not** ship in C4.0: interactive PTY, `allow_for_session` / YOLO shell,
`cmd.exe /c` / `powershell -Command` as the primary wire form, unrestricted
`PATH` search of provider-chosen basenames, durable full transcripts, Git
mutations via shell, network-download auto-allow, multi-command pipelines as one
approval object, or renderer-supplied cwd/env/argv.

---

## 2. Evidence base

### 2.1 Pillow C3 design, acceptance, process safety

| Artifact | Path | Relevance |
| --- | --- | --- |
| C3 design (accepted) | `docs/superpowers/specs/2026-08-01-c3-governed-write-design.md` | Proposal → human decision → host apply; terminal/process **explicitly excluded** (L76) |
| C3 checkpoint | `docs/verification/2026-08-02-wave-c3-checkpoint.md` | Stage 1 chain ends at governed write; C4 planning-only |
| C3 acceptance | `docs/verification/2026-08-02-wave-c3-acceptance.md` | Process safety 41/41; restart interrupts pending authority |
| C3 security research | `docs/research/2026-08-01-c3-governed-write-security-research.md` | Layer map; fail-closed ownership; secrets out of SQLite/logs |
| Stage 1 status | `docs/roadmap/2026-07-31-stage1-overall-status.md` | C4 terminal/Git next planning boundary |
| Desktop process identity | `scripts/lib/desktop-process.mjs` | Immutable PID+creationTicks+path; incomplete identity fail-closed |
| Process safety tests | `scripts/lib/desktop-process-safety.test.mjs` | 41 tests; VCTIP installation-boundary; descendant age |
| ACP Job Object | `crates/adapter-grok-acp/src/process/job_windows.rs` | `JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE` + `TerminateJobObject` |
| ACP spawn/kill | `crates/adapter-grok-acp/src/process/child.rs` | CREATE_SUSPENDED + assign job + prove all tracked PIDs gone |
| ACP env allowlist | `crates/adapter-grok-acp/src/process/env.rs` | No wholesale inherit; sensitive name filter |
| Write policy SM | `crates/kernel-policy/src/write.rs` | Terminal states never reopen; 600s TTL; approve_once only |
| Governed write bridge | `crates/runtime-host/src/governed_write.rs` | Sealed content + raw grant in host memory only |
| Write executor | `crates/tool-executor/src/write.rs` | Path deny list; reparse refuse; digests; CAS apply |
| Project capability | `crates/tool-executor/src/lib.rs` | Canonical root; resolve_target containment |
| Protocol tools | `crates/agent-protocol/src/tool/types.rs`, `manifest.rs` | Fixed catalog: list/read/search/write only today |
| Write ledger | `crates/storage-sqlite/migrations/0005_write_proposals.sql` | Metadata/digests only; no raw content/grants |
| Read ledger | `crates/storage-sqlite/migrations/0003_tool_calls.sql` | Still CHECK tool_id ∈ three reads (writes use dedicated tables) |
| Tauri write IPC | `apps/desktop/src-tauri/src/write_commands.rs` | list/decide/diff; narrow DTO; path capability required |
| Path capabilities | `apps/desktop/src-tauri/src/path_scope.rs` | Window/conversation-bound canonical roots; reparse/UNC reject |

C3 authority chain (to mirror for commands):

```text
Provider child
  -> proposes call id + tool args only
Adapter
  -> validates fixed method/schema; no project root or approval
Runtime host
  -> injects project/conversation/run; seals proposal; mints grant
SQLite
  -> immutable metadata, digests, transitions, decision digest
Conversation UI
  -> host DTO + opaque grant + decision only
Kernel policy
  -> ownership, expiry, fingerprint, single use
Executor
  -> revalidate and apply under ProjectCapability
SQLite + run events
  -> terminal receipt before UI/provider completion
```

### 2.2 Grok App (conceptual process/session reference)

Pinned: `pinned Grok App reference (out of tree)` @ `cbbbdf17…`, MIT.

| Topic | Path | Observation for Pillow |
| --- | --- | --- |
| Agent spawn | `src-tauri/src/acp_client.rs` | Spawns CLI with cwd; `kill_on_drop`; turn ceilings; **not** a product terminal |
| Process helpers | `src-tauri/src/process_util.rs` | `CREATE_NO_WINDOW`; PATH enrichment for GUI-spawned agents; `looks_runnable` |
| Pool/idle | `src-tauri/src/process_limits.rs` | Concurrent agent caps + idle recycle — product pool policy, not tool exec |
| Permission scope | `src-tauri/src/permission.rs` L91–122, L218–294 | `scope_key` for shell uses **executable basename only** (`shell:npm`); download shell can auto-allow |
| Session FSM | `src-tauri/src/session_fsm.rs` (C3 research) | `AwaitingPermission` first-class busy state — UI pattern only |
| win_shell | `src-tauri/src/win_shell.rs` | Taskbar/Show Desktop HWND integration — **unrelated** to command execution |

**Adopt as patterns only:** host-owned permission bar pacing; conversation-dominant UI; create-no-window for non-interactive children; keep background sessions alive without cancelling work.

**Reject for C4.0:** session-scoped shell allow by basename; YOLO/`bypassPermissions`; download-command default-allow; treating shell text as the approval identity; assuming killing the direct child is enough (Pillow already exceeds this with Job Objects for ACP).

Grok App does **not** implement an in-app ConPTY workbench. Interactive terminal must not be reverse-engineered from its agent CLI spawn path.

### 2.3 zbbody (conceptual only; license verified)

Root: `zbbody reference clone (out of tree)` @ `00c29341…`.

- **License:** Apache-2.0 (`LICENSE` verified).
- Prior Pillow decision (`docs/research/2026-07-30-zbbody-reference-analysis.md`): read-only interaction/lifecycle reference; no wholesale source copy; reject renderer-defined executable/argv/env/cwd.
- Local search found no dedicated ConPTY/product terminal host module under `packages/desktop/src`; terminal appears as conversation activity concept, not a security-grade executor.
- **Reuse decision:** conceptual progressive-disclosure of terminal/tool activity only. **No direct source adaptation** recommended for process execution. Any later adaptation needs commit, path, license notice in `THIRD_PARTY_NOTICES.md`.

### 2.4 Upstream baseline policy

`docs/references/upstream-baseline.md` and AGENTS rules:

- Domain state in SQLite; shell must not own agent-domain logic.
- Approvals enforced below UI; durable objects project-scoped.
- Write-capable tools require explicit approval-path tests → **command execution is write-adjacent and stricter**.
- Grok App / zbbody remain conceptual unless provenance is recorded.

---

## 3. Current Pillow seams a terminal capability would consume

```text
Renderer (React)
  conversationGateway / write-approval bar pattern / agent activity
        |  typed Tauri only (no FS, no spawn)
Desktop host (Tauri Rust)
  path_scope ──► conversation root authority
  write_commands pattern ──► list / decide / open_detail
  conversation_service ──► runtime-host + SQLite
        |
runtime-host
  governed_read / governed_write coordinators (pattern for governed_exec)
  cancel arbitration (permit exact terminal receipts before run_cancelled)
        |
adapter-grok-acp
  normalize provider proposal ── no approval authority
  process::{spawn_child, JobObject, build_child_env} ── reuse for tool children
        |
kernel-policy ── approve_once / reject / TTL / transitions
tool-executor ── ProjectCapability + new GovernedExec gateway
agent-protocol ── new ToolId + fixed manifest + wire validation
storage-sqlite ── new migration: exec_proposals (metadata) + transitions + decisions
```

| Seam | Consume how | Do not widen |
| --- | --- | --- |
| `agent-protocol` | Add `process.run@1` (and later `process.pty@1`) to fixed catalog; deny unknown tools | Open-ended MCP/shell tool names |
| `kernel-policy` | Parallel SM to write proposals or shared approval primitives | Allow-session auto-approve in C4.0 |
| `runtime-host` | Run-scoped coordinator: seal argv, mint grant, wait decision, spawn under cancel | Provider-driven apply |
| `tool-executor` | `GovernedExecToolGateway`: resolve cwd, spawn, bound I/O, kill tree | Renderer spawn |
| `storage-sqlite` | Metadata ledger like `write_proposals`; FK project→conversation→run | Full stdout as second secret store |
| Tauri | Narrow decide DTO: conversationId, proposalId, grant, decision | Passing argv/env from renderer |
| `path_scope` | Require conversation root for list/decide/stream views | Picker-less root invent |
| ACP process modules | Lift/share Job Object + env builders into a host-owned process crate or call from executor | Let adapter own product tool policy |
| Desktop process safety | Acceptance harness for residual children of tool exec | Numeric-only kill without identity |

---

## 4. Execution model comparison (Windows-first)

| Model | Mechanism | Security | Complexity | Fit |
| --- | --- | --- | --- | --- |
| **A. One-shot argv** | `CreateProcess` with `program` + `argv[]`, no shell | Strong: no metacharacters; approval binds exact argv digest | Low | **C4.0 adopt** |
| **B. Shell-string** | `cmd /c` or `powershell -Command` + free text | Weak: injection, chaining, expansion, policy evasion | Medium | **Reject for C4.0** |
| **C. Hybrid display** | Argv execution + optional human-readable “command line” string derived host-side | Strong if display string is non-authoritative | Low | **Adopt for UI only** |
| **D. ConPTY / interactive** | ConPTY pair, long-lived session, stdin streaming | High surface: TTY control, resize, secret echo, multi-command | High | **Later checkpoint** |
| **E. Provider-native shell tools** | ACP permission auto-allow / always-approve | Provider gains authority | — | **Already denied; keep denied** |

### Windows-specific notes

1. **Job Objects** are the correct tree-kill primitive (already proven in ACP child). Do not rely on `taskkill /T` as the sole success criterion; prove member PIDs gone (identity-complete), same as desktop-process safety lessons.
2. **CREATE_NO_WINDOW** for one-shot tools; interactive PTY later needs a console host / ConPTY and must not use “hidden console” as a security boundary.
3. **CREATE_SUSPENDED → AssignProcessToJobObject → ResumeThread** prevents race where the child spawns grandchildren before job assignment (ACP `child.rs` / `job_windows.rs`).
4. **Encoding:** child pipes are bytes. Prefer UTF-8 for capture; if invalid UTF-8, surface opaque replacement/`invalid_encoding` flag rather than guessing OEM code pages into durable storage. Do not claim “console CP” correctness in C4.0 receipts.
5. **PATH resolution:** host resolves `program` against an allow-listed search policy (absolute under project, or host allow-listed well-known tools). Reject bare provider basenames that resolve outside policy.
6. **`.cmd` / `.bat` / `.ps1`:** treat as shell-like; either deny in C4.0 or require explicit program path + no extra shell wrap. Prefer denying script hosts until a later policy.

---

## 5. Required decisions (locked recommendations)

### 5.1 Command proposal and approval ownership

| Actor | Owns | Must not own |
| --- | --- | --- |
| Provider | call_id, tool id/version, proposed program/argv/cwd (relative) | approval, root, grant, env secrets, absolute paths |
| Adapter | schema normalize; fixed method only | project capability; decision |
| Runtime host | identity injection; seal; grant mint; wait; spawn orchestration | UI chrome |
| Kernel policy | transitions, expiry, single-use grant | I/O |
| Executor | path/cwd resolve, spawn, caps, kill tree | approval |
| SQLite | metadata, digests, decisions, receipts | raw grant; unlimited transcript |
| Renderer | display DTO; submit decision | argv mutation; second apply |

**Policy for C4.0:** Ask mandatory; only `approve_once` | `reject`. No `allow_for_session`, `accept_edits`, or YOLO. TTL: start at **10 minutes** to match write proposals (`WRITE_PROPOSAL_TTL_SECS = 600`) unless UX research shortens it. Restart → all non-terminal exec proposals become `interrupted`; never auto-run.

**One active pending/approved/running exec proposal per run** (mirror write unique index). Same call identity + same fingerprint replays terminal receipt; different fingerprint → conflict.

### 5.2 cwd canonicalization and project containment

1. Wire `cwd` is a **portable project-relative path** (same grammar as B3/C3: no absolute, UNC, verbatim, backslash, colon, `.`/`..`, empty, control, Windows device, trailing-dot/space).
2. Host resolves via `ProjectCapability` + component walk; **reject reparse/symlink** components (reuse `resolve_target` / `is_reparse_point`).
3. Canonical cwd must `starts_with(canonical_project_root)` after every re-resolve at approve-time and at spawn-time (TOCTOU).
4. Default cwd = project root when omitted.
5. Fail closed if project root re-canonicalize drifts or is not a directory.
6. **Containment is not an OS sandbox.** C4.0 does not stop a trusted binary that opens `\\.\` or reaches outside via network; it stops *Pillow-granted* cwd/path escape. Hostile-admin / kernel threats remain out of scope (same C3 honesty).

### 5.3 Executable / argv versus shell text

**Wire (authoritative):**

```json
{
  "program": "cargo",
  "argv": ["test", "-p", "keeper-tool-executor"],
  "cwd": "."
}
```

Rules:

- `program` + `argv` are the sealed approval object (canonical JSON → digest).
- No shell operators as first-class args that imply host will re-shell (`|`, `&&`, `>`, etc. are **allowed as literal argv strings** only if the program is not a shell; they must not cause host-side interpretation).
- Host builds a **display command line** for UI (Windows `argv` quoting rules) that is **not** re-parsed for execution.
- Reject empty program; reject NUL in any string; cap argv count and total byte size (recommend: ≤ 64 args; program+argv total ≤ 32 KiB).
- Program resolution policy (C4.0):
  - Prefer explicit relative path under project after resolve; **or**
  - Host allow-list of well-known tools by basename (e.g. `cargo`, `rustc`, `git`, `node`, `pnpm`, `npm`, `python`, `python3`) resolved only via filtered PATH segments.
  - Deny absolute paths outside project root; deny UNC; deny `..` after resolve.
  - Deny shell hosts as program: `cmd`, `cmd.exe`, `powershell`, `pwsh`, `wscript`, `cscript`, `mshta`, `bash` (Git Bash) unless a later explicit design reopens them.

Shell-string tools remain a separate future method with its own threat model.

### 5.4 Allow-listed environment and secret exclusion

Start from ACP `build_child_env` (`process/env.rs`):

- **Allow base names only:** `SYSTEMROOT`, `WINDIR`, `COMSPEC`, `PATHEXT`, `PATH` (host-filtered), `TEMP`, `TMP`.
- **Never copy** names containing: `KEY`, `TOKEN`, `SECRET`, `PASSWORD`, `CREDENTIAL`, `AUTH`, provider vendor tokens (`OPENAI`, `ANTHROPIC`, `XAI`, `VOLC`, …).
- **Do not** set `HOME`/`USERPROFILE`/`APPDATA`/`LOCALAPPDATA` from parent unless an isolated profile is intentionally configured (default: omit or use a per-run temp profile under project `.pillow/runtime/…` if required for tooling — prefer omit in C4.0).
- Provider must **not** supply arbitrary env maps in C4.0. If a later overlay is needed, it is host-validated key allow-list only.
- Filter `PATH` to known safe roots (Windows system + host-configured toolchain dirs), not the full parent GUI PATH enrichment from Grok App (that enrichment is for agent CLIs, not for tool blast radius).

### 5.5 stdout/stderr ordering, encoding, bounds

| Concern | C4.0 rule |
| --- | --- |
| Capture | Piped stdout + stderr; merge into a **byte stream with stream tags** (`out`/`err`) and monotonic sequence, not wall-clock only |
| Ordering | Preserve per-stream order; interleave by read readiness; do not invent perfect total order across OS buffers |
| Encoding | Interpret as UTF-8 for display; on invalid sequences: replacement U+FFFD and `encoding=lossy` flag; never store raw secret-bearing binary as text claims |
| Caps | Hard max captured bytes (recommend **256 KiB** total combined, align near write/diff caps); excess → truncate + `truncated=true` + keep digests of full streams if still hashed during capture |
| Line caps | Optional display line cap (e.g. 4_000 lines) for UI projection |
| Progress | Optional run events with **summaries only** (bytes so far, truncated flag); not full stream in every event |
| Secrets | Redact known patterns in UI projection only as defense-in-depth; **do not** treat redaction as a security boundary |
| Exit | Record `exit_code`, `timed_out`, `cancelled`, `signal_or_status` (Windows: exit code only usually) |

### 5.6 Timeout, cancellation, restart, process-tree cleanup

| Event | Behavior |
| --- | --- |
| Default timeout | Manifest `timeout_ms` (recommend **30_000–120_000** for C4.0; tool-specific later) |
| Cancel | Host cancel → terminate Job Object → wait prove tracked PIDs gone → durable `cancelled` or `interrupted` per SM |
| Timeout | Same kill path → `failed` with public code `timeout` (or dedicated) |
| Drop / restart | Non-terminal → `interrupted`; never auto-resume execution |
| Applying/running lease | Exclusive owner + lease like write apply claim |
| Cleanup proof | Fail closed if tree members remain; do not claim success from direct child exit alone |
| MSVC vctip etc. | Out of ownership; never terminate by numeric PID without identity rules (desktop-process lessons) |

Reuse / extract ACP Job Object code rather than re-implementing ad hoc `taskkill`.

### 5.7 Durable metadata versus transcript persistence

**Always durable (SQLite):**

- proposal_id, reservation/fingerprint, project/conversation/run/call
- program, argv (as canonical JSON **or** argv_digest + sealed host memory — prefer **digest-only in SQLite** if argv can contain secrets; if argv is persisted, treat as sensitive metadata with same privacy rules as paths)
- cwd portable path, resolved cwd digest optional
- state machine, expires_at, decision grant digest, transitions
- exit_code, timed_out, truncated flags
- stdout_digest, stderr_digest, captured_bytes
- result_json bounded (≤ 8 KiB public summary), like write receipts

**Prefer host-memory / ephemeral for:**

- full captured transcript during the live run
- raw approval grant

**Optional bounded transcript store (only if product requires re-open after restart):**

- separate table with hard byte cap; never unbounded
- default **off** for C4.0 if conversation timeline already shows a truncated projection while live
- never log full transcript to adapter/provider logs

**Recommendation:** C4.0 persists **metadata + digests + exit/truncation**; live UI streams from host memory; restart loses full transcript and marks incomplete capture as interrupted/failed without inventing output.

### 5.8 One-shot checkpoint interfaces vs later PTY interfaces

#### C4.0 — `process.run@1` (one-shot)

Provider method (illustrative): `pillow/process_tool` with tool_id `process.run`.

Lifecycle (mirror write):

```text
pending_approval
  -> approved -> running -> succeeded | failed
  -> rejected | expired | cancelled | interrupted
approved -> expired | cancelled | interrupted | running
running  -> succeeded | failed | interrupted | cancelled*
```

\*Choose one terminal cancel naming and keep policy tests strict; do not allow reopen.

Tauri (mirror write_commands):

- `list_exec_proposals(conversation_id)`
- `decide_exec_proposal(conversation_id, proposal_id, approval_grant, decision)`
- `open_exec_output(conversation_id, proposal_id)` → bounded view while live / if stored

UI:

```text
timeline summary -> single approval bar (command display) -> optional output disclosure -> terminal row
```

Conversation remains dominant; no permanent terminal dashboard.

#### C4.x — `process.pty@1` (interactive Windows terminal)

Separate tool and ledger. Additional concerns **out of C4.0**:

- ConPTY create/resize/read/write
- session lifetime vs conversation/run
- stdin injection and secret echo
- multi-command under one approval (almost always wrong — prefer re-approve or explicit session trust design)
- UI terminal pane (still conversation-owned, collapsible)
- encoding/code page and VT sequences
- detach/reattach after restart (likely fail-closed interrupt)

**Interface split is mandatory:** one-shot APIs must not grow silent PTY modes via flags.

### 5.9 Explicit non-goals and fail-closed cases

**Non-goals (C4.0):**

- Interactive terminal / ConPTY / xterm.js surface
- Shell-string execution as primary wire form
- `allow_for_session` / auto-approve / YOLO
- Network tools, credential managers, keyring in env
- Git mutations via shell (owned by C4 Git research track)
- Multi-file / multi-command batch approval
- Provider-native ACP shell tools
- OS sandbox / AppContainer / Hyper-V isolation (future hardening)
- Background daemon installers, service control, driver loads
- Elevating UAC / admin shells

**Fail closed when:**

| Condition | Outcome |
| --- | --- |
| Foreign project/conversation/run | reject |
| Path/cwd grammar or reparse escape | `path_rejected` |
| Program not resolvable under policy | `program_rejected` |
| Shell host program | `program_rejected` |
| Expired / bad grant / fingerprint mismatch | conflict/deny; no spawn |
| Second decision / second spawn | conflict; replay only |
| Cancel or restart mid-flight | kill tree; interrupted/cancelled |
| Incomplete process identity at cleanup | fail cleanup; surface host error; do not greenwash |
| Output over cap | truncate + flag; still terminalize |
| Provider tries native write/shell permission | adapter deny (status quo) |
| Renderer supplies argv/env/root | schema reject |
| Busy run (active exec or write) | busy / conflict per policy |

---

## 6. Recommended C4.0 architecture (smallest safe)

### 6.1 Objects

| Object | Role |
| --- | --- |
| `ToolId::ProcessRun` / `process.run@1` | Fixed catalog entry |
| `ExecProposal` | Durable metadata ledger row |
| `SealedExecSpec` | program/argv/cwd in host memory + digests |
| `ExecApprovalDecision` | `approve_once` \| `reject` |
| `ExecReceipt` | exit, digests, truncated, timed_out |
| `LiveExecView` | UI DTO: summary, state, grant?, bounded output |

### 6.2 End-to-end flow

```text
[Provider] pillow/process_tool {callId, process.run@1, program, argv, cwd?}
    -> [Adapter] validate fixed schema; strip foreign authority
    -> [Host] inject project/conversation/run; canonicalize cwd; resolve program
    -> [SQLite] reserve fingerprint; state=pending_approval; store digests/meta
    -> [Host] mint grant; emit run_event exec_proposal_requested; wait
    -> [UI] approval bar shows host display line + cwd; Approve once | Reject
    -> [Host] policy check + consume grant
    -> [Executor] Job Object spawn; capture with caps; cancel/timeout watches
    -> [SQLite] terminal receipt + transitions
    -> [Host] wake provider with opaque ok/error; project activity row
```

### 6.3 Suggested caps (initial)

| Cap | Value | Rationale |
| --- | --- | --- |
| argv total bytes | 32 KiB | approval readability |
| argv count | 64 | flood control |
| combined capture | 256 KiB | align write/diff |
| timeout | 60 s default | human-wait vs hang |
| concurrent active exec / run | 1 | same as write |
| proposal TTL | 600 s | match C3 |
| result_json | ≤ 8 KiB | match write ledger |

Tune only with tests and security research (grok4 track).

### 6.4 Test seams (must exist before implementation claim)

1. Protocol fixtures: schema, path/cwd matrix, unknown keys deny, argv digests.
2. Policy: transitions, expiry, grant single-use, restart interrupt.
3. Executor: Job Object kill of grandchild; cwd escape; reparse; program allow/deny; byte truncate; timeout; cancel.
4. Env: secret parent env not present in child.
5. Host bridge: one pending per run; cancel receipt ordering (learn from C3-F).
6. Tauri: decide DTO cannot smuggle argv; conversation capability required.
7. Desktop acceptance: residual process identity zero after cancel/timeout/crash.
8. Encoding/loss flags unit tests.

### 6.5 Provenance rules for any later code lift

| Source | License | Allowed now |
| --- | --- | --- |
| Pillow ACP `process/*` | project | **Preferred** share/extract |
| Grok App process_util/permission | MIT | conceptual; CREATE_NO_WINDOW pattern OK to re-implement |
| zbbody | Apache-2.0 | conceptual UI only; no executor copy |
| C3 write stack | project | **Primary pattern** for approval/ledger |

---

## 7. Split with Git and interaction research

This report owns **process execution authority and Windows lifecycle**.

- **Git track (grok2):** must not depend on shell `git …` as the security boundary; prefer structured Git APIs. If Git CLI is used later, it should still go through `process.run` policy, not a bypass.
- **Interaction track (grok3):** approve-once bar + progressive output disclosure; no permanent terminal card stack; PTY UI deferred.
- **Security track (grok4):** adversarial matrix for injection, path escape, env leak, process orphans, approval replay.

Controller design gate should require all four before any implementation brief.

---

## 8. Implementation sequencing (research-only plan)

1. **C4-A** Protocol + policy + migration for `process.run@1` (no spawn).
2. **C4-B** Executor Job Object one-shot with fake/child fixtures.
3. **C4-C** Runtime host coordinator + cancel/timeout.
4. **C4-D** Tauri DTO + approval UI reuse from C3 bar.
5. **C4-E** Real desktop adversarial acceptance (cancel, timeout, grandchild, project isolation, restart).
6. **Later** ConPTY design (new research + design doc); not an additive flag on run.

No step is authorized by this research document.

---

## 9. Residual risks

1. **Argv persistence may contain secrets** (tokens passed on CLI). Digest-only SQLite storage reduces but does not eliminate live UI leakage; redaction is imperfect.
2. **Allow-listed toolchains still have full user power** (e.g. `node -e`, `python -c`). C4.0 should deny `-e`/`-c` style flags for script hosts or deny those hosts until policy matures.
3. **PATH allow-list maintenance** will lag real developer machines; too strict breaks UX, too loose widens blast radius.
4. **Job Object ≠ sandbox.** Network, filesystem outside via absolute paths inside child logic, and COM remain open.
5. **Incomplete CIM/process identity** on Windows cleanup remains a harness and product concern (C3 residual Node orphans lesson).
6. **PTY later will reopen multi-command trust** — keep interfaces split so C4.0 green does not imply interactive safety.
7. **Parallel C4 Git research** may tempt shell-based Git; reject dual authority paths.
8. Working tree is dirty with accepted C1–C3; C4 must not land until controller design + user authorization, separate from staging the C3 tree.

---

## 10. Completion

### Status: DONE

### Changed files (truth)

| Path | Action |
| --- | --- |
| `docs/research/2026-08-02-c4-terminal-process-research.md` | **created** (this file) |

No other files modified. No stage, commit, product code, tests, or reference-repo changes.

### Keeper completion signal

```text
DONE - C4 terminal/process research complete (grok1).
Report: docs/research/2026-08-02-c4-terminal-process-research.md
Recommendation: C4.0 = conversation-owned process.run@1 one-shot argv + Job Object
  + C3-style approve_once; defer ConPTY/shell-string/allow-session.
Evidence: C3 design/acceptance/process-safety; Grok App cbbbdf17 (MIT) conceptual;
  zbbody 00c29341 (Apache-2.0) conceptual only; Pillow seams mapped.
Implementation not authorized. Staging empty for this worker; only allowlist file written.
```

### Blocker signal

None for research delivery. Implementation remains **blocked** until controller integrates grok1–4 research, writes a C4 design, and receives explicit user authorization (per C3 checkpoint and C4-R dispatch brief).
