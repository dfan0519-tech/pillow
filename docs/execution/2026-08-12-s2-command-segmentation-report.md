# S2 Command Segmentation and Rule Conjunction Report

Status: IMPLEMENTATION COMPLETE; WORKER GATE ENV-BLOCKED

Completion signal: `S2-SEGMENTATION-COMPLETE`

## Scope and Outcome

S2 now applies SQLite permission rules to governed shell commands without
widening the structured process catalog. A shell command is auto-approved only
when the conservative segmenter can prove its boundaries and every segment has
an independent allow match. A deny on any segment rejects the whole proposal.
An unreliable shape or any segment without an allow remains pending for a human
decision.

The structured `process.run@1` path still uses the existing canonical
`program + argv` subject. `validate_program_catalog` was not changed.

All Cargo commands below used the task-mandated dedicated `CARGO_TARGET_DIR`.
No desktop E2E was run.

## Exit Code Table

| Check | Command | Exit code | Result |
| --- | --- | ---: | --- |
| TDD RED: missing policy API | `cargo test -p keeper-policy --test permission_rules` | 1 | Expected unresolved imports, then expected assertion failures after the minimal API stub |
| TDD RED: durable segment receipts | `cargo test -p keeper-storage-sqlite --test exec_proposal_repository conjunctive_shell_allow_persists_an_ordered_complete_snapshot_for_every_segment` | 1 | Expected `InvalidRuleReceipt` before implementation |
| TDD RED: shell auto-approval | `cargo test -p keeper-runtime-host --test governed_process_bridge simple_shell_conjunction_auto_executes_and_persists_each_contributing_rule` | 1 | Expected S1 human fallback produced `Rejected` instead of `Succeeded` |
| Formatting | `cargo fmt -p keeper-policy -p keeper-storage-sqlite -p keeper-runtime-host -p keeper-agent-protocol -- --check` | 0 | Passed |
| Policy tests | `cargo test -p keeper-policy` | 0 | 48 passed |
| SQLite tests | `cargo test -p keeper-storage-sqlite` | 0 | 184 passed |
| Runtime-host tests | `cargo test -p keeper-runtime-host` | 0 | 126 passed |
| Protocol tests | `cargo test -p keeper-agent-protocol` | 0 | 82 passed, including compile-fail doc tests |
| Tool-executor tests | `cargo test -p keeper-tool-executor` | 0 | 92 passed; catalog behavior remains pinned |
| Policy clippy, first run | `cargo clippy -p keeper-policy -- -D warnings` | 1 | Found `clippy::question_mark`; fixed before final verification |
| Policy clippy, final | `cargo clippy -p keeper-policy -- -D warnings` | 0 | Passed |
| SQLite clippy | `cargo clippy -p keeper-storage-sqlite -- -D warnings` | 0 | Passed |
| Runtime-host clippy | `cargo clippy -p keeper-runtime-host -- -D warnings` | 0 | Passed |
| Protocol clippy | `cargo clippy -p keeper-agent-protocol -- -D warnings` | 0 | Passed |
| Tool-executor clippy | `cargo clippy -p keeper-tool-executor -- -D warnings` | 0 | Passed |
| Patch whitespace | `git diff --check` | 0 | Passed |
| Sole Worker gate | `node scripts/run-worker-gate.mjs` | 1 | `ENV-BLOCKED`: workspace and secret checks passed, then the cargo-fmt supervisor could not obtain the sandbox-denied Windows CIM process snapshot at `desktop-process.mjs:161`; no script workaround was attempted |
| Desktop E2E | Prohibited by task contract | - | Not run |

The sole Worker gate did not reach TypeScript, package tests, workspace Rust
tests, workspace clippy, or protocol drift. Package-level results above are
local evidence only and are not represented as a green full gate. The
controller must rerun the sole Worker gate in an environment that permits its
owned-process snapshot.

## Conservative Segmenter Safety Argument

The accepted language is intentionally a strict subset of shell syntax:

- A command contains non-empty plain tokens separated only by ASCII space or
  tab.
- Segments are separated only by `&&`, `||`, `;`, or `|`.
- Repeated whitespace is normalized to one ASCII space in each rule subject.
- Leading, trailing, or adjacent operators fail segmentation.
- Each reliably produced subject is evaluated independently against the same
  effective SQLite rule set.
- Evaluation scans every subject before deciding. The first deny wins even if
  an earlier segment had no match. Allow is returned only when the number of
  allow matches equals the number of segments.
- `None` from segmentation maps only to `RequireConfirmation`; it never maps to
  allow and never fabricates a rule receipt.

This is a safety argument based on under-acceptance, not parser completeness.
The implementation does not claim to parse arbitrary shell. False negatives
cost another human prompt; they do not grant authority. In the accepted subset,
the constructs that could hide a second command or change token boundaries are
not admitted, so an allow for one subject cannot silently cover another
segment.

### Rejected Constructs

The segmenter deliberately returns unreliable for:

- single and double quotes;
- command substitution via `$()` or backticks;
- variable expansion using `$`, `%...%`, or `!...!`;
- output/input redirection, file-descriptor redirection, here-docs, and
  here-strings via `<` or `>`;
- newlines, control characters, and non-ASCII whitespace;
- globbing and wildcard syntax: `*`, `?`, and bracket expressions;
- POSIX and Windows escaping: backslash and caret;
- grouping, subshell, brace, comment, and tilde syntax;
- a single/background `&`, malformed operators, or empty segments;
- leading environment assignments, including `NAME=value` and `NAME+=value`,
  plus environment/compound grammar keywords such as `env`, `export`, `set`,
  `eval`, `time`, `if`, `for`, `case`, and `start`;
- known shell interpreter tokens, including common `sh`, `bash`, `cmd`,
  PowerShell, WSL, and related executable-name variants.

### Inline Shell Decision

Inline shells are classified as unreliable rather than recursively parsed.
This is the safer of the two permitted designs for this deliberately small
parser: it avoids having an outer simple-looking rule authorize an inner
language with different quoting or option semantics. Both the usual quoted
form, such as `sh -c "..."`, and an unquoted recognized interpreter token fail
closed and require a human.

## Rule Attribution and Receipt Decision

The protocol DTO was not widened. Instead, S2 adds an internal and durable
per-segment representation:

- `PermissionRuleSegmentMatchSnapshot` carries the original zero-based segment
  index plus the complete matched rule snapshot.
- New migration `0013_exec_permission_rule_segments.sql` adds
  `exec_permission_rule_segment_receipts`, keyed by proposal and segment index.
- Every row stores decision, actor, rule id, scope, project id, prefix, pattern,
  action, content digest, matched subject, and decision time.
- The approval decision, backward-compatible representative receipt, all
  segment receipts, and proposal transition commit in one SQLite transaction.
- An allow decision requires contiguous receipt indexes for all segments. A
  deny decision stores the actual index of the offending segment.
- Receipt rows do not reference the live rule row; deleting a rule does not
  erase historical attribution. A focused test deletes both contributing rules
  and proves the two receipts remain byte-for-byte readable.

The existing one-to-one `exec_permission_rule_receipts` row remains as a
compatibility projection. For a conjunctive allow it contains the lowest-index
segment's contributing rule. The authoritative audit answer is the ordered
segment-receipt list, not that projection.

### Renderer Coordination

No renderer coordination is required for this change: no event or host DTO key
was added, removed, or renamed, so the closed renderer allowlists do not need to
change. The existing singular attribution remains display-compatible. A future
UI that wants to show every contributing rule will need a separately designed
read surface, but the durable ledger is complete now.

## Invariants and Adversarial Coverage

| Invariant | Positive pin | Negative/adversarial pin |
| --- | --- | --- |
| Auto-approval never skips the ledger | Two simple shell segments are auto-executed by two distinct allow rules and persist ordered receipts `0` and `1` | Mixed allow/deny input cannot create a partial receipt; human decisions create no rule receipt |
| Project scope only tightens | A project deny on a later segment beats a matching global allow and rejects before start | Project allow is rejected before SQL and ignored by pure policy even if corrupt input reaches it |
| Project files never authorize | A SQLite global rule auto-approves the matching simple command | Fake project `.pillow` and `.grok` permission files leave the proposal human-decidable with no attribution |
| No match asks; deny beats allow | Normal `cargo test` is positively proven allowable, and two separately allowed segments satisfy conjunction | An unmatched segment requires confirmation; a later deny wins over earlier allow and earlier no-match |

Pinned command cases:

| Command shape | Expected result | Test location |
| --- | --- | --- |
| `cargo test` | Allowed by matching process rule | policy conjunction test |
| `cargo test; rm -rf x` with only a cargo allow | Human confirmation; never auto-allow | policy and runtime adversarial tests |
| `sh -c "cargo test && rm -rf x"` even with broad allow | Human confirmation | policy and runtime adversarial tests |
| Quoted argument variant | Human confirmation | policy and runtime adversarial tests |
| `$()` and backtick substitution variants | Human confirmation | policy tests; `$()` also pinned at runtime |
| Redirection and here-doc variants | Human confirmation | policy tests; output redirection also pinned at runtime |
| `echo s2-first && echo s2-second` with two rules | Auto-allow, execute, and persist two full snapshots | runtime-host integration test |

## Write-Boundary Audit

This Worker wrote only:

- `crates/kernel-policy/src/lib.rs`
- `crates/kernel-policy/src/permission_rules.rs`
- `crates/kernel-policy/tests/permission_rules.rs`
- `crates/runtime-host/src/host/process.rs`
- `crates/runtime-host/src/permission_rules.rs`
- `crates/runtime-host/tests/governed_process_bridge.rs`
- `crates/storage-sqlite/migrations/0013_exec_permission_rule_segments.sql`
- `crates/storage-sqlite/src/exec_proposal.rs`
- `crates/storage-sqlite/src/lib.rs`
- `crates/storage-sqlite/src/migrations.rs`
- `crates/storage-sqlite/src/permission_rules.rs`
- `crates/storage-sqlite/tests/exec_proposal_repository.rs`
- `crates/storage-sqlite/tests/permission_rules_repository.rs`
- `crates/agent-protocol/src/types.rs` (documentation clarification only; no
  serialized field change)
- this report

Audit results:

- No file under `apps/desktop/src/**` was touched.
- No `scripts/**` file was touched or used as a workaround.
- No historical migration was edited; S2 adds only migration `0013`.
- No `Cargo.toml` or `Cargo.lock` changed; no dependency was added.
- `crates/tool-executor/src/process.rs` has no diff, so
  `validate_program_catalog` is unchanged.
- `git diff --check` passed.
- Concurrent out-of-scope changes are visible in desktop E2E, the ACP adapter,
  acceptance scripts, and a separate shell-E2E report. They belong to another
  Worker and were preserved without edits from this Worker.
- No git write operation, desktop E2E, staging, commit, or packaging action was
  performed.

## Residual Risk

The remaining risks are explicit:

1. The segmenter proves syntax boundaries, not program semantics. A permitted
   command such as `cargo test` can intentionally execute project build scripts
   and tests.
2. Executable lookup, aliases, response files such as `tool @args.txt`, and
   command-specific configuration can redirect behavior outside the visible
   token string. Rule authors must treat these as part of the matched program's
   authority.
3. The known-interpreter deny set is finite. A new or obscure interpreter,
   wrapper, or applet that is not recognized could look like a simple command;
   a sufficiently broad rule for that outer program could authorize inner
   evaluation. Extending the deny set is a future hardening path.
4. Existing process matching deliberately retains literal-prefix behavior. A
   broad pattern can match more program names or arguments than a rule author
   intended; S2 does not alter that accepted S1 contract.
5. The event/UI compatibility projection shows one contributing rule. Complete
   multi-rule attribution is available in SQLite but not yet exposed as a
   renderer list.
6. The full Worker gate remains unverified because its owned-process supervisor
   cannot start in this sandbox. Controller rerun is required before release
   acceptance.

Conservative over-rejection is expected and is not an auto-allow risk. The most
important remaining possible wrong-allow class is semantic indirection through
an unrecognized interpreter/wrapper or external argument source combined with
a broad rule.
