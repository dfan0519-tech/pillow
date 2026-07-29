# Slice A Desktop Acceptance

Date: 2026-07-29

Verdict: **PASS for Slice A Task 5**

Baseline commit: `5c926f1 fix: allow custom titlebar dragging`

Implementation commit:
`49fe58a5cb33a7ff58aa8204a2d117340a2b7442`

## Accepted Scope

Task 5 now provides:

- a real-process desktop acceptance scenario for project create, select, and
  restart persistence;
- temporary application-data and SQLite isolation for all native acceptance
  and smoke runs;
- deterministic renderer-to-host readiness evidence;
- release and dev desktop smoke modes with measured startup time;
- full Windows process-tree cleanup on success and failure;
- a root `check` command that initializes the MSVC environment before Cargo
  tests and Clippy;
- an explicit desktop E2E command separate from fast renderer unit tests.

No cloud, Agent runtime, Obsidian, business-operation, or legacy Keeper logic
was introduced. `<workspace>/VaultSystem\keeper` remained read-only.

## Acceptance Layers

| Layer | Coverage | Result |
| --- | --- | --- |
| Renderer unit | Project launcher behavior, typed gateway, titlebar config, lifecycle IPC contract | 5 files, 28 tests passed |
| Native desktop acceptance | Two real `keeper-ide.exe` launches using production WebView2, renderer `invoke`, registered Tauri commands, and one temporary SQLite DB | Passed; 1 project restored after 1 full process restart |
| Release smoke | Release Tauri binary, temporary app data, renderer readiness command, SQLite creation, 2500 ms budget | Passed at 1765 ms |
| Dev smoke | Tauri CLI, Vite, debug desktop, temporary app data, renderer readiness command, SQLite creation | Passed at 33407 ms including dev compilation/startup |
| Rust workspace | Project command tests, SQLite unit tests, SQLite repository integration tests | 6 tests passed |

The native acceptance is not a browser gateway mock. Phase one starts the real
debug desktop and executes:

```text
renderer -> create_project -> AppState -> SqliteProjectRepository
renderer -> select_project -> AppState
```

The process tree is then terminated. Phase two starts a new desktop process
with the same temporary app-data directory and executes:

```text
renderer -> list_projects -> AppState -> same SQLite database
renderer -> select_project -> restored project
```

The host writes completion evidence only after the command results return.
The observed database size was 16384 bytes and the restored project count was
exactly 1.

## Data Isolation

Each run creates a unique directory below `%TEMP%`:

- native acceptance: `keeper-ide-native-*`;
- release smoke: `keeper-ide-release-*`;
- dev smoke: `keeper-ide-dev-*`.

`KEEPER_IDE_APP_DATA_DIR` points the real Tauri setup path at that directory.
Readiness and acceptance evidence include the resolved app-data and database
paths, and the harness rejects evidence outside the expected temporary root.
After process-tree termination, the complete temporary directory is removed
with bounded Windows `EBUSY` retries.

No test opens the user's normal Keeper IDE application-data database.

## TDD Evidence

| Cycle | RED evidence | GREEN evidence |
| --- | --- | --- |
| E2E entry | `pnpm --filter @keeper-ide/desktop test:e2e` failed because `native-acceptance-desktop.mjs` and `smoke-desktop.mjs` did not exist | Same command passed 2/2 |
| Renderer lifecycle | Focused Vitest failed to resolve `src/lib/desktopLifecycle` | Lifecycle suite passed 4/4; full renderer suite passed 28/28 |
| Native mock attempt | Tauri 2.11.5 `MockRuntime` test compiled but Windows loader exited `0xc0000139 STATUS_ENTRYPOINT_NOT_FOUND` | Replaced with a stronger real-process WebView2/Tauri IPC harness; no extra system dependency |
| Smoke cleanup | Self-test initially failed on an undefined process-handle check | `KEEPER_DESKTOP_SMOKE_SELF_TEST` reported readiness and process-tree cleanup passed |
| Native cleanup | First real acceptance reached all assertions, then failed final cleanup with transient SQLite `EBUSY` | Added bounded retry and moved success output after cleanup; rerun exited 0 |
| Dev host | Dev smoke timed out while Vite listened only on `[::1]:1420`; Tauri probed IPv4 localhost | Structure test failed first; dev URL and Vite host were pinned to `127.0.0.1`; dev smoke passed |

## Fresh Verification

All commands below were run from
`<workspace>/VaultSystem\keeper-ide`.

| Exact command | Result |
| --- | --- |
| `pnpm check` | PASS, exit 0, 28 renderer tests and 6 Rust tests; typecheck and workspace Clippy passed with zero warnings |
| `pnpm --filter @keeper-ide/desktop test:e2e` | PASS, exit 0, 2/2; 32.03 s total |
| `node scripts/native-acceptance-desktop.mjs` | PASS, exit 0; real Tauri WebView boundary, 1 restart, 1 restored project, 16384-byte temporary DB |
| `pnpm desktop:smoke` | PASS, exit 0; release readiness 1765 ms against 2500 ms budget |
| `pnpm desktop:smoke:dev` | PASS, exit 0; dev readiness 33407 ms |
| `pnpm --filter @keeper-ide/desktop build:ui` | PASS, exit 0; 46 modules transformed, Vite build 1.39 s |
| `node scripts/run-cargo.mjs build -p keeper-ide-desktop` | PASS, exit 0; dev build 15.19 s |
| `node scripts/run-cargo.mjs fmt --all -- --check` | PASS, exit 0 |
| Cleanup process/port query | PASS; 0 Keeper debug/release processes and 0 listeners on port 1420 |
| `git diff --check` | PASS, exit 0 |

The release smoke build itself completed in 51.48 s on the final
implementation commit. Build time is excluded from the 1765 ms startup
measurement.

## Windows Driver Boundary

No Windows WebDriver result is claimed.

Playwright alone cannot attach to the production Tauri 2 WebView2 and cross
the Tauri IPC boundary. A `tauri-driver` route would add Edge WebDriver
installation and browser/runtime version matching outside this repository.
The repository-local Tauri `MockRuntime` was also unusable on this host because
its test process failed during Windows loader startup with
`STATUS_ENTRYPOINT_NOT_FOUND`.

The accepted replacement starts the actual desktop executable and runs the
scenario from its real renderer through production Tauri commands. It covers
the native command and persistence boundary more strongly than a mocked
browser gateway, but it does not automate physical mouse/keyboard interaction
against the native WebView.

## Visual Evidence

Task 5 changed lifecycle and verification behavior, not layout. No fresh
Task 5 screenshot is claimed. The current layout evidence remains the Task 4
artifacts:

- `artifacts/keeper-ide-1440x960.png`;
- `artifacts/keeper-ide-1024x720.png`;
- `artifacts/keeper-ide-1024x720-resources.png`.

Those files were produced before Task 5 and were not modified or recommitted.

## Upstream Provenance

Task 5 consumed the accepted Task 1-4 history:

- Task 1 workspace/Tauri root: `19bd32d`, integrated by `493f1c6`;
- Task 2 project kernel/SQLite: `50b7a71`, with fixes `11580c5` and `7ae8da8`;
- Task 3 typed desktop project commands: `493f1c6`, hardened by `334d2b2`;
- Task 4 Grok App-based workbench: `2f814f7`, integrated by `493f1c6`;
- Grok App MIT correction: `1fff446`;
- final titlebar capability correction: `5c926f1`.

Grok App provenance remains pinned to
`1c3f9c8aa3a42e869f11afb3d075f1b52843aaa9` in
`THIRD_PARTY_NOTICES.md`. Task 5 did not add or adapt upstream UI code.

## Residual Risks

- There is no single Windows WebDriver test that performs a physical form
  click/type sequence and then crosses Tauri IPC. Renderer form interaction
  and real native IPC persistence are proven in adjacent automated layers.
- Dev startup includes Vite and debug compilation, so 33407 ms is diagnostic
  and is not compared with the 2500 ms release budget.
- Only cold launcher readiness is budgeted here. Resumed-project-shell,
  idle CPU, and idle memory budgets remain later performance gates.
- The manifest declares `pnpm@10.14.0`, while the available runner used for
  this acceptance was pnpm 11.9.0. The lockfile and all gates passed, but a
  clean-machine release should enforce the declared package-manager version.
- Task 4 native screenshots remain the latest visual evidence; Task 5 did not
  repeat native geometry capture because it made no visual layout change.
