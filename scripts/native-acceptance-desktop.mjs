import assert from "node:assert/strict";
import { randomUUID } from "node:crypto";
import { mkdtemp, mkdir, rm, stat } from "node:fs/promises";
import os from "node:os";
import path from "node:path";
import { fileURLToPath } from "node:url";
import {
  ProcessSupervisor,
  assertDesktopBaselineClean,
  captureDesktopBaseline,
  observeProcessSurvival,
  runChecked,
  startCaptured,
  waitForJsonFile,
} from "./lib/desktop-process.mjs";
import {
  assertPathSnapshotUnchanged,
  defaultWebViewProfilePath,
  snapshotPath,
} from "./lib/desktop-profile.mjs";
import {
  validateLifecycleEvidence,
  validateReadinessEvidence,
} from "./lib/desktop-readiness.mjs";
import { getMsvcEnvironment } from "./lib/msvc-env.mjs";

const repositoryRoot = fileURLToPath(new URL("..", import.meta.url));
const packageManager = process.platform === "win32" ? "cmd.exe" : "pnpm";
const packageManagerArgs = (args) =>
  process.platform === "win32"
    ? ["/d", "/s", "/c", "pnpm", ...args]
    : args;
const executable = path.join(
  repositoryRoot,
  "target",
  "debug",
  process.platform === "win32" ? "pillow.exe" : "pillow",
);

function validateEvidence(value, expected) {
  validateLifecycleEvidence(value, expected);
  assert.equal(value.phase, expected.phase);
  assert.equal(value.status, "passed", value.errorMessage ?? "acceptance failed");
  assert.equal(value.ipcBoundary, "real-tauri-webview");
  assert.ok(value.completedAtUnixMs >= value.startedAt);
  assert.equal(typeof value.projectId, "string");
  assert.ok(value.projectId.length > 0);
}

async function runPhase({
  phase,
  appDataDir,
  webviewProfileDir,
  projectName,
  projectRoot,
  environment,
}) {
  const readinessPath = path.join(appDataDir, "desktop-ready.json");
  const resultPath = path.join(appDataDir, `native-acceptance-${phase}.json`);
  await rm(readinessPath, { force: true });
  const baseline = captureDesktopBaseline();
  assertDesktopBaselineClean(baseline);
  const nonce = randomUUID();
  const launchLowerBound = Date.now();
  const processRun = startCaptured(executable, [], {
    cwd: repositoryRoot,
    env: {
      ...environment,
      PILLOW_ACCEPTANCE_PHASE: phase,
      PILLOW_ACCEPTANCE_PROJECT_NAME: projectName,
      PILLOW_ACCEPTANCE_PROJECT_ROOT: projectRoot,
      PILLOW_ACCEPTANCE_RESULT: resultPath,
      PILLOW_APP_DATA_DIR: appDataDir,
      PILLOW_LAUNCH_NONCE: nonce,
      PILLOW_STARTED_AT: String(launchLowerBound),
      WEBVIEW2_USER_DATA_FOLDER: webviewProfileDir,
    },
  });
  const supervisor = new ProcessSupervisor({
    baseline,
    rootPid: processRun.child.pid,
  });

  let readiness;
  let evidence;
  try {
    const expected = {
      appDataDir,
      launchLowerBound,
      nonce,
      phase,
      webviewProfileDir,
    };
    readiness = await waitForJsonFile(readinessPath, {
      label: `native acceptance ${phase} readiness`,
      onPoll: () => supervisor.capture(),
      processHandle: processRun.child,
      timeoutMs: 30_000,
      validate: (value) => validateReadinessEvidence(value, expected),
    });
    assert.equal(readiness.pid, processRun.child.pid);
    supervisor.trackPid(readiness.pid);
    await observeProcessSurvival(readiness.pid, 500);
    evidence = await waitForJsonFile(resultPath, {
      label: `native acceptance ${phase} result`,
      onPoll: () => supervisor.capture(),
      processHandle: processRun.child,
      timeoutMs: 30_000,
      validate: (value) => validateEvidence(value, expected),
    });
    assert.equal(evidence.pid, readiness.pid);
    return evidence;
  } catch (error) {
    const output = processRun.output();
    throw new Error(
      `${error instanceof Error ? error.message : String(error)}\n`
      + `stdout:\n${output.stdout}\nstderr:\n${output.stderr}`,
    );
  } finally {
    await supervisor.cleanup();
  }
}

const temporaryRoot = await mkdtemp(path.join(os.tmpdir(), "pillow-native-"));
const appDataDir = path.join(temporaryRoot, "app-data");
const webviewProfileDir = path.join(temporaryRoot, "webview-profile");
const projectRoot = path.join(temporaryRoot, "project");
const projectName = "Pillow Native Acceptance";
const realProfile = defaultWebViewProfilePath();
const realProfileBefore = await snapshotPath(realProfile);
const startedAt = performance.now();
let summary;

try {
  await mkdir(appDataDir, { recursive: true });
  await mkdir(webviewProfileDir, { recursive: true });
  await mkdir(projectRoot, { recursive: true });
  const environment = await getMsvcEnvironment();
  await runChecked(
    packageManager,
    packageManagerArgs([
      "--filter",
      "@pillow/desktop",
      "tauri",
      "build",
      "--debug",
      "--no-bundle",
    ]),
    { cwd: repositoryRoot, env: environment },
  );

  const created = await runPhase({
    appDataDir,
    environment,
    phase: "create",
    projectName,
    projectRoot,
    webviewProfileDir,
  });
  const database = await stat(path.join(appDataDir, "keeper.db"));
  assert.ok(database.isFile() && database.size > 0);
  const profileEntriesAfterCreate = await snapshotPath(webviewProfileDir);
  assert.ok(
    profileEntriesAfterCreate.length > 0,
    "Temporary WebView2 profile was not populated.",
  );

  const restored = await runPhase({
    appDataDir,
    environment,
    phase: "restore",
    projectName,
    projectRoot,
    webviewProfileDir,
  });
  assert.equal(restored.projectId, created.projectId);
  assert.equal(restored.restoredProjectCount, 1);

  summary = {
    appDataDir: "temporary",
    databaseBytes: database.size,
    databasePath: "temporary-app-data/keeper.db",
    elapsedMs: Math.round(performance.now() - startedAt),
    ipcBoundary: restored.ipcBoundary,
    processRestarts: 1,
    restoredProjectCount: restored.restoredProjectCount,
    webviewProfile: "temporary-and-populated",
    webviewProfileEntries: profileEntriesAfterCreate.length,
  };
} finally {
  const realProfileAfter = await snapshotPath(realProfile);
  assertPathSnapshotUnchanged(
    realProfileBefore,
    realProfileAfter,
    "real LocalAppData WebView2 profile",
  );
  await rm(temporaryRoot, {
    recursive: true,
    force: true,
    maxRetries: 20,
    retryDelay: 100,
  });
}

console.log(`KEEPER_NATIVE_ACCEPTANCE ${JSON.stringify(summary)}`);
