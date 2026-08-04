/**
 * Wave C2-E real Tauri lifecycle orchestrator.
 *
 * Proves project isolation, resource coverage, conversation-owned activity,
 * restart recovery, keyboard reachability, normal window close, and process
 * cleanup through a test-scoped WebView2 CDP connection.
 *
 * CDP is enabled via a build-time Tauri `--config` override that injects
 * window `additionalBrowserArgs` (CoreWebView2EnvironmentOptions path). The
 * env-var `WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS` is intentionally not used:
 * under High Mandatory Level it does not produce a listening endpoint.
 *
 * Resource and activity UI helpers are parallel-owned imports; this file owns
 * fixtures, registration, launch/close, CDP attachment, and the terminal record.
 */
import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import { createRequire } from "node:module";
import { randomUUID } from "node:crypto";
import {
  access,
  mkdtemp,
  mkdir,
  rm,
  writeFile,
} from "node:fs/promises";
import net from "node:net";
import os from "node:os";
import path from "node:path";
import { fileURLToPath, pathToFileURL } from "node:url";
import {
  ProcessSupervisor,
  assertDesktopBaselineClean,
  captureDesktopBaseline,
  delay,
  isProcessAlive,
  runChecked,
  startCaptured,
  waitForChildExit,
  waitForDesktopProcessSnapshotClean,
  waitForJsonFile,
} from "./lib/desktop-process.mjs";
import {
  assertPathSnapshotUnchanged,
  defaultWebViewProfilePath,
  snapshotPath,
} from "./lib/desktop-profile.mjs";
import { validateReadinessEvidence } from "./lib/desktop-readiness.mjs";
import { getMsvcEnvironment } from "./lib/msvc-env.mjs";
import {
  selectProject,
  exerciseProjectResources,
  assertRestoredProjectRecents,
} from "./lib/c2-resource-ui-acceptance.mjs";
import {
  createConversationActivity,
  assertConversationActivityRestored,
  assertConversationActivityAbsent,
} from "./lib/c2-activity-ui-acceptance.mjs";

const repositoryRoot = fileURLToPath(new URL("..", import.meta.url));
const desktopRoot = path.join(repositoryRoot, "apps", "desktop");
const packageManager = process.platform === "win32" ? "cmd.exe" : "pnpm";
const packageManagerArgs = (args) =>
  process.platform === "win32"
    ? ["/d", "/s", "/c", "pnpm", ...args]
    : args;
/** Repo target is never used for acceptance builds (avoids baking CDP into it). */
const REPO_DEBUG_EXE = path.join(
  repositoryRoot,
  "target",
  "debug",
  process.platform === "win32" ? "pillow.exe" : "pillow",
);
/** Bounded wait for supervised PIDs (including WebView2 utilities) to exit naturally. */
const KNOWN_PID_GRACE_MS = 60_000;

const PROJECT_A_NAME = "Pillow C2 Project A";
const PROJECT_B_NAME = "Pillow C2 Project B";
const OWN_PATH_A = "notes/a-only.md";
const OWN_PATH_B = "notes/b-only.md";
const MARKER_A = "PILLOW_C2_A_ONLY_MARKER";
const MARKER_B = "PILLOW_C2_B_ONLY_MARKER";
const PROMPT_A = "C2 A activity persistence";
const RECORD_PREFIX = "KEEPER_WAVE_C2_RESOURCE_ACCEPTANCE";

const desktopRequire = createRequire(path.join(desktopRoot, "package.json"));
const rootRequire = createRequire(path.join(repositoryRoot, "package.json"));

function resolveModule(name) {
  try {
    return desktopRequire.resolve(name);
  } catch {
    return rootRequire.resolve(name);
  }
}

function redactText(value) {
  return String(value)
    .replace(/[A-Za-z]:\\[^\s"'`]+/gu, "<path>")
    .replace(/\/(?:tmp|var|home|Users)\/[^\s"'`]+/gu, "<path>")
    .replace(/[0-9a-f]{8}-[0-9a-f]{4}-[1-5][0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}/giu, "<uuid>");
}

/**
 * Bind then release a free loopback TCP port. The four C2-E phases share this
 * port because it is baked into the debug binary via Tauri config at build time.
 */
function allocateCdpPort() {
  return new Promise((resolve, reject) => {
    const server = net.createServer();
    server.unref();
    server.on("error", reject);
    server.listen(0, "127.0.0.1", () => {
      const address = server.address();
      if (!address || typeof address === "string") {
        server.close();
        reject(new Error("Failed to allocate a CDP port."));
        return;
      }
      const { port } = address;
      server.close((error) => {
        if (error) reject(error);
        else resolve(port);
      });
    });
  });
}

/** Fail fast if the fixed CDP port was taken between build and launch. */
function assertCdpPortFree(port) {
  return new Promise((resolve, reject) => {
    const server = net.createServer();
    server.unref();
    server.once("error", (error) => {
      reject(
        new Error(
          `Fixed CDP port ${port} is not free before launch: ${
            error instanceof Error ? error.message : String(error)
          }`,
        ),
      );
    });
    server.listen(port, "127.0.0.1", () => {
      server.close((error) => {
        if (error) reject(error);
        else resolve();
      });
    });
  });
}

/**
 * Wry replaces default browser args when additionalBrowserArgs is set, so the
 * production defaults must be re-stated alongside remote debugging.
 * @see wry WebView2 create_environment additional_browser_args
 */
function buildAdditionalBrowserArgs(cdpPort) {
  return [
    "--disable-features=msWebOOUI,msPdfOOUI,msSmartScreenProtection",
    `--remote-debugging-port=${cdpPort}`,
  ].join(" ");
}

/**
 * Acceptance-owned Tauri config fragment. JSON Merge Patch replaces the
 * `windows` array, so the full main-window surface from tauri.conf.json is
 * restated with `additionalBrowserArgs` only.
 *
 * `dataDirectory` is not injected here: Tauri config only accepts relative
 * paths (absolute paths are ignored), and a relative path would land under
 * real LocalAppData. Temp profile isolation continues via
 * WEBVIEW2_USER_DATA_FOLDER (readiness-validated).
 */
function buildAcceptanceTauriConfig(cdpPort) {
  return {
    app: {
      windows: [
        {
          label: "main",
          title: "Pillow",
          width: 1440,
          height: 960,
          minWidth: 1024,
          minHeight: 720,
          resizable: true,
          decorations: false,
          additionalBrowserArgs: buildAdditionalBrowserArgs(cdpPort),
        },
      ],
    },
  };
}

async function writeAcceptanceTauriConfig(temporaryRoot, cdpPort) {
  const configPath = path.join(temporaryRoot, "tauri.acceptance-cdp.json");
  await writeFile(
    configPath,
    `${JSON.stringify(buildAcceptanceTauriConfig(cdpPort), null, 2)}\n`,
    "utf8",
  );
  return configPath;
}

async function waitForCdpEndpoint(port, timeoutMs = 60_000) {
  const deadline = performance.now() + timeoutMs;
  let lastError = null;
  while (performance.now() < deadline) {
    try {
      const response = await fetch(`http://127.0.0.1:${port}/json/version`);
      if (response.ok) {
        await response.json();
        return;
      }
      lastError = new Error(`CDP HTTP ${response.status}`);
    } catch (error) {
      lastError = error;
    }
    await new Promise((resolve) => setTimeout(resolve, 100));
  }
  const detail = lastError instanceof Error ? lastError.message : String(lastError);
  throw new Error(
    `CDP endpoint on port ${port} not ready: ${detail}`
    + " (expected build-time additionalBrowserArgs remote-debugging-port)",
  );
}

async function loadPlaywright() {
  const playwrightModule = await import(
    pathToFileURL(resolveModule("playwright-core")).href
  );
  const playwright = playwrightModule.default ?? playwrightModule;
  if (!playwright?.chromium?.connectOverCDP) {
    throw new Error("playwright-core chromium.connectOverCDP is unavailable.");
  }
  return playwright;
}

async function connectWebViewPage(playwright, cdpPort) {
  await waitForCdpEndpoint(cdpPort);
  const browser = await playwright.chromium.connectOverCDP(
    `http://127.0.0.1:${cdpPort}`,
  );
  const context = browser.contexts()[0];
  if (!context) {
    await browser.close().catch(() => undefined);
    throw new Error("WebView2 CDP context is missing.");
  }
  let page = context.pages().find((candidate) => {
    const url = candidate.url();
    return url && !url.startsWith("devtools://");
  });
  if (!page) {
    page = context.pages()[0];
  }
  if (!page) {
    await browser.close().catch(() => undefined);
    throw new Error("WebView2 CDP page is missing.");
  }
  await page.waitForLoadState("domcontentloaded").catch(() => undefined);
  await page.waitForSelector("[data-testid='app-sidebar']", { timeout: 60_000 });
  return { browser, page };
}

async function requestNormalWindowClose(page) {
  // exact:true — otherwise "关闭资源面板" also matches the substring "关闭".
  const closeButton = page.getByRole("button", { name: "关闭", exact: true });
  await closeButton.waitFor({ state: "visible", timeout: 30_000 });
  await closeButton.click();
}

/**
 * Wait until nothing listens on the fixed CDP port (loopback).
 * Proves the build-time remote-debugging-port endpoint released with the host.
 */
async function waitForCdpPortReleased(port, timeoutMs = 15_000, label = "cdp") {
  const deadline = performance.now() + timeoutMs;
  while (performance.now() < deadline) {
    const free = await new Promise((resolve) => {
      const server = net.createServer();
      server.unref();
      server.once("error", () => resolve(false));
      server.listen(port, "127.0.0.1", () => {
        server.close(() => resolve(true));
      });
    });
    if (free) return;
    await new Promise((resolve) => setTimeout(resolve, 50));
  }
  throw new Error(
    `${label}: CDP port ${port} still bound after host exit within ${timeoutMs} ms`,
  );
}

/**
 * Describe PIDs for residual diagnostics (name only; paths redacted).
 */
function describePids(pids) {
  if (process.platform !== "win32" || pids.length === 0) {
    return pids.map(String).join(",");
  }
  const processes = snapshotWindowsProcessesWithCommandLine();
  const byPid = new Map(processes.map((entry) => [entry.pid, entry]));
  return pids
    .map((pid) => {
      const entry = byPid.get(pid);
      if (!entry) return `${pid}:?`;
      const name = entry.name || "?";
      return `${pid}:${name}`;
    })
    .join(",");
}

/**
 * True when a process is acceptance-managed (Pillow host or its WebView2 stack),
 * not a third-party injector that happened to appear under the parent PID tree
 * (e.g. SGTool.exe overlay/hooks).
 *
 * Managed set is intentionally strict:
 * - pillow.exe
 * - msedgewebview2.exe / WebView2-named processes in our launch tree
 * - any process whose command line references the acceptance executable,
 *   temp profile, or remote-debugging-port
 *
 * Orphan WebView2 utility PIDs that match remain managed and are NOT ignored.
 */
function isAcceptanceManagedProcess(entry, {
  rootPid,
  executable,
  webviewProfileDir,
  cdpPort,
}) {
  if (!entry || !Number.isInteger(entry.pid) || entry.pid <= 0) return false;
  if (entry.pid === rootPid) return true;

  const name = String(entry.name ?? "").toLowerCase();
  const cmd = String(entry.commandLine ?? "").toLowerCase();
  const exeNorm = normalizePathForCompare(executable);
  const profileNorm = normalizePathForCompare(webviewProfileDir);

  if (name === "pillow.exe") return true;

  const isWebViewName =
    name === "msedgewebview2.exe"
    || name.includes("webview2");
  if (isWebViewName) {
    // Prefer cmdline profile binding; also accept tree members captured via parent.
    if (cmd.includes(profileNorm)) return true;
    if (cmd.includes(`--remote-debugging-port=${cdpPort}`)) return true;
    // Name-matched WebView2 that was already in the supervised tree counts.
    return true;
  }

  if (cmd.includes(exeNorm)) return true;
  if (cmd.includes(profileNorm)) return true;
  if (Number.isInteger(cdpPort) && cmd.includes(`--remote-debugging-port=${cdpPort}`)) {
    return true;
  }
  return false;
}

/**
 * After ProcessSupervisor.capture expands descendants, drop non-managed PIDs
 * (third-party injectors) through its single identity-backed tracker.
 * WebView2 utilities that remain in the set stay fail-closed.
 */
function retainAcceptanceManagedKnownPids(supervisor, scope) {
  const processes = snapshotWindowsProcessesWithCommandLine();
  const byPid = new Map(processes.map((entry) => [entry.pid, entry]));
  const retained = [];
  for (const pid of supervisor.trackedPids()) {
    if (pid === scope.rootPid) {
      retained.push(pid);
      continue;
    }
    const entry = byPid.get(pid) ?? { pid, name: "", commandLine: "" };
    // If the process already exited, keep the PID so isProcessAlive can confirm death.
    // Name-less vanished PIDs are treated as managed (must be dead).
    if (!byPid.has(pid)) {
      retained.push(pid);
      continue;
    }
    if (isAcceptanceManagedProcess(entry, scope)) {
      retained.push(pid);
    }
  }
  supervisor.retainTrackedPids((pid) => retained.includes(pid));
  return retained;
}

/**
 * Wait until every already-captured *managed* supervised PID is no longer alive.
 * Freezes the PID set at entry so reparented system trees are not absorbed mid-wait.
 * Does not kill — natural exit only. Throws if any managed PID remains after graceMs.
 * Forced cleanup after this throw never counts as pass.
 */
async function waitForKnownPidsNaturalExit(supervisor, graceMs, label, scope) {
  // Expand tree while relationships are still meaningful, then keep managed only.
  await supervisor.capture(true);
  retainAcceptanceManagedKnownPids(supervisor, scope);
  const tracked = supervisor.trackedPids();
  const deadline = performance.now() + graceMs;
  let lastAlive = [];
  while (performance.now() < deadline) {
    lastAlive = tracked.filter(isProcessAlive);
    if (lastAlive.length === 0) return;
    await delay(100);
  }
  lastAlive = tracked.filter(isProcessAlive);
  if (lastAlive.length === 0) return;
  throw new Error(
    `${label}: supervised knownPids still alive after ${graceMs} ms grace: `
    + `[${describePids(lastAlive)}] (forced cleanup will run; cannot pass)`,
  );
}

/**
 * Close through the public window-control path and require a graceful process
 * exit. Forced ProcessSupervisor cleanup is only a safety net and fails the run.
 *
 * Must not call Playwright `browser.close()` before the host exits when
 * attached via connectOverCDP: that issues Browser.close against WebView2 and
 * force-tears down the host instead of proving Pillow's normal exit path.
 *
 * Strict residual contract:
 * - public "关闭" button
 * - root pillow.exe exit status 0, no signal
 * - all captured supervised knownPids exit naturally within a bounded grace
 * - CDP port releases
 * - no pillow.exe / supervised port listeners remain
 * - orphan WebView2 utility PIDs that were captured as knownPids are NOT
 *   ignorable; any survivor after grace triggers safety-net cleanup and fail
 * - forced cleanup never counts as pass
 */
async function closeDesktopNormally({
  page,
  browser,
  processRun,
  supervisor,
  baseline,
  cdpPort,
  executable,
  webviewProfileDir,
  label,
}) {
  let forcedCleanup = false;
  const managedScope = {
    rootPid: processRun.child.pid,
    executable,
    webviewProfileDir,
    cdpPort,
  };
  // Capture supervised tree (incl. WebView2 utilities), then drop injectors.
  await supervisor.capture(true);
  retainAcceptanceManagedKnownPids(supervisor, managedScope);

  try {
    await requestNormalWindowClose(page);
  } catch (error) {
    forcedCleanup = true;
    // Only force-close the CDP target when the public UI close path failed.
    await browser.close().catch(() => undefined);
    await supervisor.cleanup().catch(() => undefined);
    throw new Error(
      `${label}: normal window close request failed: ${
        error instanceof Error ? error.message : String(error)
      }`,
    );
  }

  try {
    const exit = await waitForChildExit(
      processRun.child,
      60_000,
      label,
    );
    assert.equal(
      exit.signal,
      null,
      `${label} terminated by signal ${exit.signal} instead of graceful exit`,
    );
    assert.equal(
      exit.status,
      0,
      `${label} exit status must be 0 for normal close, got ${exit.status}`,
    );
    if (Number.isInteger(processRun.child.pid) && processRun.child.pid > 0) {
      assert.equal(
        isProcessAlive(processRun.child.pid),
        false,
        `${label} root PID ${processRun.child.pid} still alive after exit`,
      );
    }
    // Drop the CDP client only after the host has exited on its own path.
    await browser.close().catch(() => undefined);

    // Strict zero residual for managed PIDs (Pillow + WebView2 stack).
    // Orphan WebView2 utilities are not ignorable; third-party injectors are
    // not acceptance-managed and are filtered before this wait.
    await waitForKnownPidsNaturalExit(
      supervisor,
      KNOWN_PID_GRACE_MS,
      label,
      managedScope,
    );

    await waitForDesktopProcessSnapshotClean({
      port: baseline.port,
      timeoutMs: 30_000,
      label: `${label} post-exit snapshot`,
    });
    await waitForCdpPortReleased(cdpPort, 15_000, label);
  } catch (error) {
    forcedCleanup = true;
    await supervisor.cleanup().catch(() => undefined);
    throw error;
  } finally {
    // Safety net only when already failed; never re-label as pass.
    if (forcedCleanup) {
      await supervisor.cleanup().catch(() => undefined);
    }
  }
}

async function writeProjectFixtures(projectRoot, relativePath, marker) {
  const absolute = path.join(projectRoot, relativePath);
  await mkdir(path.dirname(absolute), { recursive: true });
  await writeFile(absolute, `${marker}\n`, "utf8");
}

/**
 * Acceptance-owned debug path under a temporary CARGO_TARGET_DIR.
 * Never writes to repository `target/debug/pillow.exe`.
 */
function acceptanceExecutablePath(cargoTargetDir) {
  return path.join(
    cargoTargetDir,
    "debug",
    process.platform === "win32" ? "pillow.exe" : "pillow",
  );
}

/**
 * Debug-only rebuild into an acceptance-owned temporary CARGO_TARGET_DIR with
 * a merged acceptance-owned Tauri config fragment. Never overwrites the
 * repository `target/debug/pillow.exe` binary.
 */
async function ensureDebugBuild(
  environment,
  acceptanceConfigPath,
  cargoTargetDir,
) {
  await mkdir(cargoTargetDir, { recursive: true });
  const buildEnv = {
    ...environment,
    CARGO_TARGET_DIR: cargoTargetDir,
  };
  await runChecked(
    packageManager,
    packageManagerArgs([
      "--filter",
      "@pillow/desktop",
      "tauri",
      "build",
      "--debug",
      "--no-bundle",
      "--config",
      acceptanceConfigPath,
    ]),
    { cwd: repositoryRoot, env: buildEnv },
  );
  const executable = acceptanceExecutablePath(cargoTargetDir);
  await access(executable);
  // Hard guard: acceptance must not have rewritten the repo debug binary path.
  assert.notEqual(
    path.resolve(executable),
    path.resolve(REPO_DEBUG_EXE),
    "acceptance executable must not be the repository target/debug binary",
  );
  return executable;
}

/**
 * Snapshot Win32 processes including CommandLine (for WebView2 profile proof).
 * Acceptance-local — does not mutate shared desktop-process helpers.
 */
function snapshotWindowsProcessesWithCommandLine() {
  if (process.platform !== "win32") return [];
  const command = [
    "Get-CimInstance Win32_Process |",
    "ForEach-Object { [PSCustomObject]@{",
    "pid=[int]$_.ProcessId;",
    "parentPid=[int]$_.ParentProcessId;",
    "name=[string]$_.Name;",
    "commandLine=[string]$_.CommandLine",
    "} } | ConvertTo-Json -Compress",
  ].join(" ");
  const result = spawnSync(
    "powershell.exe",
    ["-NoLogo", "-NoProfile", "-NonInteractive", "-Command", command],
    { encoding: "utf8", maxBuffer: 32 * 1024 * 1024, windowsHide: true },
  );
  if (result.status !== 0) {
    throw new Error(
      `CommandLine process snapshot failed with code ${result.status}: ${result.stderr}`,
    );
  }
  const trimmed = (result.stdout ?? "").trim();
  if (!trimmed) return [];
  const value = JSON.parse(trimmed);
  const rows = Array.isArray(value) ? value : [value];
  return rows.map((entry) => ({
    commandLine: entry.commandLine ?? "",
    name: entry.name ?? "",
    parentPid: Number(entry.parentPid),
    pid: Number(entry.pid),
  }));
}

function normalizePathForCompare(value) {
  return path.resolve(String(value)).replaceAll("/", "\\").toLowerCase();
}

/**
 * Prove from a live WebView2 browser process command line (or equivalent
 * runtime evidence) that user-data-dir is under the acceptance-owned temp
 * profile. Readiness env echo alone is not sufficient.
 *
 * @returns {{ proof: "webview2-command-line", pid: number, userDataDir: string }}
 */
function assertWebView2TempProfileFromProcess(
  webviewProfileDir,
  rootPid,
  label,
) {
  if (process.platform !== "win32") {
    throw new Error(
      `${label}: WebView2 temp-profile process proof requires Windows`,
    );
  }
  const expected = normalizePathForCompare(webviewProfileDir);
  const processes = snapshotWindowsProcessesWithCommandLine();

  // Collect descendants of the supervised root (WebView2 is typically a child).
  const tree = new Set([rootPid]);
  let changed = true;
  while (changed) {
    changed = false;
    for (const entry of processes) {
      if (
        !tree.has(entry.pid)
        && tree.has(entry.parentPid)
      ) {
        tree.add(entry.pid);
        changed = true;
      }
    }
  }

  const candidates = processes.filter((entry) => {
    const name = entry.name.toLowerCase();
    const isWebView =
      name === "msedgewebview2.exe"
      || name.includes("webview2");
    if (!isWebView) return false;
    // Prefer supervised tree; also accept any WebView2 whose cmdline names our profile.
    return tree.has(entry.pid) || entry.commandLine.toLowerCase().includes(expected);
  });

  const matches = [];
  for (const entry of candidates) {
    const cmd = entry.commandLine;
    if (!cmd) continue;
    // WebView2 / Chromium style: --user-data-dir=PATH or --user-data-dir="PATH"
    const patterns = [
      /--user-data-dir=(?:"([^"]+)"|(\S+))/iu,
      /--user-data-dir\s+(?:"([^"]+)"|(\S+))/iu,
    ];
    for (const pattern of patterns) {
      const match = cmd.match(pattern);
      if (!match) continue;
      const raw = match[1] ?? match[2] ?? "";
      if (!raw) continue;
      const resolved = normalizePathForCompare(raw);
      if (
        resolved === expected
        || resolved.startsWith(`${expected}\\`)
      ) {
        matches.push({
          pid: entry.pid,
          userDataDir: path.resolve(raw),
          name: entry.name,
        });
      }
    }
    // Some builds surface the folder without the flag form; require path token.
    if (
      matches.every((m) => m.pid !== entry.pid)
      && normalizePathForCompare(cmd).includes(expected)
    ) {
      matches.push({
        pid: entry.pid,
        userDataDir: path.resolve(webviewProfileDir),
        name: entry.name,
      });
    }
  }

  if (matches.length === 0) {
    const webviewNames = processes
      .filter((p) => /webview2|msedgewebview2/iu.test(p.name))
      .map((p) => `${p.pid}:${p.name}`)
      .slice(0, 12);
    throw new Error(
      `${label}: BLOCKED — no live WebView2 process command line proves `
      + `user-data-dir under acceptance temp profile. `
      + `Env-var readiness echo is not sufficient. `
      + `observedWebView2=[${webviewNames.join(",")}] `
      + `expectedProfile=<redacted-temp>. `
      + `If High IL prevents temp profile, stop without weakening the real-profile gate.`,
    );
  }

  return {
    proof: "webview2-command-line",
    pid: matches[0].pid,
    userDataDir: matches[0].userDataDir,
    processName: matches[0].name,
  };
}

function baseLaunchEnv({
  environment,
  appDataDir,
  webviewProfileDir,
  nonce,
  launchLowerBound,
  acceptance,
}) {
  // CDP comes from baked additionalBrowserArgs, not WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS.
  const env = {
    ...environment,
    PILLOW_RUNTIME: "fake",
    PILLOW_APP_DATA_DIR: appDataDir,
    PILLOW_LAUNCH_NONCE: nonce,
    PILLOW_STARTED_AT: String(launchLowerBound),
    WEBVIEW2_USER_DATA_FOLDER: webviewProfileDir,
  };
  // Strip env-var CDP injection if the parent shell exported it; High IL
  // ignores it, and we must not pretend that path is the acceptance driver.
  delete env.WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS;
  if (acceptance) {
    env.PILLOW_ACCEPTANCE_PHASE = acceptance.phase;
    env.PILLOW_ACCEPTANCE_PROJECT_NAME = acceptance.projectName;
    env.PILLOW_ACCEPTANCE_PROJECT_ROOT = acceptance.projectRoot;
    env.PILLOW_ACCEPTANCE_RESULT = acceptance.resultPath;
  }
  return env;
}

async function launchDesktopPhase({
  environment,
  appDataDir,
  webviewProfileDir,
  cdpPort,
  executable,
  acceptance,
  label,
  proveWebViewProfile = false,
}) {
  await assertCdpPortFree(cdpPort);
  assert.equal(typeof executable, "string", `${label}: executable path required`);
  // Never launch the repository target/debug binary for acceptance.
  assert.notEqual(
    path.resolve(executable),
    path.resolve(REPO_DEBUG_EXE),
    `${label}: refusing to launch repository target/debug/pillow.exe`,
  );
  assert.ok(
    path.resolve(executable).toLowerCase().includes(`${path.sep}cargo-target${path.sep}`)
      || path.resolve(executable).toLowerCase().includes("pillow-c2e-"),
    `${label}: executable must live under acceptance-owned temp cargo target`,
  );

  const readinessPath = path.join(appDataDir, "desktop-ready.json");
  await rm(readinessPath, { force: true });
  if (acceptance?.resultPath) {
    await rm(acceptance.resultPath, { force: true });
  }

  const baseline = captureDesktopBaseline();
  assertDesktopBaselineClean(baseline);
  const nonce = randomUUID();
  const launchLowerBound = Date.now();
  const processRun = startCaptured(executable, [], {
    cwd: repositoryRoot,
    env: baseLaunchEnv({
      environment,
      appDataDir,
      webviewProfileDir,
      nonce,
      launchLowerBound,
      acceptance,
    }),
  });
  // enforceDesktopClean=false: residual proof is (1) frozen knownPids from our
  // launch tree (incl. WebView2 descendants via capture), (2) snapshot helpers
  // for pillow.exe / supervised listeners, (3) CDP port release.
  // Avoid absorbing unrelated host listeners on baseline.port into knownPids.
  const supervisor = new ProcessSupervisor({
    baseline,
    child: processRun.child,
    enforceDesktopClean: false,
  });

  try {
    const readiness = await waitForJsonFile(readinessPath, {
      label: `${label} readiness`,
      onPoll: () => supervisor.capture(),
      processHandle: processRun.child,
      timeoutMs: 90_000,
      validate: (value) =>
        validateReadinessEvidence(value, {
          appDataDir,
          launchLowerBound,
          nonce,
          webviewProfileDir,
        }),
    });
    assert.equal(readiness.pid, processRun.child.pid);
    supervisor.trackPid(readiness.pid);
    await supervisor.capture(true);

    let webviewProfileProof = null;
    if (proveWebViewProfile) {
      // Runtime process evidence — not readiness env echo.
      webviewProfileProof = assertWebView2TempProfileFromProcess(
        webviewProfileDir,
        processRun.child.pid,
        label,
      );
      console.error(
        `${RECORD_PREFIX}_WEBVIEW_PROFILE ${JSON.stringify({
          proof: webviewProfileProof.proof,
          pid: webviewProfileProof.pid,
          processName: webviewProfileProof.processName,
          // Path redacted; only confirm it is acceptance-temp.
          profileScope: "acceptance-temp",
        })}`,
      );
    }

    return {
      baseline,
      nonce,
      processRun,
      readiness,
      supervisor,
      webviewProfileProof,
    };
  } catch (error) {
    const output = processRun.output();
    await supervisor.cleanup().catch(() => undefined);
    throw new Error(
      `${error instanceof Error ? error.message : String(error)}\n`
      + `stdout:\n${output.stdout}\nstderr:\n${output.stderr}`,
    );
  }
}

async function runRegisterPhase({
  playwright,
  environment,
  appDataDir,
  webviewProfileDir,
  cdpPort,
  executable,
  projectName,
  projectRoot,
  phaseLabel,
  proveWebViewProfile = false,
}) {
  const resultPath = path.join(
    appDataDir,
    `native-acceptance-${phaseLabel}.json`,
  );
  const launched = await launchDesktopPhase({
    environment,
    appDataDir,
    webviewProfileDir,
    cdpPort,
    executable,
    acceptance: {
      phase: "create",
      projectName,
      projectRoot,
      resultPath,
    },
    label: phaseLabel,
    proveWebViewProfile,
  });

  let browser;
  try {
    const evidence = await waitForJsonFile(resultPath, {
      label: `${phaseLabel} registration evidence`,
      onPoll: () => launched.supervisor.capture(),
      processHandle: null,
      timeoutMs: 60_000,
      validate: (value) => {
        assert.equal(value.schemaVersion, 2);
        assert.equal(value.phase, "create");
        assert.equal(value.status, "passed", value.errorMessage ?? "register failed");
        assert.equal(value.ipcBoundary, "real-tauri-webview");
        assert.equal(typeof value.projectId, "string");
        assert.ok(value.projectId.length > 0);
      },
    });

    const connected = await connectWebViewPage(playwright, cdpPort);
    browser = connected.browser;
    await closeDesktopNormally({
      page: connected.page,
      browser,
      processRun: launched.processRun,
      supervisor: launched.supervisor,
      baseline: launched.baseline,
      cdpPort,
      executable,
      webviewProfileDir,
      label: phaseLabel,
    });
    browser = undefined;

    await waitForDesktopProcessSnapshotClean({
      timeoutMs: 15_000,
      label: `${phaseLabel} inter-phase snapshot`,
    });
    assertDesktopBaselineClean(captureDesktopBaseline());

    return { projectId: evidence.projectId };
  } catch (error) {
    if (browser) await browser.close().catch(() => undefined);
    await launched.supervisor.cleanup().catch(() => undefined);
    const output = launched.processRun.output();
    throw new Error(
      `${error instanceof Error ? error.message : String(error)}\n`
      + `stdout:\n${output.stdout}\nstderr:\n${output.stderr}`,
    );
  }
}

async function runUiPhase({
  playwright,
  environment,
  appDataDir,
  webviewProfileDir,
  cdpPort,
  executable,
  label,
  drive,
  proveWebViewProfile = false,
}) {
  const launched = await launchDesktopPhase({
    environment,
    appDataDir,
    webviewProfileDir,
    cdpPort,
    executable,
    acceptance: null,
    label,
    proveWebViewProfile,
  });

  let browser;
  try {
    const connected = await connectWebViewPage(playwright, cdpPort);
    browser = connected.browser;
    await drive(connected.page);
    await closeDesktopNormally({
      page: connected.page,
      browser,
      processRun: launched.processRun,
      supervisor: launched.supervisor,
      baseline: launched.baseline,
      cdpPort,
      executable,
      webviewProfileDir,
      label,
    });
    browser = undefined;

    await waitForDesktopProcessSnapshotClean({
      timeoutMs: 15_000,
      label: `${label} inter-phase snapshot`,
    });
    assertDesktopBaselineClean(captureDesktopBaseline());
  } catch (error) {
    if (browser) await browser.close().catch(() => undefined);
    await launched.supervisor.cleanup().catch(() => undefined);
    const output = launched.processRun.output();
    throw new Error(
      `${error instanceof Error ? error.message : String(error)}\n`
      + `stdout:\n${output.stdout}\nstderr:\n${output.stderr}`,
    );
  }
}

async function withStep(step, work) {
  try {
    return await work();
  } catch (error) {
    const detail = error instanceof Error ? error.message : String(error);
    throw new Error(`${step}: ${detail}`);
  }
}

/**
 * Collapse the contextual resource pane via the public stage toggle when open.
 * Keeps the conversation chrome free of an expanded resource host before turns.
 */
async function collapseResourcePaneIfOpen(page) {
  const toggle = page.getByTestId("resource-toggle");
  if ((await toggle.count()) === 0) return;
  if ((await toggle.getAttribute("aria-pressed")) === "true") {
    await toggle.click();
    await page.getByTestId("resource-pane").waitFor({
      state: "detached",
      timeout: 20_000,
    });
  }
}

async function driveCreatePhase(page) {
  await withStep("create:select-project-a", () =>
    selectProject(page, PROJECT_A_NAME),
  );
  await withStep("create:exercise-resources-a", () =>
    exerciseProjectResources({
      page,
      ownPath: OWN_PATH_A,
      ownMarker: MARKER_A,
      foreignPath: OWN_PATH_B,
      foreignMarker: MARKER_B,
    }),
  );
  await withStep("create:collapse-resource-pane", () =>
    collapseResourcePaneIfOpen(page),
  );
  await withStep("create:conversation-activity-a", async () => {
    try {
      await createConversationActivity({ page, prompt: PROMPT_A });
    } catch (error) {
      const detail = error instanceof Error ? error.message : String(error);
      const composerError = await page
        .getByTestId("composer-send-error")
        .innerText()
        .catch(() => "");
      const userCount = await page.getByTestId("timeline-user-message").count();
      const assistantCount = await page
        .getByTestId("timeline-assistant-message")
        .count();
      const conversationRows = await page.getByTestId("conversation-row").count();
      const body = await page.locator("body").innerText().catch(() => "");
      const snippet = redactText(body).slice(0, 800);
      throw new Error(
        `${detail}\n`
        + `activityDiag={composerError:${JSON.stringify(composerError)},`
        + `userMsgs:${userCount},assistantMsgs:${assistantCount},`
        + `conversationRows:${conversationRows}}\n`
        + `bodySnippet=${snippet}`,
      );
    }
  });

  await withStep("create:select-project-b", () =>
    selectProject(page, PROJECT_B_NAME),
  );
  await withStep("create:assert-activity-absent-on-b", () =>
    assertConversationActivityAbsent({
      page,
      forbiddenPrompt: PROMPT_A,
    }),
  );
  await withStep("create:exercise-resources-b", () =>
    exerciseProjectResources({
      page,
      ownPath: OWN_PATH_B,
      ownMarker: MARKER_B,
      foreignPath: OWN_PATH_A,
      foreignMarker: MARKER_A,
    }),
  );
}

/**
 * After project selection, conversation scope is cleared and no row is auto-
 * selected. Open a public conversation-row so the durable timeline loads.
 *
 * Real SQLite keeps the default title "新对话" (rename-on-send is mock-only).
 * Prefer a row whose title contains the prompt when present; otherwise open
 * the first row under the active project's conversation group.
 */
async function openConversationForPrompt(page, prompt) {
  const group = page.getByTestId("project-conversations");
  await group.waitFor({ state: "visible", timeout: 30_000 });
  const byPrompt = group.getByTestId("conversation-row").filter({
    hasText: prompt,
  });
  const byDefault = group.getByTestId("conversation-row").filter({
    hasText: "新对话",
  });
  const anyRow = group.getByTestId("conversation-row");
  let row = byPrompt;
  if ((await byPrompt.count()) === 0) {
    row = (await byDefault.count()) > 0 ? byDefault : anyRow;
  }
  await row.first().waitFor({ state: "visible", timeout: 30_000 });
  await row.first().click();
  await page
    .getByTestId("timeline-user-message")
    .filter({ hasText: prompt })
    .waitFor({ state: "visible", timeout: 30_000 });
}

async function driveRestorePhase(page) {
  await withStep("restore:select-project-a", () =>
    selectProject(page, PROJECT_A_NAME),
  );
  await withStep("restore:recents-a", () =>
    assertRestoredProjectRecents({
      page,
      ownPath: OWN_PATH_A,
      ownMarker: MARKER_A,
      foreignPath: OWN_PATH_B,
      foreignMarker: MARKER_B,
    }),
  );
  await withStep("restore:open-conversation-a", () =>
    openConversationForPrompt(page, PROMPT_A),
  );
  await withStep("restore:activity-a", () =>
    assertConversationActivityRestored({
      page,
      prompt: PROMPT_A,
      foreignPrompt: `foreign-${PROMPT_A}`,
    }),
  );

  await withStep("restore:select-project-b", () =>
    selectProject(page, PROJECT_B_NAME),
  );
  await withStep("restore:recents-b", () =>
    assertRestoredProjectRecents({
      page,
      ownPath: OWN_PATH_B,
      ownMarker: MARKER_B,
      foreignPath: OWN_PATH_A,
      foreignMarker: MARKER_A,
    }),
  );
  await withStep("restore:activity-absent-on-b", () =>
    assertConversationActivityAbsent({
      page,
      forbiddenPrompt: PROMPT_A,
    }),
  );
}

function passedRecord() {
  return {
    schemaVersion: 1,
    status: "passed",
    ipcBoundary: "real-tauri-webview",
    projectIsolation: "passed",
    restartRecovery: "passed",
    resourceCoverage: "tree-search-preview-recents",
    activityCoverage: "conversation-owned-real-events",
    keyboardReachable: "passed",
    normalClose: "passed",
    processCleanup: "passed",
    phases: ["register-a", "register-b", "create", "restore"],
  };
}

function failedRecord(error) {
  return {
    schemaVersion: 1,
    status: "failed",
    ipcBoundary: "real-tauri-webview",
    projectIsolation: "failed",
    restartRecovery: "failed",
    resourceCoverage: "failed",
    activityCoverage: "failed",
    keyboardReachable: "failed",
    normalClose: "failed",
    processCleanup: "failed",
    phases: ["register-a", "register-b", "create", "restore"],
    error: redactText(error instanceof Error ? error.message : String(error)),
  };
}

const temporaryRoot = await mkdtemp(path.join(os.tmpdir(), "pillow-c2e-"));
const appDataDir = path.join(temporaryRoot, "app-data");
const webviewProfileDir = path.join(temporaryRoot, "webview-profile");
/** Acceptance-owned cargo target — never repository `target/`. */
const cargoTargetDir = path.join(temporaryRoot, "cargo-target");
const projectARoot = path.join(temporaryRoot, "project-a");
const projectBRoot = path.join(temporaryRoot, "project-b");
const realProfile = defaultWebViewProfilePath();
const realProfileBefore = await snapshotPath(realProfile);
const startedAt = performance.now();
let summary;
let ownedPidsDead = false;
let executable = null;

try {
  await mkdir(appDataDir, { recursive: true });
  await mkdir(webviewProfileDir, { recursive: true });
  await mkdir(cargoTargetDir, { recursive: true });
  await mkdir(projectARoot, { recursive: true });
  await mkdir(projectBRoot, { recursive: true });
  await writeProjectFixtures(projectARoot, OWN_PATH_A, MARKER_A);
  await writeProjectFixtures(projectBRoot, OWN_PATH_B, MARKER_B);

  const environment = await getMsvcEnvironment();
  // One fixed free port for all four phases; baked into the acceptance binary.
  const cdpPort = await allocateCdpPort();
  const acceptanceConfigPath = await writeAcceptanceTauriConfig(
    temporaryRoot,
    cdpPort,
  );
  console.error(
    `${RECORD_PREFIX}_CDP ${JSON.stringify({
      cdpPort,
      configPath: path.basename(acceptanceConfigPath),
      driver: "tauri-config-additionalBrowserArgs",
      cargoTargetDir: "acceptance-temp",
    })}`,
  );
  executable = await ensureDebugBuild(
    environment,
    acceptanceConfigPath,
    cargoTargetDir,
  );
  const playwright = await loadPlaywright();

  await runRegisterPhase({
    playwright,
    environment,
    appDataDir,
    webviewProfileDir,
    cdpPort,
    executable,
    projectName: PROJECT_A_NAME,
    projectRoot: projectARoot,
    phaseLabel: "register-a",
    // Prove temp profile on first live launch via WebView2 process cmdline.
    proveWebViewProfile: true,
  });

  await runRegisterPhase({
    playwright,
    environment,
    appDataDir,
    webviewProfileDir,
    cdpPort,
    executable,
    projectName: PROJECT_B_NAME,
    projectRoot: projectBRoot,
    phaseLabel: "register-b",
  });

  await runUiPhase({
    playwright,
    environment,
    appDataDir,
    webviewProfileDir,
    cdpPort,
    executable,
    label: "create",
    drive: driveCreatePhase,
  });

  await runUiPhase({
    playwright,
    environment,
    appDataDir,
    webviewProfileDir,
    cdpPort,
    executable,
    label: "restore",
    drive: driveRestorePhase,
  });

  await waitForDesktopProcessSnapshotClean({
    timeoutMs: 15_000,
    label: "c2e suite end",
  });
  assertDesktopBaselineClean(captureDesktopBaseline());
  ownedPidsDead = true;

  summary = passedRecord();
  summary.durationMs = Math.round(performance.now() - startedAt);
} catch (error) {
  summary = failedRecord(error);
  summary.durationMs = Math.round(performance.now() - startedAt);
  process.exitCode = 1;
} finally {
  try {
    const realProfileAfter = await snapshotPath(realProfile);
    assertPathSnapshotUnchanged(
      realProfileBefore,
      realProfileAfter,
      "real LocalAppData WebView2 profile",
    );
  } catch (error) {
    summary = failedRecord(error);
    summary.durationMs = Math.round(performance.now() - startedAt);
    process.exitCode = 1;
  }

  // Remove acceptance-owned temporary root (incl. cargo-target) only after
  // owned processes are gone. Never leave CDP-baked binaries under repo target.
  try {
    await waitForDesktopProcessSnapshotClean({
      timeoutMs: 15_000,
      label: "pre-temp-rm snapshot",
    });
    ownedPidsDead = true;
  } catch {
    ownedPidsDead = false;
  }

  if (ownedPidsDead) {
    await rm(temporaryRoot, {
      recursive: true,
      force: true,
      maxRetries: 20,
      retryDelay: 100,
    }).catch(() => undefined);
  } else if (summary?.status === "passed") {
    summary = failedRecord(
      new Error("Refusing to delete temporary root while desktop processes remain."),
    );
    process.exitCode = 1;
  }
}

if (summary?.status === "passed") {
  console.log(`${RECORD_PREFIX} ${JSON.stringify(summary)}`);
} else {
  console.error(`${RECORD_PREFIX} ${JSON.stringify(summary)}`);
}
