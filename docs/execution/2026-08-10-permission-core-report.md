# Pillow Permission Core Minimal Slice Execution Report

Date: 2026-08-10  
Scope: permission-rules design section 10.1 only  
Status: `PERMISSION-CORE-BLOCKED: BLOCKED-BY-CONCURRENT - the required worker gate is stopped by the parallel renderer worker's event-payload type error`

## 1. Scope delivered

Implemented the requested core slice:

- actions: `allow`, `deny`;
- prefixes: `process`, `write`, `edit`;
- migration `0012`, SQLite CRUD, global rules, and project deny-only rules;
- pure rule matching with deny-over-allow precedence;
- host evaluation for durable pending write and process proposals;
- atomic rule decisions and persistent receipts containing the complete matched-rule snapshot;
- exact Tauri CRUD command names and `PermissionRuleView` shape;
- `rule_subject` and `auto_approved_by_rule` on write/process proposal decision views.

Not implemented, by scope: Read/Grep rules, `ask`, remembered authorization, always-approve mode, sandboxing, MCP/WebFetch, shell-chain interpretation, Grok configuration import, or renderer UI.

## 2. Report-first and live-state evidence

- This report existed as an explicitly unfinished skeleton before the first product-code edit.
- Initial observation: branch `main`, HEAD `9b2abba47bf63dca28b4af073ca2338bbc22cf46`, worktree and staging clean; the skeleton was then the first untracked path.
- Final staging remains empty. No commit, add, checkout, reset, or other Git write was performed.
- Renderer paths and unrelated execution/research documents that appeared later belong to parallel work and were preserved without editing by this worker.

## 3. Storage schema and CRUD

Migration `0012_permission_rules.sql` is append-only after `0011`; no historical migration was changed.

`permission_rules` stores:

| Column | Purpose |
| --- | --- |
| `rule_id` | UUID primary key |
| `scope`, `project_id` | `global` with no project, or `project` with one registered project |
| `prefix` | `process`, `write`, or `edit` |
| `pattern` | bounded rule pattern, at most 4,096 bytes |
| `action` | `allow` or `deny` |
| `content_digest` | canonical `sha256:` digest of the versioned complete rule body |
| `created_at` | RFC 3339 creation time |

The migration also adds one-to-one `write_permission_rule_receipts` and `exec_permission_rule_receipts` extension tables. They reference the existing C3/C4 decision rows by proposal id, but intentionally do not reference the live rule row: deleting a rule cannot erase historical attribution.

CHECK constraints cover shape, enum membership, digest shape, bounds, and scope/project-id shape. Semantic authority validation remains in Rust. The repository exposes typed add/list/delete operations, restart persistence, and effective loading of global rules plus only the current project's denies. The Tauri layer exposes exactly:

```text
list_permission_rules(scope, project_id) -> Vec<PermissionRuleView>
add_permission_rule(scope, project_id, prefix, pattern, action) -> PermissionRuleView
delete_permission_rule(rule_id) -> ()
```

`PermissionRuleView` serializes the required snake-case fields: `rule_id`, `scope`, `project_id`, `prefix`, `pattern`, `action`, `created_at`.

## 4. Evaluation placement and ordering

The pure matcher lives in `kernel-policy`; SQLite remains the durable authority; `runtime-host` performs orchestration. This keeps the desktop shell free of agent-domain policy while letting the host combine structural policy, proposal lifecycle, evaluation, execution, and receipts.

The order is:

1. validate the provider proposal and all existing structural policy;
2. create the existing durable `pending_approval` proposal;
3. load the effective rules from Pillow's SQLite store and evaluate the canonical subject;
4. expose/emit the requested proposal lifecycle;
5. on a rule match, atomically revalidate the exact live rule snapshot, write the existing C3/C4 decision plus the rule receipt, and continue through the existing apply/spawn or reject path;
6. on no match, leave the proposal on the existing human confirmation path.

Rule evaluation does not create a second execution path and does not replace the existing grants. The old decision vocabulary remains `approve_once` / `reject`; the new one-to-one receipt records that the actor was `rule_engine` and whether the rule decision was `approve_by_rule` or `reject_by_rule`.

Any rule-store or receipt failure fails closed. If a rule changes or is deleted between evaluation and decision, exact snapshot revalidation rolls the decision back instead of executing with stale authority.

## 5. Pinned glob algorithm

This is a Pillow-defined deterministic wildcard language, documented and tested here. It does **not** claim bit-level compatibility with Grok.

- Matching is case-sensitive on every platform.
- Only `*` and `**` are operators. `?`, bracket expressions, and backslashes are literal characters.
- The implementation is a dynamic-programming whole-string matcher; it does not use regex and has no exponential backtracking.
- An empty pattern is the bare prefix and matches every subject in that same prefix class.
- Prefix class must match before pattern matching.
- Process matching succeeds on either literal character-prefix matching or full wildcard matching. Any run of stars matches any character sequence, including spaces and `/`; there is deliberately no word boundary.
- Write/edit matching is full-subject path matching. One `*` cannot cross `/`; a run of two or more stars can. `**/` matches zero or more complete directory segments.

### 5.1 Canonical subjects

- `process`: the structurally sealed catalog program followed by each literal argv element, joined with one ASCII space. There is no quoting, token rewriting, or shell parsing. Example: program `cargo` plus argv `test`, `-p`, `keeper-policy` becomes `cargo test -p keeper-policy`.
- `write`: the validated portable proposal path for create mode.
- `edit`: the validated portable proposal path for modify mode.

### 5.2 Golden matrix

| Prefix | Pattern | Subject | Match | Pinned behavior |
| --- | --- | --- | ---: | --- |
| process | empty | `cargo --version` | yes | bare prefix matches all processes |
| process | `cargo*` | `cargo --version` | yes | required `Bash(cargo*)` case |
| process | `cargo` | `cargo test` | yes | literal prefix |
| process | `cargo` | `cargo-nextest run` | yes | deliberately no word boundary |
| process | `cargo *` | `cargo` | no | required literal space is absent |
| process | `cargo *` | `cargo test -p a/b` | yes | process star crosses spaces and slash |
| process | `cargo?` | `cargoX` | no | `?` is literal |
| edit | `src/*` | `src/lib.rs` | yes | one path segment |
| edit | `src/*` | `src/nested/lib.rs` | no | one star does not cross slash |
| edit | `src/**` | `src/nested/lib.rs` | yes | double star crosses slash |
| write | `src/**/*.rs` | `src/lib.rs` | yes | `**/` accepts zero directories |
| write | `src/**/*.rs` | `src/a/b/lib.rs` | yes | `**/` accepts multiple directories |
| write | `src/**/*.rs` | `src/a/b/lib.ts` | no | literal suffix remains exact |
| write | `SRC/**` | `src/lib.rs` | no | case-sensitive on Windows too |
| edit | `src/lib.rs` | `src/lib.rs.bak` | no | path literals are whole-subject |

## 6. Receipts and attribution

Every automatic allow or deny first has a durable pending proposal and then a persistent decision. Automatic approval never skips the ledger.

Each rule receipt persists:

- proposal decision and actor (`rule_engine`);
- `rule_id`;
- rule `scope` and `project_id`;
- `prefix`, `pattern`, and `action`;
- rule `content_digest`;
- exact `match_subject`;
- `decided_at`.

Therefore the receipt can answer both "which rule decided this proposal?" and "what exactly did that rule contain at decision time?" even after the live rule is deleted. Tests cover deletion after approval and exact snapshot mismatch rollback.

The fixed renderer-facing attribution shape is `auto_approved_by_rule: Option<{prefix, pattern, action}>`; it is populated from the durable receipt for both live and reopened terminal proposal views. The field name is retained exactly even when `action` is `deny`.

## 7. Project tighten-only enforcement

Project allow is rejected or ignored at four boundaries:

1. Tauri `add_permission_rule` explicitly returns `permission_rules_project_allow_forbidden` before storage mutation;
2. the SQLite repository's typed `add` independently returns `ProjectAllowForbidden`;
3. effective loading selects project rows only when `action = 'deny'`;
4. the pure evaluator ignores a project allow even if corrupt input bypasses the preceding boundaries.

Rule-receipt validation independently rejects a project-scoped allow snapshot. Project command access also resolves the supplied id through the registered-project authority. Global scope always canonicalizes `project_id` to `None`.

## 8. Structural policy remains authoritative

- Write: existing path/capability preparation completes before rule evaluation. An allow cannot bypass path validation or write preparation.
- Process: `GovernedProcessToolGateway::seal` checks the structural program catalog before reservation and rule evaluation. A global `process:* allow` cannot introduce an unlisted program; the integration test uses `pnpm` and proves that it is rejected without creating a proposal.
- Rule evaluation occurs only for already-valid pending proposals. It does not alter catalog contents, capabilities, fingerprints, ownership, expiry, grants, leases, or terminalization.

## 9. Seven required tests

| Requirement | Test/evidence | Result |
| --- | --- | --- |
| 1. Pure prefix/glob/subject matrix | `pure_prefix_glob_and_subject_matrix_is_pinned`, canonical subject test, and prefix-class test | PASS |
| 2. Project cannot insert allow | storage `project_scope_explicitly_rejects_allow_before_sql`; Tauri `command_explicitly_rejects_project_allow`; corrupt-input evaluator test | PASS |
| 3. Deny beats allow | pure order-independent precedence test; runtime project deny over global allow before write | PASS |
| 4. Complete receipt fields | write allow snapshot survives live-rule deletion; process project deny snapshot; runtime reopened views | PASS |
| 5. No project-directory config | SQLite authority accepts no project root; explicit `permission_rule_authority_has_no_project_directory_config_loader` guard checks for file-loader/config-source paths | PASS |
| 6. Program catalog still applies | `allow_rule_cannot_introduce_program_outside_structural_catalog` | PASS |
| 7. No match requires confirmation | pure `no_matching_rule_still_requires_confirmation`; all pre-existing human approval bridge tests remain green | PASS |

Additional coverage verifies migration idempotence, CRUD across restart, bounded inputs, scope shape, command wire serialization, registered-project validation, command registration, automatic write application, automatic process execution, requested/decided/started lifecycle order, and durable attribution after restart.

## 10. Verification commands and exit codes

All Rust commands used `CARGO_TARGET_DIR=<repo>/target-perm`; the machine-specific absolute value is intentionally omitted from this report.

| Command | Exit code | Result |
| --- | ---: | --- |
| storage permission-rule targeted RED before API existed | 1 | expected TDD compile failure |
| kernel matcher targeted RED before module existed | 1 | expected TDD compile failure |
| runtime bridge targeted RED before DTO/host integration | 1 | expected TDD compile failure |
| Tauri command targeted RED before command module/DTO fields | 1 | expected TDD compile failure |
| `cargo test -p keeper-kernel-policy` | 101 | operator used the wrong package name; corrected to `keeper-policy` |
| `cargo test -p keeper-policy; cargo test -p keeper-storage-sqlite; cargo test -p keeper-runtime-host` | 0 | all three core packages passed |
| `cargo test -p pillow-desktop --lib` | 0 | 151 passed |
| targeted four-package clippy, first run | 1 | found new `too_many_arguments`; implementation refactored, no lint suppressed |
| targeted four-package clippy, rerun with `-D warnings` | 0 | passed |
| `corepack pnpm test` | 0 | 35 files, 429 tests passed |
| `node scripts/run-cargo.mjs test --workspace` | 0 | entire Rust workspace passed after implementation |
| `node scripts/run-cargo.mjs clippy --workspace -- -D warnings` | 0 | entire Rust workspace passed |
| `node scripts/check-acp-protocol-drift.mjs` | 0 | protocol pins current |
| final storage receipt/repository tests after dependency-boundary cleanup | 0 | 59 passed |
| final storage clippy with `-D warnings` | 0 | passed |
| `git diff --check` | 0 | no whitespace errors |
| standalone `corepack pnpm typecheck` | 1 | concurrent renderer test type error described below |
| required `node scripts/run-worker-gate.mjs` after code freeze | 1 | workspace, secret scan, and Rust format passed; stopped at desktop typecheck exit 2 |

The final gate's sole diagnostic is in the parallel renderer worker's `apps/desktop/tests/permission-rules-ui.test.tsx`: that test adds `auto_approved_by_rule` to an agent runtime-event payload whose existing TypeScript union does not contain that property. This worker is forbidden to edit `apps/desktop/src/**` or the renderer test and did not do so. Later gate stages were independently run and are green as recorded above.

## 11. Write-boundary audit and untouched areas

Core changes are confined to:

- `crates/storage-sqlite/**`;
- `crates/kernel-policy/**`;
- `crates/runtime-host/**`;
- `apps/desktop/src-tauri/src/**`;
- this report.

The temporary root-lockfile delta was eliminated; `Cargo.lock` has no content diff. This worker did not edit renderer code, `knowledge-indexer`, `git-executor`, `kernel-resource`, `kernel-project`, `adapter-grok-acp`, scripts, fixtures, historical migrations, or `.superpowers/`. It did not run desktop E2E and performed no Git write operation.

## 12. Residual risks and controller follow-up

- The wildcard behavior is deliberately Pillow-specific. Grok parity remains unclaimed; notably, process literal prefixes have no word boundary and matching is uniformly case-sensitive.
- argv subjects are literal space joins, not shell command lines. Shell chaining and quoting semantics remain explicitly out of scope.
- The durable decision/list DTOs and SQLite receipts carry rule attribution. The existing agent runtime-event schema was not widened because `crates/agent-protocol/**` is outside this worker's whitelist. The parallel renderer test currently assumes such an event-schema extension; the controller must either keep attribution on the accepted proposal decision/receipt DTO boundary or assign a separately scoped protocol change.
- A rule deleted or edited after evaluation but before its decision causes a fail-closed rollback and may require a fresh proposal. This is intentional stale-authority protection.

## 13. Final status

`PERMISSION-CORE-BLOCKED: BLOCKED-BY-CONCURRENT - core implementation and independent gates are green, but the required aggregate gate is stopped by the parallel renderer worker's incompatible runtime-event test payload.`
