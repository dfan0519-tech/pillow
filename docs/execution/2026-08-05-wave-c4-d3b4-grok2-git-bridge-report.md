# C4 D3B4 Grok2 — typed Git bridge report

Date: 2026-08-05  
Worker: grok2 (Grok CLI implementation worker)  
Task: `c4-d3b4-git-bridge`  
Brief: `docs/execution/2026-08-05-wave-c4-d3b4-grok2-git-bridge-brief.md`  
Design: `docs/superpowers/specs/2026-08-05-c4-git-changes-design.md`

## Keeper

- Attempted worker boot (`agentRole: worker`) → HTTP 400 `worker claims require local parent session id` (no parent session supplied by dispatcher).
- Record-only controller boot succeeded for the same task: session `<session-id>`, project `pillow` / VK-058, agentName `grok2`.
- Keeper used for session record only; no execution, judgment, Vault query, or prompt rewrite.

## Delivered

Hardened the renderer-side typed Tauri Git bridge for the accepted read-only Git slice. No UI mounting, no React state, no Rust changes.

### Changed files (allowlist only)

| Path | Change |
| --- | --- |
| `apps/desktop/src/lib/gitGateway.ts` | Wire-contract source: six public errors, path-free messages, DTOs, `GitGateway` interface, `normalizeGitError` |
| `apps/desktop/src/lib/tauriGitGateway.ts` | Expanded strict validation; exact invokes; generation/repo echo checks; binary/unsupported field denial |
| `apps/desktop/tests/tauri-git-gateway.test.ts` | **Created** focused hostile/malformed invoke + DTO suite (18 tests) |
| `docs/execution/2026-08-05-wave-c4-d3b4-grok2-git-bridge-report.md` | This report |

No other paths were modified by this worker.

## Contract decisions

1. **Single wire source** — All public DTOs, error codes, and the `GitGateway` interface remain in `gitGateway.ts`. The Tauri adapter only validates and invokes.
2. **Exact invokes** — `discover_git_repositories` `{ projectId }`; `get_git_status` `{ projectId, repositoryId, generation }`; `open_git_diff` `{ projectId, repositoryId, generation, path }`. No root, executable, argv, or native path arguments.
3. **Portable paths** — Repository id `.` is valid. Other ids and paths must be project-relative, `/`-separated, with no absolute/native/backslash/traversal/control characters.
4. **Bounds** — Discovery ≤ 32 repos; status ≤ 5_000 files; text diff ≤ 256 KiB and ≤ 4_000 logical lines (aligned with `keeper-git-executor`).
5. **Text vs binary/unsupported** — Text requires `unified` + consistent `lineCount`/`byteCount`. Binary and unsupported **reject** any present `unified`/`lineCount`/`byteCount` (including wrong types) so no invented unified text can leak through.
6. **Generation echoes** — Repository id mismatch → `git_generation_mismatch`; generation drift → `git_generation_stale`. These codes survive gateway catch via `normalizeGitError`.
7. **Error vocabulary** — Only the six public codes. Unknown/malformed Tauri payloads become path-free `git_unavailable`. Host messages are never echoed.
8. **Out of scope (unchanged)** — No loading flags, stale UI props, repository selectors, React state, product mounting, or native Rust.

## Gates

### Focused Vitest

```powershell
pnpm --filter @pillow/desktop exec vitest run tests/tauri-git-gateway.test.ts
```

- **PASS**: 1 file, **18 tests passed**, exit 0.

```powershell
pnpm --filter @pillow/desktop test -- tests/tauri-git-gateway.test.ts
```

- Workspace script currently runs the full desktop Vitest suite with the filter argument after `--` still collected broadly; full suite also **PASS** (28 files / 341 tests, exit 0). Focused evidence is the `exec vitest run` command above.

### Typecheck

```powershell
pnpm --filter @pillow/desktop typecheck
```

- **PASS**: `tsc --noEmit`, exit 0.

### Diff check

```powershell
git diff --check -- apps/desktop/src/lib/gitGateway.ts apps/desktop/src/lib/tauriGitGateway.ts apps/desktop/tests/tauri-git-gateway.test.ts docs/execution/2026-08-05-wave-c4-d3b4-grok2-git-bridge-report.md
```

- **PASS**: exit 0 (CRLF normalization notices only; no whitespace errors).

Desktop E2E was not run (brief forbids).

## Coverage proven by tests

- Exact invoke names and camelCase argument keys only  
- Rejection of root/executable/argv/native smuggling on invoke payloads  
- Native, absolute, backslash, traversal paths on discovery/status/diff  
- Extra fields, invalid enums, unsafe integers  
- Oversized repository lists and status file lists  
- Stale generation and repository mismatch retain specific codes through the gateway  
- Line/byte inconsistency and oversized text diffs  
- Binary/unsupported text injection  
- Six public error codes with path-free fixed messages; unknown → `git_unavailable`  

## Residual risk

- Native Tauri commands (`git_commands.rs`) are a parallel Round 1A task; this bridge is proven against mocked invokes and strict DTO validation, not live Git fixtures.
- `pnpm --filter @pillow/desktop test -- <file>` may still expand the full suite depending on workspace script wiring; controllers should prefer `exec vitest run` for focused evidence.
- Duplicate Git view types in `executionTypes.ts` remain until Round 2A (Grok3) converges the pane onto `gitGateway.ts` DTOs.
- No product mounting yet; Round 2B owns App/Workbench generation guards.

## Tree hygiene

- HEAD not advanced; no stage, commit, reset, clean, or restore.
- Only allowlisted Git bridge paths were edited/created.
- Unrelated dirty/untracked work elsewhere in the tree was left untouched.
- Staging remains empty from this worker.

## Completion

C4 D3B4 Round 1B typed Git bridge delivered for controller review.
