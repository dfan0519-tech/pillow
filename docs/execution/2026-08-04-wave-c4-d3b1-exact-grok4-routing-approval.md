# C4 D3B1 exact Grok4 routing approval

## Authority

The user explicitly approved D3B implementation and instructed the controller
to use Grok4 heavily because that account has ample quota. This brief records
the narrow operational authority to create one reusable Pillow project role in
Keeper, bound to runtime `grok-cli` and slot `grok4`, solely so approved-write
Orchestrator tasks can be pin-or-fail routed to the requested account.
Because the user has ordered implementation to start, the controller may also
move Pillow's project lifecycle from `watch` to `active`; `watch` is an
observe-only dispatch state and would otherwise contradict that instruction.

## Boundaries

- The role changes project routing metadata only; it does not change Keeper
  code, credentials, account labels, provider keys, or shared policy.
- The lifecycle change applies only to Pillow and records D3B1 as its active
  task; no other project or workstream lifecycle may be changed.
- The role key is `grok4-development`, capability is `code`, and binding is
  `{ kind: runtime, runtimeId: grok-cli, slotId: grok4 }`.
- Each task must still use approved-write mode, an exact Pillow file allowlist,
  a unique request id, and its own completion report.
- A worker may not stage, commit, reset, clean, package, publish, or modify
  Keeper/reference repositories.
- The controller remains responsible for source review, integration, tests,
  process cleanup evidence, and final acceptance.

COMPLETION: authorized by explicit user instruction
