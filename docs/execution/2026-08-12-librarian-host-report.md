# Librarian host command execution report

## Scope and outcome

Execution is in progress. This report records only the host-side wiring for the
existing librarian audit contract.

## Verification exit codes

| Command | Exit code | Result |
| --- | ---: | --- |
| Focused host tests | pending | Not run yet. |
| `node scripts/run-cargo.mjs test -p pillow-desktop --lib` | pending | Not run yet. |
| `cargo test -p keeper-knowledge-indexer` | pending | Not run yet. |

## Command registration evidence

Execution evidence will be recorded after implementation and verification.

## DTO key alignment

The final report will list every returned key and its renderer allowlist match.

## Missing-input error evidence

The final report will record separate evidence for a missing ledger, an invalid
handoff document path, and unavailable Git metadata, including proof that none
can be returned as an all-PASS report.

## Explicit audit time evidence

The final report will record how the host-provided time appears in the returned
result.

## Write-boundary audit

The final audit will enumerate only files changed within the authorized host,
indexer, and report paths.

## Residual risks

Residual risks will be recorded after the specified gates finish.
