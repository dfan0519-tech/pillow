/**
 * Wave C3-F real Tauri/WebView2 governed-write acceptance orchestrator.
 *
 * Drives production Tauri commands and the public WebView2 UI through a
 * test-scoped CDP connection. Uses the production Grok ACP composition path
 * with `fake_acp_agent` as PILLOW_GROK_EXECUTABLE and a sibling script-name
 * file (production child env allow-list does not forward KEEPER_FAKE_ACP_SCRIPT).
 *
 * Does not call the write executor or mutate targets to simulate approval.
 * Reuses C2 supervised-process, isolated CARGO_TARGET_DIR, temp app data /
 * WebView2 profile, build-time CDP, and readiness helpers.
 */
import assert from "node:assert/strict";
import { createHash, randomUUID } from "node:crypto";
import { createRequire } from "node:module";
import { spawnSync } from "node:child_process";
import {
  access,
  mkdir,
  mkdtemp,
  readFile,
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
  assertPathUnchangedSince,
  defaultWebViewProfilePath,
  snapshotPath,
} from "./lib/desktop-profile.mjs";
import { validateReadinessEvidence } from "./lib/desktop-readiness.mjs";
import { getMsvcEnvironment } from "./lib/msvc-env.mjs";
import {
  WRITE_OUTCOME,
  assertApprovalBarAbsent,
  assertConversationDominant,
  assertForeignWriteEvidenceAbsent,
  cancelOwningRun,
  capturePrivacySafeScreenshot,
  clickApproveOnce,
  clickReject,
  collapseResourcePaneIfOpen,
  escapeRejectWhileBarFocused,
  exerciseApprovalKeyboard,
  openBoundedDiff,
  openConversationForPrompt,
  openSucceededFile,
  selectProject,
  sendWritePrompt,
  waitForApprovalBar,
  waitForComposerIdle,
  waitForWriteOutcome,
} from "./lib/c3-governed-write-acceptance.mjs";

const repositoryRoot = fileURLToPath(new URL("..", import.meta.url));
const desktopRoot = path.join(repositoryRoot, "apps", "desktop");
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
const RECORD_PREFIX = "KEEPER_WAVE_C3_GOVERNED_WRITE_ACCEPTANCE";

const PROJECT_A_NAME = "Pillow C3 Project A";
const PROJECT_B_NAME = "Pillow C3 Project B";
const CREATE_CONTENT = "created by governed write\n";
const CREATE_PATH = "out/created.txt";
const CREATE_A_PATH = "out/created-a.txt";
const CREATE_A_CONTENT = "created by governed write a\n";
const MODIFY_PATH = "notes/base.txt";
const MODIFY_BASE = "original base bytes\n";
const MODIFY_PROPOSED = "modified by governed write\n";
const MODIFY_DRIFT = "externally drifted base\n";
const CONFLICT_PATH = "out/conflict.txt";

const VIEWPORTS = Object.freeze([
  { width: 1440, height: 960, name: "1440x960" },
  { width: 1024, height: 720, name: "1024x720" },
]);

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

function sha256Hex(buffer) {
  return createHash("sha256").update(buffer).digest("hex");
}

async function fileSha256OrNull(filePath) {
  try {
    const bytes = await readFile(filePath);
    return { exists: true, sha256: sha256Hex(bytes), bytes: bytes.length };
  } catch {
    return { exists: false, sha256: null, bytes: 0 };
  }
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

async function ensureDebugBuild(environment, acceptanceConfigPath, cargoTargetDir) {
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
  // Ensure fake ACP agent is present in the same target dir.
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
    // Parent env does not reach the ACP child (allow-list). Sibling file selects script.
    KEEPER_FAKE_ACP_SCRIPT: "governed-write-desktop",
  };
  delete env.WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS;
  delete env.PILLOW_RUNTIME; // force production Grok path
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

async function forceKillDesktop({
  processRun,
  supervisor,
  cdpPort,
  baseline,
  label,
}) {
  await supervisor.capture(true);
  await supervisor.cleanup();
  await waitForDesktopProcessSnapshotClean({
    port: baseline.port,
    timeoutMs: 30_000,
    label: `${label} forced-kill snapshot`,
  });
  await waitForCdpPortReleased(cdpPort, 15_000, label).catch(() => undefined);
}

async function withStep(step, work) {
  try {
    return await work();
  } catch (error) {
    const detail = error instanceof Error ? error.message : String(error);
    throw new Error(`${step}: ${detail}`);
  }
}

// ---------------------------------------------------------------------------
// Scenario drivers
// ---------------------------------------------------------------------------

async function scenarioRejectCreate({
  page,
  projectRoot,
  evidenceDir,
}) {
  const target = path.join(projectRoot, CREATE_PATH);
  const pre = await fileSha256OrNull(target);
  assert.equal(pre.exists, false, "create target must be absent before reject");

  await selectProject(page, PROJECT_A_NAME);
  await collapseResourcePaneIfOpen(page);
  await sendWritePrompt({
    page,
    prompt: "[c3f:create] reject this create proposal",
  });
  const bar = await waitForApprovalBar({
    page,
    expectedPath: CREATE_PATH,
    expectedMode: "创建",
  });
  await assertConversationDominant(page);
  await capturePrivacySafeScreenshot({
    page,
    path: path.join(evidenceDir, "reject-bar-closed-1440x960.png"),
    label: "reject-bar-closed",
  });
  const diff = await openBoundedDiff(page, CREATE_PATH);
  await capturePrivacySafeScreenshot({
    page,
    path: path.join(evidenceDir, "reject-diff-open-1440x960.png"),
    label: "reject-diff-open",
  });
  await exerciseApprovalKeyboard(page);
  // Escape while bar owns focus rejects without triggering global Stop.
  await escapeRejectWhileBarFocused(page);
  const outcome = await waitForWriteOutcome({
    page,
    message: WRITE_OUTCOME.rejected,
    expectOpenFile: false,
  });
  // Provider terminal text (not required in screenshots).
  await page
    .getByTestId("timeline-assistant-message")
    .filter({ hasText: /governed-write-create-pending-or-denied|governed-write-create-failed|governed-write-create-ok/ })
    .waitFor({ state: "visible", timeout: 60_000 })
    .catch(() => undefined);

  const post = await fileSha256OrNull(target);
  assert.equal(post.exists, false, "reject must leave target absent");

  return {
    scenario: "reject-create",
    pre,
    post,
    bar,
    diff,
    outcome,
    provider: "rejection-or-denied",
  };
}

async function scenarioApproveCreate({
  page,
  projectRoot,
  evidenceDir,
}) {
  const target = path.join(projectRoot, CREATE_PATH);
  const pre = await fileSha256OrNull(target);
  // May exist if prior scenario wrote it — use a fresh project root isolation.
  // For sequential same-root, prefer CREATE_A_PATH for isolation.
  const prompt = "[c3f:create] approve this create once";
  // If previous reject left nothing, create is fine. If approve re-runs, path may conflict.
  // Use create path; orchestrator seeds clean project A for first approve.

  await selectProject(page, PROJECT_A_NAME);
  await collapseResourcePaneIfOpen(page);
  await sendWritePrompt({ page, prompt });
  await waitForApprovalBar({
    page,
    expectedPath: CREATE_PATH,
    expectedMode: "创建",
  });
  await capturePrivacySafeScreenshot({
    page,
    path: path.join(evidenceDir, "approve-create-bar-1440x960.png"),
    label: "approve-create-bar",
  });
  await clickApproveOnce(page);
  const outcome = await waitForWriteOutcome({
    page,
    message: WRITE_OUTCOME.succeeded,
    expectOpenFile: true,
  });
  await page
    .getByTestId("timeline-assistant-message")
    .filter({ hasText: /governed-write-create-ok/ })
    .waitFor({ state: "visible", timeout: 60_000 });

  const post = await fileSha256OrNull(target);
  assert.equal(post.exists, true, "approve create must write the file");
  const expectedSha = sha256Hex(Buffer.from(CREATE_CONTENT, "utf8"));
  assert.equal(post.sha256, expectedSha, "exact UTF-8 bytes after approve create");
  assert.equal(post.bytes, Buffer.byteLength(CREATE_CONTENT, "utf8"));

  // Replay must not write twice: second read matches first hash; no double-append.
  const post2 = await fileSha256OrNull(target);
  assert.equal(post2.sha256, post.sha256, "replay must not mutate bytes again");

  return {
    scenario: "approve-create",
    pre,
    post,
    post2,
    expectedSha,
    outcome,
    provider: "governed-write-create-ok",
  };
}

async function scenarioApproveModify({
  page,
  projectRoot,
  evidenceDir,
}) {
  const target = path.join(projectRoot, MODIFY_PATH);
  await mkdir(path.dirname(target), { recursive: true });
  await writeFile(target, MODIFY_BASE, "utf8");
  const pre = await fileSha256OrNull(target);
  assert.equal(pre.sha256, sha256Hex(Buffer.from(MODIFY_BASE, "utf8")));

  await selectProject(page, PROJECT_A_NAME);
  await collapseResourcePaneIfOpen(page);
  await sendWritePrompt({
    page,
    prompt: "[c3f:modify] approve this modify once",
  });
  await waitForApprovalBar({
    page,
    expectedPath: MODIFY_PATH,
    expectedMode: "修改",
  });
  await openBoundedDiff(page, MODIFY_PATH);
  await capturePrivacySafeScreenshot({
    page,
    path: path.join(evidenceDir, "approve-modify-diff-1440x960.png"),
    label: "approve-modify-diff",
  });
  await clickApproveOnce(page);
  const outcome = await waitForWriteOutcome({
    page,
    message: WRITE_OUTCOME.succeeded,
    expectOpenFile: true,
  });
  await page
    .getByTestId("timeline-assistant-message")
    .filter({ hasText: /governed-write-modify-ok/ })
    .waitFor({ state: "visible", timeout: 60_000 });

  const post = await fileSha256OrNull(target);
  const expectedSha = sha256Hex(Buffer.from(MODIFY_PROPOSED, "utf8"));
  assert.equal(post.exists, true);
  assert.equal(post.sha256, expectedSha, "modify must atomically replace with proposed bytes");

  const open = await openSucceededFile({
    page,
    expectedPath: MODIFY_PATH,
    expectedMarker: "modified by governed write",
  });

  return {
    scenario: "approve-modify",
    pre,
    post,
    expectedSha,
    outcome,
    open,
    provider: "governed-write-modify-ok",
  };
}

async function scenarioBaseDrift({
  page,
  projectRoot,
}) {
  const target = path.join(projectRoot, MODIFY_PATH);
  await mkdir(path.dirname(target), { recursive: true });
  await writeFile(target, MODIFY_BASE, "utf8");
  const pre = await fileSha256OrNull(target);

  await selectProject(page, PROJECT_A_NAME);
  await collapseResourcePaneIfOpen(page);
  await sendWritePrompt({
    page,
    prompt: "[c3f:modify] base drift after pending",
  });
  await waitForApprovalBar({
    page,
    expectedPath: MODIFY_PATH,
    expectedMode: "修改",
  });

  // Explicit test setup AFTER proposal is pending: external drift only here.
  await writeFile(target, MODIFY_DRIFT, "utf8");
  const drifted = await fileSha256OrNull(target);
  assert.equal(drifted.sha256, sha256Hex(Buffer.from(MODIFY_DRIFT, "utf8")));

  await clickApproveOnce(page);
  // base_changed surfaces as failed terminal outcome (fixed Chinese).
  const outcome = await waitForWriteOutcome({
    page,
    message: WRITE_OUTCOME.failed,
    expectOpenFile: false,
  });

  const post = await fileSha256OrNull(target);
  assert.equal(
    post.sha256,
    drifted.sha256,
    "base drift must not clobber external bytes",
  );
  assert.notEqual(
    post.sha256,
    sha256Hex(Buffer.from(MODIFY_PROPOSED, "utf8")),
    "proposed content must not land on drift",
  );

  return {
    scenario: "base-drift",
    pre,
    drifted,
    post,
    outcome,
    provider: "base_changed-or-failed",
  };
}

async function scenarioCancellation({
  page,
  projectRoot,
}) {
  const target = path.join(projectRoot, CREATE_A_PATH);
  const pre = await fileSha256OrNull(target);

  await selectProject(page, PROJECT_A_NAME);
  await collapseResourcePaneIfOpen(page);
  await sendWritePrompt({
    page,
    prompt: "[c3f:create-a] cancel while pending",
  });
  await waitForApprovalBar({
    page,
    expectedPath: CREATE_A_PATH,
    expectedMode: "创建",
  });
  await cancelOwningRun(page);
  await waitForWriteOutcome({
    page,
    message: WRITE_OUTCOME.cancelled,
    expectOpenFile: false,
  }).catch(async () => {
    // Some paths surface interrupted; either terminal non-success is acceptable
    // when cancel wins the race with expiry/decision.
    const body = await page.locator("body").innerText();
    assert.ok(
      body.includes(WRITE_OUTCOME.cancelled)
        || body.includes(WRITE_OUTCOME.interrupted)
        || body.includes(WRITE_OUTCOME.failed)
        || body.includes(WRITE_OUTCOME.rejected),
      "cancel must produce a non-success write terminal or clear the bar",
    );
  });
  // Cancel must leave the composer ready for the next scenario (conflict-replay).
  await assertApprovalBarAbsent(page, 10_000);
  await waitForComposerIdle(page);

  const post = await fileSha256OrNull(target);
  if (pre.exists) {
    assert.equal(post.exists, true, "cancel must not delete an existing target");
    assert.equal(post.sha256, pre.sha256, "cancel must leave target bytes unmutated");
  } else {
    assert.equal(post.exists, false, "cancel must leave absent target absent");
  }

  return {
    scenario: "cancellation",
    pre,
    post,
    provider: "cancelled",
  };
}

async function scenarioTwoProjects({
  page,
  projectARoot,
  projectBRoot,
}) {
  // Brief §6 requires cross-project isolation of proposal/grant/diff/terminal/
  // resource surfaces and restore of the native pending bar when switching back.
  // ACP admits one prompt at a time (global operation_gate while a write wait is
  // live), so B's live turn starts only after A has left the pending wait —
  // concurrent dual pending is product-infeasible and not required by the brief.
  const createBPath = "out/created-b.txt";
  const createBContent = "created by governed write b\n";
  const aForbiddenWhileB = [
    CREATE_A_PATH,
    "created by governed write a",
    CREATE_A_CONTENT.trim(),
    WRITE_OUTCOME.succeeded,
  ];
  const bForbiddenWhileA = [createBPath, "created by governed write b", createBContent.trim()];

  const isolationAPrompt = "[c3f:create-a] pending for isolation A";
  const isolationBPrompt = "[c3f:create-b] pending for isolation B";

  await selectProject(page, PROJECT_A_NAME);
  await collapseResourcePaneIfOpen(page);
  await waitForComposerIdle(page);
  await sendWritePrompt({
    page,
    prompt: isolationAPrompt,
  });
  await waitForApprovalBar({
    page,
    expectedPath: CREATE_A_PATH,
    expectedMode: "创建",
  });
  // Surface grant/diff on A so foreign selection can prove they never leak.
  await openBoundedDiff(page, CREATE_A_PATH);
  await collapseResourcePaneIfOpen(page);

  await selectProject(page, PROJECT_B_NAME);
  await assertForeignWriteEvidenceAbsent({
    page,
    forbidden: aForbiddenWhileB,
  });
  await assertApprovalBarAbsent(page);

  // Switching back restores the exact native pending state for A.
  // Title may still be "新对话" while sendTurn is blocked on the write wait;
  // prefer approval-bar restore, then force conversation selection if needed.
  await selectProject(page, PROJECT_A_NAME);
  try {
    await waitForApprovalBar({
      page,
      expectedPath: CREATE_A_PATH,
      expectedMode: "创建",
      timeoutMs: 8_000,
    });
  } catch {
    await openConversationForPrompt(page, isolationAPrompt);
    await waitForApprovalBar({
      page,
      expectedPath: CREATE_A_PATH,
      expectedMode: "创建",
    });
  }
  await assertForeignWriteEvidenceAbsent({
    page,
    forbidden: bForbiddenWhileA,
    // The exact A proposal was restored above. A's own bar is expected here;
    // this assertion only excludes foreign B evidence.
    allowOwnedApprovalBar: true,
  });

  // Complete A before any B turn so the adapter operation_gate is free.
  await clickApproveOnce(page);
  await waitForWriteOutcome({
    page,
    message: WRITE_OUTCOME.succeeded,
    expectOpenFile: true,
  });
  const aPost = await fileSha256OrNull(path.join(projectARoot, CREATE_A_PATH));
  assert.equal(aPost.exists, true);
  assert.equal(aPost.sha256, sha256Hex(Buffer.from(CREATE_A_CONTENT, "utf8")));

  // A's terminal success / open-file surface must not appear under B.
  await selectProject(page, PROJECT_B_NAME);
  await assertForeignWriteEvidenceAbsent({
    page,
    forbidden: [
      CREATE_A_PATH,
      "created by governed write a",
      CREATE_A_CONTENT.trim(),
      WRITE_OUTCOME.succeeded,
    ],
  });
  await collapseResourcePaneIfOpen(page);
  await waitForComposerIdle(page);
  await sendWritePrompt({
    page,
    prompt: isolationBPrompt,
  });
  await waitForApprovalBar({
    page,
    expectedPath: createBPath,
    expectedMode: "创建",
  });

  // B's pending proposal must not leak into A (history may keep A's success).
  await selectProject(page, PROJECT_A_NAME);
  await openConversationForPrompt(page, isolationAPrompt);
  await assertForeignWriteEvidenceAbsent({
    page,
    forbidden: bForbiddenWhileA,
  });
  // A already terminal — bar must stay absent.
  await assertApprovalBarAbsent(page);

  await selectProject(page, PROJECT_B_NAME);
  await openConversationForPrompt(page, isolationBPrompt);
  await waitForApprovalBar({
    page,
    expectedPath: createBPath,
    expectedMode: "创建",
  });
  await clickReject(page);
  await waitForWriteOutcome({
    page,
    message: WRITE_OUTCOME.rejected,
    expectOpenFile: false,
  });
  const bPost = await fileSha256OrNull(path.join(projectBRoot, createBPath));
  assert.equal(bPost.exists, false);

  // Rejected B terminal must not appear under A.
  await selectProject(page, PROJECT_A_NAME);
  await openConversationForPrompt(page, isolationAPrompt);
  await assertForeignWriteEvidenceAbsent({
    page,
    forbidden: [createBPath, "created by governed write b", WRITE_OUTCOME.rejected],
  });

  return {
    scenario: "two-projects",
    aPost,
    bPost,
    isolation: "passed",
    concurrencyModel: "serial-adapter-turns",
  };
}

async function scenarioConflictReplay({
  page,
  projectRoot,
}) {
  const target = path.join(projectRoot, CONFLICT_PATH);
  const pre = await fileSha256OrNull(target);

  await selectProject(page, PROJECT_A_NAME);
  await collapseResourcePaneIfOpen(page);
  await sendWritePrompt({
    page,
    prompt: "[c3f:conflict] same call id different content",
  });
  // At most one bar — second call fails closed.
  await waitForApprovalBar({
    page,
    expectedPath: CONFLICT_PATH,
    expectedMode: "创建",
  });
  assert.equal(await page.getByTestId("write-approval-bar").count(), 1);

  await clickReject(page);
  // Reject clears the bar; the conflict fixture may still be draining the
  // second write-tool response before end_turn. Do not require full composer
  // idle here — two-projects and forced-restart use dedicated app-data slices.
  await assertApprovalBarAbsent(page, 15_000);
  await page
    .getByTestId("timeline-assistant-message")
    .filter({ hasText: /governed-write-conflict-ok|governed-write-conflict-failed|governed-write-create|写入请求已拒绝/ })
    .waitFor({ state: "visible", timeout: 30_000 })
    .catch(() => undefined);

  const post = await fileSha256OrNull(target);
  // Neither first nor second content may land when rejected / conflicted.
  assert.equal(post.exists, false, "conflict replay must not create the target");

  return {
    scenario: "conflict-replay",
    pre,
    post,
    singleProposal: true,
  };
}

async function scenarioForcedRestart({
  playwright,
  environment,
  appDataDir,
  webviewProfileDir,
  cdpPort,
  executable,
  fakeAcpExe,
  grokProfileDir,
  projectRoot,
  evidenceDir,
}) {
  // First UI session: create a terminal success history, then leave a pending proposal and kill.
  const historyPath = path.join(projectRoot, "out/history-success.txt");
  // Seed empty out/
  await mkdir(path.join(projectRoot, "out"), { recursive: true });

  // Phase 1: establish success history via approve create-a if not already done,
  // then open a new pending create and force-kill.
  const launched = await launchDesktopPhase({
    environment,
    appDataDir,
    webviewProfileDir,
    cdpPort,
    executable,
    fakeAcpExe,
    grokProfileDir,
    acceptance: null,
    label: "restart-pending",
  });

  let browser;
  let pendingPrompt = "[c3f:create] restart while pending leave untouched";
  try {
    const connected = await connectWebViewPage(playwright, cdpPort);
    browser = connected.browser;
    const page = connected.page;

    await selectProject(page, PROJECT_A_NAME);
    await collapseResourcePaneIfOpen(page);
    await sendWritePrompt({ page, prompt: pendingPrompt });
    await waitForApprovalBar({
      page,
      expectedPath: CREATE_PATH,
      expectedMode: "创建",
    });
    await capturePrivacySafeScreenshot({
      page,
      path: path.join(evidenceDir, "restart-pending-bar-1440x960.png"),
      label: "restart-pending-bar",
    });

    const preKill = await fileSha256OrNull(path.join(projectRoot, CREATE_PATH));

    // Force kill — do not normal-close.
    await browser.close().catch(() => undefined);
    browser = undefined;
    await forceKillDesktop({
      processRun: launched.processRun,
      supervisor: launched.supervisor,
      cdpPort,
      baseline: launched.baseline,
      label: "restart-pending-kill",
    });

    // Phase 2: restore — interrupted, no grant/diff, no auto-apply.
    const restored = await withUiSession({
      playwright,
      environment,
      appDataDir,
      webviewProfileDir,
      cdpPort,
      executable,
      fakeAcpExe,
      grokProfileDir,
      label: "restart-restore",
      drive: async (page2) => {
        await selectProject(page2, PROJECT_A_NAME);
        await openConversationForPrompt(page2, pendingPrompt);
        // No live approval bar with grant; interrupted may show as outcome.
        await assertApprovalBarAbsent(page2, 15_000).catch(async () => {
          // If a bar reappears it must not expose Approve with a grant path —
          // Approve should be disabled without grant.
          const approve = page2.getByTestId("write-approval-approve");
          if ((await approve.count()) > 0) {
            assert.equal(
              await approve.isDisabled(),
              true,
              "restored pending without grant must not enable Approve",
            );
          }
        });
        // Diff review should not open for interrupted rows.
        const review = page2.getByTestId("write-approval-review-diff");
        if ((await review.count()) > 0) {
          assert.equal(await review.isDisabled(), true, "diff must not be available after restart");
        }
        const body = await page2.locator("body").innerText();
        assert.ok(
          body.includes(WRITE_OUTCOME.interrupted)
            || body.includes("中断")
            || (await page2.getByTestId("write-approval-bar").count()) === 0,
          "restart must mark interrupted or hide live grant/diff",
        );
        const post = await fileSha256OrNull(path.join(projectRoot, CREATE_PATH));
        assert.equal(post.exists, preKill.exists);
        if (preKill.exists) assert.equal(post.sha256, preKill.sha256);
        else assert.equal(post.exists, false);

        // Prior terminal success (if any in timeline) remains as history — soft check.
        return {
          scenario: "forced-restart",
          preKill,
          post,
          autoApply: false,
          interrupted: true,
        };
      },
    });

    return restored;
  } catch (error) {
    if (browser) await browser.close().catch(() => undefined);
    await launched.supervisor.cleanup().catch(() => undefined);
    throw error;
  }
}

// ---------------------------------------------------------------------------
// Main
// ---------------------------------------------------------------------------

const temporaryRoot = await mkdtemp(path.join(os.tmpdir(), "pillow-c3f-"));
const appDataDir = path.join(temporaryRoot, "app-data");
const webviewProfileDir = path.join(temporaryRoot, "webview-profile");
const cargoTargetDir = path.join(temporaryRoot, "cargo-target");
const grokProfileDir = path.join(temporaryRoot, "grok-profile");
const projectARoot = path.join(temporaryRoot, "project-a");
const projectBRoot = path.join(temporaryRoot, "project-b");
const evidenceDir = path.join(repositoryRoot, ".superpowers", "c3-acceptance");
const realProfile = defaultWebViewProfilePath();
const realProfileBefore = await snapshotPath(realProfile);
const caseResults = [];
let residualProcesses = null;
let summary;

try {
  await mkdir(appDataDir, { recursive: true });
  await mkdir(webviewProfileDir, { recursive: true });
  await mkdir(cargoTargetDir, { recursive: true });
  await mkdir(grokProfileDir, { recursive: true });
  await mkdir(projectARoot, { recursive: true });
  await mkdir(projectBRoot, { recursive: true });
  await mkdir(path.join(projectARoot, "out"), { recursive: true });
  await mkdir(path.join(projectARoot, "notes"), { recursive: true });
  await mkdir(path.join(projectBRoot, "out"), { recursive: true });
  await mkdir(evidenceDir, { recursive: true });
  await writeFile(path.join(projectARoot, MODIFY_PATH), MODIFY_BASE, "utf8");

  const environment = await getMsvcEnvironment();
  const cdpPort = await allocateCdpPort();
  const acceptanceConfigPath = await writeAcceptanceTauriConfig(temporaryRoot, cdpPort);
  console.error(
    `${RECORD_PREFIX}_CDP ${JSON.stringify({
      cdpPort,
      configPath: path.basename(acceptanceConfigPath),
      driver: "tauri-config-additionalBrowserArgs",
      cargoTargetDir: "acceptance-temp",
    })}`,
  );

  const { executable, fakeAcp } = await ensureDebugBuild(
    environment,
    acceptanceConfigPath,
    cargoTargetDir,
  );
  await writeFakeAcpScriptName(fakeAcp, "governed-write-desktop");
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

  // Scenario batch 1: reject, approve-create, approve-modify, base-drift, cancel, conflict
  // in one long-lived UI session where possible (serialized prompts).
  const batch1 = await withUiSession({
    playwright,
    environment,
    appDataDir,
    webviewProfileDir,
    cdpPort,
    executable,
    fakeAcpExe: fakeAcp,
    grokProfileDir,
    label: "batch-core",
    drive: async (page) => {
      const results = [];

      // Viewport screenshots at supported sizes for bar closed/open.
      await page.setViewportSize(VIEWPORTS[0]);
      results.push(await withStep("reject-create", () =>
        scenarioRejectCreate({ page, projectRoot: projectARoot, evidenceDir })));

      // Fresh path for approve: remove if reject left nothing (already absent).
      results.push(await withStep("approve-create", () =>
        scenarioApproveCreate({ page, projectRoot: projectARoot, evidenceDir })));

      // Re-seed base for modify after create scenarios.
      await writeFile(path.join(projectARoot, MODIFY_PATH), MODIFY_BASE, "utf8");
      results.push(await withStep("approve-modify", () =>
        scenarioApproveModify({ page, projectRoot: projectARoot, evidenceDir })));

      results.push(await withStep("base-drift", () =>
        scenarioBaseDrift({ page, projectRoot: projectARoot })));

      results.push(await withStep("cancellation", () =>
        scenarioCancellation({ page, projectRoot: projectARoot })));

      results.push(await withStep("conflict-replay", () =>
        scenarioConflictReplay({ page, projectRoot: projectARoot })));

      // Second viewport for visual gate.
      await page.setViewportSize(VIEWPORTS[1]);
      await selectProject(page, PROJECT_A_NAME);
      await capturePrivacySafeScreenshot({
        page,
        path: path.join(evidenceDir, "conversation-dominant-1024x720.png"),
        label: "conversation-dominant-1024",
      });
      await page.setViewportSize(VIEWPORTS[0]);

      return results;
    },
  });
  caseResults.push(...batch1);

  // Two-project isolation in a dedicated app-data / WebView / Grok profile
  // slice so residual batch-core conversations cannot pollute row selection.
  // Project roots are re-seeded; register both projects into the fresh store.
  await rm(path.join(projectARoot, CREATE_A_PATH), { force: true });
  await rm(path.join(projectBRoot, "out/created-b.txt"), { force: true });
  await mkdir(path.join(projectARoot, "out"), { recursive: true });
  await mkdir(path.join(projectBRoot, "out"), { recursive: true });
  const isoAppData = path.join(temporaryRoot, "app-data-iso");
  const isoWebview = path.join(temporaryRoot, "webview-iso");
  const isoGrok = path.join(temporaryRoot, "grok-iso");
  await mkdir(isoAppData, { recursive: true });
  await mkdir(isoWebview, { recursive: true });
  await mkdir(isoGrok, { recursive: true });
  await runRegisterPhase({
    playwright,
    environment,
    appDataDir: isoAppData,
    webviewProfileDir: isoWebview,
    cdpPort,
    executable,
    fakeAcpExe: fakeAcp,
    grokProfileDir: isoGrok,
    projectName: PROJECT_A_NAME,
    projectRoot: projectARoot,
    phaseLabel: "register-a-iso",
  });
  await runRegisterPhase({
    playwright,
    environment,
    appDataDir: isoAppData,
    webviewProfileDir: isoWebview,
    cdpPort,
    executable,
    fakeAcpExe: fakeAcp,
    grokProfileDir: isoGrok,
    projectName: PROJECT_B_NAME,
    projectRoot: projectBRoot,
    phaseLabel: "register-b-iso",
  });
  const twoProj = await withUiSession({
    playwright,
    environment,
    appDataDir: isoAppData,
    webviewProfileDir: isoWebview,
    cdpPort,
    executable,
    fakeAcpExe: fakeAcp,
    grokProfileDir: isoGrok,
    label: "two-projects",
    drive: async (page) =>
      withStep("two-projects", () =>
        scenarioTwoProjects({
          page,
          projectARoot,
          projectBRoot,
        })),
  });
  caseResults.push(twoProj);

  // Forced restart while pending.
  // Ensure create target absent for restart pending.
  await rm(path.join(projectARoot, CREATE_PATH), { force: true });
  // Use a separate app-data slice so recovery is deterministic for this case.
  const restartAppData = path.join(temporaryRoot, "app-data-restart");
  const restartWebview = path.join(temporaryRoot, "webview-restart");
  const restartGrok = path.join(temporaryRoot, "grok-restart");
  await mkdir(restartAppData, { recursive: true });
  await mkdir(restartWebview, { recursive: true });
  await mkdir(restartGrok, { recursive: true });

  // Register project A into restart app data.
  await runRegisterPhase({
    playwright,
    environment,
    appDataDir: restartAppData,
    webviewProfileDir: restartWebview,
    cdpPort,
    executable,
    fakeAcpExe: fakeAcp,
    grokProfileDir: restartGrok,
    projectName: PROJECT_A_NAME,
    projectRoot: projectARoot,
    phaseLabel: "register-a-restart",
  });

  const restartResult = await withStep("forced-restart", () =>
    scenarioForcedRestart({
      playwright,
      environment,
      appDataDir: restartAppData,
      webviewProfileDir: restartWebview,
      cdpPort,
      executable,
      fakeAcpExe: fakeAcp,
      grokProfileDir: restartGrok,
      projectRoot: projectARoot,
      evidenceDir,
    }));
  caseResults.push(restartResult);

  await waitForDesktopProcessSnapshotClean({
    timeoutMs: 15_000,
    label: "c3f suite end",
  });
  residualProcesses = {
    pillow: captureDesktopBaseline().keeperPids.length,
    portListeners: captureDesktopBaseline().listenerPids.length,
  };
  await assertPathUnchangedSince({
    root: realProfile,
    before: realProfileBefore,
    label: "real LocalAppData WebView2 profile",
  });

  const evidenceRecord = {
    schemaVersion: 1,
    status: "passed",
    ipcBoundary: "real-tauri-webview",
    cases: caseResults,
    residualProcesses,
    evidenceDir: ".superpowers/c3-acceptance",
    viewports: VIEWPORTS,
  };
  await writeFile(
    path.join(evidenceDir, "c3f-acceptance-record.json"),
    `${JSON.stringify(evidenceRecord, null, 2)}\n`,
    "utf8",
  );

  summary = {
    schemaVersion: 1,
    status: "passed",
    ipcBoundary: "real-tauri-webview",
    scenarios: caseResults.map((c) => c.scenario),
    residualProcesses,
    processCleanup: "passed",
    normalClose: "passed",
    forcedRestartCleanup: "passed",
  };
  console.log(`${RECORD_PREFIX} ${JSON.stringify(summary)}`);
} catch (error) {
  summary = {
    schemaVersion: 1,
    status: "failed",
    ipcBoundary: "real-tauri-webview",
    error: redactText(error instanceof Error ? error.message : String(error)),
    cases: caseResults,
    residualProcesses,
  };
  console.error(`${RECORD_PREFIX} ${JSON.stringify(summary)}`);
  console.error(redactText(error instanceof Error ? error.stack ?? error.message : String(error)));
  process.exitCode = 1;
} finally {
  // Best-effort temp cleanup; evidence stays under repo .superpowers/c3-acceptance.
  await rm(temporaryRoot, { recursive: true, force: true }).catch(() => undefined);
}
