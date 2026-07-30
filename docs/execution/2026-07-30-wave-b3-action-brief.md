# Wave B3 Action Brief: Governed Read Tool Foundation

Date: 2026-07-30

Status: Active

Controller: Codex Desktop

Baseline: `782e220`

## Objective

Give Pillow the first project-aware code-reading capability without
allowing a provider, renderer, or arbitrary MCP server to obtain filesystem
authority.

The accepted flow is:

```text
provider tool proposal
  -> Pillow validates a fixed tool manifest
  -> Pillow resolves the durable project capability
  -> Pillow executes a bounded read operation
  -> Pillow persists normalized activity and receipt
  -> Pillow returns bounded output to the provider
  -> renderer projects the committed activity
```

## Locked Product Boundary

This wave may implement:

- a project-bound canonical root capability;
- provider-neutral tool proposal and result contracts;
- fixed-version `fs.list`, `fs.read`, and `fs.search` manifests;
- bounded read-only execution;
- durable tool activity and audit receipts;
- per-run activity projection needed to display the real calls.

This wave must not implement:

- file writes, patching, create, delete, rename, or destructive Git;
- a shell, terminal execution, arbitrary executable launch, or command string;
- arbitrary local or remote MCP servers;
- browser control, network tools, credentials, or key resolution;
- provider-native tools or provider-owned permission policy;
- Obsidian, company operations, multi-provider routing, or autonomous project
  discovery.

## Work Package B3-0: Project Capability Boundary

Replace the process-global path grant list with a project-scoped authority.

Required properties:

1. A selected folder is canonicalized before registration.
2. A grant is bound to the durable `project_id`.
3. Lookup requires both `project_id` and the expected canonical root.
4. Renderer input cannot register an arbitrary existing directory without a
   native-picker capability.
5. Grants cannot be reused across projects, conversations, or windows.
6. UNC, device paths, alternate data streams, traversal, reparse-point
   ambiguity, and canonicalization failure are rejected fail-closed.
7. Restart behavior is explicit. An expired ephemeral picker grant cannot
   silently become durable authority.

Acceptance:

- focused unit tests for path identity and cross-project denial;
- Windows path escape matrix;
- project command integration tests;
- no regression in open/create/select project flows.

## Work Package B3-1: Read Tool Contract And Executor

Add a provider-neutral Pillow-owned tool boundary.

Minimum proposal fields:

- `project_id`
- `conversation_id`
- `run_id`
- `call_id`
- `tool_id`
- `tool_version`
- structured arguments
- canonical argument digest

The provider cannot submit an executable, environment, cwd, effect class, or
approval level.

Initial manifests:

- `fs.list@1`
- `fs.read@1`
- `fs.search@1`

Executor limits must include output byte caps, result count caps, per-call
timeouts, cancellation checks, UTF-8/binary classification, and secret-safe
public errors.

Acceptance:

- unknown tool/version/field rejection;
- project mismatch and path-escape rejection;
- symlink/junction and replacement-race coverage where the platform permits;
- cancellation and output-limit tests;
- deterministic protocol fixtures;
- no write side effects.

## Work Package B3-2: Durable Activity Projection

Persist every tool transition before emission:

```text
proposed -> validated -> executing -> succeeded | failed | cancelled
```

The renderer must preserve activity by `run_id`; a new run cannot erase the
previous run's plan, tools, sub-agents, artifacts, or receipts.

The first UI is intentionally quiet:

- one concise activity row in the owning run;
- expandable input summary and bounded output;
- real status and timestamps;
- resource-pane navigation for readable results;
- no infrastructure IDs as primary labels.

Acceptance:

- multi-turn activity survives timeline reload;
- duplicate event replay is idempotent;
- project switching cannot leak activity;
- variable-height messages remain stable;
- restart marks non-terminal work interrupted rather than running forever.

## Sequence And Merge Gates

1. B3-0 is a hard dependency and security gate.
2. B3-1 may build against an explicit B3-0 interface but cannot merge first.
3. B3-2 may develop against fixtures in parallel, then integrate after B3-1.
4. Each package receives an independent review before integration.
5. Final acceptance requires `pnpm check`, focused desktop tests, path escape
   tests on Windows, a real Tauri recovery smoke, and a secret/residual scan.

## Controller Non-Negotiables

- `validate -> persist -> commit -> emit` remains unchanged.
- SQLite remains the only operational authority.
- Provider-native `tools=[]` and `mcpServers=[]` remain in force until the
  Pillow-owned gateway proves its acceptance suite.
- No worker may modify the legacy Keeper repository or the reference
  `<workspace>/zbbody` repository.
- Scope expansion requires a revised brief before implementation.

