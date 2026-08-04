# Wave C3-E Controller Final Fix Brief

Owner: Grok implementation worker

Controller session: `<keeper-session>`

Status: ACTIVE — narrow final correction before controller acceptance.

## Finding 1: Existing Error Instances Can Bypass Fixed Copy

`normalizeWriteApprovalError` currently returns an incoming
`WriteApprovalGatewayError` unchanged. A gateway implementation can therefore
construct a known code with a secret-bearing arbitrary message; ResourcePane
will render that message. Normalize instances exactly like object-shaped native
errors: keep a known code but replace its message with the fixed renderer copy;
unknown/invalid codes become fixed unavailable. Never echo the incoming
message.

Add an adversarial diff/list/decision regression using a
`WriteApprovalGatewayError` whose message contains a distinctive grant/token;
prove it is absent from DOM and console.

## Finding 2: Diff Counts Must Describe The Reviewed Text

The native `BoundedDiff` defines `byteCount` from the UTF-8 byte length of
`unified` and `lineCount` from the returned bounded text. The Tauri parser only
checks independent upper bounds, so a malformed DTO can display misleading
review counts. Require:

- `byteCount === TextEncoder(unified).length`;
- `lineCount` equals the executor's line-count rule for the exact returned
  text;
- existing 262144-byte / 4000-line caps continue to apply.

Read the executor's `count_lines` implementation and mirror that exact pure
rule. Add mismatch regressions including empty text, final newline and Unicode.

## Whitelist And Gates

Modify only:

- `apps/desktop/src/lib/writeApprovalGateway.ts`
- `apps/desktop/src/lib/tauriWriteApprovalGateway.ts`
- `apps/desktop/tests/write-approval-gateway.test.ts`
- `apps/desktop/tests/write-approval-workbench.test.tsx` only if needed for the
  DOM/console regression
- `docs/execution/2026-08-01-wave-c3e-approval-ui-report.md`

Run focused C3-E tests, full desktop Vitest, typecheck, production UI build,
diff/staging checks and cleanup. No Rust/native edits, stage or commit. Update
the report and end with `COMPLETION: DONE` or `COMPLETION: BLOCKED`.
