# Wave B2 Security And Acceptance Research

Date: 2026-07-30

Status: Research only (read-only analysis; no product code)

Owner: Keeper B2 security and acceptance designer

Worktree: `<repo>-worktrees\wave-b2-security`

## 0. Purpose And Non-Claims

This document defines the **minimum security, process, diagnostic, and native
acceptance gates** required before a real Grok ACP adapter can be called
**industrial** for Slice B2.

It is grounded in:

- product order and gates in `AGENTS.md`;
- Stage 1 design security, adapter contract, and Slice B scope in
  `docs/superpowers/specs/2026-07-29-pillow-first-design.md`;
- B1 conversation-kernel acceptance truth and process cleanup evidence in
  `docs/verification/2026-07-30-slice-b1-acceptance.md`;
- native process supervision patterns in `scripts/lib/desktop-process.mjs` and
  `scripts/conversation-recovery-desktop.mjs`;
- bounded host semantics in `crates/runtime-host/src/**`;
- read-only legacy reference behavior in
  `<workspace>/VaultSystem\keeper\lib\agent-runtime-adapters.js`,
  `grok-acp-client.js`, and `keeper-acp-tool-policy.js`.

### Explicit non-claims

- This report is **not** product implementation and does **not** modify product
  code.
- Slice B1 fake-stream evidence is **not** Grok connectivity evidence.
- B2 industrial readiness is **not** Stage 1 completion (Codex, full IDE
  resources, packaging, and Slice E remain later).
- Credentials, account tokens, machine paths, and live session data were **not**
  inspected for this research.
- Project trust is **not** a sandbox; the adapter must declare actual isolation
  guarantees and the UI must not claim more than the runtime provides.

### B2 product boundary (minimum)

Replace the fake adapter at the runtime-host boundary while preserving B1
architecture order:

```text
validate protocol payload / kind
  -> persist event and legal run status in SQLite
  -> commit (single IMMEDIATE transaction)
  -> emit renderer event
```

B2 minimum product behavior:

1. probe Grok Build installation and authentication health with truthful
   semantics;
2. start or resume an ACP session for a project-scoped conversation;
3. stream normalized provider-neutral events into the B1 conversation kernel;
4. enforce B2 **read-only** permission policy on ACP permission requests;
5. cancel with process-tree cleanup and zero residuals;
6. recover or mark interrupted after host/app restart;
7. export redacted diagnostics only.

B2 does **not** require write-capable tool approval UI, model picker, quota
billing UI, multi-provider routing, or Obsidian.

---

## 1. Trust Boundary And Threat Model

### 1.1 Trust zones

| Zone | Components | Trust posture |
| --- | --- | --- |
| **Z0 Locked kernel** | policy decisions, audit schema, migrations, release verification, secret reference IDs | Highest. No provider payload may rewrite policy or schema. |
| **Z1 Domain / host** | SQLite authority, `ConversationService`, `RuntimeHost`, protocol validation | Authoritative for conversation/run truth. Accepts only validated neutral events. |
| **Z2 Adapter** | `adapter-grok-acp` (to be introduced), ACP client, launch builder | Semi-trusted. Translates Grok ACP only. Must not own domain state. |
| **Z3 Child runtime** | Grok Build / Grok CLI process tree under isolated profile | Untrusted input source. May crash, hang, emit oversized or malicious JSON-RPC, request elevated tools, or retain local session files. |
| **Z4 Renderer** | React workbench, Tauri invoke/listen | Least privilege. No direct SQLite, process launch, or secret material. |
| **Z5 Diagnostics / acceptance** | logs, probe DTO, export bundles, E2E evidence JSON | Public-safe. Must be redacted and free of secrets/account labels. |

### 1.2 Authority order (non-negotiable)

1. **SQLite** is the only operational authority for conversation/run state.
2. **RuntimeHost** bounds concurrency, cancel, and protocol ingress.
3. **Adapter** owns only transport + process lifecycle + event normalization.
4. **Renderer** is a projection of committed domain events.
5. Never emit a renderer event that has not been durably committed (B1 Fix
   truth retained).

### 1.3 Assets

| Asset | Failure impact if compromised |
| --- | --- |
| User project files | Unauthorized read/write outside workspace or silent mutation under "read-only" B2 |
| OS credential store / secret refs | Token theft; account takeover |
| Isolated Grok profile (`HOME` / AppData under account home) | Cross-account session bleed |
| SQLite `keeper.db` | Conversation integrity loss; false run completion |
| ACP stdio channel | Prompt injection into host; tool permission escalation |
| Diagnostic bundles and acceptance evidence | Secret leakage into repos, CI logs, support packages |
| Desktop process residual | Orphan Grok children retaining locks/network; poisoned baselines |

### 1.4 Adversaries and abuse cases

| ID | Threat | Primary mitigation |
| --- | --- | --- |
| T1 | Provider process escapes project cwd via path traversal | Resolve real paths; reject `..` and UNC; require project under trusted workspace root |
| T2 | Parent process env leaks `API_KEY` / tokens into child | Environment allow-list + sensitive-name denylist; never pass full `process.env` |
| T3 | Cross-account profile reuse (`USERPROFILE`/`APPDATA` shared) | Per-account isolated process home; override `HOME`/`USERPROFILE`/`APPDATA`/`LOCALAPPDATA` |
| T4 | ACP permission request auto-approved for write/execute | B2 policy: allow only recognized read/search once; deny all other kinds |
| T5 | Oversized stdout line / unbounded response growth | Line, payload, response, queue, and timeout hard limits; fail closed |
| T6 | Cancel leaves orphan process tree | `taskkill /T /F` (or equivalent kill-tree) + CIM/snapshot proof of death |
| T7 | Secrets enter SQLite / renderer / logs | Forbidden protocol keys; redaction on persist/export; opaque secret IDs only |
| T8 | Probe reports healthy while unauthenticated | Auth-health truth table; `healthy=false` when install or auth fails |
| T9 | Fake/fixture path cited as live connectivity | Explicit adapter name; live native gate mandatory for B2 PASS |
| T10 | Executable/args override from untrusted options | Launch builder rejects caller-supplied executable/args |
| T11 | Malformed JSON-RPC / dual terminals / foreign run IDs | ProtocolGuard + validate_payload; single terminal; run_id match |
| T12 | Diagnostic export includes session cookies or account labels | Redaction rules + negative tests with synthetic blocked values |
| T13 | Hook/capability tokens leak into public run surfaces | Loopback-only grant validation; capability never stored in renderer DTO |
| T14 | Shutdown races leave Pending runs or residual slots | B1 admission linearization retained; B2 must join child tree before idle |

### 1.5 Security surface classes (design §8)

| Surface | B2 rule |
| --- | --- |
| Locked | Policy engine inputs, audit schema, migration history, release verification |
| Editable by Agent | **None in B2.** B2 is read-only tool scope; no project file mutation via ACP tools |
| Append-only | Run events, tool receipts (public activity), audit records |
| Human controlled | Credentials (OS keychain), enabling the runtime, destructive future modes |

---

## 2. Environment Allow-List And Profile Isolation

### 2.1 Environment allow-list (minimum)

Port the legacy launch invariant: **build a new env map; do not inherit the
parent wholesale.**

**Allowed base names** (case-normalized match; values copied only if non-null
and not sensitive):

| Name | Purpose |
| --- | --- |
| `SYSTEMROOT` | Windows system root |
| `WINDIR` | Windows directory |
| `COMSPEC` | Command processor |
| `PATHEXT` | Executable extensions |
| `PATH` | Locate Grok binary deps only after allow-list filter |
| `TEMP` / `TMP` | Temporary files for the child |
| `USERPROFILE` / `HOME` | **Only when overridden** to the isolated account process home |
| `APPDATA` / `LOCALAPPDATA` | **Only when overridden** to directories under that process home |

**Sensitive name denylist** (substring match, case-insensitive): any env name
matching `KEY|TOKEN|SECRET|PASSWORD|CREDENTIAL|AUTH|OPENAI|ANTHROPIC|XAI|VOLC|MINIMAX|MIMO`
must **never** be copied from the parent, even if the name also appears on the
allow-list (defense in depth; `PATH` is the main allow-list member that must
still be free of secret-shaped values).

**Forbidden inheritance examples:**

- `XAI_API_KEY`, `OPENAI_API_KEY`, `ANTHROPIC_API_KEY`, bearer tokens;
- provider session cookies or auth headers;
- machine-global Grok profile paths pointing at the interactive user profile;
- caller-supplied arbitrary env maps that reintroduce secrets.

### 2.2 Profile isolation requirements

A leased Grok account slot maps to an **isolated process home**:

1. Account configuration references a dedicated home directory (or `.grok`
   config dir whose **parent** is the process home). Paths must exist, be local
   (no UNC), and not contain `..` segments.
2. Child env sets:
   - `HOME` = isolated process home;
   - `USERPROFILE` = isolated process home;
   - `APPDATA` = `<processHome>/AppData/Roaming` when present;
   - `LOCALAPPDATA` = `<processHome>/AppData/Local` when present.
3. Working directory (`cwd` / ACP session cwd) is the **project workspace path**,
   resolved via realpath, required to be a directory, and **strictly below** the
   trusted project/workspace root.
4. Executable path is config-owned only (`requireFile` equivalent). Callers may
   not override `executable` or `args`.
5. ACP launch shape (legacy reference):
   `agent --no-leader stdio` under project cwd with isolated env — no prompt
   packet secrets on argv.

### 2.3 Optional hook grants (if retained)

If B2 keeps a local hook bridge:

- endpoint must match loopback only:
  `http://127.0.0.1|localhost:<port>/…` with an explicit trusted path;
- capability token must be a fixed-width high-entropy secret (legacy: 64 hex);
- capability is blocked-value material: never written to SQLite message bodies,
  renderer DTOs, probe detail, or diagnostic bundles;
- prefer binding by opaque run id, not embedding secrets in prompts.

B2 may omit hooks entirely if Grok ACP stdio alone satisfies streaming; if
omitted, document the omission in acceptance as intentional scope reduction.

### 2.4 Concurrency and session isolation

| Limit | B2 minimum |
| --- | --- |
| Host max concurrent turns | Default **4**, hard cap **32** (existing `HostConfig`) |
| One active ACP prompt per client connection | Reject concurrent prompt on same client (`acp_busy` class) |
| Project scoping | Every session/run/conversation remains project-scoped |
| Account slot | One leased slot per live child; no silent fallback to another account |

---

## 3. Secret, Account, And Session Redaction Rules

### 3.1 Absolute bans

The following must **never** appear in:

- SQLite operational columns (messages, events, run metadata, probe cache);
- renderer state / Tauri event payloads;
- structured logs at default levels;
- diagnostic export bundles;
- acceptance evidence JSON committed or printed to CI;

…except as **opaque identifiers** (UUID secret ref, account slot id without
label secrets, run id):

| Class | Examples |
| --- | --- |
| API material | API keys, bearer tokens, OAuth access/refresh tokens, hook capabilities |
| Account material | Account labels used as blocked values, email identities, raw home paths in public text |
| Session material | Provider session cookies, ACP private session files contents, raw auth.json |
| Env material | Full parent env dumps, child env maps containing secrets |
| Provider raw | Unvalidated provider payloads with credential field names |

### 3.2 Protocol boundary forbidden keys

Retain and enforce `agent-protocol` forbidden top-level payload keys
(case-insensitive), including at least:

`api_key`, `apikey`, `authorization`, `auth_token`, `access_token`,
`refresh_token`, `password`, `secret`, `bearer`, `provider`, `provider_id`,
`provider_payload`, `openai`, `anthropic`, `grok_api`, `acp_session`,
`raw_provider`.

Unknown fields on typed payloads remain rejected (`deny_unknown_fields`).

### 3.3 Text safety (public surfaces)

Port legacy `agent-run-safety` intent without wholesale copy:

- strip or reject credential-shaped assignment contexts;
- reject shaped secrets (`sk-…`, `xai-…`, `bearer …`, auth.json mentions in
  public fields);
- reject absolute path-like strings in **public** summaries where relative
  project paths are required;
- maintain a **blockedValues** set from account labels and any issued
  capabilities for the run;
- cap public text lengths (legacy packet text ≤ 500; tool activity summaries
  short and kind/phase based).

### 3.4 Where secrets may live

| Location | Allowed content |
| --- | --- |
| OS credential store | Secret material, referenced by opaque ID |
| Adapter-private memory (non-logged) | Short-lived launch env for child spawn only |
| Isolated profile on disk | Grok's own session files under isolated home (not Keeper SQLite) |
| SQLite | Opaque secret **references** only, never values |

### 3.5 Diagnostic export contract

`export_diagnostics` (adapter contract) must return a structured bundle that:

1. includes adapter name, version, probe health booleans, recent error **codes**;
2. includes process residual counts (should be zero) and last cancel outcome;
3. redacts stdout/stderr samples through the same text safety filters;
4. fails closed: if redaction cannot prove safety, omit the field rather than
   export raw text;
5. never includes child env maps, account home paths, or keychain material.

---

## 4. Child Process Supervision And Kill-Tree Requirements

### 4.1 Lifecycle ownership

```text
RuntimeHost.spawn_turn / send_turn
  -> adapter arms run
  -> adapter spawns Grok ACP child (if not already session-owned)
  -> stdio JSON-RPC client bound to that child
  -> cancel_turn OR terminal event OR shutdown
  -> kill-tree + join + disarm
  -> host slot/permit release
  -> ConversationService pump reaches durable terminal
```

The **adapter** owns the OS process tree. The **host** owns concurrency slots
and cancel publication. The **service** owns durable run status.

### 4.2 Kill-tree minimum (Windows first)

Port legacy runner + desktop acceptance patterns:

1. On cancel, timeout, protocol fatal, or shutdown: terminate the **entire**
   process tree rooted at the ACP child PID.
2. Windows primary tool: `taskkill.exe /PID <pid> /T /F` with `windowsHide`,
   no shell.
3. Kill-tree attempt is **bounded** (wall timeout); on failure, escalate to
   residual tracking and fail the shutdown/cancel path rather than lying about
   success.
4. After kill, **do not** trust `kill(pid, 0)` alone. Require a fresh process
   snapshot (CIM `Win32_Process` or equivalent) showing:
   - no tracked descendant PIDs alive;
   - no unexpected Grok CLI / keeper residual owned by the run (define name
     match allow-list for the supervised binary);
   - for native desktop acceptance: no leftover `pillow.exe` outside
     baseline and no inherited port listeners (existing `ProcessSupervisor`).
5. stdin end + stdout detach occur even when kill fails, so the ACP client
   settles pending RPCs with a closed-connection error.

### 4.3 Supervision rules

| Rule | Requirement |
| --- | --- |
| Track roots | Record child PID at spawn; periodically expand descendants |
| No detach without owner | Never spawn with detached process group that host cannot kill |
| Cancel visibility | Host cancel flag + adapter cancel RPC + kill-tree; any one path must not leave a live tree after bounded wait |
| Shutdown order | Close admission → cancel workers → wait admitted sync turns → adapter.shutdown (kills children) → join workers → zero slots |
| Drop safety | Dropping a spawned turn handle must still cancel/kill (best-effort) even if host is already shut down |
| Acceptance proof | Graceful exit claims require shutdown-success marker + exit status 0 + clean snapshot (B1 pattern retained for B2 live path) |

### 4.4 Residual definition (B2)

A B2 turn is residual-free only when **all** hold:

- host `in_flight_turns == 0` and worker registry empty for that run;
- service `active_run_count == 0` after durable terminal;
- ACP child PID and descendants not alive;
- no open stdio handles held by the adapter for that session/run;
- SQLite run status is a legal terminal (`Completed` / `Failed` /
  `Cancelled` / `Interrupted`) consistent with the last event kind.

---

## 5. Bounded Queue, Line, Payload, Response, And Timeout Limits

Hard limits exist to force fail-closed behavior. Values below are the **B2
minimum industrial defaults** derived from B1 host constants and legacy ACP
client constants; implementation may tighten but must not silently remove.

| Bound | Default | On breach |
| --- | --- | --- |
| Per-turn event queue capacity | **64** (`DEFAULT_EVENT_QUEUE_CAPACITY`) | Backpressure (`push_blocking`) or `EventQueueFull`; cancel may still commit `run_cancelled` |
| Host max concurrent turns | **4** (cap **32**) | `try_acquire` rejects new turns |
| ACP request timeout | **180_000 ms** (3 minutes) per JSON-RPC request | Reject with `acp_timeout`; cancel session prompt; kill-tree if process unresponsive |
| ACP max line bytes | **1_048_576** (1 MiB) cumulative buffer / line | Close connection `acp_output_too_large`; fail turn; kill-tree |
| ACP max response length | **12_000** chars of concatenated agent text | `acp_response_too_large`; cancel prompt |
| Max tool activity entries (public) | **24** | Mark overflow; cancel if overflow treated as abuse (legacy) |
| Tool call id length | **≤ 200**, no control chars | Ignore update |
| Prompt packet / public text caps | **≤ 500** for packet-like fields | Truncate or replace with safe fallback |
| Protocol payload size | Implementation-defined but finite; reject multi-MB tool dumps at adapter | `InvalidEvent` / turn failed |
| Pop / pump idle timeout | Existing host pop timeouts retained | Stall detection → interrupt path |
| Service shutdown timeout | **15 s** public shutdown (B1) | `shutdown_incomplete` / prevent app exit |
| Native acceptance phase timeout | **60 s** readiness / evidence / marker / exit waits (B1 scripts) | Fail acceptance |
| Desktop cleanup poll | ~50 ms poll, multi-attempt kill until snapshot clean | Fail cleanup with residual PIDs listed |

### 5.1 Queue and cancel arbitration

Retain B1 delivery semantics:

- after host cancel, only `run_cancelled` may commit;
- a blocked `turn_completed` must not win after cancel;
- closed delivery rejects non-cancel producers;
- protocol guard enforces monotonic sequence, expected `run_id`, single
  terminal, and `finish()` requiring exactly one terminal.

### 5.2 Stall and heartbeat

Design requires adapter heartbeat and stall detection. B2 minimum:

- emit or map periodic health so the host can observe liveness;
- if no protocol progress within a configured stall window while a prompt is
  active, cancel + kill-tree + durable `Interrupted` or `Failed` with stable
  error code (not a silent hang).

---

## 6. Permission-Request Behavior For B2 Read-Only Scope

### 6.1 Scope statement

Slice B2 industrial gate is **read-only tool execution** for Grok ACP:

- recognized **read** and **search** tools may be allowed **once** when the
  agent presents an `allow_once` option;
- all write, delete, move, execute, fetch, network, credential, and unknown
  tools are **denied**;
- denied write-class kinds may set `proposalRequired` for future slices but
  must **not** execute in B2;
- Ask remains the product default; B2 does not introduce always-allow project
  policies for write tools.

This matches design §3.4 (read-only may be allowed by policy; write/process
classified separately) and legacy `keeper-acp-tool-policy.js` behavior.

### 6.2 Decision algorithm (normative for B2)

On ACP `session/request_permission`:

1. If the active prompt is cancelled → respond cancelled; do not allow tools.
2. Resolve `toolCallId` against the session-local tool registry.
3. If call is **same session**, kind is **recognized**, kind ∈ `{read, search}`,
   and options include `allow_once` → select `allow_once`; append public
   activity `allowed`.
4. Otherwise → select `reject_once` / `reject_always` if present, else
   cancelled outcome; append public activity `denied`.
5. If denied kind ∈ proposal set `{edit, delete, move, execute, fetch, other}`
   and kind was recognized → `proposalRequired = true` (metadata only).
6. Never auto-select `allow_always` in B2.
7. Public activity maps private write kinds to public `write` and **must not**
   show write as `allowed`/`succeeded` in public projections.

### 6.3 Client capability declaration

ACP `initialize` client capabilities for B2 must not advertise host-side file
write or terminal control that Keeper does not implement:

- `fs.readTextFile: false`, `fs.writeTextFile: false` (or omit until Slice C
  hosts FS tools under policy);
- `terminal: false` until integrated terminal is a governed IDE resource.

Grok may still request tools via ACP; Keeper policy answers those requests.
The UI must not claim FS/terminal mediation that the host does not provide.

### 6.4 Approval-path tests (gate)

Even though B2 is read-only, **permission-path tests are mandatory**:

- unit: read/search allow once; edit/execute deny; cancelled prompt denies;
- unit: unknown kind denied; foreign sessionId denied;
- integration: permission request during live/fixture stream does not mutate
  workspace files;
- negative: injected `allow_always` option is ignored for write kinds.

Write-capable tool **execution** remains a later slice and requires explicit
approval-path tests per `AGENTS.md` before enablement.

---

## 7. Probe And Auth-Health Truth Semantics

### 7.1 Probe DTO (protocol)

```text
RuntimeProbe {
  healthy: bool,
  adapter_name: string,   // e.g. "grok-acp" — never "fake" for live path
  version: string,        // adapter or detected CLI version
  detail: option<string>  // human-readable; never credentials
}
```

### 7.2 Truth table

| Install present | Executable valid | Auth / account usable | `healthy` | `detail` class (examples) |
| --- | --- | --- | --- | --- |
| No | — | — | **false** | `not_installed` |
| Yes | No / path missing | — | **false** | `executable_missing` |
| Yes | Yes | No / expired / no lease | **false** | `unauthenticated` / `account_unavailable` |
| Yes | Yes | Yes | **true** | optional version/build note |
| Adapter shut down | — | — | error `ShutDown` | not a healthy probe |

Rules:

1. **`healthy` is conjunctional.** Installation alone is not health.
2. **No credential material in `detail`.** Use stable codes and short safe
   phrases only.
3. **Fake adapter probe is not B2 evidence.** Live native acceptance must
   exercise `adapter_name == "grok-acp"` (or the final chosen industrial name)
   against a real binary path configuration **without** printing secrets.
4. Probe must be callable without starting a billable turn when possible;
   if auth check requires a subprocess, it must still obey env isolation and
   kill-tree cleanup.
5. UI may show degraded connection state from probe; it must not invent
   "connected" when `healthy == false`.

### 7.3 Auth without secret inspection

Acceptance and probes may assert:

- configuration references an opaque account/slot id;
- isolated home directory **exists** and is not the interactive user profile
  when multi-account isolation is enabled;
- child env construction unit tests use synthetic env maps;

They must **not** open keychain items, print tokens, or read live auth.json
contents into logs.

---

## 8. Acceptance Test Matrix

B2 industrial PASS requires evidence at **every** layer below. Fake-only green
CI is insufficient for B2.

### 8.1 Protocol fixtures

| ID | Fixture intent | Pass criteria |
| --- | --- | --- |
| F1 | Canonical happy-path ACP-normalized event stream → neutral JSONL | Deterministic sequences; single terminal; no forbidden keys |
| F2 | Permission allow read + deny write interleaved | Public tool activity correct; no write success |
| F3 | Oversized line / malformed JSON | Adapter fails closed with stable error codes |
| F4 | Cancel mid-stream | Exactly one terminal `run_cancelled` (or mapped cancel) |
| F5 | Dual terminal attempt | Second terminal rejected; first terminal stands |
| F6 | Foreign run_id / session mismatch | Rejected at guard; no SQLite pollution |

Fixtures live under tests (e.g. `tests/fixtures/protocol/…`) and are replayed
by unit/integration without network when possible. A **separate** live corpus
may record redacted shapes only.

### 8.2 Unit tests

| Area | Required cases |
| --- | --- |
| Env builder | Allow-list copy; sensitive names dropped; overrides for HOME/APPDATA; no full env clone |
| Path validation | Reject `..`, UNC, non-directories, project outside root |
| Launch builder | Reject executable/args overrides; ACP argv shape stable |
| ACP client limits | Timeout, max line, max response, tool activity overflow |
| Tool policy | Read/search allow once; write/execute deny; cancel short-circuit |
| Redaction | Forbidden keys; blockedValues; diagnostic export omits secrets |
| Probe truth | Matrix in §7.2 with synthetic states |
| Protocol validate | Existing payload validation retained + provider field rejection |
| Host integration seams | Cancel vs completed race; queue full; shutdown residual zero |

### 8.3 Integration tests

| ID | Scenario | Pass criteria |
| --- | --- | --- |
| I1 | Host + grok adapter with **fake child** (stdio script) | Full validate→persist→emit path; no real network |
| I2 | Cancel kills fake child tree | PID dead; slots zero; durable Cancelled |
| I3 | Child exit mid-turn | Durable Failed/Interrupted; no residual workers |
| I4 | Permission deny does not write workspace files | Temp project tree hash unchanged |
| I5 | Restart recovery | Inflight marked Interrupted; timeline restore (B1 semantics) |
| I6 | Concurrent turn cap | Fifth turn rejected or queued per policy; no slot leak |
| I7 | Diagnostic export | Bundle parses; secret canaries absent |

### 8.4 Live native acceptance

Runs on Windows with a real Grok Build/CLI install **only in controlled
environments**. CI without the binary reports **SKIP** (not PASS).

| ID | Scenario | Pass criteria |
| --- | --- | --- |
| N1 | Probe healthy when install+auth available | `healthy=true`, adapter name industrial, detail safe |
| N2 | Probe unhealthy when binary removed / misconfigured | `healthy=false`, safe detail code |
| N3 | Start session + one read-only turn | Stream commits to SQLite; UI timeline restores after restart |
| N4 | Cancel live turn | Kill-tree complete; residual process snapshot clean; one terminal |
| N5 | App graceful exit during/after live turn | Shutdown marker + exit 0 + no `pillow.exe` residual + port clean |
| N6 | Isolated profile | Child cannot see interactive user Grok profile (prove via env unit + optional live check without dumping secrets) |

Live scripts should extend the B1 `ProcessSupervisor` model: baseline clean,
track PIDs, cleanup until snapshot clean, never claim graceful exit from forced
`taskkill` alone.

### 8.5 Negative acceptance tests

| ID | Injection | Expected |
| --- | --- | --- |
| X1 | Parent env contains `XAI_API_KEY=…` | Child env map excludes it |
| X2 | Permission request for `execute` / `edit` | Denied; workspace unchanged |
| X3 | Malformed JSON-RPC line | Connection closed; turn failed; process cleaned |
| X4 | Response exceeds 12k chars | Cancel; error code; no partial false Completed |
| X5 | Forbidden payload key from adapter bug | Rejected at boundary; no emit |
| X6 | Secret canary string in tool output sample | Redacted/omitted from SQLite public fields and diagnostics |
| X7 | Project path outside workspace root | Launch refused before spawn |
| X8 | Caller passes executable override | Launch refused |
| X9 | Kill-tree failure simulated | Cancel/shutdown returns failure; no idle lie |
| X10 | Probe with missing auth | `healthy=false`; UI not "connected" |
| X11 | Cite fake adapter as B2 | Acceptance harness rejects `adapter_name=fake` for live gate |
| X12 | Double terminal / sequence hole | Guard rejects; durable state consistent |

---

## 9. Rollback And Failure-Injection Matrix

### 9.1 Rollback postures

| Trigger | User-visible state | Data posture | Process posture |
| --- | --- | --- | --- |
| Feature flag / config disable Grok ACP | Fake or "runtime unavailable"; no live spawn | SQLite retained; no migration rollback required | No child processes |
| Adapter init failure | Probe unhealthy; send_turn refused | No partial session rows without terminal handling | No orphan child |
| Mid-turn protocol fatal | Run `Failed` or `Interrupted` | Events committed up to failure; no renderer-only ghosts | Kill-tree |
| Cancel | Run `Cancelled` | Single cancel terminal | Kill-tree |
| App crash | On restart, inflight → `Interrupted` (B1) | Atomic event/status retained | Supervisors clean residuals on next acceptance/run |
| Failed app exit drain | Exit prevented (B1 ExitRequested) | Domain consistent | Operator must resolve residual before forced kill |
| Bad release | Revert to previous app version | Migrations must remain backward-readable or blocked by version gate | — |

B2 must not require destructive DB rollback for adapter disable. Disabling the
adapter is a **routing/config** rollback, not a schema rollback.

### 9.2 Failure-injection matrix (engineering)

| Injection point | Mechanism | Expected durable outcome | Residual |
| --- | --- | --- | --- |
| Spawn fails | Invalid executable path | Turn not admitted or Failed; no active entry | Zero |
| stdin write fails | Broken pipe | ACP closed error; terminal Failed | Kill-tree / already dead |
| stdout flood | Test double emits >1 MiB line | `acp_output_too_large`; Failed | Zero |
| RPC timeout | Clock / hung fake child | `acp_timeout`; cancel+kill | Zero |
| Permission flood | Many tool requests | Activity overflow handling; optional cancel | Zero |
| Cancel vs completed race | Barrier tests (B1 style) | Exactly one terminal | Zero slots |
| Emitter failure | Non-authoritative emit error | SQLite still reaches real terminal (B1 Fix 5) | Zero |
| Authoritative process failure | Forced process error | `Interrupted`; no invented terminal event | Zero or fail-closed drain error |
| Kill-tree fails | Stub taskkill error | Shutdown/cancel **errors**; not success | Reported residual PIDs |
| Auth revoked mid-session | Synthetic probe/session error | Healthy false / turn Failed | Zero |
| Queue full | Capacity 1 + slow consumer | Backpressure or queue full error; cancel wins | Zero after cancel |
| Shutdown during arm | Race with `ShutDown` | No residual Starting records | Zero |

Every injection that claims "cleaned" must assert **host idle + service active
zero + process snapshot clean** (for real PIDs).

---

## 10. Conditions That Force HOLD

B2 is **HOLD** (cannot be declared industrial PASS) if **any** of the following
are true:

### 10.1 Security HOLD

1. Child process inherits unrestricted parent environment or known secret-bearing
   variables.
2. Profile isolation is missing: child uses the interactive user
   `USERPROFILE`/`APPDATA` when multi-account isolation is required.
3. Write/execute/fetch tools can run without an explicit later-slice approval
   design (B2 must deny).
4. Secrets, tokens, capabilities, or raw auth material appear in SQLite,
   renderer events, default logs, or diagnostic exports.
5. Probe can report `healthy=true` while install or auth is known-failed.
6. Executable/args can be overridden by untrusted callers.
7. Project cwd can resolve outside the trusted workspace root.

### 10.2 Process HOLD

8. Cancel or shutdown leaves live Grok/descendant processes under residual
   checks.
9. Kill-tree is best-effort without bounded failure signaling (success lied).
10. Host or service reports idle while slots, workers, or active pumps remain.
11. Native acceptance claims `gracefulExit` without shutdown marker + exit 0 +
    clean snapshot.

### 10.3 Truth / scope HOLD

12. Acceptance evidence uses only the fake adapter or fixture replay to claim
    Grok connectivity.
13. UI claims sandbox/FS/terminal mediation the runtime does not provide.
14. Architecture order is violated (emit before commit; status without event;
    conflicting-kind replay accepted).
15. Required gates missing: typecheck, lint, unit, integration, protocol
    fixtures, clippy `-D warnings`, desktop smoke budgets, process cleanup
    evidence, committed verification report.
16. Live native gate is marked PASS on a machine without exercising the real
    binary (SKIP must not be relabeled PASS).
17. Unresolved Critical/Important review findings on security, residual
    processes, or redaction.

### 10.4 Explicit non-HOLD (out of scope for B2 industrial bar)

The following do **not** by themselves force B2 HOLD (they remain later
gates):

- absence of Codex/OpenCode adapters;
- absence of full IDE file tree/editor/terminal product surfaces (Slice C);
- absence of write-capable agent edits with human approval UX;
- absence of packaging/signing/updater (Slice E);
- performance budgets beyond existing smoke, unless B2 introduces a regression
  beyond Stage 1 budgets;
- multi-provider routing or model marketplace UI.

---

## 11. Industrial B2 PASS Checklist (Summary)

B2 may be called industrial only when **all** are true:

1. Trust boundaries documented and enforced in code paths (host/adapter/child).
2. Env allow-list + sensitive denylist + isolated profile env overrides.
3. Secret redaction rules enforced at protocol, persistence, log, and export.
4. Kill-tree cancel/shutdown with snapshot-proven residuals at zero.
5. All numeric bounds in §5 enforced with fail-closed tests.
6. Permission policy implements §6 read-only scope with tests.
7. Probe auth-health implements §7 truth table.
8. Fixture, unit, integration, negative, and (when available) live native gates
   in §8 are green or correctly SKIP.
9. Failure-injection matrix §9 covered for process and protocol fatals.
10. No HOLD condition in §10 applies.
11. Verification report committed with residual risks, non-claims, and SHAs.
12. Independent review has no open Critical/Important security or truth findings.

---

## 12. Source Mapping (Traceability)

| Requirement area | Primary sources |
| --- | --- |
| Product order / gates | `AGENTS.md` |
| Permissions, secrets, adapter contract, Slice B | `docs/superpowers/specs/2026-07-29-pillow-first-design.md` |
| B1 truth, shutdown, residuals, B2 non-claim | `docs/verification/2026-07-30-slice-b1-acceptance.md` |
| Process snapshot / kill / graceful proof | `scripts/lib/desktop-process.mjs`, `scripts/conversation-recovery-desktop.mjs` |
| Queue, concurrency, cancel, protocol guard | `crates/runtime-host/src/**` |
| Forbidden payload keys, probe DTO | `crates/agent-protocol/src/validate.rs`, `types.rs` |
| Env allow-list, isolation, ACP launch | legacy `agent-runtime-adapters.js` |
| ACP limits, permission RPC | legacy `grok-acp-client.js` |
| Read-only tool policy | legacy `keeper-acp-tool-policy.js` |
| Kill-tree | legacy `agent-runner.js` + desktop supervisor |
| Redaction intent | legacy `agent-run-safety.js` (port intent, not wholesale copy) |
| Roadmap B2 bullets | `docs/roadmap/2026-07-30-stage1-plan-comparison.md` |

---

## 13. Residual Research Risks

1. Exact Grok Build binary name/version matrix on Windows may require a thin
   probe implementation spike before N1/N2 can be automated in CI.
2. Whether Grok ACP session resume is available depends on agent capabilities
   returned at `initialize`; B2 must degrade truthfully when `loadSession` /
   resume is false.
3. Hook grants may be unnecessary for pure stdio ACP; retaining them expands
   the secret surface and should be an explicit design choice.
4. Live native tests need a controlled secret store setup that never prints
   material — operators must use opaque refs only.
5. Slice C terminal/FS host tools will reopen permission scope; B2 policy must
   not silently carry forward as always-allow when those land.

---

## 14. Document Control

| Field | Value |
| --- | --- |
| Deliverable path | `docs/research/2026-07-30-grok-security-acceptance.md` |
| Analysis mode | Read-only |
| Product code changes | None |
| Credential inspection | None |
| Next implementation consumer | Slice B2 Grok ACP adapter plan / implementation waves |
