# Librarian monitoring surface execution report

## Scope and outcome

Implemented the renderer-only project health surface in Settings. Opening the
surface starts a project-scoped audit; the user can run it again; a new run
clears the prior report before waiting for the host. The health capability is
independent of runtime-role configuration, while the same surface names the
configured information-manager slot when one exists.

No host, native crate, or desktop-host file was changed. The required host
command is still absent and is specified in full below for controller wiring.

## Verification exit codes

| Command | Exit code | Result |
| --- | ---: | --- |
| `corepack pnpm typecheck` | 0 | PASS — desktop `tsc --noEmit` completed. |
| `corepack pnpm --filter @pillow/desktop test` | 0 | PASS — 48 files and 579 tests passed. |

The focused renderer/contract run passed 20 of 20 tests across the two new
test files before the full desktop suite.

## T1: health panel evidence

Focused suite: `apps/desktop/tests/librarian-monitor-section.test.tsx`.

- `renders all four PASS checks with evidence and no failure affordance` binds
  each of the four visible PASS states to its own label and evidence summary.
  Its row-level negative controls prove there is no FAIL label or findings
  expander on any all-pass row.
- `renders FAIL evidence as expandable findings without weakening PASS rows`
  proves a FAIL row, two expandable findings, and a simultaneous PASS row. Its
  negative controls prove the failed row is not labelled PASS, the passed row
  has no findings affordance, and no invented finding is rendered.
- `shows no invented health status when no host audit gateway is available`
  proves that an unavailable host produces neither PASS nor FAIL and disables
  rerun instead of inventing an all-green report.

## T2: manual rerun evidence

- `clears the previous report while a manual rerun is in progress and then
  refreshes` proves the button changes to `检查中…`, is disabled, and removes all
  prior PASS/evidence content before the next report resolves. The new FAIL
  report appears only after completion, and the old summary remains absent.
- `reports a rerun failure without showing the previous result or host detail`
  proves a fixed failure message, zero stale PASS/FAIL rows, and no host-detail
  echo after the second call rejects.
- `does not let a late report from the previous project replace the current
  result` has mutation evidence: removing the request-generation checks made
  the focused test fail; restoring them made it pass.

## T3: librarian responsibility evidence

- `names the configured librarian as responsible and omits setup guidance`
  proves `由 资料守望者 负责` and proves the setup prompt and slot identifier are
  absent.
- `guides librarian setup while keeping the built-in audit available` proves
  the no-role guidance, four PASS rows, an enabled rerun button, and one initial
  automatic audit. Its negative control proves no configured-owner claim is
  shown.
- `keeps audit results visible when responsibility lookup fails` proves role
  lookup failure is not misreported as “no role,” does not echo host detail,
  and does not suppress the four audit results.

## T4: host command contract

Focused suite: `apps/desktop/tests/librarian-audit-gateway.test.ts`.

Command: `run_librarian_audit`.

Input is one exact camel-case object:

| Key | Type | Contract |
| --- | --- | --- |
| `projectId` | string | Required project identity. This is the only key. The renderer never sends a root, handoff path, ledger path, executable, or argument vector. |

Successful return is one `LibrarianAuditReportDto` with this exact key set:

| Key | Type | Contract |
| --- | --- | --- |
| `projectId` | string | Must equal the requested project. |
| `outcome` | `pass` or `fail` | Must equal the aggregate of the four check statuses. |
| `checkedAt` | RFC 3339 string | Completion time for this run. |
| `checks` | `LibrarianAuditCheckDto[4]` | Exactly four distinct required checks; no missing or duplicate item. |

Each `LibrarianAuditCheckDto` has exactly these keys:

| Key | Type | Contract |
| --- | --- | --- |
| `id` | enum | One each of `inflight_handoff_consistency`, `pending_acceptance_visibility`, `handoff_ledger_references`, and `inflight_inventory`. |
| `status` | `pass` or `fail` | PASS requires no findings; FAIL requires at least one finding. |
| `summary` | string | Required bounded, user-readable evidence summary. |
| `findings` | `LibrarianAuditFindingDto[]` | Empty for PASS; one or more entries for FAIL; maximum 256. |

Each `LibrarianAuditFindingDto` has exactly these keys:

| Key | Type | Contract |
| --- | --- | --- |
| `code` | lower snake-case string | Stable finding category, maximum 64 characters. |
| `summary` | string | Required bounded, user-readable item text. |

Normal audit findings are a successful IPC response even when `outcome` is
`fail`. Failure to execute or read authoritative project inputs must reject the
command instead of returning fabricated PASS checks. The host must map the
existing audit-success and findings outcomes to `pass` and `fail`; other tool
failures are IPC errors.

For structured IPC failures, the renderer recognizes these public `code`
values: `librarian_audit_project_unavailable`, `librarian_audit_unavailable`,
and `librarian_audit_failed`. Unknown rejection shapes become the generic
failed code. `invalid_librarian_audit_response` is renderer-owned and means the
successful payload failed contract validation; it is not a host outcome.

All three response levels use exact-key validation. The renderer also rejects
project mismatch, invalid time, missing/duplicate checks, inconsistent
aggregate status, PASS-with-findings, FAIL-without-findings, host absolute
paths, private run-identifier labels, and credential-shaped evidence. Host
error messages are replaced with fixed public copy and are never logged.

## Write-boundary audit

- Modified: `apps/desktop/src/features/settings/SettingsPanel.tsx`.
- Added renderer modules:
  `apps/desktop/src/features/settings/LibrarianMonitorSection.tsx`,
  `librarianAuditGateway.ts`, and `librarianAuditTypes.ts` in the same folder.
- Added style: `apps/desktop/src/styles/librarian-monitor.css`.
- Added tests: `apps/desktop/tests/librarian-audit-gateway.test.ts` and
  `apps/desktop/tests/librarian-monitor-section.test.tsx`.
- Added this report only under `docs/execution/`.
- No staged files and no Git write operation. No renderer network call,
  browser-storage write, console logging, credential field, or native-path
  input was added.
- Concurrent changes under `crates/**`, `apps/desktop/src-tauri/**`, and other
  execution reports belong to other workers and were not modified by this
  task.

## Residual risks

1. `run_librarian_audit` is not registered by the current host. Until the
   controller wires the command exactly as specified, a real desktop run will
   show an explicit failed-run state and no prior or invented result.
2. The renderer rejects unsafe evidence, but the host still owns selection of
   the authoritative handoff and ledger and must emit only privacy-safe
   summaries.
3. Per the execution contract, worker gate, desktop smoke, screenshots, and
   E2E were not run. The controller retains those acceptance steps after host
   wiring.

LIBRARIAN-SURFACE-COMPLETE
