# C4 D3B1 approval arbitration report

## Changed files

- `apps/desktop/src/features/conversations/approvalArbitration.ts`
  - Adds a pure, project/conversation-scoped selector over display-safe write
    and process candidates.
  - Selects available decisions by oldest valid expiry, then opaque request id;
    exact expiry/id cross-domain ties select write.
  - Projects a fresh renderer model containing only domain, opaque request id,
    conversation id, expiry, summary, and risk.
- `apps/desktop/tests/approval-arbitration.test.ts`
  - Covers ordering, both tie breakers, project/conversation isolation, empty
    inputs, unavailable decisions, invalid expiry fail-closed behavior, input
    immutability, and authority-free projection.

## Verification performed

From `apps/desktop`:

```text
npm run test -- --run tests/approval-arbitration.test.ts
# 1 test file passed; 7 tests passed

npm run typecheck
# passed
```

## Controller test plan

Run the focused test and typecheck commands above before integration. Once the
controller wires the selector to concrete write and process projections, add
the integration coverage there and run the relevant workbench/desktop suite;
this slice intentionally mounts no UI and changes no gateway or authority
boundary.

## Risks and boundaries

- Invalid expiry timestamps deliberately receive no selection because they
  cannot be safely ordered as a decision deadline.
- This is an unwired model only. Gateway normalization, approval authority,
  and UI mounting remain unchanged and require separate integration work.
- Pre-existing or concurrent native bridge changes and unrelated execution
  briefs remain untouched.

## Repository status

- HEAD: `main` (no commit created)
- Staging: clean; no files staged
- Working tree contains pre-existing or concurrent native-process bridge and
  unrelated changes outside this slice, plus the three allowlisted D3B1
  arbitration files above.

COMPLETION: complete
