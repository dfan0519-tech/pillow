# Wave C3 Research — Governed Write Security And Architecture

Date: 2026-08-01
Researcher: grok3 (read-only architecture)
Workspace: `this repository`
Baseline HEAD inspected: `7b7f70f` (dirty working tree; C2 checkpoint accepted, unstaged)
Scope: evaluate C3 governed write/diff/approval without modifying product code

Local session orchestration (optional, out of tree) was used only to record the research session; it is not a product dependency.

---

## 1. Executive recommendation

**Ship C3 first release as whole-file create/modify of bounded UTF-8 text under a host-owned proposal → human approval → base-hash compare-and-swap apply pipeline.**

Do **not** ship multi-file shadow workspaces, patch languages, rename/delete, binary writes, directory mutation, terminal, Git, network, credentials, Obsidian, or new runtimes in the first C3 release.

| Choice | Decision |
| --- | --- |
| Apply model | Option 1 (whole-file replacement) with mandatory base content hash CAS |
| Diff model | Host-computed bounded unified view from stored base + proposed content; never trust provider diffs |
| Approval model | Durable SQLite proposal + append-only decision/receipt ledger; renderer proposes UI actions only |
| First-release surface | One path per proposal; create or overwrite UTF-8 text only |
| Port style | Typed contracts inspired by B3 tool reservations and legacy operator-approval *invariants*, not wholesale JS copy |

This is the lowest-complexity model that reuses existing path, ownership, reservation, receipt, and progressive-disclosure machinery while closing the critical write threats (path escape, TOCTOU, replay, renderer bypass, partial write).

---

## 2. Current architecture map (exact local paths / lines)

### 2.1 Layer diagram (as implemented today)

```text
Renderer (React)
  resourceGateway / conversationGateway / agentActivity*
        |  Tauri commands only (no FS authority)
Desktop host (Tauri Rust)
  path_scope ──► project/conversation root grants
  resource_commands ──► kernel-resource (read-only UI)
  conversation_service ──► runtime-host + SQLite
        |
runtime-host::governed_read
  binds run_id → ToolActiveContext + ProjectCapability
        |
adapter-grok-acp
  normalizes provider server requests → ProviderToolProposal
  decide_permission always denies provider-native tool authority
        |
tool-executor::GovernedReadToolGateway
  reserve → claim → execute → complete receipt
        |
agent-protocol (fixed manifests: fs.list|fs.read|fs.search @1)
storage-sqlite (projects, conversations, runs, events, tool_calls)
```

**C3 write path does not exist yet.** Checkpoint and roadmap both state C3 is planning-only.

Evidence:

- `docs/verification/2026-08-01-wave-c2-checkpoint.md` L43–55 — C3 design locks required; terminal/Git/Obsidian/credentials/new runtimes excluded.
- `docs/roadmap/2026-07-31-stage1-overall-status.md` L37 — C3 “Planning next”; L78+ exclude writes/rename/delete/patch.

### 2.2 Protocol ownership and tool identity

| Concern | Location | Lines / notes |
| --- | --- | --- |
| Fixed tool catalog (read-only only) | `crates/agent-protocol/src/tool/types.rs` | `ToolId` L61–91: only `fs.list`, `fs.read`, `fs.search` |
| Wire proposal envelope | same | `ToolProposal` L144–153: `project_id`, `conversation_id`, `run_id`, `call_id`, `tool_id`, `tool_version`, `arguments`, `arguments_digest` |
| Opaque validated proposal | same | `ValidatedToolProposal` L193–264; private fields; compile-fail forge guards |
| Execution states | same | `ToolExecutionState` L317–337: proposed → validated → executing → succeeded/failed/cancelled |
| Path validation (protocol) | `crates/agent-protocol/src/tool/validate.rs` | Identity L362–375; portable path L401–432; Windows devices/ADS/`:` L408–429; result scope L387–399 |
| Context binding | same | `validate_for_context` L36–86 rejects foreign project/conversation/run |
| Fingerprint + reservation key | same + `reservation.rs` | Call identity key L168–179; full-proposal fingerprint L181–196; classify Unreserved/Replay/Conflict L74–88 in `reservation.rs` |
| Manifest caps | `crates/agent-protocol/src/tool/manifest.rs` | List 256 KiB / 1000; read 1 MiB; search 512 KiB / 500; timeouts L5–9, L32–81 |
| Canonical public errors | `crates/agent-protocol/src/tool/public_error.rs` | Fixed messages L30–47; reject non-canonical wire L68–88 |
| Security fixtures | `crates/agent-protocol/tests/tool_contract_security.rs` | Replay vs conflict L99–133; secret-safe parsers L56–96; Windows path matrix L400–441 |

### 2.3 Durable store and ownership graph

| Concern | Location | Lines / notes |
| --- | --- | --- |
| Project authority | `crates/storage-sqlite/migrations/0001_projects.sql` | L1–10: `id`, `root_path`, `canonical_root_path` UNIQUE, `trust_state` |
| Conversation/run/event ownership | `migrations/0002_conversations.sql` | L1–10 conversations FK project; L27–42 runs; L50–76 `run_events` append-only-ish unique (run_id, sequence) |
| Tool call ledger | `migrations/0003_tool_calls.sql` | L4–32 reservation PK + UNIQUE call identity; tool_id CHECK only three reads; L40–55 transition history UNIQUE (reservation_key, state) |
| Reserve / claim / complete | `crates/storage-sqlite/src/tool_call.rs` | `reserve` L76–134; `claim_execution` L251–327 with lease; `complete_execution` L329–398; context FK join L407–434; allowed transitions L475–487 |

Ownership invariant already enforced for reads:

1. `ensure_context_exists` joins `runs → conversations → projects` (tool_call.rs L413–421).
2. Reservation key = hash(project, conversation, run, call_id) only.
3. Fingerprint = hash(identity + tool + version + arguments_digest).
4. Same key + different fingerprint = Conflict (not silent overwrite).

### 2.4 Tool executor path safety (read baseline to extend)

| Concern | Location | Lines / notes |
| --- | --- | --- |
| Project capability | `crates/tool-executor/src/lib.rs` | `ProjectCapability::new` L37–52: canonicalize root directory |
| Governed gateway | same | `GovernedReadToolGateway::execute` L186–234: reserve → validate → claim → execute → complete |
| Symlink/reparse refusal | same | `resolve_target` L618–635 component walk; `is_reparse_point` L670–679 |
| Check-then-use hardening | same | read re-resolves after open L393–396; search re-resolves L458–460 |
| Public failures | same | opaque `ToolPublicErrorCode` only; no path strings in errors |

UI resource core mirrors the same portable path + reparse rules:

- `crates/kernel-resource/src/lib.rs` L357–388 path grammar; L417–436 resolve; L579–583 Windows reparse.

### 2.5 Runtime host and conversation service

| Concern | Location | Lines / notes |
| --- | --- | --- |
| Bridge: provider → host authority | `crates/runtime-host/src/governed_read.rs` | Adapter supplies only call/tool/args (L19–25); host injects project/conversation/run (L122–150); `execute_governed_read` L126–175 |
| Run registration | `crates/runtime-host/src/host/mod.rs` | `register_governed_read_run` L119–138; `release_governed_read_run` L140–142 |
| Conversation send_turn | `apps/desktop/src-tauri/src/conversation_service/service.rs` | Resolve cwd first L285–288; create run L295–301; register governed read L303–314; release on session/spawn fail L323, L358 |
| Restart reconciliation | `.../reconciliation.rs` | Terminal tool receipts projected into contiguous lifecycle events L23–142; receipt re-validated L145–177 |
| Recover inflight | `service.rs` L168–178 | Mark interrupted + reconcile terminal tool receipts |

### 2.6 Path capabilities (window / project / conversation)

| Concern | Location | Lines / notes |
| --- | --- | --- |
| Capability state | `apps/desktop/src-tauri/src/path_scope.rs` | Grants/permits/generations L19–30 |
| Picker TTL + revoke | same | TTL 5 min L88; consume L173–198; window revoke L446–457 |
| Conversation root require | same | L420–444: project_id + conversation_id + canonical root must match grant |
| Windows disk-only + reparse reject | same | validate_picker L517–544; reject_reparse L547–563; no UNC/verbatim except normalized VerbatimDisk L579–597 |

### 2.7 Renderer contracts (activity + resources)

| Concern | Location | Lines / notes |
| --- | --- | --- |
| Resource gateway (read-only) | `apps/desktop/src/lib/resourceGateway.ts` | Interface L138–150; opaque error normalization L114–136; no roots/handles |
| Resource pane props | `apps/desktop/src/features/resources/resourceTypes.ts` | `projectId` + gateway + relative path callback L13–17 |
| Activity ownership | `apps/desktop/src/features/conversations/agentActivityTypes.ts` | Conversation-scoped records L89–118; IDs never rendered L103–106 |
| Timeline projection | `agentActivityFromTimeline.ts` | Portable paths only; forbidden display ID carrier L19–64 |
| Tauri resource commands | `apps/desktop/src-tauri/src/resource_commands.rs` | Read-only; project resolve L277–283; opaque codes L245–275; binary content stripped for text DTO L99–115 |

### 2.8 Provider authority is intentionally denied

| Concern | Location | Lines / notes |
| --- | --- | --- |
| Permission always deny | `crates/adapter-grok-acp/src/map/permission.rs` | Comment L113; `allowed: false` L176–178; write denied even with allow_always (test L567+) |
| Fixed read normalization | `provider_tools.rs` | Only `fs.list|fs.read|fs.search` L148; ACP text-read maps to `fs.read` L112–124 |
| Reject unknown write tools | tests in same file | `fs.write` rejected around L435 |

### 2.9 Legacy reference (read-only) — approval invariants to port

Source: `legacy Keeper (out of tree)\lib\operator-approval-authority.js` (frozen product; do not modify).

| Invariant | Legacy evidence | Port as typed contract (not copy) |
| --- | --- | --- |
| Immutable task digest | L50–82 `taskDigestFields` / `approvalDigest` | Proposal content hash separate from lifecycle status |
| Decision envelope | L84–105 taskId, decision, digest, timestamp, nonce | Strict schema + deny_unknown_fields |
| HMAC signature over canonical lines | L107–112, L95–105 | Host-held approval secret; renderer never holds raw secret |
| TTL window | L6, L144–146, L165–167 default 30s | Configurable; fail closed on expiry |
| Nonce anti-replay | L150–185 Map + prune | Durable consumed-nonce or one-shot decision row in SQLite |
| Reject wrong task/decision/digest | tests `keeper-operator-approval.test.js` L79–150 | Unit + integration fixtures |

Inventory already records PORT target as `crates/kernel-policy` (`docs/migration/legacy-capability-inventory.md` L19). That crate does not yet exist in the dirty tree; C3 should introduce a narrow policy module rather than grow UI or adapter ownership of approvals.

### 2.10 Product design constraints already locked

From `docs/superpowers/specs/2026-07-29-pillow-first-design.md`:

- Ask default; write classified separately (L96–104).
- SQLite only operational authority; approvals/tool receipts required durable objects (L108–126).
- Secrets never in renderer/logs/model/SQLite (L102–103, L128–129).

From AGENTS engineering rules:

- Domain state in SQLite.
- Desktop shell must not own agent-domain logic.
- Approval/audit/secret boundaries enforced below UI.
- Every durable object project-scoped.
- Write-capable tools require explicit approval-path tests.

---

## 3. Three-option comparison

Assumptions for all options: project-relative portable paths; host path resolver equal to B3; human approval required; durable receipts; first-release excludes rename/delete/binary/dirs/terminal/Git/network/credentials/Obsidian/new runtimes unless noted.

### Option A — Whole-file replacement after approval

**Mechanism**

1. Agent proposes `path` + full proposed UTF-8 content (+ create vs modify intent).
2. Host reads current base (if exists), computes `base_content_hash`, stores proposal bytes, host-builds bounded diff for UI.
3. Human approves/rejects/expires.
4. On apply: re-resolve path (no reparse), re-hash base, CAS compare, write temp in target directory, `fsync`, atomic rename over target.

| Dimension | Assessment |
| --- | --- |
| Security | Strong: no patch interpreter; full content is the approved object; CAS blocks silent drift |
| Complexity | Lowest among viable options |
| UX | Diff is reconstructible; large files need strict caps |
| Crash safety | Temp+rename well understood on Windows/NTFS |
| Multi-file | Out of scope unless later batching of independent approvals |
| Fit for C3.0 | **Best** |

**Risks**

- Large whole-file payloads in SQLite if caps are loose.
- Create-vs-modify confusion if “absent base” is not a first-class mode.
- TOCTOU still requires re-check at apply (not unique to A).

### Option B — Canonical patch proposal with base-hash CAS

**Mechanism**

1. Agent proposes path + base hash + canonical patch (e.g. unified diff or JSON ops).
2. Host validates patch grammar, bound sizes, and that applying patch to *current* base yields UTF-8 within caps.
3. Approval binds to `(path, base_hash, patch_digest)` not free-form text.
4. Apply: re-hash base, apply patch in memory, atomic whole-file write of result.

| Dimension | Assessment |
| --- | --- |
| Security | Good if patch language is host-owned and fail-closed; bad if provider free-form diffs trusted |
| Complexity | High: patch parser, context-line rules, encoding edge cases, adversarial patches |
| UX | Smaller wire payloads; risk of unreadable/ambiguous diffs |
| Crash safety | Same as A after materialization |
| Multi-file | Still one-file unless extended |
| Fit for C3.0 | **Defer** as optional optimization after A |

**Risks**

- Patch application bugs are classic RCE/integrity footguns.
- Context drift produces confusing failures without improving safety over whole-file CAS.
- Dual representation (patch vs resulting content) complicates approval identity.

### Option C — Staged shadow-workspace transaction

**Mechanism**

1. Host creates isolated shadow tree for a run/proposal set.
2. Agent writes freely inside shadow under policy.
3. Host diffs shadow vs real project; human approves transaction.
4. Atomic commit of N files (or rollback).

| Dimension | Assessment |
| --- | --- |
| Security | Strong *if* shadow isolation is perfect; hard on Windows junctions/ADS/locking |
| Complexity | Highest: lifecycle, disk budget, cleanup, crash, multi-file atomicity |
| UX | Best for multi-file agent edits |
| Crash safety | Requires durable transaction journal + commit protocol |
| Multi-file | Native strength |
| Fit for C3.0 | **Too large**; candidate C3.x / C4-adjacent after single-file writes proven |

**Risks**

- Shadow escape via reparse/junction into real tree.
- Incomplete cleanup leaves toxic residue.
- UI must explain multi-file transaction integrity (all-or-nothing).
- Overlaps future terminal/Git workflows; pulls Stage 1 later concerns forward.

### Comparison matrix

| Criterion | A whole-file | B patch+CAS | C shadow txn |
| --- | --- | --- | --- |
| First-release complexity | Low | Medium-high | High |
| Path/TOCTOU reuse of B3 | Direct | Direct | Needs new isolation layer |
| Approval object clarity | Full content digest | Patch+base digest | Transaction manifest digest |
| Atomic single-file apply | Native | Native after materialize | Needs multi-file protocol |
| Diff leakage control | Host-built from stored bytes | Host-built after dry-apply | Host-built from shadow |
| Windows reparse risk surface | Existing resolve_target | Existing + patch IO | Expanded |
| Alignment with C2 progressive disclosure | One proposal card | One proposal card | Multi-file review UI |
| Residual product risk | Payload size | Patch correctness | Isolation + cleanup |

**Recommendation:** A for C3.0, with B’s **base-hash CAS** property mandatory, and C deferred.

---

## 4. Recommended contract and state machine

### 4.1 New tool / domain objects (typed; do not overload read manifests)

Suggested identities (names illustrative; keep provider-neutral):

| Object | Purpose |
| --- | --- |
| `ToolId::FsWrite` / `fs.write@1` or domain type `WriteProposal` | Create/modify one portable relative UTF-8 file |
| `WriteProposal` | Durable proposal envelope |
| `WriteApprovalDecision` | Human approve/reject; signed or host-authenticated |
| `WriteApplyReceipt` | Terminal apply result bound to proposal fingerprint |

**Do not** auto-allow write via ACP `session/request_permission`. Provider permission remains observation-only (`decide_permission` stays deny). Writes enter only through Pillow-owned proposal + host approval commands.

### 4.2 Proposal wire shape (recommended)

```text
WriteProposal {
  project_id, conversation_id, run_id, call_id,
  tool_id: "fs.write", tool_version: 1,
  mode: "create" | "modify",
  path: ProjectRelativePath,
  encoding: "utf-8",                 // only
  proposed_content: string,          // bounded
  proposed_content_digest: sha256,   // of canonical UTF-8 bytes
  base_content_hash: sha256 | null,  // null only for create of non-existing path
  base_byte_len: u64,
  arguments_digest: sha256,          // canonical args without raw content if content stored out-of-band
  proposal_fingerprint: sha256       // full semantic identity
}
```

Content may live as a SQLite BLOB/TEXT column with byte cap rather than re-inlining into every event payload. Event stream should carry **summaries and digests**, not multi-megabyte bodies (see leakage).

**Hard caps (initial recommendation; tune with perf gates):**

| Cap | Suggested default | Rationale |
| --- | --- | --- |
| Proposed content bytes | 256 KiB (strict) / hard max 1 MiB | Align near read caps; UI diff cost |
| Diff display lines/bytes | e.g. 2_000 lines / 256 KiB | Progressive disclosure |
| Path length | 4096 / component 255 | Match existing protocol |
| Concurrent pending proposals per conversation | small (e.g. 8) | Prevent flood |
| Approval TTL | 5–15 minutes human UX; shorter for automated tests | Legacy used 30s for HTTP operator path; desktop human needs longer *proposal* life but **decision** nonce still short-lived |

### 4.3 State machine

Separate **proposal lifecycle** from **approval decision** and **apply execution**.

```text
                 ┌──────────────┐
                 │  proposed    │  (parsed, not trusted)
                 └──────┬───────┘
                        │ host validates path/schema/digest/context
                        v
                 ┌──────────────┐
                 │  validated   │  durable reservation (call identity)
                 └──────┬───────┘
                        │ persist content + base hash snapshot metadata
                        v
                 ┌──────────────┐
          ┌──────┤pending_approval├──────┐
          │      └──────┬───────┘      │
     reject/expire/cancel│ approve      │ conversation/run cancel
          │              v              │
          v       ┌──────────────┐      v
     ┌────────┐   │   approved   │   ┌──────────┐
     │rejected│   └──────┬───────┘   │cancelled │
     │expired │          │ apply claim│          │
     └────────┘          v            └──────────┘
                  ┌──────────────┐
                  │  applying    │  exclusive lease (like tool claim)
                  └──────┬───────┘
               success / fail (CAS miss, IO, cancel)
                         v
              ┌─────────────────────┐
              │ succeeded | failed  │  terminal + append-only receipt
              └─────────────────────┘
```

**Rules**

1. **Reservation:** reuse B3 semantics — same `(project, conversation, run, call)` cannot change fingerprint (Conflict); identical fingerprint may Replay terminal receipt.
2. **One live non-terminal write per path per project** (recommended): second pending proposal for same path Conflicts or queues only after explicit product decision; default = Conflict to avoid approval races.
3. **Approval cannot mutate payload:** decision rows reference `proposal_fingerprint` + `content_digest` + `base_content_hash`; any mismatch is reject.
4. **Apply is host-only:** no renderer/provider path reaches `std::fs::write` on project files.
5. **Cancel:** conversation cancel / run cancel / shutdown marks pending proposals cancelled if not applying; if applying, cooperative cancel only when safe before rename; after rename, success stands.
6. **Restart recovery:** non-terminal proposals become `expired` or `cancelled` on recover policy (prefer fail-closed `interrupted`/`expired` requiring re-proposal); never auto-apply across process restart without a durable `approved` decision still within TTL *and* explicit product rule. **Recommended first release:** restart invalidates unused approvals; only terminal receipts survive (mirrors cautious recovery).

### 4.4 Host apply algorithm (atomic, Windows-aware)

1. `require` conversation grant / project capability (path_scope + ProjectCapability).
2. `validate_portable_path` (protocol) then `resolve_target` component-wise with symlink/reparse reject.
3. For `modify`: target must be existing regular file (not symlink); re-read all bytes (cap); hash == `base_content_hash` else fail `BaseHashMismatch` (public code without path).
4. For `create`: target must not exist; parent must exist as real directory (no auto-mkdir of deep trees beyond last component — **first release: parent must already exist** to avoid directory mutation scope creep).
5. Write `.{name}.pillow-tmp-{uuid}` in the **same directory** as target (same volume for atomic rename).
6. Write content, `sync_all`, close.
7. Re-check path resolve + base hash one last time (TOCTOU).
8. Atomic replace: Windows `replace_file` / rename-with-replace pattern; on failure delete temp.
9. Persist terminal receipt with post-hash of written content.
10. Emit lifecycle events to owning conversation only after durable receipt (same persistence-before-emit discipline as reads).

### 4.5 Approval authentication (port invariants, new module)

Recommended `kernel-policy` (or `storage-sqlite` approval submodule for C3.0 minimalism):

- **Local desktop:** approval command must arrive from Tauri command path with window/conversation capability, not raw provider IPC.
- **Decision envelope:** `{ proposal_id, decision: approve|reject, proposal_fingerprint, content_digest, base_content_hash, timestamp, nonce, signature? }`.
- **Anti-replay:** durable unique nonce or single-use decision row; second submit → Conflict.
- **TTL:** reject if `now - proposal.created_at > proposal_ttl` or decision timestamp skew.
- **No secret in renderer:** if HMAC used, signing happens in host when presenting approval challenge; renderer returns opaque decision token issued by host, not free-form signed blobs forged offline — *or* host signs challenge and verifies response. Prefer **host-issued one-time approval grant id** stored in SQLite over shipping legacy key files into the IDE unless multi-process operator HTTP returns.

Legacy HTTP operator signing is a reference for *properties*, not the desktop UX transport.

### 4.6 UI progressive disclosure contract

Extend conversation activity, not a global task board:

1. Compact summary: `Write proposal · path · pending approval`.
2. Details: host-built diff (truncated notice), mode create/modify, expiry countdown.
3. Actions: Approve / Reject (disabled when expired/cancelled).
4. Result: succeeded/failed with opaque error (`Base changed`, `Path rejected`, …) — **never** absolute paths, never raw IO strings.
5. Artifacts: optional “open file” via existing portable relative path callback only after success.

Renderer must not receive proposed content larger than display caps without truncation markers; full content stays host-side for apply.

---

## 5. Threat / control matrix

| Threat | Attack sketch | Control in recommended design | Existing substrate to reuse |
| --- | --- | --- | --- |
| Cross-project write | Proposal uses foreign `project_id` | `ToolActiveContext` / conversation grant mismatch fail-closed | validate.rs L43–50; path_scope L420–444; tool_call ensure_context L407–434 |
| Cross-conversation write | Reuse call against other conversation | Context fields in reservation key + FK graph | reservation key L168–179; conversations FK |
| Proposal identity forge | Mutate content after approval | Fingerprint + content_digest bound into approval and apply | fingerprint L181–196; Validated* opacity |
| Replay approve | Resubmit old decision | Single-use nonce/decision row; terminal proposal ignores new decisions | legacy verifyDecision L161–185; tool claim lease pattern |
| Conflicting payloads same call_id | Retry with different body | Reservation Conflict | tool_call reserve L87–93; security test L99–133 |
| Approval expiry bypass | Approve after TTL | Host clock check; state → expired | legacy TTL L165–167 |
| Rejection ignore | Apply after reject | Apply only from `approved` + valid decision id | new state machine |
| Cancel race | Cancel during apply | Lease owner; post-rename irreversible; pre-rename cancel → cancelled | claim_execution L251–327 |
| Restart recovery abuse | Auto-apply pending on boot | Fail-closed expire pending approvals on recover | recover_inflight L168–178 pattern |
| Path traversal | `../`, absolute, UNC | Portable path grammar + component resolve | validate.rs L401–432; kernel-resource L357–388 |
| Absolute / device / ADS | `C:`, `COM1`, `file:stream` | Reject `:` and device names | validate.rs L408–429; security test L400–441 |
| Symlink/junction TOCTOU | Swap link after approve | Reparse reject each component; re-resolve at apply; no follow | tool-executor resolve_target L618–635 |
| Base hash drift | File edited externally | CAS fail `BaseHashMismatch`; no clobber | new apply step 3/7 |
| Partial write / crash | Kill mid-write | Temp file + fsync + atomic replace; no in-place truncate | new apply algorithm |
| Renderer authority bypass | Fake Tauri invoke / mutated DTO | Commands re-load proposal from SQLite by id; ignore client content | resource/conversation command pattern |
| Provider authority bypass | ACP permission allow write | Keep `decide_permission` deny; no provider write executor | permission.rs L113–178 |
| Diff bomb | Huge proposed content | Byte/line caps; store + display truncation | manifest-style caps |
| Encoding smuggling | Invalid UTF-8 as text | Reject non-UTF-8; no binary mode in C3.0 | FsRead already classifies binary |
| Secret / debug leakage | Error strings with paths/secrets | Canonical public error codes only; protocol errors secret-safe | public_error.rs; tool_contract_security L48–53, L364–397 |
| Receipt tampering | Edit result_json | Validate receipt against proposal; append transitions | reconciliation L145–177; transitions table |
| Concurrent two approvers | Double apply | Exclusive apply lease; second Completes as Busy/Conflict | claim_execution |
| Parent directory create escape | create `a/b/c` builds tree | Require existing parent in C3.0 | task boundary |
| Hardlink / open handle races | Replace while locked | Document residual risk; fail with opaque IO error; optional deny if not writable exclusive | residual risks |

---

## 6. Proposed test matrix

### 6.1 Protocol / contract unit tests

| ID | Case | Expect |
| --- | --- | --- |
| P1 | Unknown tool `fs.write` rejected **until** catalog extended; then only version 1 | Fail-closed catalog |
| P2 | Path matrix: `..`, UNC, backslash, `C:`, ADS `:`, devices, trailing dot/space | InvalidArguments |
| P3 | Non-UTF-8 content / NUL bytes | InvalidArguments |
| P4 | Content over cap | InvalidArguments |
| P5 | Digest mismatch vs canonical content | ArgumentsDigestMismatch |
| P6 | ActiveContextMismatch foreign project/conversation/run | Error |
| P7 | Reservation Replay vs Conflict | Same as read security tests |
| P8 | Public error canonicalization rejects custom messages | Parse fail |

### 6.2 Approval / policy tests

| ID | Case | Expect |
| --- | --- | --- |
| A1 | Approve valid pending proposal | State approved; decision durable |
| A2 | Reject | Terminal rejected; apply impossible |
| A3 | Expired TTL | Decision rejected; state expired |
| A4 | Replay same nonce/decision | Conflict / already used |
| A5 | Decision fingerprint ≠ stored proposal | Reject |
| A6 | Renderer-supplied alternate content ignored | Apply uses SQLite body only |
| A7 | Cross-window / cross-conversation approve | PathScope NotAllowed / MissingContext |

### 6.3 Apply / filesystem tests (temp project roots)

| ID | Case | Expect |
| --- | --- | --- |
| W1 | Modify happy path atomic replace | File content + post-hash receipt |
| W2 | Create happy path | New file; parent pre-existed |
| W3 | Base hash drift before apply | Failed BaseHashMismatch; file unchanged |
| W4 | Symlink at any component | PathRejected |
| W5 | Junction/reparse on Windows | PathRejected |
| W6 | Target is directory | PathRejected |
| W7 | Create when file exists | Failed |
| W8 | Modify when missing | Failed |
| W9 | Crash between temp write and rename (simulated) | No truncated target; temp cleaned or quarantined |
| W10 | Concurrent apply same path | One success; one Conflict/Busy |
| W11 | Cancel before apply | Cancelled; no write |
| W12 | Cancel during applying after rename | Success stands |

### 6.4 Integration: conversation / runtime / adapter

| ID | Case | Expect |
| --- | --- | --- |
| I1 | Provider ACP write permission request | Always denied; no disk mutation |
| I2 | Provider `fs.write` server method | MethodNotFound / InvalidParams |
| I3 | Governed write only after host registration of run | RunNotRegistered otherwise |
| I4 | Timeline events only after durable receipt | Persistence-before-emit |
| I5 | Restart with pending approval | No auto-apply; proposal expired/cancelled; no partial file |
| I6 | Restart with terminal success | Receipt + lifecycle reconciliation |

### 6.5 Renderer / gateway tests

| ID | Case | Expect |
| --- | --- | --- |
| U1 | Activity filters foreign conversationId | No render |
| U2 | Diff truncation notice; no raw machine paths | Opaque errors |
| U3 | Approve/reject buttons disabled when terminal | UI state machine |
| U4 | Resource pane still read-only (no write API) | Gateway surface unchanged for writes |
| U5 | Desktop screenshots at supported Windows sizes for approval disclosure | AGENTS UI gate |

### 6.6 Security regression pack (must stay green)

Existing:

- `crates/agent-protocol/tests/tool_contract_security.rs`
- `crates/tool-executor/tests/*`
- `crates/runtime-host/tests/governed_read_bridge.rs`
- adapter permission tests denying write
- path_scope unit tests in `path_scope.rs`

New write suites must not weaken these.

---

## 7. Task-boundary recommendations

### 7.1 C3.0 in-scope (recommended PR slices)

1. **Protocol:** `fs.write@1` types, validation, digests, public errors (`BaseHashMismatch`, `ApprovalRequired`, `ApprovalExpired`, `ApprovalRejected` as needed).
2. **Storage:** tables for write proposals, decisions, apply receipts / extend tool_calls carefully (prefer dedicated tables over overloading read CHECK constraints in `0003_tool_calls.sql` L12).
3. **Executor:** `GovernedWriteToolGateway` apply algorithm + path safety.
4. **Policy:** approval issue/verify/expire (kernel-policy or storage-backed).
5. **Conversation host:** commands `list_pending_write_proposals`, `approve_write`, `reject_write`; wire run registration.
6. **Adapter:** still deny provider-native writes; optionally surface Pillow-normalized write proposals only through host channel.
7. **Renderer:** progressive disclosure card + host-built diff view.
8. **Tests:** matrix §6 + approval-path tests mandatory per AGENTS.

### 7.2 Explicit C3.0 exclusions (unless evidence forces otherwise)

Evidence **supports** exclusion; nothing in the dirty tree requires these for a minimal useful write:

| Excluded | Why |
| --- | --- |
| Rename / delete / directory create/remove | Extra path semantics and undo complexity |
| Binary writes | Encoding/preview/diff unsafe |
| Multi-file transactions / shadow workspaces | Option C cost |
| Provider-trusted patches | Option B cost |
| Terminal / Git / network / MCP arbitrary tools | Roadmap C4+ |
| Credentials / secret file writes | Secret boundary |
| Obsidian vault mutation | Stage 2 |
| New runtimes | Stage D |
| Auto-approve any write | Product Ask-default |
| Deep mkdir -p on create | Directory mutation |

### 7.3 Suggested sequencing inside C3

```text
C3-A  protocol + storage + pure apply (no UI)
C3-B  approval decisions + expiry/replay tests
C3-C  conversation commands + timeline events
C3-D  renderer disclosure + screenshots
C3-E  desktop smoke + restart recovery + independent review
```

Do not open C4 terminal/Git until C3-E accepted.

### 7.4 Non-goals for this research deliverable

- No code changes beyond this document.
- No Keeper product directory writes.
- No Git staging/commits.
- No vault INGEST in this turn (controller may ingest later).

---

## 8. Residual risks

| Residual risk | Severity | Mitigation / accept |
| --- | --- | --- |
| NTFS alternate streams / exotic reparse not covered by attribute bit alone | Medium | Keep component symlink_metadata + reparse bit; add integration fixtures on Windows CI |
| Exclusive file locks by other processes | Low-Med | Opaque failure; retry guidance without path leak |
| Same-volume atomic rename assumptions | Medium | Always write temp in target directory; document no cross-device targets |
| Human approves malicious content | High (inherent) | Diff UX, caps, Ask-default; future allowlists/path policy |
| SQLite stores full file contents | Medium | Caps; optional content GC after terminal; never log content |
| Long approval TTL vs short decision nonce imbalance | Low | Separate proposal TTL from decision submit skew window |
| Event payload size if raw content embedded in run_events | Medium | Digests + truncated previews only in events |
| Shadow/workspace agents later undermine single-file model | Product | Keep C3 contract stable; add Option C as explicit later design |
| No `kernel-policy` crate yet | Process | Create minimal module; avoid putting crypto/approval in React |
| Dirty tree size / uncommitted C2 surface | Process | Land C3 design against checkpoint; avoid mixing unrelated refactors |

---

## 9. Architecture delta summary (read → write)

| Read (B3/C1/C2) | Write (C3 recommended) |
| --- | --- |
| Auto-execute after validate+claim | Stop at pending_approval |
| No human decision object | Durable approve/reject + TTL/replay |
| Output is data to model/UI | Input is data from model; disk mutation after human |
| Receipt is read result | Receipt is apply result + post-hash |
| Provider permission deny still true | Remains deny; writes not unlocked by ACP |
| Resource gateway read-only | Stays read-only; writes via conversation approval commands |

---

## 10. Conclusion

Pillow’s dirty tree already implements a strong **governed read** spine: project-scoped SQLite ownership, portable path grammar, reparse refusal, reservation/replay/conflict, lease claims, secret-safe errors, provider permission deny, and conversation-owned progressive disclosure. C3 should **extend that spine** with a human approval gate and atomic whole-file apply under base-hash CAS, not invent a parallel filesystem authority in the renderer or provider.

**Recommended first release:** Option A whole-file create/modify UTF-8 text + mandatory base-hash CAS + host-built diffs + durable approval/receipt ledger. Defer patch languages and shadow workspaces.

COMPLETION: DONE
