# Wave C4 Research — Security Threat Model And Acceptance Matrix

Date: 2026-08-02
Researcher: grok4 (slot `grok4`, controller-boot agentName)
Workspace: `<workspace>\pillow`
Baseline HEAD inspected: `7b7f70f7360131cd1cfd984a8b19bdfb5725c21b`
Scope: adversarial threat model and executable acceptance matrix for C4
terminal and Git. Research only; product implementation not authorized.

Keeper Bridge: session `<keeper-session>`, project `pillow`,
agentRole `controller` (boot accepted; task
`C4 security model and acceptance research`).

---

## 0. Purpose, non-claims, and evidence order

### 0.1 Purpose

Define the **minimum security and acceptance gates** that C4 terminal and Git
must pass before either capability can be controller-accepted. The model is
grounded in:

1. accepted C3 governed-write authority (design, implementation plan, final
   acceptance, real desktop harness);
2. Windows process-safety lessons from the forced-restart / bare-PID incident
   and the repaired `ProcessSupervisor`;
3. local Grok App boundaries at pinned commit
   `cbbbdf17bb8db1588b6707f3f42f6b00397e27f9` (MIT) under
   `<workspace>\VaultSystem\references\grok-app`;
4. current Pillow protocol validation, `kernel-policy`, receipts,
   cancellation, restart recovery, and desktop E2E harness seams.

### 0.2 Explicit non-claims

- This report is **research only**. It does not authorize C4 design, product
  code, staging, or commit.
- Reference implementations (Grok App, zbbody, legacy Keeper, Codex) are
  **evidence**, not authority. Pillow must re-derive typed contracts.
- Sibling C4 research reports (`terminal-process`, `git-governance`,
  `interaction`) were not present at write time; this security matrix stands
  alone and must be re-aligned if those reports narrow product surface.
- C3 is an accepted **working-tree checkpoint**, not a released package.
- Project trust is **not** an OS sandbox. C4 must declare actual isolation
  limits; the UI must not claim more.

### 0.3 Evidence inventory (required reads)

| Evidence class | Path / identity | Role for C4 |
| --- | --- | --- |
| C3 design | `docs/superpowers/specs/2026-08-01-c3-governed-write-design.md` | Authority spine: host seals payload, human one-shot grant, CAS apply, restart invalidates non-terminal |
| C3 acceptance | `docs/verification/2026-08-02-wave-c3-acceptance.md` | Eight real scenarios; Critical 0 / Important 0 pattern |
| C3 security research | `docs/research/2026-08-01-c3-governed-write-security-research.md` | Threat/control table template; path/replay/privacy controls |
| C3 process incident | `docs/handoff/2026-08-01-pillow-controller-handoff.md` §forced-restart | Bugcheck `0xEF` from `taskkill` + bare PID reuse |
| Process supervisor | `scripts/lib/desktop-process.mjs` + `desktop-process-safety.test.mjs` | Identity-bound cleanup; no numeric kill; residual proof |
| Process repair report | `docs/execution/2026-08-01-desktop-process-cleanup-safety-fix-report.md` | Evolution 7→41 safety tests; VCTIP/incomplete-descendant rules |
| Grok App baseline | `docs/references/upstream-baseline.md` + clone HEAD `cbbbdf17…` | Permission, shell scope, git argv, process pool (evidence only) |
| Grok interaction research | `docs/research/2026-08-01-c3-grok-app-interaction-research.md` | Approve UX primitives; YOLO/session-cache warnings |
| B2 security research | `docs/research/2026-07-30-grok-security-acceptance.md` | Env allow-list, profile isolation, cancel residual rules |
| Protocol | `crates/agent-protocol/**` | Fixed catalogs, portable paths, public errors, write reservation |
| Policy | `crates/kernel-policy/**` | Terminal non-reopen; approve_once; expiry |
| Write executor / ledger | `crates/tool-executor/tests/governed_write.rs`, storage write tables | CAS, cancel, concurrent claim patterns |
| Restart recovery | `apps/desktop/src-tauri/src/conversation_service/service.rs` `recover_inflight` | Inflight → interrupted; write proposals invalidated |
| Desktop E2E harness | `scripts/wave-c3-governed-write-acceptance.mjs`, `apps/desktop/tests/e2e/c3-governed-write.spec.ts` | Real Tauri/WebView2 scenario + residual process proof pattern |

### 0.4 Authority order (non-negotiable; inherited from C3)

```text
Provider child (untrusted)
  -> proposes method + arguments only
Adapter
  -> validates fixed schema; no project root, no approval, no process kill
Runtime host
  -> injects project/conversation/run identity; seals proposal
SQLite
  -> durable metadata, digests, transitions, decisions, receipts
Conversation UI
  -> renders host DTO; returns proposal id + opaque one-time grant + decision
Kernel policy
  -> ownership, expiry, fingerprint, single-use grant
Capability executor (terminal or git)
  -> revalidates seal; applies only under host policy
SQLite + run events
  -> terminal receipt before UI/provider completion
```

The provider can never approve. The renderer never supplies executable args,
cwd roots, Git argv, env maps, base digests, or apply content. Read-only
`resourceGateway` remains read-only.

---

## 1. Severity model

Severity is used for **acceptance rejection**, not for informal triage. A gate
fails if any Critical remains open, or if any Important remains open without an
explicit controller-accepted residual with compensating control.

### 1.1 Severity definitions

| Level | Definition | Acceptance effect |
| --- | --- | --- |
| **Critical** | Unauthorized durable mutation, process kill of non-owned identity, secret/credential leakage into durable store or public surface, cross-project/cross-conversation authority break, approval text ≠ executed action, or auto-apply after restart | **Hard reject.** C4 cannot be accepted. |
| **Important** | Bypass of a required control with bounded impact (e.g. output flood without durable write, incomplete residual proof that leaves supervised residuals, stale Git status shown as authoritative, session-scope auto-allow of a new tool class) | **Hard reject** unless controller records residual + compensating control and user accepts residual. Default for industrial Alpha: treat Important as reject. |
| **Minor** | UX friction, timing flakiness with no authority break, documentation debt, performance within soft budget but above hard budget by <2× in one fixture | Acceptable with tracked follow-up; must not silence Important. |
| **Info** | Design notes, deferred scope, reference comparison | No gate impact. |

### 1.2 Mapping from C3 practice

C3 final acceptance used **Critical 0 / Important 0** with independent review.
C4 inherits the same bar. Process-safety repairs that left residuals rather
than force-kill unrelated PIDs are **correct fail-closed behavior**, not
Important product defects — provided the residual is reported and not silently
declared clean.

### 1.3 Asset inventory for C4

| Asset | Compromise impact |
| --- | --- |
| Project filesystem | Arbitrary code via scripts; secret file reads; destructive Git restore |
| Git object store / worktree | History rewrite, wrong branch, submodule theft, worktree escape |
| OS process namespace | Kill unrelated services (historical bugcheck); orphan agents holding locks |
| Credentials / env | Token theft from parent env, Git helpers, `.env`, key files |
| SQLite operational authority | False receipts; approval replay; silent lifecycle lies |
| Conversation UI / screenshots | Secret leakage in DOM, logs, E2E artifacts |
| Diagnostic / residual reports | False green cleanup; PID reuse stories that hide orphans |

### 1.4 Trust zones

| Zone | Components | Trust posture |
| --- | --- | --- |
| Z0 Locked kernel | Policy transitions, migrations, release verification, secret refs | Highest; provider payloads cannot rewrite |
| Z1 Domain / host | SQLite, ConversationService, RuntimeHost, protocol validate | Authority for run/proposal truth |
| Z2 Adapter | `adapter-grok-acp` | Transport + normalize only |
| Z3 Child runtime | Grok CLI / future terminal children / Git CLI | Untrusted I/O and process trees |
| Z4 Renderer | React workbench | Least privilege; no FS/process/Git argv |
| Z5 Diagnostics / acceptance | Logs, E2E records, screenshots, process snapshots | Public-safe; redacted |

---

## 2. Capability ladder and product boundary (security view)

C4 must not collapse into "agent may shell freely." Recommend a **ladder** so
acceptance can gate each rung:

| Rung | Capability | Approval default | C4 first checkpoint candidate |
| --- | --- | --- | --- |
| T0 | Deny all process execution | n/a | Current C3 exit state |
| T1 | One-shot **argv** execution (no shell), project cwd, bounded output | `approve_once` per sealed proposal | **Yes** |
| T2 | Interactive ConPTY / shell session | Separate design; higher residual | Later |
| G0 | Deny all Git mutation | n/a | Current |
| G1 | Read-only Git: discover / status / branch / bounded diff | Auto-allow **read** only under host argv policy | **Yes** |
| G2 | Stage / unstage / commit / branch-switch / single-file restore | `approve_once` per sealed Git proposal | Optional second checkpoint |
| G3 | Reset-hard, clean, force-checkout, rebase, merge, push, arbitrary argv | Explicitly excluded | **No** for Stage 1 |

Security rule: **T1 and G1 may ship without T2/G2**, but G2/T2 cannot ship
without T1/G1 gates green. Session-scoped auto-allow (`allow_for_session`),
YOLO, AcceptEdits, and download-command heuristics from Grok App are **not**
default C4 product policy (evidence of risk, not adoption).

---

## 3. Capability-by-capability threat table

Each row lists primary severity if control fails, attack sketch, required
control, and reusable Pillow substrate.

### 3.1 Terminal / process execution (T1)

| ID | Threat | Sev | Attack sketch | Required control | Substrate / evidence |
| --- | --- | --- | --- | --- | --- |
| TERM-01 | Command injection via shell metacharacters | Critical | Proposal shows `npm test`; executor runs `cmd /c` with injected `& del /s` | Sealed **argv array** only; no shell; no string join into `cmd.exe /c` | C3 seal pattern; Grok App git "safe argv — never shell" (`commands.rs` worktree/diff) |
| TERM-02 | Approval-text / actual-command mismatch | Critical | UI displays pretty summary; host runs different argv after approve | Fingerprint over exact `executable + argv + cwd + env_digest + timeout`; UI shows host-normalized display of **sealed** fields only; apply reloads seal | C3 `proposal_fingerprint` + grant digest |
| TERM-03 | Cwd escape / project-switch race | Critical | Approve while cwd still project A; switch project or rewrite junction | Capture canonical project root at proposal; re-resolve cwd at execute; require cwd strictly under project root; project switch parks/invalidates foreign grants | C3 path_scope + two-projects scenario |
| TERM-04 | Symlink / reparse-point escape | Critical | `cwd` or executable path is junction out of project | Component-wise reparse reject (no follow) on cwd, executable, and every path arg classified as path-like | B3/C3 `resolve_target` reparse refuse |
| TERM-05 | Environment / credential leakage into child | Critical | Child inherits `XAI_API_KEY`, user tokens | Build env from allow-list; sensitive-name denylist; never `process.env` wholesale | B2 research §2; Grok App enriched PATH only as evidence of need, not full inherit |
| TERM-06 | Malicious output escape sequences | Important | CSI sequences rewrite terminal UI; OSC clipboard; DCS | Strip / neutralize C0/C1 control (except `\n\r\t`); no raw PTY passthrough in T1; UI renders escaped text | New; not present in C3 write path |
| TERM-07 | Output flooding / DoS | Important | Infinite stdout fills SQLite or UI | Byte/line caps; ring buffer; fail-closed truncation marker; no full stream in SQLite | Protocol manifest caps pattern (256 KiB class) |
| TERM-08 | Encoding faults | Important | Invalid UTF-8 / mixed code pages smuggle binary | Decode lossy with explicit `encoding_fault` flag; reject NUL in metadata; binary mode out of T1 product claims | C3 UTF-8-only discipline |
| TERM-09 | Child / grandchild escape after cancel | Critical | Cancel parent; grandchild keeps writing/network | Cooperative cancel first; host-owned kill only via stable handle for **root** process of the supervised job; no bare PID tree kill; residual proof | ProcessSupervisor + forced-restart incident |
| TERM-10 | PID reuse kill of unrelated process | Critical | Historical: `taskkill /T /F` after PID reuse killed `svchost` → bugcheck `0xEF` | Identity = pid + path + name + FILETIME ticks; incomplete identity fail-closed; never terminate observation-only descendants by PID | `desktop-process.mjs`; handoff §forced-restart |
| TERM-11 | Incomplete identity treated as clean | Critical | Snapshot missing creation ticks → assume dead | States: absent / present-incomplete / present-complete / never-observed / confirmed-dead / identity-mismatch / identity-replaced / identity-incomplete | Safety tests 41/41 |
| TERM-12 | Host restart auto-continues execution | Critical | Pending approved command runs after reboot | Restart: non-terminal terminal proposals → `interrupted`; no auto-exec; live grants destroyed | C3 `recover_inflight` + write proposal interrupt |
| TERM-13 | Orphan processes after harness/app exit | Important | Residual agent holds file locks; false green | Residual observation of named product binaries + supervised listeners; fail cleanup if residual; never force-kill unbound | C3 E2E residual proof pattern |
| TERM-14 | Executable override from untrusted options | Critical | Provider sets `executable=powershell` | Config-owned executable allow-list; reject caller-supplied binary path outside list; resolve under reparse policy | B2 T10; launch builder |
| TERM-15 | Timeout race double-apply | Important | Timeout and approve both fire | Exclusive claim lease; single terminal receipt; second path Conflict | Tool claim pattern |
| TERM-16 | Performance budget breach | Important | Command hangs UI / fills disk | Hard timeout; output cap; concurrent one-shot per run default 1; process pool cap separate from T1 | Grok App process_limits as evidence of need only |

### 3.2 Git read surface (G1)

| ID | Threat | Sev | Attack sketch | Required control | Substrate / evidence |
| --- | --- | --- | --- | --- | --- |
| GIT-R01 | Option injection via path/ref | Critical | Path `-c core.sshCommand=…` or `--exec=…` | Fixed argv templates; path after `--`; reject leading `-` in path/ref; no free-form argv | Grok App `git` argv arrays + sanitize helpers |
| GIT-R02 | Config alias / `-c` injection | Critical | Alias expands to write | Never pass user `-c`; set `GIT_CONFIG_NOSYSTEM=1` + isolated empty config where feasible; prefer libgit2/gix later | Evidence: need stronger than Grok App (App does not fully isolate config) |
| GIT-R03 | Hooks execution on read | Critical | Malicious `post-checkout` not on pure read, but `diff` filters may run | Use `--no-ext-diff`; disable textconv/filters where API allows (`--no-textconv` when available); document residual if porcelain still invokes attributes | Grok App `git_file_diff` uses `--no-ext-diff` |
| GIT-R04 | External diff / textconv RCE | Critical | `.gitattributes` filter runs attacker binary | Same as GIT-R03; fail-closed if only unsafe APIs available | Grok App evidence; C4 must tighten |
| GIT-R05 | Nested repo / ownership escape | Critical | Nested `.git` outside trust; dubious ownership | Discover only under project root; refuse repos whose worktree root is outside project; refuse Windows unsafe ownership without explicit trust UX | Portable path + project capability |
| GIT-R06 | Submodule / worktree path escape | Critical | Worktree path points outside project | Parse porcelain; reject absolute paths outside root; no auto-init submodule fetch in G1 | Grok App worktree sanitize as partial evidence |
| GIT-R07 | Stale status / false clean | Important | UI shows clean while dirty | Status bound to `head_oid + index_digest + status_epoch`; refresh invalidates; never claim "clean" without fresh host snapshot | C3 base-hash CAS analogy |
| GIT-R08 | Diff bomb | Important | Huge binary as text diff | Byte/line caps (align C3 256 KiB / 4k lines class); binary detection; truncation marker | C3 diff caps; Grok App 400_000 char take as upper caution |
| GIT-R09 | Secret leakage in diff | Important | Diff includes `.env` / keys | Reuse C3 denied path/leaf policy for default redaction or refusal to open credential-like paths in pane | C3 denied `.env`, keys, pem |
| GIT-R10 | Cross-project status leakage | Critical | Project B status in Project A pane | All DTOs project-scoped; UI filter by conversation/project ownership | C3 two-projects + project-isolation fix |

### 3.3 Git mutation surface (G2)

| ID | Threat | Sev | Attack sketch | Required control | Substrate / evidence |
| --- | --- | --- | --- | --- | --- |
| GIT-W01 | Destructive restore without base proof | Critical | Restore file while user edited after status | Sealed `path + base_blob_oid/hash`; re-check before restore; refuse on drift | C3 base-hash CAS |
| GIT-W02 | Stage/commit approval mismatch | Critical | UI lists file A; commit includes file B | Fingerprint over exact path set + op + optional message digest | C3 fingerprint |
| GIT-W03 | Commit author / credential abuse | Critical | Commit with stolen author or credential helper prompt | No credential helper UI in Stage 1; fixed author from host config or refuse; `GIT_TERMINAL_PROMPT=0` | B2 env isolation |
| GIT-W04 | Branch switch with dirty tree | Important | Silent data loss | Fail-closed unless sealed force flag **excluded**; default refuse dirty switch | Explicit exclusion of force checkout |
| GIT-W05 | Hooks on commit/checkout | Critical | Malicious hook RCE on approve | Prefer pure library writes; if CLI, isolate config and disable hooks (`core.hooksPath` empty dir) with tests proving hooks did not run | Must exceed Grok App |
| GIT-W06 | Conflict / merge state lie | Important | Report success during conflicted merge | Detect `MERGE_HEAD` / unmerged; refuse G2 ops with opaque conflict code | New |
| GIT-W07 | Replay stage across conversations | Critical | Reuse grant from conv A on conv B | Same grant + ownership + fingerprint rules as C3 | C3 approval policy |
| GIT-W08 | Nested submodule push-adjacent ops | Critical | Submodule update fetches remote | Exclude fetch/push/submodule update from G2 | Roadmap exclusion |
| GIT-W09 | Worktree add outside project | Critical | Sibling path escapes root | Refuse worktree paths outside project (stricter than Grok App sibling placement) | Grok App places siblings — **do not adopt** without project bound |

### 3.4 Cross-cutting authority / privacy / DoS

| ID | Threat | Sev | Attack sketch | Required control | Substrate / evidence |
| --- | --- | --- | --- | --- | --- |
| X-01 | Cross-project approval replay | Critical | Approve proposal id from project A while viewing B | Decision checks conversation + project FK + grant digest + fingerprint in one transaction | C3 design §7; project-isolation fix |
| X-02 | Cross-conversation approval replay | Critical | Same as X-01 across chats | Same | C3 |
| X-03 | Provider-native permission auto-allow | Critical | ACP `session/request_permission` for shell/git | Keep adapter `decide_permission` deny; only Pillow methods enter C4 | Adapter permission deny tests |
| X-04 | Renderer authority bypass | Critical | Forged Tauri invoke with alternate argv | Commands reload sealed proposal by id; ignore client argv/cwd/content | C3 apply DTO |
| X-05 | DOM / log / SQLite / screenshot privacy | Important | Secrets in timeline, logs, E2E PNGs, SQLite events | Metadata + digests only in SQLite; redacted diagnostics; screenshots scrub; portable paths only | C3 content-not-in-SQLite; native-path privacy fix |
| X-06 | Public error path leakage | Important | IO errors with absolute paths | Canonical public error codes only | `public_error.rs` |
| X-07 | Denial of service (CPU/disk/UI) | Important | Parallel commands + huge diffs | Caps: concurrent ops, output bytes, proposal TTL, pool size | Manifests + process_limits evidence |
| X-08 | False residual-clean report | Critical | Harness claims clean while orphans live | Injected residual observation under `enforceDesktopClean`; unbound residuals reported not killed | Process safety closure design |
| X-09 | Allow-session / YOLO widening | Critical | Port Grok App AlwaysApprove for shell | Product default Ask; C4 first checkpoint **approve_once only** for T1/G2; no session auto-allow for execute/mutate | C3 no allow-for-session |
| X-10 | Diff/base drift (write + git) | Critical | External edit after approve | CAS / oid recheck immediately before mutate | C3 base-drift scenario |

---

## 4. Sealed proposal contracts (security shape)

These shapes are **acceptance requirements** for any later design; names may
change if sibling research renames them.

### 4.1 Terminal one-shot (`proc.exec@1` illustrative)

```text
TerminalProposal {
  project_id, conversation_id, run_id, call_id,
  tool_id: "proc.exec", tool_version: 1,
  executable: allowlisted_id | resolved_path_under_policy,
  argv: [string],                 // exact sealed argv, no shell
  cwd_relative: portable path or ".",
  env_digest: sha256 of canonical allow-listed env map,
  timeout_ms: u32,
  output_byte_cap: u64,           // e.g. 262_144
  proposal_fingerprint: sha256,   // over all sealed fields
  expires_at
}
```

Rules:

1. Host, not provider, resolves executable and captures env digest.
2. Renderer decision DTO: `{ proposalId, conversationId, approvalGrant, decision }` only.
3. Execute reloads seal, re-resolves cwd/executable, re-checks project grant.
4. Transcript may store truncated stdout/stderr digests + capped preview; never
   unbounded stream in SQLite.
5. No `allow_for_session` for T1 in first checkpoint.

### 4.2 Git read (`git.status@1` / `git.diff@1` illustrative)

```text
GitReadRequest {
  project_id, conversation_id, run_id, call_id,
  op: status | branch | diff | discover,
  path?: portable path,          // for single-file diff
  head_hint?: oid,               // optional freshness
}
```

Rules:

1. Fixed argv or library calls only.
2. Diff output bounded; binary → opaque "binary" result.
3. Credential-like paths fail closed or redacted per C3 deny list.
4. Auto-allow **read** only after host validation (no human bar), unless
   product later chooses Ask for all Git — default recommendation: G1 auto,
   G2 Ask.

### 4.3 Git mutate (`git.mutate@1` illustrative)

```text
GitMutateProposal {
  project_id, conversation_id, run_id, call_id,
  op: stage | unstage | commit | switch_branch | restore_file,
  paths: [portable path],        // exact set
  message_digest?: sha256,       // commit only; raw message host-memory
  base_refs: { head_oid, index_digest, path_blob_oids… },
  proposal_fingerprint,
  expires_at
}
```

Rules mirror C3: host captures base refs; approve_once; CAS; receipt; restart
interrupts non-terminal.

---

## 5. Process lifecycle and residual-process proof

### 5.1 Lessons that are mandatory for C4

From the forced-restart incident and repaired supervisor:

1. **Never** use `taskkill.exe`, `/T` tree kill, or numeric PID kill of
   non-handle-owned processes.
2. Root termination only through launcher-owned stable `ChildProcess` handle.
3. Descendants are observation-only; never terminated by PID.
4. Identity completeness is a security control, not a nicety.
5. Residual processes that cannot be proven may remain; cleanup **fails
   closed** rather than expanding kill authority.
6. Default-off MSVC `vctip.exe` allow is observation only at a strict
   Program Files path; never numeric kill of telemetry helpers.

### 5.2 C4 child process contract

| Phase | Required behavior |
| --- | --- |
| Spawn | Host owns spawn; argv sealed; env allow-listed; cwd re-resolved; CREATE_NO_WINDOW on Windows GUI host |
| Track | Immutable first identity; mismatch permanent |
| Cancel | Cooperative signal/API first; then root handle kill only |
| Wait | Budgeted observation; incomplete identity does not equal dead |
| Restart | No resume of live OS children across host restart; proposals interrupted |
| Proof | Fresh snapshot: product binaries + supervised listeners absent **or** residual report with fail |

### 5.3 Residual-process proof (acceptance artifact)

Every real desktop adversarial scenario must emit a machine-readable record
analogous to C3 `c3f-acceptance-record.json`:

```text
processCleanup: "passed" | "failed"
residuals: {
  pillow: 0,
  fake_acp_or_agent_children: 0,
  supervised_listeners: 0,
  unbound_reported: [...]   // never silently dropped
}
identityGates: exercised PID-reuse / incomplete-identity cases in unit layer
```

A green product claim **cannot** rest on "we killed everything by name."

### 5.4 What C4 product process supervision must not copy from harness

Harness supervisor is for **acceptance ownership** of the desktop app under
test. Product C4 must implement a **domain** process supervisor for agent
children with the same identity laws, but must not:

- kill WebView2/Pillow UI to cancel a tool;
- absorb third-party injectors into ownership;
- declare VCTIP or generic Node orphans as owned.

---

## 6. Focused unit and integration tests

Tests are **executable acceptance criteria**. Names are normative for the
security matrix; implementation may map them to crate test modules.

### 6.1 Protocol / policy units

| ID | Case | Expect | Sev if fail |
| --- | --- | --- | --- |
| U-P01 | Unknown tool ids for shell/git rejected until catalog fixed | Fail-closed | Critical |
| U-P02 | Terminal proposal rejects shell string field / metachar-only forms | InvalidArguments | Critical |
| U-P03 | Argv entry beginning with unsafe patterns still cannot break template (no shell) | Sealed argv only | Critical |
| U-P04 | Git path/ref starting with `-` rejected | InvalidArguments | Critical |
| U-P05 | Portable path matrix (same as C3) for cwd and git paths | Reject | Critical |
| U-P06 | Reservation Replay vs Conflict for terminal/git proposals | Same spine as write | Critical |
| U-P07 | Public errors reject custom path-bearing messages | Parse fail | Important |
| U-P08 | Policy: terminal/git non-terminal → interrupted legal; terminal non-reopen | Match kernel-policy style | Critical |
| U-P09 | Grant single-use; fingerprint mismatch | Conflict | Critical |
| U-P10 | Expiry at exact TTL boundary | Expired; no execute | Critical |

### 6.2 Terminal execution units / integration

| ID | Case | Expect | Sev if fail |
| --- | --- | --- | --- |
| U-T01 | Happy path argv under project cwd | Receipt + capped output digest | — |
| U-T02 | Approval display fields ≠ seal fields (mutated DTO) | Apply uses seal; ignore client | Critical |
| U-T03 | Cwd junction/reparse out of project | PathRejected; no spawn | Critical |
| U-T04 | Executable outside allow-list | Reject; no spawn | Critical |
| U-T05 | Env contains `*_TOKEN` / `*_KEY` from parent | Not present in child env | Critical |
| U-T06 | Output exceeds cap | Truncated + marker; process cancelled/reaped | Important |
| U-T07 | ANSI/OSC malicious sequences | Neutralized in UI DTO | Important |
| U-T08 | Cancel before spawn complete | Cancelled; no orphan root | Critical |
| U-T09 | Cancel after spawn | Root handle path; residual proof | Critical |
| U-T10 | Grandchild remains after root exit | Reported residual; no PID kill of grandchild | Important |
| U-T11 | Simulated PID reuse of root | No kill of new process; mismatch | Critical |
| U-T12 | Incomplete creation ticks | Fail-closed cleanup / no kill | Critical |
| U-T13 | Restart with pending approved exec | Interrupted; never auto-run | Critical |
| U-T14 | Concurrent second exec same run | Busy/Conflict | Important |
| U-T15 | Timeout | Terminal failed/cancelled receipt; reaped | Important |
| U-T16 | NUL / non-UTF-8 stdout | Encoding fault flag; no SQLite binary dump | Important |

### 6.3 Git units / integration

| ID | Case | Expect | Sev if fail |
| --- | --- | --- | --- |
| U-G01 | Status inside project repo | Bounded DTO | — |
| U-G02 | Nested repo outside project root | Reject / not discovered as owned | Critical |
| U-G03 | Diff uses fixed safe flags (`--no-ext-diff`, path after `--`) | Fixture asserts argv or library equivalent | Critical |
| U-G04 | External diff attribute would run attacker | Must not execute (hook/filter probe fixture) | Critical |
| U-G05 | Alias in global gitconfig would rewrite command | Isolated config or library path; alias not executed | Critical |
| U-G06 | Path option injection (`--output=…`, `-c`) | Rejected | Critical |
| U-G07 | Large/binary file diff | Truncated or binary marker | Important |
| U-G08 | Credential-like path diff | Denied/redacted | Important |
| U-G09 | Restore with base drift | Fail CAS; file unchanged | Critical |
| U-G10 | Stage path set mismatch after approve | Conflict; no partial stage | Critical |
| U-G11 | Commit when hooks planted | Hooks do not run **or** op refused with documented residual | Critical |
| U-G12 | Switch branch dirty tree | Fail-closed | Important |
| U-G13 | Submodule path escape | Reject | Critical |
| U-G14 | Worktree path outside project | Reject | Critical |
| U-G15 | Cross-conversation git grant replay | Conflict / NotAllowed | Critical |
| U-G16 | Stale status presented as fresh | Epoch/oid mismatch fails consumer | Important |
| U-G17 | MERGE_HEAD present | Mutate refused | Important |
| U-G18 | Restart mid-approved mutate | Interrupted; no partial commit | Critical |

### 6.4 Privacy / UI / adapter units

| ID | Case | Expect | Sev if fail |
| --- | --- | --- | --- |
| U-U01 | Foreign conversationId filters proposals | No render | Critical |
| U-U02 | No raw session/run/tool ids in approval bar | Absent | Important |
| U-U03 | No absolute machine paths in errors/UI | Opaque codes | Important |
| U-U04 | Provider shell permission request | Always denied; no spawn | Critical |
| U-U05 | resourceGateway still has no write/exec | Type/surface test | Critical |
| U-U06 | Screenshot fixtures scrub secrets | No env tokens in PNG OCR/text harness if used | Important |
| U-U07 | SQLite event payloads lack full stdout/argv secrets | Digests/caps only | Important |

### 6.5 Process-safety units (extend existing)

| ID | Case | Expect | Sev if fail |
| --- | --- | --- | --- |
| U-S01 | Existing 41 desktop-process safety cases remain green | No regression | Critical |
| U-S02 | New product supervisor fixtures: identity incomplete | No kill | Critical |
| U-S03 | New product supervisor: confirmed-dead then PID reuse | Ignore reuse | Critical |
| U-S04 | Forbidden termination scan (`taskkill`, bare kill APIs) | Static deny in product path | Critical |

---

## 7. Real desktop adversarial scenarios

Pattern: extend C3-F harness style (real Tauri/WebView2, supervised host,
scenario record, residual proof). Serial execution; no parallel desktop hosts
sharing residual claims.

### 7.1 Scenario pack (minimum for C4 checkpoint)

| Scenario | Steps | Pass criteria | Fail severity |
| --- | --- | --- | --- |
| `term-reject` | Provider proposes safe argv; human Reject | No process; terminal receipt rejected; residual 0 | Critical if process ran |
| `term-approve-echo` | Approve sealed `echo`/allowlisted argv | Exact argv receipt; capped output; residual 0 | Critical if mismatch |
| `term-approval-mismatch` | Renderer attempts alternate argv on decide | Host ignores; either reject or still runs **seal only** (prove seal) | Critical |
| `term-cwd-escape` | Proposal cwd uses `..` / junction fixture | No spawn; opaque error | Critical |
| `term-env-leak` | Parent env has synthetic `XAI_API_KEY=…`; child dumps env | Secret absent from child output/receipt | Critical |
| `term-output-flood` | Command emits >cap | Truncation; host remains responsive; residual 0 | Important |
| `term-cancel` | Cancel while running sleep/long | Cancelled receipt; root reaped; residual 0 or reported fail-closed | Critical if orphan owned product child |
| `term-forced-restart` | Kill/restart host with pending approval | Interrupted; never auto-run; residual handled | Critical |
| `term-two-projects` | Pending in A; switch to B | No A grant/exec in B UI; no leakage | Critical |
| `git-status-readonly` | Open status for project repo | Bounded DTO; no hooks executed (probe) | Critical if hook ran |
| `git-diff-injection` | File named like option / malicious attributes | Safe diff or reject; no external binary | Critical |
| `git-restore-drift` | Approve restore; mutate file before apply | CAS fail; bytes preserved | Critical |
| `git-mutate-reject` | Reject stage/commit proposal | Index/HEAD unchanged | Critical |
| `git-two-projects` | Status/diff isolation | No cross project paths | Critical |
| `git-forced-restart` | Pending mutate across restart | Interrupted; no commit | Critical |
| `privacy-surfaces` | Screenshots + logs for approve bars | No roots, grants, secrets, raw ids | Important |
| `process-residual-proof` | End of suite | `processCleanup=passed`; product residuals 0 | Critical if false clean |

### 7.2 Visual / interaction adversarial (desktop)

| ID | Expect |
| --- | --- |
| V1 | Single approval bar; no permanent card stack |
| V2 | Esc rejects when bar focused; Tab trap on actions |
| V3 | Conversation switch parks foreign proposals |
| V4 | Diff/status open only in collapsible context pane |
| V5 | Screenshots at 1440×960 and 1024×720 (C3 pattern) |
| V6 | No YOLO / Allow session controls for T1/G2 in first checkpoint |

### 7.3 Performance budgets (release gates, not diagnostics)

| Budget | Hard limit (first checkpoint) | Measurement |
| --- | --- | --- |
| One-shot output stored in SQLite | ≤ 262,144 bytes metadata/preview total class | Receipt size tests |
| Diff display | ≤ 262,144 bytes and ≤ 4,000 lines (align C3) | Diff builder tests |
| Command default timeout | Product constant (recommend ≤ 120s one-shot; tests inject shorter) | Timeout tests |
| Concurrent T1 per run | 1 non-terminal | Policy test |
| Concurrent G2 per repo | 1 non-terminal | Policy test |
| Desktop adversarial suite wall time | Recorded; flake < controller threshold | Harness timing |
| Residual proof poll budget | Match supervisor defaults; fail closed on exceed | Safety tests |

Breach of hard budget without truncation/fail-closed = **Important**.

---

## 8. Explicit Critical / Important rejection criteria

### 8.1 Critical — automatic C4 rejection

Reject C4 (any rung that is in scope for the gate) if **any** of the following
is true:

1. **Authority break:** provider or renderer can execute process or mutate Git
   without host seal + (for mutate/exec) human `approve_once`.
2. **Seal mismatch:** approved fingerprint fields differ from executed
   argv/cwd/env/git op/paths.
3. **Cross-scope:** proposal/grant/receipt from project A or conversation A
   affects project B or conversation B.
4. **Path/reparse escape:** cwd, git worktree, or path arg resolves outside
   project via `..`, junction, symlink, or nested repo escape.
5. **Credential leakage:** parent secrets appear in child env, SQLite, logs,
   screenshots, or public errors.
6. **Unsafe process kill:** product or harness uses bare PID / `taskkill` /
   tree kill that can target non-owned identity; or PID-reuse kill demonstrated.
7. **Restart auto-apply:** non-terminal terminal/git proposals execute or
   mutate after host restart without new human decision.
8. **Hooks/filters RCE on read or governed mutate:** adversarial repo fixture
   executes attacker code during status/diff/commit/restore.
9. **False residual clean:** acceptance record claims clean while product-owned
   children or supervised listeners remain, **or** cleanup "succeeds" by
   killing unbound processes.
10. **Destructive Git ops present:** reset-hard, clean -fd, force checkout,
    push, or arbitrary argv passthrough reachable from C4 UI/API.
11. **Provider permission path live for shell/git:** ACP allow path grants
    execute/mutate.
12. **Independent review or controller gate** finds Critical > 0.

### 8.2 Important — default reject for industrial Alpha

Reject unless controller documents residual + user override:

1. Output flood freezes UI or unbounded disk growth without cap.
2. Escape sequences not neutralized in T1 transcript UI.
3. Stale Git status presented as authoritative without epoch/oid.
4. Incomplete residual proof that leaves **owned** orphans without fail.
5. Session-scoped auto-allow introduced for execute/mutate.
6. Encoding faults silently interpreted as success text.
7. Performance hard budget exceeded without fail-closed behavior.
8. Privacy: portable path discipline broken (native roots in DOM).
9. Cancel leaves **owned** root alive past budget without failed cleanup.
10. Important > 0 from independent security review.

### 8.3 Non-rejection (correct fail-closed)

Do **not** reject solely because:

- cleanup reports residual unbound Node/VCTIP and refuses kill;
- incomplete identity blocked termination;
- base drift / CAS rejected a mutate;
- user reject/expiry left no disk or git change;
- G1 auto-read denied a credential-like path.

These are security successes when receipts and UX are truthful.

---

## 9. Reference implementation notes (evidence ≠ authority)

### 9.1 Grok App at `cbbbdf17…` — adopt / adapt / avoid

| Topic | Evidence | C4 security stance |
| --- | --- | --- |
| Permission policies incl. AlwaysApprove / AcceptEdits | `permission.rs` | **Avoid** as default; C3 already chose approve_once only |
| Session allow_cache by scope_key | shell key = executable basename only | **Avoid** for exec; basename `npm` is too coarse (TERM-02/X-09) |
| Download command auto-allow heuristics | curl/wget parsing | **Avoid** — shell parsing is brittle |
| Git argv without shell | `git_file_diff`, worktree ops | **Adopt concept** of argv-only |
| `--no-ext-diff` | `git_file_diff` | **Adopt**; extend with stronger filter isolation |
| Worktree sibling outside project | `git_worktree_add` | **Do not adopt** placement outside project root |
| Process pool limits | `process_limits.rs` | **Adopt concept** of hard concurrency caps |
| Lexical normalize for outside-project | `is_outside_project` | **Partial** — must add reparse refuse like Pillow B3 |
| Enriched PATH for GUI spawn | `process_util.rs` | **Adapt** carefully; still not full env inherit |

### 9.2 Pillow C3 — mandatory inheritance

| Invariant | Carry into C4 |
| --- | --- |
| Host seals payload; SQLite stores digests/metadata only | Yes for terminal/git mutate |
| One-shot grant; raw grant memory-only; digest durable | Yes |
| Restart interrupts non-terminal | Yes |
| Base CAS before mutate | Yes for restore/stage-sensitive ops |
| Portable paths; reparse reject | Yes for cwd and git paths |
| Two-project isolation E2E | Yes |
| Public opaque errors | Yes |
| Provider permission deny | Yes |
| Real desktop residual process proof | Yes |

### 9.3 Process incident — mandatory inheritance

Any C4 design that reintroduces bare-PID tree kill is a **Critical** design
defect regardless of feature urgency.

---

## 10. Recommended control architecture (security integration)

```text
C4-A  Protocol + policy vocabulary for proc.exec / git.read / git.mutate
C4-B  Durable proposal ledger (metadata) + grant/replay/expiry (extend C3)
C4-C  Terminal argv executor + env allow-list + output caps + cancel
C4-D  Process identity supervisor for product children (laws from §5)
C4-E  Git read path (fixed argv or library) + filter/hook probes
C4-F  Git mutate (optional) with CAS + hook isolation
C4-G  UI approval bar + contextual status/diff (no card stack)
C4-H  Desktop adversarial pack + residual proof + screenshots
C4-I  Independent security review; controller Critical/Important 0
```

Ordering constraint: **C4-D laws before shipping C4-C cancel**. **Hook/filter
probes before claiming G1 safe**. **No G2 without G1 and T1 residual proof
green**.

---

## 11. Explicit exclusions (Stage 1 / first C4 checkpoints)

The following are **out of scope** and are Critical if they appear without a
new design authorization:

- interactive unrestricted PTY as first checkpoint;
- shell-string execution (`cmd /c`, `powershell -Command`, `bash -lc`);
- session/YOLO/auto-approve for execute or Git mutate;
- `git reset --hard`, `clean`, force checkout, rebase, merge, cherry-pick,
  push, pull, fetch, submodule update, arbitrary `git` argv passthrough;
- network tools, MCP arbitrary process, credential file writes;
- Obsidian / company-ops features;
- Codex/OpenCode runtime expansion (Stage D);
- claiming "sandbox" stronger than actual Windows process isolation.

---

## 12. Acceptance gate checklist (controller)

C4 first checkpoint (T1 + G1 recommended) is accepted only when:

1. All unit packs U-P, U-T, U-G (G1 subset), U-U, U-S pass.
2. Real desktop scenarios in §7 for in-scope rungs pass with residual proof.
3. Process-safety suite remains green (41/41 or successor count) with no
   reintroduction of forbidden termination.
4. `pnpm check`, production UI build, `cargo fmt --check`, `git diff --check`,
   full serial desktop E2E pass.
5. Screenshots reviewed for privacy and conversation dominance.
6. Independent review Critical 0 / Important 0 (or Important residual
   explicitly user-accepted — default no).
7. Controller re-runs final gates and writes acceptance/checkpoint docs.
8. Provenance: reference commits recorded; no whole-app copy; licenses noted
   if any adaptation lands later.

---

## 13. Residual risks (research-time)

1. Sibling C4 architecture reports were missing at write time; executable argv
   allow-list and Git library-vs-CLI choice may shift controls — re-map this
   matrix when those reports land.
2. Full Git config/hook isolation on Windows is hard; pure library paths reduce
   but may not eliminate attribute filter residual — require probe fixtures.
3. Grandchild processes outside job objects remain a Windows residual; product
   must document "observation-only descendants" honestly.
4. T1 without ConPTY still leaves agents wanting interactive terminals (T2);
   do not backdoor shell strings to compensate.
5. Grok App session-cache coarseness and YOLO modes are attractive UX hazards
   for implementers; security gate must keep them default-off.
6. Performance numbers are starting budgets; live E2E may force tighter caps.
7. Dirty working tree / unstaged C3 checkpoint means C4 design must not assume
   released packaging or clean git history for provenance of product code.

---

## 14. Keeper signal

```text
topic: completion
body: DONE — C4 security threat model and acceptance matrix written to
docs/research/2026-08-02-c4-security-acceptance-research.md only.
Evidence: C3 design/acceptance, process-safety 41-test laws + forced-restart
taskkill/PID-reuse incident, Grok App cbbbdf17 permission/git/process
boundaries (evidence not authority), Pillow protocol/policy/receipts/cancel/
restart/E2E harness. Delivered severity model, capability threat tables
(TERM/GIT/X), sealed proposal shapes, unit/integration packs, real desktop
adversarial scenarios, residual-process proof, Critical/Important rejection
criteria. No product code, stage, or commit. Sibling C4 research files were
absent; residual alignment required after grok1–3 land.
```

---

## 15. Closeout

**Status: DONE**

**Changed files (truth):**

- Created: `docs/research/2026-08-02-c4-security-acceptance-research.md`

**No other files modified.** Staging not used. No product implementation.

**Residual risks:** see §13.

**Keeper signal:** see §14 (post to bridge messages on controller request).
