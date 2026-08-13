# Permission-rule quarantine + in-product recovery — execution report

- Date: 2026-08-13
- Scope: `crates/storage-sqlite/**`, `crates/kernel-policy/tests/**`,
  `apps/desktop/src-tauri/**`, `apps/desktop/src/{lib,features/permissions,styles}/**`,
  `apps/desktop/tests/**`, this report
- Status: **COMPLETE**
- Signal: **PERMISSION-RULE-QUARANTINE-COMPLETE**

## 1. Problem (do not re-argue)

`validate_loaded_rules` walked every stored rule and returned `Err` on the first
invalid one. Consequences of that single decision:

1. One bad row disabled **every** permission rule.
2. `list_permission_rules` went through the same path, so the Settings list —
   the only surface that can delete a rule — could not render either. Recovery
   required editing SQLite by hand.
3. `evaluate_host_permission_rule_conjunction` maps a load failure to
   `GovernedProcessBridgeError::Gateway`, so a bad row did not merely stop
   auto-approval: it failed every governed process proposal *before* the user
   could decide anything.

Reachable today without any new exploit: a `rule_id` that is not a UUID, a
`content_digest` that does not describe its row, or a `created_at` that is not
RFC 3339 (that last one failed the load one layer earlier still, as a rusqlite
conversion error surfaced as `PermissionRuleError::Sqlite`).

This is the same shape as the 2026-08-08 settings self-brick: stored user data
the product refuses to read, and the only repair UI living behind the refusal.

### 1.1 One correction to the task premise

The task described EA-03 as already landed on `main` — `validate_pattern`
rejecting empty and whitespace-only patterns, with a report at
`docs/execution/2026-08-12-ea03-report.md`. **Neither is in this repository.**
`git log --all -- 'docs/execution/*ea03*'` is empty, and `validate_pattern` at
`9b4335e` bounded length and rejected `\0 \r \n` only. EA-03 is still listed as
open in `docs/references/external-audit-report-2026-08-12.md` (P2/Medium,
sequence B: "绕过前端空 pattern 校验").

That matters because it inverts the failure mode. Without the pattern rule, an
empty-pattern row is *valid*, so it loads, and kernel policy reads it as
match-all (`permission_rule_matches` returns `true` for an empty pattern) — a
planted blank `global process allow` auto-approves every command. Quarantine
alone would not have covered the scenario the task describes, so this change
lands the storage-side half of EA-03 as well: `validate_pattern` now rejects
empty and whitespace-only patterns, which closes both directions at once (write
time rejects, load time quarantines). The renderer gateway already rejected
blank patterns before submit; that was the "renderer-only validation" EA-03
objected to, and the check now exists below the UI.

## 2. Decision: quarantine, not a one-time migration

Chosen: skip the unreadable row, keep the rest, report it for deletion.

Rejected: a migration that deletes rows the product could never have written.

| Reason | Detail |
| --- | --- |
| Coverage | A migration can only delete shapes enumerated today. Quarantine covers every present and future invalid shape, including ones a migration could not legitimately judge — a digest mismatch may be a real rule that was rewritten. |
| Evidence | Silently deleting the planted row destroys the only trace of the exploit the user was hit by. The user sees it, then removes it. |
| Precedent | The self-brick fix degraded and pointed at Settings; it explicitly did **not** rewrite stored user data (`residual risk 1` in that report). Same rule here. |
| Authority | Deleting rows at startup is a write the user did not ask for. Deleting an *inert* row on an explicit click is. |

Failing closed is unchanged and non-negotiable: an unreadable row is never
evaluated. Quarantine only changes what happens to the *other* rows.

## 3. What changed

### Layer 1 — SQLite authority (`crates/storage-sqlite/src/permission_rules.rs`)

- Rows are read as raw column text and classified per row
  (`RawPermissionRuleRow::classify`) instead of typed-read-then-validate-all.
  A single unreadable row can no longer produce a store-wide error.
- `list` / `list_effective` return only readable rules.
- `list_unreadable()` reports every row that is not a readable rule, **unscoped**
  — a row whose `scope` column is itself unreadable belongs to no scope, and a
  scoped query would never return it. With `list`, this accounts for every row
  in the table.
- `delete_unreadable(rule_id)` removes one reported row. It relaxes the id shape
  (an unreadable row's id may not be a UUID, which is exactly what `delete`
  refuses) and re-classifies inside the deleting transaction, so the relaxed id
  can never reach a readable rule. `delete` keeps its UUID requirement.
- `validate_pattern` rejects empty / whitespace-only patterns (§1.1).

Reason codes, checked in this order (first failure wins):

| Code | Row defect |
| --- | --- |
| `column` | required column absent or not stored as text |
| `scope` / `prefix` / `action` | enum column outside its set |
| `scope_shape` | global row with a project id, or project row without one |
| `project_allow` | project-scoped allow |
| `pattern` | empty, whitespace-only, oversize, or control characters |
| `rule_id` | not a UUID |
| `created_at` | not RFC 3339 |
| `content_digest` | digest does not describe the row |

The pattern bound is checked before the digest so an oversize pattern cannot
reach `content_digest`'s own `expect`.

### Layer 2 — Host commands (`permission_rule_commands.rs`, `lib.rs`)

- `list_unreadable_permission_rules`, `delete_unreadable_permission_rule`, both
  registered in `generate_handler!`.
- `UnreadablePermissionRuleView` carries `rule_id`, `reason`, and raw
  scope/project/prefix/action/created_at for recognition. It deliberately
  carries **no `pattern`**: the product does not re-display a pattern it cannot
  vouch for as a rule the user wrote, and does not offer it back for re-adding.
- `PermissionRuleView` is untouched, so the existing renderer key contract does
  not move (AGENTS.md: adding a field to a host DTO is a breaking change).
- Quarantine is silent in the evaluation path by design, so the skipped-row count
  is logged once, where Settings loads the recovery list.

### Layer 3 — Renderer (`permissionRulesGateway.ts`, `permissionPattern.ts`)

- `UnreadablePermissionRuleView` + strict parser: `rule_id` and `reason` are
  required, everything else best-effort (these rows are unreadable precisely
  because a column is not what the product writes, so a strict parse would
  re-create the failure it exists to recover from). A `pattern` sent by a host is
  dropped, not carried.
- Interface + both gateways (`Tauri`, `Mock`) gained the two methods;
  `MockPermissionRulesGateway` takes an optional second constructor argument and
  has `setUnreadableRules` / `getUnreadableSnapshot` seams.
- `unreadableRuleReasonLabel` / `unreadableRuleShapeLabel` for display.

### Layer 4 — Settings UI (`PermissionRulesSection.tsx`, `permission-rules.css`)

- An 「无法读取的规则」 block above the rule lists, rendered **only** when the
  store holds such a row, each row with its reason and a 删除 button.
- The quarantine list loads in its own `try` so a failure there reports itself
  without hiding the rules that did load.

## 4. Tests, with negative controls

New / changed tests:

| Layer | Test | Pins |
| --- | --- | --- |
| storage | `an_empty_pattern_row_is_quarantined_and_the_rest_of_the_store_still_loads` | EA-03 seq-B row skipped; global + project lists still render; absent from `list_effective`; reported as `pattern`; removable; rules survive the repair |
| storage | `every_unreadable_row_shape_is_reported_and_removable` | bad UUID, tampered digest, whitespace pattern, `\r\n` pattern, bad `created_at` — each skipped, reported with the right code, removable, readable rule untouched |
| storage | `a_project_scoped_allow_row_is_quarantined_not_evaluated` | project allow is not a rule and not evaluated |
| storage | `rows_with_missing_or_wrong_typed_columns_are_reported_not_fatal` | NULL column, unknown scope, blob id (CHECK-free table) — three unreadable rows do not take the readable one down; all three removable |
| storage | `readable_rules_are_not_reachable_through_the_repair_path` | `delete_unreadable` refuses a readable rule; id bounds; `delete` still rejects a non-UUID id |
| storage | `quarantine_survives_reopen_and_accounts_for_every_stored_row` | quarantine is a property of the row, not of one process; `list + list_unreadable == COUNT(*)` |
| storage | `add_rejects_empty_and_whitespace_only_patterns` | write-time half of §1.1 |
| policy | `an_empty_pattern_matches_every_subject_in_its_class` | *why* the store must never load a blank pattern: it is match-all and reaches `Allow` |
| host | `unreadable_rows_are_listed_and_removable_without_hiding_readable_rules` | command contract + exact wire keys + `pattern` / `content_digest` not exported |
| host | `the_repair_command_refuses_a_readable_rule` | `permission_rules_not_found`, rule intact |
| host | `desktop_invoke_handler_registers_exact_permission_rule_commands` | both new commands registered |
| wiring | `a_quarantined_row_neither_grants_authority_nor_breaks_rule_evaluation` | end-to-end: planted match-all row present → `node --version` still auto-approves **by the user's rule**, receipt attribution names it, planted row is reported and removable |
| renderer | `1b`, `1b2`, `1b3` | quarantined row rendered with reason and removed; clean store renders no block; parser key set drops an untrusted pattern |

**Negative controls run (each reverted).** A test that only passes after a change
proves nothing on its own:

| Simulated prior behaviour | Result |
| --- | --- |
| all-or-nothing load (panic when any row is unreadable) | 5 storage quarantine tests **FAILED**, 10 pre-existing tests still passed |
| quarantine on, `validate_pattern` without the empty/whitespace rule | 4 storage tests **FAILED** and the wiring test **FAILED** — the planted blank row won attribution over the user's rule, i.e. the exploit reproduced |

## 5. Commands & exit codes

| Command | Exit |
| --- | --- |
| `node scripts/run-worker-gate.mjs` | **GATE_EXIT=0** (`PILLOW_WORKER_GATE: OK`, all 8 steps) |
| `corepack pnpm test` (inside the gate) | 0 — 54 files, 699 tests |

First gate run failed at the `secrets` step: the new policy test used an
absolute host user-profile path as its subject. Replaced with a project-relative
path. (Naming the literal here trips the same scanner, so it stays described
rather than quoted.) No desktop
E2E or screenshots (controller-owned; this change adds one conditional Settings
block whose render is covered by vitest).

## 6. Residual risks

1. **A row whose `rule_id` is not valid UTF-8.** Reported and inert, but not
   individually deletable: the repair path matches `CAST(rule_id AS TEXT)`
   against the text it reported, and a lossy rendering will not match. Reaching
   this requires writing a blob into a TEXT-affinity `NOT NULL` column, i.e. a
   hand-edited database — in which case the user has a database editor already.
   Valid-UTF-8 blob ids *are* removable (covered by test).
2. **Existing bad rows are not rewritten or deleted for the user.** By design
   (§2). They are inert and one click from gone.
3. **EA-03 is not closed by this change.** Only its storage-side pattern rule is.
   Sequence A (renderer holding a reusable write `approval_grant`) and the
   host-private-grant work are untouched, and the audit's requirement of a
   direct-Tauri-call negative test for EA-03 is not satisfied here.
4. `list_effective` reports nothing about skipped rows to its callers; the count
   is logged only where Settings loads the recovery list. A governed proposal
   evaluated while a quarantined row exists behaves exactly as if the row were
   absent, which is the intent, but there is no per-decision receipt of it.

## 7. Touched files

- `crates/storage-sqlite/src/permission_rules.rs`
- `crates/storage-sqlite/src/lib.rs`
- `crates/storage-sqlite/tests/permission_rules_repository.rs`
- `crates/kernel-policy/tests/permission_rules.rs`
- `apps/desktop/src-tauri/src/permission_rule_commands.rs`
- `apps/desktop/src-tauri/src/permission_rule_commands_tests.rs`
- `apps/desktop/src-tauri/src/lib.rs` (two command registrations)
- `apps/desktop/src-tauri/tests/permission_rules_wiring.rs`
- `apps/desktop/src-tauri/Cargo.toml` (`rusqlite` dev-dependency: tests plant
  bypassing-caller rows; no production desktop code opens SQLite directly)
- `apps/desktop/src/lib/permissionRulesGateway.ts`
- `apps/desktop/src/features/permissions/{PermissionRulesSection.tsx,permissionPattern.ts}`
- `apps/desktop/src/styles/permission-rules.css`
- `apps/desktop/tests/permission-rules-ui.test.tsx`
- `docs/execution/2026-08-13-permission-rule-quarantine-report.md`

**PERMISSION-RULE-QUARANTINE-COMPLETE**
