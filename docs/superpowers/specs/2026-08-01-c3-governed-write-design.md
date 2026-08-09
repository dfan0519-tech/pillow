# Pillow C3 Governed Write / Diff / Approval Design

Date: 2026-08-01

Status: accepted by delegated controller authority; implementation authorized;
no staging or commit authorized

## 1. Decision

C3.0 adds one governed write capability:

> A provider may propose creation or modification of one bounded UTF-8 text
> file inside the owning project. Pillow seals the exact proposal in the active
> host coordinator, persists its identity and digests, computes the base
> snapshot and review diff, waits for one explicit human decision, and applies
> the sealed content only if the target still matches the captured base hash.

The controller compared three approaches after three independent Grok research
passes:

| Approach | Decision | Reason |
| --- | --- | --- |
| Whole-file content + base-hash CAS | Adopt | Smallest auditable object; no patch interpreter; clear approval identity |
| Provider patch + base-hash CAS | Defer | Adds adversarial patch parsing and dual patch/result identity |
| Shadow workspace transaction | Defer | Adds multi-file isolation, cleanup and crash-commit complexity |

## 2. Research And Provenance

The design uses local evidence from:

- Grok App at fixed commit
  `cbbbdf17bb8db1588b6707f3f42f6b00397e27f9`, MIT;
- zbbody at inspected commit
  `00c293414333b6fcc4e4598389fa3952c8d0b09b`, Apache-2.0;
- the accepted Pillow B3/C1/C2 dirty-tree checkpoint;
- frozen Keeper approval invariants under
  `legacy Keeper (out of tree)\lib\operator-approval-authority.js`.

Grok App and zbbody remain conceptual interaction references. C3 copies no
upstream application source. Any later direct adaptation must add commit,
source path, license and change notice to `THIRD_PARTY_NOTICES.md`.

Research artifacts:

- `docs/research/2026-08-01-c3-grok-app-interaction-research.md`
- `docs/research/2026-08-01-c3-zbbody-interaction-research.md`
- `docs/research/2026-08-01-c3-governed-write-security-research.md`

## 3. Product Scope

### 3.1 Included

- provider-neutral `fs.write@1` proposal;
- exactly one portable project-relative path per proposal;
- `create` for a missing file whose parent already exists;
- `modify` for an existing regular UTF-8 file;
- proposed content at most 262,144 UTF-8 bytes;
- host-captured base content and SHA-256 digest;
- host-generated bounded unified diff;
- one explicit `approve_once` or `reject` decision;
- ten-minute proposal expiry;
- base-hash compare-and-swap immediately before apply;
- same-directory temporary file, flush and atomic replacement;
- durable proposal, transition, decision and receipt evidence in SQLite;
- conversation-owned pending state, diff review and terminal result;
- restart recovery that invalidates every non-terminal approval.

### 3.2 Excluded

- allow-for-session or auto-approve writes;
- rename, delete, move, copy, chmod or directory mutation;
- binary or non-UTF-8 writes;
- multi-file transactions or shadow workspaces;
- provider-authored diffs or patch languages;
- direct editor save outside the governed proposal path;
- terminal, process execution, Git, network, MCP, credentials or key files;
- Obsidian and company-operation features;
- Codex/OpenCode or other runtime expansion.

## 4. Authority Boundaries

```text
Provider child
  -> proposes call id, path, mode and complete UTF-8 content
Adapter
  -> validates fixed method/schema; supplies no project root or approval
Runtime host
  -> injects project/conversation/run identity and captures base snapshot
SQLite
  -> stores immutable proposal metadata/digests, expiry and transitions
Conversation UI
  -> renders host DTO; submits proposal id + opaque one-time grant + decision
Kernel policy
  -> verifies ownership, expiry, exact fingerprint and single use
Write executor
  -> verifies sealed payload against stored metadata, revalidates and applies
SQLite + run events
  -> persist terminal receipt before UI/provider completion is emitted
```

The provider can never approve. The renderer never supplies replacement
content, base content, digests, roots or native paths to the apply command.
`resourceGateway` remains read-only.

## 5. Typed Contract

### 5.1 Provider method

Method: `pillow/write_tool`

```json
{
  "sessionId": "active ACP session id",
  "callId": "provider call identity",
  "toolId": "fs.write",
  "toolVersion": 1,
  "arguments": {
    "path": "src/example.ts",
    "mode": "create",
    "content": "complete UTF-8 content"
  }
}
```

Only these keys are accepted. `mode` is `create` or `modify`. `content` must
contain no NUL and must encode to at most 262,144 bytes.

### 5.2 Host-enriched proposal

```rust
pub struct ValidatedWriteProposal {
    proposal_id: WriteProposalId,
    project_id: String,
    conversation_id: String,
    run_id: RunId,
    call_id: String,
    path: String,
    mode: WriteMode,
    proposed_content: String, // sealed active-host memory only
    proposed_digest: ContentDigest,
    base_content: Option<String>,
    base_digest: Option<ContentDigest>,
    proposal_fingerprint: WriteProposalFingerprint,
    expires_at: DateTime<Utc>,
}
```

For `create`, the target must be absent and `base_*` are `None`. For `modify`,
the target must be a regular UTF-8 file within the same 262,144-byte cap and
`base_*` are mandatory. The host, not the provider, captures `base_*`.

### 5.3 Lifecycle

```rust
pub enum WriteProposalState {
    PendingApproval,
    Approved,
    Applying,
    Succeeded,
    Rejected,
    Expired,
    Cancelled,
    Interrupted,
    Failed,
}
```

Allowed transitions:

```text
pending_approval -> approved -> applying -> succeeded | failed
pending_approval -> rejected | expired | cancelled | interrupted
approved         -> expired | cancelled | interrupted | applying
applying         -> succeeded | failed | interrupted
```

Terminal states never reopen. The same project/conversation/run/call identity
with the same fingerprint replays terminal evidence; a different fingerprint
conflicts.

### 5.4 Approval command DTO

```ts
type WriteApprovalDecision = "approve_once" | "reject";

type WriteProposalView = {
  proposalId: string;
  conversationId: string;
  path: string;
  mode: "create" | "modify";
  state: WriteProposalState;
  summary: string;
  expiresAt: string;
  approvalGrant?: string;
  errorCode?: WritePublicErrorCode;
};

type WriteDiffView = {
  proposalId: string;
  path: string;
  mode: "create" | "modify";
  unified: string;
  truncated: boolean;
  lineCount: number;
  byteCount: number;
};
```

The apply command accepts only `conversationId`, `proposalId`,
`approvalGrant`, and `decision`. Native code reloads every authoritative field.
Diff content is fetched separately with `conversationId` and `proposalId`; it
is available only while that exact proposal remains live in host memory.

## 6. Storage

Migration `0005_write_proposals.sql` adds dedicated tables. Historical B3
migrations are not rewritten.

`write_proposals` stores immutable identity, portable path, mode,
proposed/base digests and byte lengths, fingerprint, state, expiry, terminal
receipt and timestamps. It never stores proposed or base content.
`write_proposal_transitions` is append-only. `write_approval_decisions` stores
one decision per proposal, the consumed grant digest, decision, proposal
fingerprint and timestamp.

The active runtime coordinator holds the bounded proposed/base UTF-8 payloads
and the raw approval grant in memory. It verifies their digests against SQLite
before diff or apply. Losing that memory makes the proposal `interrupted`; this
is intentional and prevents source or secret-bearing content from becoming a
second durable content store.

Required database invariants:

- foreign keys bind project -> conversation -> run;
- unique `(project_id, conversation_id, run_id, call_id)`;
- one active pending proposal per run;
- one decision per proposal;
- one use per approval-grant digest;
- terminal receipt required for succeeded/failed apply outcomes;
- no raw approval grant is stored;
- proposed/base content never enters SQLite, run event payloads or logs.

## 7. Approval Policy

- Ask is mandatory for every write.
- A proposal expires ten minutes after durable creation.
- An opaque 256-bit grant is created in host memory; only its SHA-256 digest is
  stored. Restart destroys the raw grant and recovery marks non-terminal rows
  `interrupted`.
- The decision command checks conversation ownership, proposal state, expiry,
  grant digest and proposal fingerprint in one SQLite transaction.
- A repeated or conflicting decision returns an opaque conflict and cannot
  trigger a second apply.
- Rejection and expiry are terminal receipts, not deletion of history.

## 8. Write And Diff Execution

### 8.1 Path policy

Reuse the B3 portable grammar: no absolute, UNC, verbatim, backslash, colon,
dot/parent, empty, control, Windows-device or trailing-dot/space components.
Reject symlink/reparse components on every resolution.

C3 additionally denies writes beneath `.git`, `.pillow`, `.keeper`, `.ssh`,
and denies credential-like leaf names `.env`, `.env.local`, `id_rsa`,
`id_ed25519`, plus extensions `.pem`, `.key`, `.p12`, `.pfx`, `.jks`, and
`.keystore`. `.env.example` remains allowed.

### 8.2 Diff

The host computes a deterministic line-oriented unified diff from stored base
and proposed content. Display output is capped at 262,144 bytes and 4,000
lines, with an explicit truncation marker. The provider never supplies the
review diff.

### 8.3 Apply

1. Reload immutable proposal metadata and the accepted decision from SQLite,
   then verify the sealed in-memory payload against every stored digest.
2. Recheck project capability, portable path and denied path policy.
3. Re-resolve every existing component and reject reparse points.
4. Re-read target: create still absent; modify still regular UTF-8 and its
   SHA-256 equals the captured base digest.
5. Create a random temporary file in the target directory using `create_new`.
6. Write the stored proposed bytes, flush and `sync_all`.
7. Re-run path/base checks immediately before replacement.
8. Create uses same-directory rename; modify uses an atomic Windows replace
   primitive and an equivalent atomic rename on supported non-Windows hosts.
9. Clean the temporary file on every pre-replace failure.
10. Persist the post-write digest and terminal receipt before emitting success.

If another local process races path replacement between the final check and
the OS atomic operation, Pillow fails closed where the OS exposes the race.
C3 is not an operating-system sandbox against a hostile administrator.

## 9. Runtime And Recovery

The adapter continues to reject provider-native write permission grants.
Only `pillow/write_tool` enters C3.

The active prompt emits a durable `write_proposal_requested` event, then waits
on a host-owned condition with cancellation and expiry checks. Approval applies
the proposal and wakes the exact owning run. Rejection, timeout and cancellation
return fixed opaque provider errors. Other conversations may continue.

On restart, `pending_approval` and `approved` rows become `interrupted`; no
proposal auto-applies. An `applying` row is reconciled without writing: target
digest equal to the proposed digest becomes recovered `succeeded`; target still
equal to the base (or still absent for create) becomes `interrupted`; any other
state becomes opaque `failed` evidence. Terminal receipts remain replayable.

## 10. UI And Interaction

Conversation remains dominant:

```text
timeline summary -> single approval bar -> contextual diff pane -> terminal row
```

- One active approval bar appears above the composer for the selected
  conversation; no permanent card stack.
- The bar shows portable path, create/modify label, expiry and Approve/Reject.
- Opening the diff uses the existing collapsible resource pane.
- Escape rejects only when the approval bar owns focus; Tab is trapped within
  the two actions; the primary action receives initial focus.
- Switching conversations parks the approval; returning rehydrates it from
  native state. Foreign proposals never render.
- After decision, the bar disappears and the owning timeline retains a compact
  terminal outcome. Success offers `Open file`; errors are fixed, path-free
  public messages.
- No allow-session, YOLO, raw JSON, raw content, native path or debug ID appears.

## 11. Acceptance

C3 is accepted only when all of the following pass:

1. protocol validation, ownership, replay/conflict and public-error tests;
2. approval expiry, rejection, replay, cross-conversation and restart tests;
3. create/modify, base drift, reparse, denied path, cancellation, concurrent
   decision and atomic-failure tests;
4. real adapter proposal -> UI approval -> disk mutation -> provider receipt;
5. two-project isolation and restart E2E with no auto-apply;
6. desktop screenshots at supported Windows sizes;
7. `pnpm check`, production UI build, `cargo fmt --all -- --check`,
   `git diff --check`, complete desktop E2E and process cleanup;
8. independent Codex2 review reports Critical 0 and Important 0;
9. controller independently reruns the final gates and records a checkpoint.

## 12. Working-Tree Governance

Baseline HEAD remains `7b7f70f`. C1/B3/C2 are an accepted unstaged dirty-tree
checkpoint. C3 workers receive non-overlapping file white-lists. No reset,
discard, staging or commit is permitted. `legacy Keeper (out of tree)` remains
frozen and read-only.
