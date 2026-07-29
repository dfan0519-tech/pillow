# Slice A Desktop Acceptance

Date: 2026-07-29

Verdict: **PASS for Slice A Task 5 after rereview corrections**

Baseline commits:

- `49fe58a5cb33a7ff58aa8204a2d117340a2b7442` - initial native acceptance
- `7760a16d27d81f1f60a3750332e8239cc813e903` - initial acceptance report

The rereview correction is committed with this report. Its final hash is
recorded in the Task 5 completion message and repository history.

## Corrected Findings

The rereview identified five acceptance defects. This correction closes them
without adding product features:

1. Native acceptance and smoke set `WEBVIEW2_USER_DATA_FOLDER` to a unique
   temporary directory. The harness verifies that the temporary profile is
   populated and that the complete baseline manifest of the real
   `%LOCALAPPDATA%\com.keeperide.desktop\EBWebView` profile is unchanged.
2. Readiness is emitted only after `listProjects()` succeeds and the renderer's
   lifecycle command successfully writes the host marker. Schema 2 evidence
   contains a unique nonce, host PID, `startedAt`, app-data path, database path,
   WebView profile path, and `readyAtUnixMs`.
3. A Windows process supervisor records the launch root, captured descendants,
   `keeper-ide.exe` processes, and port 1420 owners. Cleanup still runs when the
   root has exited, checks `taskkill` results, and returns only after a fresh
   process/port snapshot proves zero known or desktop residuals.
4. `run-cargo.mjs` handles `SIGINT` and `SIGTERM`, forwards the signal, waits
   briefly, force-cleans the captured child tree, and preserves normal child
   exit codes. Its executable self-test covers exit code 7 and signal cleanup.
5. MSVC discovery now prefers `vswhere -requires
   Microsoft.VisualStudio.Component.VC.Tools.x86.x64`. Both the preferred and
   direct fallback paths must resolve and execute `cl.exe` and `link.exe`.

No cloud, Agent runtime, Obsidian, business-operation, or legacy Keeper logic
was introduced. `<workspace>/VaultSystem\keeper` remained read-only.

## Acceptance Layers

| Layer | Coverage | Result |
| --- | --- | --- |
| Renderer | Project launcher behavior and readiness success/failure gate | 5 files, 30 tests passed |
| Native acceptance | Two real debug `keeper-ide.exe` launches through WebView2 renderer `invoke`, registered Tauri commands, and temporary SQLite | Passed; one project restored after one full process restart |
| Release smoke | Release Tauri binary, schema 2 marker, temporary DB/profile, post-marker survival, 2500 ms budget | Passed; marker startup 2477 ms |
| Dev smoke | Tauri CLI, Vite, debug desktop, schema 2 marker, temporary DB/profile, post-marker survival | Passed; marker startup 44165 ms |
| Process self-test | success, thrown failure, E2E-wrapper timeout, parent-exited, port owner, known descendants | Passed |
| Cargo wrapper self-test | normal exit preservation, signal forwarding/cleanup, MSVC executable validation | Passed |
| Rust workspace | Command, SQLite unit, and repository integration tests | 6 tests passed |

The native acceptance is not a mocked gateway. Phase one executes:

```text
real WebView2 renderer
  -> create_project Tauri command
  -> AppState
  -> SqliteProjectRepository
  -> select_project Tauri command
```

The first process tree is fully terminated. Phase two starts a new desktop
process with the same temporary app-data directory and executes:

```text
real WebView2 renderer
  -> list_projects Tauri command
  -> same temporary SQLite database
  -> select_project Tauri command
```

The project ID matches across launches, `restoredProjectCount` is exactly 1,
and the observed SQLite file is 16384 bytes.

## Readiness Contract

Readiness requires both of these events:

1. the initial renderer `listProjects()` promise resolves successfully;
2. `mark_project_launcher_ready` returns successfully after atomically writing
   the host marker.

A list failure renders its safe error but does not call the lifecycle command.
A marker write failure rejects the lifecycle command and cannot create a valid
marker.

The harness generates a UUID nonce and launch lower-bound timestamp before
spawn. It rejects a marker unless:

- nonce matches exactly;
- PID is positive and belongs to the captured launch tree;
- `startedAt` is at or after the launch lower bound;
- `readyAtUnixMs` is at or after `startedAt`;
- app-data, database, and WebView profile paths match the temporary paths.

After accepting the marker, the harness observes the marker PID alive for
another 500 ms before continuing.

Release startup is measured from the marker's same-clock
`readyAtUnixMs - startedAt` values. `observedReadyMs` separately records when
the Node harness saw the marker, including Windows CIM polling overhead:

| Mode | Marker startup | Harness observed | Budget |
| --- | ---: | ---: | ---: |
| Release | 2477 ms | 3573 ms | 2500 ms |
| Dev | 44165 ms | 44807 ms | Diagnostic only |

## Data Isolation

The acceptance distinguishes three independent data surfaces:

| Surface | Test location | Assertion |
| --- | --- | --- |
| Application data | `%TEMP%\keeper-ide-*\app-data` | `KEEPER_IDE_APP_DATA_DIR` resolves inside the unique temporary root |
| SQLite database | `<temporary app-data>\keeper.db` | Real file, 16384 bytes, reused only between the two native phases |
| WebView2 profile | `%TEMP%\keeper-ide-*\webview-profile` | `WEBVIEW2_USER_DATA_FOLDER` is set before launch and the profile is populated |

The user's real WebView2 profile baseline is
`%LOCALAPPDATA%\com.keeperide.desktop\EBWebView`. Before launch and after full
process cleanup, the harness recursively snapshots every relative path, entry
kind, byte size, and modification time. Native acceptance, release smoke, dev
smoke, and the smoke self-test require exact before/after equality.

Observed temporary WebView profile entry counts:

- direct native acceptance: 240 entries;
- final release smoke: 238 entries;
- final dev smoke: 279 entries.

Temporary app data, SQLite, WebView profile, project fixture, and evidence files
are removed only after process cleanup and real-profile comparison complete.

`WEBVIEW2_USER_DATA_FOLDER` is a supported WebView2 override. Microsoft
documents that an override replaces the `userDataFolder` passed to
`CreateCoreWebView2EnvironmentWithOptions`:
<https://learn.microsoft.com/en-us/microsoft-edge/webview2/reference/win32/webview2-idl>.
The locked `wry 0.55.1` source also passes its resolved data directory to that
WebView2 API.

## Process Cleanup

The supervisor captures:

- launch root PID;
- recursive descendants while readiness/output is polled;
- every `keeper-ide.exe` PID;
- every TCP listener owner on port 1420.

On every success, failure, or timeout path it refreshes the process snapshot,
issues one checked `taskkill /T /F` request for all live known PIDs, refreshes
again, and repeats only if a survivor exists. A non-zero `taskkill` result is
accepted only when all requested PIDs are already dead. Returning from cleanup
means a fresh snapshot found no live known PID, no `keeper-ide.exe`, and no
port 1420 listener.

The executable smoke self-test proves:

- success with a root, descendant, and port 1420 listener;
- an action that throws while cleanup still runs;
- the same `runSupervisedCommand` timeout path used by the outer E2E wrapper;
- a parent that exits while its detached child remains.

The final independent query found:

```json
{"KeeperIdeProcesses":0,"Port1420Listeners":0}
```

## TDD Evidence

| Cycle | RED evidence | GREEN evidence |
| --- | --- | --- |
| Renderer readiness | Focused Vitest: 1 failed, 29 passed; the new failure-path assertion observed one invalid readiness call | Focused/full renderer: 30/30 passed |
| Smoke contract | Focused E2E saw only the old `readiness` and `processTreeCleanup` fields | Schema, profile, and four cleanup fields passed |
| Cargo wrapper | `run-cargo --self-test` was forwarded to Cargo and rejected as an unexpected argument | Executable self-test reports exit, signal cleanup, and MSVC passed |
| Parent exited | First self-test produced an unsettled top-level await because the parent exited before the listener attached | Explicit already-exited branch and detached-child cleanup passed |
| Outer timeout | CIM polling initially pushed the smoke wrapper beyond its 60 s E2E timeout | Debounced capture plus forced signal/final snapshots; focused harness 2/2 passed |

The final responsibility split keeps the process supervisor at 347 lines, the
main smoke launcher at 193 lines, and the smoke self-test at 184 lines.

## Fresh Verification

All commands ran from `<workspace>/VaultSystem\keeper-ide`.

| Exact command | Result |
| --- | --- |
| `pnpm check` | PASS, exit 0, 91.6 s; workspace check, typecheck, 30 renderer tests, 6 Rust tests, Clippy zero warnings |
| `pnpm --filter @keeper-ide/desktop test:e2e` | PASS, exit 0, 3/3, 94.566 s |
| `node scripts/native-acceptance-desktop.mjs` | PASS, exit 0; one restart, one restored project, 16384-byte DB, 240 WebView profile entries |
| `pnpm desktop:smoke` | PASS, exit 0; marker startup 2477 ms, observed 3573 ms, 238 WebView profile entries |
| `pnpm desktop:smoke:dev` | PASS, exit 0; marker startup 44165 ms, observed 44807 ms, 279 WebView profile entries |
| `pnpm desktop:smoke:self-test` | PASS, exit 0; six evidence fields passed |
| `pnpm desktop:cargo:self-test` | PASS, exit 0; exit code, signal cleanup, and MSVC passed |
| `pnpm --filter @keeper-ide/desktop build:ui` | PASS, exit 0; 46 modules, Vite 2.30 s |
| `node scripts/run-cargo.mjs fmt --all -- --check` | PASS, exit 0 |
| `git diff --check` | PASS, exit 0 |
| Process/port query | PASS; zero `keeper-ide.exe`, zero port 1420 listeners |

The final release build completed in 1 minute 9 seconds. Build time is excluded
from startup timing.

The resolved toolchain evidence was:

```text
discovery: vswhere-with-vc-component
VsDevCmd: C:\Program Files (x86)\Microsoft Visual Studio\2022\BuildTools\Common7\Tools\VsDevCmd.bat
cl: C:\Program Files (x86)\Microsoft Visual Studio\2022\BuildTools\VC\Tools\MSVC\14.44.35207\bin\Hostx64\x64\cl.exe
link: C:\Program Files (x86)\Microsoft Visual Studio\2022\BuildTools\VC\Tools\MSVC\14.44.35207\bin\Hostx64\x64\link.exe
```

## Windows Driver Boundary

No Windows WebDriver result is claimed.

Playwright alone cannot attach to the production Tauri 2 WebView2 and cross
the Tauri IPC boundary. A `tauri-driver` route would require Edge WebDriver
installation and browser/runtime version matching outside this repository.
This Task 5 replacement starts the real desktop executable and drives the
scenario from its real renderer through production Tauri commands and SQLite.
It does not automate physical mouse/keyboard interaction against the native
WebView.

No fresh Task 5 screenshot is claimed. Task 5 changes lifecycle and
verification behavior, not layout. The latest visual evidence remains the
Task 4 artifacts.

## Upstream Provenance

Task 5 consumes the accepted Task 1-4 history:

- Task 1 workspace/Tauri root: `19bd32d`, integrated by `493f1c6`;
- Task 2 project kernel/SQLite: `50b7a71`, fixes `11580c5`, `7ae8da8`;
- Task 3 typed desktop commands: `493f1c6`, hardened by `334d2b2`;
- Task 4 workbench: `2f814f7`, integrated by `493f1c6`;
- Grok App MIT correction: `1fff446`;
- titlebar capability correction: `5c926f1`;
- initial Task 5 implementation/report: `49fe58a`, `7760a16`.

Grok App provenance remains pinned to
`1c3f9c8aa3a42e869f11afb3d075f1b52843aaa9` in
`THIRD_PARTY_NOTICES.md`. This correction did not add or adapt upstream UI
code.

## Residual Risks

- There is no single physical Windows UI automation that clicks/types into the
  native WebView and then crosses Tauri IPC. Renderer interaction and real
  native persistence remain proven in adjacent automated layers.
- The Cargo executable self-test invokes the same registered signal path with a
  controlled in-process signal source. Actual Windows console Ctrl+C delivery
  depends on the launching terminal and is not separately automated.
- Release marker startup passed by 23 ms. Repeated performance sampling is
  required before treating 2500 ms as a stable release distribution rather
  than a single-machine acceptance point.
- Dev startup includes Vite and debug compilation and is diagnostic only.
- Resumed-project-shell, idle CPU, and idle memory budgets remain later gates.
- The manifest declares `pnpm@10.14.0`, while this runner used pnpm 11.9.0.
