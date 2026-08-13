# Overbroad process allow rules

Date: 2026-08-13  
Worker: grok3  
Controller: Claude  
Scope: rule matching and allow-pattern suggestion only (not approval mode)  
Status: DONE  

Completion signal: `OVERBROAD-RULES-COMPLETE`

---

## 0. Constraints

- Allowed: `crates/kernel-policy` permission-rule matching + that crate’s tests, renderer pattern helpers, always-rule dialog, desktop tests, this report.
- `crates/kernel-policy/src/lib.rs` only re-exports the new pure functions so integration tests (and a later settings label) can name them.
- Not touched: storage, tauri host, runtime-host, settings, approval-mode files.
- No git writes. No gate. No E2E.
- v1 process-subject encoding and `crates/kernel-policy/tests/fixtures/process-subject-v1.json` are unchanged.
- Deny matching surface is unchanged.

---

## 1. Exit-code table

Commands actually run. Isolated cargo target dir for this worker slot (not the controller `target/`).

| Command | Log | EXIT |
| --- | --- | ---: |
| `node scripts/run-cargo.mjs test -p keeper-policy --test permission_rules -- suggested_allow_pattern_from_node_build_js_does_not_auto_allow_a_different_script` (old code, functions not exported) | `.superpowers/round50/ob-red1.txt` | 101 |
| `node scripts/run-cargo.mjs test -p keeper-policy` (after the change) | `.superpowers/round50/ob-log1.txt` | 0 |
| `corepack pnpm --filter @pillow/desktop test` | `.superpowers/round50/ob-log2.txt` | 0 |
| `cargo fmt -p keeper-policy -- --check` | (stdout only) | 0 |
| `cargo fmt --all -- --check` | `.superpowers/round50/ob-fmt-check.txt` | 1 |

`cargo fmt --all -- --check` EXIT=1 is **BLOCKED-BY-CONCURRENT**: the only diff is `crates/runtime-host/src/governed_write.rs`, which the two-tier permissions worker owns and this ticket must not touch. `keeper-policy` itself is rustfmt-clean.

`ob-log1.txt` pin: `permission_rules` 25 passed (includes the five new tests), plus existing `process_policy` / `process_subject_conformance` / `write_policy`.  
`ob-log2.txt` pin: 58 files, 762 passed, including `permission-overbroad.test.ts` (20) and the updated always-rule dialog cases.

---

## 2. Red-then-green evidence

**File:** `crates/kernel-policy/tests/permission_rules.rs`  
**Test:** `suggested_allow_pattern_from_node_build_js_does_not_auto_allow_a_different_script`

The test starts from the v1 subject for `node` + `build.js`, asks `suggest_process_allow_pattern` for an allow pattern, then evaluates that pattern against the v1 subject for `node` + `anything-else.js`. It requires:

- the suggestion is not classified overbroad
- the suggestion is not `node*` / `node *` / `node`
- the approved subject still auto-allows under the suggestion
- the other script is a confirmable shape (so a match would actually auto-allow)
- the other script still requires confirmation

**Old code (red).** The functions did not exist. Compiler:

```text
error[E0432]: unresolved imports
  keeper_policy::example_also_allowed_process_subject
  keeper_policy::is_overbroad_process_allow_pattern
  keeper_policy::suggest_process_allow_pattern
 --> crates\kernel-policy\tests\permission_rules.rs:6:5
error: could not compile `keeper-policy` (test "permission_rules") due to 1 previous error
```

EXIT=101. Log: `.superpowers/round50/ob-red1.txt`.

**New code (green).** Same test name: `ok`. Full `keeper-policy` suite EXIT=0.

Renderer twin (same assertion, real match — not just string shape):  
`apps/desktop/tests/permission-overbroad.test.ts` →  
`does not suggest a pattern that would auto-allow node anything-else.js`.

---

## 3. What counts as overbroad

Pure functions (not JSX):

- Kernel: `is_overbroad_process_allow_pattern`
- Renderer: `isOverbroadProcessAllowPattern` (same table)

A process **allow** pattern is overbroad when it has **no argument constraint**:

1. Empty / whitespace-only / `*` — matches everything in class.
2. Program token contains `*` (`node*`, `no*de`, `*node`, `node.exe*`) — program name itself is a glob.
3. After dropping tokens that are only `*`, nothing remains except the program (`node`, `node *`, `node **`, `/usr/bin/node *`).

Not overbroad:

- `node build.js`, `node build.js *`, `cargo test *` — at least one concrete argument token.
- A decodable v1-encoded subject — that pins one argv.

### Considered and not taken

| Candidate | Why not |
| --- | --- |
| Any `*` is overbroad | `cargo test *` is the documented word-boundary-safe form. Forbidding every star would push people to prefix-only `cargo test`, which also hits `cargo testing`. |
| Only glued `node*` is overbroad; `node *` is fine | `node *` is the actual “any script” mis-allow. |
| Change match so prefix cannot cross argv | Would silently reinterpret already-stored rules. Encoding/matching stay put. |
| Delete or rewrite stored wide rules | User owns those rows. This ticket does not decide for them. |
| Refuse to save an overbroad allow | Goal is “not casual”, not “forbidden”. The dialog warns; submit stays enabled. |
| Always suggest the exact v1 encoding | Narrowest, but unreadable for ordinary bijective argv. Full space-join of every concrete token matches the approved command and does not authorize a different first argument. |

Deny suggestion is unchanged (`program *` / `program first-arg *`). Only `action === "allow"` uses the narrower suggestion.

---

## 4. Already-stored wide rules

**Choice: leave them. Do not delete, rewrite, or ignore.**

- Match and evaluate are unchanged, so a stored `node*` still auto-allows a confirmable `node anything-else.js`. That is pinned by `stored_overbroad_allow_still_auto_allows_a_different_confirmed_script`.
- Deny of `node*` / `node *` still hits a different script (`deny_match_surface_is_not_narrowed_for_broad_process_patterns`).
- Settings is out of scope (another worker owns two-tier mode). The classifier is exported so a later settings pass can *label* wide rows without this ticket touching that surface.

---

## 5. Residual risk

Wide patterns can still be stored. That is deliberate.

1. **User types `node*` / `node *` and saves.** The dialog now shows a concrete consequence derived from the current subject (for `node build.js` → `node build.js.other`, and that contrast is itself evaluated as a real auto-allow). Submit is not blocked.
2. **Bare-program allow.** Prefix match has no narrower space-join form: suggested `node` still authorizes `node <anything>`. The dialog treats it as overbroad and warns. Closing this needs a matching-language change, which this ticket refuses.
3. **Full-argv prefix still stretches.** `node build.js` also matches `node build.js --flag` and `node build.jsx` (no last-token word boundary). The existing word-boundary note covers the sibling-name case. Extra flags on the *same* first argument are residual.
4. **User-typed `cargo test *`** still auto-allows any confirmed `cargo test …` argv. That is an argument-constrained pattern, so it is not classified overbroad.
5. **Already-stored `node*`** still auto-allows any *confirmed* node argv (file scripts, `--version`, …). Interpreter confirmation still stops `-e` / `--eval` / `bash -c` / `npx` / `-exec`. Unrecognised wrappers plus a stored wide rule remain the residual hole the audit named; this ticket only stops that rule being the default one-click save.
6. **Delegation programs** (`npx`, `env`, …) do not get a specific “also allow” example, because confirmation would refuse the contrast. The generic “no argument constraint” warning still shows.

---

## 6. What changed

**Kernel**

- `is_overbroad_process_allow_pattern`
- `suggest_process_allow_pattern` — full concrete argv, no casual `program *`
- `example_also_allowed_process_subject` — contrast argv is the first operand plus `.other`, then run through real `evaluate_permission_rules`

**Renderer**

- Allow suggestion from `node build.js` is now `node build.js`, not `node *` / `node build.js *`
- `AlwaysRuleDialog` prefills from the allow suggestion; if the user widens the pattern, `always-rule-overbroad-warning` names the derived command that would also auto-allow

v1 subject encoding, process confirmation table, and deny matching are untouched.
