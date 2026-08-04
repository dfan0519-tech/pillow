# Pillow C4 Governed Process and Git Design

Date: 2026-08-02

Status: Proposed design for user review; implementation not authorized

Owner: Codex Desktop controller

## 1. Decision

C4 is delivered as a capability ladder. The first checkpoint, **C4.0**, adds:

1. `process.run@1`: one conversation-owned, project-scoped, approve-once
   process execution using a sealed structured argv specification;
2. read-only Git repository discovery, identity, status, branch/worktree
   observation and bounded diff through host-fixed Git command templates.

C4.0 does **not** add a shell, an interactive terminal, Git mutation, network
tools, credentials, another runtime, Obsidian or company operations.

After C4.0 is independently accepted:

- **C4.1** receives a separate design for user-origin interactive ConPTY.
- **C4.2** receives a separate design for governed Git stage, unstage, commit,
  safe branch switch and single-file restore.

Neither later rung may be implemented by adding flags to C4.0 interfaces.

## 2. Product experience

The conversation remains the visual centre.

- An Agent-proposed command appears as one compact activity row in its owning
  conversation.
- A pending command uses the existing single approval-bar position above the
  composer: **Approve once**, **Reject**, and **Review**.
- Review opens the existing contextual pane. Live bounded stdout/stderr uses a
  `Terminal output` mode; it is not an interactive shell.
- Completed commands collapse to a one-line summary with origin, status, safe
  program label, exit code and duration.
- Git is a `Changes` mode of the same contextual resource pane. It shows the
  selected repository, branch/HEAD identity, bounded status and an on-demand
  host-built diff.
- At 1440x960 the contextual pane remains on the right. At 1024x720 command
  output may use a collapsible stage-bottom strip capped at 40% of stage
  height so that the conversation and composer stay dominant.

Pillow does not add a global terminal dashboard, a permanent task table, a
successful-command card stack or a Git worktree manager as primary chrome.

## 3. Capability ladder

| Rung | Capability | Approval | State |
| --- | --- | --- | --- |
| T0 | No process execution | n/a | Accepted C3 state |
| T1 | Structured one-shot argv execution | Approve once per sealed proposal | C4.0 |
| T2 | Interactive ConPTY | Separate design | C4.1 only |
| G0 | Read-only Git discovery/status/diff/identity | No approval; bounded project read | C4.0 |
| G1 | Stage/unstage/commit/switch/restore | Separate sealed approval per semantic mutation | C4.2 only |
| G2 | Reset/clean/force/history integration/network/arbitrary argv | Forbidden | Not planned |

C4.0 is accepted only when T1 and G0 are both green. T2 or G1 cannot begin
until that checkpoint is accepted.

## 4. Authority invariants

1. Every proposal, process and Git observation belongs to one project. A
   provider, renderer or reference project never supplies the native root.
2. Every Agent-proposed command belongs to one conversation, run and tool call.
3. The provider may propose `program`, `argv` and portable relative `cwd`; it
   cannot approve, select an executable path, provide environment variables or
   widen the tool catalog.
4. The host canonicalizes and seals the effective command. The approval UI
   displays that sealed command; execution consumes the same fingerprint.
5. Only `approve_once` and `reject` exist. Grants are opaque, memory-only,
   single-use and expire after 600 seconds.
6. One non-terminal process proposal is allowed per run. Replaying an identical
   terminal call returns its receipt; the same identity with a different
   fingerprint fails closed.
7. Renderer decisions cannot mutate argv, cwd, environment, Git arguments or
   project identity.
8. Restart invalidates pending, approved and running authority as
   `interrupted`; nothing auto-resumes or auto-runs.
9. Project or conversation switching changes projection only. It cannot move
   ownership, replay approval or silently cancel another owner's process.
10. Project trust is not described as an OS sandbox.

## 5. C4.0 process contract

### 5.1 Wire proposal

```json
{
  "toolId": "process.run",
  "version": 1,
  "program": "cargo",
  "argv": ["test", "-p", "keeper-tool-executor"],
  "cwd": "."
}
```

Rules:

- `program` is a host tool-registry label, not a path.
- `argv` contains literal arguments. Pillow never joins or re-parses it as a
  shell string.
- `cwd` follows the existing portable project-relative path grammar.
- Unknown fields, empty program, NUL, controls, absolute paths, UNC paths,
  verbatim paths, colon paths, backslashes, dot traversal, Windows device
  names and trailing-dot/space components fail closed.
- Limits: at most 64 argv entries and 32 KiB combined program/argv bytes.

### 5.2 Initial tool registry

C4.0 resolves only controller-approved labels to absolute executables found
under filtered host-configured toolchain roots. The initial catalog labels are:

- `cargo`
- `rustc`
- `node`
- `python`
- `python3`

The executor probes those labels on each host against the fixed root policy.
A missing label is an opaque unavailable-tool error, not a PATH fallback.

Explicitly denied in `process.run@1`:

- `git` — Git uses the dedicated semantic Git executor;
- `cmd`, `powershell`, `pwsh`, `bash`, `wscript`, `cscript`, `mshta`;
- `.cmd`, `.bat`, `.ps1`, `.vbs` and other script-host shims;
- arbitrary absolute or project-local executables;
- package-manager shell shims such as `npm.cmd` or `pnpm.cmd`.

Support for a denied class requires a new design. C4.0 does not smuggle it in
through a hidden wrapper program. Arguments such as `node -e` or `python -c`
remain visible literal argv and therefore represent arbitrary code execution;
the approval UI must classify them as high risk and state that project-path
containment is not an OS sandbox. The policy uses explicit per-tool validators,
not an undocumented shell or wrapper fallback.

### 5.3 cwd resolution

The host:

1. injects the selected `ProjectCapability`;
2. resolves each portable component below the canonical project root;
3. rejects symlink/reparse-point components;
4. re-canonicalizes at proposal creation, approval and spawn;
5. compares the sealed root/cwd fingerprint at execution;
6. fails closed on drift, missing directories or containment uncertainty.

### 5.4 environment

Child processes start from a new environment. Only these host-selected base
names may be copied when required:

- `SYSTEMROOT`
- `WINDIR`
- `COMSPEC`
- `PATHEXT`
- filtered `PATH`
- isolated `TEMP` and `TMP`

Names containing `KEY`, `TOKEN`, `SECRET`, `PASSWORD`, `CREDENTIAL`, `AUTH`
or provider names are never copied. The provider and renderer cannot supply an
environment map. `HOME`, `USERPROFILE`, `APPDATA` and `LOCALAPPDATA` are not
inherited; tools that require a profile are unavailable until an isolated
profile design is accepted.

### 5.5 process lifecycle

On Windows, the shared process executor uses this order:

1. resolve the approved absolute executable;
2. create the child suspended;
3. create a Pillow-owned Job Object with kill-on-close;
4. assign the child to that Job Object;
5. start asynchronous stdout/stderr capture and cancellation ownership;
6. resume the primary thread;
7. wait for terminal exit, cancellation or timeout;
8. close/terminate through the owned stable handles only;
9. prove the owned process tree is gone before recording cleanup success.

Pillow must extract or share the accepted ACP Job Object implementation. It
must not reintroduce `taskkill`, name-wide cleanup, accumulated numeric PID
termination or a best-effort fallback. Descendant observations require stable
identity evidence; unknown or incomplete identity fails closed.

Default timeout is 60 seconds. The manifest may select a fixed value between
1 and 600 seconds per tool label. The provider cannot select a timeout.

### 5.6 output

- stdout and stderr are captured as tagged chunks with a monotonic sequence.
- Per-stream ordering is preserved; Pillow does not claim perfect cross-stream
  ordering beyond observed readiness.
- Invalid UTF-8 is replaced for display and marks `encoding=lossy`.
- Combined captured output is capped at 256 KiB and 4,000 display lines.
- Truncation records byte counts, stream digests and `truncated=true`.
- Escape/control sequences are neutralized before DOM projection and copy.
- Raw output is never written to ordinary logs or run-event payloads.

C4.0 persists no full transcript. SQLite retains terminal metadata, byte
counts, digests, truncation/encoding flags, exit code and a bounded fixed-copy
summary. Live output remains in a host-owned bounded buffer. After restart,
the durable summary remains but the UI explicitly states that live output was
not retained. Structured tests and approved artifacts may persist through
their existing governed channels; transcript scraping does not invent them.

## 6. C4.0 Git read contract

### 6.1 Separation from process.run

Git read uses a dedicated `git-executor` module with closed semantic
operations. It may share the lower-level supervised child implementation, but
it cannot call or expose `process.run@1`, accept provider argv or bypass Git
parsing and bounds.

Initial operations:

- `git.discover@1`
- `git.status@1`
- `git.diff@1`
- branch, HEAD and worktree identity returned by status/discovery DTOs

The first UI is user-driven. Provider-facing Git read tools may be added only
after the native/UI path passes its focused gate, using the existing governed
read lifecycle rather than process approval.

### 6.2 repository ownership

- The project root and nested repositories below it may be discovered.
- `.git` files are resolved only when their target remains within an accepted
  project-owned worktree boundary.
- Linked worktrees outside the project are displayed as unavailable metadata;
  Pillow does not read or mutate them.
- Submodule identity may be shown, but no submodule initialization, update or
  network action occurs.
- Repository selection uses a stable project-relative identity and generation
  counter. Project switch clears all in-flight results.

### 6.3 fixed Git invocation

The host resolves one trusted `git.exe` and uses fixed argv templates only.
Each invocation uses a clean environment and disables interactive prompts,
pagers, hooks, external diff, textconv, credential helpers and system/global
configuration where Git permits it. Local configuration is parsed or probed
for unsafe hooks, filters and external commands before C4 claims a safe read.

Read templates must use machine formats such as porcelain v2 with NUL
termination. Human-coloured output is never parsed. Exit codes, record counts,
bytes and parse completion are checked before publishing a DTO.

### 6.4 bounded DTOs

Status and diff DTOs contain only project-relative portable paths and enums.
They never contain native roots or raw Git diagnostics.

Initial bounds:

- at most 5,000 status entries;
- at most 2 MiB aggregate status transport;
- at most 256 KiB text diff per opened file;
- binary, oversized, submodule and unsupported entries return explicit typed
  summaries rather than partial text claims;
- rename records keep old and new portable paths as one semantic entry;
- ignored files are omitted by default and may be requested only through a
  separately bounded UI action.

Refresh is explicit plus host invalidation after a Pillow-owned file change.
Every response carries repository identity and generation. A stale response
is discarded, never silently merged.

## 7. Persistence and event model

SQLite remains the operational authority.

`exec_proposals` stores:

- proposal, project, conversation, run and tool-call identity;
- tool label, argv digest and portable cwd;
- sealed fingerprint and lifecycle state;
- expiry, decision and consumed-grant digest;
- started/completed timestamps;
- exit code, timeout/cancel flags, byte counts, stream digests, truncation and
  encoding flags;
- bounded terminal receipt.

It does not store the raw grant, native executable/root, arbitrary environment
or full transcript. The host keeps the sealed executable/argv/cwd only for the
live proposal. Restart invalidates proposals that no longer have that sealed
material.

Git read results are snapshots, not a second operational authority. Durable
state is limited to selected repository identity and UI preference if needed;
status/diff is re-read from the repository and generation-checked.

Runtime events add fixed validated kinds for process proposal, decision,
start and terminal outcome. Output chunks use a bounded native stream and do
not become unbounded durable run events.

## 8. UI states and privacy

The renderer receives host-built DTOs only.

- Command display is derived from the sealed argv but is never re-parsed for
  execution.
- Origin is explicit: `agent` or `user`. C4.0 has no free-form user shell;
  user-origin actions are host-defined quick actions using the same proposal
  and confirmation boundary.
- The approval bar permits one pending action for the selected conversation.
- Review shows program label, literal arguments, portable cwd, risk class and
  expiry without native paths or internal identifiers.
- Escape rejects only when the approval bar owns focus.
- Closing the output pane does not cancel a process. Cancellation is explicit.
- Switching conversation parks projection; switching project hard-clears it.
- Terminal outcomes remain durable one-line activity. Successful history does
  not create a permanent stack of expanded cards.
- Copy uses only already-sanitized visible text.
- Errors use fixed opaque codes. Raw provider, Git, process, native path,
  grant, session, run and tool identifiers stay out of DOM, screenshots and
  user-facing logs.

## 9. Error and recovery behavior

| Condition | Result |
| --- | --- |
| Unknown tool/program | Opaque unavailable error; no proposal |
| Invalid argv/cwd | Protocol rejection; no durable authority |
| Approval expiry/reject | Terminal receipt; never spawn |
| Fingerprint/base drift | Conflict receipt; never spawn/apply |
| Spawn/Job assignment failure | Fail closed; suspended child terminated through owned handles |
| Timeout/cancel | Terminate owned Job Object, prove cleanup, record terminal result |
| Cleanup uncertainty | Do not claim terminal cleanup success; surface controller-grade failure evidence |
| Host restart | Non-terminal proposal/run becomes interrupted; no replay |
| Output overflow | Continue bounded execution, mark truncation, retain digests |
| Invalid Git output | Discard snapshot and show typed refresh failure |
| Git identity/generation drift | Discard stale result; require refresh |
| Unsafe Git config/hook/filter | Read capability unavailable with opaque reason |

## 10. Acceptance

C4.0 requires all of the following:

1. protocol fixtures for exact schemas, unknown-field denial, argv limits,
   cwd matrix and canonical fingerprints;
2. policy tests for transitions, expiry, single-use grant, replay, conflict,
   cancellation and restart interruption;
3. storage tests for metadata-only persistence, uniqueness, migration,
   receipt replay and absence of raw grant/native paths/transcript;
4. process tests for environment exclusion, suspended assignment, grandchild
   cleanup, timeout, cancel, lossy encoding and byte truncation;
5. Git fixtures for porcelain v2 `-z`, rename, binary, large file, nested repo,
   linked worktree, detached HEAD, conflict, submodule and unsafe config;
6. typed Tauri/TypeScript parity tests and renderer tests proving no authority
   crosses the DTO boundary;
7. real desktop scenarios covering approve, reject, expiry, cancel, timeout,
   project/conversation isolation, forced restart, Git refresh drift and
   residual-process proof;
8. 1024x720 and 1440x960 screenshots proving conversation dominance, one
   approval bar and collapsible context;
9. full `pnpm check`, Rust workspace/doctests, Clippy `-D warnings`, production
   UI build, serial desktop E2E, `cargo fmt --all --check`,
   `git diff --check` and empty cached diff;
10. independent development review followed by controller source review,
    Critical 0 / Important 0 and controller-rerun gates.

No test may weaken the accepted process-safety laws, add numeric PID killing
or replace evidence with a larger timeout.

## 11. Explicit exclusions

C4.0 excludes:

- interactive PTY or shell strings;
- shell hosts, script shims and arbitrary executables;
- background/daemon sessions and session-wide approval;
- provider environment, secrets, credentials and network tools;
- Git stage, unstage, commit, restore, switch or worktree mutation;
- Git reset, clean, force, rebase, merge, cherry-pick, push, pull, fetch,
  remote/submodule mutation and arbitrary Git argv;
- provider-native/MCP process tools;
- Codex/OpenCode runtime expansion;
- Obsidian and company-operation features;
- full transcript persistence or a claim of Windows sandboxing.

## 12. Provenance

Research references:

- Pillow C1-C3 accepted working tree at HEAD `7b7f70f` plus recorded C3
  checkpoint;
- Grok App local clone at
  `cbbbdf17bb8db1588b6707f3f42f6b00397e27f9`, MIT;
- zbbody/AionUi local clone at
  `00c293414333b6fcc4e4598389fa3952c8d0b09b`, Apache-2.0;
- Pillow ACP Windows process implementation and C3 process-safety evidence.

This design adopts concepts and existing Pillow code patterns. It authorizes
no direct external source adaptation. Any later adapted file must record the
source repository, exact commit, path, license and required notice before it
enters the implementation allowlist.

## 13. Research inputs

- `docs/research/2026-08-02-c4-terminal-process-research.md`
- `docs/research/2026-08-02-c4-git-governance-research.md`
- `docs/research/2026-08-02-c4-terminal-git-interaction-research.md`
- `docs/research/2026-08-02-c4-security-acceptance-research.md`

This document does not authorize implementation, staging or commit.
