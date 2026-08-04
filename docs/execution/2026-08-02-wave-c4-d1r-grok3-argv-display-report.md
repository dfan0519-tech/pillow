# C4-D1R Grok3 development report — unambiguous argv display

Owner: grok3 (implementation worker)
Role: real Grok CLI slot `grok3`
Keeper worker session: `<keeper-session>`
Keeper parent session: `<keeper-session>`
Date: 2026-08-02
Task: `c4-d1r-argv-display` (controller fix for Task 4 display helper)

## Scope

Correct `formatArgvDisplay` so approval and review show sealed argv boundaries
unambiguously. Display-only encoding; execution continues to consume the
original `string[]`. No component layout changes, no product mount.

## Controller finding

`argv.join(" ")` collapses sealed argument boundaries. Example:

| argv | old display |
| --- | --- |
| `["a b", "c"]` | `a b c` |
| `["a", "b", "c"]` | `a b c` |

Those arrays were indistinguishable in the UI.

## Encoding (deterministic, display-only)

```ts
export function formatArgvDisplay(argv: readonly string[]): string {
  return argv.map((arg) => JSON.stringify(arg)).join(" ");
}
```

| Property | Behavior |
| --- | --- |
| Per-arg encoding | `JSON.stringify(arg)` |
| Separators | single space between encoded args |
| Empty argv | `""` |
| Empty string arg | `""` (visible quoted empty) |
| Quotes / backslashes / whitespace / Unicode | preserved by JSON string rules |
| Shell parsing | never; helper is pure string map/join |
| Execution | unchanged; still uses original `argv` array |

Example:

| argv | display |
| --- | --- |
| `["a b", "c"]` | `"a b" "c"` |
| `["a", "b", "c"]` | `"a" "b" "c"` |
| `["test", "-p", "keeper-tool-executor"]` | `"test" "-p" "keeper-tool-executor"` |

## Delivered (allowlist only)

| Path | Change |
| --- | --- |
| `apps/desktop/src/features/execution/executionTypes.ts` | `formatArgvDisplay` → JSON-string per arg |
| `apps/desktop/tests/c4-execution-primitives.test.tsx` | boundary unit tests + UI expectation updates |
| `docs/execution/2026-08-02-wave-c4-d1r-grok3-argv-display-report.md` | this report |

Components (`CommandActivityRow`, `CommandApprovalBar`, `TerminalOutputView`)
already call `formatArgvDisplay`; they were not modified.

## TDD evidence

### RED

Focused file failed **7** assertions under the old `join(" ")` helper:

- 3 UI surfaces expecting quoted JSON encoding
- 4 dedicated `formatArgvDisplay` cases (encoding, specials, collisions, no shell)

### GREEN

```text
corepack pnpm --filter @pillow/desktop test -- c4-execution-primitives.test.tsx
corepack pnpm --filter @pillow/desktop typecheck
corepack pnpm --filter @pillow/desktop build
git diff --check -- apps/desktop/src/features/execution/executionTypes.ts \
  apps/desktop/tests/c4-execution-primitives.test.tsx
```

| Gate | Result |
| --- | --- |
| Focused file `c4-execution-primitives.test.tsx` | **22/22** passed |
| Desktop vitest aggregate (filter still runs full suite) | **22 files / 268 tests** passed |
| Typecheck | exit 0 |
| Production UI build | exit 0 (`vite build`, ~1.23s) |
| `git diff --check` (allowlist paths) | exit 0 |
| Product mount | none (layout/components/CSS entry unchanged) |
| Staging | empty |

### Collision coverage

Tests assert pairs that collide under `join(" ")` remain distinct under the
encoding, including `["a b","c"]` vs `["a","b","c"]`, multi-token vs single
token with spaces, empty-string boundaries, embedded quotes/backslashes, and
shell-looking metacharacters treated as literal display text only.

## Explicit non-goals

- No Workbench / ResourcePane / RunActivity mount
- No Tauri gateway, process executor, policy, or protocol changes
- No stage, commit, package, or publish
- Pre-existing dirty tree outside allowlist left untouched

## Risks / residual

1. **Display format change** — UI now shows quoted JSON tokens instead of bare
   space-joined text. Expected for sealed-boundary honesty; copy still uses
   already-sanitized visible text only.
2. **Unmounted** — product still has no process/Git UI until later wiring tasks.
3. **Vitest filter** — pnpm/vitest still executes the full desktop suite when
   the file filter arg is passed; focused file is green inside the aggregate.
4. **Allowlist files remain untracked** from D1; this repair only edits those
   untracked sources plus the new report. Not staged.

## Staging / process

- Staging empty; no commit.
- No desktop E2E or real Tauri process started for this slot.
- Pillow residual processes not launched by this worker.

## Completion

**Status: DONE**

Worker complete for unambiguous argv display repair. Not controller-accepted.
