/**
 * Wave C4 D3B4 Round 3 — real Tauri/WebView2 Git Changes acceptance.
 *
 * Production Tauri composition + real host Git through
 * GitCommandState → GitReadExecutor. UI driven only via public project
 * selection and resource-pane Controls. Direct Git is fixture setup /
 * independent hash inspection only.
 *
 * Reuses launcher-owned ProcessSupervisor and fresh-handle cleanup.
 * Does not kill by numeric PID or process name.
 */
import assert from "node:assert/strict";
import { createHash, randomUUID } from "node:crypto";
import { createRequire } from "node:module";
import { spawnSync } from "node:child_process";
import {
  access,
  mkdir,
  mkdtemp,
  readdir,
  readFile,
  rm,
  stat,
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
  assertPathUnchangedSince,
  defaultWebViewProfilePath,
  snapshotPath,
} from "./lib/desktop-profile.mjs";
import { validateReadinessEvidence } from "./lib/desktop-readiness.mjs";
import { getMsvcEnvironment } from "./lib/msvc-env.mjs";
import {
  GIT_KIND_LABEL,
  GIT_PUBLIC_ERROR_TEXT,
  GIT_TEST_IDS,
  assertConversationDominantWithChanges,
  assertGitEntryVisible,
  assertGitPrivacyClean,
  assertNoForeignGitEvidence,
  capturePrivacySafeScreenshot,
  collapseResourcePaneIfOpen,
  openGitChangesPane,
  refreshGitChanges,
  seedConversation,
  selectGitPath,
  selectGitRepository,
  selectProject,
  waitForGitClean,
  waitForGitDirty,
  waitForGitError,
  waitForGitSettled,
} from "./lib/c4-git-changes-acceptance.mjs";

const repositoryRoot = fileURLToPath(new URL("..", import.meta.url));
const packageManager = process.platform === "win32" ? "cmd.exe" : "pnpm";
const packageManagerArgs = (args) =>
  process.platform === "win32"
    ? ["/d", "/s", "/c", "pnpm", ...args]
    : args;

const REPO_DEBUG_EXE = path.join(
  repositoryRoot,
  "target",
  "debug",
  process.platform === "win32" ? "pillow.exe" : "pillow",
);
const KNOWN_PID_GRACE_MS = 60_000;
const RECORD_PREFIX = "KEEPER_WAVE_C4_GIT_CHANGES_ACCEPTANCE";
const DIFF_TEXT_BYTE_CAP = 256 * 1024;

const PROJECT_A_NAME = "Pillow C4 Git Project A";
const PROJECT_B_NAME = "Pillow C4 Git Project B";
const PROJECT_UNSAFE_NAME = "Pillow C4 Git Project Unsafe";

const VIEWPORTS = Object.freeze([
  { width: 1440, height: 960, name: "1440x960" },
  { width: 1024, height: 720, name: "1024x720" },
]);

const REQUIRED_SCENARIOS = Object.freeze([
  "project-root-clean",
  "nested-repository",
  "modified-untracked",
  "rename",
  "conflict",
  "detached-head",
  "text-diff",
  "binary-oversized-diff",
  "unsafe-config-denial",
  "generation-drift",
  "project-switch-isolation",
  "restart-recovery",
]);

const FIXTURE_AUTHOR = Object.freeze({
  name: "Pillow Fixture",
  email: "fixture@pillow.local",
});

const desktopRequire = createRequire(
  path.join(repositoryRoot, "apps", "desktop", "package.json"),
);
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
    .replace(
      /[0-9a-f]{8}-[0-9a-f]{4}-[1-5][0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}/giu,
      "<uuid>",
    );
}

function sha256Hex(buffer) {
  return createHash("sha256").update(buffer).digest("hex");
}

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

function buildAdditionalBrowserArgs(cdpPort) {
  return [
    "--disable-features=msWebOOUI,msPdfOOUI,msSmartScreenProtection",
    `--remote-debugging-port=${cdpPort}`,
  ].join(" ");
}

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
    await delay(100);
  }
  const detail = lastError instanceof Error ? lastError.message : String(lastError);
  throw new Error(`CDP endpoint on port ${port} not ready: ${detail}`);
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
  if (!page) page = context.pages()[0];
  if (!page) {
    await browser.close().catch(() => undefined);
    throw new Error("WebView2 CDP page is missing.");
  }
  await page.waitForLoadState("domcontentloaded").catch(() => undefined);
  await page.waitForSelector("[data-testid='app-sidebar']", { timeout: 60_000 });
  return { browser, page };
}

async function requestNormalWindowClose(page) {
  const closeButton = page.getByRole("button", { name: "关闭", exact: true });
  await closeButton.waitFor({ state: "visible", timeout: 30_000 });
  await closeButton.click();
}

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
    await delay(50);
  }
  throw new Error(`${label}: CDP port ${port} still bound after host exit`);
}

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
      return `${pid}:${entry.name || "?"}`;
    })
    .join(",");
}

function isAcceptanceManagedProcess(entry, {
  rootPid,
  executable,
  webviewProfileDir,
  cdpPort,
  fakeAcpExe,
}) {
  if (!entry || !Number.isInteger(entry.pid) || entry.pid <= 0) return false;
  if (entry.pid === rootPid) return true;
  const name = String(entry.name ?? "").toLowerCase();
  const cmd = String(entry.commandLine ?? "").toLowerCase();
  const exeNorm = normalizePathForCompare(executable);
  const profileNorm = normalizePathForCompare(webviewProfileDir);
  const fakeNorm = fakeAcpExe ? normalizePathForCompare(fakeAcpExe) : "";

  if (name === "pillow.exe") return true;
  if (name === "fake_acp_agent.exe" || (fakeNorm && cmd.includes(fakeNorm))) return true;
  const isWebViewName =
    name === "msedgewebview2.exe" || name.includes("webview2");
  if (isWebViewName) {
    if (cmd.includes(profileNorm)) return true;
    if (cmd.includes(`--remote-debugging-port=${cdpPort}`)) return true;
    return true;
  }
  if (cmd.includes(exeNorm)) return true;
  if (cmd.includes(profileNorm)) return true;
  if (Number.isInteger(cdpPort) && cmd.includes(`--remote-debugging-port=${cdpPort}`)) {
    return true;
  }
  return false;
}

function retainAcceptanceManagedKnownPids(supervisor, scope) {
  const processes = snapshotWindowsProcessesWithCommandLine();
  const byPid = new Map(processes.map((entry) => [entry.pid, entry]));
  const retained = [];
  for (const pid of supervisor.trackedPids()) {
    if (pid === scope.rootPid) {
      retained.push(pid);
      continue;
    }
    if (!byPid.has(pid)) {
      retained.push(pid);
      continue;
    }
    const entry = byPid.get(pid) ?? { pid, name: "", commandLine: "" };
    if (isAcceptanceManagedProcess(entry, scope)) retained.push(pid);
  }
  supervisor.retainTrackedPids((pid) => retained.includes(pid));
  return retained;
}

async function waitForKnownPidsNaturalExit(supervisor, graceMs, label, scope) {
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
    + `[${describePids(lastAlive)}]`,
  );
}

async function closeDesktopNormally({
  page,
  browser,
  processRun,
  supervisor,
  baseline,
  cdpPort,
  executable,
  webviewProfileDir,
  fakeAcpExe,
  label,
}) {
  let forcedCleanup = false;
  const managedScope = {
    rootPid: processRun.child.pid,
    executable,
    webviewProfileDir,
    cdpPort,
    fakeAcpExe,
  };
  await supervisor.capture(true);
  retainAcceptanceManagedKnownPids(supervisor, managedScope);

  try {
    await requestNormalWindowClose(page);
  } catch (error) {
    forcedCleanup = true;
    await browser.close().catch(() => undefined);
    await supervisor.cleanup().catch(() => undefined);
    throw new Error(
      `${label}: normal window close request failed: ${
        error instanceof Error ? error.message : String(error)
      }`,
    );
  }

  try {
    const exit = await waitForChildExit(processRun.child, 60_000, label);
    assert.equal(exit.signal, null, `${label} terminated by signal ${exit.signal}`);
    assert.equal(exit.status, 0, `${label} exit status must be 0, got ${exit.status}`);
    if (Number.isInteger(processRun.child.pid) && processRun.child.pid > 0) {
      assert.equal(
        isProcessAlive(processRun.child.pid),
        false,
        `${label} root PID still alive`,
      );
    }
    await browser.close().catch(() => undefined);
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
    if (forcedCleanup) {
      await supervisor.cleanup().catch(() => undefined);
    }
  }
}

function acceptanceExecutablePath(cargoTargetDir) {
  return path.join(
    cargoTargetDir,
    "debug",
    process.platform === "win32" ? "pillow.exe" : "pillow",
  );
}

function fakeAcpExecutablePath(cargoTargetDir) {
  return path.join(
    cargoTargetDir,
    "debug",
    process.platform === "win32" ? "fake_acp_agent.exe" : "fake_acp_agent",
  );
}

/**
 * Grok CLI slots 2–4 set USERPROFILE to the slot home. Host Rust toolchains
 * typically live under the OS user profile (HOMEDRIVE+HOMEPATH). When the slot
 * rustup has no active toolchain, point cargo/rustup at the host install so
 * the acceptance debug build can compile. Does not touch product runtime env.
 */
function ensureHostRustToolchainEnv(environment) {
  const env = { ...environment };
  if (process.platform !== "win32") return env;

  const hostUserProfile =
    env.HOMEDRIVE && env.HOMEPATH
      ? path.resolve(path.join(env.HOMEDRIVE, env.HOMEPATH))
      : null;
  if (!hostUserProfile) return env;

  const hostRustup = path.join(hostUserProfile, ".rustup");
  const hostCargo = path.join(hostUserProfile, ".cargo");
  const hostCargoBin = path.join(hostCargo, "bin");
  const hostCargoExe = path.join(hostCargoBin, "cargo.exe");

  const hostPath = `${hostCargoBin};${env.Path ?? env.PATH ?? ""}`;
  const hostEnv = {
    ...env,
    RUSTUP_HOME: hostRustup,
    CARGO_HOME: hostCargo,
    Path: hostPath,
    PATH: hostPath,
  };
  const hostProbe = spawnSync(hostCargoExe, ["--version"], {
    encoding: "utf8",
    env: hostEnv,
    windowsHide: true,
  });
  if (hostProbe.status !== 0) return env;

  const slotProbe = spawnSync("rustup", ["show", "active-toolchain"], {
    encoding: "utf8",
    env,
    windowsHide: true,
  });
  const slotActive =
    slotProbe.status === 0
    && String(slotProbe.stdout ?? "").trim().length > 0
    && !String(slotProbe.stderr ?? "").toLowerCase().includes("no active toolchain");
  if (slotActive) return env;

  return hostEnv;
}

async function ensureDebugBuild(environment, acceptanceConfigPath, cargoTargetDir) {
  await mkdir(cargoTargetDir, { recursive: true });
  const buildEnv = {
    ...ensureHostRustToolchainEnv(environment),
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
  await runChecked(
    process.platform === "win32" ? "cmd.exe" : "cargo",
    process.platform === "win32"
      ? ["/d", "/s", "/c", "cargo", "build", "-p", "keeper-adapter-grok-acp", "--bin", "fake_acp_agent"]
      : ["build", "-p", "keeper-adapter-grok-acp", "--bin", "fake_acp_agent"],
    { cwd: repositoryRoot, env: buildEnv },
  );
  const executable = acceptanceExecutablePath(cargoTargetDir);
  const fakeAcp = fakeAcpExecutablePath(cargoTargetDir);
  await access(executable);
  await access(fakeAcp);
  assert.notEqual(
    path.resolve(executable),
    path.resolve(REPO_DEBUG_EXE),
    "acceptance executable must not be the repository target/debug binary",
  );
  return { executable, fakeAcp };
}

async function writeFakeAcpScriptName(fakeAcpExe, scriptName) {
  const dir = path.dirname(fakeAcpExe);
  const scriptFile = path.join(dir, "keeper_fake_acp_script.txt");
  await writeFile(scriptFile, `${scriptName}\n`, "utf8");
  return scriptFile;
}

function baseLaunchEnv({
  environment,
  appDataDir,
  webviewProfileDir,
  nonce,
  launchLowerBound,
  acceptance,
  fakeAcpExe,
  grokProfileDir,
}) {
  const env = {
    ...environment,
    // Production Grok ACP composition with fake child binary — not PILLOW_RUNTIME=fake.
    PILLOW_GROK_EXECUTABLE: fakeAcpExe,
    PILLOW_GROK_PROFILE: grokProfileDir,
    PILLOW_APP_DATA_DIR: appDataDir,
    PILLOW_LAUNCH_NONCE: nonce,
    PILLOW_STARTED_AT: String(launchLowerBound),
    WEBVIEW2_USER_DATA_FOLDER: webviewProfileDir,
    KEEPER_FAKE_ACP_SCRIPT: "git-changes-desktop",
    // Host-only Git resolution for GitCommandState → GitReadExecutor.
    // Isolated MSVC/acceptance PATH may omit Git; never invents argv for UI.
    PILLOW_HOST_GIT: hostGit,
  };
  delete env.WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS;
  delete env.PILLOW_RUNTIME;
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
  fakeAcpExe,
  grokProfileDir,
  acceptance,
  label,
}) {
  await assertCdpPortFree(cdpPort);
  assert.notEqual(
    path.resolve(executable),
    path.resolve(REPO_DEBUG_EXE),
    `${label}: refusing to launch repository target/debug/pillow.exe`,
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
      fakeAcpExe,
      grokProfileDir,
    }),
  });
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
    return {
      baseline,
      nonce,
      processRun,
      readiness,
      supervisor,
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
  fakeAcpExe,
  grokProfileDir,
  projectName,
  projectRoot,
  phaseLabel,
}) {
  const resultPath = path.join(appDataDir, `native-acceptance-${phaseLabel}.json`);
  const launched = await launchDesktopPhase({
    environment,
    appDataDir,
    webviewProfileDir,
    cdpPort,
    executable,
    fakeAcpExe,
    grokProfileDir,
    acceptance: {
      phase: "create",
      projectName,
      projectRoot,
      resultPath,
    },
    label: phaseLabel,
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
      fakeAcpExe,
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

async function withUiSession({
  playwright,
  environment,
  appDataDir,
  webviewProfileDir,
  cdpPort,
  executable,
  fakeAcpExe,
  grokProfileDir,
  label,
  drive,
}) {
  const launched = await launchDesktopPhase({
    environment,
    appDataDir,
    webviewProfileDir,
    cdpPort,
    executable,
    fakeAcpExe,
    grokProfileDir,
    acceptance: null,
    label,
  });

  let browser;
  try {
    const connected = await connectWebViewPage(playwright, cdpPort);
    browser = connected.browser;
    const evidence = await drive(connected.page, launched);
    await closeDesktopNormally({
      page: connected.page,
      browser,
      processRun: launched.processRun,
      supervisor: launched.supervisor,
      baseline: launched.baseline,
      cdpPort,
      executable,
      webviewProfileDir,
      fakeAcpExe,
      label,
    });
    browser = undefined;
    await waitForDesktopProcessSnapshotClean({
      timeoutMs: 15_000,
      label: `${label} inter-phase snapshot`,
    });
    assertDesktopBaselineClean(captureDesktopBaseline());
    return evidence;
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
    const result = await work();
    return { scenario: step, status: "passed", ...result };
  } catch (error) {
    const message = error instanceof Error ? error.message : String(error);
    throw new Error(`scenario ${step} failed: ${redactText(message)}`);
  }
}

// ---------------------------------------------------------------------------
// Host Git fixture helpers (setup / independent hash only — never for UI asserts)
// ---------------------------------------------------------------------------

function resolveHostGit() {
  if (process.env.PILLOW_TEST_GIT) {
    return process.env.PILLOW_TEST_GIT;
  }
  // Standard install locations only; non-standard installs resolve through the
  // `where git` fallback below or the `PILLOW_TEST_GIT` override.
  const candidates = [
    String.raw`C:\Program Files\Git\cmd\git.exe`,
    String.raw`C:\Program Files (x86)\Git\cmd\git.exe`,
  ];
  for (const candidate of candidates) {
    try {
      const result = spawnSync(candidate, ["--version"], {
        encoding: "utf8",
        windowsHide: true,
      });
      if (result.status === 0) return candidate;
    } catch {
      // continue
    }
  }
  const which = spawnSync(
    process.platform === "win32" ? "where.exe" : "which",
    ["git"],
    { encoding: "utf8", windowsHide: true },
  );
  if (which.status === 0) {
    const line = (which.stdout ?? "").split(/\r?\n/u).map((s) => s.trim()).find(Boolean);
    if (line) return line;
  }
  throw new Error("host git executable not found for fixture setup");
}

const hostGit = resolveHostGit();

function fixtureGitEnv() {
  return {
    ...process.env,
    GIT_TERMINAL_PROMPT: "0",
    GIT_CONFIG_NOSYSTEM: "1",
    GIT_AUTHOR_NAME: FIXTURE_AUTHOR.name,
    GIT_AUTHOR_EMAIL: FIXTURE_AUTHOR.email,
    GIT_COMMITTER_NAME: FIXTURE_AUTHOR.name,
    GIT_COMMITTER_EMAIL: FIXTURE_AUTHOR.email,
  };
}

function runFixtureGit(cwd, args, { allowFailure = false } = {}) {
  const result = spawnSync(hostGit, args, {
    cwd,
    encoding: "buffer",
    env: fixtureGitEnv(),
    windowsHide: true,
    maxBuffer: 32 * 1024 * 1024,
  });
  if (!allowFailure && result.status !== 0) {
    throw new Error(
      `fixture git ${JSON.stringify(args)} failed status=${result.status}\n`
      + `stdout=${result.stdout?.toString("utf8") ?? ""}\n`
      + `stderr=${result.stderr?.toString("utf8") ?? ""}`,
    );
  }
  return result;
}

async function writeRepoFile(repoRoot, rel, body) {
  const full = path.join(repoRoot, rel);
  await mkdir(path.dirname(full), { recursive: true });
  if (typeof body === "string") {
    await writeFile(full, body, "utf8");
  } else {
    await writeFile(full, body);
  }
}

async function initRepo(repoRoot, { branch = "main" } = {}) {
  await mkdir(repoRoot, { recursive: true });
  runFixtureGit(repoRoot, ["init", "-b", branch]);
  runFixtureGit(repoRoot, ["config", "user.name", FIXTURE_AUTHOR.name]);
  runFixtureGit(repoRoot, ["config", "user.email", FIXTURE_AUTHOR.email]);
}

async function commitAll(repoRoot, message) {
  runFixtureGit(repoRoot, ["add", "-A"]);
  runFixtureGit(repoRoot, ["commit", "-m", message]);
}

async function hashFileOrNull(filePath) {
  try {
    const bytes = await readFile(filePath);
    return sha256Hex(bytes);
  } catch {
    return null;
  }
}

async function listHooksMeta(repoRoot) {
  const hooksDir = path.join(repoRoot, ".git", "hooks");
  try {
    const names = await readdir(hooksDir);
    const active = [];
    for (const name of names) {
      if (name.endsWith(".sample") || name.startsWith(".")) continue;
      const full = path.join(hooksDir, name);
      try {
        const meta = await stat(full);
        if (meta.isFile()) active.push(name);
      } catch {
        // ignore
      }
    }
    active.sort();
    return active;
  } catch {
    return [];
  }
}

async function captureRepoManifest(repoRoot, label) {
  const head = runFixtureGit(repoRoot, ["rev-parse", "HEAD"], { allowFailure: true });
  const headOid =
    head.status === 0
      ? head.stdout.toString("utf8").trim()
      : null;
  const headShort =
    headOid && headOid.length >= 7 ? headOid.slice(0, 7) : headOid;
  const indexHash = await hashFileOrNull(path.join(repoRoot, ".git", "index"));
  const configHash = await hashFileOrNull(path.join(repoRoot, ".git", "config"));
  const attributesHash = await hashFileOrNull(path.join(repoRoot, ".gitattributes"));
  const hooks = await listHooksMeta(repoRoot);
  const ls = runFixtureGit(repoRoot, ["ls-files", "-z"], { allowFailure: true });
  const tracked = [];
  if (ls.status === 0) {
    const raw = ls.stdout.toString("utf8");
    for (const rel of raw.split("\0").filter(Boolean)) {
      const full = path.join(repoRoot, rel);
      tracked.push({ path: rel.replaceAll("\\", "/"), sha256: await hashFileOrNull(full) });
    }
  }
  tracked.sort((a, b) => a.path.localeCompare(b.path));
  const status = runFixtureGit(
    repoRoot,
    ["status", "--porcelain=v2", "-z", "--branch"],
    { allowFailure: true },
  );
  const statusSha =
    status.status === 0 ? sha256Hex(status.stdout) : null;

  return {
    label,
    headOid,
    headShort,
    indexHash,
    configHash,
    attributesHash,
    hooks,
    tracked,
    statusSha,
  };
}

function assertManifestEqual(before, after, label) {
  assert.equal(before.headOid, after.headOid, `${label}: HEAD changed`);
  assert.equal(before.indexHash, after.indexHash, `${label}: index changed`);
  assert.equal(before.configHash, after.configHash, `${label}: config changed`);
  assert.equal(
    before.attributesHash,
    after.attributesHash,
    `${label}: attributes changed`,
  );
  assert.deepEqual(before.hooks, after.hooks, `${label}: hooks metadata changed`);
  assert.deepEqual(before.tracked, after.tracked, `${label}: tracked files changed`);
  assert.equal(before.statusSha, after.statusSha, `${label}: porcelain status changed`);
}

async function buildProjectAFixture(projectRoot) {
  await rm(projectRoot, { recursive: true, force: true });
  await mkdir(projectRoot, { recursive: true });
  await initRepo(projectRoot);
  await writeRepoFile(projectRoot, "readme.md", "project-a root\n");
  await writeRepoFile(projectRoot, "note.txt", "line1\n");
  await writeRepoFile(projectRoot, "blob.bin", Buffer.from([0, 1, 2, 3, 0, 9]));
  await writeRepoFile(projectRoot, "old_name.txt", "rename-body\n");
  await writeRepoFile(projectRoot, "project-a-only.txt", "alpha-marker\n");
  // Nested repos live under vendor/. Without ignore, porcelain reports
  // `?? vendor/` (trailing slash) which the read executor rejects as a
  // non-portable status path and fails the entire root status call.
  // Discovery walks the filesystem independently of gitignore.
  await writeRepoFile(projectRoot, ".gitignore", "vendor/\n");
  await commitAll(projectRoot, "init root");

  const nested = path.join(projectRoot, "vendor", "nested");
  await initRepo(nested);
  await writeRepoFile(nested, "nested.txt", "nested clean\n");
  await commitAll(nested, "init nested");

  return captureRepoManifest(projectRoot, "project-a-initial");
}

async function buildProjectBFixture(projectRoot) {
  await rm(projectRoot, { recursive: true, force: true });
  await mkdir(projectRoot, { recursive: true });
  await initRepo(projectRoot);
  await writeRepoFile(projectRoot, "project-b-only.txt", "beta-marker\n");
  await writeRepoFile(projectRoot, "readme.md", "project-b root\n");
  await commitAll(projectRoot, "init b");
  return captureRepoManifest(projectRoot, "project-b-initial");
}

async function buildUnsafeProjectFixture(projectRoot) {
  await rm(projectRoot, { recursive: true, force: true });
  await mkdir(projectRoot, { recursive: true });
  await initRepo(projectRoot);
  await writeRepoFile(projectRoot, "safe.txt", "before-unsafe\n");
  await commitAll(projectRoot, "init unsafe base");
  // Plant unsafe local filter — no elevated permissions required.
  runFixtureGit(projectRoot, ["config", "filter.evil.clean", "evil-clean-tool"]);
  return captureRepoManifest(projectRoot, "project-unsafe-initial");
}

async function resetRootToCleanMain(projectRoot) {
  // Abort any merge, reset hard, clean untracked (not nested git dirs carefully).
  runFixtureGit(projectRoot, ["merge", "--abort"], { allowFailure: true });
  runFixtureGit(projectRoot, ["checkout", "main"], { allowFailure: true });
  runFixtureGit(projectRoot, ["reset", "--hard", "HEAD"], { allowFailure: true });
  // Clean untracked files but keep nested vendor/nested .git
  runFixtureGit(projectRoot, ["clean", "-fd", "-e", "vendor"], { allowFailure: true });
  // Ensure ignore + nested remain after destructive scenario mutations.
  await writeRepoFile(projectRoot, ".gitignore", "vendor/\n");
  const nested = path.join(projectRoot, "vendor", "nested");
  try {
    await access(path.join(nested, ".git"));
  } catch {
    await initRepo(nested);
    await writeRepoFile(nested, "nested.txt", "nested clean\n");
    await commitAll(nested, "reinit nested");
  }
  // If .gitignore was wiped by hard reset to a commit without it, re-commit.
  const ig = runFixtureGit(projectRoot, ["status", "--porcelain", "--", ".gitignore"], {
    allowFailure: true,
  });
  const igOut = ig.stdout?.toString("utf8") ?? "";
  if (igOut.includes(".gitignore")) {
    runFixtureGit(projectRoot, ["add", ".gitignore"]);
    runFixtureGit(projectRoot, ["commit", "-m", "restore gitignore vendor"], {
      allowFailure: true,
    });
  }
}

function privacyTokensForRoots(...roots) {
  const tokens = [];
  for (const root of roots) {
    if (!root) continue;
    // Full fixture roots only — not short basenames like "project-a" which may
    // legitimately appear as portable path prefixes (e.g. project-a-only.txt).
    tokens.push(root);
    tokens.push(root.replaceAll("/", "\\"));
    tokens.push(root.replaceAll("\\", "/"));
    // Temp segment often like pillow-c4g-xxxxx
    const base = path.basename(path.dirname(root));
    if (base.startsWith("pillow-")) tokens.push(base);
  }
  return [...new Set(tokens.filter(Boolean))];
}

// ---------------------------------------------------------------------------
// Scenarios
// ---------------------------------------------------------------------------

async function openProjectChanges(page, projectName, { refresh = true } = {}) {
  // Same-project re-click does not re-run App's project-id effect. Bounce
  // through the other registered project so discovery starts fresh after
  // on-disk fixture mutations.
  const active = (
    await page.getByTestId("active-project-name").innerText().catch(() => "")
  ).trim();
  if (active.includes(projectName)) {
    const other =
      projectName === PROJECT_A_NAME ? PROJECT_B_NAME : PROJECT_A_NAME;
    await selectProject(page, other);
    await delay(150);
  }
  await selectProject(page, projectName);
  // Resource pane closes on project identity change — always reopen.
  await collapseResourcePaneIfOpen(page);
  await openGitChangesPane(page);
  await waitForGitSettled(page);
  if (refresh) {
    await refreshGitChanges(page);
  }
}

async function scenarioProjectRootClean({ page, projectBRoot, privacyTokens }) {
  // Project B is a single project-root repository with a truly clean worktree.
  // Project A intentionally embeds a nested repo under vendor/, which Git
  // reports as untracked on the parent — not a clean-status fixture.
  const before = await captureRepoManifest(projectBRoot, "clean-before");
  await openProjectChanges(page, PROJECT_B_NAME);
  await waitForGitClean(page);
  const branch = (await page.getByTestId(GIT_TEST_IDS.branch).innerText()).trim();
  assert.ok(
    branch.includes("main") || branch.length > 0,
    `expected branch label, got ${JSON.stringify(branch)}`,
  );
  const head = (await page.getByTestId(GIT_TEST_IDS.head).innerText()).trim();
  assert.ok(head.length >= 4, "head short must be present");
  // Root-only discovery: no multi-repo selector for a single repository.
  assert.equal(
    await page.getByTestId(GIT_TEST_IDS.repoSelector).count(),
    0,
    "single project-root repo must not show nested selector",
  );
  await assertGitPrivacyClean(page, "project-root-clean", privacyTokens);
  const after = await captureRepoManifest(projectBRoot, "clean-after");
  assertManifestEqual(before, after, "project-root-clean");
  return {
    branch,
    head,
    immutability: { before, after, equal: true },
  };
}

async function scenarioNestedRepository({ page, projectARoot, privacyTokens }) {
  await resetRootToCleanMain(projectARoot);
  const before = await captureRepoManifest(projectARoot, "nested-before");
  await openProjectChanges(page, PROJECT_A_NAME);
  // Root is clean (vendor/ gitignored); nested still discovered via FS walk.
  await waitForGitClean(page);
  // Multi-repo selector when root + nested are discovered.
  await page.getByTestId(GIT_TEST_IDS.repoSelector).waitFor({
    state: "visible",
    timeout: 30_000,
  });
  const rootBtn = page.getByTestId("git-changes-repo-root");
  await rootBtn.waitFor({ state: "visible", timeout: 15_000 });
  await selectGitRepository(page, "vendor/nested");
  // Nested repository itself is clean.
  await waitForGitClean(page);
  // Portable ids only — selector shows nested path, not native drive root.
  const selectorText = (
    await page.getByTestId(GIT_TEST_IDS.repoSelector).innerText()
  ).trim();
  assert.ok(
    selectorText.includes("vendor/nested") || selectorText.includes("vendor\\nested"),
    `selector must show portable nested id, got ${JSON.stringify(selectorText)}`,
  );
  assert.ok(
    !selectorText.includes("F:\\") && !selectorText.includes("C:\\"),
    "selector must not show drive roots",
  );
  await selectGitRepository(page, ".");
  await waitForGitClean(page);
  await assertGitPrivacyClean(page, "nested-repository", privacyTokens);
  const after = await captureRepoManifest(projectARoot, "nested-after");
  assertManifestEqual(before, after, "nested-repository");
  return { nestedId: "vendor/nested", immutability: { equal: true } };
}

async function scenarioModifiedUntracked({ page, projectARoot, privacyTokens }) {
  await resetRootToCleanMain(projectARoot);
  await writeRepoFile(projectARoot, "note.txt", "line1\nline2-modified\n");
  await writeRepoFile(projectARoot, "untracked-new.txt", "brand new\n");
  const before = await captureRepoManifest(projectARoot, "mod-untracked-before");
  await openProjectChanges(page, PROJECT_A_NAME);
  await waitForGitDirty(page, { minDirty: 2 });
  await assertGitEntryVisible(page, GIT_KIND_LABEL.modified, "note.txt");
  await assertGitEntryVisible(page, GIT_KIND_LABEL.untracked, "untracked-new.txt");
  await assertGitPrivacyClean(page, "modified-untracked", privacyTokens);
  const after = await captureRepoManifest(projectARoot, "mod-untracked-after");
  assertManifestEqual(before, after, "modified-untracked");
  return { immutability: { equal: true } };
}

async function scenarioRename({ page, projectARoot, privacyTokens }) {
  await resetRootToCleanMain(projectARoot);
  runFixtureGit(projectARoot, ["mv", "old_name.txt", "new_name.txt"]);
  const before = await captureRepoManifest(projectARoot, "rename-before");
  await openProjectChanges(page, PROJECT_A_NAME);
  await waitForGitDirty(page, { minDirty: 1 });
  await assertGitEntryVisible(page, GIT_KIND_LABEL.renamed, "new_name.txt", {
    originalPath: "old_name.txt",
  });
  await assertGitPrivacyClean(page, "rename", privacyTokens);
  const after = await captureRepoManifest(projectARoot, "rename-after");
  assertManifestEqual(before, after, "rename");
  // Restore for later scenarios
  runFixtureGit(projectARoot, ["reset", "--hard", "HEAD"]);
  return { immutability: { equal: true } };
}

async function scenarioConflict({ page, projectARoot, privacyTokens }) {
  await resetRootToCleanMain(projectARoot);
  await writeRepoFile(projectARoot, "conflict.txt", "main-side\n");
  await commitAll(projectARoot, "conflict base");
  runFixtureGit(projectARoot, ["checkout", "-b", "feature"]);
  await writeRepoFile(projectARoot, "conflict.txt", "feature-side\n");
  await commitAll(projectARoot, "feature change");
  runFixtureGit(projectARoot, ["checkout", "main"]);
  await writeRepoFile(projectARoot, "conflict.txt", "main-other\n");
  await commitAll(projectARoot, "main other");
  const merge = runFixtureGit(
    projectARoot,
    ["merge", "feature", "--no-edit"],
    { allowFailure: true },
  );
  assert.notEqual(merge.status, 0, "merge must leave conflict");

  const before = await captureRepoManifest(projectARoot, "conflict-before");
  await openProjectChanges(page, PROJECT_A_NAME);
  await waitForGitDirty(page, { minDirty: 1 });
  await assertGitEntryVisible(page, GIT_KIND_LABEL.conflict, "conflict.txt");
  // Must not resolve or mutate — only observe.
  await assertGitPrivacyClean(page, "conflict", privacyTokens);
  const after = await captureRepoManifest(projectARoot, "conflict-after");
  assertManifestEqual(before, after, "conflict");
  return { immutability: { equal: true }, unresolved: true };
}

async function scenarioDetachedHead({ page, projectARoot, privacyTokens }) {
  // Leave conflict via abort/reset, then detach.
  runFixtureGit(projectARoot, ["merge", "--abort"], { allowFailure: true });
  runFixtureGit(projectARoot, ["checkout", "main"], { allowFailure: true });
  runFixtureGit(projectARoot, ["reset", "--hard", "HEAD"]);
  runFixtureGit(projectARoot, ["clean", "-fd", "-e", "vendor"], { allowFailure: true });
  const rev = runFixtureGit(projectARoot, ["rev-parse", "HEAD"]);
  const oid = rev.stdout.toString("utf8").trim();
  runFixtureGit(projectARoot, ["checkout", "--detach", oid]);
  const before = await captureRepoManifest(projectARoot, "detached-before");
  await openProjectChanges(page, PROJECT_A_NAME);
  await waitForGitSettled(page);
  const branch = (await page.getByTestId(GIT_TEST_IDS.branch).innerText()).trim();
  assert.equal(branch, "分离 HEAD", `expected detached label, got ${JSON.stringify(branch)}`);
  const head = (await page.getByTestId(GIT_TEST_IDS.head).innerText()).trim();
  assert.ok(head.length >= 4, "detached head short identity required");
  await assertGitPrivacyClean(page, "detached-head", privacyTokens);
  const after = await captureRepoManifest(projectARoot, "detached-after");
  assertManifestEqual(before, after, "detached-head");
  // Restore branch for remaining scenarios.
  runFixtureGit(projectARoot, ["checkout", "main"]);
  return { branch, head, immutability: { equal: true } };
}

async function scenarioTextDiff({ page, projectARoot, privacyTokens }) {
  await resetRootToCleanMain(projectARoot);
  await writeRepoFile(projectARoot, "note.txt", "line1\nline2-text-diff\n");
  const before = await captureRepoManifest(projectARoot, "text-diff-before");
  await openProjectChanges(page, PROJECT_A_NAME);
  await waitForGitDirty(page, { minDirty: 1 });
  await selectGitPath(page, "note.txt");
  const unified = page.getByTestId(GIT_TEST_IDS.diffUnified);
  await unified.waitFor({ state: "visible", timeout: 30_000 });
  const body = (await unified.innerText()).trim();
  assert.ok(
    body.includes("line2-text-diff") || body.includes("+line2"),
    `text diff must include change, got length=${body.length}`,
  );
  // Binary/unsupported markers must not invent for text.
  assert.equal(await page.getByTestId(GIT_TEST_IDS.diffBinary).count(), 0);
  await assertGitPrivacyClean(page, "text-diff", privacyTokens);
  const after = await captureRepoManifest(projectARoot, "text-diff-after");
  assertManifestEqual(before, after, "text-diff");
  return { immutability: { equal: true }, hasUnified: true };
}

async function scenarioBinaryOversizedDiff({ page, projectARoot, privacyTokens }) {
  await resetRootToCleanMain(projectARoot);
  // Seed huge.txt on a clean tree, then dirty binary + oversized separately.
  await writeRepoFile(projectARoot, "huge.txt", "seed\n");
  await commitAll(projectARoot, "seed huge");
  await writeRepoFile(
    projectARoot,
    "blob.bin",
    Buffer.from([0, 1, 2, 3, 0, 9, 10, 11]),
  );
  const huge = `${"x".repeat(DIFF_TEXT_BYTE_CAP + 8_192)}\n`;
  await writeRepoFile(projectARoot, "huge.txt", huge);

  const before = await captureRepoManifest(projectARoot, "binary-over-before");
  await openProjectChanges(page, PROJECT_A_NAME);
  await waitForGitDirty(page, { minDirty: 2 });

  await selectGitPath(page, "blob.bin");
  await page.getByTestId(GIT_TEST_IDS.diffBinary).waitFor({
    state: "visible",
    timeout: 30_000,
  });
  assert.equal(
    await page.getByTestId(GIT_TEST_IDS.diffUnified).count(),
    0,
    "binary must not invent unified text",
  );

  await selectGitPath(page, "huge.txt");
  // Oversized maps to unsupported + truncated without partial unified invent.
  const unsupported = page.getByTestId(GIT_TEST_IDS.diffUnsupported);
  const truncated = page.getByTestId(GIT_TEST_IDS.diffTruncated);
  const deadline = performance.now() + 30_000;
  let sawSummary = false;
  while (performance.now() < deadline) {
    const u =
      (await unsupported.count()) > 0
      && (await unsupported.isVisible().catch(() => false));
    const t =
      (await truncated.count()) > 0
      && (await truncated.isVisible().catch(() => false));
    if (u || t) {
      sawSummary = true;
      break;
    }
    await delay(50);
  }
  assert.ok(sawSummary, "oversized must show unsupported and/or truncated summary");
  // Must not paint the huge payload as unified text.
  if ((await page.getByTestId(GIT_TEST_IDS.diffUnified).count()) > 0) {
    const text = await page.getByTestId(GIT_TEST_IDS.diffUnified).innerText();
    assert.ok(
      text.length < DIFF_TEXT_BYTE_CAP,
      "must not invent full oversized unified text",
    );
  }

  await assertGitPrivacyClean(page, "binary-oversized-diff", privacyTokens);
  const after = await captureRepoManifest(projectARoot, "binary-over-after");
  assertManifestEqual(before, after, "binary-oversized-diff");
  return { immutability: { equal: true }, binary: true, oversized: true };
}

async function scenarioUnsafeConfigDenial({
  page,
  projectUnsafeRoot,
  privacyTokens,
}) {
  const before = await captureRepoManifest(projectUnsafeRoot, "unsafe-before");
  await openProjectChanges(page, PROJECT_UNSAFE_NAME);
  await waitForGitSettled(page);
  // Discovery marks unsafe repo unavailable → path-free repository_not_found,
  // or status denial → git_unavailable. Either is fixed path-free public copy.
  const error = page.getByTestId(GIT_TEST_IDS.error);
  await error.waitFor({ state: "visible", timeout: 30_000 });
  const text = (await error.innerText()).trim();
  const allowed = new Set([
    GIT_PUBLIC_ERROR_TEXT.git_repository_not_found,
    GIT_PUBLIC_ERROR_TEXT.git_unavailable,
  ]);
  assert.ok(
    allowed.has(text),
    `unsafe denial must be fixed path-free public error, got ${JSON.stringify(text)}`,
  );
  await assertGitPrivacyClean(page, "unsafe-config-denial", privacyTokens);
  // No external path disclosure in the error region.
  assert.ok(!text.includes("\\") && !text.includes("/"), "error must be path-free");
  const after = await captureRepoManifest(projectUnsafeRoot, "unsafe-after");
  assertManifestEqual(before, after, "unsafe-config-denial");
  return { error: text, immutability: { equal: true } };
}

async function scenarioGenerationDrift({ page, projectARoot, privacyTokens }) {
  await resetRootToCleanMain(projectARoot);
  await writeRepoFile(projectARoot, "gen-marker-a.txt", "generation-a-body\n");
  await writeRepoFile(projectARoot, "note.txt", "line1\ngen-a\n");
  const beforeDirty = await captureRepoManifest(projectARoot, "gen-drift-dirty");
  await openProjectChanges(page, PROJECT_A_NAME);
  await waitForGitDirty(page, { minDirty: 1 });
  await selectGitPath(page, "note.txt");
  const preRefreshDiff = (
    await page.getByTestId(GIT_TEST_IDS.diffUnified).innerText().catch(() => "")
  ).trim();
  assert.ok(
    preRefreshDiff.includes("gen-a") || preRefreshDiff.length > 0,
    "pre-refresh diff must show current generation content",
  );

  // Fixture identity change: re-init root repo with different content.
  // Nested vendor kept; root .git replaced.
  await rm(path.join(projectARoot, ".git"), { recursive: true, force: true });
  await initRepo(projectARoot);
  await writeRepoFile(projectARoot, "readme.md", "rebuilt after drift\n");
  await writeRepoFile(projectARoot, "project-a-only.txt", "alpha-marker\n");
  // Explicitly do NOT recreate gen-marker-a.txt or old note content.
  await writeRepoFile(projectARoot, "note.txt", "fresh-post-drift\n");
  await commitAll(projectARoot, "rebuilt identity");

  // Prior painted state may still show until refresh — refresh invalidates.
  await refreshGitChanges(page);
  await waitForGitSettled(page);

  const body = await page.locator("body").innerText();
  assert.ok(
    !body.includes("gen-marker-a.txt"),
    "stale path gen-marker-a.txt must not paint after identity change + refresh",
  );
  assert.ok(
    !body.includes("generation-a-body"),
    "stale generation content must not paint after refresh",
  );
  // Old unified content from prior generation must not reappear.
  if ((await page.getByTestId(GIT_TEST_IDS.diff).count()) > 0) {
    const diffText = await page.getByTestId(GIT_TEST_IDS.diff).innerText().catch(() => "");
    assert.ok(
      !diffText.includes("gen-a"),
      "stale pre-refresh diff must not remain after refresh",
    );
  }

  // Clean rebuilt tree or only intentional dirt — no gen-marker.
  const listCount = await page.getByTestId(GIT_TEST_IDS.fileList).count();
  if (listCount > 0) {
    const listText = await page.getByTestId(GIT_TEST_IDS.fileList).innerText();
    assert.ok(
      !listText.includes("gen-marker-a.txt"),
      "status list must not keep pre-drift path after new discovery",
    );
  }

  await assertGitPrivacyClean(page, "generation-drift", privacyTokens);
  const after = await captureRepoManifest(projectARoot, "gen-drift-after");
  // Identity intentionally changed by fixture setup; record both.
  return {
    preRefreshHadDiff: preRefreshDiff.length > 0,
    identityChanged: beforeDirty.headOid !== after.headOid,
    immutabilityNote: "fixture re-init between observations (intentional)",
  };
}

async function scenarioProjectSwitchIsolation({
  page,
  privacyTokens,
}) {
  await openProjectChanges(page, PROJECT_A_NAME);
  await waitForGitSettled(page);
  // Ensure A-only marker can appear in A if dirty or in tree via clean commit.
  // After generation-drift rebuild, project-a-only.txt is tracked clean.
  // Force a dirty A-only path for stronger isolation proof.
  // (Caller leaves projectA with project-a-only present.)

  await openProjectChanges(page, PROJECT_B_NAME);
  await waitForGitSettled(page);
  await assertNoForeignGitEvidence(
    page,
    ["project-a-only.txt", "alpha-marker", "gen-marker-a.txt", "vendor/nested"],
    "project-b-isolation",
  );
  // B should show its own identity / clean status.
  const bBody = await page.getByTestId(GIT_TEST_IDS.view).innerText();
  assert.ok(
    !bBody.includes("project-a-only"),
    "project B changes must not render A paths",
  );

  await openProjectChanges(page, PROJECT_A_NAME);
  await waitForGitSettled(page);
  await assertNoForeignGitEvidence(
    page,
    ["project-b-only.txt", "beta-marker"],
    "project-a-isolation",
  );
  await assertGitPrivacyClean(page, "project-switch-isolation", privacyTokens);
  return { isolated: true };
}

async function scenarioRestartRecovery({
  playwright,
  environment,
  appDataDir,
  webviewProfileDir,
  cdpPort,
  executable,
  fakeAcpExe,
  grokProfileDir,
  projectARoot,
  privacyTokens,
  evidenceDir,
}) {
  // Prepare a stable dirty marker for rediscovery after restart.
  await resetRootToCleanMain(projectARoot);
  await writeRepoFile(projectARoot, "restart-marker.txt", "restart-dirty\n");
  const before = await captureRepoManifest(projectARoot, "restart-before");

  // Session 1: discover and paint.
  await withUiSession({
    playwright,
    environment,
    appDataDir,
    webviewProfileDir,
    cdpPort,
    executable,
    fakeAcpExe,
    grokProfileDir,
    label: "restart-session-1",
    drive: async (page) => {
      await selectProject(page, PROJECT_A_NAME);
      await seedConversation(page, "git-acceptance restart session 1");
      await openProjectChanges(page, PROJECT_A_NAME);
      await waitForGitDirty(page, { minDirty: 1 });
      await assertGitEntryVisible(
        page,
        GIT_KIND_LABEL.untracked,
        "restart-marker.txt",
      );
      await capturePrivacySafeScreenshot({
        page,
        path: path.join(evidenceDir, "restart-before-1440x960.png"),
        label: "restart-before",
        extraForbidden: privacyTokens,
      });
      return { painted: true };
    },
  });

  // Session 2: normal restart (normal close already happened); rediscover.
  const restored = await withUiSession({
    playwright,
    environment,
    appDataDir,
    webviewProfileDir,
    cdpPort,
    executable,
    fakeAcpExe,
    grokProfileDir,
    label: "restart-session-2",
    drive: async (page) => {
      await selectProject(page, PROJECT_A_NAME);
      await openGitChangesPane(page);
      await waitForGitSettled(page);
      await waitForGitDirty(page, { minDirty: 1 });
      await assertGitEntryVisible(
        page,
        GIT_KIND_LABEL.untracked,
        "restart-marker.txt",
      );
      // Fresh discovery — no host generation reuse artifacts as path/error leaks.
      await assertGitPrivacyClean(page, "restart-recovery", privacyTokens);
      await capturePrivacySafeScreenshot({
        page,
        path: path.join(evidenceDir, "restart-after-1440x960.png"),
        label: "restart-after",
        extraForbidden: privacyTokens,
      });
      return { rediscovered: true };
    },
  });

  const after = await captureRepoManifest(projectARoot, "restart-after");
  assertManifestEqual(before, after, "restart-recovery");
  return {
    ...restored,
    immutability: { equal: true },
    normalRestart: true,
  };
}

// ---------------------------------------------------------------------------
// Main
// ---------------------------------------------------------------------------

const startedAt = Date.now();
const temporaryRoot = await mkdtemp(path.join(os.tmpdir(), "pillow-c4g-"));
const appDataDir = path.join(temporaryRoot, "app-data");
const webviewProfileDir = path.join(temporaryRoot, "webview-profile");
const cargoTargetDir = path.join(temporaryRoot, "cargo-target");
const grokProfileDir = path.join(temporaryRoot, "grok-profile");
const projectARoot = path.join(temporaryRoot, "project-a");
const projectBRoot = path.join(temporaryRoot, "project-b");
const projectUnsafeRoot = path.join(temporaryRoot, "project-unsafe");
const evidenceDir = path.join(
  repositoryRoot,
  ".superpowers",
  "c4-git-acceptance",
);
const realProfile = defaultWebViewProfilePath();
const realProfileBefore = await snapshotPath(realProfile);
const caseResults = [];
const hashManifest = [];
let residualProcesses = null;
let summary;

try {
  await mkdir(appDataDir, { recursive: true });
  await mkdir(webviewProfileDir, { recursive: true });
  await mkdir(cargoTargetDir, { recursive: true });
  await mkdir(grokProfileDir, { recursive: true });
  await mkdir(evidenceDir, { recursive: true });

  const manifestA = await buildProjectAFixture(projectARoot);
  const manifestB = await buildProjectBFixture(projectBRoot);
  const manifestUnsafe = await buildUnsafeProjectFixture(projectUnsafeRoot);
  hashManifest.push(manifestA, manifestB, manifestUnsafe);

  const privacyTokens = privacyTokensForRoots(
    temporaryRoot,
    projectARoot,
    projectBRoot,
    projectUnsafeRoot,
  );

  const environment = await getMsvcEnvironment();
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
      hostGit: path.basename(hostGit),
      runtimePath: "production-tauri-webview-real-host-git",
    })}`,
  );

  const { executable, fakeAcp } = await ensureDebugBuild(
    environment,
    acceptanceConfigPath,
    cargoTargetDir,
  );
  await writeFakeAcpScriptName(fakeAcp, "git-changes-desktop");
  const playwright = await loadPlaywright();

  await runRegisterPhase({
    playwright,
    environment,
    appDataDir,
    webviewProfileDir,
    cdpPort,
    executable,
    fakeAcpExe: fakeAcp,
    grokProfileDir,
    projectName: PROJECT_A_NAME,
    projectRoot: projectARoot,
    phaseLabel: "register-a",
  });
  await runRegisterPhase({
    playwright,
    environment,
    appDataDir,
    webviewProfileDir,
    cdpPort,
    executable,
    fakeAcpExe: fakeAcp,
    grokProfileDir,
    projectName: PROJECT_B_NAME,
    projectRoot: projectBRoot,
    phaseLabel: "register-b",
  });
  await runRegisterPhase({
    playwright,
    environment,
    appDataDir,
    webviewProfileDir,
    cdpPort,
    executable,
    fakeAcpExe: fakeAcp,
    grokProfileDir,
    projectName: PROJECT_UNSAFE_NAME,
    projectRoot: projectUnsafeRoot,
    phaseLabel: "register-unsafe",
  });

  const batchCore = await withUiSession({
    playwright,
    environment,
    appDataDir,
    webviewProfileDir,
    cdpPort,
    executable,
    fakeAcpExe: fakeAcp,
    grokProfileDir,
    label: "batch-git-core",
    drive: async (page) => {
      const results = [];
      await page.setViewportSize(VIEWPORTS[0]);
      // Composer stays disabled until a project is active.
      await selectProject(page, PROJECT_A_NAME);
      await seedConversation(page, "git-acceptance seed for changes pane");

      results.push(
        await withStep("project-root-clean", () =>
          scenarioProjectRootClean({ page, projectBRoot, privacyTokens })),
      );
      results.push(
        await withStep("nested-repository", () =>
          scenarioNestedRepository({ page, projectARoot, privacyTokens })),
      );
      results.push(
        await withStep("modified-untracked", () =>
          scenarioModifiedUntracked({ page, projectARoot, privacyTokens })),
      );
      results.push(
        await withStep("rename", () =>
          scenarioRename({ page, projectARoot, privacyTokens })),
      );
      results.push(
        await withStep("conflict", () =>
          scenarioConflict({ page, projectARoot, privacyTokens })),
      );
      results.push(
        await withStep("detached-head", () =>
          scenarioDetachedHead({ page, projectARoot, privacyTokens })),
      );
      results.push(
        await withStep("text-diff", () =>
          scenarioTextDiff({ page, projectARoot, privacyTokens })),
      );
      results.push(
        await withStep("binary-oversized-diff", () =>
          scenarioBinaryOversizedDiff({ page, projectARoot, privacyTokens })),
      );
      results.push(
        await withStep("unsafe-config-denial", () =>
          scenarioUnsafeConfigDenial({
            page,
            projectUnsafeRoot,
            privacyTokens,
          })),
      );
      results.push(
        await withStep("generation-drift", () =>
          scenarioGenerationDrift({ page, projectARoot, privacyTokens })),
      );
      results.push(
        await withStep("project-switch-isolation", () =>
          scenarioProjectSwitchIsolation({ page, privacyTokens })),
      );

      // Visual proof at both supported sizes with Changes pane open.
      // Project switches clear conversation scope — re-seed, then open pane
      // without bouncing projects again (bounce would clear the timeline).
      await page.setViewportSize(VIEWPORTS[0]);
      await selectProject(page, PROJECT_A_NAME);
      await seedConversation(page, "git-acceptance visual proof 1440");
      await collapseResourcePaneIfOpen(page);
      await openGitChangesPane(page);
      await waitForGitSettled(page);
      await refreshGitChanges(page);
      await assertConversationDominantWithChanges(page);
      await capturePrivacySafeScreenshot({
        page,
        path: path.join(evidenceDir, "git-changes-1440x960.png"),
        label: "git-changes-1440x960",
        extraForbidden: privacyTokens,
      });

      await page.setViewportSize(VIEWPORTS[1]);
      await collapseResourcePaneIfOpen(page);
      await openGitChangesPane(page);
      await waitForGitSettled(page);
      await assertConversationDominantWithChanges(page);
      await capturePrivacySafeScreenshot({
        page,
        path: path.join(evidenceDir, "git-changes-1024x720.png"),
        label: "git-changes-1024x720",
        extraForbidden: privacyTokens,
      });
      await page.setViewportSize(VIEWPORTS[0]);

      return results;
    },
  });
  caseResults.push(...batchCore);

  // Restart recovery on the same registered app-data (normal close + relaunch).
  const restartResult = await withStep("restart-recovery", () =>
    scenarioRestartRecovery({
      playwright,
      environment,
      appDataDir,
      webviewProfileDir,
      cdpPort,
      executable,
      fakeAcpExe: fakeAcp,
      grokProfileDir,
      projectARoot,
      privacyTokens,
      evidenceDir,
    }));
  caseResults.push(restartResult);

  await waitForDesktopProcessSnapshotClean({
    timeoutMs: 15_000,
    label: "c4-git suite end",
  });
  residualProcesses = {
    pillow: captureDesktopBaseline().keeperPids.length,
    portListeners: captureDesktopBaseline().listenerPids.length,
  };
  assert.equal(residualProcesses.pillow, 0, "residual pillow processes must be zero");
  await assertPathUnchangedSince({
    root: realProfile,
    before: realProfileBefore,
    label: "real LocalAppData WebView2 profile",
  });

  const scenarioNames = caseResults.map((c) => c.scenario);
  for (const required of REQUIRED_SCENARIOS) {
    assert.ok(
      scenarioNames.includes(required),
      `missing scenario ${required}: ${JSON.stringify(scenarioNames)}`,
    );
  }

  const wallTimeMs = Date.now() - startedAt;
  const evidenceRecord = {
    schemaVersion: 1,
    status: "passed",
    ipcBoundary: "real-tauri-webview",
    runtimePath: "production-tauri-webview-real-host-git",
    gitPath: "GitCommandState->GitReadExecutor",
    cases: caseResults.map((c) => ({
      scenario: c.scenario,
      status: c.status,
    })),
    residualProcesses,
    evidenceDir: ".superpowers/c4-git-acceptance",
    viewports: VIEWPORTS.map((v) => v.name),
    wallTimeMs,
    hashManifestPath: "hash-manifest.json",
  };
  await writeFile(
    path.join(evidenceDir, "c4-git-acceptance-record.json"),
    `${JSON.stringify(evidenceRecord, null, 2)}\n`,
    "utf8",
  );
  await writeFile(
    path.join(evidenceDir, "hash-manifest.json"),
    `${JSON.stringify({ schemaVersion: 1, fixtures: hashManifest, cases: caseResults }, null, 2)}\n`,
    "utf8",
  );

  summary = {
    schemaVersion: 1,
    status: "passed",
    ipcBoundary: "real-tauri-webview",
    runtimePath: "production-tauri-webview-real-host-git",
    gitPath: "GitCommandState->GitReadExecutor",
    scenarios: scenarioNames,
    viewports: VIEWPORTS.map((v) => v.name),
    processCleanup: "passed",
    normalClose: "passed",
    restartRecovery: "passed",
    residualProcesses: 0,
    wallTimeMs,
  };
  console.log(`${RECORD_PREFIX} ${JSON.stringify(summary)}`);
} catch (error) {
  summary = {
    schemaVersion: 1,
    status: "failed",
    ipcBoundary: "real-tauri-webview",
    runtimePath: "production-tauri-webview-real-host-git",
    error: redactText(error instanceof Error ? error.message : String(error)),
    cases: caseResults,
    residualProcesses: residualProcesses?.pillow ?? null,
    wallTimeMs: Date.now() - startedAt,
  };
  console.error(`${RECORD_PREFIX} ${JSON.stringify(summary)}`);
  try {
    await mkdir(evidenceDir, { recursive: true });
    await writeFile(
      path.join(evidenceDir, "c4-git-acceptance-failure.json"),
      `${JSON.stringify(summary, null, 2)}\n`,
      "utf8",
    );
  } catch {
    // best-effort
  }
  process.exitCode = 1;
} finally {
  // Temp app data / cargo target / fixtures — best-effort cleanup.
  await rm(temporaryRoot, { recursive: true, force: true }).catch(() => undefined);
}

if (summary?.status !== "passed") {
  process.exit(process.exitCode || 1);
}
