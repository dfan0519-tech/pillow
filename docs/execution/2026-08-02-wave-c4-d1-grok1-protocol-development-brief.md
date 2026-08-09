# C4-D1 Grok1 development brief — process.run protocol

Role: real Grok CLI slot `grok1`, implementation worker.

Read first:

- `root AGENTS.md`
- `shared vault rules (out of tree)`
- `this repository\AGENTS.md`
- `docs/superpowers/specs/2026-08-02-c4-governed-process-git-design.md`
- Task 1 of `docs/superpowers/plans/2026-08-02-c4-governed-process-git-implementation.md`

Boot Keeper with `project=pillow`, task `c4-d1-process-protocol`, your slot
name and `agentRole=worker` before editing.

## Deliverable

Implement Task 1 exactly with TDD: `ToolId::ProcessRun`,
`ProcessRunArguments`, `ProcessRunOutput`, process variants, fixed manifest,
wire conversion and strict validation. Values are fixed: 64 argv entries,
32,768 combined bytes, program label `^[a-z][a-z0-9_-]{0,31}$`, timeout
60,000 ms and portable cwd grammar inherited from existing filesystem tools.

## Write allowlist

- `crates/agent-protocol/src/tool/types.rs`
- `crates/agent-protocol/src/tool/manifest.rs`
- `crates/agent-protocol/src/tool/wire.rs`
- `crates/agent-protocol/src/tool/validate.rs`
- `crates/agent-protocol/tests/tool_contract.rs`
- `crates/agent-protocol/tests/tool_contract_security.rs`
- `docs/execution/2026-08-02-wave-c4-d1-grok1-protocol-development-report.md`

Do not modify any other file. Do not stage or commit. Preserve all existing
dirty hunks. Run the exact Task 1 RED/GREEN gates. The report must contain
changed files, RED proof, GREEN counts, scoped diff review, cached-diff state,
residual processes and remaining risks. End with one Keeper completion or
blocker signal.
