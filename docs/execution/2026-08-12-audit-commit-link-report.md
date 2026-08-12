# Audit commit link execution report

Date: 2026-08-12  
Status: **COMPLETE**

## 1. Delivered interface

- `verdict record --commit GIT_ID` validates the complete value as 7..64 ASCII hexadecimal characters, normalizes it to lower case, and persists one `commit=<id>` field in the verdict summary.
- Recording never invokes Git. Commit existence and prefix uniqueness remain the responsibility of `audit` in its repository context.
- `knowledge-indexer doctor` is the one-command health entry. With no flags it resolves the current repository, `.superpowers/ledger/pillow.db`, project `pillow`, the lexicographically latest portable `docs/handoff/*-pillow-controller-handoff.md`, a 24-hour threshold, and the current UTC time.
- Every `doctor` default can be overridden, including `--now`. The resolved time and its source are printed before the audit report.
- Completed tasks dispatched before the fixed audit-enforcement cutoff `2026-08-12T00:00:00Z` are visibly exempted from the missing-verdict gate. Tasks at or after the cutoff still fail.
- Exit meanings remain disjoint: `0=all checks PASS`, `3=one or more check findings`, `1=tool/argument error`.

## 2. T1 — verdict commit link

### Positive path

The controlled run used repository commit `fc98880646ffb770fc06eb342eeeeac9eb5bbd3f` against a read-only copy of the live ledger:

| Point | `accepted_verdict_commit` evidence |
| --- | --- |
| Before record | `accepted_verdicts=23 verified=0 unrecorded=23 commit_findings=0` |
| `verdict record --commit fc98880646ffb770fc06eb342eeeeac9eb5bbd3f` | `EXIT=0` |
| After record | `accepted_verdicts=24 verified=1 unrecorded=23 commit_findings=0` |

The brief's `22` count was an earlier snapshot; the live ledger copy contained `23` accepted verdicts when this verification began. The controlled addition increased only `verified` and the accepted total by one.

Automated coverage: `verdict_record_commit_flag_is_strict_and_increases_verified_count` asserts the persisted marker and the audit transition to `verified=1`.

### Negative path

- Manual malformed value `abc123` returned `EXIT=1` with `invalid --commit: expected 7..64 hexadecimal characters`.
- A post-rejection audit remained `accepted_verdicts=24 verified=1 unrecorded=23 commit_findings=0`, proving no verdict was written.
- The automated test also rejects `123456g` and a 65-character hexadecimal string; the final verdict count remains one in its isolated database.
- The pre-existing missing-marker semantic remains pinned: missing marker is `PASS` with `unrecorded` visible, while malformed, ambiguous, missing-repository, and ambiguous-prefix claims remain failures.

## 3. T2 — one-command health check

Common form:

```text
knowledge-indexer doctor
```

Optional overrides remain available:

```text
knowledge-indexer doctor [--db PATH] [--project NAME] [--repo DIR] \
  [--doc PORTABLE_PATH] [--now RFC3339] [--stale-after-hours N]
```

The live-ledger-copy run omitted `--now` and printed:

```text
invocation: mode=`doctor` now=`2026-08-12T02:24:33.830625200+00:00` now_source=`system_clock_default` stale_after_hours=24
```

The injected-time run printed:

```text
invocation: mode=`doctor` now=`2026-08-12T02:00:00+00:00` now_source=`--now` stale_after_hours=24
```

`doctor_runs_with_defaults_records_effective_now_and_preserves_exit_codes` additionally parses the default output back as RFC3339, proves that the latest matching handoff is selected, verifies exact `--now` injection, and drives `0`, `3`, and `1` through the same audit execution path.

## 4. T3 — fixed, visible historical exemption

Selected policy: a task in `completed_without_verdict` is historical only when:

```text
dispatched_at < 2026-08-12T00:00:00Z
```

The live-ledger-copy report showed:

```text
completed_without_verdict=0 total_without_verdict=14 historical_exemptions=14 exemption_basis=dispatched_at<2026-08-12T00:00:00Z
document_ledger_findings=1 historical_completed_exemptions=14 exemption_basis=dispatched_at<2026-08-12T00:00:00Z
```

This does not disable either check. The remaining document finding was independently reported as `claimed=0 ledger_dispatched=3`. The boundary test proves `23:59:59Z` is exempt and exactly `00:00:00Z` still makes both `completed_without_verdict` and its document-consistency counterpart fail.

The earlier missing-verdict test was not weakened: its fixture moved to the exact cutoff so it continues to assert the active FAIL direction.

## 5. Exit-code table — executed evidence

Every command used full stdout/stderr redirection and then printed the native exit code; complete logs were read without a `tail` pipeline. Command shape:

```text
COMMAND > LOG 2>&1; echo EXIT=$?
```

| Exit | Meaning | Executed trigger | Observed evidence |
| ---: | --- | --- | --- |
| `0` | All checks PASS | Controlled empty ledger plus portable no-claim input `crates/knowledge-indexer/Cargo.toml` | `AUDIT_PASS_EXIT=0`, overall `PASS`, four checks `PASS` |
| `3` | Check finding | Live ledger copy with latest handoff | `BASELINE_EXIT=3`; overall `FAIL`; `inflight_count_mismatch claimed=0 ledger_dispatched=3` |
| `1` | Tool error | `doctor --now not-rfc3339` | `DOCTOR_ERROR_EXIT=1`; explicit RFC3339 error, no check result masquerading as FAIL |

The malformed `--commit` run independently returned `1`; the doctor integration test independently executes all three codes.

## 6. RED/GREEN and gates

| Evidence | RED | GREEN/final |
| --- | --- | --- |
| T1 commit flag | `EXIT=101`; expected summary marker absent | Focused test `EXIT=0`; persisted marker and `verified=1` |
| T2 doctor | `EXIT=101`; `unknown subcommand: doctor` | Focused test `EXIT=0`; defaults, source, override, and exit map pass |
| T3 cutoff | `EXIT=101`; historical task was still `FAIL` | Focused test `EXIT=0`; strict before/at boundary passes |
| Librarian integration suite | — | `10 passed, 0 failed` |
| `cargo fmt -p keeper-knowledge-indexer -- --check` | Initial mechanical diff `EXIT=1`, then formatted | Fresh final `EXIT=0` |
| `cargo clippy -p keeper-knowledge-indexer --all-targets -- -D warnings` | — | Fresh final `EXIT=0` |
| `cargo test -p keeper-knowledge-indexer` | Baseline `66 passed, 0 failed` | Final `EXIT=0`; `69 passed, 0 failed` |

The final package gate compiles and runs `#[cfg(test)]`; no `cargo check` result was substituted for it.

## 7. Write-boundary audit

This worker changed only:

- `crates/knowledge-indexer/src/bin/knowledge_indexer.rs`
- `crates/knowledge-indexer/src/librarian_audit.rs`
- `crates/knowledge-indexer/tests/librarian_audit.rs`
- `docs/execution/2026-08-12-audit-commit-link-report.md`

Baseline and final status comparison kept the other workers' root manifests, desktop files, runtime-slot files, new role-dispatch crate, and their reports untouched. There was no staging, commit, checkout, reset, or other Git mutation.

Production audit/doctor paths only read the selected document, ledger snapshot, directory names, system time, and local Git history; they expose no document write API. The real ledger and handoff were never modified: all write-path evidence used temporary database copies. No network access was introduced or used.

Temporary-copy cleanup: **ENV-BLOCKED**. Two generated system-temporary directories containing the controlled database copies and logs were resolved and verified inside the temporary root, but the sandbox rejected both dynamic and exact recursive deletion. No repository path was targeted, and no deletion workaround was attempted.

## 8. Residual risks

1. Commit linkage is still encoded in `summary` because the durable verdict schema lives outside this task's write boundary. A future structured field must update every host DTO and closed renderer allowlist together.
2. Shape-valid does not mean repository-valid at record time by design. Audit later determines local existence and unique prefix matching; it does not prove that the commit contains the accepted change.
3. The task schema has no completion timestamp, so the fixed exemption uses `dispatched_at`. A task dispatched before the cutoff but completed later is also exempt; the printed basis makes that limitation observable.
4. `doctor` selects the lexicographically latest matching handoff filename. A naming-convention change or missing default ledger/document produces tool error `1`; explicit flags remain the escape hatch.
5. The live snapshot still has a real handoff/ledger mismatch (`claimed=0`, `ledger_dispatched=3`), so the current one-command audit correctly exits `3` even though the historical-verdict and commit-marker checks pass.
6. Opening SQLite retains the storage layer's existing migration behavior. Verification used copies so the source ledger stayed unchanged; a byte-for-byte read-only audit would require a storage-level read-only open API outside this scope.
7. Desktop smoke was not run: no UI file was changed, desktop paths were owned by a parallel worker, and the brief's package-level Rust test was the explicit closeout gate.

## Completion signal

`AUDIT-COMMIT-LINK-COMPLETE`
