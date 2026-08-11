# Interpreter hardening report

Date: 2026-08-11  
Worker: grok1  
Controller: Claude  
Scope: `crates/kernel-policy/**` only  
Status: DONE  

Completion signal: `INTERPRETER-HARDENING-COMPLETE`

---

## 0. Constraints

- Isolated cargo target dir for this worker (`target-interp`).
- Forbidden concurrent trees: `storage-sqlite`, `adapter-grok-acp`, desktop `src-tauri`.
- Allowed gates: `cargo test -p keeper-policy`, `cargo clippy -p keeper-policy -- -D warnings`, `cargo fmt --check` (package-scoped fmt).
- No desktop E2E. No git write operations.
- Report contains no machine-absolute paths.

---

## 1. Exit-code table

Commands run with isolated `CARGO_TARGET_DIR` pointing at this worker's target tree. Exit codes are from the commands themselves (no pipe to `tail`).

| Command | Exit | Result | Notes |
| --- | ---: | --- | --- |
| `cargo test -p keeper-policy` | 0 | PASS | lib + `permission_rules` (16) + `process_policy` (21) + `write_policy` (16) |
| `cargo clippy -p keeper-policy -- -D warnings` | 0 | PASS | |
| `cargo fmt -p keeper-policy -- --check` | 0 | PASS | After one `cargo fmt -p keeper-policy` format pass |

No `BLOCKED-BY-CONCURRENT` on in-scope gates.

---

## 2. Upstream research (file:line)

Upstream pin: `xai-org/grok-build` @ `75e73f3`  
Path prefix: `crates/codegen/xai-grok-workspace/src/permission/`

### What upstream does solve

| Behavior | Location |
| --- | --- |
| Deny/ask walk every chained segment; wrappers peeled; `bash -c` script recursed; undecomposable → **AskFailClosed** | `policy.rs` `evaluate_bash_command_policy` / `evaluate_bash_command_gate` ~L128–145; `evaluate_bash_command_segments` ~L147–162 (fail-closed at L152–153) |
| Allow is **conjunctive** over segments; undecomposable → allow false | `policy.rs` `evaluate` Bash branch ~L276–284; `bash_chain_fully_allowed` ~L292–327 |
| Inline shell `-c` classification + recursion budget | `policy.rs` `shell_dash_c_script` ~L390–508; recursion in `evaluate_command_words` ~L192–208 and `bash_chain_fully_allowed` ~L313–324 |
| Supported shells for `-c` recursion: **`bash` \| `sh` \| `dash` \| `zsh` \| `ksh` only** | `policy.rs` L394–396 (`NotInline` for any other program head) |
| Broad-pattern UI warning (not a hard gate) | `policy.rs` `bash_pattern_is_broad` ~L587–593 |

### What upstream does **not** solve for this ticket

For `node -e` / `python -c` / `perl -e` / package runners:

1. `shell_dash_c_script` returns **`NotInline`** when the program is not one of the five shells (`policy.rs` L394–396).
2. Allow path then matches the **joined words string** against Bash allow patterns (`bash_words_allowed` ~L329–342; freeform glob / word-boundary prefix ~L538–560).
3. Therefore a user allow like `Bash(node*)` / `Bash(python*)` still auto-approves `node -e …` / `python -c …` at the **policy rule** layer.
4. Built-in always-safe lists in `manager/mod.rs` (`is_safe_command_words_str` ~L296–325) do **not** include interpreters, so the **default** path still prompts — but **user allow rules** are the hole this ticket targets.
5. Tests parse `python -c "…"` as plain words (`bash_command_splitting.rs` ~L1420–1430, ~L1516–1525) without a non-shell eval gate.

**Conclusion:** upstream fully hardens **shell** inline `-c` via recursion + fail-closed, and hardens chains via allow conjunction. It does **not** close the “broad program allow ⇒ interpreter inline eval auto-allow” hole. Pillow cannot copy a finished non-shell shape; it must add one with the same failure direction (unconfirmed ⇒ ask).

---

## 3. Chosen path and failure direction

### Choice: **(b) match-time downgrade**, with positive “confirmed shape” framing

When `evaluate_permission_rules` would return **Allow** for a **Process** subject, it also requires:

```text
process_subject_confirmed_for_allow(subject) == true
```

Otherwise the result is **RequireConfirmation**. Deny is never weakened. Write/Edit allows are untouched.

### Why not (a) or (c) alone

| Path | Why not primary |
| --- | --- |
| (a) create-time intercept | Needs storage/UI/host command changes outside `kernel-policy`. A pure validator could be exported later; match-time still required for corrupt/legacy rows. |
| (c) argument shape on the rule | Would need schema/DTO expansion (breaking allowlist work in other crates). Match-time shape gate achieves the same product effect for today’s string patterns without cross-crate scope. |

### Failure direction

**Not** “program ∈ interpreter denylist ⇒ ask”.

**Is** “subject shape cannot be confirmed free of opaque evaluation / command-delegation payloads ⇒ ask”.

Concretely:

1. **Code-payload short flags** (`-e` / `-c` / `-r` and short clusters ≤3 letters): unconfirmed unless the program is on a **positive** table of tools that reuse that letter for non-eval semantics (`git -c`, `grep -e`, `ps -e`, `ls -c`, `cargo`/`npm`/… for release/recursive flags, etc.). Unknown program + `-e`/`-c`/`-r` ⇒ prompt.
2. **Long options** that introduce code (`--eval`, `--print`, `--command`, `--execute`, `--expression`) ⇒ unconfirmed.
3. **Subcommand** bare token `eval` (e.g. `deno eval`) ⇒ unconfirmed.
4. **Delegation forms** that cannot be confirmed from the outer subject alone: programs `env` / `xargs` / `npx` / `pnpx` / `bunx` / `uvx`, and flags `-exec` / `-execdir` ⇒ unconfirmed.
5. Multi-character single-dash options (`-name`, `-type`) are **not** treated as short-flag clusters (avoids false positive on `find . -name …`).

New / obscure interpreters are **not** listed. If they accept `-e`/`-c`/`-r`/`--eval`, they fail closed. That is the opposite of denylist-of-interpreters.

### Code

- `crates/kernel-policy/src/permission_rules.rs` — gate + `process_subject_confirmed_for_allow`
- `crates/kernel-policy/src/lib.rs` — re-export
- `crates/kernel-policy/tests/permission_rules.rs` — adversarial + benign + deny + conjunction pins

---

## 4. Adversarial cases

All under a matching **global Process allow** (e.g. `node*`). Expected: **RequireConfirmation** (and `process_subject_confirmed_for_allow == false`).

| Subject | Pattern | Expected | Covered by |
| --- | --- | --- | --- |
| `node -e code` | `node*` | require confirm | `opaque_eval_shapes_…` |
| `node --eval code` | `node*` | require confirm | same |
| `node.exe -e code` | `node*` | require confirm | same |
| `python -c code` | `python*` | require confirm | same |
| `python3 -c code` | `python*` | require confirm | same |
| `/usr/bin/python3 -c code` | `python*` | require confirm | same |
| `perl -e code` | `perl*` | require confirm | same |
| `ruby -e code` | `ruby*` | require confirm | same |
| `php -r code` | `php*` | require confirm | same |
| `deno eval code` | `deno*` | require confirm | same |
| `sh -c cargo test` | `sh*` | require confirm | same (+ segmenter still fails closed on shell tokens when used) |
| `bash -c cargo test` | `bash*` | require confirm | same |
| `bash -xc cargo test` | `bash*` | require confirm | same |
| `env node --version` | `env*` | require confirm | same |
| `xargs rm -rf /` | `xargs*` | require confirm | same |
| `find . -exec rm {} ;` | `find*` | require confirm | same |
| `find . -execdir rm {} ;` | `find*` | require confirm | same |
| `npx cowsay hi` | `npx*` | require confirm | same |
| `uvx ruff check` | `uvx*` | require confirm | same |
| `bunx cowsay hi` | `bunx*` | require confirm | same |
| `cargo test && node -e code` (conjunction) | `cargo*` + `node*` | require confirm | `opaque_eval_segment_blocks_…` |
| `node -e code` with project deny `node -e` | allow+deny | **Deny** | `deny_still_wins_on_opaque_eval_shapes` |

---

## 5. Non-regression (must still auto-allow)

Positive controls under matching global Process allow. A “reject everything” implementation would fail these.

| Subject | Pattern | Expected |
| --- | --- | --- |
| `cargo test` | `cargo*` | Allow |
| `cargo test -p keeper-policy` | `cargo test*` | Allow |
| `npm run build` | `npm*` | Allow |
| `git status` | `git*` | Allow |
| `git -c advice.detachedHead=false status` | `git*` | Allow |
| `rustc --version` | `rustc*` | Allow |
| `node --version` | `node*` | Allow |
| `python --version` | `python*` | Allow |
| `ls -c` | `ls*` | Allow |
| `ps -ef` | `ps*` | Allow |
| `grep -e pattern file.txt` | `grep*` | Allow |
| `find . -name *.rs` | `find*` | Allow |
| `cargo test && node --version` (conjunction) | `cargo*` + `node*` | Allow (2 segments) |

Evidence: test `normal_toolchain_commands_still_auto_allow_under_matching_rules` and positive half of `opaque_eval_segment_blocks_conjunctive_allow_of_a_chain`.

Invariant checks retained: deny > allow, project allow ignored, no match ⇒ confirm, edit allow unaffected by process shape.

---

## 6. Residual risk

Worst cases this scheme still **auto-allows** under a broad program allow (e.g. `node*`, `python*`, `mycmd*`):

1. **File-based script execution** without inline-eval flags: `node evil.js`, `python evil.py`, `ruby evil.rb`. Shape has no `-e`/`-c`/`--eval`; payload is a path we do not open or classify.
2. **Unknown package/tool runners** not in the small delegation set and without code-payload flags (e.g. a new `foox install-and-run …`).
3. **Unusual eval flags** outside the confirmed set (`-e`/`-c`/`-r`/`--eval`/`--print`/`--command`/`--execute`/`--expression` and bare `eval`), e.g. some host’s `--jitcode` or `lua -l` load tricks.
4. **Quoted / unsegmentable shell** still fails the conservative segmenter to confirmation (good); if a future host path feeds a pre-joined subject that already expanded an eval payload into “normal-looking” tokens, this pure function cannot recover intent.
5. **Intentional broad allow of a whole toolchain** still grants full non-eval power of that tool (`cargo test` can run build scripts; `npm run build` runs package scripts). That is product intent for allow rules, not closed by this ticket.

Mitigations left for later stages (out of scope): richer argv-prefix rules (path c), create-time broad-pattern warnings (path a), OS sandbox, or host-side catalog constraints on structured `process.run`.

---

## 7. Files touched

| Path | Change |
| --- | --- |
| `crates/kernel-policy/src/permission_rules.rs` | Process allow shape gate + `process_subject_confirmed_for_allow` |
| `crates/kernel-policy/src/lib.rs` | Re-export |
| `crates/kernel-policy/tests/permission_rules.rs` | Adversarial + benign + deny + conjunction tests |
| `docs/execution/2026-08-11-interpreter-hardening-report.md` | This report |

No edits under `storage-sqlite`, `adapter-grok-acp`, or `apps/desktop/src-tauri`.

---

INTERPRETER-HARDENING-COMPLETE
