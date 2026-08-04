# Wave C3-C Controller Rejection Fix Brief

Owner: grok3 implementation worker
Controller: Codex Desktop
Date: 2026-08-01

## Verdict

C3-C is not accepted. Tests are green, but controller source review found
content-privacy, state-machine, diff-correctness and post-write evidence gaps.

## Startup And Whitelist

Work only in `<workspace>\pillow`; boot as grok3 under controller session
`<keeper-session>`; read all AGENTS, the C3 design, accepted C3-A/B
reports, current C3-C report and this brief. Preserve every dirty-tree change.
No stage/commit/reset/worktree.

Allowed files only:

- `crates/tool-executor/src/write.rs`
- `crates/tool-executor/src/write_diff.rs`
- `crates/tool-executor/src/lib.rs`
- `crates/tool-executor/tests/governed_write.rs`
- `crates/tool-executor/tests/write_diff.rs`
- `docs/execution/2026-08-01-wave-c3c-write-executor-report.md`

## Required Corrections

### 1. No source-bearing Debug surface

`BaseSnapshot` currently derives `Debug` while containing `base_content`, and
`BoundedDiff` derives `Debug` while containing the unified source diff.
`PreparedWriteProposal::Debug` also includes that diff. Replace these with
redacted/no Debug implementations. Tests must place distinctive base,
proposed and diff secrets into every public structure and prove none appears in
`format!("{value:?}")`. Keep `Serialize` absent for sealed/source-bearing types.

### 2. Cancellation follows the accepted state machine

C3-A does not allow `applying -> cancelled`. Once apply is claimed, a
pre-mutation cancellation must transition `applying -> interrupted`, never
attempt an illegal transition and then disguise it as `write_failed`. Rename or
align the outcome so it cannot claim durable `Cancelled` while storing another
state. Add an exact state/history test and prove no mutation/temp residue.

### 3. Produce a valid deterministic zero-context unified diff

The current hunk header counts do not include emitted context, insertion uses
`-0,0` even when inserted after an existing prefix, and a final-newline-only
change can emit an empty hunk. Use a linear common-prefix/suffix algorithm but
emit no context unless header ranges include it correctly. Cover exact strings
for:

- create;
- replacement after a common prefix/suffix;
- insertion after N existing lines (`old_start=N, old_count=0`);
- deletion after N retained lines (`new_start=N, new_count=0`);
- add/remove final newline (must show the affected last line and marker);
- Unicode and identical text.

Keep exact byte/line caps and the explicit truncation marker. No quadratic LCS.

### 4. Verify the actual post-write target

After the OS atomic operation reports success, reopen the target through the
same regular-file/reparse/root checks, hash the actual on-disk UTF-8 bytes, and
require actual bytes/digest to equal durable proposed metadata before writing a
success receipt. Do not return a digest calculated only from sealed memory.
Add an injectable/narrow test seam if needed to prove a post-operation mismatch
cannot persist success.

For Windows modify, handle an ambiguous `ReplaceFileW` failure conservatively:
read-only inspect the target. If it now exactly equals the proposed digest and
bytes, persist success; if it is still base, persist `write_failed`; otherwise
persist opaque failure. Never run a second replacement. Create failures remain
failures even if a racer created identical content, because create requires
absence.

### 5. Non-Windows create must not overwrite a racing target

The current `exists` check followed by POSIX `rename` can overwrite a target
created in between. Use an atomic no-clobber operation such as same-filesystem
`hard_link(temp, target)` followed by temp unlink, or fail closed on unsupported
hosts. Never use replacing `rename` for create.

## Mandatory Gates

Run focused diff/write tests, all tool-executor targets, C3 protocol/policy/
storage regressions, `cargo check --workspace --all-targets`, fmt, diff check
and staging-empty check. Update the C3-C report with rejection and fresh counts;
end with `## COMPLETION: DONE` or `BLOCKED`.
