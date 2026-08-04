# Wave C3-D Controller Final Fix Brief

Owner: grok3 implementation worker
Controller: Codex Desktop
Keeper worker session: `<keeper-session>`
Runtime allocation: `grok3` / `<keeper-lease>`
Status: active; C3-D remains rejected

## Scope

Close the remaining controller findings after the first rejection repair. Do
not start C3-E, do not touch renderer files, and do not stage or commit.

## Remaining rejection findings

### 1. Never fabricate a terminal result when durable transition fails

`finalize_expire`, `finalize_cancel`, `finalize_interrupt` and
`reconcile_expired_for_owner` still swallow storage errors. In particular,
expiry reconciliation can construct a synthetic `StoredWriteProposal` and
publish an expired terminal marker even though SQLite never committed that
state. The cancel/expire helpers can also read a still-pending row after a
failed transition and return a contradictory terminal error.

Required invariant:

- a requested/resolved terminal event, waiter receipt or native terminal DTO is
  published only after the exact terminal state is durably readable;
- storage failure must return a fixed `Gateway`/unavailable error, never panic,
  never fabricate metadata, never claim success/expired/cancelled, and never
  trigger a second write;
- clear or quarantine live secrets/waiter state safely so no stale grant remains
  actionable; restart recovery may later reconcile the durable non-terminal;
- handle `gateway.apply(...)` errors under the same invariant instead of
  leaving a live waiter silently pending until TTL.

Add adversarial tests that make the temporary SQLite write-proposal store fail
after a proposal becomes live and prove: no resolved event/terminal DTO is
invented, provider/native gets only an opaque unavailable error, no second
write occurs, and grant/content/diff are not exposed afterward. Test seams, if
needed, must be private `#[cfg(test)]` or the existing non-default
`test-support` feature only.

### 2. Unknown applying observation is not `Neither`

`recover_write_proposals_with_roots` currently maps a missing/unresolvable
project root to `Neither`, which is a false claim that the target was observed
and matched neither digest. `recover_write_proposals` therefore marks all
orphaned applying rows failed when no resolver is supplied.

Required invariant:

- only the executor's real read-only observation may produce
  `MatchesProposed | StillBaseOrAbsent | Neither`;
- missing/unresolvable project authority must leave the applying row unchanged
  and return an explicit opaque recovery error (or an equivalent fail-closed
  result), never synthesize `Neither`;
- desktop startup must not silently ignore such a recovery failure.

Add a test proving an applying row remains applying when root authority cannot
be resolved, while valid create/modify observations still reconcile exactly.

### 3. Durable listing must work after restart with no active run

The owner-scoped free `list_write_proposals` still obtains SQLite by searching
`guard.runs`. Immediately after restart there are no bound runs, so the native
list returns empty even though startup just transitioned proposals to
`interrupted`/reconciled terminal state.

Use the `RuntimeHost` instance's own `governed_write_gateway` for durable
listing. Keep live state owner-scoped; never select an arbitrary run/gateway.
Add a restart test that creates a pending proposal, drops/reopens the host,
runs recovery, binds no new run, and lists the owning conversation's durable
`interrupted` proposal with no grant/diff.

### 4. Direct diff open must honor expiry and terminal state

`open_write_proposal_diff` currently returns live diff bytes without first
reconciling expiry and without rejecting a terminal-marked live entry. Calling
diff directly just after the deadline can expose a diff that should already be
destroyed.

Run owner-scoped expiry reconciliation before diff lookup, then require a
non-terminal entry with `now < expires_at`. Test that direct diff at/past expiry
returns `NotLive`/expired, wakes the waiter durably, and exposes no diff or
grant.

### 5. Restore the real workspace Clippy gate

Controller command:

```text
cargo clippy --workspace --all-targets -- -D warnings
```

currently fails at
`crates/kernel-resource/tests/read_only_resource_core.rs:20` on
`ResourceReader::default()` for a unit struct. The accepted baseline was full
workspace Clippy green, so it cannot be waived as unrelated. Apply only the
mechanical Clippy suggestion (`ResourceReader`) and rerun the focused resource
tests plus full workspace Clippy.

## Whitelist

- C3-D runtime/storage/executor/native/test files already authorized by the two
  prior C3-D briefs
- `crates/kernel-resource/tests/read_only_resource_core.rs` for the single
  mechanical Clippy repair only
- `docs/execution/2026-08-01-wave-c3d-runtime-native-report.md`
- this brief

No renderer files. No production-visible mutation hooks. No stage/commit.

## Required gates

Run all new RED/GREEN cases, then:

```text
cargo test -p keeper-runtime-host --features test-support --tests --lib -- --test-threads=1
cargo test -p keeper-tool-executor --tests --lib
cargo test -p keeper-storage-sqlite --tests --lib
cargo test -p keeper-adapter-grok-acp --tests --lib
cargo test -p pillow-desktop --lib write_
cargo test -p keeper-resource --tests --lib
cargo check --workspace
cargo clippy --workspace --all-targets -- -D warnings
cargo fmt --all -- --check
git diff --check
git diff --cached --name-only
```

Update the C3-D report with exact files, test counts and remaining risks. Use
`COMPLETION: DONE` only if every command is green and the staging area is empty.
