# Wave C3-E Report — Conversation Approval And Diff UI

Owner: grok3-c3e-final (implementation worker)
Controller session: `<keeper-session>`
Worker session (assigned): `<keeper-session>`
Bridge boot session: `<keeper-session>`
Allowed lease: `<keeper-lease>` on `grok3`
Date: 2026-08-01
Task: C3-E conversation-owned approval bar + governed-write diff UI — **controller final correction**

## Scope

Show one conversation-owned approval bar above the composer, open the
host-generated bounded diff in the existing contextual resource pane, submit
an exact one-shot decision, and retain a compact terminal outcome in the
owning timeline. No Rust/native edits. No stage/commit.

This report supersedes prior delivery status: controller **final-fix** findings
are closed below. Worker completion is not controller acceptance.

## Interaction delivered

| Contract | Implementation |
| --- | --- |
| One pending bar per selected conversation | `selectOldestPendingProposal` (expiresAt, then proposalId); foreign filtered |
| Portable path / create·modify / expiry / Approve once / Reject / Review diff | `WriteApprovalBar` |
| Initial focus Approve; Tab cycles actions; Escape rejects only while bar owns focus | Focus trap + focus ownership check; does not call global cancel |
| Disable after submit; reconcile via re-list | Grant cleared after settle; bar parks until native list returns |
| No polling | Re-list on conversation entry + `writeProposalRefreshToken` from accepted `write_proposal_*` events only |
| Stale async suppression | Project/conversation generation guards on list, decide, and diff |
| Grant memory only | `grantByProposalRef` + bar `grantRef`; never DOM / log / storage / snapshot |
| Atomic grant replace on same-scope list | Clear map; retain only selected pending grant; clear bar+diff when none |
| Diff in contextual pane | `WriteDiffPreview` via `writeDiffTarget` on `ResourcePane`; truncation + counts |
| Expiry disables all three actions + closes diff + re-lists | Bar + Workbench `handleExpiryDeadline` |
| Terminal timeline row | `write_proposal_resolved` → activity projection; success-only `打开文件` through resource gateway |

## Controller final-fix findings — closed

### 1. Existing Error instances can bypass fixed copy — FIXED

**Was:** `normalizeWriteApprovalError` returned an incoming
`WriteApprovalGatewayError` unchanged. A gateway implementation could construct
a known code with a secret-bearing arbitrary message; `ResourcePane` would
render that message.

**Fix:** Normalize instances exactly like object-shaped native errors:

1. Keep a known gateway code (`HostWriteApprovalErrorCode` or
   `invalid_write_response`).
2. Replace `message` with the fixed renderer copy for that code.
3. Never echo the incoming message.
4. Shared fixed invalid-response copy is exported as
   `FIXED_INVALID_WRITE_RESPONSE_MESSAGE`.

**Regressions:**

- Gateway:
  `rewrites existing WriteApprovalGatewayError instances to fixed copy and never echoes their message`
- Workbench / DOM+console:
  `normalizes WriteApprovalGatewayError instances so secret messages never reach DOM or console`
  (adversarial instance carries grant + distinctive token; fixed
  `Write proposal is unavailable.` in alert; secret absent from body text/HTML
  and console spies)

### 2. Diff counts must describe the reviewed text — FIXED

**Was:** Tauri parser only checked independent upper bounds
(`byteCount ≤ 262144`, `lineCount ≤ 4000`, UTF-8 unified ≤ 262144). A malformed
DTO could display misleading review counts.

**Fix:** After caps still apply, require:

- `byteCount === TextEncoder().encode(unified).length`
- `lineCount` equals the executor pure rule mirrored from
  `crates/tool-executor/src/write_diff.rs` `count_lines`:
  - empty → `0`
  - count `\n` byte terminators
  - text without a trailing newline still has one logical line

**Regressions:**

- Gateway:
  `rejects diffs whose byteCount or lineCount do not describe the reviewed text`
  (empty mismatch, final-newline mismatch, Unicode char-vs-UTF-8 byte
  mismatch; matching empty / final-newline / Unicode accept)
- Accept-path DTO fixture counts corrected to exact reviewed text

## Prior controller rejection findings — still closed

1. Raw grants outlive live proposal — atomic grant replace
2. Expiry disables all three actions + closes governed diff + re-lists
3. Error/log boundary fixed-copy for object-shaped host errors + no secret logging
4. Fail-closed DTO parsing (grant/state/error/timestamp/path/caps)
5. Whitelist reconciliation (redundant `workbench.css` import removed earlier)

## Changed files (this final correction only)

### Modified (whitelist only)

| Path | Change |
| --- | --- |
| `apps/desktop/src/lib/writeApprovalGateway.ts` | Rewrite `WriteApprovalGatewayError` instances to fixed copy; export fixed invalid-response message |
| `apps/desktop/src/lib/tauriWriteApprovalGateway.ts` | Require `byteCount`/`lineCount` match reviewed text via UTF-8 + executor `count_lines` |
| `apps/desktop/tests/write-approval-gateway.test.ts` | Instance rewrite + count mismatch/match regressions; fixture byte counts corrected |
| `apps/desktop/tests/write-approval-workbench.test.tsx` | Adversarial instance DOM/console regression for diff open path |
| `docs/execution/2026-08-01-wave-c3e-approval-ui-report.md` | This report |

### Not touched

- Any Rust / `src-tauri` native command source
- Old Keeper / unlisted product files
- Stage / commit / package / publish
- Dirty-tree accepted C1/C2/C3 work outside this whitelist

## TDD evidence

### RED → GREEN (final-fix contracts)

1. Instance rewrite: existing `WriteApprovalGatewayError(code, secretMessage)`
   no longer passes through; message becomes fixed copy.
2. Diff counts: independent upper-bound-only DTO rejected when counts do not
   describe `unified` (empty, trailing newline, multi-byte Unicode).

### GREEN

```text
pnpm exec vitest run \
  tests/write-approval-gateway.test.ts \
  tests/write-approval-workbench.test.tsx
→ 2 files, 28 tests passed
```

Focused C3-E suite (including prior rejection coverage) remains green under
full desktop Vitest.

## Gates

| Gate | Result |
| --- | --- |
| Focused Vitest (gateway + workbench final-fix) | **28/28 PASS** |
| Complete desktop Vitest | **243/243 PASS** (21 files) |
| `tsc --noEmit` | **PASS** |
| `pnpm run build:ui` | **PASS** (vite production build) |
| `git diff --check` (whitelist) | **PASS** (exit 0) |
| Cached-diff / staging | Staging empty; no stage/commit |
| Residual processes | No residual pillow-targeted vitest/tsc node processes after gates |
| Screenshots | Not produced in this worker run (unit/integration only) |

## Security notes

- Renderer never sends root, content, base, digest, or fingerprint fields.
- `approvalGrant` is stored only in component/ref memory for the exact selected
  pending proposal after each authoritative list; sent only as the decide
  argument; cleared after settle and on scope switch / empty list.
- Grant asserted absent from DOM text/attributes and console spies.
- Host error **messages** are never accepted into renderer `Error.message`,
  including when the thrown value is already a `WriteApprovalGatewayError`.
- Diff text is for human review only; not logged.
- Diff metadata counts cannot lie about the reviewed `unified` payload.
- Terminal non-success copy is fixed and path-free; proposal ids never render.

## Risks / follow-ups

1. C3-F must drive real fake-ACP → UI approval → disk mutation E2E; this wave
   is renderer-only against the frozen native DTO.
2. Expiry UI disables and re-lists; native remains the only authority that marks
   `expired` — correct by design.
3. Tauri boundary rejects non-canonical grants and count mismatches at parse
   time; fixture grants use exact 43-char base64url; fixture diffs use exact
   UTF-8/`count_lines` counts.
4. Worker completion is not controller acceptance.

## COMPLETION: DONE
